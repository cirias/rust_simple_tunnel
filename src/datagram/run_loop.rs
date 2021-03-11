use std::io;
use std::os::unix::io::AsRawFd;

use crate::poller::{Event, Poller};

use super::traits::{Rx, Tx};

const POLL_KEY_A: usize = 100;
const POLL_KEY_B: usize = 200;

pub fn run<T1: Rx + Tx + AsRawFd, T2: Rx + Tx + AsRawFd>(mut a: T1, mut b: T2) -> io::Result<()> {
    set_nonblock(a.as_raw_fd())?;
    set_nonblock(b.as_raw_fd())?;

    let mut events = Vec::new();
    let mut ab_buf = DatagramBuffer::new();
    let mut ba_buf = DatagramBuffer::new();

    let mut poller = Poller::new()?;
    poller.add(a.as_raw_fd(), Event::all(POLL_KEY_A))?;
    poller.add(b.as_raw_fd(), Event::all(POLL_KEY_B))?;

    loop {
        // Wait for at least one I/O event.
        events.clear();
        poller.wait(&mut events, None)?;

        for ev in &events {
            if (ev.key == POLL_KEY_A && ev.readable) || (ev.key == POLL_KEY_B && ev.writable) {
                ab_buf.read_write(&mut a, &mut b).no_block()?;
            }
            if (ev.key == POLL_KEY_B && ev.readable) || (ev.key == POLL_KEY_A && ev.writable) {
                ba_buf.read_write(&mut b, &mut a).no_block()?;
            }
        }
    }
}

struct DatagramBuffer {
    buf: [u8; 2048],
    size: usize,
}

impl DatagramBuffer {
    fn new() -> Self {
        Self {
            buf: [0u8; 2048],
            size: 0,
        }
    }

    fn read_write<S: Rx, D: Tx>(&mut self, src: &mut S, dst: &mut D) -> io::Result<()> {
        if self.size == 0 {
            self.size += src.recv(&mut self.buf)?;
            assert!(self.size > 0, "should not read size zero");
        }
        loop {
            self.size -= dst.send(&self.buf[..self.size])?;
            assert_eq!(self.size, 0, "should send full datagram at once");
            dst.flush()?;
            self.size += src.recv(&mut self.buf)?;
            assert!(self.size > 0, "should not read size zero");
        }
    }
}

fn set_nonblock(fd: i32) -> io::Result<()> {
    match unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

pub trait NonBlockingResult<T> {
    fn no_block(self) -> io::Result<Option<T>>;
}

impl<T> NonBlockingResult<T> for io::Result<T> {
    fn no_block(self) -> io::Result<Option<T>> {
        match self {
            Ok(x) => Ok(Some(x)),
            Err(e) => match e.kind() {
                io::ErrorKind::WouldBlock => Ok(None),
                _ => Err(e),
            },
        }
    }
}
