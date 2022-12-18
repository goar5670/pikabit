use byteorder::{BigEndian, ByteOrder};
use log::{error, info};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{sleep, Duration},
};

use super::piece::PieceHandler;
use crate::concurrency::SharedRef;
use crate::constants::message_ids::*;
use crate::peer::State;

pub struct MessageHandler {
    stream_ref: SharedRef<TcpStream>,
}

impl MessageHandler {
    pub fn new(stream_ref: &SharedRef<TcpStream>) -> Self {
        Self {
            stream_ref: stream_ref.clone(),
        }
    }

    async fn _send(&self, length: u32, id: Option<u8>) {
        let mut buf: Vec<u8> = vec![];
        buf.write_u32(length).await.unwrap();
        if length != 0 {
            let message_id = id.unwrap();
            buf.push(message_id);
        }

        let _ = self.stream_ref.get_handle().await.write(&mut buf).await;
    }

    async fn _recv_len(&self) -> u32 {
        let mut buf = [0u8; 4];
        let n = self
            .stream_ref
            .get_handle()
            .await
            .try_read(&mut buf)
            .unwrap_or_default();

        match n {
            0 => 0,
            4 => BigEndian::read_u32(&buf),
            _ => {
                let _ = self
                    .stream_ref
                    .get_handle()
                    .await
                    .read_exact(&mut buf[n..])
                    .await
                    .unwrap();
                BigEndian::read_u32(&buf)
            }
        }
    }

    async fn _recv(&self, buf: &mut [u8]) -> bool {
        let n = self
            .stream_ref
            .get_handle()
            .await
            .read_exact(buf)
            .await
            .unwrap_or(0);
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

    pub async fn send(handler: Arc<Self>, length: u32, id: Option<u8>) {
        handler._send(length, id).await;
    }

    pub async fn recv_loop<'a>(
        self: Arc<Self>,
        peer_state_ref: SharedRef<State>,
        piece_handler: Arc<PieceHandler>,
    ) {
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
            if message_id == CHOKE {
                peer_state_ref.get_handle().await.0 = true;
                info!("choked");
            } else if message_id == UNCHOKE {
                peer_state_ref.get_handle().await.0 = false;
                info!("unchoked");
            } else if message_id == INTERESTED {
            } else if message_id == NOT_INTERESTED {
            } else if message_id == HAVE {
                piece_handler
                    .enqueue_piece(BigEndian::read_u32(&buf[1..]))
                    .await;
            } else if message_id == BITFIELD {
                piece_handler.enqueue_bitfield(&buf[1..]).await;
            } else if message_id == REQUEST {
            } else if message_id == PIECE {
                let piece_index = BigEndian::read_u32(&buf[1..5]);
                let block_offset = BigEndian::read_u32(&buf[5..9]);
                let block = &buf[9..];
                piece_handler
                    .recv_block(piece_index, block_offset, block)
                    .await;
            } else if message_id == CANCEL {
                info!("cancel message received");
            }
        }
    }

    pub async fn keep_alive(self: Arc<Self>) {
        loop {
            self._send(0, None).await;
            sleep(Duration::from_secs(2 * 60)).await;
        }
    }
}
