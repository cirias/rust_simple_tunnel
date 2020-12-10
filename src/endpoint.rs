use std::convert::TryInto;
use std::io;
use std::net::Ipv4Addr;
use std::os::unix::io::{AsRawFd, RawFd};

use anyhow::{anyhow, Result};
use tun::{platform::posix::Fd, platform::Device as Tun, IntoAddress};

use super::poller::{Event, Poller};

const IP_PACKET_HEADER_MIN_SIZE: usize = 20;
// const IP_PACKET_HEADER_MAX_SIZE: usize = 32;
const MTU: usize = 1500;
const DEFAULT_BUF_SIZE: usize = 8 * 1024;

const POLL_KEY_TUN: usize = 100;
const POLL_KEY_CONN: usize = 200;

pub struct PeerInfo {
    ip: Ipv4Addr,
}

pub struct Endpoint<Ctr: Connector> {
    conn: Ctr::Connection,
    tun: Tun,
}

impl<Ctr: Connector> Endpoint<Ctr> {
    pub fn new<IP: IntoAddress>(ip: IP, connector: Ctr) -> Result<Self> {
        let ip = ip.into_address()?;
        let mut conn = connector.connect()?;

        write_peer_info(&mut conn, PeerInfo { ip })?;
        let peer_info = read_peer_info(&mut conn)?;

        let peer_ip = peer_info.ip;
        let mut config: tun::Configuration = Default::default();
        config.address(ip).destination(peer_ip).mtu(MTU as i32).up();
        let tun =
            tun::create(&config).or_else(|e| Err(anyhow!("could not create tun: {:?}", e)))?;

        Ok(Self { conn, tun })
    }

    pub fn run(mut self) -> Result<()> {
        let mut poller = Poller::new()?;

        poller.add(self.tun.as_raw_fd(), Event::readable(POLL_KEY_TUN))?;
        poller.add(self.conn.as_raw_fd(), Event::readable(POLL_KEY_CONN))?;

        let mut events = Vec::new();
        let mut tun_conn_buf = PacketBuf::with_capacity(DEFAULT_BUF_SIZE);
        let mut conn_tun_buf = PacketBuf::with_capacity(DEFAULT_BUF_SIZE);

        set_nonblock(self.tun.as_raw_fd())?;
        set_nonblock(self.conn.as_raw_fd())?;

        loop {
            // Wait for at least one I/O event.
            events.clear();
            poller.wait(&mut events, None)?;

            for ev in &events {
                if ev.key == POLL_KEY_TUN && ev.readable {
                    if is_blocked(tun_conn_buf.read(&mut self.tun))? {
                        log::debug!("block: read from tun");
                    }
                }

                if ev.key == POLL_KEY_CONN && ev.readable {
                    if is_blocked(conn_tun_buf.read(&mut self.conn))? {
                        log::debug!("block: read from conn");
                    }
                }

                if ev.key == POLL_KEY_TUN {
                    if ev.writable && is_blocked(conn_tun_buf.write(&mut self.tun))? {
                        log::debug!("block: write to tun");
                        poller.modify(self.tun.as_raw_fd(), Event::all(POLL_KEY_TUN))?;
                    } else {
                        poller.modify(self.tun.as_raw_fd(), Event::readable(POLL_KEY_TUN))?;
                    }
                } else if conn_tun_buf.ready_to_write() {
                    poller.modify(self.tun.as_raw_fd(), Event::all(POLL_KEY_TUN))?;
                }

                if ev.key == POLL_KEY_CONN {
                    if ev.writable && is_blocked(tun_conn_buf.write(&mut self.conn))? {
                        log::debug!("block: write to conn");
                        poller.modify(self.conn.as_raw_fd(), Event::all(POLL_KEY_CONN))?;
                    } else {
                        poller.modify(self.conn.as_raw_fd(), Event::readable(POLL_KEY_CONN))?;
                    }
                } else if tun_conn_buf.ready_to_write() {
                    poller.modify(self.conn.as_raw_fd(), Event::all(POLL_KEY_CONN))?;
                }
            }
        }
    }
}

pub fn read_peer_info<R: io::Read>(mut r: R) -> io::Result<PeerInfo> {
    // TODO add checksum
    let mut ip_buf = [0; 4];
    r.read_exact(&mut ip_buf)?;

    let ip = Ipv4Addr::new(ip_buf[0], ip_buf[1], ip_buf[2], ip_buf[3]);
    Ok(PeerInfo { ip })
}

pub fn write_peer_info<W: io::Write + Unpin>(mut w: W, peer_info: PeerInfo) -> io::Result<()> {
    // TODO add checksum
    w.write_all(&peer_info.ip.octets())?;
    Ok(())
}

pub fn try_clone_tun_fd(tun: &Tun) -> io::Result<Fd> {
    let raw_fd = tun.as_raw_fd();
    cvt(unsafe { libc::fcntl(raw_fd, libc::F_DUPFD_CLOEXEC, 0) })
        .map(|raw_fd| Fd::new(raw_fd).unwrap())
}

fn cvt(t: i32) -> io::Result<i32> {
    if t == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}

pub trait Connector {
    type Connection: io::Write + io::Read + AsRawFd + Send;

    fn connect(&self) -> io::Result<Self::Connection>;
}

pub struct Connection<T> {
    pub inner: T,
}

impl<T: io::Read> io::Read for Connection<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<T: io::Write> io::Write for Connection<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<T: AsRawFd> AsRawFd for Connection<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

fn is_blocked(res: io::Result<()>) -> io::Result<bool> {
    match res {
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(true),
        Err(e) => Err(e),
        _ => Ok(false),
    }
}

fn set_nonblock(fd: i32) -> io::Result<()> {
    match unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

struct PacketBuf {
    buf: Vec<u8>,
    packet_start: usize,
    packet_end: usize,
    packet_length: usize,
}

impl PacketBuf {
    fn with_capacity(cap: usize) -> Self {
        let mut buf = Vec::with_capacity(cap);
        buf.resize(cap, 0);
        PacketBuf {
            buf,
            packet_start: 0,
            packet_end: 0,
            packet_length: 0,
        }
    }

    fn read<R: io::Read>(&mut self, r: &mut R) -> io::Result<()> {
        if self.packet_start > 0 {
            // read is called in the middle of writing
            return Ok(());
        }

        if self.packet_length == 0 {
            while self.packet_end < IP_PACKET_HEADER_MIN_SIZE {
                let n = r.read(&mut self.buf[self.packet_end..])?;
                self.packet_end += n;
            }

            let header = &self.buf[0..IP_PACKET_HEADER_MIN_SIZE];
            self.packet_length = u16::from_be_bytes(header[2..4].try_into().unwrap()) as usize;
            log::trace!("read packet length: {}", self.packet_length);
        }

        while self.packet_end < self.packet_length {
            let n = r.read(&mut self.buf[self.packet_end..])?;
            self.packet_end += n;
        }

        assert_eq!(
            self.packet_end, self.packet_length,
            "one read returned buf should never across multiple packets"
        );
        log::trace!("read whole packet: {}", self.packet_end);

        Ok(())
    }

    fn ready_to_write(&self) -> bool {
        // has data and is not in the middle of reading
        return self.packet_length > 0 && self.packet_end >= self.packet_length;
    }

    fn write<W: io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        if !self.ready_to_write() {
            return Ok(());
        }

        while self.packet_end - self.packet_start > 0 {
            let n = w.write(&self.buf[self.packet_start..self.packet_end])?;
            self.packet_start += n;
        }
        log::trace!("write packet");

        w.flush()?;
        log::trace!("flush packet");

        self.packet_start = 0;
        self.packet_end = 0;
        self.packet_length = 0;

        Ok(())
    }
}
