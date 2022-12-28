pub mod msg_ids {
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
