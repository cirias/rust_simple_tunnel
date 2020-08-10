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
use futures_util::stream::Stream;
// use futures_util::stream::{SplitSink, SplitStream, Stream};
use smol::io::{AsyncRead, AsyncWrite};

use super::endpoint::*;

pub struct ListenConnector<T> {
    pub connector: T,
}

#[async_trait]
impl<T: Connector + Send + Sync> Connector for ListenConnector<T> {
    type Connection = Connection<WebSocketStream<T::Connection>>;

    async fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect().await?;
        let socket = accept_async(conn)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Connection {
            inner: socket,
            write_state: Default::default(),
            close_state: Default::default(),
        })
    }
}

pub struct ClientConnector<T> {
    pub connector: T,
    pub url: String,
}

#[async_trait]
impl<T: Connector + Send + Sync> Connector for ClientConnector<T> {
    type Connection = Connection<WebSocketStream<T::Connection>>;

    async fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.connector.connect().await?;
        let (socket, _resp) = client_async(self.url.clone(), conn)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(Connection {
            inner: socket,
            write_state: Default::default(),
            close_state: Default::default(),
        })
    }
}

pub struct Connection<T> {
    inner: T,
    write_state: MessageState,
    close_state: MessageState,
}
/*
 *
 * impl<T: AsyncRead + AsyncWrite + Unpin + Send> Split for Connection<WebSocketStream<T>> {
 *     type Write = Connection<SplitSink<WebSocketStream<T>, Message>>;
 *     type Read = Connection<SplitStream<WebSocketStream<T>>>;
 *     fn try_split(self) -> io::Result<(Self::Write, Self::Read)> {
 *         use futures_util::stream::StreamExt;
 *         let (w, r) = self.inner.split();
 *         Ok((
 *             Connection {
 *                 inner: w,
 *                 write_state: Default::default(),
 *                 close_state: Default::default(),
 *             },
 *             Connection {
 *                 inner: r,
 *                 write_state: Default::default(),
 *                 close_state: Default::default(),
 *             },
 *         ))
 *     }
 * }
 */

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
                anyhow!(
                    "read buffer({}) is smaller than received({})",
                    buf.len(),
                    received.len()
                ),
            )));
        }
        buf[..received.len()].copy_from_slice(&received);

        Poll::Ready(Ok(received.len()))
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncWrite for Connection<WebSocketStream<T>> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut state = self.write_state.clone();
        let mut inner = Pin::new(&mut self.inner);

        loop {
            if let Some(p) = (&mut state).step(cx, inner.as_mut(), || Message::binary(buf.to_vec()))
            {
                self.write_state = state;
                return p.map_ok(|_| buf.len());
            }
        }
        /*
         *         let mut inner = Pin::new(&mut self.inner);
         *
         *         match inner
         *             .as_mut()
         *             .poll_ready(cx)
         *             .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
         *         {
         *             Poll::Pending => return Poll::Pending,
         *             Poll::Ready(_) => {
         *                 inner
         *                     .start_send(Message::binary(buf.to_vec()))
         *                     .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
         *             }
         *         };
         *
         *         Poll::Ready(Ok(buf.len()))
         */
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let inner = Pin::new(&mut self.inner);

        inner
            .poll_flush(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    // TODO **BUG**
    // This will send send multiple close message when `poll_ready` returns Ready but
    // `poll_flush` does not.
    //
    // One solution is to store a future, and poll that future here.
    // But base on my current rust knowledge, to achieve that without using unsafe,
    // the only way is to use Arc and Mutex, which is quite complex.
    // And it becomes worse when the poll fn has its own arguments.
    // It will be more memory copy, and it has to assert that
    // the args are the same for each poll call of the same future.
    //
    // This makes me feel, maybe using AsyncRead/AsyncWrite as the contract is not a good idea.
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut state = self.close_state.clone();
        let mut inner = Pin::new(&mut self.inner);

        // TODO should close the inner connection after flush
        loop {
            if let Some(p) = (&mut state).step(cx, inner.as_mut(), || Message::Close(None)) {
                self.close_state = state;
                return p;
            }
        }
    }
}

#[derive(Clone)]
enum MessageState {
    MessageNotSent,
    MessageSent,
}

impl Default for MessageState {
    fn default() -> Self {
        MessageState::MessageNotSent
    }
}

impl MessageState {
    fn poll_send_message<S: Sink<Message, Error = WsError> + Unpin>(
        self: &mut Self,
        cx: &mut Context<'_>,
        mut sink: Pin<&mut S>,
        message: impl FnOnce() -> Message,
    ) -> Poll<io::Result<()>> {
        match sink
            .as_mut()
            .poll_ready(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                sink.as_mut()
                    .start_send((message)())
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                *self = MessageState::MessageSent;
                Poll::Ready(Ok(()))
            }
        }
    }

    fn poll_flush<S: Sink<Message, Error = WsError> + Unpin>(
        self: &mut Self,
        cx: &mut Context<'_>,
        sink: Pin<&mut S>,
    ) -> Poll<io::Result<()>> {
        futures_util::ready!(sink.poll_flush(cx))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        *self = MessageState::MessageNotSent;
        Poll::Ready(Ok(()))
    }

    fn step<S: Sink<Message, Error = WsError> + Unpin>(
        self: &mut Self,
        cx: &mut Context<'_>,
        sink: Pin<&mut S>,
        message: impl FnOnce() -> Message,
    ) -> Option<Poll<io::Result<()>>> {
        match &self {
            MessageState::MessageNotSent => match self.poll_send_message(cx, sink, message) {
                Poll::Ready(Ok(())) => None,
                o => Some(o),
            },
            MessageState::MessageSent => Some(self.poll_flush(cx, sink)),
        }
    }
}
