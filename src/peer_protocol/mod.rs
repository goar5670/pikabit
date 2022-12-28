// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use futures::future::join_all;
use log::{error, warn};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc::{self, Sender},
    task::JoinHandle,
};

use crate::bitfield::*;
use peer::{Peer, PeerId};

use msg::Message;

pub mod fops;
pub mod msg;
pub mod peer;
pub mod piece;
pub mod requests;

pub async fn spawn_prch(
    mut peer: Peer,
    client_tx: Sender<RelayedMessage>,
    handshake_payload: [u8; 68],
) -> Result<(Arc<PeerId>, Sender<Message>, JoinHandle<()>), String> {
    let mut stream = peer.connect().await?;

    let peer_id = Arc::new(handshake(&handshake_payload, &mut stream).await);
    peer.set_id(peer_id.clone());

    let (read_half, write_half) = stream.into_split();

    let (tx, mut rx) = mpsc::channel(40);

    let (msg_tx, sh_handle) = msg::spawn_sh(write_half);
    let rh_handle = msg::spawn_rh(read_half, tx);

    let join_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let _ = client_tx.send((peer.get_id().unwrap(), msg)).await;
        }

        join_all(vec![sh_handle, rh_handle]).await;
    });

    Ok((peer_id, msg_tx, join_handle))
}

fn verify_handshake(received: &[u8], sent: &[u8]) -> Result<[u8; 20], &'static str> {
    if received[..20] != sent[..20] {
        error!(
            "Incorrect protocol name: {}",
            String::from_utf8(received[..20].to_vec()).unwrap()
        );
        return Err("Incorrect protocol name");
    } else if received[28..48] != sent[28..48] {
        error!(
            "Incorrect info hash: {:?}, expected: {:?}",
            &received[28..48],
            &sent[28..48]
        );
        return Err("Incorrect info hash");
    }

    if received[20..28] != sent[20..28] {
        warn!("Different protocol extension: {:?}", &received[20..28]);
    }

    Ok(received[received.len() - 20..].try_into().unwrap())
}

async fn handshake(payload: &[u8; 68], stream: &mut TcpStream) -> PeerId {
    let _ = stream.write(payload).await;

    let mut buf = [0u8; 68];
    let _ = stream.read_exact(&mut buf).await.unwrap();

    let peer_id = verify_handshake(&buf, payload).unwrap();
    PeerId::from(&peer_id)
}

pub type RelayedMessage = (Arc<PeerId>, Message);

pub struct PeerTracker {
    pub state: peer::State,
    pub msg_tx: Sender<Message>,
    pub have: BitfieldOwned,
}

impl PeerTracker {
    pub fn new(have: BitfieldOwned, msg_tx: Sender<Message>) -> Self {
        Self {
            state: peer::State::new(),
            have,
            msg_tx,
        }
    }
}
