use futures::future::join_all;
use log::trace;
use regex::Regex;
use std::{collections::HashMap, sync::Arc};

use http::HttpTracker;
use tokio::{net::UdpSocket, sync::mpsc::Sender};

use self::udp::UdpTracker;
use crate::conc::{self, SharedMut};
use crate::error::Result;

pub mod http;
pub mod metadata;
pub mod udp;

#[derive(Debug)]
enum Protocol {
    UDP,
    HTTP,
}

enum Tracker {
    Udp(UdpTracker),
    Http(HttpTracker),
}

impl Tracker {
    async fn new_udp(url: &str, socket: &SharedMut<UdpSocket>) -> Self {
        Self::Udp(UdpTracker::new(url, socket).await)
    }

    fn new_http(url: &str) -> Self {
        Self::Http(HttpTracker::new(url))
    }

    async fn get_peers(
        &self,
        info_hash: Arc<[u8; 20]>,
        peer_id: Arc<[u8; 20]>,
    ) -> Result<Vec<[u8; 6]>> {
        let peers_res = match self {
            Self::Udp(u) => u.get_peers(&info_hash, &peer_id).await,
            Self::Http(h) => h.get_peers(&info_hash, &peer_id).await,
        };

        trace!("{:?}", peers_res);

        peers_res
    }
}

fn parse_peers(buf: &[u8]) -> Vec<[u8; 6]> {
    buf.chunks(6)
        .map(|chunk| chunk.try_into().unwrap())
        .collect()
}

fn parse_tracker_url(url: &str) -> Result<(Protocol, String)> {
    if url.starts_with("udp://") {
        let re = Regex::new(r"//(.+:[0-9]+)")?;
        let addr = re.captures(url).unwrap().get(1).unwrap().as_str();
        return Ok((Protocol::UDP, addr.to_owned()));
    } else if url.starts_with("http://") || url.starts_with("https://") {
        return Ok((Protocol::HTTP, url.to_owned()));
    }

    Err(format!("Failure parsing tracker url {}", url).into())
}

pub async fn spawn_tch(
    metadata: &metadata::Metadata,
    peer_id: Arc<[u8; 20]>,
    port: u16,
    tx: Sender<[u8; 6]>,
) {
    let info_hash = Arc::new(metadata.info.hash());
    let socket = SharedMut::new(UdpSocket::bind("0.0.0.0:6881").await.unwrap());

    let mut handles = vec![];

    let peers_map: SharedMut<HashMap<[u8; 6], bool>> = SharedMut::new(HashMap::new());

    for list in metadata.announce_list.as_ref().unwrap() {
        for url in list {
            if let Ok((t, url)) = parse_tracker_url(&url) {
                println!("{}", url);

                let tx_clone = tx.clone();
                let socket_clone = socket.clone();
                let info_hash_clone = info_hash.clone();
                let peer_id_clone = peer_id.clone();
                let pm_clone = peers_map.clone();

                let handle = tokio::spawn(async move {
                    let tracker = match t {
                        Protocol::UDP => Tracker::new_udp(&url, &socket_clone).await,
                        Protocol::HTTP => Tracker::new_http(&url),
                    };

                    let peers = conc::timeout(3, tracker.get_peers(info_hash_clone, peer_id_clone))
                        .await
                        .unwrap_or(vec![]);

                    for addr in peers {
                        trace!("{:?}", addr);
                        if !pm_clone.lock().await.contains_key(&addr) {
                            let _ = tx_clone.send(addr).await;
                            pm_clone.lock().await.insert(addr, true);
                        }
                    }
                });
                handles.push(handle);
            }
        }
    }

    join_all(handles).await;
}
