use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use crate::message::{Rx, Tx};

pub struct Socket<T>(pub T);

impl<T: io::Read> Rx for Socket<T> {
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<T: io::Write> Tx for Socket<T> {
    fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<T: AsRawFd> AsRawFd for Socket<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
