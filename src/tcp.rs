use std::io;
use std::net;

use super::endpoint::*;

pub struct ListenConnector {
    pub listener: net::TcpListener,
}

impl ListenConnector {
    pub fn new<A: net::ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let listener = net::TcpListener::bind(addr)?;
        Ok(Self { listener })
    }
}

impl Connector for ListenConnector {
    type Connection = Connection<net::TcpStream>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let (stream, _addr) = self.listener.accept()?;
        Ok(Connection { inner: stream })
    }
}

pub struct StreamConnector<A: net::ToSocketAddrs> {
    pub addr: A,
}

impl<A: net::ToSocketAddrs> Connector for StreamConnector<A> {
    type Connection = Connection<net::TcpStream>;

    fn connect(&self) -> io::Result<Self::Connection> {
        let stream = net::TcpStream::connect(&self.addr)?;
        Ok(Connection { inner: stream })
    }
}
