use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::anyhow;
use async_tungstenite::{
    tungstenite::{error::Error as WsError, Message},
    WebSocketStream,
};
use futures_util::sink::Sink;
use futures_util::stream::Stream;
use smol::io::{AsyncRead, AsyncWrite};

pub struct Connection<T> {
    inner: WebSocketStream<T>,
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncRead for Connection<T> {
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
        buf.copy_from_slice(&received);

        Poll::Ready(Ok(received.len()))
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncWrite for Connection<T> {
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
