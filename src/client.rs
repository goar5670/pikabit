use std::string::ToString;
use rand::{distributions::Alphanumeric, Rng};
use std::fs;
use serde_bencode;
use std::net::Ipv4Addr;
use byteorder::{BigEndian, ByteOrder};

use crate::http::{TrackerRequest, TrackerResponse, Event};
use crate::metadata::{Metadata};

struct PeerId {
  inner: [u8; 20],
}

impl PeerId {
  pub fn new() -> Self {
    let now = std::time::SystemTime::now().duration_since(
        std::time::SystemTime::UNIX_EPOCH
    );

    let timestamp = &now.unwrap().as_secs().to_string();

    const CLIENT_ID: &'static str = "-pikaB";

    let mut peer_id: String = "".to_owned();

    let rand_filler: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20 - CLIENT_ID.len() - timestamp.len())
        .map(char::from)
        .collect();
    
    peer_id.push_str(CLIENT_ID);
    peer_id.push_str(timestamp);
    peer_id.push_str(rand_filler.as_str());

    assert_eq!(peer_id.len(), 20);

    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(peer_id.as_bytes());

    Self {
      inner: bytes,
    }
  }
}

impl ToString for PeerId {
  fn to_string(self: &Self) -> String {
    String::from_utf8(self.inner.to_vec()).unwrap()
  }
}

#[derive(Debug, PartialEq)]
struct Peer {
  ip: Ipv4Addr,
  port: u16,
}

impl From<&[u8; 6]> for Peer {
  fn from(buf: &[u8; 6]) -> Self {
    Self {
      ip: Ipv4Addr::from(BigEndian::read_u32(buf)),
      port: BigEndian::read_u16(&buf[4..])
    }
  }
}

pub struct Client {
  peer_id: PeerId,
  torrent: Metadata,
  info_hash: [u8; 20],
  port: u16,
  peers: Vec<Peer>,
}

impl Client {
  pub fn new(filename: &str, port: Option<u16>) -> Self {
    let file: Vec<u8> = fs::read(filename).unwrap();
    let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();
    
    let info_hash = torrent.get_info_hash();
    let default_port = 6881;

    Self {
      peer_id: PeerId::new(),
      info_hash,
      torrent,
      port: match port {
        Some(p) => p,
        None => default_port,
      },
      peers: Vec::new(),
    }
  }

  fn set_peers(self: &mut Self, peers: &Vec<[u8; 6]>) {
    self.peers = Vec::new();
    for peer in peers {
      self.peers.push(Peer::from(peer));
    }    
  }

  fn tracker_start_request(self: &Self) -> TrackerRequest {
    TrackerRequest::new(
      self.torrent.get_tracker_url(),
      self.info_hash,
      self.peer_id.to_string(),
      self.port,
      Some(Event::Started),
    )
  }

  // todo: implement stop, resume functionality

  pub fn run(self: &mut Self) {
    // todo: implement stalling (repeat request until getting peers)

    let started_request = self.tracker_start_request();
    let started_response: TrackerResponse = serde_bencode::from_bytes(&started_request.get()).unwrap();

    let peers = started_response.get_peers();
    println!("{:#?} {:?}", started_response, peers);

    self.set_peers(&peers);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn peer_from_slice() {
    let slice: [u8; 6] = [10, 123, 0, 47, 26, 225];
    let peer = Peer::from(&slice);

    assert_eq!(peer, Peer { ip: Ipv4Addr::new(10, 123, 0, 47), port: 6881 });
  }
}