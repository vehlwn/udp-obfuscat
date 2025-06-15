pub const CONNTRACK_TIMEOUT: u64 = 120;

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub struct ConntrackKey {
    pub peer_addr: std::net::SocketAddr,
    pub listener_id: usize,
}

pub struct ConntrackValue {
    pub sock: tokio::net::UdpSocket,
    pub has_data_in: tokio::sync::Notify,
}
impl ConntrackValue {
    pub fn new(sock: tokio::net::UdpSocket) -> Self {
        Self {
            sock,
            has_data_in: Default::default(),
        }
    }
}

pub type ConnTrackMap = std::collections::HashMap<ConntrackKey, std::sync::Arc<ConntrackValue>>;
