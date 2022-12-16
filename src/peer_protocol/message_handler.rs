use sha1::{Digest, Sha1};
use std::collections::HashMap;

use super::{bitfield::*, *};
use crate::constants::message_ids::*;

async fn _send(stream_ref: &SharedRef<TcpStream>, length: u32, id: Option<u8>) {
    let mut buf: Vec<u8> = vec![];
    buf.write_u32(length).await.unwrap();
    if length != 0 {
        let message_id = id.unwrap();
        buf.push(message_id);
    }

    let _ = stream_ref.get_handle().await.write(&mut buf).await;
    // println!("sent message, id {:?}, buffer: {:?}", id, buf);
}

async fn _recv_len(stream_ref: &SharedRef<TcpStream>) -> u32 {
    let mut buf = [0u8; 4];
    let _ = stream_ref
        .get_handle()
        .await
        .try_read(&mut buf)
        .unwrap_or_default();
    BigEndian::read_u32(&buf)
}

async fn _recv(
    stream_ref: &SharedRef<TcpStream>,
    buf: &mut [u8],
    n: u32,
) -> Result<(), std::io::Error> {
    let _ = stream_ref.get_handle().await.read_exact(buf).await;

    // if buf[0] != PIECE {
    //     println!("recieved message, len: {} {:?}", n, buf.to_vec());
    // } else {
    //     println!(
    //         "received piece, len: {}, message_id: {}, index: {}, offset: {}",
    //         n,
    //         buf[0],
    //         BigEndian::read_u32(&buf[1..]),
    //         BigEndian::read_u32(&buf[5..]),
    //     )
    // }

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
            println!("unchoked");
        } else if message_id == CHOKE {
            peer_ref.get_handle().await.choke_me();
            println!("choked");
        } else if message_id == HAVE {
            piece_handler
                .enqueue_piece(BigEndian::read_u32(&buf[1..]))
                .await;
        } else if message_id == BITFIELD {
            let piece_handler = Arc::clone(&piece_handler);
            let bitfield = Bitfield::from((&buf[1..], piece_handler.info.num_pieces()));
            tokio::spawn(async move {
                for i in 0..bitfield.len() {
                    if bitfield.get(i) == true {
                        piece_handler.enqueue_piece(i).await;
                        sleep(Duration::from_millis(2)).await;
                    }
                }
            });
        } else if message_id == PIECE {
            let piece_index = BigEndian::read_u32(&buf[1..5]);
            let block_offset = BigEndian::read_u32(&buf[5..9]);
            let block = &buf[9..];
            piece_handler
                .recv_block(piece_index, block_offset, block)
                .await;
        }
    }
}

pub async fn keep_alive(stream_ref: SharedRef<TcpStream>) {
    loop {
        _send(&stream_ref, 0, None).await;
        sleep(Duration::from_secs(60)).await;
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
        // println!("sent request piece message, {:?}", self);
    }
}

struct Piece {
    index: u32,
    mask: Bitfield,
    bytes: Vec<u8>,
    block_size: u32,
    hash: [u8; 20],
}

impl Piece {
    fn new(index: u32, len: u32, num_blocks: u32, block_size: u32, hash: &[u8; 20]) -> Self {
        Self {
            index,
            mask: Bitfield::new(num_blocks),
            bytes: vec![0; len as usize],
            block_size,
            hash: hash.clone(),
        }
    }

    fn _block_index(self: &Self, offset: u32) -> u32 {
        assert!(
            offset % self.block_size == 0,
            "{} {}",
            offset,
            self.block_size
        );
        offset / self.block_size
    }

    fn recv_block(self: &mut Self, offset: u32, block: &[u8]) -> u32 {
        let block_index = self._block_index(offset);
        let rem = self.mask.set(block_index);

        let s = offset as usize;
        let e = s + block.len();

        self.bytes[s..e].copy_from_slice(block);

        rem
    }

    fn verify_hash(self: &Self) -> bool {
        let piece_hash: [u8; 20] = Sha1::digest(&self.bytes).into();
        self.hash == piece_hash
    }

    fn save(self: &Self) -> Result<(), &'static str> {
        if self.mask.rem() > 0 {
            Err("Piece is not fully downloaded yet")
        } else if !self.verify_hash() {
            Err("Piece hash verification failed")
        } else {
            println!("Piece {} saved", self.index);
            Ok(())
        }
    }

    async fn request(self: &Self, stream_ref: &SharedRef<TcpStream>) {
        let mut cur_offset: u32 = 0;
        let piece_len = self.bytes.len() as u32;

        while cur_offset != piece_len {
            let block_size = cmp::min(self.block_size, piece_len - cur_offset);
            BlockRequest(self.index, cur_offset, block_size)
                .send(stream_ref)
                .await;
            cur_offset += block_size;
        }
    }
}

pub struct PieceHandler {
    requests_queue: SharedRef<VecDeque<u32>>,
    info: Arc<Info>,
    downloaded_pieces: SharedRef<Bitfield>,
    requested_pieces: SharedRef<HashMap<u32, Piece>>,
    requests_cap: u32,
    block_size: u32,
}

impl PieceHandler {
    pub fn new(metadata: &Metadata) -> Self {
        let info = Arc::clone(&metadata.info);
        let num_pieces = info.num_pieces();
        let block_size = 16 * 1024;
        println!("number of pieces: {}", num_pieces);
        Self {
            requests_queue: SharedRef::new(Some(VecDeque::new())),
            info,
            downloaded_pieces: SharedRef::new(Some(Bitfield::new(num_pieces))),
            requested_pieces: SharedRef::new(Some(HashMap::new())),
            requests_cap: 5,
            block_size,
        }
    }

    async fn _full(self: &Self) -> bool {
        return self.requested_pieces.get_handle().await.len() as u32 > self.requests_cap;
    }

    pub async fn enqueue_piece(self: &Self, piece_index: u32) {
        self.requests_queue
            .get_handle()
            .await
            .push_back(piece_index);
    }

    pub async fn request_piece(self: &Self, piece_index: u32, stream_ref: &SharedRef<TcpStream>) {
        let piece = Piece::new(
            piece_index,
            self.info.piece_len(piece_index),
            self.info.num_blocks(piece_index, self.block_size),
            self.block_size,
            &self.info.piece_hash(piece_index),
        );

        piece.request(stream_ref).await;

        self.requested_pieces
            .get_handle()
            .await
            .insert(piece_index, piece);
    }

    pub async fn recv_block(self: &Self, piece_index: u32, block_offset: u32, block: &[u8]) {
        let mut rp = self.requested_pieces.get_handle().await;
        let piece = rp.get_mut(&piece_index).unwrap();
        if piece.recv_block(block_offset, block) == 0 {
            piece.save().unwrap();
            rp.remove(&piece_index);
            self.downloaded_pieces.get_handle().await.set(piece_index);
            println!(
                "size of map {}",
                rp.len(),
            );
        }
    }

    pub async fn request_loop(
        self: &Self,
        stream_ref: SharedRef<TcpStream>,
        peer_ref: SharedRef<Peer>,
    ) {
        while self.downloaded_pieces.get_handle().await.rem() > 0 {
            if self._full().await || peer_ref.get_handle().await.state.client_choked {
                sleep(Duration::from_millis(2)).await;
            } else {
                let mut q = self.requests_queue.get_handle().await;
                if let Some(piece_index) = q.pop_front() {
                    self.request_piece(piece_index, &stream_ref).await;
                }
            }
        }
    }
}
