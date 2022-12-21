// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use futures::future::join_all;
use std::sync::Arc;
use tokio::{
    sync::mpsc::{self, Sender},
    task::JoinHandle,
};

use crate::file;
use crate::peer::*;
use crate::{concurrency::SharedRw, metadata::Info};
use msg::{Message, RecvHandler, SendHandler};
use piece::PieceHandler;

mod bitfield;
mod msg;
mod piece;

pub struct PeerHandler {
    peer: Peer,
    // (am_choked, am_interested, peer_choked, peer_interested)
    state: SharedRw<State>,
    client_id: Arc<PeerId>,
    info: Arc<Info>,
}

impl PeerHandler {
    pub fn new(
        peer: Peer,
        fh_tx: Sender<file::Cmd>,
        info: Arc<Info>,
        client_id: Arc<PeerId>,
    ) -> (Sender<String>, JoinHandle<()>) {
        let mut handler = Self {
            peer,
            state: SharedRw::new((true, false, true, false)),
            client_id,
            info,
        };

        let (tx, _) = mpsc::channel(40);

        let join_handle = tokio::spawn(async move {
            let mut stream = handler.peer.connect().await.unwrap();
            handler
                .peer
                .handshake(&handler._handshake_payload(), &mut stream)
                .await;

            let (read_half, write_half) = stream.into_split();

            let (msg_tx, sh_handle) = SendHandler::new(write_half);
            let (ph_tx, ph_handle) = PieceHandler::new(
                handler.info.clone(),
                handler.state.clone(),
                fh_tx.clone(),
                msg_tx.clone(),
            );
            let rh_handle = RecvHandler::new(read_half, handler.state.clone(), ph_tx);
            let _ = msg_tx.send(Message::Interested).await;

            join_all(vec![sh_handle, ph_handle, rh_handle]).await;
        });

        (tx, join_handle)
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
}
