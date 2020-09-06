use std::io;
use std::net::Ipv4Addr;

use anyhow::{anyhow, Context, Result};
use tun::{platform::posix::Fd, platform::Device as Tun, IntoAddress};

const IP_PACKET_HEADER_MIN_SIZE: usize = 20;
const IP_PACKET_HEADER_MAX_SIZE: usize = 32;
const MTU: usize = 1500;
// const DEFAULT_BUF_SIZE: usize = 8 * 1024;

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

    pub fn run(self) -> Result<()> {
        let mut tun_w = self.tun;
        let mut tun_r = try_clone_tun_fd(&tun_w)?;

        let (mut conn_w, mut conn_r) = self.conn.split()?;

        let res: Result<Vec<_>> = easy_parallel::Parallel::new()
            .add(move || {
                io::copy(&mut conn_r, &mut tun_w).context("could not copy conn to tun")
                /*
                 * // use BufReader to reduce calling of systemcall `read`
                 * let conn_r = io::BufReader::with_capacity(DEFAULT_BUF_SIZE, conn_r);
                 * copy_packet(conn_r, tun_w).context("could not copy conn to tun")
                 */
            })
            .add(move || {
                io::copy(&mut tun_r, &mut conn_w).context("could not copy tun to conn")
                /*
                 * // BufReader is required. Tun device driver doesn't provide an internal read buffer.
                 * // So without BufReader, `read_exact` will cause the lost of the unread data.
                 * let tun_r = io::BufReader::with_capacity(DEFAULT_BUF_SIZE, tun_r);
                 * copy_packet(tun_r, conn_w).context("could not copy tun to conn")
                 */
            })
            .run()
            .into_iter()
            .collect();
        res?;
        Ok(())
    }
}

pub fn copy_packet<W: io::Write, R: io::Read>(mut src: R, mut dst: W) -> Result<()> {
    loop {
        let packet = read_packet(&mut src).context("could not read packet")?;
        let buf = &packet.0[..];
        dst.write_all(buf)
            .with_context(|| format!("could not write packet: {:?}", buf))?;
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
    use std::os::unix::io::AsRawFd;
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
    type Connection: io::Write + io::Read + Split + Send;

    fn connect(&self) -> io::Result<Self::Connection>;
}

pub trait Split {
    type Writer: io::Write + Send;
    type Reader: io::Read + Send;

    fn split(self) -> io::Result<(Self::Writer, Self::Reader)>;
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
