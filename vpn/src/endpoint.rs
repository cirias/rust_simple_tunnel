use std::io;
use std::net::Ipv4Addr;

use anyhow::{Context, Result};
use smol::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tun::{platform::posix::Fd, platform::Device as Tun, IntoAddress};

const IP_PACKET_HEADER_MIN_SIZE: usize = 20;
const IP_PACKET_HEADER_MAX_SIZE: usize = 32;
const MTU: usize = 1500;

pub struct PeerInfo {
    ip: Ipv4Addr,
}

pub struct Packet(Vec<u8>);

impl Packet {
    fn with_capacity(cap: usize) -> Self {
        Packet(Vec::with_capacity(cap))
    }
}

impl Drop for Packet {
    fn drop(&mut self) {}
}

pub struct Endpoint<Ctr: Connector> {
    conn: Ctr::Connection,
    tun: Tun,
}

impl<Ctr: Connector> Endpoint<Ctr> {
    pub async fn new<IP: IntoAddress>(ip: IP, connector: Ctr) -> Result<Self> {
        let ip = ip.into_address()?;
        let mut conn = connector.connect()?;

        write_peer_info(&mut conn, PeerInfo { ip }).await?;
        let peer_info = read_peer_info(&mut conn).await?;

        let peer_ip = peer_info.ip;
        let mut config: tun::Configuration = Default::default();
        config.address(ip).destination(peer_ip).mtu(MTU as i32).up();
        let tun = tun::create(&config)?; //.or_else(|e| Err(anyhow!("could not create tun: {:?}", e)))

        Ok(Self { conn, tun })
    }

    pub async fn read_write(self) -> Result<()> {
        let tun1 = self.tun;
        let conn1 = self.conn;

        let tun2 = try_clone_tun_fd(&tun1)?;
        let conn2 = conn1.try_clone()?;

        let tun1 = blocking::Unblock::new(tun1);
        let tun2 = blocking::Unblock::new(tun2);

        let conn_to_tun = smol::Task::spawn(async {
            copy_packet(conn1, tun1)
                // futures_lite::io::copy(conn1, tun1)
                .await
                .context("could not copy conn to tun")
        });
        let tun_to_conn = smol::Task::spawn(async {
            copy_packet(tun2, conn2)
                // futures_lite::io::copy(tun2, conn2)
                .await
                .context("could not copy tun to conn")
        });

        smol::future::try_join(tun_to_conn, conn_to_tun).await?;

        Ok(())
    }
}

pub async fn copy_packet<W: AsyncWrite + Unpin + Send, R: AsyncRead + Unpin + Send>(
    mut src: R,
    mut dst: W,
) -> Result<()> {
    loop {
        let packet = read_packet(&mut src)
            .await
            .context("could not read packet")?;
        let buf = &packet.0[..];
        /*
         * println!("{:?}", buf);
         * let mut n = 0;
         * while n < buf.len() {
         *     let w = dst.write(&buf[n..]).await.context("could not write")?;
         *     println!("written: {}", w);
         *     if w == 0 {
         *         panic!("write zero: {:?}", &buf[n..]);
         *     }
         *     n += w;
         * }
         */
        dst.write_all(buf)
            .await
            .with_context(|| format!("could not write packet: {:?}", buf))?;
        dst.flush().await.context("could not flush")?;
    }
}

pub async fn read_peer_info<R: AsyncRead + Unpin>(mut r: R) -> io::Result<PeerInfo> {
    // TODO add checksum
    let mut ip_buf = [0; 4];
    r.read_exact(&mut ip_buf).await?;

    let ip = Ipv4Addr::new(ip_buf[0], ip_buf[1], ip_buf[2], ip_buf[3]);
    Ok(PeerInfo { ip })
}

pub async fn write_peer_info<W: AsyncWrite + Unpin>(
    mut w: W,
    peer_info: PeerInfo,
) -> io::Result<()> {
    // TODO add checksum
    w.write_all(&peer_info.ip.octets()).await?;
    Ok(())
}

pub async fn read_packet<R: AsyncRead + Unpin>(mut r: R) -> io::Result<Packet> {
    use std::convert::TryInto;

    let mut packet = Packet::with_capacity(IP_PACKET_HEADER_MAX_SIZE + MTU);

    let buf = &mut packet.0;
    buf.resize(IP_PACKET_HEADER_MIN_SIZE, 0); // it won't allocate memory if it is already sufficient
    r.read_exact(&mut buf[0..IP_PACKET_HEADER_MIN_SIZE]).await?;

    let len = u16::from_be_bytes(buf[2..4].try_into().unwrap()) as usize;
    buf.resize(len, 0);
    r.read_exact(&mut buf[IP_PACKET_HEADER_MIN_SIZE..len])
        .await?;
    // println!("packet: {:X?}", buf);

    Ok(packet)
}

pub fn try_clone_tun_fd(tun: &Tun) -> io::Result<Fd> {
    use std::os::unix::io::AsRawFd;
    Ok(Fd::new(tun.as_raw_fd()).unwrap())
    /*
     * let raw_fd = tun.as_raw_fd();
     * cvt(unsafe { libc::fcntl(raw_fd, libc::F_DUPFD_CLOEXEC, 0) })
     *     .map(|raw_fd| Fd::new(raw_fd).unwrap())
     */
}

/*
 * fn cvt(t: i32) -> io::Result<i32> {
 *     if t == -1 {
 *         Err(io::Error::last_os_error())
 *     } else {
 *         Ok(t)
 *     }
 * }
 */

pub trait Connector {
    type Connection: AsyncRead + AsyncWrite + TryClone + Unpin + Send + 'static;

    fn connect(&self) -> io::Result<Self::Connection>;
}

pub trait TryClone: Sized {
    fn try_clone(&self) -> io::Result<Self>;
}
