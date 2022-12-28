use log::{info, warn};
use std::{cmp, collections::HashMap, sync::Arc};
use tokio::{sync::mpsc::Sender, task::JoinHandle};

use crate::conc::{SharedMut, SharedRw};

use super::msg::Message;
use super::peer::PeerId;
use super::piece::PieceTracker;
use super::PeerTracker;

pub struct RequestsTracker {
    requested: HashMap<Arc<PeerId>, u32>,
    cap: u32,
}

impl RequestsTracker {
    pub fn new(cap: Option<u32>) -> Self {
        Self {
            requested: HashMap::new(),
            cap: cap.unwrap_or(5),
        }
    }

    pub fn full(&self, peer_id: &Arc<PeerId>) -> bool {
        self.cnt(peer_id) == self.cap
    }

    pub fn increase(&mut self, peer_id: Arc<PeerId>) {
        if self.requested.contains_key(&peer_id) {
            *self.requested.get_mut(&peer_id).unwrap() += 1;
        } else {
            self.requested.insert(peer_id.clone(), 1);
        }
    }

    pub fn decrease(&mut self, peer_id: &Arc<PeerId>) {
        if self.requested.contains_key(peer_id) {
            let cnt = self.requested.get_mut(peer_id).unwrap();
            if *cnt > 0 {
                *cnt -= 1;
            } else {
                warn!("Unexpected remove when cnt is 0. peer_id: {:?}", peer_id,);
            }
        } else {
            warn!(
                "Unexpected remove when requested doesn't contain peer_id. peer_id: {:?}",
                peer_id,
            );
        }
    }

    pub fn cnt(&self, peer_id: &Arc<PeerId>) -> u32 {
        self.requested.get(peer_id).copied().unwrap_or(0)
    }
}

pub fn spawn_reqh(
    pc_tracker: SharedRw<PieceTracker>,
    pr_map: SharedMut<HashMap<Arc<PeerId>, SharedRw<PeerTracker>>>,
    req_tracker: SharedRw<RequestsTracker>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while pc_tracker.get().await.rem() > 0 {
            while pc_tracker.get().await.is_empty() {}

            let next_piece = pc_tracker.get_mut().await.next_piece();
            if let Some(piece_index) = next_piece {
                'outer: loop {
                    for (peer_id, pr_tracker) in pr_map.lock().await.iter() {
                        if !req_tracker.get().await.full(peer_id)
                            && !pr_tracker.get_mut().await.state.am_choked
                        {
                            req_tracker.get_mut().await.increase(peer_id.clone());
                            let (piece_length, block_size) = async {
                                let lock = pc_tracker.get().await;
                                (
                                    lock.metadata.piece_len(piece_index),
                                    lock.metadata.block_size(),
                                )
                            }
                            .await;
                            request_piece(
                                piece_index,
                                piece_length,
                                block_size,
                                &pr_tracker.get().await.msg_tx,
                            )
                            .await;
                            pc_tracker.get_mut().await.on_piece_requested(piece_index);
                            info!(
                                "peer_id: {:?}, requested piece {}, reqt_len: {}, rem: {}",
                                peer_id,
                                piece_index,
                                req_tracker.get().await.cnt(peer_id),
                                pc_tracker.get().await.pieces_pq.len()
                            );
                            break 'outer;
                        }
                    }
                }
            }
        }
    })
}

async fn request_piece(
    piece_index: u32,
    piece_length: u32,
    block_size: u32,
    msg_tx: &Sender<Message>,
) {
    let mut cur_offset: u32 = 0;

    while cur_offset != piece_length {
        let length = cmp::min(block_size, piece_length - cur_offset);
        let _ = msg_tx
            .send(Message::Request(piece_index, cur_offset, length))
            .await;
        cur_offset += length;
    }
}
