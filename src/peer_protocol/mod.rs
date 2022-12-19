// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Sender};

use crate::file;
use crate::peer::*;
use crate::{concurrency::SharedRef, metadata::Info};
use msg::{Message, RecvHandler, SendHandler};
use piece::PieceHandler;

mod bitfield;
mod msg;
mod piece;

pub struct PeerHandler {
    peer: Peer,
    // (am_choked, am_interested, peer_choked, peer_interested)
    state: SharedRef<State>,
    fh_tx: Sender<file::Cmd>,
    client_id: Arc<PeerId>,
    info: Arc<Info>,
}

impl PeerHandler {
    pub fn new(
        peer: Peer,
        fh_tx: Sender<file::Cmd>,
        info: Arc<Info>,
        client_id: Arc<PeerId>,
    ) -> Self {
        Self {
            peer,
            state: SharedRef::new((true, false, true, false)),
            fh_tx,
            client_id,
            info,
        }
    }

    fn _handshake_payload(self: &Self) -> [u8; 68] {
        let mut payload = [0u8; 68];
        payload[0] = 19;
        payload[1..20].copy_from_slice("BitTorrent protocol".as_bytes());
        payload[20..28].copy_from_slice(&[0u8; 8]);
        payload[28..48].copy_from_slice(&self.info.hash());
        payload[48..].copy_from_slice(self.client_id.as_bytes());

        payload
    }

    pub async fn run(mut self) {
        let mut stream = self.peer.connect().await.unwrap();
        self.peer
            .handshake(&self._handshake_payload(), &mut stream)
            .await;

        let (read_half, write_half) = stream.into_split();

        let (msg_tx, msg_rx) = mpsc::channel(40);
        let (ph_tx, ph_rx) = mpsc::channel(40);
        let piece_handler = PieceHandler::new(
            self.info.clone(),
            &self.state,
            self.fh_tx.clone(),
            msg_tx.clone(),
            ph_rx,
        );
        let recv_handler = RecvHandler::new(read_half);
        let send_handler = SendHandler::new(write_half);

        let mut handles = vec![];

        handles.push(tokio::spawn(send_handler.run(msg_rx)));
        let _ = msg_tx.send(Message::Interested).await;
        handles.push(tokio::spawn(recv_handler.run(self.state.clone(), ph_tx)));
        handles.push(tokio::spawn(piece_handler.run()));

        join_all(handles).await;
    }
}
