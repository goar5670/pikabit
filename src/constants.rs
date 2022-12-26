pub mod torrents {
    pub const UBUNTU22: &str = "torrents/ubuntu-22.10-desktop-amd64.iso.torrent";
    pub const FREE_BSD: &str = "torrents/FreeBSD 12.4 RELEASE Sparc64 Disc1 ISO.torrent";
    pub const UBUNTU22_INFO_HASH: &str = "99c82bb73505a3c0b453f9fa0e881d6e5a32a0c1";
    pub const FREE_BSD_INFO_HASH: &str = "f47932ba13094be79904a714a406e7c809636c53";
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

pub mod timeouts {
    use tokio::time::Duration;

    pub const PEER_CONNECTION: Duration = Duration::from_secs(60);
}

pub const CLIENT_PREFIX: &str = "-pB";
pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
