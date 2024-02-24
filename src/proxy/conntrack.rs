use std::sync::atomic::{AtomicI32, Ordering};

pub struct ConntrackValue {
    client_sock: tokio::net::UdpSocket,
    m_num_packets_in: AtomicI32,
    m_num_packets_out: AtomicI32,
    pub has_data_in: tokio::sync::Notify,
}
impl ConntrackValue {
    pub fn new(client_sock: tokio::net::UdpSocket) -> Self {
        Self {
            client_sock,
            m_num_packets_in: AtomicI32::new(0),
            m_num_packets_out: AtomicI32::new(0),
            has_data_in: tokio::sync::Notify::new(),
        }
    }
    pub async fn recv(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        return self.client_sock.recv(buf).await;
    }
    pub async fn send(&self, buf: &[u8]) -> std::io::Result<usize> {
        return self.client_sock.send(buf).await;
    }

    pub fn inc_packets_in(&self) {
        let old = self.m_num_packets_in.load(Ordering::Relaxed);
        let new = old.saturating_add(1);
        self.m_num_packets_in.store(new, Ordering::Relaxed);
        self.has_data_in.notify_one();
    }

    pub fn inc_packets_out(&self) {
        let old = self.m_num_packets_out.load(Ordering::Relaxed);
        let new = old.saturating_add(1);
        self.m_num_packets_out.store(new, Ordering::Relaxed);
    }

    fn num_packets_in(&self) -> i32 {
        self.m_num_packets_in.load(Ordering::Relaxed)
    }
    fn num_packets_out(&self) -> i32 {
        self.m_num_packets_out.load(Ordering::Relaxed)
    }

    pub fn is_assured(&self) -> bool {
        let a = self.num_packets_in();
        let b = self.num_packets_out();
        let min = a.min(b);
        let max = a.max(b);
        min >= 1 && max >= 2
    }
}

pub type ConnTrackMap =
    std::collections::HashMap<std::net::SocketAddr, std::sync::Arc<ConntrackValue>>;

pub const UDP_TIMEOUT: u64 = 30;
pub const UDP_TIMEOUT_STREAM: u64 = 120;
