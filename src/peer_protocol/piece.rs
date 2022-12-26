use priority_queue::PriorityQueue;
use std::{cmp::Reverse, collections::HashMap};

use crate::bitfield::{Bitfield, BitfieldOwned, BitfieldRef};

pub struct Metadata {
    total_len: u64,
    piece_len: u32,
    block_size: u32,
    // hashes: Vec<u8>,
}

impl Metadata {
    fn new(total_len: u64, piece_len: u32, block_size: Option<u32>) -> Self {
        Self {
            total_len,
            piece_len,
            block_size: block_size.unwrap_or(1024 * 16),
        }
    }

    fn last_piece_len(&self) -> u32 {
        let ret = self.total_len % self.piece_len as u64;
        if ret == 0 {
            return self.piece_len;
        }
        ret as u32
    }

    pub fn num_pieces(&self) -> u32 {
        ((self.total_len + self.piece_len as u64 - 1) / self.piece_len as u64) as u32
    }

    pub fn piece_len(&self, piece_index: u32) -> u32 {
        if piece_index == self.num_pieces() - 1 {
            return self.last_piece_len();
        }
        self.piece_len
    }

    fn num_blocks(&self, piece_index: u32) -> u32 {
        (self.piece_len(piece_index) + self.block_size - 1) / self.block_size
    }

    fn block_index(&self, block_offset: u32) -> u32 {
        debug_assert_eq!(
            block_offset % self.block_size,
            0,
            "{} {}",
            block_offset,
            self.block_size
        );
        block_offset / self.block_size
    }

    pub fn block_size(&self) -> u32 {
        self.block_size
    }
}

struct PieceInfo {
    have: BitfieldOwned,
    // offset_in_buffer, offset_in_piece
    offsets: Vec<(u32, u32)>,
}

impl PieceInfo {
    fn new(piece_len: u32) -> Self {
        Self {
            have: BitfieldOwned::new(piece_len),
            offsets: vec![(0, 0); piece_len as usize],
        }
    }

    fn add_block(&mut self, block_index: u32, offset_in_buffer: u32, offset_in_piece: u32) -> u32 {
        *self.offsets.get_mut(block_index as usize).unwrap() = (offset_in_buffer, offset_in_piece);
        self.have.set(block_index)
    }
}

pub struct PieceTracker {
    pub metadata: Metadata,
    pub pieces_pq: PriorityQueue<u32, Reverse<u32>>,
    have: BitfieldOwned,
    buffered: HashMap<u32, PieceInfo>,
}

impl PieceTracker {
    pub fn new(total_len: u64, piece_len: u32) -> Self {
        let metadata = Metadata::new(total_len, piece_len, None);
        Self {
            have: BitfieldOwned::new(metadata.num_pieces()),
            metadata,
            pieces_pq: PriorityQueue::new(),
            buffered: HashMap::new(),
        }
    }

    pub fn update_single(&mut self, piece_index: u32) {
        if self.have.get(piece_index).unwrap() {
            return;
        }

        if !self
            .pieces_pq
            .change_priority_by(&piece_index, |p| p.0 += 1)
        {
            self.pieces_pq.push(piece_index, Reverse(1));
        }
    }

    pub fn update_multiple(&mut self, buf: &[u8]) {
        let bytebuf = BitfieldRef::new(buf, self.metadata.num_pieces());

        for i in 0..self.metadata.num_pieces() {
            if !bytebuf.get(i).unwrap() {
                self.update_single(i);
            }
        }
    }

    pub fn rem(&self) -> u32 {
        self.have.rem()
    }

    pub fn on_block_buffered(
        &mut self,
        piece_index: u32,
        offset_in_buffer: u32,
        offset_in_piece: u32,
    ) -> u32 {
        match self.buffered.get_mut(&piece_index) {
            None => {
                let mut piece_info = PieceInfo::new(self.metadata.num_blocks(piece_index));
                let rem = piece_info.add_block(
                    self.metadata.block_index(offset_in_piece),
                    offset_in_buffer,
                    offset_in_piece,
                );
                self.buffered.insert(piece_index, piece_info);
                rem
            }
            Some(piece_info) => piece_info.add_block(
                self.metadata.block_index(offset_in_piece),
                offset_in_buffer,
                offset_in_piece,
            ),
        }
    }

    pub fn on_piece_completed(&mut self, piece_index: u32) {
        self.have.set(piece_index);
        self.buffered.remove(&piece_index);
    }

    pub fn next_piece(&mut self) -> Option<u32> {
        self.pieces_pq.pop().map(|item| item.0)
    }

    pub fn is_empty(&self) -> bool {
        self.pieces_pq.is_empty()
    }
}

pub struct PieceBuffer {
    inner: HashMap<u32, Vec<u8>>,
}

impl PieceBuffer {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn append(&mut self, piece_index: u32, buf: &[u8]) -> u32 {
        let offset = match self.inner.get_mut(&piece_index) {
            None => {
                self.inner.insert(piece_index, buf.to_vec());
                0
            }
            Some(v) => {
                let offset = v.len();
                v.extend_from_slice(buf);

                offset
            }
        };

        offset as u32
    }
}
