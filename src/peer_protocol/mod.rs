// todo: add uTP (bep 29) | priority: low
// todo: implement block pipelining (from bep 3) | priority: low

use log::{error, info, warn};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc::Sender, Semaphore},
    task::JoinHandle,
};

use crate::{bitfield::*, error};
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
) -> Result<(Arc<PeerId>, Sender<Message>, JoinHandle<()>), String> {
    let mut stream = peer.connect().await?;

    let peer_id = Arc::new(
        handshake(&handshake_payload, &mut stream)
            .await
            .map_err(|e| format!("{:?}", e))?,
    );
    peer.id = Some(peer_id.clone());

    let (read_half, write_half) = stream.into_split();

    let (msg_tx, sh_handle) = msg::spawn_sh(write_half);
    let rh_handle = msg::spawn_rh(read_half, client_tx, peer.addr);

    let join_handle = tokio::spawn(async {
        let res = tokio::select! {
            s = sh_handle => s,
            r = rh_handle => r,
        };
        info!("existed peer connection task with {:?}", res);
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

async fn handshake(payload: &[u8; 68], stream: &mut TcpStream) -> error::Result<PeerId> {
    stream.write(payload).await?;

    let mut buf = [0u8; 68];
    stream.read_exact(&mut buf).await?;

    let peer_id = verify_handshake(&buf, payload).unwrap();
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
