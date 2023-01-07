use futures::future::join_all;
use log::{info, trace};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use http::HttpTracker;
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};

use self::udp::{spawn_udp_rh, UdpTracker};
use crate::{conc::SharedMut, error::Result};

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
    fn new_udp(addr: SocketAddr, socket: SharedMut<UdpSocket>, rx: Receiver<Vec<u8>>) -> Self {
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
    ) -> Result<Vec<SocketAddr>> {
        let peers_res = match self {
            Self::Udp(u) => u.get_peers(&info_hash, &peer_id, port).await,
            Self::Http(h) => h.get_peers(&info_hash, &peer_id, port).await,
        };

        if peers_res.is_err() {
            trace!("tracker: {}, error: {:?}", self.to_string(), peers_res);
        }

        peers_res
    }
}

impl ToString for Tracker {
    fn to_string(&self) -> String {
        match self {
            Self::Udp(t) => format!("{:?}", t.addr),
            Self::Http(t) => format!("{:?}", t),
        }
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
    client_tx: Sender<SocketAddr>,
) -> JoinHandle<()> {
    let info_hash = Arc::new(metadata.info.hash());
    let socket = SharedMut::new(UdpSocket::bind(format!("0.0.0.0:{port}")).await.unwrap());

    let peers_map: SharedMut<HashSet<SocketAddr>> = SharedMut::new(HashSet::new());

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
                            Tracker::new_udp(addr, socket.clone(), rx)
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

    let udp_rh_handle = spawn_udp_rh(tracker_map, socket.clone());

    tokio::spawn(async move {
        let mut udp_handles = vec![];
        let mut http_handles = vec![];

        trackers_list.into_iter().for_each(|mut tracker| {
            let client_tx_clone = client_tx.clone();
            let info_hash_clone = info_hash.clone();
            let peer_id_clone = peer_id.clone();
            let pm_clone = peers_map.clone();
            let tracker_type = match tracker {
                Tracker::Udp(_) => Protocol::UDP,
                Tracker::Http(_) => Protocol::HTTP,
            };

            let handle = tokio::spawn(async move {
                let peers = tracker
                    .get_peers(info_hash_clone, peer_id_clone, port)
                    .await
                    .unwrap_or(vec![]);

                let peers_len = peers.len();

                for addr in peers {
                    let mut lock = pm_clone.lock().await;
                    if !lock.contains(&addr) {
                        trace!("new peer {:?} from {}", addr, tracker.to_string());
                        lock.insert(addr);
                        let _ = client_tx_clone.send(addr).await;
                    }
                }
                info!(
                    "processed {} peers from tracker {}, exiting task",
                    peers_len,
                    tracker.to_string()
                );
            });

            match tracker_type {
                Protocol::HTTP => http_handles.push(handle),
                Protocol::UDP => udp_handles.push(handle),
            };
        });

        join_all(http_handles).await;
        tokio::select! {
            _ = join_all(udp_handles) => {},
            _ = udp_rh_handle => {},
        };

        info!("exiting tch task");
    })
}
