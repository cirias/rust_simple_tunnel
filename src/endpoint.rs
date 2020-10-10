use std::io;
use std::net::Ipv4Addr;

use anyhow::{anyhow, Context, Result};
use std::os::unix::io::{AsRawFd, RawFd};
use tun::{platform::posix::Fd, platform::Device as Tun, IntoAddress};

use super::poller::{Event, Poller};

const IP_PACKET_HEADER_MIN_SIZE: usize = 20;
const IP_PACKET_HEADER_MAX_SIZE: usize = 32;
const MTU: usize = 1500;
const DEFAULT_BUF_SIZE: usize = 8 * 1024;

const POLL_KEY_TUN: usize = 1;
const POLL_KEY_CONN: usize = 2;

pub struct PeerInfo {
    ip: Ipv4Addr,
}

pub struct Packet(Vec<u8>);

impl Packet {
    fn with_capacity(cap: usize) -> Self {
        // TODO reduce memory allocation with a buffer pool, but that maybe even slower
        Packet(Vec::with_capacity(cap))
    }
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
        let mut tun_conn_buf = Vec::with_capacity(DEFAULT_BUF_SIZE);
        tun_conn_buf.resize(DEFAULT_BUF_SIZE, 0);
        let mut conn_tun_buf = Vec::with_capacity(DEFAULT_BUF_SIZE);
        conn_tun_buf.resize(DEFAULT_BUF_SIZE, 0);

        loop {
            // Wait for at least one I/O event.
            events.clear();
            poller.wait(&mut events, None)?;

            for ev in &events {
                if ev.key == POLL_KEY_TUN {
                    copy_packet(&mut tun_conn_buf, &mut self.tun, &mut self.conn)?;
                    poller.modify(self.tun.as_raw_fd(), Event::readable(ev.key))?;
                } else if ev.key == POLL_KEY_CONN {
                    copy_packet(&mut conn_tun_buf, &mut self.conn, &mut self.tun)?;
                    poller.modify(self.conn.as_raw_fd(), Event::readable(ev.key))?;
                }
            }
        }
    }
}

pub fn copy_packet<W: io::Write, R: io::Read>(
    buf: &mut Vec<u8>,
    mut src: R,
    mut dst: W,
) -> Result<()> {
    let n = src.read(&mut buf[..]).context("could not read")?;
    assert!(n < buf.len(), "buffer size is too small");
    if n == 0 {
        return Err(anyhow!("read return empty"));
    }
    let buf = &buf[..n];
    dst.write_all(buf)
        .with_context(|| format!("could not write packet: {:?}", buf))?;
    dst.flush()
        .with_context(|| format!("could not flush packet: {:?}", buf))?;
    Ok(())
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

pub fn read_packet<R: io::Read>(mut r: R) -> io::Result<Packet> {
    use std::convert::TryInto;

    let mut packet = Packet::with_capacity(IP_PACKET_HEADER_MAX_SIZE + MTU);

    let buf = &mut packet.0;
    buf.resize(IP_PACKET_HEADER_MIN_SIZE, 0); // it won't allocate memory if it is already sufficient
    r.read_exact(&mut buf[0..IP_PACKET_HEADER_MIN_SIZE])?;

    let len = u16::from_be_bytes(buf[2..4].try_into().unwrap()) as usize;
    buf.resize(len, 0);
    r.read_exact(&mut buf[IP_PACKET_HEADER_MIN_SIZE..len])?;

    Ok(packet)
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
