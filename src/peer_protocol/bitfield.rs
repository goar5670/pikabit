#[derive(Clone)]
pub struct Bitfield {
    inner: Vec<u8>,
    cnt_marked: u32,
    cur: u32,
    length: u32,
}

impl Bitfield {
    pub fn new(size: u32) -> Self {
        Self {
            length: size,
            cur: 0,
            cnt_marked: 0,
            inner: vec![0; ((size + 7) / 8) as usize],
        }
    }

    fn _get_bit_index(index: u32) -> (usize, u8) {
        ((index / 8) as usize, (index % 8) as u8)
    }

    fn _check_offset(byte: u8, offset: u8) -> bool {
        (byte & (1 << offset)) != 0
    }

    // pub fn len(self: &Self) -> u32 {
    //     self.length
    // }

    pub fn rem(self: &Self) -> u32 {
        self.length - self.cnt_marked
    }

    pub fn get(self: &Self, index: u32) -> Option<bool> {
        if index >= self.length {
            return None;
        }

        let (byte_index, offset) = Self::_get_bit_index(index);
        Some(Self::_check_offset(
            *self.inner.get(byte_index).unwrap(),
            offset,
        ))
    }

    pub fn set(self: &mut Self, index: u32) -> u32 {
        if index >= self.length {
            panic!("Requested index is out of bounds");
        }

        let (byte_index, offset) = Self::_get_bit_index(index);
        let byte = self.inner.get_mut(byte_index).unwrap();
        if *byte & (1 << offset) == 1 {
            return self.rem();
        }

        *byte &= 1 << offset;
        self.cnt_marked += 1;

        self.rem()
    }
}

impl From<(&[u8], u32)> for Bitfield {
    fn from((buf, size): (&[u8], u32)) -> Self {
        let mut bitfield = Bitfield::new(size);
        bitfield.inner = buf.to_vec();
        bitfield.cnt_marked = buf.iter().map(|i| i.count_ones()).sum();

        bitfield
    }
}

impl Iterator for Bitfield {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        let ret = self.get(self.cur);
        self.cur += 1;

        ret
    }
}
