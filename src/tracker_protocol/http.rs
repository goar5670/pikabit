
use reqwest;
use serde_bytes::ByteBuf;
use serde_derive::*;
use serde_qs as qs;
use std::net::Ipv4Addr;
use urlencoding;

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
pub struct Request {
    #[serde(skip)]
    base_url: String,
    #[serde(skip)]
    info_hash: [u8; 20],
    #[serde(rename = "peer_id")]
    client_id: String,
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

impl Request {
    pub fn new(
        base_url: String,
        info_hash: [u8; 20],
        client_id: String,
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
    ) -> Self {
        Self {
            base_url,
            info_hash,
            client_id,
            port,
            uploaded,
            downloaded,
            left,
            compact,
            no_peer_id,
            event,
            ip,
            numwant,
            key,
            tracker_id,
        }
    }
    pub async fn get(&self) -> Vec<u8> {
        let encoded_info_hash = urlencoding::encode_binary(&self.info_hash);
        let url = format!(
            "{}?info_hash={}&{}",
            self.base_url,
            encoded_info_hash,
            qs::to_string(&self).unwrap()
        );
        let response = reqwest::get(url).await.unwrap().bytes().await.unwrap();

        Vec::from(response)
    }
}

#[derive(Debug, Deserialize)]
pub struct Response {
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

    interval: u32,
    complete: u32,
    incomplete: u32,
    peers: ByteBuf,
}

impl Response {
    pub fn get_peers(&self) -> Vec<[u8; 6]> {
        self.peers
            .chunks(6)
            .map(|chunck| chunck.try_into().unwrap())
            .collect()
    }
}