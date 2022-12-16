// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use byteorder::{BigEndian, ByteOrder};
use rand::{distributions::Alphanumeric, Rng};
use std::{
    cmp,
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{Mutex, MutexGuard},
    time::{sleep, Duration},
};

use crate::metadata::{Info, Metadata};
use message_handler::PieceHandler;

mod bitfield;
mod message_handler;

#[derive(Debug, PartialEq)]
pub struct PeerId {
    inner: [u8; 20],
}

impl PeerId {
    pub fn new() -> Self {
        let timestamp = get_timestamp();

        const CLIENT_PREFIX: &'static str = "-pB";
        const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

        let cur_len = cmp::min(
            19,
            CLIENT_PREFIX.len() + CLIENT_VERSION.len() + timestamp.len(),
        );

        let rand_filler: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(19 - cur_len)
            .map(char::from)
            .collect();

        let peer_id = CLIENT_PREFIX.to_string() + CLIENT_VERSION + "-" + &rand_filler + &timestamp;

        debug_assert_eq!(peer_id.len(), 20);

        Self {
            inner: peer_id.as_bytes()[..20].try_into().unwrap(),
        }
    }

    fn to_vec(self: &Self) -> Vec<u8> {
        self.inner.to_vec()
    }
}

impl From<&[u8]> for PeerId {
    fn from(slice: &[u8]) -> Self {
        debug_assert_eq!(slice.len(), 20);

        Self {
            inner: slice.try_into().unwrap(),
        }
    }
}

impl ToString for PeerId {
    fn to_string(self: &Self) -> String {
        String::from_utf8(self.inner.to_vec()).unwrap()
    }
}

#[derive(Debug, PartialEq)]
struct ConnectionState {
    client_choked: bool,
    client_interested: bool,
    peer_choked: bool,
    peer_interested: bool,
}

impl ConnectionState {
    pub fn new() -> Self {
        Self {
            client_choked: true,
            client_interested: false,
            peer_choked: true,
            peer_interested: false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Peer {
    address: SocketAddr,
    state: ConnectionState,
    id: Option<PeerId>,
}

impl Peer {
    pub async fn connect(self: &Self) -> io::Result<TcpStream> {
        println!("{:?}", self);
        TcpStream::connect(&self.address).await
    }

    pub fn unchoke_me(self: &mut Self) {
        self.state.client_choked = false;
    }
}

impl From<[u8; 6]> for Peer {
    fn from(buf: [u8; 6]) -> Self {
        Self {
            address: SocketAddr::from((
                Ipv4Addr::from(BigEndian::read_u32(&buf)),
                BigEndian::read_u16(&buf[4..]),
            )),
            state: ConnectionState::new(),
            id: None,
        }
    }
}

pub struct SharedRef<T> {
    inner: Option<Arc<Mutex<T>>>,
}

impl<T> Clone for SharedRef<T> {
    fn clone(self: &Self) -> Self {
        Self {
            inner: self.inner.as_ref().map(|mapref| Arc::clone(&mapref)),
        }
    }
}

impl<T> SharedRef<T> {
    pub fn new(data: Option<T>) -> Self {
        Self {
            inner: data.map(|mapref| Arc::new(Mutex::new(mapref))),
        }
    }
    pub async fn get_handle<'a>(self: &'a Self) -> MutexGuard<'a, T> {
        self.inner.as_ref().unwrap().lock().await
    }
}

pub struct PeerHandler {
    peer: SharedRef<Peer>,
    stream: SharedRef<TcpStream>,
}

impl PeerHandler {
    pub fn new(peer: Peer) -> Self {
        Self {
            peer: SharedRef::new(Some(peer)),
            stream: SharedRef::new(None),
        }
    }

    async fn connect(self: &mut Self) {
        self.stream = SharedRef::new(Some(self.peer.get_handle().await.connect().await.unwrap()));
    }

    fn get_handshake_payload(self: &Self, info_hash: &[u8; 20], client_id: &PeerId) -> Vec<u8> {
        let mut payload: Vec<u8> = Vec::new();

        let protocol_name = "BitTorrent protocol".as_bytes();
        payload.push(19);
        payload.extend(protocol_name);
        payload.extend([0u8; 8]);
        payload.extend(info_hash);
        payload.extend(client_id.to_string().as_bytes());

        payload
    }

    async fn handshake(self: &Self, payload: Vec<u8>) -> [u8; 20] {
        let stream_ref = self.stream.clone();
        let mut stream = stream_ref.get_handle().await;
        let _ = stream.write(&payload).await;

        let mut buff = [0u8; 128];
        let buff_len = stream.read(&mut buff).await.unwrap();

        println!("buff_len = {},{:?}", buff_len, &buff[payload.len() - 20..]);
        // debug_assert_eq!(buff[..payload.len() - 20], payload[..payload.len() - 20]);
        // debug_assert_eq!(buff_len, payload.len());

        buff[buff_len - 20..buff_len].try_into().unwrap()
    }

    pub async fn run(self: &mut Self, metadata: &Metadata, client_id: &PeerId) {
        self.connect().await;
        let info_hash = &metadata.get_info_hash();
        self.peer.get_handle().await.id = Some(PeerId::from(
            self.handshake(self.get_handshake_payload(info_hash, client_id))
                .await
                .as_ref(),
        ));
        println!(
            "peer id: {}",
            String::from_utf8(self.peer.get_handle().await.id.as_ref().unwrap().to_vec()).unwrap()
        );

        let piece_handler: Arc<PieceHandler> = Arc::new(PieceHandler::new(&metadata));

        message_handler::send(self.stream.clone(), 1, Some(2)).await;
        let keep_alive_handle = tokio::spawn(message_handler::keep_alive(self.stream.clone()));
        let recv_loop_handle = tokio::spawn(message_handler::recv_loop(
            self.stream.clone(),
            self.peer.clone(),
            Arc::clone(&piece_handler),
        ));
        piece_handler
            .request_loop(self.stream.clone(), self.peer.clone())
            .await;

        keep_alive_handle.await.unwrap();
        recv_loop_handle.await.unwrap();
    }
}

fn get_timestamp() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH);
    let timestamp: String = now.unwrap().as_secs().to_string();

    timestamp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_from_slice() {
        let slice: [u8; 6] = [10, 123, 0, 47, 26, 225];
        let peer = Peer::from(slice);

        assert_eq!(
            peer,
            Peer {
                address: SocketAddr::from((Ipv4Addr::new(10, 123, 0, 47), 6881)),
                state: ConnectionState::new(),
                id: None,
            }
        );
    }
}
