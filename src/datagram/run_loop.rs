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
    poller.add(a.as_raw_fd(), Event::all(POLL_KEY_A))?;
    poller.add(b.as_raw_fd(), Event::all(POLL_KEY_B))?;

    loop {
        // Wait for at least one I/O event.
        events.clear();
        poller.wait(&mut events, None)?;

        for ev in &events {
            if (ev.key == POLL_KEY_A && ev.readable) || (ev.key == POLL_KEY_B && ev.writable) {
                let mut is_first_time = true;
                loop {
                    if ev.readable || !is_first_time {
                        if is_blocked(ab_buf.read(&mut a))? {
                            log::debug!("block: read from a");
                            break;
                        }
                    }
                    if is_blocked(ab_buf.write(&mut b))? {
                        log::debug!("block: write to b");
                        break;
                    }
                    is_first_time = false
                }
            }

            if (ev.key == POLL_KEY_B && ev.readable) || (ev.key == POLL_KEY_A && ev.writable) {
                let mut is_first_time = true;
                loop {
                    if ev.readable || !is_first_time {
                        if is_blocked(ba_buf.read(&mut b))? {
                            log::debug!("block: read from b");
                            break;
                        }
                    }
                    if is_blocked(ba_buf.write(&mut a))? {
                        log::debug!("block: write to a");
                        break;
                    }
                    is_first_time = false
                }
            }
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
