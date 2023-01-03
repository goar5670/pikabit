use futures::future::join_all;
use log::{info, trace, warn};
use serde_bencode;
use sha1::{Digest, Sha1};
use std::{collections::HashMap, fs, mem, sync::Arc};
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};

use crate::{
    bitfield::*,
    conc::{SharedMut, SharedRw},
    peer_protocol::{
        self,
        fops::FileManager,
        msg::Message,
        peer::{Peer, PeerId},
        piece::{PieceBuffer, PieceTracker},
        requests::{self, RequestsTracker},
        PeerTracker, RelayedMessage,
    },
    stats::StatsTracker,
    tracker_protocol::{metadata::Metadata, spawn_tch},
};

type PeerMap = HashMap<Arc<PeerId>, SharedRw<PeerTracker>>;

pub struct Client {
    pr_map: SharedMut<PeerMap>,
    req_tracker: SharedRw<RequestsTracker>,
    stats_tracker: SharedMut<StatsTracker>,
    pc_tracker: SharedRw<PieceTracker>,
    peer_id: PeerId,
    torrent: Metadata,
    port: u16,
}

impl Client {
    pub fn new(filename: &str, port: Option<u16>) -> Self {
        let file: Vec<u8> = fs::read(filename).unwrap();
        let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();

        let port = port.unwrap_or(6881);

        let pr_map = SharedMut::new(HashMap::new());
        let req_tracker = SharedRw::new(RequestsTracker::new(None));
        let stats_tracker = SharedMut::new(StatsTracker::new(None, torrent.info.len()));
        let pc_tracker = SharedRw::new(PieceTracker::new(
            torrent.info.len(),
            torrent.info.piece_len(),
            torrent.info.piece_hashes(),
        ));

        Self {
            pr_map,
            req_tracker,
            stats_tracker,
            pc_tracker,
            peer_id: PeerId::new(),
            torrent,
            port,
        }
    }

    // todo: implement stop, resume functionality | priority: high

    // todo: implement non compact response format | priority: high

    fn hs_payload(&self) -> [u8; 68] {
        let info_hash = self.torrent.info.hash();
        let mut payload = [0u8; 68];
        payload[0] = 19;
        payload[1..20].copy_from_slice("BitTorrent protocol".as_bytes());
        payload[20..28].copy_from_slice(&[0u8; 8]);
        payload[28..48].copy_from_slice(&info_hash);
        payload[48..].copy_from_slice(self.peer_id.as_bytes());

        payload
    }

    async fn on_piece_completed(
        pc_tracker: &SharedRw<PieceTracker>,
        pbuf: &PieceBuffer,
        req_tracker: &SharedRw<RequestsTracker>,
        stats_tracker: &SharedMut<StatsTracker>,
        fman: &mut FileManager,
        piece_index: u32,
        peer_id: &Arc<PeerId>,
    ) {
        let mut lock = pc_tracker.get_mut().await;
        let piece = lock.ordered_piece(piece_index, pbuf);
        let piece_hash: [u8; 20] = Sha1::digest(&piece).try_into().unwrap();
        if !lock.verify_piece_hash(piece_index, &piece_hash) {
            warn!("hash verification failed for piece {piece_index}");
            lock.update_single(piece_index);
        } else {
            fman.write_piece(lock.metadata.piece_offset(piece_index), &piece)
                .await;
            lock.on_piece_saved(piece_index);
            let rem = lock.rem();
            mem::drop(lock);
            req_tracker.get_mut().await.decrease(&peer_id);
            info!(
                "peer_id: {:?}, piece {} saved, rem {}",
                peer_id, piece_index, rem,
            );
            stats_tracker
                .lock()
                .await
                .update(pc_tracker.get().await.metadata.piece_len(piece_index));
        }
    }

    async fn handle_peer_com(&self, mut peer_rx: Receiver<RelayedMessage>) -> JoinHandle<()> {
        let pc_tracker = self.pc_tracker.clone();
        let req_tracker = self.req_tracker.clone();
        let stats_tracker = self.stats_tracker.clone();
        let pr_map = self.pr_map.clone();
        let mut fman = FileManager::new(&self.torrent.info).await;

        tokio::spawn(async move {
            let mut pbuf = PieceBuffer::new();
            while let Some((peer_id, msg)) = peer_rx.recv().await {
                if let Message::Piece(piece_index, offset_in_piece, buf) = msg {
                    let offset_in_buffer = pbuf.append(piece_index, &buf);
                    let rem = pc_tracker.get_mut().await.on_block_buffered(
                        piece_index,
                        offset_in_buffer,
                        offset_in_piece,
                        buf.len(),
                    );

                    if rem == 0 {
                        Self::on_piece_completed(
                            &pc_tracker,
                            &pbuf,
                            &req_tracker,
                            &stats_tracker,
                            &mut fman,
                            piece_index,
                            &peer_id,
                        )
                        .await;
                    }
                } else {
                    let mut lock = pr_map.lock().await;
                    let mut pr_tracker = lock.get_mut(&peer_id).unwrap().get_mut().await;
                    match msg {
                        Message::Have(piece_index) => {
                            if !pr_tracker.state.am_choked && pr_tracker.state.am_interested {
                                pc_tracker.get_mut().await.update_single(piece_index);
                            }
                            pr_tracker.have.set(piece_index);
                        }
                        Message::Bitfield(buf) => {
                            if !pr_tracker.state.am_choked && pr_tracker.state.am_interested {
                                pc_tracker.get_mut().await.update_multiple(&buf);
                            }
                            pr_tracker.have = BitfieldRef::new(
                                &buf,
                                pc_tracker.get().await.metadata.num_pieces(),
                            )
                            .into();
                        }
                        Message::Choke => pr_tracker.state.am_choked = true,
                        Message::Unchoke => {
                            if pr_tracker.state.am_choked {
                                pr_tracker.state.am_choked = false;
                                if pr_tracker.have.cnt_marked() > 0
                                    && pr_tracker.state.am_interested
                                {
                                    pc_tracker
                                        .get_mut()
                                        .await
                                        .update_multiple(pr_tracker.have.bytes());
                                }
                            }
                        }
                        Message::Interested => pr_tracker.state.peer_interested = true,
                        Message::NotInterested => pr_tracker.state.peer_interested = false,
                        _ => {}
                    }
                }
            }
        })
    }

    async fn handle_tracker_com(&self, peer_tx: Sender<RelayedMessage>) -> JoinHandle<()> {
        let (tracker_tx, mut tracker_rx) = mpsc::channel(40);

        let num_pieces = self.pc_tracker.get().await.metadata.num_pieces();
        let pr_map = self.pr_map.clone();

        spawn_tch(
            &self.torrent,
            Arc::new(*self.peer_id.as_bytes()),
            self.port,
            tracker_tx.clone(),
        )
        .await;

        let hs_payload = self.hs_payload();

        tokio::spawn(async move {
            let mut pch_handles = vec![];
            while let Some(addr) = tracker_rx.recv().await {
                let pr_map_clone = pr_map.clone();
                let tx_clone = peer_tx.clone();

                let handle = tokio::spawn(async move {
                    match peer_protocol::spawn_prch(Peer::from(addr), tx_clone, hs_payload).await {
                        Ok((peer_id, msg_tx, pch_handle)) => {
                            if pr_map_clone.lock().await.contains_key(&peer_id) {
                                return Ok(());
                            }

                            let _ = msg_tx.send(Message::Interested).await;
                            let mut pr_tracker =
                                PeerTracker::new(BitfieldOwned::new(num_pieces), msg_tx);
                            pr_tracker.state.am_interested = true;

                            pr_map_clone
                                .lock()
                                .await
                                .insert(peer_id, SharedRw::new(pr_tracker));

                            pch_handle.await
                        }
                        Err(str) => {
                            warn!("prch error {}", str);
                            Ok(())
                        }
                    }
                });
                pch_handles.push(handle);
            }

            join_all(pch_handles).await;
        })
    }

    fn handle_stats(&self) -> JoinHandle<()> {
        let stats_tracker = self.stats_tracker.clone();
        tokio::spawn(async move {
            loop {
                stats_tracker.lock().await.print();
                tokio::time::sleep(tokio::time::Duration::from_millis(1234)).await;
            }
        })
    }

    pub async fn run(&self) {
        let (peer_tx, peer_rx) = mpsc::channel(40);

        let _ = tokio::select! {
            h = self.handle_tracker_com(peer_tx.clone()).await => h,
            h = self.handle_stats() => h,
            h = self.handle_peer_com(peer_rx).await => h,
            h = requests::spawn_reqh(
                self.pc_tracker.clone(),
                self.pr_map.clone(),
                self.req_tracker.clone()
            ) => h,
        };
    }
}
