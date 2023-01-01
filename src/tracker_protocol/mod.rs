use futures::future::join_all;
use log::trace;
use regex::Regex;
use std::net::{SocketAddr, ToSocketAddrs};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;

use http::HttpTracker;
use tokio::{
    net::UdpSocket,
    sync::mpsc::{Receiver, Sender},
};

use self::udp::{spawn_udp_rh, UdpTracker};
use crate::conc::SharedMut;
use crate::error::Result;

pub mod http;
pub mod metadata;
pub mod udp;

#[derive(Debug, PartialEq)]
enum Protocol {
    UDP,
    HTTP,
}

#[derive(Debug)]
enum Tracker {
    Udp(UdpTracker),
    Http(HttpTracker),
}

impl Tracker {
    fn new_udp(addr: SocketAddr, socket: &SharedMut<UdpSocket>, rx: Receiver<Vec<u8>>) -> Self {
        Self::Udp(UdpTracker::new(addr, socket, rx))
    }

    fn new_http(url: &str) -> Self {
        Self::Http(HttpTracker::new(url))
    }

    async fn get_peers(
        &mut self,
        info_hash: Arc<[u8; 20]>,
        peer_id: Arc<[u8; 20]>,
        port: u16,
    ) -> Result<Vec<[u8; 6]>> {
        let peers_res = match self {
            Self::Udp(u) => u.get_peers(&info_hash, &peer_id, port).await,
            Self::Http(h) => h.get_peers(&info_hash, &peer_id, port).await,
        };

        trace!("tracker: {:?}, error: {:?}", self, peers_res);

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

fn full_announce_list(metadata: &metadata::Metadata) -> Vec<String> {
    let list = metadata
        .announce_list
        .as_ref()
        .map_or(vec![], |v| v.clone());
    let mut flattened: Vec<String> = list.into_iter().flatten().collect();

    if let Some(x) = metadata.announce.as_ref() {
        flattened.push(x.to_owned());
    }

    flattened
}

pub async fn spawn_tch(
    metadata: &metadata::Metadata,
    peer_id: Arc<[u8; 20]>,
    port: u16,
    client_tx: Sender<[u8; 6]>,
) {
    let info_hash = Arc::new(metadata.info.hash());
    let socket = SharedMut::new(UdpSocket::bind(format!("0.0.0.0:{port}")).await.unwrap());

    let mut handles = vec![];

    let peers_map: SharedMut<HashMap<[u8; 6], bool>> = SharedMut::new(HashMap::new());

    let announce_list = full_announce_list(metadata);

    let mut tracker_map = HashMap::new();

    let trackers_list: Vec<Tracker> = announce_list
        .iter()
        .filter_map(|url| {
            if let Ok((t, url)) = parse_tracker_url(url) {
                if t == Protocol::UDP {
                    let res = url.to_socket_addrs();
                    let v = res
                        .unwrap_or(vec![].into_iter())
                        .map(|addr| {
                            let (tx, rx) = mpsc::channel(40);
                            tracker_map.insert(addr, tx);
                            Tracker::new_udp(addr, &socket, rx)
                        })
                        .collect();
                    return Some(v);
                } else {
                    return Some(vec![Tracker::new_http(&url)]);
                }
            }
            None
        })
        .flatten()
        .collect();

    spawn_udp_rh(tracker_map, socket.clone()).await;

    trackers_list.into_iter().for_each(|mut tracker| {
        let client_tx_clone = client_tx.clone();
        let info_hash_clone = info_hash.clone();
        let peer_id_clone = peer_id.clone();
        let pm_clone = peers_map.clone();

        let handle = tokio::spawn(async move {
            let peers = tracker
                .get_peers(info_hash_clone, peer_id_clone, port)
                .await
                .unwrap_or(vec![]);

            for addr in peers {
                trace!("new peer {:?}", addr);
                if !pm_clone.lock().await.contains_key(&addr) {
                    let _ = client_tx_clone.send(addr).await;
                    pm_clone.lock().await.insert(addr, true);
                }
            }
        });
        handles.push(handle);
    });

    join_all(handles).await;
}
