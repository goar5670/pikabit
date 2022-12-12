use rand::{distributions::Alphanumeric, Rng};
use std::{
  fs,
  cmp,
  thread,
  time,
  string::ToString,
  error::Error
};
use serde_bencode;

use crate::tracker_protocol::http::{
  Request,
  Response,
  Event
};
use crate::metadata::Metadata;
use crate::peer_protocol::Peer;

struct ClientId {
  inner: [u8; 20],
}

impl ClientId {
  pub fn new() -> Self {
    let timestamp = get_timestamp();

    const CLIENT_PREFIX: &'static str = "-pB";
    const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

    let cur_len = cmp::min(19, CLIENT_PREFIX.len() + CLIENT_VERSION.len() + timestamp.len());

    let rand_filler: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(19 - cur_len)
        .map(char::from)
        .collect();
    
    let client_id = CLIENT_PREFIX.to_string() + CLIENT_VERSION + "-" + &rand_filler + &timestamp;

    debug_assert_eq!(client_id.len(), 20);

    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(&client_id.as_bytes()[..20]);

    Self {
      inner: bytes,
    }
  }
}

impl ToString for ClientId {
  fn to_string(self: &Self) -> String {
    String::from_utf8(self.inner.to_vec()).unwrap()
  }
}

pub struct Client {
  client_id: ClientId,
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
      client_id: ClientId::new(),
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

  fn tracker_start_request(self: &Self) -> Request {
    Request::new(
      self.torrent.get_tracker_url(),
      self.torrent.get_info_hash(),
      self.client_id.to_string(),
      self.port,
      0,
      0,
      1024,
      1,
      0,
      Some(Event::Started),
      None,
      Some(50),
      None,
      None
    )
  }

  // todo: implement stop, resume functionality | priority: high

  fn request_peers(self: &mut Self) -> Result<Vec<[u8; 6]>, Box<dyn Error>> {
    let started_request: Request = self.tracker_start_request();
    let mut iters = 0;
    // todo: move this to config
    let max_iters = 100;

    while iters < max_iters {
      let response: Response = serde_bencode::from_bytes(&started_request.get()).unwrap();
      let peers = response.get_peers();

      if peers.len() != 0 {
        return Ok(peers);
      }
      iters += 1;
      thread::sleep(time::Duration::from_secs(2));
    }

    panic!("Couldn't get peers from the tracker");
  }

  pub fn run(self: &mut Self) {
    let peers = self.request_peers().unwrap();
    self.set_peers(&peers);
  }
}

fn get_timestamp() -> String {
  let now = std::time::SystemTime::now().duration_since(
    std::time::SystemTime::UNIX_EPOCH
  );
  let timestamp: String = now.unwrap().as_secs().to_string();

  timestamp
}
