use log::{debug, error, info, trace, warn};
use std::time::Duration;
use std::{cmp, sync::Arc};
use tokio::sync::{RwLockWriteGuard, Semaphore};
use tokio::{sync::mpsc::Sender, task::JoinHandle};

use crate::bitfield::Bitfield;
use crate::conc::SharedRw;

use super::msg::Message;
use super::piece::PieceTracker;
use super::PeerTracker;

pub struct PeerRequestsHandler {
    pc_tracker: SharedRw<PieceTracker>,
    pr_tracker: SharedRw<PeerTracker>,
    sem: Arc<Semaphore>,
    cap: u32,
}

impl PeerRequestsHandler {
    fn new(
        pc_tracker: SharedRw<PieceTracker>,
        pr_tracker: SharedRw<PeerTracker>,
        sem: Arc<Semaphore>,
        cap: u32,
    ) -> Self {
        Self {
            pc_tracker,
            pr_tracker,
            sem,
            cap,
        }
    }

    async fn msg_tx(&self) -> Sender<Message> {
        self.pr_tracker.get().await.msg_tx.clone()
    }

    async fn next_piece(&self, lock: &RwLockWriteGuard<'_, PieceTracker>) -> Option<u32> {
        for piece_index in &lock.needed {
            if self
                .pr_tracker
                .get()
                .await
                .have
                .get(*piece_index)
                .unwrap_or(false)
            {
                return Some(*piece_index);
            }
        }
        None
    }

    async fn request_piece(&self, piece_index: u32, mut lock: RwLockWriteGuard<'_, PieceTracker>) {
        let mut cur_offset: u32 = 0;
        let msg_tx = self.msg_tx().await;
        lock.reserve_piece(piece_index);
        lock.needed.remove(&piece_index);
        let piece_length = lock.metadata.piece_len(piece_index);
        let block_size = lock.metadata.block_size();
        drop(lock);

        while cur_offset != piece_length {
            let length = cmp::min(block_size, piece_length - cur_offset);
            let _ = msg_tx
                .send(Message::Request(piece_index, cur_offset, length))
                .await;
            cur_offset += length;
        }
    }

    // hueristic about timeout to check if request was fullfilled
    async fn request_timeout(&self) -> Duration {
        let f = (self.cap - self.sem.available_permits() as u32) as u64;
        Duration::from_secs(f * 10)
    }

    async fn spawn_delayed_check(&self, piece_index: u32) {
        let pc_tracker = self.pc_tracker.clone();
        let timeout = self.request_timeout().await;

        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            let mut lock = pc_tracker.get_mut().await;
            if lock.is_reserved(piece_index).unwrap_or(false)
                && !lock.has_piece(piece_index).unwrap_or(true)
            {
                lock.unreserve_piece(piece_index);
                lock.update_single(piece_index);
                warn!("delayed check on piece {} yielded false", piece_index);
            }
        });
    }
}

pub async fn spawn_reqh(
    pc_tracker: SharedRw<PieceTracker>,
    pr_tracker: SharedRw<PeerTracker>,
    cap: u32,
) -> JoinHandle<()> {
    let reqs_sem = pr_tracker.get().await.reqs_sem.clone();
    let handler = PeerRequestsHandler::new(pc_tracker, pr_tracker, reqs_sem, cap);

    tokio::spawn(async move {
        loop {
            let sem = handler.sem.clone();
            let permit = match sem.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    error!("permit acquire error {:?}", e);
                    break;
                }
            };
            let pc_tracker_lock = handler.pc_tracker.get_mut().await;
            let next_piece = handler.next_piece(&pc_tracker_lock).await;

            if let Some(piece_index) = next_piece {
                permit.forget();
                debug!(
                    "requesting piece {} from {:?}",
                    piece_index,
                    handler.pr_tracker.get().await.id
                );
                handler.request_piece(piece_index, pc_tracker_lock).await;
                handler.spawn_delayed_check(piece_index).await;
            } else {
                trace!("permits before {}", sem.available_permits());
                drop(permit);
                drop(pc_tracker_lock);
                trace!("permits before {}", sem.available_permits());
                info!("no pieces to request from peer");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        info!("exiting reqh task");
    })
}
