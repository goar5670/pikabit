use byteorder::{BigEndian, ByteOrder};
use log::{error, info, warn};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{sleep, Duration},
};

use super::piece::PieceHandler;
use super::shared_data::SharedRef;
use super::Peer;
use crate::constants::message_ids::*;

async fn _send(stream_ref: &SharedRef<TcpStream>, length: u32, id: Option<u8>) {
    let mut buf: Vec<u8> = vec![];
    buf.write_u32(length).await.unwrap();
    if length != 0 {
        let message_id = id.unwrap();
        buf.push(message_id);
    }

    let _ = stream_ref.get_handle().await.write(&mut buf).await;
    // println!("sent message, id {:?}, buffer: {:?}", id, buf);
}

async fn _recv_len(stream_ref: &SharedRef<TcpStream>) -> u32 {
    let mut buf = [0u8; 4];
    let n = stream_ref
        .get_handle()
        .await
        .try_read(&mut buf)
        .unwrap_or_default();

    match n {
        0 => 0,
        4 => BigEndian::read_u32(&buf),
        _ => {
            let _ = stream_ref
                .get_handle()
                .await
                .read_exact(&mut buf[n..])
                .await
                .unwrap();
            BigEndian::read_u32(&buf)
        }
    }
}

async fn _recv(stream_ref: &SharedRef<TcpStream>, buf: &mut [u8]) -> bool {
    let n = stream_ref
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

pub async fn send(stream_ref: SharedRef<TcpStream>, length: u32, id: Option<u8>) {
    _send(&stream_ref, length, id).await;
}

pub async fn recv_loop<'a>(
    stream_ref: SharedRef<TcpStream>,
    peer_ref: SharedRef<Peer>,
    piece_handler: Arc<PieceHandler>,
) {
    loop {
        let n: u32 = _recv_len(&stream_ref).await;
        if n == 0 {
            continue;
        }

        let mut buf: Vec<u8> = vec![0; n as usize];
        if !_recv(&stream_ref, &mut buf).await {
            return;
        }

        let message_id = buf[0];
        if message_id == CHOKE {
            peer_ref.get_handle().await.choke_me();
            println!("choked");
        } else if message_id == UNCHOKE {
            peer_ref.get_handle().await.unchoke_me();
            println!("unchoked");
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
            println!("cancel message received");
        }
    }
}

pub async fn keep_alive(stream_ref: SharedRef<TcpStream>) {
    loop {
        _send(&stream_ref, 0, None).await;
        sleep(Duration::from_secs(2 * 60)).await;
    }
}
