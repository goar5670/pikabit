use serde_derive::{Deserialize, Serialize};
use serde_bencode;
use serde_bytes::ByteBuf;
use sha1::{Sha1, Digest};

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
  // #[serde(default)]
  // encoding: Option<String>,
  // #[serde(default)]
  // httpseeds: Option<Vec<String>>,
  // #[serde(default)]
  // #[serde(rename = "announce-list")]
  // announce_list: Option<Vec<Vec<String>>>,
  // #[serde(default)]
  // #[serde(rename = "creation date")]
  // creation_date: Option<i64>,
  // comment: Option<String>,
  // #[serde(default)]
  // #[serde(rename = "created by")]
  // created_by: Option<String>,

  // todo: check for the correctness of the metadta
}

impl Metadata {  
  pub fn get_info_hash(self: &Self) -> [u8; 20] {
    let bencoded = serde_bencode::to_bytes(&self.info).unwrap();
    let info_hash = sha1_hash(&bencoded);
    
    info_hash
  }

  pub fn get_tracker_url(self: &Self) -> String {
    self.announce.clone()
  }    
}

fn sha1_hash(bytes: &[u8]) -> [u8; 20] {
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
  use super::Metadata;
  use super::super::constants;
  use std::fs;

  #[test]
  fn info_hash() {
    const TORRENT_FILENAME: &'static str = constants::torrents::UBUNTU22;
    let file: Vec<u8> = fs::read(TORRENT_FILENAME).unwrap();
    let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();
    
    let info_hash = torrent.get_info_hash();
    debug_assert_eq!(hex::encode(info_hash), constants::torrents::UBUNTU22_INFO_HASH);
    debug_assert_eq!(info_hash.len(), 20);
  }
}