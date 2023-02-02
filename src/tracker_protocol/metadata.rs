use serde_bencode;
use serde_bytes::ByteBuf;
use serde_derive::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

// #[derive(Debug, Deserialize)]
// struct Node(String, i64);

#[derive(Serialize, Deserialize, Debug)]
pub struct FileInfo {
    pub path: Vec<String>,
    pub length: u64,
    #[serde(default)]
    md5sum: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Info {
    pub name: String,
    pub pieces: ByteBuf,
    #[serde(rename = "piece length")]
    piece_length: u32,
    #[serde(default)]
    pub length: Option<u64>,
    #[serde(default)]
    pub files: Option<Vec<FileInfo>>,

    #[serde(default)]
    md5sum: Option<String>,
    #[serde(default)]
    private: Option<u8>,
    #[serde(default)]
    path: Option<Vec<String>>,
    #[serde(default)]
    #[serde(rename = "root hash")]
    root_hash: Option<String>,
}

impl Info {
    pub fn piece_len(&self) -> u32 {
        self.piece_length
    }

    pub fn len(&self) -> u64 {
        self.length.unwrap_or_else(|| {
            self.files
                .as_ref()
                .unwrap()
                .iter()
                .fold(0, |acc, f| f.length + acc)
        })
    }

    pub fn filename(&self) -> &String {
        &self.name
    }

    pub fn hash(&self) -> [u8; 20] {
        let bencoded = serde_bencode::to_bytes(&self).unwrap();
        let info_hash = Sha1::digest(bencoded);

        info_hash.into()
    }

    pub fn piece_hashes(&self) -> Vec<u8> {
        self.pieces.clone().into_vec()
    }
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub info: Info,
    #[serde(default)]
    pub announce: Option<String>,
    #[serde(rename = "announce-list")]
    pub announce_list: Option<Vec<Vec<String>>>,
    // #[serde(default)]
    // nodes: Option<Vec<Node>>,
    // #[serde(default)]
    // encoding: Option<String>,
    // #[serde(default)]
    // httpseeds: Option<Vec<String>>,
    // #[serde(default)]
    // #[serde(default)]
    // #[serde(rename = "creation date")]
    // creation_date: Option<i64>,
    // comment: Option<String>,
    // #[serde(default)]
    // #[serde(rename = "created by")]
    // created_by: Option<String>,
}

impl Metadata {
    pub fn get_tracker_url(&self) -> String {
        self.announce.as_ref().unwrap().clone()
    }
}

#[cfg(test)]
mod test {
    use super::Metadata;
    use std::fs;

    #[test]
    fn info_hash() {
        const TORRENT_FILENAME: &str = "torrents/test_info_hash";
        const INFO_HASH: &str = "f47932ba13094be79904a714a406e7c809636c53";
        let file: Vec<u8> = fs::read(TORRENT_FILENAME).unwrap();
        let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();

        let info_hash = torrent.info.hash();
        debug_assert_eq!(hex::encode(info_hash), INFO_HASH,);
        debug_assert_eq!(info_hash.len(), 20);
    }
}
