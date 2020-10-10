use std::io;
use std::os::unix::io::{AsRawFd, RawFd};

use anyhow::anyhow;
use tungstenite::{accept, client, Message, WebSocket};

use super::endpoint::*;

pub struct ListenConnector<T> {
    pub connector: T,
}

impl<T: Connector + Send> Connector for ListenConnector<T> {
    type Connection = WebSocketReadWriter<T::Connection>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect()?;
        let socket = accept(conn).map_err(|_e| {
            io::Error::new(io::ErrorKind::Other, anyhow!("could not accept websocket"))
        })?;
        Ok(WebSocketReadWriter { inner: socket })
    }
}

pub struct ClientConnector<T> {
    pub connector: T,
    pub url: String,
}

impl<T: Connector + Send> Connector for ClientConnector<T> {
    type Connection = WebSocketReadWriter<T::Connection>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect()?;
        let (socket, _resp) = client(self.url.clone(), conn).map_err(|_e| {
            io::Error::new(io::ErrorKind::Other, anyhow!("could not connect websocket"))
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
            let m = self
                .inner
                .read_message()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
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
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        // TODO ignore WouldBlock, because in that case, write_message will still queue the message.
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner
            .write_pending()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}
