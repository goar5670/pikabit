use std::cmp;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp,
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
    time::{self, Duration},
};

use byteorder::{BigEndian, ByteOrder};
use log::{debug, error};

use crate::constants::message_ids::*;

use crate::constants::message_ids;

#[derive(PartialEq, Eq, Debug)]
pub enum Message {
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request(u32, u32, u32),
    Piece(u32, u32, Vec<u8>),
    Empty,
}

impl Message {
    pub async fn send(self, send_handler: &mut SendHandler) {
        match self {
            Message::Request(index, begin, length) => {
                let mut buf: Vec<u8> = vec![];
                buf.write_u32(index).await.unwrap();
                buf.write_u32(begin).await.unwrap();
                buf.write_u32(length).await.unwrap();

                send_handler
                    .send(13, Some(message_ids::REQUEST), Some(&mut buf))
                    .await;
            }
            Message::Interested => {
                send_handler
                    .send(1, Some(message_ids::INTERESTED), None)
                    .await
            }
            _ => {}
        }
    }
}

pub struct RecvHandler {
    read_half: tcp::OwnedReadHalf,
}

impl RecvHandler {
    pub fn new(read_half: tcp::OwnedReadHalf, peer_tx: Sender<Message>) -> JoinHandle<()> {
        let mut handler = Self { read_half };

        tokio::spawn(async move {
            loop {
                let n: u32 = handler._recv_len().await;
                if n == 0 {
                    continue;
                }

                let mut buf: Vec<u8> = vec![0; n as usize];
                if !handler._recv(&mut buf).await {
                    return;
                }

                let msg_id = buf[0];
                debug!(
                    "received msg id: {}, {:?}",
                    msg_id,
                    &buf[1..cmp::min(buf.len(), 12)]
                );

                let msg = match msg_id {
                    CHOKE => Message::Choke,
                    UNCHOKE => Message::Unchoke,
                    HAVE => Message::Have(BigEndian::read_u32(&buf[1..])),
                    BITFIELD => Message::Bitfield(buf[1..].to_vec()),
                    PIECE => Message::Piece(
                        BigEndian::read_u32(&buf[1..5]),
                        BigEndian::read_u32(&buf[5..9]),
                        buf[9..].to_vec(),
                    ),
                    _ => Message::Empty,
                };

                if msg != Message::Empty {
                    let _ = peer_tx.send(msg).await;
                }
            }
        })
    }

    async fn _recv_len(&mut self) -> u32 {
        let mut buf = [0u8; 4];
        let _ = self.read_half.read_exact(&mut buf).await.unwrap();

        BigEndian::read_u32(&buf)
    }

    async fn _recv(&mut self, buf: &mut [u8]) -> bool {
        let n = self.read_half.read_exact(buf).await.unwrap_or(0);
        if n != buf.len() {
            error!(
                "Unexpected behaviour, read {} bytes expected {}",
                n,
                buf.len()
            );
            error!("{:?}", &buf[..20]);
        }

        n == buf.len()
    }
}

pub struct SendHandler {
    write_half: tcp::OwnedWriteHalf,
}

impl SendHandler {
    pub fn new(write_half: tcp::OwnedWriteHalf) -> (Sender<Message>, JoinHandle<()>) {
        let mut handler = Self { write_half };

        let (tx, mut rx): (Sender<Message>, Receiver<Message>) = mpsc::channel(40);
        let join_handle = tokio::spawn(async move {
            loop {
                match time::timeout(Duration::from_secs(2 * 60), rx.recv()).await {
                    Ok(Some(msg)) => msg.send(&mut handler).await,
                    Ok(None) => return,
                    Err(_) => handler.send(0, None, None).await,
                }
            }
        });

        (tx, join_handle)
    }

    async fn send(&mut self, length: u32, message_id: Option<u8>, payload: Option<&mut Vec<u8>>) {
        debug!("send message {} {:?} {:?}", length, message_id, &payload);

        let mut buf: Vec<u8> = vec![];
        buf.write_u32(length).await.unwrap();
        if length != 0 {
            let message_id = message_id.unwrap();
            buf.push(message_id);

            if let Some(p) = payload {
                buf.append(p);
            }
        }

        let _ = self.write_half.write(&buf).await;
    }
}
