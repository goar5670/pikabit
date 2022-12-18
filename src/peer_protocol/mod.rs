// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use byteorder::{BigEndian, ByteOrder};
use log::{error, info, warn};
use rand::{distributions::Alphanumeric, Rng};
use std::{
    cmp,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::concurrency::SharedRef;
use crate::metadata::Metadata;
use piece::PieceHandler;

mod bitfield;
mod message;
mod piece;

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
        TcpStream::connect(&self.address).await
    }

    pub fn unchoke_me(self: &mut Self) {
        self.state.client_choked = false;
    }

    pub fn choke_me(self: &mut Self) {
        self.state.client_choked = true;
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

    async fn _connect(self: &mut Self) {
        self.stream = SharedRef::new(Some(self.peer.get_handle().await.connect().await.unwrap()));
    }

    fn _handshake_payload(self: &Self, info_hash: &[u8; 20], client_id: &PeerId) -> Vec<u8> {
        let mut payload: Vec<u8> = Vec::new();
        payload.push(19);
        payload.extend("BitTorrent protocol".as_bytes());
        payload.extend([0u8; 8]);
        payload.extend(info_hash);
        payload.extend(client_id.to_string().as_bytes());

        payload
    }

    fn _verify_handshake(
        self: &Self,
        received: &[u8],
        sent: &[u8],
    ) -> Result<[u8; 20], &'static str> {
        if received[..20] != sent[..20] {
            error!(
                "Incorrect protocol name: {}",
                String::from_utf8(received[..20].to_vec()).unwrap()
            );
            return Err("Incorrect protocol name");
        } else if received[28..48] != sent[28..48] {
            error!(
                "Incorrect info hash: {:?}, expected: {:?}",
                &received[28..48],
                &sent[28..48]
            );
            return Err("Incorrect info hash");
        }

        if received[20..28] != sent[20..28] {
            warn!("Different protocol extension: {:?}", &received[20..28]);
        }

        Ok(received[received.len() - 20..].try_into().unwrap())
    }

    async fn _handshake(
        self: &Self,
        info_hash: &[u8; 20],
        client_id: &PeerId,
    ) -> Result<[u8; 20], &'static str> {
        let payload = self._handshake_payload(info_hash, client_id);
        let stream_ref = self.stream.clone();
        let mut stream = stream_ref.get_handle().await;
        let _ = stream.write(&payload).await;

        let mut buf = [0u8; 68];
        let n = stream.read_exact(&mut buf).await.unwrap();

        if n != payload.len() {
            warn!("Unexpected buffer length {}, expected {}", n, payload.len());
        }

        self._verify_handshake(&buf, &payload)
    }

    pub async fn run(self: &mut Self, metadata: &Metadata, client_id: &PeerId) {
        self._connect().await;
        let info_hash = &metadata.get_info_hash();
        if let Ok(peer_id) = self._handshake(info_hash, client_id).await {
            self.peer.get_handle().await.id = Some(PeerId::from(peer_id.as_ref()))
        }
        info!(
            "peer id: {}",
            String::from_utf8(self.peer.get_handle().await.id.as_ref().unwrap().to_vec()).unwrap()
        );

        let piece_handler: Arc<PieceHandler> = Arc::new(PieceHandler::new(&metadata).await);

        message::send(self.stream.clone(), 1, Some(2)).await;
        let keep_alive_handle = tokio::spawn(message::keep_alive(self.stream.clone()));
        let recv_loop_handle = tokio::spawn(message::recv_loop(
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
