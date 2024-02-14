use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::Context;

type ConnTrackMap = std::collections::HashMap<SocketAddr, Arc<tokio::net::UdpSocket>>;

const UDP_TIMEOUT_STREAM: u64 = 120;

pub struct UdpProxy {
    listener: Arc<tokio::net::UdpSocket>,
    local_address: SocketAddr,
    remote_address: SocketAddr,
    conntrack_table: Arc<Mutex<ConnTrackMap>>,
    packet_transformer: Box<dyn crate::filters::Transform + Send + Sync>,
}

impl UdpProxy {
    pub async fn new(
        local_address: SocketAddr,
        remote_address: SocketAddr,
        packet_transformer: Box<dyn crate::filters::Transform + Send + Sync>,
    ) -> anyhow::Result<Self> {
        let listener = Arc::new(
            tokio::net::UdpSocket::bind(local_address)
                .await
                .with_context(|| {
                    format!("Failed to bind listening socket to address {local_address}")
                })?,
        );
        return Ok(Self {
            listener: listener.clone(),
            local_address: listener
                .local_addr()
                .context("Failed to get local_addr from listener")?,
            remote_address,
            conntrack_table: Arc::new(Mutex::new(ConnTrackMap::default())),
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
        proxy_conn: Arc<tokio::net::UdpSocket>,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        scopeguard::defer! {
            log::debug!("Removing conntrack key {peer_addr}");
            let mut conntrack_lock = self.conntrack_table.lock().unwrap();
            conntrack_lock.remove(&peer_addr);
        }
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            let recv_len = match tokio::time::timeout(
                std::time::Duration::from_secs(UDP_TIMEOUT_STREAM),
                proxy_conn.recv(read_buf.as_mut()),
            )
            .await
            {
                Ok(recv_result) => recv_result
                    .with_context(|| format!("proxy_conn.recv failed for peer {peer_addr}"))?,
                Err(_) => break,
            };
            self.listener
                .send_to(&read_buf[..recv_len], peer_addr)
                .await
                .context("listener.send_to failed")?;
        }
        return Ok(());
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            let (recv_len, peer_addr) = self
                .listener
                .recv_from(read_buf.as_mut())
                .await
                .context("listener.recv_from failed")?;

            let proxy_conn = {
                let mut conntrack_lock = self.conntrack_table.lock().unwrap();
                use std::collections::hash_map::Entry;
                match conntrack_lock.entry(peer_addr) {
                    Entry::Vacant(v) => {
                        let client_sock = connect_udp_socket(self.remote_address)
                            .await
                            .context("Failed to create client UDP socket")?;
                        let client_sock = Arc::new(client_sock);

                        log::debug!(
                            "Creating conntrack key {peer_addr} -> {}",
                            self.remote_address
                        );
                        v.insert(client_sock.clone());

                        let client_sock_ = Arc::clone(&client_sock);
                        let self_ = Arc::clone(&self);
                        tokio::spawn(async move {
                            if let Err(e) = self_.reply_loop(client_sock_, peer_addr).await {
                                log::error!("reply_loop failed: {e}");
                            }
                        });
                        client_sock
                    }
                    Entry::Occupied(o) => o.get().clone(),
                }
            };
            let read_buf = &mut read_buf[..recv_len];
            self.packet_transformer.transform(read_buf);
            match proxy_conn.send(read_buf).await {
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
