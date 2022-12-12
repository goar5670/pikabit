// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use std::net::{Ipv4Addr, SocketAddr};
use byteorder::{BigEndian, ByteOrder};

#[derive(Debug, PartialEq)]
struct ConnectionState {
  client_choking: bool,
  client_interested: bool,
  peer_choking: bool,
  peer_interested: bool,
}

impl ConnectionState {
  pub fn new() -> Self {
    Self {
      client_choking: true,
      client_interested: false,
      peer_choking: true,
      peer_interested: false,
    }
  }
}

#[derive(Debug, PartialEq)]
pub struct Peer {
  address: SocketAddr,
  state: ConnectionState,
}

impl Peer {
  fn handshake(self: &Self, info_hash: &[u8; 20], client_id: &[u8; 20]) {
    let mut payload: Vec<u8> = Vec::new();

    let protocol_name = "BitTorrent protocol".as_bytes();
    payload.push(19);
    payload.extend(protocol_name);
    payload.extend([0u8; 8]);
    payload.extend(info_hash);
    payload.extend(client_id);
  }
}

impl From<&[u8; 6]> for Peer {
  fn from(buf: &[u8; 6]) -> Self {
    Self {
      address: SocketAddr::from((
        Ipv4Addr::from(BigEndian::read_u32(buf)),
        BigEndian::read_u16(&buf[4..])
      )),
      state: ConnectionState::new(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn peer_from_slice() {
    let slice: [u8; 6] = [10, 123, 0, 47, 26, 225];
    let peer = Peer::from(&slice);

    assert_eq!(peer, Peer {
      address: SocketAddr::from((
        Ipv4Addr::new(10, 123, 0, 47),
        6881
      )),
      state: ConnectionState::new(),
    });
  }
}
