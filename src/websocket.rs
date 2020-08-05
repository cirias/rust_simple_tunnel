use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::anyhow;
use async_trait::async_trait;
use async_tungstenite::{
    accept_async, client_async,
    tungstenite::{error::Error as WsError, Message},
    WebSocketStream,
};
use futures_util::sink::Sink;
use futures_util::stream::{SplitSink, SplitStream, Stream};
use smol::io::{AsyncRead, AsyncWrite};

use super::endpoint::*;

pub struct Listener<T> {
    pub connector: T,
}

#[async_trait]
impl<T: Connector + Send + Sync> Connector for Listener<T> {
    type Connection = Connection<WebSocketStream<T::Connection>>;

    async fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect().await?;
        let socket = accept_async(conn)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Connection { inner: socket })
    }
}

pub struct Client<T> {
    pub connector: T,
    pub url: String,
}

#[async_trait]
impl<T: Connector + Send + Sync> Connector for Client<T> {
    type Connection = Connection<WebSocketStream<T::Connection>>;

    async fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect().await?;
        let (socket, _resp) = client_async(self.url.clone(), conn)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Connection { inner: socket })
    }
}

pub struct Connection<T> {
    inner: T,
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Split for Connection<WebSocketStream<T>> {
    type Write = Connection<SplitSink<WebSocketStream<T>, Message>>;
    type Read = Connection<SplitStream<WebSocketStream<T>>>;
    fn try_split(self) -> io::Result<(Self::Write, Self::Read)> {
        use futures_util::stream::StreamExt;
        let (w, r) = self.inner.split();
        Ok((Connection { inner: w }, Connection { inner: r }))
    }
}

impl<T: Stream<Item = Result<Message, WsError>> + Unpin> AsyncRead for Connection<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let inner = Pin::new(&mut self.inner);

        let r: Result<Message, WsError> = match futures_util::ready!(inner.poll_next(cx)) {
            None => return Poll::Ready(Ok(0)),
            Some(r) => r,
        };
        let m = match r {
            Err(e) => return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Ok(m) => m,
        };
        let received = match m {
            Message::Binary(received) => received,
            _ => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    anyhow!("received message is not binary"),
                )))
            }
        };
        // TODO use an internal buffer
        if received.len() > buf.len() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                anyhow!("read buffer is smaller than received"),
            )));
        }
        buf[..received.len()].copy_from_slice(&received);

        Poll::Ready(Ok(received.len()))
    }
}

impl<T: Sink<Message, Error = WsError> + Unpin> AsyncWrite for Connection<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut inner = Pin::new(&mut self.inner);

        match inner
            .as_mut()
            .poll_ready(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(_) => {
                inner
                    .start_send(Message::binary(buf.to_vec()))
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            }
        };

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let inner = Pin::new(&mut self.inner);

        inner
            .poll_flush(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut inner = Pin::new(&mut self.inner);

        match inner
            .as_mut()
            .poll_ready(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(_) => {
                inner
                    .start_send(Message::Close(None))
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            }
        };

        futures_util::ready!(Pin::new(&mut self.inner).poll_flush(cx))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Poll::Ready(Ok(()))
    }
}
