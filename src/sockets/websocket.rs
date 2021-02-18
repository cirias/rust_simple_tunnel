use std::os::unix::io::{AsRawFd, RawFd};
use std::{io, net};

use anyhow::anyhow;
use tungstenite::{
    accept_hdr, client,
    handshake::server::{Callback, ErrorResponse, Request, Response},
    http, Error, Message, WebSocket,
};

use native_tls::{Identity, Protocol, TlsAcceptor, TlsConnector, TlsStream};
use std::fs::File;
use std::io::Read;

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

impl AsRawFd for Socket<TlsStream<net::TcpStream>> {
    fn as_raw_fd(&self) -> RawFd {
        let tcp_stream = self.web_socket.get_ref().get_ref();
        tcp_stream.as_raw_fd()
    }
}

pub struct TlsTcpListener {
    pub listener: net::TcpListener,
    pub pkcs12_path: String,
    pub pkcs12_password: String,
    pub auth: BasicAuthentication,
}

impl TlsTcpListener {
    pub fn accept(&self) -> io::Result<Socket<TlsStream<net::TcpStream>>> {
        let mut file = File::open(&self.pkcs12_path)?;
        let mut identity = vec![];
        file.read_to_end(&mut identity)?;
        let identity = Identity::from_pkcs12(&identity, &self.pkcs12_password)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let tls_acceptor = TlsAcceptor::builder(identity)
            .min_protocol_version(Some(Protocol::Tlsv12))
            .build()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let (tcp_stream, _addr) = self.listener.accept()?;

        let tls_stream = tls_acceptor.accept(tcp_stream).map_err(|e| match e {
            native_tls::HandshakeError::Failure(e) => io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not connect tls stream: failure: {:?}", e),
            ),
            native_tls::HandshakeError::WouldBlock(_) => io::Error::new(
                io::ErrorKind::WouldBlock,
                anyhow!("could not connect tls stream: would block"),
            ),
        })?;

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

pub fn connect_tls_tcp<A: net::ToSocketAddrs>(
    addr: A,
    hostname: &str,
    accept_invalid_certs: bool,
    auth: BasicAuthentication,
) -> io::Result<Socket<TlsStream<net::TcpStream>>> {
    let tls_connector = TlsConnector::builder()
        .danger_accept_invalid_certs(accept_invalid_certs)
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let tcp_stream = net::TcpStream::connect(addr)?;

    let tls_stream = tls_connector
        .connect(hostname, tcp_stream)
        .map_err(|e| match e {
            native_tls::HandshakeError::Failure(e) => io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not connect tls stream: failure: {:?}", e),
            ),
            native_tls::HandshakeError::WouldBlock(_) => io::Error::new(
                io::ErrorKind::WouldBlock,
                anyhow!("could not connect tls stream: would block"),
            ),
        })?;

    let uri = format!("wss://{:}/ws", hostname);
    let request = Request::builder()
        .uri(&uri)
        .header(http::header::AUTHORIZATION, auth.autherization())
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
