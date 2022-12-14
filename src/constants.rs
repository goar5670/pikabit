pub mod torrents {
    pub const UBUNTU22: &'static str = "torrents/ubuntu-22.10-desktop-amd64.iso.torrent";
    pub const UBUNTU22_INFO_HASH: &'static str = "99c82bb73505a3c0b453f9fa0e881d6e5a32a0c1";
}

pub mod message_ids {
    pub const CHOKE: u8 = 0;
    pub const UNCHOKE: u8 = 1;
    pub const INTERESTED: u8 = 2;
    pub const NOT_INTERESTED: u8 = 3;
    pub const HAVE: u8 = 4;
    pub const BITFIELD: u8 = 5;
    pub const REQUEST: u8 = 6;
    pub const PIECE: u8 = 7;
    pub const CANCEL: u8 = 8;
}
