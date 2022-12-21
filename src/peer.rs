use byteorder::{BigEndian, ByteOrder};
use log::{error, info, warn};
use rand::{distributions::Alphanumeric, Rng};
use std::{
    cmp,
    net::{Ipv4Addr, SocketAddr},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

use crate::common;
use crate::constants::{timeouts, CLIENT_PREFIX, CLIENT_VERSION};

#[derive(Debug, PartialEq)]
pub struct PeerId {
    inner: [u8; 20],
}

impl PeerId {
    pub fn new() -> Self {
        let timestamp = common::get_timestamp();

        let cur_len = cmp::min(
            19,
            CLIENT_PREFIX.len() + CLIENT_VERSION.len() + timestamp.len(),
        );

        let rand_filler: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(19 - cur_len)
            .map(char::from)
            .collect();

        let peer_id = CLIENT_PREFIX.to_string() + CLIENT_VERSION + "-" + &rand_filler + &timestamp;

        debug_assert_eq!(peer_id.len(), 20);

        Self {
            inner: peer_id.as_bytes()[..20].try_into().unwrap(),
        }
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.inner
    }
}

impl From<&[u8; 20]> for PeerId {
    fn from(slice: &[u8; 20]) -> Self {
        Self {
            inner: slice.clone(),
        }
    }
}

impl ToString for PeerId {
    fn to_string(self: &Self) -> String {
        String::from_utf8(self.inner.to_vec()).unwrap()
    }
}

#[derive(Debug, PartialEq)]
pub struct Peer {
    address: SocketAddr,
    id: Option<PeerId>,
}

impl Peer {
    pub async fn connect(self: &Self) -> Result<TcpStream, String> {
        timeout(timeouts::PEER_CONNECTION, TcpStream::connect(&self.address))
            .await
            .map_err(|_| format!("connection timed out, {:?}", timeouts::PEER_CONNECTION))
            .unwrap()
            .map_err(|e| format!("{:?}", e))
    }

    fn _verify_handshake(
        self: &Self,
        received: &[u8],
        sent: &[u8],
    ) -> Result<[u8; 20], &'static str> {
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

    pub async fn handshake(self: &mut Self, payload: &[u8; 68], stream: &mut TcpStream) {
        let _ = stream.write(payload).await;

        let mut buf = [0u8; 68];
        let n = stream.read_exact(&mut buf).await.unwrap();

        if n != payload.len() {
            warn!("Unexpected buffer length {}, expected {}", n, payload.len());
        }

        let peer_id = self._verify_handshake(&buf, payload).unwrap();
        info!("peer id: {}", String::from_utf8(peer_id.to_vec()).unwrap());
        self.id = Some(PeerId::from(&peer_id));
    }
}

impl From<[u8; 6]> for Peer {
    fn from(buf: [u8; 6]) -> Self {
        Self {
            address: SocketAddr::from((
                Ipv4Addr::from(BigEndian::read_u32(&buf)),
                BigEndian::read_u16(&buf[4..]),
            )),
            id: None,
        }
    }
}

pub type State = (bool, bool, bool, bool);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_from_slice() {
        let slice: [u8; 6] = [10, 123, 0, 47, 26, 225];
        let peer = Peer::from(slice);

        assert_eq!(
            peer,
            Peer {
                address: SocketAddr::from((Ipv4Addr::new(10, 123, 0, 47), 6881)),
                id: None,
            }
        );
    }
}
