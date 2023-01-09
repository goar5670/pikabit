use anyhow;
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use log::{error, info, warn};
use std::net::SocketAddr;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp,
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
    time::{self, Duration},
};

use crate::peer_protocol::peer::Peer;
use crate::{constants::msg, error::expect_eq};

pub type RelayedMessage = (SocketAddr, Message);

#[derive(PartialEq, Eq, Debug)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request(u32, u32, u32),
    Piece(u32, u32, Vec<u8>),
    Cancel(u32, u32, u32),
}

impl Message {
    pub fn serialize(self) -> anyhow::Result<Vec<u8>> {
        let (length, msg_id, payload) = match self {
            Message::Request(index, begin, length) => {
                let mut buf: Vec<u8> = vec![];
                WriteBytesExt::write_u32::<BigEndian>(&mut buf, index)?;
                WriteBytesExt::write_u32::<BigEndian>(&mut buf, begin)?;
                WriteBytesExt::write_u32::<BigEndian>(&mut buf, length)?;

                (msg::len::REQUEST, Some(msg::id::REQUEST), Some(buf))
            }
            Message::Interested => (msg::len::INTERESTED, Some(msg::id::INTERESTED), None),
            Message::KeepAlive => (0, None, None),
            _ => (0, None, None),
        };
        let mut buf: Vec<u8> = vec![];
        WriteBytesExt::write_u32::<BigEndian>(&mut buf, length)?;
        if let Some(id) = msg_id && length != 0 {
            buf.push(id);

            if let Some(mut p) = payload {
                buf.append(&mut p);
            }
        }

        Ok(buf)
    }

    pub fn deserialize(buf: &[u8]) -> anyhow::Result<Self> {
        let msg_id = buf[4];

        let msg = match msg_id {
            msg::id::CHOKE => Self::Choke,
            msg::id::UNCHOKE => Self::Unchoke,
            msg::id::HAVE => Self::Have(BigEndian::read_u32(&buf[5..])),
            msg::id::BITFIELD => Self::Bitfield(buf[5..].to_vec()),
            msg::id::PIECE => Self::Piece(
                BigEndian::read_u32(&buf[5..9]),
                BigEndian::read_u32(&buf[9..13]),
                buf[13..].to_vec(),
            ),
            msg::id::CANCEL => Self::Cancel(
                BigEndian::read_u32(&buf[5..9]),
                BigEndian::read_u32(&buf[9..13]),
                BigEndian::read_u32(&buf[13..]),
            ),
            _ => Self::KeepAlive,
        };

        Ok(msg)
    }
}

pub async fn recv_msg(read_half: &mut tcp::OwnedReadHalf) -> anyhow::Result<Vec<u8>> {
    let mut buf: Vec<u8> = vec![0; 4];

    let n = read_half.read_exact(&mut buf).await?;
    expect_eq(n, buf.len(), "recv_msg reading length")?;

    let length = BigEndian::read_u32(&buf) as usize;

    if length == 0 {
        return Ok(buf);
    }

    buf.resize(4 + length, 0);

    let n = read_half.read_exact(&mut buf[4..]).await?;
    expect_eq(n, buf.len() - 4, "recv_msg reading rest")?;

    Ok(buf)
}

pub fn spawn_rh(
    mut read_half: tcp::OwnedReadHalf,
    client_tx: Sender<RelayedMessage>,
    peer: Peer,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let buf = match recv_msg(&mut read_half).await {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("recv_msg error {:?}", e);
                    break;
                }
            };

            let msg = match Message::deserialize(&buf) {
                Ok(msg) => msg,
                Err(e) => {
                    error!("message deserialize error {:?}", e);
                    break;
                }
            };

            if msg != Message::KeepAlive {
                let _ = client_tx.send((peer.addr, msg)).await;
            }
        }
        info!("exiting rh task of peer {:?}", peer.id);
    })
}

pub fn spawn_sh(mut write_half: tcp::OwnedWriteHalf) -> (Sender<Message>, JoinHandle<()>) {
    let (tx, mut rx): (Sender<Message>, Receiver<Message>) = mpsc::channel(40);
    let join_handle = tokio::spawn(async move {
        loop {
            let msg = match time::timeout(Duration::from_secs(2 * 60), rx.recv()).await {
                Ok(Some(msg)) => msg,
                Ok(None) => break,
                Err(_) => Message::KeepAlive,
            };

            let buf = match msg.serialize() {
                Ok(b) => b,
                Err(e) => {
                    error!("error serializing message: {:?}", e);
                    continue;
                }
            };
            // todo: handle errors on write
            let _ = write_half.write(&buf).await;
        }
        info!("exiting sh task");
    });

    (tx, join_handle)
}
