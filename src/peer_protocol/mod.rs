// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use std::sync::Arc;
use tokio::net::TcpStream;
use futures::future::join_all;

use crate::{concurrency::SharedRef, metadata::Metadata};
use crate::peer::*;
use piece::PieceHandler;

mod bitfield;
mod message;
mod piece;

pub struct PeerHandler {
    peer: Peer,
    // (am_choked, am_interested, peer_choked, peer_interested)
    state: SharedRef<State>,
    stream: SharedRef<TcpStream>,
}

impl PeerHandler {
    pub fn new(peer: Peer) -> Self {
        Self {
            peer,
            state: SharedRef::new(Some((true, false, true, false))),
            stream: SharedRef::new(None),
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
        self.stream = SharedRef::new(Some(self.peer.connect().await.unwrap()));
        let handshake_payload = self._handshake_payload(&metadata.get_info_hash(), client_id);
        self.peer.handshake(&handshake_payload, self.stream.clone()).await;

        let piece_handler: Arc<PieceHandler> = Arc::new(PieceHandler::new(&metadata).await);

        let mut handles = vec![];
        message::send(self.stream.clone(), 1, Some(2)).await;
        handles.push(tokio::spawn(message::keep_alive(self.stream.clone())));
        handles.push(tokio::spawn(message::recv_loop(
            self.stream.clone(),
            self.state.clone(),
            Arc::clone(&piece_handler),
        )));
        piece_handler.request_loop(self.stream.clone(), self.state.clone()).await;

        join_all(handles).await;
    }
}
