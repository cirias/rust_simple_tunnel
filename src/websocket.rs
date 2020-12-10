use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use anyhow::anyhow;
use tungstenite::{
    accept_hdr, client,
    handshake::server::{Callback, ErrorResponse, Request, Response},
    http, Error, Message, WebSocket,
};

use super::endpoint::*;

pub struct Authentication {
    pub username: String,
    pub password: String,
}

impl Authentication {
    fn autherization(&self) -> String {
        let creds = base64::encode(format!("{}:{}", self.username, self.password));
        format!("Basic {}", creds)
    }
}

pub struct ListenConnector<T> {
    connector: T,
    autherization: String,
}

impl<T> ListenConnector<T> {
    pub fn new(connector: T, auth: Authentication) -> Self {
        Self {
            connector,
            autherization: auth.autherization(),
        }
    }
}

impl<T: Connector + Send> Connector for ListenConnector<T> {
    type Connection = WebSocketReadWriter<T::Connection>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect()?;
        let callback = AutherizationCallback(&self.autherization);
        let socket = accept_hdr(conn, callback).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not accept websocket: {}", e),
            )
        })?;
        Ok(WebSocketReadWriter { inner: socket })
    }
}

struct AutherizationCallback<'a>(&'a str);

impl<'a> Callback for AutherizationCallback<'a> {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let autherization = &request.headers()[http::header::AUTHORIZATION];
        if autherization != self.0 {
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

pub struct ClientConnector<T> {
    connector: T,
    uri: String,
    autherizatoin: String,
}

impl<T> ClientConnector<T> {
    pub fn new(connector: T, uri: String, auth: Authentication) -> Self {
        Self {
            connector,
            uri,
            autherizatoin: auth.autherization(),
        }
    }
}

impl<T: Connector + Send> Connector for ClientConnector<T> {
    type Connection = WebSocketReadWriter<T::Connection>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect()?;

        let request = Request::builder()
            .uri(&self.uri)
            .header(http::header::AUTHORIZATION, &self.autherizatoin)
            .body(())
            .unwrap();

        let (socket, _resp) = client(request, conn).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not connect websocket: {}", e),
            )
        })?;
        Ok(WebSocketReadWriter { inner: socket })
    }
}

pub struct WebSocketReadWriter<T> {
    inner: WebSocket<T>,
}

impl<T: AsRawFd> AsRawFd for WebSocketReadWriter<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.get_ref().as_raw_fd()
    }
}

impl<T: io::Write + io::Read> io::Read for WebSocketReadWriter<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let m = self.inner.read_message().map_err(|e| match e {
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

impl<T: io::Write + io::Read> io::Write for WebSocketReadWriter<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner
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
        self.inner.write_pending().map_err(|e| match e {
            Error::Io(e) if e.kind() == io::ErrorKind::WouldBlock => e,
            _ => io::Error::new(
                io::ErrorKind::Other,
                anyhow!("could not write pending: {}", e),
            ),
        })
    }
}
