use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;

struct ConntrackValue {
    sock: tokio::net::UdpSocket,
    has_data_in: tokio::sync::Notify,
}
impl ConntrackValue {
    fn new(sock: tokio::net::UdpSocket) -> Self {
        Self {
            sock,
            has_data_in: tokio::sync::Notify::new(),
        }
    }
}

type ConnTrackMap = HashMap<SocketAddr, Arc<ConntrackValue>>;

const CONNTRACK_TIMEOUT: u64 = 120;

pub struct UdpProxy {
    listener: tokio::net::UdpSocket,
    local_address: SocketAddr,
    remote_address: SocketAddr,
    conntrack_table: tokio::sync::Mutex<ConnTrackMap>,
    packet_transformer: Box<crate::filters::IFilter>,
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
            listener,
            local_address,
            remote_address,
            conntrack_table: tokio::sync::Mutex::new(ConnTrackMap::default()),
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
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(CONNTRACK_TIMEOUT)) => {
                    break;
                }
                recv_result = ct_value.sock.recv(read_buf.as_mut()) => {
                    let recv_len = recv_result
                        .with_context(|| format!("proxy_conn.recv failed for peer {peer_addr}"))?;

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
                    // Update timeout and continue
                    continue;
                }
            }
        }
        return Ok(());
    }
    async fn get_or_insert_conntrack_entry(
        self: &Arc<Self>,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<Arc<ConntrackValue>> {
        let mut conntrack_lock = self.conntrack_table.lock().await;
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
                let self_ = Arc::clone(self);
                tokio::spawn(async move {
                    if let Err(e) = self_.reply_loop(ct_value_, peer_addr).await {
                        log::error!("reply_loop failed: {e}");
                    }
                    log::debug!("Removing conntrack key {peer_addr}");
                    let mut conntrack_lock = self_.conntrack_table.lock().await;
                    conntrack_lock.remove(&peer_addr);
                });
                return Ok(ct_value);
            }
            Entry::Occupied(o) => {
                return Ok(o.get().clone());
            }
        }
    }

    pub async fn run(self: &Arc<Self>) -> anyhow::Result<()> {
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            let (recv_len, peer_addr) = self
                .listener
                .recv_from(read_buf.as_mut())
                .await
                .context("listener.recv_from failed")?;

            let ct_value = self.get_or_insert_conntrack_entry(peer_addr).await?;
            ct_value.has_data_in.notify_one();

            let read_buf = &mut read_buf[..recv_len];
            // In client mode: encrypt from peer and send to udp-obfuscat server.
            // In server mode: decrypt from peer and send to upstream.
            self.packet_transformer.transform(read_buf);
            match ct_value.sock.send(read_buf).await {
                Ok(send_len) => {
                    if send_len != recv_len {
                        log::error!(
                            "Cannot send entire datagram to {}: {send_len} != {recv_len}",
                            self.remote_address,
                        );
                    }
                }
                Err(e) => {
                    log::error!(
                        "Cannot send {recv_len} bytes datagram to {}: {e}",
                        self.remote_address,
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::filters::{Head, IFilter, Xor};

    #[tokio::test]
    async fn proxy_transforms() {
        let proxy_addr = "127.0.0.1:6060".parse().unwrap();
        let upstream_addr = "127.0.0.1:7070".parse().unwrap();
        let filter: Box<IFilter> = Box::new(Head::new(Box::new(Xor::with_key(vec![3])), 3));
        let proxy = Arc::new(
            UdpProxy::new(proxy_addr, upstream_addr, filter)
                .await
                .unwrap(),
        );
        let upstream_task = async move {
            let listener = tokio::net::UdpSocket::bind(upstream_addr).await.unwrap();
            let mut read_buf = crate::common::datagram_buffer();
            let (recv_len, _) = listener.recv_from(read_buf.as_mut()).await.unwrap();
            let data = &read_buf[..recv_len];
            // Server xored only the first 3 bytes: (3 ^ 7) == 4.
            assert_eq!(data, [4, 4, 4, 7, 7, 7, 7, 7]);
        };
        let proxy_task = async move {
            proxy.run().await.unwrap();
        };

        let client_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client_sock.connect(proxy_addr).await.unwrap();
        client_sock.send(&[7_u8; 8]).await.unwrap();
        tokio::select! {
            _ = upstream_task => {}
            _ = proxy_task => {}
        }
    }
    #[tokio::test]
    async fn proxy_proxies() {
        let proxy_client_addr = "127.0.0.1:6061".parse().unwrap();
        let proxy_server_addr = "127.0.0.1:6071".parse().unwrap();
        let upstream_addr = "127.0.0.1:7071".parse().unwrap();

        let key_data = vec![3];
        let filter_client: Box<IFilter> =
            Box::new(Head::new(Box::new(Xor::with_key(key_data.clone())), 3));
        let filter_server: Box<IFilter> =
            Box::new(Head::new(Box::new(Xor::with_key(key_data.clone())), 3));

        let proxy_client = Arc::new(
            UdpProxy::new(proxy_client_addr, proxy_server_addr, filter_client)
                .await
                .unwrap(),
        );
        let proxy_server = Arc::new(
            UdpProxy::new(proxy_server_addr, upstream_addr, filter_server)
                .await
                .unwrap(),
        );

        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let upstream_task = async move {
            let listener = tokio::net::UdpSocket::bind(upstream_addr).await.unwrap();
            let mut read_buf = crate::common::datagram_buffer();
            let (recv_len, peer) = listener.recv_from(read_buf.as_mut()).await.unwrap();
            let data = &read_buf[..recv_len];
            assert_eq!(data, b"hello from client");
            listener
                .send_to(b"hello from upstream", peer)
                .await
                .unwrap();
            // Must wait until client finishes
            done_rx.await.unwrap();
        };
        let proxy_client_task = async move {
            proxy_client.run().await.unwrap();
        };
        let proxy_server_task = async move {
            proxy_server.run().await.unwrap();
        };

        let client_task = async move {
            let client_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            client_sock.connect(proxy_client_addr).await.unwrap();
            client_sock.send(b"hello from client").await.unwrap();
            let mut read_buf = crate::common::datagram_buffer();
            let n = client_sock.recv(read_buf.as_mut()).await.unwrap();
            assert_eq!(&read_buf[..n], b"hello from upstream");
            done_tx.send(()).unwrap();
        };

        tokio::select! {
            _ = upstream_task => {}
            _ = proxy_client_task => {}
            _ = proxy_server_task => {}
            _ = client_task => {}
        }
    }
}
