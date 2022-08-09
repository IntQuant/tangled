use std::{
    io, net::{SocketAddr, UdpSocket}, sync::{atomic::AtomicBool, Arc}
};

use crossbeam::{
    self, atomic::AtomicCell, channel::{unbounded, Receiver, Sender}
};

use error::NetError;
use reactor::{Destination, Reliability, RemotePeer, Settings, Shared};

const DATAGRAM_MAX_LEN: usize = 1500;
const MAX_MESSAGE_LEN: usize = 1200;

pub mod error;
mod reactor;
mod util;

struct Datagram {
    pub size: usize,
    pub data: [u8; DATAGRAM_MAX_LEN],
}

pub type PeerId = u16;
pub type SeqId = u16;

pub struct ReceivedMessage {
    pub src: PeerId,
    pub data: Vec<u8>,
}

pub struct Message {
    pub dst: Destination,
    pub data: Vec<u8>,
    pub reliability: Reliability,
}

#[derive(Default)]
pub enum PeerState {
    #[default]
    PendingConnection,
    Connected,
    Disconnected,
}

type Channel<T> = (Sender<T>, Receiver<T>);

#[derive(Clone)]
pub struct Peer {
    shared: Arc<Shared>,
}

impl Peer {
    fn new(
        bind_addr: SocketAddr,
        host_addr: Option<SocketAddr>,
        settings: Option<Settings>,
    ) -> io::Result<Self> {
        let socket = UdpSocket::bind(bind_addr)?;
        //socket.set_read_timeout(Some(Duration::from_millis(500)))?;
        let shared = Arc::new(Shared {
            socket,
            inbound_channel: unbounded(),
            outbound_channel: unbounded(),
            keep_alive: AtomicBool::new(true),
            host_addr,
            peer_state: Default::default(),
            remote_peers: Default::default(),
            max_packets_per_second: 256,
            my_id: AtomicCell::new(if host_addr.is_none() { Some(0) } else { None }),
            settings: settings.unwrap_or_default(),
        });
        if host_addr.is_none() {
            shared.remote_peers.insert(0, RemotePeer::default());
        }
        reactor::Reactor::start(Arc::clone(&shared));
        Ok(Peer { shared })
    }

    pub fn host(bind_addr: SocketAddr, settings: Option<Settings>) -> io::Result<Self> {
        Self::new(bind_addr, None, settings)
    }

    pub fn connect(host_addr: SocketAddr, settings: Option<Settings>) -> io::Result<Self> {
        Self::new("0.0.0.0:0".parse().unwrap(), Some(host_addr), settings)
    }

    pub fn send(
        &self,
        dst: Destination,
        data: Vec<u8>,
        reliability: Reliability,
    ) -> Result<(), NetError> {
        if data.len() > MAX_MESSAGE_LEN {
            return Err(NetError::MessageTooLong);
        }
        if reliability == Reliability::Unreliable
            && self.shared.outbound_channel.0.len() * 2 > self.shared.max_packets_per_second.into()
        {
            return Err(NetError::Dropped);
        }
        self.shared.outbound_channel.0.send(Message {
            dst,
            data,
            reliability,
        })?;
        Ok(())
    }

    pub fn recv(&self) -> impl Iterator<Item = ReceivedMessage> + '_ {
        self.shared.inbound_channel.1.try_iter()
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.shared
            .keep_alive
            .store(false, std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod test {
    use std::{thread, time::Duration};

    use crate::{reactor::Settings, Peer};

    #[test_log::test]
    fn test_peer() {
        let settings = Some(Settings {
            confirm_max_period: Duration::from_millis(100),
            connection_timeout: Duration::from_millis(1000),
            ..Default::default()
        });
        let addr = "127.0.0.1:56001".parse().unwrap();
        let host = Peer::host(addr, settings.clone()).unwrap();
        assert_eq!(host.shared.remote_peers.len(), 1);
        let peer = Peer::connect(addr, settings.clone()).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(peer.shared.remote_peers.len(), 2);
        assert_eq!(host.shared.remote_peers.len(), 2);
        let data = vec![128, 51, 32];
        peer.send(
            crate::reactor::Destination::One(0),
            data.clone(),
            crate::reactor::Reliability::Reliable,
        )
        .unwrap();
        thread::sleep(Duration::from_millis(10));
        assert_eq!(host.recv().next().unwrap().data, data);
        drop(peer);
        thread::sleep(Duration::from_millis(1200));
        assert_eq!(host.shared.remote_peers.len(), 1);
    }
}
