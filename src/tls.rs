use std::fs::File;
use std::io::{self, Read};
use std::os::unix::io::{AsRawFd, RawFd};

use anyhow::anyhow;
use native_tls::{Identity, TlsAcceptor, TlsConnector, TlsStream};

use super::endpoint::*;

/*
 * Commands to generate self-signed certification
 *
 * openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365
 * openssl pkcs12 -export -out identity.pfx -inkey key.pem -in cert.pem
 */

pub struct ServerConnector<T> {
    pub connector: T,
    pub pkcs12_path: String,
    pub pkcs12_password: String,
}

impl<T: Connector> Connector for ServerConnector<T> {
    type Connection = Connection<TlsStream<T::Connection>>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let mut file = File::open(&self.pkcs12_path)?;
        let mut identity = vec![];
        file.read_to_end(&mut identity)?;
        let identity = Identity::from_pkcs12(&identity, &self.pkcs12_password)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let acceptor =
            TlsAcceptor::new(identity).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let conn = self.connector.connect()?;
        let stream = acceptor.accept(conn).map_err(|e| match e {
            native_tls::HandshakeError::Failure(e) => io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not accept tls stream: {:?}", e),
            ),
            native_tls::HandshakeError::WouldBlock(_) => {
                io::Error::new(io::ErrorKind::WouldBlock, anyhow!("would block"))
            }
        })?;
        Ok(Connection { inner: stream })
    }
}

pub struct ClientConnector<T> {
    pub connector: T,
    pub hostname: String,
    pub accept_invalid_certs: bool,
}

impl<T: Connector> Connector for ClientConnector<T> {
    type Connection = Connection<TlsStream<T::Connection>>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(self.accept_invalid_certs)
            .build()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let conn = self.connector.connect()?;
        let stream = connector
            .connect(&self.hostname, conn)
            .map_err(|e| match e {
                native_tls::HandshakeError::Failure(e) => io::Error::new(
                    io::ErrorKind::Other,
                    anyhow!("could not connect tls stream: {:?}", e),
                ),
                native_tls::HandshakeError::WouldBlock(_) => {
                    io::Error::new(io::ErrorKind::WouldBlock, anyhow!("would block"))
                }
            })?;
        Ok(Connection { inner: stream })
    }
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

impl<T: AsRawFd> AsRawFd for Connection<TlsStream<T>> {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.get_ref().as_raw_fd()
    }
}
