use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::Context;

type ConnTrackMap = HashMap<SocketAddr, Arc<tokio::net::UdpSocket>>;

const CONNTRACK_TIMEOUT: u64 = 120;

struct SharedState {
    listener: tokio::net::UdpSocket,
    local_address: SocketAddr,
    remote_address: SocketAddr,
    conntrack_table: Mutex<ConnTrackMap>,
    packet_transformer: Box<crate::filters::IFilter>,
}

impl SharedState {
    async fn reply_loop(
        &self,
        proxy_conn: Arc<tokio::net::UdpSocket>,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            let recv_len = match tokio::time::timeout(
                std::time::Duration::from_secs(CONNTRACK_TIMEOUT),
                proxy_conn.recv(read_buf.as_mut()),
            )
            .await
            {
                Ok(recv_result) => recv_result
                    .with_context(|| format!("proxy_conn.recv failed for peer {peer_addr}"))?,
                Err(_) => break,
            };

            let read_buf = &mut read_buf[..recv_len];
            // In client mode: decrypt from udp-obfuscat server and send to peer.
            // In server mode: encrypt from upstream and send to peer.
            self.packet_transformer.transform(read_buf);
            self.listener
                .send_to(read_buf, peer_addr)
                .await
                .context("listener.send_to failed")?;
        }
        return Ok(());
    }
}

pub struct UdpProxy {
    state: Arc<SharedState>,
}

impl UdpProxy {
    pub async fn new(
        local_address: SocketAddr,
        remote_address: SocketAddr,
        packet_transformer: Box<crate::filters::IFilter>,
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
            state: Arc::new(SharedState {
                listener,
                local_address,
                remote_address,
                conntrack_table: Mutex::new(ConnTrackMap::default()),
                packet_transformer,
            }),
        });
    }

    pub fn get_local_address(&self) -> &SocketAddr {
        &self.state.local_address
    }
    pub fn get_remote_address(&self) -> &SocketAddr {
        &self.state.remote_address
    }

    async fn get_or_insert_conntrack_entry(
        &self,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<Arc<tokio::net::UdpSocket>> {
        let mut conntrack_lock = self.state.conntrack_table.lock().unwrap();
        use std::collections::hash_map::Entry;
        match conntrack_lock.entry(peer_addr) {
            Entry::Vacant(v) => {
                let client_sock = connect_udp_socket(self.state.remote_address)
                    .await
                    .context("Failed to create client UDP socket")?;
                let ct_value = Arc::new(client_sock);

                log::debug!(
                    "Creating conntrack key {peer_addr} -> {}",
                    self.state.remote_address
                );
                v.insert(Arc::clone(&ct_value));

                let ct_value_ = Arc::clone(&ct_value);
                let state = Arc::clone(&self.state);
                tokio::spawn(async move {
                    if let Err(e) = state.reply_loop(ct_value_, peer_addr).await {
                        log::error!("reply_loop failed: {e}");
                    }
                    log::debug!("Removing conntrack key {peer_addr}");
                    let mut conntrack_lock = state.conntrack_table.lock().unwrap();
                    conntrack_lock.remove(&peer_addr);
                });
                return Ok(ct_value);
            }
            Entry::Occupied(o) => {
                return Ok(o.get().clone());
            }
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            let (recv_len, peer_addr) = self
                .state
                .listener
                .recv_from(read_buf.as_mut())
                .await
                .context("listener.recv_from failed")?;

            let ct_value = self.get_or_insert_conntrack_entry(peer_addr).await?;

            let read_buf = &mut read_buf[..recv_len];
            // In client mode: encrypt from peer and send to udp-obfuscat server.
            // In server mode: decrypt from peer and send to upstream.
            self.state.packet_transformer.transform(read_buf);
            match ct_value.send(read_buf).await {
                Ok(send_len) => {
                    if send_len != recv_len {
                        log::error!(
                            "Cannot send entire datagram to {}: {send_len} != {recv_len}",
                            self.state.remote_address,
                        );
                    }
                }
                Err(e) => {
                    log::error!(
                        "Cannot send {recv_len} bytes datagram to {}: {e}",
                        self.state.remote_address,
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
