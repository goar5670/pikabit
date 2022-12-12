use serde_derive::{Deserialize, Serialize};
use serde_bencode;
use serde_bytes::ByteBuf;
use sha1::{Sha1, Digest};
use urlencoding;
use rand::{distributions::Alphanumeric, Rng};

use super::constants;

// #[derive(Debug, Deserialize)]
// struct Node(String, i64);

#[derive(Serialize, Deserialize, Debug)]
pub struct File {
  path: Vec<String>,
  length: i64,
  #[serde(default)]
  md5sum: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Info {
  name: String,
  pieces: ByteBuf,
  #[serde(rename = "piece length")]
  piece_length: i64,
  #[serde(default)]
  md5sum: Option<String>,
  #[serde(default)]
  length: Option<i64>,
  #[serde(default)]
  files: Option<Vec<File>>,
  #[serde(default)]
  private: Option<u8>,
  #[serde(default)]
  path: Option<Vec<String>>,
  #[serde(default)]
  #[serde(rename = "root hash")]
  root_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
  info: Info,
  announce: String,
  // #[serde(default)]
  // nodes: Option<Vec<Node>>,
  #[serde(default)]
  encoding: Option<String>,
  #[serde(default)]
  httpseeds: Option<Vec<String>>,
  #[serde(default)]
  #[serde(rename = "announce-list")]
  announce_list: Option<Vec<Vec<String>>>,
  #[serde(default)]
  #[serde(rename = "creation date")]
  creation_date: Option<i64>,
  comment: Option<String>,
  #[serde(default)]
  #[serde(rename = "created by")]
  created_by: Option<String>,
}

impl Metadata {  
  pub fn get_info_hash(self: &Self) -> String {
    let bencoded = serde_bencode::to_bytes(&self.info).unwrap();
  
    let hashed = sha1_hash(&bencoded);
  
    let info_hash = urlencoding::encode_binary(&hashed);
  
    info_hash.into_owned()
  }

  pub fn generate_peer_id(self: &Self) -> String {
    let now = std::time::SystemTime::now().duration_since(
        std::time::SystemTime::UNIX_EPOCH
    );

    let timestamp = &now.unwrap().as_secs().to_string();

    let mut client_id: String = "-PIKABIT".to_owned();

    let rand_filler: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20 - timestamp.len() - client_id.len())
        .map(char::from)
        .collect();
      
    client_id.push_str(timestamp);
    client_id.push_str(rand_filler.as_str());

    client_id
  }
}


fn sha1_hash(bytes: &Vec<u8>) -> [u8; 20] {
  let mut hasher = Sha1::new();
  hasher.update(&bytes);
  let result_arr = hasher.finalize();
  debug_assert_eq!(result_arr.len(), 20);
  let mut result = [0u8; 20];
  result.copy_from_slice(&result_arr);
  result
}

#[cfg(test)]
mod test {
  use super::*;
  use std::fs;

  const TORRENT_FILENAME: &'static str = constants::torrents::UBUNTU22;
  
  #[test]
  fn info_hash() {
    let file: Vec<u8> = fs::read(TORRENT_FILENAME).unwrap();
    let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();
    
    let info_hash = torrent.get_info_hash();
    debug_assert_eq!(info_hash, constants::torrents::UBUNTU22_INFO_HASH)
  }

  #[test]
  fn peer_id() {
    let file: Vec<u8> = fs::read(TORRENT_FILENAME).unwrap();
    let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();

    let peer_id = torrent.generate_peer_id();
    debug_assert_eq!(peer_id.len(), 20);
  } 
}