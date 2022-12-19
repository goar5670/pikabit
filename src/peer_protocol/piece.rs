use log::{debug, error};
use sha1::{Digest, Sha1};
use std::{
    cmp,
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::{
    sync::mpsc::{Receiver, Sender},
    time::{sleep, Duration},
};

use super::bitfield::Bitfield;
use super::msg::*;
use crate::concurrency::SharedRef;
use crate::file;
use crate::metadata::Info;
use crate::peer;

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

    async fn save(self: &Self, offset: u64, fh_tx: &Sender<file::Cmd>) -> Result<(), &'static str> {
        if self.mask.rem() > 0 {
            Err("Piece is not fully downloaded yet")
        } else if !self.verify_hash() {
            Err("Piece hash verification failed")
        } else {
            match fh_tx
                .send(file::Cmd::WritePiece(offset, self.bytes.clone()))
                .await
            {
                Ok(_) => {
                    debug!(
                        "Piece {} wrote to disk, offset: {}, size: {}",
                        self.index,
                        offset,
                        self.bytes.len()
                    );

                    return Ok(());
                }
                Err(e) => {
                    error!("{:?}", e);
                    return Err("Could not write piece to disk");
                }
            }
        }
    }

    async fn request(self: &Self, msg_tx: &Sender<Message>) {
        let mut cur_offset: u32 = 0;
        let piece_len = self.bytes.len() as u32;

        while cur_offset != piece_len {
            let block_size = cmp::min(self.block_size, piece_len - cur_offset);
            let _ = msg_tx
                .send(Message::Request(Request::new(
                    self.index, cur_offset, block_size,
                )))
                .await;
            cur_offset += block_size;
        }
    }
}

pub struct PieceHandler {
    requests_queue: VecDeque<u32>,
    info: Arc<Info>,
    downloaded_pieces: Bitfield,
    requested_pieces: HashMap<u32, Piece>,
    requests_cap: u32,
    block_size: u32,
    peer_state_ref: SharedRef<peer::State>,
    fh_tx: Sender<file::Cmd>,
    msg_tx: Sender<Message>,
    own_rx: Receiver<Cmd>,
}

impl PieceHandler {
    pub fn new(
        info: Arc<Info>,
        peer_state_ref: &SharedRef<peer::State>,
        fh_tx: Sender<file::Cmd>,
        msg_tx: Sender<Message>,
        own_rx: Receiver<Cmd>,
    ) -> Self {
        Self {
            requests_queue: VecDeque::new(),
            fh_tx,
            downloaded_pieces: Bitfield::new(info.num_pieces()),
            requested_pieces: HashMap::new(),
            info,
            requests_cap: 1,
            block_size: 16 * 1024,
            peer_state_ref: peer_state_ref.clone(),
            msg_tx,
            own_rx,
        }
    }

    fn _full(self: &Self) -> bool {
        return self.requested_pieces.len() as u32 > self.requests_cap;
    }

    fn _piece_offset(self: &Self, piece_index: u32) -> u64 {
        self.info.num_blocks(piece_index, self.block_size) as u64
            * self.block_size as u64
            * piece_index as u64
    }

    pub fn enqueue_piece(&mut self, piece_index: u32) {
        self.requests_queue.push_back(piece_index);
    }

    pub async fn enqueue_bitfield(&mut self, bitfield: &[u8]) {
        let bitfield = Bitfield::from((bitfield, self.info.num_pieces()));
        for (i, bit) in bitfield.enumerate() {
            if bit {
                self.enqueue_piece(i as u32);
                sleep(Duration::from_millis(2)).await;
            }
        }
    }

    pub async fn request_piece(self: &mut Self, piece_index: u32) {
        let piece = Piece::new(
            piece_index,
            self.info.piece_len(piece_index),
            self.info.num_blocks(piece_index, self.block_size),
            self.block_size,
            &self.info.piece_hash(piece_index),
        );

        piece.request(&self.msg_tx).await;

        self.requested_pieces.insert(piece_index, piece);
    }

    pub async fn recv_block(self: &mut Self, piece_index: u32, block_offset: u32, block: &[u8]) {
        let offset = self._piece_offset(piece_index);
        let piece = self.requested_pieces.get_mut(&piece_index).unwrap();
        let rem = piece.recv_block(block_offset, block);
        if rem == 0 {
            piece.save(offset, &self.fh_tx).await.unwrap();
            self.requested_pieces.remove(&piece_index);
            self.downloaded_pieces.set(piece_index);
        }
    }

    pub async fn run(mut self) {
        while self.downloaded_pieces.rem() > 0 {
            if let Ok(msg) = self.own_rx.try_recv() {
                match msg {
                    Cmd::EnqPiece(index) => self.enqueue_piece(index),
                    Cmd::EnqBitfield(buf) => self.enqueue_bitfield(&buf).await,
                    Cmd::RecvBlock(piece_index, block_offset, buf) => {
                        self.recv_block(piece_index, block_offset, &buf).await
                    }
                }
            }

            if !self._full() && !self.peer_state_ref.get_handle().await.0 {
                if let Some(piece_index) = self.requests_queue.pop_front() {
                    self.request_piece(piece_index).await;
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum Cmd {
    EnqBitfield(Vec<u8>),
    EnqPiece(u32),
    RecvBlock(u32, u32, Vec<u8>),
}
