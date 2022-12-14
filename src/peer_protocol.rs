// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use rand::{distributions::Alphanumeric, Rng};
use std::{
    cmp,
    thread,
    time::Duration,
    collections::VecDeque,
    io::{self, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    sync::{Arc, Mutex, MutexGuard},
};

use crate::metadata::{Metadata, Info};

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
    client_choking: bool,
    client_interested: bool,
    peer_choking: bool,
    peer_interested: bool,
}

impl ConnectionState {
    pub fn new() -> Self {
        Self {
            client_choking: true,
            client_interested: false,
            peer_choking: true,
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
    pub fn connect(self: &Self) -> io::Result<TcpStream> {
        TcpStream::connect(&self.address)
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

type Handler<T> = Option<Arc<Mutex<T>>>;

fn get_handle<T>(handler: &Handler<T>) -> MutexGuard<T> {
    handler.as_ref().unwrap().lock().unwrap()
}

#[derive(Debug)]
struct BlockRequest(u32, u32, u32);

impl BlockRequest {
    pub fn send(self: &Self, handler: &Handler<TcpStream>) {
        let mut buf: Vec<u8> = vec![];
        buf.write_u32::<BigEndian>(13).unwrap();
        buf.push(6);
        buf.write_u32::<BigEndian>(self.0).unwrap();
        buf.write_u32::<BigEndian>(self.1).unwrap();
        buf.write_u32::<BigEndian>(self.2).unwrap();

        let _ = get_handle(handler).write(&mut buf);
        println!("sent request piece message, {:?}", self);
    }
}

pub struct PieceHandler<'a> {
    requests: VecDeque<BlockRequest>,
    info: &'a Info,
    downloaded_pieces: Vec<bool>,
    queue_cap: u32,
    block_size: u32,
}

impl<'a> PieceHandler<'a> {
    fn new(info: &'a Info) -> Self {
        Self {
            requests: VecDeque::new(),
            info,
            downloaded_pieces: vec![false; info.get_num_pieces() as usize],
            queue_cap: 100,
            block_size: 16 * 1024,
        }
    }

    pub fn request_piece(self: &mut Self, handler: &Handler<TcpStream>, piece_index: u32) {
        let mut cur_offset: u32 = 0;
        let piece_len = self.info.get_piece_len(piece_index);
        println!("piece length: {}", piece_len);
        while cur_offset != piece_len {
            let block_size = cmp::min(self.block_size, piece_len - cur_offset);
            let request = BlockRequest(piece_index, cur_offset, block_size);
            request.send(handler);
            cur_offset += block_size;
            thread::sleep(Duration::from_millis(10));
        }
    }
}

mod message_handler {
    use super::{get_handle, Handler, PieceHandler};
    use crate::constants::message_ids::*;
    use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
    use std::{
        io::{Read, Write},
        net::TcpStream,
        thread,
        time::Duration,
    };

    async fn _send(
        handler: &Handler<TcpStream>,
        length: u32,
        id: Option<u8>,
        // payload: Option<&mut Vec<u8>>,
    ) {
        let mut buf: Vec<u8> = vec![];
        buf.write_u32::<BigEndian>(length).unwrap();
        if length != 0 {
            let message_id = id.unwrap();
            buf.push(message_id);
        }

        let _ = get_handle(handler).write(&mut buf);
        println!("sent message, id {:?}, buffer: {:?}", id, buf);
    }

    fn _recv_len(handler: &Handler<TcpStream>) -> u32 {
        let mut buf = [0u8; 4];
        let _ = get_handle(handler).read_exact(&mut buf).unwrap();
        BigEndian::read_u32(&buf)
    }

    fn _recv(handler: &Handler<TcpStream>, buf: &mut [u8], n: u32) -> Result<(), std::io::Error> {
        let mut stream = get_handle(handler);
        let _ = match stream.read_exact(buf) {
            Err(e) => {
                eprintln!("failed to read from socket; err = {:?}", e);
                return Err(e);
            }
            _ => 0,
        };

        if n < 9 {
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
        handler: Handler<TcpStream>,
        length: u32,
        id: Option<u8>,
        // payload: Option<&mut Vec<u8>>,
    ) {
        _send(&handler, length, id).await;
    }

    pub async fn recv_loop<'a>(handler: Handler<TcpStream>, piece_handler: &mut PieceHandler<'a>) {
        loop {
            let n: u32 = _recv_len(&handler);
            if n == 0 {
                continue;
            }
            
            let mut buf: Vec<u8> = vec![0; n as usize];
            let _ = _recv(&handler, &mut buf, n).unwrap();

            let message_id = buf[0];
            if message_id == HAVE {
                piece_handler.request_piece(&handler, BigEndian::read_u32(&buf[1..]));
            }

            thread::sleep(Duration::from_millis(1));
        }
    }

    pub async fn keep_alive(handler: Handler<TcpStream>) {
        loop {
            _send(&handler, 0, None).await;
            thread::sleep(Duration::from_secs(60));
        }
    }
}

pub struct PeerHandler {
    peer: Peer,
    stream: Handler<TcpStream>,
}

impl PeerHandler {
    pub fn new(peer: Peer) -> Self {
        Self { peer, stream: None }
    }

    fn get_stream(self: &Self) -> Handler<TcpStream> {
        self.stream
            .as_ref()
            .map(|stream_ref| Arc::clone(stream_ref))
    }

    fn connect(self: &mut Self) {
        self.stream = Some(Arc::new(Mutex::new(self.peer.connect().unwrap())))
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

    fn handshake(self: &Self, payload: Vec<u8>) -> [u8; 20] {
        let handler = self.get_stream().unwrap();
        let mut stream = handler.lock().unwrap();
        let _ = stream.write(&payload);
        let mut buff = [0u8; 128];
        let buff_len = stream.read(&mut buff).unwrap();

        debug_assert_eq!(buff_len, payload.len());
        debug_assert_eq!(buff[..buff_len - 20], payload[..buff_len - 20]);

        let mut ret = [0u8; 20];
        ret.copy_from_slice(&buff[buff_len - 20..buff_len]);

        ret
    }

    pub async fn run<'a>(self: &mut Self, metadata: &'a Metadata, client_id: &PeerId) {
        self.connect();
        let info_hash = &metadata.get_info_hash();
        self.peer.id = Some(PeerId::from(
            self.handshake(self.get_handshake_payload(info_hash, client_id))
                .as_ref(),
        ));
        println!(
            "peer id: {}",
            String::from_utf8(self.peer.id.as_ref().unwrap().to_vec()).unwrap()
        );

        let mut piece_handler: PieceHandler<'a> =
            PieceHandler::new(&metadata.info);

        message_handler::send(self.get_stream(), 1, Some(2)).await;
        let handle = tokio::spawn(message_handler::keep_alive(self.get_stream()));
        message_handler::recv_loop(self.get_stream(), &mut piece_handler).await;

        handle.await.unwrap();
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
