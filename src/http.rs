use std::{net::Ipv4Addr};
use urlencoding;
use serde_qs as qs;
use serde_derive::*;
use serde_bytes::ByteBuf;
use reqwest;

#[derive(Debug, Serialize)]
pub enum Event {
  #[serde(rename = "started")]
  Started,
  #[serde(rename = "stopped")]
  Stopped,
  #[serde(rename = "completed")]
  Completed,
}

#[derive(Debug, Serialize)]
pub struct TrackerRequest {
  #[serde(skip)]
  base_url: String,
  #[serde(skip)]
  info_hash: [u8; 20],
  peer_id: String,
  port: u16,
  uploaded: u64,
  downloaded: u64,
  left: u64,
  compact: u8,
  no_peer_id: u8,
  event: Option<Event>,
  ip: Option<Ipv4Addr>,
  numwant: Option<u32>,
  key: Option<String>,
  tracker_id: Option<String>,
}

impl TrackerRequest {
  pub fn new(
    base_url: String, info_hash: [u8; 20], peer_id: String,
    port: u16, event: Option<Event>
  ) -> Self {
    Self {
      base_url,
      info_hash,
      peer_id,
      port,
      uploaded: 0,
      downloaded: 0,
      left: 1024,
      compact: 0,
      no_peer_id: 0,
      event,
      ip: None,
      numwant: Some(50),
      key: None,
      tracker_id: None,
    }
  }
  pub fn get(self: &Self) -> Vec<u8> {
    let encoded_info_hash = urlencoding::encode_binary(&self.info_hash);
    let url = format!("{}?info_hash={}&{}", self.base_url, encoded_info_hash, qs::to_string(&self).unwrap());
    let response = reqwest::blocking::get(url).unwrap().bytes().unwrap();

    Vec::from(response)
  }
}

#[derive(Debug, Deserialize)]
pub struct TrackerResponse {
  #[serde(default)]
  #[serde(rename = "failure response")]
  failure_response: Option<String>,
  #[serde(default)]
  #[serde(rename = "warning message")]
  warning_message: Option<String>,
  #[serde(default)]
  #[serde(rename = "min interval")]
  min_interval: Option<u32>,
  #[serde(default)]
  #[serde(rename = "tracker id")]
  tracker_id: Option<String>,

  complete: u32,
  incomplete: u32,
  peers: ByteBuf,
}

impl TrackerResponse {
  // todo: handle response failure
  pub fn get_peers(self: &Self) -> Vec<[u8; 6]> {
    self.peers.chunks(6).map(|chunck| {
      let mut ret = [0u8; 6];
      ret.copy_from_slice(chunck);

      ret
    }).collect()
  }
}


