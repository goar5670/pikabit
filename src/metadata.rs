// todo: potentially move this to tracker_protocol | priority: med

use serde_bencode;
use serde_bytes::ByteBuf;
use serde_derive::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

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
    piece_length: u32,

    #[serde(default)]
    md5sum: Option<String>,
    #[serde(default)]
    length: Option<u32>,
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

impl Info {
    fn _get_last_piece_len(self: &Self) -> u32 {
        let ret = self.length.unwrap() % self.piece_length;
        if ret == 0 {
            return self.piece_length;
        }
        ret
    }

    pub fn get_piece_len(self: &Self, piece_index: u32) -> u32 {
        if piece_index == self.get_num_pieces() - 1 {
            return self._get_last_piece_len();
        }
        self.piece_length
    }

    pub fn get_length(self: &Self) -> u32 {
        self.length.unwrap()
    }

    pub fn get_num_pieces(self: &Self) -> u32 {
        // debug_assert_eq!(self.info.length.unwrap() % self.info.piece_length, 0);
        (self.length.unwrap() + self.piece_length - 1) / self.piece_length
    }

}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub info: Info,
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

    // pub fn get_piece_length(self: &Self) -> u64 {
    //     self.info.piece_length
    // }
}

pub fn sha1_hash(bytes: &[u8]) -> [u8; 20] {
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
    use super::super::constants;
    use super::Metadata;
    use std::fs;

    #[test]
    fn info_hash() {
        const TORRENT_FILENAME: &'static str = constants::torrents::UBUNTU22;
        let file: Vec<u8> = fs::read(TORRENT_FILENAME).unwrap();
        let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();

        let info_hash = torrent.get_info_hash();
        debug_assert_eq!(
            hex::encode(info_hash),
            constants::torrents::UBUNTU22_INFO_HASH
        );
        debug_assert_eq!(info_hash.len(), 20);
    }
}
