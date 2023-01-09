// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use anyhow;
use log::{info, warn};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc::Sender, Semaphore},
    task::JoinHandle,
};

use crate::{bitfield::*, error::expect_eq};
use peer::{Peer, PeerId};

use msg::{Message, RelayedMessage};

pub mod fops;
pub mod msg;
pub mod peer;
pub mod piece;
pub mod requests;

pub async fn spawn_prch(
    mut peer: Peer,
    client_tx: Sender<RelayedMessage>,
    handshake_payload: [u8; 68],
) -> anyhow::Result<(Arc<PeerId>, Sender<Message>, JoinHandle<()>)> {
    let mut stream = peer.connect().await?;

    let peer_id = Arc::new(handshake(&handshake_payload, &mut stream).await?);
    peer.id = Some(peer_id.clone());

    let (read_half, write_half) = stream.into_split();

    let (msg_tx, sh_handle) = msg::spawn_sh(write_half);
    let rh_handle = msg::spawn_rh(read_half, client_tx, peer);

    let join_handle = tokio::spawn(async {
        let res = tokio::select! {
            s = sh_handle => s,
            r = rh_handle => r,
        };
        info!("existed peer connection task with {:?}", res);
    });

    Ok((peer_id, msg_tx, join_handle))
}

fn verify_handshake(received: &[u8], sent: &[u8]) -> anyhow::Result<[u8; 20]> {
    expect_eq(
        &received[..20],
        &sent[..20],
        "verify_handshake protocol name",
    )?;
    expect_eq(
        &received[28..48],
        &sent[28..48],
        "verify_handshake info hash",
    )?;

    if received[20..28] != sent[20..28] {
        warn!("Different protocol extension: {:?}", &received[20..28]);
    }

    Ok(received[received.len() - 20..].try_into()?)
}

async fn handshake(payload: &[u8; 68], stream: &mut TcpStream) -> anyhow::Result<PeerId> {
    stream.write(payload).await?;

    let mut buf = [0u8; 68];
    stream.read_exact(&mut buf).await?;

    let peer_id = verify_handshake(&buf, payload)?;
    Ok(PeerId::from(&peer_id))
}

pub struct PeerTracker {
    pub id: Arc<PeerId>,
    pub state: peer::State,
    pub msg_tx: Sender<Message>,
    pub have: BitfieldOwned,
    pub reqs_sem: Arc<Semaphore>,
}

impl PeerTracker {
    pub fn new(peer_id: Arc<PeerId>, have: BitfieldOwned, msg_tx: Sender<Message>) -> Self {
        Self {
            id: peer_id,
            state: peer::State::new(),
            have,
            msg_tx,
            reqs_sem: Arc::new(Semaphore::new(0)),
        }
    }
}
