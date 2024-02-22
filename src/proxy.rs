use std::net::SocketAddr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context;

struct ConntrackValue {
    client_sock: tokio::net::UdpSocket,
    num_packets_in: AtomicI32,
    num_packets_out: AtomicI32,
    has_data_in: tokio::sync::Notify,
}
impl ConntrackValue {
    fn new(client_sock: tokio::net::UdpSocket) -> Self {
        Self {
            client_sock,
            num_packets_in: AtomicI32::new(0),
            num_packets_out: AtomicI32::new(0),
            has_data_in: tokio::sync::Notify::new(),
        }
    }

    fn inc_packets_in(&self) {
        let old = self.num_packets_in.load(Ordering::Relaxed);
        let new = old.saturating_add(1);
        self.num_packets_in.store(new, Ordering::Relaxed);
        self.has_data_in.notify_one();
    }

    fn inc_packets_out(&self) {
        let old = self.num_packets_out.load(Ordering::Relaxed);
        let new = old.saturating_add(1);
        self.num_packets_out.store(new, Ordering::Relaxed);
    }

    fn get_num_packets_in(&self) -> i32 {
        self.num_packets_in.load(Ordering::Relaxed)
    }
    fn get_num_packets_out(&self) -> i32 {
        self.num_packets_out.load(Ordering::Relaxed)
    }

    fn is_assured(&self) -> bool {
        let a = self.get_num_packets_in();
        let b = self.get_num_packets_out();
        let min = a.min(b);
        let max = a.max(b);
        min >= 1 && max >= 2
    }
}

type ConnTrackMap = std::collections::HashMap<SocketAddr, Arc<ConntrackValue>>;

const UDP_TIMEOUT: u64 = 30;
const UDP_TIMEOUT_STREAM: u64 = 120;

pub struct UdpProxy {
    listener: tokio::net::UdpSocket,
    local_address: SocketAddr,
    remote_address: SocketAddr,
    conntrack_table: Mutex<ConnTrackMap>,
    packet_transformer: Box<dyn crate::filters::Transform + Send + Sync>,
}

impl UdpProxy {
    pub async fn new(
        local_address: SocketAddr,
        remote_address: SocketAddr,
        packet_transformer: Box<dyn crate::filters::Transform + Send + Sync>,
    ) -> anyhow::Result<Self> {
        let listener = tokio::net::UdpSocket::bind(local_address)
            .await
            .with_context(|| {
                format!("Failed to bind listening socket to address {local_address}")
            })?;
        let local_address = listener
            .local_addr()
            .context("Failed to get local_addr from listener")?;
        return Ok(Self {
            listener,
            local_address,
            remote_address,
            conntrack_table: Mutex::new(ConnTrackMap::default()),
            packet_transformer,
        });
    }

    pub fn get_local_address(&self) -> &SocketAddr {
        &self.local_address
    }
    pub fn get_remote_address(&self) -> &SocketAddr {
        &self.remote_address
    }

    async fn reply_loop(
        &self,
        ct_value: Arc<ConntrackValue>,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        scopeguard::defer! {
            log::debug!("Removing conntrack key {peer_addr}");
            let mut conntrack_lock = self.conntrack_table.lock().unwrap();
            conntrack_lock.remove(&peer_addr);
        }
        let mut read_buf = crate::common::datagram_buffer();
        let mut timeout = UDP_TIMEOUT;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(timeout)) => {
                    break;
                }
                recv_result = ct_value.client_sock.recv(read_buf.as_mut()) => {
                    let recv_len = recv_result
                        .with_context(|| format!("proxy_conn.recv failed for peer {peer_addr}"))?;
                    ct_value.inc_packets_out();

                    let read_buf = &mut read_buf[..recv_len];
                    // In client mode: decrypt from udp-obfuscat server and send to peer.
                    // In server mode: encrypt from upstream and send to peer.
                    self.packet_transformer.transform(read_buf);
                    self.listener
                        .send_to(read_buf, peer_addr)
                        .await
                        .context("listener.send_to failed")?;
                }
                _ = ct_value.has_data_in.notified() => {
                    if ct_value.is_assured() {
                        timeout = UDP_TIMEOUT_STREAM;
                    }
                }
            }
        }
        return Ok(());
    }

    async fn get_or_insert_conntrack_entry(
        self: &Arc<Self>,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<Arc<ConntrackValue>> {
        let mut conntrack_lock = self.conntrack_table.lock().unwrap();
        use std::collections::hash_map::Entry;
        match conntrack_lock.entry(peer_addr) {
            Entry::Vacant(v) => {
                let client_sock = connect_udp_socket(self.remote_address)
                    .await
                    .context("Failed to create client UDP socket")?;
                let ct_value = Arc::new(ConntrackValue::new(client_sock));

                log::debug!(
                    "Creating conntrack key {peer_addr} -> {}",
                    self.remote_address
                );
                v.insert(Arc::clone(&ct_value));

                let ct_value_ = Arc::clone(&ct_value);
                let self_ = Arc::clone(&self);
                tokio::spawn(async move {
                    if let Err(e) = self_.reply_loop(ct_value_, peer_addr).await {
                        log::error!("reply_loop failed: {e}");
                    }
                });
                return Ok(ct_value);
            }
            Entry::Occupied(o) => {
                return Ok(o.get().clone());
            }
        }
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            let (recv_len, peer_addr) = self
                .listener
                .recv_from(read_buf.as_mut())
                .await
                .context("listener.recv_from failed")?;

            let ct_value = self.get_or_insert_conntrack_entry(peer_addr).await?;
            ct_value.inc_packets_in();

            let read_buf = &mut read_buf[..recv_len];
            // In client mode: encrypt from peer and send to udp-obfuscat server.
            // In server mode: decrypt from peer and send to upstream.
            self.packet_transformer.transform(read_buf);
            match ct_value.client_sock.send(read_buf).await {
                Ok(send_len) => {
                    if send_len != recv_len {
                        log::error!(
                            "Cannot send entire datagram to {}: {} != {}",
                            self.remote_address,
                            send_len,
                            recv_len
                        );
                    }
                }
                Err(e) => {
                    log::error!(
                        "Cannot send entire datagram to {}: {}",
                        self.remote_address,
                        e
                    );
                }
            }
        }
    }
}

fn get_unspec_sock_addr(base: &SocketAddr) -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    match base {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}

async fn connect_udp_socket(remote_address: SocketAddr) -> anyhow::Result<tokio::net::UdpSocket> {
    let local_address = get_unspec_sock_addr(&remote_address);
    let ret = tokio::net::UdpSocket::bind(local_address)
        .await
        .with_context(|| format!("Failed to bind UDP socket to address {local_address:?}"))?;
    ret.connect(remote_address)
        .await
        .with_context(|| format!("Failed to connect UDP socket to address {remote_address}"))?;
    return Ok(ret);
}
