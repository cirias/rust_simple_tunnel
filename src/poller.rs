use std::convert::TryInto;
use std::io;
use std::os::unix::io::RawFd;
use std::ptr;
use std::time::Duration;

/// Calls a libc function and results in `io::Result`.
#[cfg(unix)]
macro_rules! syscall {
    ($fn:ident $args:tt) => {{
        let res = unsafe { libc::$fn $args };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

/// Interface to epoll.
pub struct Poller {
    /// File descriptor for the epoll instance.
    epoll_fd: RawFd,
    /// Events buffer
    events: Events,
}

impl Poller {
    /// Creates a new poller.
    pub fn new() -> io::Result<Poller> {
        // Create an epoll instance.
        //
        // Use `epoll_create1` with `EPOLL_CLOEXEC`.
        let epoll_fd = syscall!(syscall(
            libc::SYS_epoll_create1,
            libc::EPOLL_CLOEXEC as libc::c_int
        ))
        .map(|fd| fd as libc::c_int)?;

        let poller = Poller {
            epoll_fd,
            events: Events::new(),
        };

        log::trace!("new: epoll_fd={}", epoll_fd);
        Ok(poller)
    }

    /// Adds a new file descriptor.
    pub fn add(&self, fd: RawFd, ev: Event) -> io::Result<()> {
        log::trace!("add: epoll_fd={}, fd={}, ev={:?}", self.epoll_fd, fd, ev);
        self.ctl(libc::EPOLL_CTL_ADD, fd, Some(ev))
    }

    /// Modifies an existing file descriptor.
    pub fn modify(&self, fd: RawFd, ev: Event) -> io::Result<()> {
        log::trace!("modify: epoll_fd={}, fd={}, ev={:?}", self.epoll_fd, fd, ev);
        self.ctl(libc::EPOLL_CTL_MOD, fd, Some(ev))
    }

    /// Deletes a file descriptor.
    pub fn delete(&self, fd: RawFd) -> io::Result<()> {
        log::trace!("remove: epoll_fd={}, fd={}", self.epoll_fd, fd);
        self.ctl(libc::EPOLL_CTL_DEL, fd, None)
    }

    /// Waits for I/O events with an optional timeout.
    pub fn wait(
        &mut self,
        events: &mut Vec<Event>,
        timeout: Option<Duration>,
    ) -> io::Result<usize> {
        // Wait for I/O events.
        self.wait_events(timeout)?;

        // Collect events.
        let len = events.len();
        events.extend(self.events.iter().filter(|ev| ev.key != usize::MAX));
        Ok(events.len() - len)
    }

    fn wait_events(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        log::trace!("wait: epoll_fd={}, timeout={:?}", self.epoll_fd, timeout);

        // Timeout in milliseconds for epoll.
        let timeout_ms = match timeout {
            Some(t) if t == Duration::from_secs(0) => 0,
            Some(t) => {
                // Round up to a whole millisecond.
                let mut ms = t.as_millis().try_into().unwrap_or(std::i32::MAX);
                if Duration::from_millis(ms as u64) < t {
                    ms = ms.saturating_add(1);
                }
                ms
            }
            _ => -1,
        };

        // Wait for I/O events.
        let res = syscall!(epoll_wait(
            self.epoll_fd,
            self.events.list.as_mut_ptr() as *mut libc::epoll_event,
            self.events.list.len() as libc::c_int,
            timeout_ms as libc::c_int,
        ))?;
        self.events.len = res as usize;
        log::trace!("new events: epoll_fd={}, res={}", self.epoll_fd, res);

        Ok(())
    }

    /// Passes arguments to `epoll_ctl`.
    fn ctl(&self, op: libc::c_int, fd: RawFd, ev: Option<Event>) -> io::Result<()> {
        let mut ev = ev.map(|ev| {
            let mut flags = libc::EPOLLONESHOT;
            if ev.readable {
                flags |= read_flags();
            }
            if ev.writable {
                flags |= write_flags();
            }
            libc::epoll_event {
                events: flags as _,
                u64: ev.key as u64,
            }
        });
        syscall!(epoll_ctl(
            self.epoll_fd,
            op,
            fd,
            ev.as_mut()
                .map(|ev| ev as *mut libc::epoll_event)
                .unwrap_or(ptr::null_mut()),
        ))?;
        Ok(())
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        log::trace!("drop: epoll_fd={}", self.epoll_fd,);

        let _ = syscall!(close(self.epoll_fd));
    }
}

/// Epoll flags for all possible readability events.
fn read_flags() -> libc::c_int {
    libc::EPOLLIN | libc::EPOLLRDHUP | libc::EPOLLHUP | libc::EPOLLERR | libc::EPOLLPRI
}

/// Epoll flags for all possible writability events.
fn write_flags() -> libc::c_int {
    libc::EPOLLOUT | libc::EPOLLHUP | libc::EPOLLERR
}

/// A list of reported I/O events.
pub struct Events {
    list: Box<[libc::epoll_event]>,
    len: usize,
}

unsafe impl Send for Events {}

impl Events {
    /// Creates an empty list.
    pub fn new() -> Events {
        let ev = libc::epoll_event { events: 0, u64: 0 };
        let list = vec![ev; 1000].into_boxed_slice();
        let len = 0;
        Events { list, len }
    }

    /// Iterates over I/O events.
    pub fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.list[..self.len].iter().map(|ev| Event {
            key: ev.u64 as usize,
            readable: (ev.events as libc::c_int & read_flags()) != 0,
            writable: (ev.events as libc::c_int & write_flags()) != 0,
        })
    }
}

/// Indicates that a file descriptor or socket can read or write without blocking.
#[derive(Debug)]
pub struct Event {
    /// Key identifying the file descriptor or socket.
    pub key: usize,
    /// Can it do a read operation without blocking?
    pub readable: bool,
    /// Can it do a write operation without blocking?
    pub writable: bool,
}

impl Event {
    /// All kinds of events (readable and writable).
    ///
    /// Equivalent to: `Event { key, readable: true, writable: true }`
    pub fn all(key: usize) -> Event {
        Event {
            key,
            readable: true,
            writable: true,
        }
    }

    /// Only the readable event.
    ///
    /// Equivalent to: `Event { key, readable: true, writable: false }`
    pub fn readable(key: usize) -> Event {
        Event {
            key,
            readable: true,
            writable: false,
        }
    }

    /// Only the writable event.
    ///
    /// Equivalent to: `Event { key, readable: false, writable: true }`
    pub fn writable(key: usize) -> Event {
        Event {
            key,
            readable: false,
            writable: true,
        }
    }

    /// No events.
    ///
    /// Equivalent to: `Event { key, readable: false, writable: false }`
    pub fn none(key: usize) -> Event {
        Event {
            key,
            readable: false,
            writable: false,
        }
    }
}
