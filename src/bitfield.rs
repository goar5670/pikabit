pub trait Bitfield {
    fn len(&self) -> u32;
    fn cnt_marked(&self) -> u32;
    fn get(&self, index: u32) -> Option<bool>;
    fn bytes(&self) -> &[u8];

    fn bit_index(index: u32) -> (usize, u8) {
        ((index / 8) as usize, (index % 8) as u8)
    }

    fn check_offset(byte: u8, offset: u8) -> bool {
        (byte & (1 << offset)) != 0
    }

    fn rem(&self) -> u32 {
        self.len() - self.cnt_marked()
    }
}

pub struct BitfieldRef<'a> {
    bytes: &'a [u8],
    cnt_marked: u32,
    length: u32,
}

impl<'a> BitfieldRef<'a> {
    pub fn new(buf: &'a [u8], length: u32) -> Self {
        Self {
            bytes: buf,
            cnt_marked: 0,
            length,
        }
    }
}

impl Bitfield for BitfieldRef<'_> {
    fn bytes(&self) -> &[u8] {
        self.bytes
    }

    fn len(&self) -> u32 {
        self.length
    }

    fn cnt_marked(&self) -> u32 {
        self.cnt_marked
    }

    fn get(self: &Self, index: u32) -> Option<bool> {
        if index >= self.len() {
            return None;
        }

        let (byte_index, offset) = Self::bit_index(index);
        Some(Self::check_offset(
            *self.bytes.get(byte_index).unwrap(),
            offset,
        ))
    }
}

pub struct BitfieldOwned {
    bytes: Vec<u8>,
    cnt_marked: u32,
    length: u32,
}

impl Bitfield for BitfieldOwned {
    fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn len(&self) -> u32 {
        self.length
    }

    fn cnt_marked(&self) -> u32 {
        self.cnt_marked
    }

    fn get(self: &Self, index: u32) -> Option<bool> {
        if index >= self.len() {
            return None;
        }

        let (byte_index, offset) = Self::bit_index(index);
        Some(Self::check_offset(
            *self.bytes.get(byte_index).unwrap(),
            offset,
        ))
    }
}

impl BitfieldOwned {
    pub fn new(length: u32) -> Self {
        Self {
            bytes: vec![0; Self::bit_index(length).0 + 1],
            length,
            cnt_marked: 0,
        }
    }

    pub fn set(self: &mut Self, index: u32) -> u32 {
        if index >= self.len() {
            panic!("Requested index {} is out of bounds {}", index, self.len());
        }

        let (byte_index, offset) = Self::bit_index(index);
        let byte = self.bytes.get_mut(byte_index).unwrap();
        if *byte & (1 << offset) > 0 {
            return self.rem();
        }

        *byte &= 1 << offset;
        self.cnt_marked += 1;

        self.rem()
    }
}
