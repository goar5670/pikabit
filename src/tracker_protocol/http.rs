use log::info;
use reqwest;
use serde_bencode;
use serde_bytes::ByteBuf;
use serde_derive::*;
use serde_qs as qs;
use std::net::Ipv4Addr;
use urlencoding;

use crate::error::Result;

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

#[derive(Debug)]
pub struct HttpTracker {
    url: String,
}

impl HttpTracker {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
        }
    }

    async fn announce(
        &self,
        info_hash: &[u8; 20],
        peer_id: &[u8; 20],
        port: u16,
    ) -> Result<Vec<u8>> {
        let request = Request {
            port,
            downloaded: 0,
            left: 100, // todo: send real left
            uploaded: 0,
            event: Some(Event::Started),
            compact: 1,
            no_peer_id: 1,
            ip: None,
            numwant: None,
            key: None,
            tracker_id: None,
        };

        let enc_info_hash = urlencoding::encode_binary(info_hash);
        let enc_peer_id = urlencoding::encode_binary(peer_id);

        let request_url = format!(
            "{}?info_hash={}&peer_id={}&{}",
            self.url,
            enc_info_hash,
            enc_peer_id,
            qs::to_string(&request).unwrap(),
        );

        info!("{}", request_url);

        let res = reqwest::get(request_url).await?.bytes().await?;

        let de_res: Response = serde_bencode::from_bytes(&res)?;

        Ok(de_res.peers.to_vec())
    }

    pub async fn get_peers(
        &self,
        info_hash: &[u8; 20],
        peer_id: &[u8; 20],
        port: u16,
    ) -> Result<Vec<[u8; 6]>> {
        Ok(super::parse_peers(
            &self.announce(info_hash, peer_id, port).await?,
        ))
    }
}
