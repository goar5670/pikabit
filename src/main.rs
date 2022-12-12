use std::fs;
use pikabit::{
  metadata::Metadata,
  constants
};


fn main() {
  const TORRENT_FILENAME: &'static str = constants::torrents::UBUNTU22;

  let file: Vec<u8> = fs::read(TORRENT_FILENAME).unwrap();
  let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();
  
  let info_hash = torrent.get_info_hash();
  let peer_id = torrent.generate_peer_id();

  println!("{:?} {:?}", info_hash, peer_id);
}
