use std::io;
use std::net;
use std::pin::Pin;
use std::task::{Context, Poll};

use smol::io::{AsyncRead, AsyncWrite};

use super::endpoint::*;

pub struct ListenConnector {
    pub listener: net::TcpListener,
}

/*
 * impl ListenConnector {
 *     pub fn new(listener: net::TcpListener) -> Self {
 *         ListenConnector { listener }
 *     }
 * }
 */

impl Connector for ListenConnector {
    type Connection = AsyncTcpStream;

    fn connect(&self) -> io::Result<Self::Connection> {
        let (stream, _addr) = self.listener.accept()?;
        let inner = smol::Async::new(stream)?;
        Ok(AsyncTcpStream { inner })
    }
}

pub struct StreamConnector<A: net::ToSocketAddrs> {
    pub addr: A,
}

impl<A: net::ToSocketAddrs> Connector for StreamConnector<A> {
    type Connection = AsyncTcpStream;

    fn connect(&self) -> io::Result<Self::Connection> {
        let stream = net::TcpStream::connect(&self.addr)?;
        let inner = smol::Async::new(stream)?;
        Ok(AsyncTcpStream { inner })
    }
}

pub struct AsyncTcpStream {
    inner: smol::Async<net::TcpStream>,
}

impl AsRef<smol::Async<net::TcpStream>> for AsyncTcpStream {
    fn as_ref(&self) -> &smol::Async<net::TcpStream> {
        &self.inner
    }
}

impl TryClone for AsyncTcpStream {
    fn try_clone(&self) -> io::Result<Self> {
        let stream = self.inner.get_ref().try_clone()?;
        let inner = smol::Async::new(stream)?;
        Ok(AsyncTcpStream { inner })
    }
}

impl AsyncRead for AsyncTcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.inner).poll_read(cx, buf) }
    }
}

impl AsyncWrite for AsyncTcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.inner).poll_write(cx, buf) }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.inner).poll_flush(cx) }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.inner).poll_close(cx) }
    }
}
