// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use futures::future::join_all;
use tokio::sync::mpsc;

use crate::peer::*;
use crate::{concurrency::SharedRef, metadata::Metadata};
use piece::PieceHandler;

use msg::{Message, RecvHandler, SendHandler};

mod bitfield;
mod msg;
mod piece;

pub struct PeerHandler {
    peer: Peer,
    // (am_choked, am_interested, peer_choked, peer_interested)
    state: SharedRef<State>,
}

impl PeerHandler {
    pub fn new(peer: Peer) -> Self {
        Self {
            peer,
            state: SharedRef::new((true, false, true, false)),
        }
    }

    fn _handshake_payload(self: &Self, info_hash: &[u8; 20], client_id: &PeerId) -> [u8; 68] {
        let mut payload = [0u8; 68];
        payload[0] = 19;
        payload[1..20].copy_from_slice("BitTorrent protocol".as_bytes());
        payload[20..28].copy_from_slice(&[0u8; 8]);
        payload[28..48].copy_from_slice(info_hash);
        payload[48..].copy_from_slice(client_id.as_bytes());

        payload
    }

    pub async fn run(self: &mut Self, metadata: &Metadata, client_id: &PeerId) {
        let mut stream = self.peer.connect().await.unwrap();
        let handshake_payload = self._handshake_payload(&metadata.get_info_hash(), client_id);
        self.peer.handshake(&handshake_payload, &mut stream).await;

        let (read_half, write_half) = stream.into_split();

        let (msg_tx, msg_rx) = mpsc::channel(40);
        let (ph_tx, ph_rx) = mpsc::channel(40);
        let piece_handler = PieceHandler::new(&metadata, &self.state, msg_tx.clone(), ph_rx).await;
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
