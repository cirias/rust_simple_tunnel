use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use smol::io::{AsyncRead, AsyncWrite};

use super::endpoint::*;

pub struct Retrier<T> {
    pub inner: T,
}

#[async_trait]
impl<T: 'static + Connector + Clone + Send + Sync + Unpin> Connector for Retrier<T> {
    type Connection = Pin<Box<RetryConnection<T>>>;

    async fn connect(&self) -> io::Result<Self::Connection> {
        let conn = self.inner.connect().await?;
        let connector = self.inner.clone();

        Ok(RetryConnection::new(connector, conn))
    }
}

pub struct RetryConnection<T: Connector> {
    connector: T,
    state: State<T>,
    _marker: std::marker::PhantomPinned,
}

enum State<T: Connector> {
    Ready(T::Connection),
    Connecting(Pin<Box<dyn Future<Output = io::Result<T::Connection>> + Send>>),
}

impl<T: 'static + Connector + Send + Sync> RetryConnection<T> {
    fn new(connector: T, conn: T::Connection) -> Pin<Box<Self>> {
        let c = RetryConnection {
            connector,
            state: State::Ready(conn),
            _marker: std::marker::PhantomPinned,
        };
        Box::pin(c)
    }

    fn poll_with_mut<D>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        mut f: impl FnMut(Pin<&mut T::Connection>, &mut Context) -> Poll<io::Result<D>>,
    ) -> Poll<io::Result<D>> {
        let this = unsafe { self.get_unchecked_mut() };
        let cr = unsafe {
            let ptr: *const T = &this.connector;
            &*ptr
        };
        loop {
            match &mut this.state {
                State::Ready(cn) => {
                    let res = futures_util::ready!(f(Pin::new(cn), cx));
                    if is_connection_error(&res) {
                        let fut = Box::pin(connect_with_retry(cr));
                        this.state = State::Connecting(fut);
                        continue;
                    }
                    return Poll::Ready(res);
                }
                State::Connecting(fut) => {
                    let cn = futures_util::ready!(Future::poll(fut.as_mut(), cx))?;
                    this.state = State::Ready(cn);
                }
            }
        }
    }
}

impl<T: 'static + Connector + Send + Sync> AsyncRead for RetryConnection<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_with_mut(cx, |cn, cx| cn.poll_read(cx, buf))
    }
}

impl<T: 'static + Connector + Send + Sync> AsyncWrite for RetryConnection<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_with_mut(cx, |cn, cx| cn.poll_write(cx, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_with_mut(cx, |cn, cx| cn.poll_flush(cx))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_with_mut(cx, |cn, cx| cn.poll_close(cx))
    }
}

fn is_connection_error<T>(res: &io::Result<T>) -> bool {
    if let Err(err) = res {
        return err.kind() == io::ErrorKind::UnexpectedEof;
    }
    false
}

async fn connect_with_retry<T: Connector>(connector: &T) -> io::Result<T::Connection> {
    loop {
        if let Ok(conn) = connector.connect().await {
            // TODO what are the errors we shouldn't retry
            return Ok(conn);
        }
        smol::Timer::new(std::time::Duration::from_secs(1)).await;
    }
}
