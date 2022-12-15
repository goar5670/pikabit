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

        let mut bytes = [0u8; 20];
        bytes.copy_from_slice(&peer_id.as_bytes()[..20]);

        Self { inner: bytes }
    }

    fn to_vec(self: &Self) -> Vec<u8> {
        self.inner.to_vec()
    }
}

impl From<&[u8]> for PeerId {
    fn from(slice: &[u8]) -> Self {
        debug_assert_eq!(slice.len(), 20);

        let mut bytes = [0u8; 20];
        bytes.copy_from_slice(&slice[..20]);

        Self { inner: bytes }
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

#[derive(Debug)]
struct BlockRequest(u32, u32, u32);

impl BlockRequest {
    pub async fn send(self: &Self, handler: &SharedRef<TcpStream>) {
        let mut buf: Vec<u8> = vec![];
        buf.write_u32(13).await.unwrap();
        buf.push(6);
        buf.write_u32(self.0).await.unwrap();
        buf.write_u32(self.1).await.unwrap();
        buf.write_u32(self.2).await.unwrap();

        let _ = handler.get_handle().await.write(&mut buf).await;
        println!("sent request piece message, {:?}", self);
    }
}

#[derive(Clone)]
struct Bitfield {
    inner: Vec<u8>,
    cnt_marked: u32,
    length: u32,
}

impl Bitfield {
    pub fn new(size: u32) -> Self {
        Self {
            length: size,
            cnt_marked: 0,
            inner: vec![0; ((size + 7) / 8) as usize],
        }
    }

    fn _get_bit_index(index: u32) -> (usize, u8) {
        ((index / 8) as usize, (index % 8) as u8)
    }

    fn _check_offset(byte: u8, offset: u8) -> bool {
        (byte & (1 << offset)) == 1
    }

    pub fn len(self: &Self) -> u32 {
        self.length
    }

    pub fn rem(self: &Self) -> u32 {
        self.length - self.cnt_marked
    }

    pub fn get(self: &Self, index: u32) -> bool {
        if index >= self.length {
            panic!("Requested index is out of bounds");
        }

        let (byte_index, offset) = Self::_get_bit_index(index);
        Self::_check_offset(*self.inner.get(byte_index).unwrap(), offset)
    }

    pub fn set(self: &mut Self, index: u32) {
        if index >= self.length {
            panic!("Requested index is out of bounds");
        }

        let (byte_index, offset) = Self::_get_bit_index(index);
        let byte = self.inner.get_mut(byte_index).unwrap();
        if *byte & (1 << offset) == 1 {
            return;
        }

        println!("before set {}", byte);
        *byte &= 1 << offset;
        self.cnt_marked += 1;
        println!("after set {}", self.inner.get(byte_index).unwrap());
    }
}

impl From<(&[u8], u32)> for Bitfield {
    fn from((buf, size): (&[u8], u32)) -> Self {
        let mut bitfield = Bitfield::new(size);
        bitfield.inner = buf.to_vec();

        for i in buf {
            bitfield.cnt_marked += i.count_ones();
        }

        bitfield
    }
}

mod message_handler {
    use super::*;
    use crate::constants::message_ids::*;

    async fn _send(stream_ref: &SharedRef<TcpStream>, length: u32, id: Option<u8>) {
        let mut buf: Vec<u8> = vec![];
        buf.write_u32(length).await.unwrap();
        if length != 0 {
            let message_id = id.unwrap();
            buf.push(message_id);
        }

        let _ = stream_ref.get_handle().await.write(&mut buf).await;
        println!("sent message, id {:?}, buffer: {:?}", id, buf);
    }

    async fn _recv_len(stream_ref: &SharedRef<TcpStream>) -> u32 {
        let mut buf = [0u8; 4];
        let _ = stream_ref.get_handle().await.read_exact(&mut buf).await;
        BigEndian::read_u32(&buf)
    }

    async fn _recv(
        stream_ref: &SharedRef<TcpStream>,
        buf: &mut [u8],
        n: u32,
    ) -> Result<(), std::io::Error> {
        let _ = stream_ref.get_handle().await.read_exact(buf).await;

        if buf[0] != PIECE {
            println!("recieved message, len: {} {:?}", n, buf.to_vec());
        } else {
            println!(
                "received piece, len: {}, message_id: {}, index: {}, offset: {}",
                n,
                buf[0],
                BigEndian::read_u32(&buf[1..]),
                BigEndian::read_u32(&buf[5..]),
            )
        }

        Ok(())
    }

    pub async fn send(
        stream_ref: SharedRef<TcpStream>,
        length: u32,
        id: Option<u8>,
        // payload: Option<&mut Vec<u8>>,
    ) {
        _send(&stream_ref, length, id).await;
    }

    pub async fn recv_loop<'a>(
        stream_ref: SharedRef<TcpStream>,
        peer_ref: SharedRef<Peer>,
        piece_handler: Arc<PieceHandler>,
    ) {
        loop {
            let n: u32 = _recv_len(&stream_ref).await;
            if n == 0 {
                continue;
            }

            let mut buf: Vec<u8> = vec![0; n as usize];
            let _ = _recv(&stream_ref, &mut buf, n).await.unwrap();

            let message_id = buf[0];
            if message_id == UNCHOKE {
                peer_ref.get_handle().await.unchoke_me();
            } else if message_id == HAVE {
                piece_handler
                    .queue_piece(BigEndian::read_u32(&buf[1..]))
                    .await;
            } else if message_id == BITFIELD {
                let piece_handler = Arc::clone(&piece_handler);
                let bitfield = Bitfield::from((&buf[1..], piece_handler.info.num_pieces()));
                tokio::spawn(async move {
                    for i in 0..bitfield.len() {
                        if bitfield.get(i) == true {
                            piece_handler.queue_piece(i).await;
                            sleep(Duration::from_millis(2)).await;
                        }
                    }
                });
            } else if message_id == PIECE {
                *piece_handler.num_requests.get_handle().await -= 1;
                // let piece_index = BigEndian::read_u32(&buf[1..5]);
                // let piece_bitfield = piece_handler.downloaded_blocks.get_mut(piece_index);
            }

            sleep(Duration::from_millis(1)).await;
        }
    }

    pub async fn keep_alive(stream_ref: SharedRef<TcpStream>) {
        loop {
            _send(&stream_ref, 0, None).await;
            sleep(Duration::from_secs(60)).await;
        }
    }
}

pub struct PieceHandler {
    requests_queue: SharedRef<VecDeque<BlockRequest>>,
    info: Arc<Info>,
    downloaded_pieces: Bitfield,
    // downloaded_blocks: Vec<Bitfield>,
    // requested_pieces: Bitfield,
    num_requests: SharedRef<u32>,
    requests_cap: u32,
    block_size: u32,
}

impl PieceHandler {
    fn new(metadata: &Metadata) -> Self {
        let info = Arc::clone(&metadata.info);
        let num_pieces = info.num_pieces();
        let block_size = 16 * 1024;
        // let num_blocks = info.num_blocks(0, block_size);
        println!("number of pieces: {}", num_pieces);
        Self {
            requests_queue: SharedRef::new(Some(VecDeque::new())),
            info,
            downloaded_pieces: Bitfield::new(num_pieces),
            // downloaded_blocks: vec![Bitfield::new(num_blocks); num_pieces as usize],
            // requested_pieces: Bitfield::new(num_pieces),
            num_requests: SharedRef::new(Some(0)),
            requests_cap: 10,
            block_size,
        }
    }

    async fn _full(self: &Self) -> bool {
        return *self.num_requests.get_handle().await > self.requests_cap
    }

    pub async fn queue_piece(self: &Self, piece_index: u32) {
        // println!("{}", piece_index);
        let mut cur_offset: u32 = 0;
        let piece_len = self.info.piece_len(piece_index);
        // println!("piece length: {}", piece_len);
        while cur_offset != piece_len {
            let block_size = cmp::min(self.block_size, piece_len - cur_offset);
            let request = BlockRequest(piece_index, cur_offset, block_size);
            self.requests_queue.get_handle().await.push_back(request);
            cur_offset += block_size;
        }
    }

    pub async fn request_loop(self: &Self, stream_ref: SharedRef<TcpStream>, peer_ref: SharedRef<Peer>) {
        while self.downloaded_pieces.rem() > 0 {
            if self._full().await || peer_ref.get_handle().await.state.client_choked {
                // println!("{}", cnt);
                // println!("{}", peer_ref.get_handle().await.state.client_choked);
                sleep(Duration::from_millis(2)).await;
            } else {
                let mut q = self.requests_queue.get_handle().await;
                // print!("got here");
                if q.len() > 0 {
                    // print!("and there");
                    q.front().unwrap().send(&stream_ref).await;
                    q.pop_front();
                    *self.num_requests.get_handle().await += 1;
                }
                // println!("");
            }
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

        let mut ret = [0u8; 20];
        ret.copy_from_slice(&buff[buff_len - 20..buff_len]);

        ret
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
        piece_handler.request_loop(self.stream.clone(), self.peer.clone()).await;

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
