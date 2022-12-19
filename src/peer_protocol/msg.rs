use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp,
    sync::mpsc::{Receiver, Sender},
    time::{self, Duration},
};

use byteorder::{BigEndian, ByteOrder};
use log::{error, info, trace, warn};

use super::piece::Cmd;
use crate::concurrency::SharedRef;
use crate::constants::message_ids::*;
use crate::peer::State;

use crate::constants::message_ids;

pub enum Message {
    Request(Request),
    Interested,
}

impl Message {
    pub async fn send(&self, send_handler: &mut SendHandler) {
        match self {
            Message::Request(request) => request.send(send_handler).await,
            Message::Interested => {
                send_handler
                    .send(1, Some(message_ids::INTERESTED), None)
                    .await
            }
        }
    }
}

pub struct Request {
    index: u32,
    begin: u32,
    length: u32,
}

impl Request {
    pub fn new(index: u32, begin: u32, length: u32) -> Self {
        Self {
            index,
            begin,
            length,
        }
    }
    async fn send(&self, send_handler: &mut SendHandler) {
        let mut buf: Vec<u8> = vec![];
        buf.write_u32(self.index).await.unwrap();
        buf.write_u32(self.begin).await.unwrap();
        buf.write_u32(self.length).await.unwrap();

        send_handler
            .send(13, Some(message_ids::REQUEST), Some(&mut buf))
            .await;
    }
}

pub struct RecvHandler {
    read_half: tcp::OwnedReadHalf,
}

impl RecvHandler {
    pub fn new(read_half: tcp::OwnedReadHalf) -> Self {
        Self { read_half }
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

        return n == buf.len();
    }

    pub async fn run(mut self, peer_state_ref: SharedRef<State>, piece_handler_tx: Sender<Cmd>) {
        loop {
            let n: u32 = self._recv_len().await;
            if n == 0 {
                continue;
            }

            let mut buf: Vec<u8> = vec![0; n as usize];
            if !self._recv(&mut buf).await {
                return;
            }

            let message_id = buf[0];
            match message_id {
                CHOKE => {
                    peer_state_ref.get_handle().await.0 = true;
                    info!("choked");
                }

                UNCHOKE => {
                    peer_state_ref.get_handle().await.0 = false;
                    info!("unchoked");
                }

                HAVE => {
                    let _ = piece_handler_tx
                        .send(Cmd::EnqPiece(BigEndian::read_u32(&buf[1..])))
                        .await;
                }

                BITFIELD => {
                    trace!("Bitfield {:?}", &buf[1..]);
                    let _ = piece_handler_tx
                        .send(Cmd::EnqBitfield(buf[1..].to_vec()))
                        .await;
                }

                PIECE => {
                    let piece_index = BigEndian::read_u32(&buf[1..5]);
                    let block_offset = BigEndian::read_u32(&buf[5..9]);
                    let block = buf[9..].to_vec();
                    trace!("received block {} {}", piece_index, block_offset);
                    let _ = piece_handler_tx
                        .send(Cmd::RecvBlock(piece_index, block_offset, block))
                        .await;
                }

                CANCEL => {
                    warn!("canceled piece");
                }

                _ => {}
            }
        }
    }
}

pub struct SendHandler {
    write_half: tcp::OwnedWriteHalf,
}

impl SendHandler {
    pub fn new(write_half: tcp::OwnedWriteHalf) -> Self {
        Self { write_half }
    }

    async fn send(&mut self, length: u32, message_id: Option<u8>, payload: Option<&mut Vec<u8>>) {
        trace!("send message {} {:?} {:?}", length, message_id, &payload);

        let mut buf: Vec<u8> = vec![];
        buf.write_u32(length).await.unwrap();
        if length != 0 {
            let message_id = message_id.unwrap();
            buf.push(message_id);

            if let Some(p) = payload {
                buf.append(p);
            }
        }

        let _ = self.write_half.write(&mut buf).await;
    }

    pub async fn run(mut self, mut msg_rx: Receiver<Message>) {
        loop {
            match time::timeout(Duration::from_secs(2 * 60), msg_rx.recv()).await {
                Ok(Some(msg)) => msg.send(&mut self).await,
                Ok(None) => return,
                Err(_) => self.send(0, None, None).await,
            }
        }
    }
}