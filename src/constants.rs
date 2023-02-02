pub mod msg {
    pub mod id {
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

    pub mod len {
        pub const CHOKE: u32 = 1;
        pub const UNCHOKE: u32 = 1;
        pub const INTERESTED: u32 = 1;
        pub const NOT_INTERESTED: u32 = 1;
        pub const HAVE: u32 = 5;
        pub const BITFIELD_PREAMBLE: u32 = 1;
        pub const REQUEST: u32 = 13;
        pub const PIECE_PREAMBLE: u32 = 9;
        pub const CANCEL: u32 = 13;
    }
}

pub mod timeouts {
    pub const PEER_CONNECTION: u64 = 60;
    pub const KEEP_ALIVE: u64 = 2 * 60;
}

pub const REQUESTS_CAPACITY: u32 = 5;

pub const CLIENT_PREFIX: &str = "-pB";
pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
