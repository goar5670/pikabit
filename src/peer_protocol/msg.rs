use std::io::{self, ErrorKind};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp,
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
    time::{self, Duration},
};

use byteorder::{BigEndian, ByteOrder};
use log::{error, info, trace, warn};

use crate::constants::msg_ids;

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
    Empty,
}

impl Message {
    pub async fn send(self, write_half: &mut tcp::OwnedWriteHalf) {
        let (length, msg_id, payload) = match self {
            Message::Request(index, begin, length) => {
                let mut buf: Vec<u8> = vec![];
                buf.write_u32(index).await.unwrap();
                buf.write_u32(begin).await.unwrap();
                buf.write_u32(length).await.unwrap();

                (13, Some(msg_ids::REQUEST), Some(buf))
            }
            Message::Interested => (1, Some(msg_ids::INTERESTED), None),
            Message::KeepAlive => (0, None, None),
            _ => (0, None, None),
        };

        let mut buf: Vec<u8> = vec![];
        buf.write_u32(length).await.unwrap();
        if !msg_id.is_none() && length != 0 {
            debug_assert!(msg_id.unwrap() > msg_ids::CHOKE && msg_id.unwrap() <= msg_ids::CANCEL);
            buf.push(msg_id.unwrap());

            if let Some(mut p) = payload {
                debug_assert_eq!(p.len() as u32, length - 1);
                buf.append(&mut p);
            }
        }

        let _ = write_half.write(&buf).await;
    }
}

pub fn spawn_rh(mut read_half: tcp::OwnedReadHalf, peer_tx: Sender<Message>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let res = recv_len(&mut read_half).await;

            if res
                .as_ref()
                .is_err_and(|e| e.kind() == ErrorKind::UnexpectedEof)
            {
                break;
            } else if res.is_err() {
                warn!("{:?}", res);
                continue;
            }

            let n = res.unwrap();

            if n == 0 {
                continue;
            }

            let mut buf: Vec<u8> = vec![0; n as usize];
            if !recv(&mut read_half, &mut buf).await {
                return;
            }

            let msg_id = buf[0];
            if msg_id != msg_ids::PIECE {
                info!("received msg id: {}, {:?}", msg_id, &buf);
            } else {
                trace!(
                    "received block: {} {}",
                    BigEndian::read_u32(&buf[1..5]),
                    BigEndian::read_u32(&buf[5..9])
                );
            }

            // todo: deserialize
            let msg = match msg_id {
                msg_ids::CHOKE => Message::Choke,
                msg_ids::UNCHOKE => Message::Unchoke,
                msg_ids::HAVE => Message::Have(BigEndian::read_u32(&buf[1..])),
                msg_ids::BITFIELD => Message::Bitfield(buf[1..].to_vec()),
                msg_ids::PIECE => Message::Piece(
                    BigEndian::read_u32(&buf[1..5]),
                    BigEndian::read_u32(&buf[5..9]),
                    buf[9..].to_vec(),
                ),
                msg_ids::CANCEL => Message::Cancel(
                    BigEndian::read_u32(&buf[1..5]),
                    BigEndian::read_u32(&buf[5..9]),
                    BigEndian::read_u32(&buf[9..]),
                ),
                _ => Message::Empty,
            };

            if msg != Message::Empty {
                let _ = peer_tx.send(msg).await;
            }
        }
    })
}

async fn recv_len(read_half: &mut tcp::OwnedReadHalf) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    let _ = read_half.read_exact(&mut buf).await?;

    Ok(BigEndian::read_u32(&buf))
}

async fn recv(read_half: &mut tcp::OwnedReadHalf, buf: &mut [u8]) -> bool {
    let n = read_half.read_exact(buf).await.unwrap_or(0);
    if n != buf.len() {
        error!(
            "Unexpected behaviour, read {} bytes expected {}",
            n,
            buf.len()
        );
        error!("{:?}", &buf);
    }

    n == buf.len()
}

pub fn spawn_sh(mut write_half: tcp::OwnedWriteHalf) -> (Sender<Message>, JoinHandle<()>) {
    let (tx, mut rx): (Sender<Message>, Receiver<Message>) = mpsc::channel(40);
    let join_handle = tokio::spawn(async move {
        loop {
            match time::timeout(Duration::from_secs(2 * 60), rx.recv()).await {
                Ok(Some(msg)) => msg.send(&mut write_half).await,
                Ok(None) => return,
                Err(_) => Message::KeepAlive.send(&mut write_half).await,
            }
        }
    });

    (tx, join_handle)
}
