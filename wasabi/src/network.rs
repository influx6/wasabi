use bytes;
use mio;
use mio::net;
use slab::Slab;
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::net::Shutdown;
use std::net::SocketAddr;
use std::result;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time;

#[derive(Debug)]
enum NetTcp {
    Listener(mio::net::TcpListener),
    Stream(mio::net::TcpStream),
}

#[derive(Debug)]
pub struct NetLoop {
    slab: Slab<NetTcp>,
    poll: Arc<mio::Poll>,
    pub is_listening: bool,
    event_receiver: mpsc::Receiver<mio::event::Event>,
}

pub fn event_to_ints(event: &mio::Event) -> ((i64, i64)) {
    // 0 << 0 | 1 << 1 | 0 << 2 | 1 << 3

    let unix_ready = mio::unix::UnixReady::from(event.readiness());
    let state: i64 = if unix_ready.is_readable() { 1 } else { 0 }
        | if unix_ready.is_writable() {
            1 << 1
        } else {
            0 << 1
        }
        | if unix_ready.is_hup() { 1 << 2 } else { 0 << 2 }
        | if unix_ready.is_error() {
            1 << 3
        } else {
            0 << 3
        };
    (event.token().0 as i64, state)
}

pub fn addr_to_bytes(addr: SocketAddr, b: &mut [u8]) -> Result<()> {
    match addr {
        SocketAddr::V4(a) => {
            b[0..4].copy_from_slice(&a.ip().octets());
            b[4..6].copy_from_slice(&bytes::u16_as_u8_le(a.port()));
            Ok(())
        }
        SocketAddr::V6(_) => Err(Error::new(ErrorKind::Other, "IPV6 not supported")),
    }
}

impl NetLoop {
    pub fn new() -> Self {
        let poll = Arc::new(mio::Poll::new().unwrap());
        let (event_sender, event_receiver) = mpsc::channel();
        let t_poll = poll.clone();
        thread::spawn(move || {
            let mut events = mio::Events::with_capacity(1024);
            loop {
                t_poll.poll(&mut events, None).unwrap();
                for event in events.iter() {
                    event_sender.send(event).unwrap();
                }
            }
        });
        Self {
            slab: Slab::new(),
            is_listening: false,
            poll,
            event_receiver,
        }
    }
    pub fn try_recv(&mut self) -> result::Result<mio::Event, mpsc::TryRecvError> {
        let event = self.event_receiver.try_recv()?;
        Ok(event)
    }
    pub fn recv(&mut self) -> result::Result<mio::Event, mpsc::RecvError> {
        let event = self.event_receiver.recv()?;
        Ok(event)
    }
    pub fn recv_timeout(
        &mut self,
        timeout: time::Duration,
    ) -> result::Result<mio::Event, mpsc::RecvTimeoutError> {
        let event = self.event_receiver.recv_timeout(timeout)?;
        Ok(event)
    }
    pub fn tcp_listen(&mut self, addr: &SocketAddr) -> Result<usize> {
        let listener = net::TcpListener::bind(addr)?;
        let id = self.slab.insert(NetTcp::Listener(listener));
        self.poll.register(
            self.get_listener_ref(id)?,
            mio::Token(id),
            mio::Ready::readable() | mio::Ready::writable(),
            // https://carllerche.github.io/mio/mio/struct.Poll.html#edge-triggered-and-level-triggered
            mio::PollOpt::edge(),
        )?;
        self.is_listening = true;
        Ok(id)
    }
    pub fn tcp_connect(&mut self, addr: &SocketAddr) -> Result<usize> {
        let stream = net::TcpStream::connect(addr)?;
        self.is_listening = true;
        self.register_stream(stream)
    }
    fn register_stream(&mut self, stream: mio::net::TcpStream) -> Result<usize> {
        let id = self.slab.insert(NetTcp::Stream(stream));
        self.poll.register(
            self.get_stream_ref(id)?,
            mio::Token(id),
            mio::Ready::readable() | mio::Ready::writable(),
            mio::PollOpt::edge(),
        )?;
        Ok(id)
    }
    pub fn tcp_accept(&mut self, id: usize) -> Result<usize> {
        if let Some(err) = self.get_listener_ref(id)?.take_error()? {
            println!("accept error {:?}", err);
        }
        let (stream, _) = self.get_listener_ref(id)?.accept()?;
        self.register_stream(stream)
    }
    pub fn get_error(&mut self, id: usize) -> Result<Option<Error>> {
        match self.slab.get(id) {
            Some(ntcp) => match ntcp {
                NetTcp::Listener(listener) => listener.take_error(),
                NetTcp::Stream(stream) => stream.take_error(),
            },
            None => Err(Error::new(
                ErrorKind::Other,
                "Network object not found in slab",
            )),
        }
    }
    pub fn local_addr(&self, i: usize) -> Result<SocketAddr> {
        match self.slab_get(i)? {
            NetTcp::Listener(listener) => listener.local_addr(),
            NetTcp::Stream(stream) => stream.local_addr(),
        }
    }
    pub fn peer_addr(&self, i: usize) -> Result<SocketAddr> {
        self.get_stream_ref(i)?.peer_addr()
    }
    pub fn read_stream(&self, i: usize, b: &mut [u8]) -> Result<usize> {
        if let Some(err) = self.get_stream_ref(i)?.take_error()? {
            println!("stream error {:?}", err);
        }
        self.get_stream_ref(i)?.read(b)
    }
    pub fn shutdown(&mut self, i: usize, how: Shutdown) -> Result<()> {
        self.get_stream_ref(i)?.shutdown(how)
    }
    pub fn write_stream(&self, i: usize, b: &[u8]) -> Result<usize> {
        self.get_stream_ref(i)?.write(b)
    }
    pub fn close(&mut self, i: usize) -> Result<()> {
        if self.slab.contains(i) {
            self.slab.remove(i); // value is dropped and connection is closed
        };
        Ok(())
    }
    fn slab_get(&self, i: usize) -> Result<&NetTcp> {
        match self.slab.get(i) {
            Some(ntcp) => Ok(ntcp),
            None => Err(Error::new(
                ErrorKind::Other,
                "Network object not found in slab",
            )),
        }
    }
    fn get_listener_ref(&self, i: usize) -> Result<&mio::net::TcpListener> {
        match self.slab_get(i)? {
            NetTcp::Listener(listener) => Ok(listener),
            _ => Err(Error::new(
                ErrorKind::Other,
                "Network object not found in slab",
            )),
        }
    }
    fn get_stream_ref(&self, i: usize) -> Result<&mio::net::TcpStream> {
        match self.slab_get(i)? {
            NetTcp::Stream(s) => Ok(s),
            _ => Err(Error::new(
                ErrorKind::Other,
                "Network object not found in slab",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn as_u16_le(array: &[u8]) -> u16 {
        u16::from(array[0]) | (u16::from(array[1]) << 8)
    }

    #[test]
    fn test_addr_to_bytes() {
        let mut mem = vec![0u8; 6];
        addr_to_bytes("1.2.3.4:100".parse().unwrap(), &mut mem).unwrap();
        assert_eq!(mem, [1, 2, 3, 4, 100, 0]);

        let mut mem = vec![0u8; 6];
        addr_to_bytes("127.0.0.1:34254".parse().unwrap(), &mut mem).unwrap();
        assert_eq!(as_u16_le(&mem[4..6]), 34254u16);
    }

    #[test]
    fn listen_connect_read_write() {
        let mut nl = NetLoop::new();
        let listener = nl.tcp_listen(&"127.0.0.1:34254".parse().unwrap()).unwrap();
        let conn = nl.tcp_connect(&"127.0.0.1:34254".parse().unwrap()).unwrap();

        let to_write = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        loop {
            let event = nl.event_receiver.recv().unwrap();
            if event.token().0 == conn && event.readiness().is_writable() {
                nl.write_stream(event.token().0, &to_write).unwrap();
            } else if event.token().0 == listener && event.readiness().is_readable() {
                nl.tcp_accept(event.token().0).unwrap();
            } else if event.token().0 == 2 && event.readiness().is_readable() {
                let mut b = [0; 9];
                nl.read_stream(event.token().0, &mut b).unwrap();
                assert_eq!(b, to_write);
                break;
            // listener connection
            } else {
                continue;
            }
        }
    }
}
