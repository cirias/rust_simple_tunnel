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
    let mut ab_buf = MessageBuffer::new();
    let mut ba_buf = MessageBuffer::new();

    let mut poller = Poller::new()?;
    poller.add(a.as_raw_fd(), Event::readable(POLL_KEY_A))?;
    poller.add(b.as_raw_fd(), Event::readable(POLL_KEY_B))?;

    loop {
        // Wait for at least one I/O event.
        events.clear();
        poller.wait(&mut events, None)?;

        for ev in &events {
            if ev.key == POLL_KEY_A {
                if ev.readable && is_blocked(ab_buf.read(&mut a))? {
                    log::debug!("block: read from a");
                }
                if ev.writable && is_blocked(ba_buf.write(&mut a))? {
                    log::debug!("block: write to a");
                }
            }

            if ev.key == POLL_KEY_B {
                if ev.readable && is_blocked(ba_buf.read(&mut b))? {
                    log::debug!("block: read from b");
                }
                if ev.writable && is_blocked(ab_buf.write(&mut b))? {
                    log::debug!("block: write to b");
                }
            }

            let mut a_event = Event::none(POLL_KEY_A);
            let mut b_event = Event::none(POLL_KEY_B);
            if ab_buf.empty() {
                a_event.readable = true;
            } else {
                b_event.writable = true;
            }
            if ba_buf.empty() {
                b_event.readable = true;
            } else {
                a_event.writable = true;
            }
            poller.modify(a.as_raw_fd(), a_event)?;
            poller.modify(b.as_raw_fd(), b_event)?;
        }
    }
}

fn set_nonblock(fd: i32) -> io::Result<()> {
    match unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

fn is_blocked(res: io::Result<()>) -> io::Result<bool> {
    match res {
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(true),
        Err(e) => Err(e),
        _ => Ok(false),
    }
}

struct MessageBuffer {
    buf: [u8; 2048],
    size: usize,
    pending: bool,
}

impl MessageBuffer {
    fn new() -> Self {
        Self {
            buf: [0u8; 2048],
            size: 0,
            pending: false,
        }
    }

    fn empty(&self) -> bool {
        !self.pending
    }

    fn read<R: Rx>(&mut self, socket: &mut R) -> io::Result<()> {
        if !self.empty() {
            return Ok(());
        }
        self.size = socket.recv(&mut self.buf)?;
        self.pending = true;

        Ok(())
    }

    fn write<T: Tx>(&mut self, socket: &mut T) -> io::Result<()> {
        if self.empty() {
            return Ok(());
        }

        while self.size > 0 {
            let n = socket.send(&self.buf[..self.size])?;
            assert_eq!(n, self.size, "should send full message at once");
            self.size = 0;
        }

        socket.flush()?;
        self.pending = false;

        Ok(())
    }
}
