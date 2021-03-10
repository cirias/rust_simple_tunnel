use std::sync::Arc;

use std::fs;
use std::io;
use std::io::Seek;
use std::net;
use std::os::unix::io::{AsRawFd, RawFd};

use anyhow::anyhow;
use tungstenite::{
    accept_hdr, client,
    handshake::server::{Callback, ErrorResponse, Request, Response},
    http, Error, Message, WebSocket,
};

use rustls;
use rustls::Session;
use webpki;

use crate::message::{Rx, Tx};

pub struct Socket<T> {
    web_socket: WebSocket<T>,
}

impl<T: io::Write + io::Read> Rx for Socket<T> {
    fn recv(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let m = self.web_socket.read_message().map_err(|e| match e {
                Error::Io(e) if e.kind() == io::ErrorKind::WouldBlock => e,
                _ => io::Error::new(
                    io::ErrorKind::Other,
                    anyhow!("could not read message: {}", e),
                ),
            })?;
            let received = match m {
                Message::Binary(received) => received,
                _ => continue,
            };

            if received.len() > buf.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    anyhow!(
                        "read buffer({}) is smaller than received({})",
                        buf.len(),
                        received.len()
                    ),
                ));
            }
            buf[..received.len()].copy_from_slice(&received);
            return Ok(received.len());
        }
    }
}

impl<T: io::Write + io::Read> Tx for Socket<T> {
    fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.web_socket
            .write_message(Message::binary(buf.to_vec()))
            .or_else(|e| match e {
                // ignore WouldBlock, because in that case, write_message will still queue the message.
                Error::Io(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
                _ => Err(io::Error::new(
                    io::ErrorKind::Other,
                    anyhow!("could not write message: {}", e),
                )),
            })?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.web_socket.write_pending().map_err(|e| match e {
            Error::Io(e) if e.kind() == io::ErrorKind::WouldBlock => e,
            _ => io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not write pending: {}", e),
            ),
        })
    }
}

impl<S: Session> AsRawFd for Socket<rustls::StreamOwned<S, net::TcpStream>> {
    fn as_raw_fd(&self) -> RawFd {
        let tcp_stream = self.web_socket.get_ref().get_ref();
        tcp_stream.as_raw_fd()
    }
}

pub struct TlsTcpListener {
    listener: net::TcpListener,
    tls_config: Arc<rustls::ServerConfig>,
    auth: BasicAuthentication,
}

impl TlsTcpListener {
    pub fn new(
        listener: net::TcpListener,
        cert_path: &str,
        key_path: &str,
        auth: BasicAuthentication,
    ) -> io::Result<Self> {
        let certs = load_certs(&cert_path)?;
        let keys = load_private_keys(&key_path)?;
        if keys.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                anyhow!("file {:} does not contain any private key", &key_path),
            ));
        }
        let mut tls_config = rustls::ServerConfig::new(rustls::NoClientAuth::new());
        tls_config
            .set_single_cert(certs, keys[0].clone())
            .or_else(|e| {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    anyhow!("could not set single cert: {:?}", e),
                ))
            })?;

        Ok(Self {
            listener,
            tls_config: Arc::new(tls_config),
            auth,
        })
    }

    pub fn accept(
        &self,
    ) -> io::Result<Socket<rustls::StreamOwned<rustls::ServerSession, net::TcpStream>>> {
        let (mut tcp_stream, _addr) = self.listener.accept()?;
        tcp_stream.set_nodelay(true)?;

        let mut tls_session = rustls::ServerSession::new(&self.tls_config);
        tls_session.complete_io(&mut tcp_stream)?;

        let tls_stream = rustls::StreamOwned::new(tls_session, tcp_stream);

        let callback = AutherizationCallback(self.auth.autherization());
        let web_socket = accept_hdr(tls_stream, callback).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not accept websocket: {}", e),
            )
        })?;

        Ok(Socket { web_socket })
    }
}

struct AutherizationCallback(String);

impl Callback for AutherizationCallback {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let autherization = &request.headers()[http::header::AUTHORIZATION];
        if autherization != &self.0 {
            let resp = Response::builder()
                .header(
                    http::header::WWW_AUTHENTICATE,
                    "Basic realm=\"access the service\"",
                )
                .status(http::StatusCode::UNAUTHORIZED)
                .body(None)
                .unwrap();
            Err(resp)
        } else {
            Ok(response)
        }
    }
}

pub struct TlsTcpConnector {
    hostname: webpki::DNSName,
    tls_config: Arc<rustls::ClientConfig>,
    auth: BasicAuthentication,
}

impl TlsTcpConnector {
    pub fn new(hostname: &str, ca_cert_path: &str, auth: BasicAuthentication) -> io::Result<Self> {
        let mut tls_config = rustls::ClientConfig::new();
        let ca_cert_file = fs::File::open(ca_cert_path)?;
        let mut ca_cert_reader = io::BufReader::new(ca_cert_file);
        tls_config
            .root_store
            .add_pem_file(&mut ca_cert_reader)
            .or_else(|_e| {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    anyhow!("could not add ca cert"),
                ))
            })?;

        let hostname = webpki::DNSNameRef::try_from_ascii_str(hostname)
            .or_else(|e| {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    anyhow!("invalid hostname: {:?}", e),
                ))
            })?
            .to_owned();

        Ok(Self {
            hostname,
            tls_config: Arc::new(tls_config),
            auth,
        })
    }

    pub fn connect<A: net::ToSocketAddrs>(
        &self,
        addr: A,
    ) -> io::Result<Socket<rustls::StreamOwned<rustls::ClientSession, net::TcpStream>>> {
        let mut tcp_stream = net::TcpStream::connect(addr)?;
        tcp_stream.set_nodelay(true)?;

        let mut tls_session = rustls::ClientSession::new(&self.tls_config, self.hostname.as_ref());
        tls_session.complete_io(&mut tcp_stream)?;

        let tls_stream = rustls::StreamOwned::new(tls_session, tcp_stream);

        let uri = format!("wss://{:}/ws", AsRef::<str>::as_ref(&self.hostname));
        let request = Request::builder()
            .uri(&uri)
            .header(http::header::AUTHORIZATION, self.auth.autherization())
            .body(())
            .unwrap();

        let (web_socket, _resp) = client(request, tls_stream).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not connect websocket: {}", e),
            )
        })?;

        Ok(Socket { web_socket })
    }
}

pub struct BasicAuthentication {
    pub username: String,
    pub password: String,
}

impl BasicAuthentication {
    fn autherization(&self) -> String {
        let creds = base64::encode(format!("{}:{}", self.username, self.password));
        format!("Basic {}", creds)
    }
}

fn load_private_keys(filename: &str) -> io::Result<Vec<rustls::PrivateKey>> {
    let keyfile = fs::File::open(filename)?;
    let mut reader = io::BufReader::new(keyfile);
    let mut keys = rustls::internal::pemfile::pkcs8_private_keys(&mut reader).or_else(|_e| {
        Err(io::Error::new(
            io::ErrorKind::Other,
            anyhow!("file contains invalid private key"),
        ))
    })?;

    if keys.is_empty() {
        reader.seek(io::SeekFrom::Start(0))?;
        keys = rustls::internal::pemfile::rsa_private_keys(&mut reader).or_else(|_e| {
            Err(io::Error::new(
                io::ErrorKind::Other,
                anyhow!("file contains invalid rsa private key"),
            ))
        })?;
    }

    Ok(keys)
}

fn load_certs(filename: &str) -> io::Result<Vec<rustls::Certificate>> {
    let certfile = fs::File::open(filename)?;
    let mut reader = io::BufReader::new(certfile);
    rustls::internal::pemfile::certs(&mut reader).or_else(|_e| {
        Err(io::Error::new(
            io::ErrorKind::Other,
            anyhow!("file contains invalid cert"),
        ))
    })
}
