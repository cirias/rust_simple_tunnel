use std::io;

pub trait Rx {
    /// receive one single message.
    /// message may not come in order.
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

pub trait Tx {
    /// send one single message.
    /// message may not queued in a buffer instead of sent immediately
    fn send(&mut self, buf: &[u8]) -> io::Result<usize>;

    /// flush the buffer to send all queued messages.
    fn flush(&mut self) -> io::Result<()>;
}
