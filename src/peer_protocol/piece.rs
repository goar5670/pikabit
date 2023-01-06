use std::collections::{HashMap, HashSet};

use crate::bitfield::{Bitfield, BitfieldOwned, BitfieldRef};

pub struct Metadata {
    total_len: u64,
    piece_len: u32,
    block_size: u32,
    hashes: Vec<u8>,
}

impl Metadata {
    fn new(total_len: u64, piece_len: u32, block_size: Option<u32>, hashes: Vec<u8>) -> Self {
        Self {
            total_len,
            piece_len,
            block_size: block_size.unwrap_or(1024 * 16),
            hashes,
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

    pub fn piece_offset(&self, piece_index: u32) -> u64 {
        self.piece_len as u64 * piece_index as u64
    }

    pub fn piece_hash(&self, piece_index: u32) -> &[u8] {
        let offset = (piece_index * 20) as usize;
        &self.hashes[offset..offset + 20]
    }
}

#[derive(Debug)]
pub struct BlockInfo {
    offset_in_buffer: u32,
    pub offset_in_piece: u32,
    size: usize,
}

impl BlockInfo {
    fn new(offset_in_buffer: u32, offset_in_piece: u32, block_size: usize) -> Self {
        Self {
            offset_in_buffer,
            offset_in_piece,
            size: block_size,
        }
    }

    pub fn get_range(&self) -> (usize, usize) {
        (
            self.offset_in_buffer as usize,
            self.offset_in_buffer as usize + self.size,
        )
    }
}

pub struct PieceInfo {
    have: BitfieldOwned,
    // offset_in_buffer, offset_in_piece
    blocks: Vec<Option<BlockInfo>>,
}

impl PieceInfo {
    fn new(num_blocks: u32) -> Self {
        Self {
            have: BitfieldOwned::new(num_blocks),
            blocks: (0..num_blocks).map(|_| None).collect(),
        }
    }

    fn add_block(
        &mut self,
        block_index: u32,
        offset_in_buffer: u32,
        offset_in_piece: u32,
        block_size: usize,
    ) -> u32 {
        let block = self.blocks.get_mut(block_index as usize).unwrap();
        *block = Some(BlockInfo::new(
            offset_in_buffer,
            offset_in_piece,
            block_size,
        ));
        self.have.set(block_index, true)
    }
}

pub struct PieceTracker {
    pub metadata: Metadata,
    pub needed: HashSet<u32>,
    have: BitfieldOwned,
    requested: BitfieldOwned,
    buffered: HashMap<u32, PieceInfo>,
}

impl PieceTracker {
    pub fn new(total_len: u64, piece_len: u32, hashes: Vec<u8>) -> Self {
        let metadata = Metadata::new(total_len, piece_len, None, hashes);
        Self {
            have: BitfieldOwned::new(metadata.num_pieces()),
            requested: BitfieldOwned::new(metadata.num_pieces()),
            metadata,
            needed: HashSet::new(),
            buffered: HashMap::new(),
        }
    }

    pub fn update_single(&mut self, piece_index: u32) {
        if !self.has_piece(piece_index).unwrap_or(true)
            && !self.is_reserved(piece_index).unwrap_or(true)
        {
            self.needed.insert(piece_index);
        }
    }

    pub fn update_multiple(&mut self, buf: &[u8]) {
        let bitfield = BitfieldRef::new(buf, self.metadata.num_pieces());

        for i in 0..bitfield.len() {
            if bitfield.get(i).unwrap() {
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
        block_size: usize,
    ) -> u32 {
        match self.buffered.get_mut(&piece_index) {
            None => {
                let mut piece_info = PieceInfo::new(self.metadata.num_blocks(piece_index));
                let rem = piece_info.add_block(
                    self.metadata.block_index(offset_in_piece),
                    offset_in_buffer,
                    offset_in_piece,
                    block_size,
                );
                self.buffered.insert(piece_index, piece_info);
                rem
            }
            Some(piece_info) => piece_info.add_block(
                self.metadata.block_index(offset_in_piece),
                offset_in_buffer,
                offset_in_piece,
                block_size,
            ),
        }
    }

    pub fn reserve_piece(&mut self, piece_index: u32) {
        self.requested.set(piece_index, false);
    }

    pub fn unreserve_piece(&mut self, piece_index: u32) {
        self.requested.set(piece_index, true);
    }

    pub fn is_reserved(&self, piece_index: u32) -> Option<bool> {
        self.requested.get(piece_index)
    }

    pub fn ordered_piece(&self, piece_index: u32, pbuf: &PieceBuffer) -> Vec<u8> {
        let piece = pbuf.get(piece_index);
        let mut ordered: Vec<u8> = vec![];

        for (i, binfo) in self.piece_ordering(piece_index).iter().enumerate() {
            assert!(
                !binfo.is_none(),
                "binfo is none, piece_index {}, block_index {}",
                piece_index,
                i
            );
            let binfo = binfo.as_ref().unwrap();
            let (s, e) = binfo.get_range();
            let offset_in_piece = binfo.offset_in_piece;
            assert_eq!(
                offset_in_piece,
                ordered.len() as u32,
                "Error while reordering piece, offset: {}, current piece length: {}",
                offset_in_piece,
                ordered.len()
            );
            ordered.extend_from_slice(&piece[s..e]);
        }

        ordered
    }

    pub fn verify_piece_hash(&self, piece_index: u32, hash: &[u8; 20]) -> bool {
        self.metadata.piece_hash(piece_index) == hash
    }

    pub fn on_piece_saved(&mut self, piece_index: u32) {
        self.have.set(piece_index, true);
        self.buffered.remove(&piece_index);
    }

    pub fn piece_ordering(&self, piece_index: u32) -> &Vec<Option<BlockInfo>> {
        &self.buffered.get(&piece_index).unwrap().blocks
    }

    pub fn is_empty(&self) -> bool {
        self.needed.is_empty()
    }

    pub fn has_piece(&self, piece_index: u32) -> Option<bool> {
        self.have.get(piece_index)
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

    pub fn get(&self, piece_index: u32) -> &Vec<u8> {
        self.inner.get(&piece_index).unwrap()
    }
}
