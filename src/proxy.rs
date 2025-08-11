use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;

use crate::{conntrack as ct, dns};

pub struct UdpProxy {
    listeners: Vec<tokio::net::UdpSocket>,
    local_addresses: Vec<SocketAddr>,
    remote_addresses: Vec<SocketAddr>,
    conntrack_table: tokio::sync::Mutex<ct::ConnTrackMap>,
    packet_transformer: Box<crate::filters::IFilter>,
}

impl UdpProxy {
    pub async fn new(
        listener_config: &crate::config::ListenerOptions,
        remote_config: &crate::config::RemoteOptions,
        packet_transformer: Box<crate::filters::IFilter>,
    ) -> anyhow::Result<Self> {
        let local_addrs = dns::resolve_and_filter_ips(
            &listener_config.address,
            dns::ResolveOptions::default()
                .set_ipv4_only(listener_config.ipv4_only)
                .set_ipv6_only(listener_config.ipv6_only),
        )
        .await?;

        if local_addrs.is_empty() {
            anyhow::bail!("No listen address available");
        }

        let mut listeners = Vec::new();
        let mut local_addresses = Vec::new();
        for addr in local_addrs {
            let listener = tokio::net::UdpSocket::bind(addr)
                .await
                .with_context(|| format!("Failed to bind listening socket to '{addr}'"))?;
            let local_address = listener.local_addr().context("UdpSocket::local_addr")?;
            listeners.push(listener);
            local_addresses.push(local_address);
        }
        if listeners.is_empty() {
            anyhow::bail!("Cannot bind UDP socket");
        }

        let remote_addresses = dns::resolve_and_filter_ips(
            &vec![remote_config.address.clone()],
            dns::ResolveOptions::default()
                .set_ipv4_only(remote_config.ipv4_only)
                .set_ipv6_only(remote_config.ipv6_only),
        )
        .await?;

        return Ok(Self {
            listeners,
            local_addresses,
            remote_addresses,
            conntrack_table: Default::default(),
            packet_transformer,
        });
    }

    pub fn get_local_address(&self) -> &[SocketAddr] {
        return &self.local_addresses;
    }
    pub fn get_remote_address(&self) -> &[SocketAddr] {
        return &self.remote_addresses;
    }

    /// Read from upstream and send back to peer through listening socket
    async fn reply_loop(
        &self,
        ct_key: ct::ConntrackKey,
        ct_value: Arc<ct::ConntrackValue>,
    ) -> anyhow::Result<()> {
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(ct::CONNTRACK_TIMEOUT)) => {
                    break;
                }
                recv_result = ct_value.sock.recv(read_buf.as_mut()) => {
                    let recv_len = recv_result
                        .with_context(|| format!("ct_value.sock.recv failed from peer {}",
                                ct_value.sock.peer_addr().unwrap()))?;

                    let read_buf = &mut read_buf[..recv_len];
                    // In client mode: decrypt from udp-obfuscat server and send to peer.
                    // In server mode: encrypt from upstream and send to peer.
                    self.packet_transformer.transform(read_buf);
                    self.listeners[ct_key.listener_id]
                        .send_to(read_buf, ct_key.peer_addr)
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
        key: ct::ConntrackKey,
    ) -> anyhow::Result<Arc<ct::ConntrackValue>> {
        let mut conntrack_lock = self.conntrack_table.lock().await;
        use std::collections::hash_map::Entry;
        match conntrack_lock.entry(key) {
            Entry::Vacant(v) => {
                let client_sock = connect_udp_socket(&self.remote_addresses)
                    .await
                    .context("Failed to create client UDP socket")?;
                let ct_value = Arc::new(ct::ConntrackValue::new(client_sock));

                log::debug!(
                    "Creating conntrack key {} -> {}",
                    key.peer_addr,
                    ct_value.sock.peer_addr().unwrap()
                );
                v.insert(Arc::clone(&ct_value));

                let ct_value_ = Arc::clone(&ct_value);
                let self_ = Arc::clone(self);
                tokio::spawn(async move {
                    if let Err(e) = self_.reply_loop(key, ct_value_).await {
                        log::error!("reply_loop failed: {e:?}");
                    }
                    log::debug!("Removing conntrack key {}", key.peer_addr);
                    let mut conntrack_lock = self_.conntrack_table.lock().await;
                    conntrack_lock.remove(&key);
                });
                return Ok(ct_value);
            }
            Entry::Occupied(o) => {
                return Ok(o.get().clone());
            }
        }
    }

    pub async fn run(self: &Arc<Self>) -> anyhow::Result<()> {
        let mut listen_tasks = Vec::new();
        for i in 0..self.listeners.len() {
            let self_ = Arc::clone(&self);
            listen_tasks.push(tokio::spawn(async move { self_.listen_loop(i).await }));
        }
        let (res, _, _) = futures::future::select_all(listen_tasks).await;
        return res.unwrap();
    }
    async fn listen_loop(self: &Arc<Self>, listener_id: usize) -> anyhow::Result<()> {
        let mut read_buf = crate::common::datagram_buffer();
        loop {
            let (recv_len, peer_addr) = self.listeners[listener_id]
                .recv_from(read_buf.as_mut())
                .await
                .context("listener.recv_from failed")?;

            let ct_value = self
                .get_or_insert_conntrack_entry(ct::ConntrackKey {
                    peer_addr,
                    listener_id,
                })
                .await?;
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
                            ct_value.sock.peer_addr().unwrap(),
                        );
                    }
                }
                Err(e) => {
                    log::error!(
                        "Cannot send {recv_len} bytes datagram to {}: {e:?}",
                        ct_value.sock.peer_addr().unwrap(),
                    );
                }
            }
        }
    }
}

fn get_unspec_sock_addr(base: &SocketAddr) -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    return match base {
        SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    };
}

async fn connect_udp_socket(
    remote_address: &Vec<SocketAddr>,
) -> anyhow::Result<tokio::net::UdpSocket> {
    let mut last_err = None;
    for remote_address in remote_address {
        let local_address = get_unspec_sock_addr(&remote_address);
        let ret = match tokio::net::UdpSocket::bind(local_address).await {
            Ok(ok) => ok,
            Err(e) => {
                last_err = Some(
                    anyhow::Error::new(e)
                        .context(format!("Failed to bind UDP socket to '{local_address}'")),
                );
                continue;
            }
        };
        match ret.connect(remote_address).await {
            Ok(_) => return Ok(ret),
            Err(e) => {
                last_err = Some(anyhow::Error::new(e).context(format!(
                    "Failed to connect UDP socket to '{remote_address}'"
                )));
                continue;
            }
        }
    }
    return Err(last_err.unwrap_or(anyhow::Error::msg("Cannot resolve to any address")));
}

#[cfg(test)]
mod proxy_test;
