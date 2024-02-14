pub const MAX_DATAGRAM_SIZE: usize = u16::MAX as usize;

pub fn datagram_buffer() -> Box<[u8; MAX_DATAGRAM_SIZE]> {
    Box::new([0u8; MAX_DATAGRAM_SIZE])
}
