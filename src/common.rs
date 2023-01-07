use byteorder::{BigEndian, ByteOrder};
use std::{
    net::{Ipv4Addr, SocketAddr},
    time::SystemTime,
};

pub fn get_timestamp() -> String {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    let timestamp: String = now.unwrap().as_secs().to_string();

    timestamp
}

pub fn addr_from_buf(buf: &[u8; 6]) -> SocketAddr {
    SocketAddr::from((
        Ipv4Addr::from(BigEndian::read_u32(buf)),
        BigEndian::read_u16(&buf[4..]),
    ))
}
