use futures::future::join_all;
use log::{info, warn};
use serde_bencode;
use std::{collections::HashMap, error::Error, fs};
use tokio::sync::mpsc;

use crate::bitfield::*;
use crate::conc::{SharedMut, SharedRw};

use crate::metadata::Metadata;
use crate::peer_protocol::{
    self,
    msg::Message,
    peer::{Peer, PeerId},
    piece::{PieceBuffer, PieceTracker},
    requests::{self, RequestsTracker},
    PeerTracker,
};
use crate::tracker_protocol::http::{Event, Request, Response};

pub struct Client {
    peer_id: PeerId,
    torrent: Metadata,
    port: u16,
}

impl Client {
    pub fn new(filename: &str, port: Option<u16>) -> Self {
        let file: Vec<u8> = fs::read(filename).unwrap();
        let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();

        let default_port = 6881;

        Self {
            peer_id: PeerId::new(),
            torrent,
            port: match port {
                Some(p) => p,
                None => default_port,
            },
        }
    }

    fn tracker_start_request(&self) -> Request {
        Request::new(
            self.torrent.get_tracker_url(),
            self.torrent.info.hash(),
            self.peer_id.to_string(),
            self.port,
            0,
            0,
            self.torrent.info.len(),
            1,
            0,
            Some(Event::Started),
            None,
            Some(50),
            None,
            None,
        )
    }

    // todo: implement stop, resume functionality | priority: high

    // todo: implement non compact response format | priority: high

    async fn request_peers(&mut self) -> Result<Vec<[u8; 6]>, Box<dyn Error>> {
        let started_request: Request = self.tracker_start_request();
        let mut iters = 0;
        // todo: move this to config
        let max_iters = 100;

        info!("Fetching peers from tracker...");

        while iters < max_iters {
            let response: Response =
                serde_bencode::from_bytes(&started_request.get().await).unwrap();
            let peers = response.get_peers();

            if !peers.is_empty() {
                return Ok(peers);
            }
            iters += 1;
        }

        panic!("Couldn't get peers from the tracker");
    }

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

    pub async fn run(mut self) {
        let peer_list = self.request_peers().await.unwrap();
        info!("number of peers {}", peer_list.len());

        let pc_tracker = SharedRw::new(PieceTracker::new(
            self.torrent.info.len(),
            self.torrent.info.piece_len(0),
        ));
        let pct_clone = pc_tracker.clone();

        // let (fh_tx, fh_handle) = FileHandler::new(&self.torrent.info.filename()).await;
        let (tx, mut rx) = mpsc::channel(40);

        let pr_map = SharedMut::new(HashMap::new());

        let mut pch_handles = vec![];
        let num_pieces = self.torrent.info.num_pieces();
        let hs_payload = self.hs_payload();

        for peer in peer_list {
            let pr_map_clone = pr_map.clone();
            let tx_clone = tx.clone();

            let pch_handle = tokio::spawn(async move {
                match peer_protocol::spawn_prch(Peer::from(peer), tx_clone, hs_payload).await {
                    Ok((peer_id, msg_tx, pch_handle)) => {
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
                        warn!("{}", str);
                        Ok(())
                    }
                }
            });

            pch_handles.push(pch_handle);
        }

        let pr_map_clone = pr_map.clone();

        let req_tracker = SharedRw::new(RequestsTracker::new(None));
        let reqt_clone = req_tracker.clone();

        let mut pbuf = PieceBuffer::new();

        let recv_handle = tokio::spawn(async move {
            while let Some((peer_id, msg)) = rx.recv().await {
                if let Message::Piece(piece_index, offset_in_piece, buf) = msg {
                    let offset_in_buffer = pbuf.append(piece_index, &buf);
                    let rem = pc_tracker.get_mut().await.on_block_buffered(
                        piece_index,
                        offset_in_buffer,
                        offset_in_piece,
                    );

                    if rem == 0 {
                        info!(
                            "peer_id: {:?}, piece {} saved, rem {}",
                            peer_id,
                            piece_index,
                            pc_tracker.get().await.rem()
                        );
                        req_tracker.get_mut().await.decrease(peer_id);
                        pc_tracker.get_mut().await.on_piece_completed(piece_index);
                        // get piece from buf and save it
                    }
                } else {
                    let lock = pr_map.lock().await;
                    let mut pr_tracker = lock.get(&peer_id).unwrap().get_mut().await;
                    match msg {
                        Message::Have(piece_index) => {
                            if !pr_tracker.state.am_choked && pr_tracker.state.am_interested {
                                pc_tracker.get_mut().await.update_single(piece_index);
                            }
                            pr_tracker.update_have(piece_index);
                        }
                        Message::Bitfield(buf) => {
                            if !pr_tracker.state.am_choked && pr_tracker.state.am_interested {
                                pc_tracker.get_mut().await.update_multiple(&buf);
                            }
                            let bitfield = BitfieldRef::new(
                                &buf,
                                pc_tracker.get().await.metadata.num_pieces(),
                            );

                            for i in 0..pc_tracker.get().await.metadata.num_pieces() {
                                if bitfield.get(i).unwrap() {
                                    pr_tracker.update_have(i);
                                }
                            }
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
        });

        let rqh_handle = requests::spawn_reqh(pct_clone, pr_map_clone, reqt_clone);

        join_all(pch_handles).await;
        join_all(vec![rqh_handle, recv_handle]).await;
    }
}
