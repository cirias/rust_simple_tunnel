use std::io;

use anyhow::anyhow;
use tungstenite::{accept, client, Message, WebSocket};

use super::endpoint::*;

pub struct ListenConnector<T> {
    pub connector: T,
}

impl<T: Connector + Send> Connector for ListenConnector<T> {
    type Connection = Connection<T::Connection>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let writer = {
            let conn = self.connector.connect()?;
            let socket = accept(conn).map_err(|_e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    anyhow!("could not accept websocket for write"),
                )
            })?;
            WebSocketReadWriter { inner: socket }
        };

        let reader = {
            let conn = self.connector.connect()?;
            let socket = accept(conn).map_err(|_e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    anyhow!("could not accept websocket for read"),
                )
            })?;
            WebSocketReadWriter { inner: socket }
        };

        Ok(Connection { writer, reader })
    }
}

pub struct ClientConnector<T> {
    pub connector: T,
    pub url: String,
}

impl<T: Connector + Send> Connector for ClientConnector<T> {
    type Connection = Connection<T::Connection>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let reader = {
            let conn = self.connector.connect()?;
            let (socket, _resp) = client(self.url.clone(), conn).map_err(|_e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    anyhow!("could not connect websocket for read"),
                )
            })?;
            WebSocketReadWriter { inner: socket }
        };

        let writer = {
            let conn = self.connector.connect()?;
            let (socket, _resp) = client(self.url.clone(), conn).map_err(|_e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    anyhow!("could not connect websocket for write"),
                )
            })?;
            WebSocketReadWriter { inner: socket }
        };

        Ok(Connection { writer, reader })
    }
}

pub struct WebSocketReadWriter<T> {
    inner: WebSocket<T>,
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
        self.inner
            .write_pending()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner
            .write_pending()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

pub struct Connection<T> {
    writer: WebSocketReadWriter<T>,
    reader: WebSocketReadWriter<T>,
}

impl<T: io::Write + io::Read + Send> io::Read for Connection<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

impl<T: io::Write + io::Read + Send> io::Write for Connection<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<T: io::Write + io::Read + Send> Split for Connection<T> {
    type Writer = WebSocketReadWriter<T>;
    type Reader = WebSocketReadWriter<T>;

    fn split(self) -> io::Result<(Self::Writer, Self::Reader)> {
        Ok((self.writer, self.reader))
    }
}
