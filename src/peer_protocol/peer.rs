use rand::{distributions::Alphanumeric, Rng};
use std::{cmp, fmt::Debug, net::SocketAddr, sync::Arc};
use tokio::{net::TcpStream, time::timeout};

use crate::common;
use crate::constants::{timeouts, CLIENT_PREFIX, CLIENT_VERSION};

#[derive(PartialEq, Eq, Hash)]
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
        Self { inner: *slice }
    }
}

impl Debug for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id_str = String::from_utf8(self.inner.to_vec())
            .unwrap_or(String::from_utf8(PeerId::new().inner.to_vec()).unwrap());
        id_str.fmt(f)
    }
}

#[derive(Debug, PartialEq)]
pub struct Peer {
    pub addr: SocketAddr,
    pub id: Option<Arc<PeerId>>,
}

impl Peer {
    pub async fn connect(&self) -> Result<TcpStream, String> {
        match timeout(timeouts::PEER_CONNECTION, TcpStream::connect(&self.addr)).await {
            Ok(r) => r.map_err(|e| format!("{e:?}")),
            Err(_) => Err(format!(
                "connection timed out, {:?}",
                timeouts::PEER_CONNECTION
            )),
        }
    }
}

impl From<SocketAddr> for Peer {
    fn from(addr: SocketAddr) -> Self {
        Self { addr, id: None }
    }
}

pub struct State {
    pub am_choked: bool,
    pub am_interested: bool,
    pub peer_choked: bool,
    pub peer_interested: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            am_choked: true,
            am_interested: false,
            peer_choked: true,
            peer_interested: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::addr_from_buf;
    use std::net::Ipv4Addr;

    #[test]
    fn peer_from_slice() {
        let slice: [u8; 6] = [10, 123, 0, 47, 26, 225];
        let peer = Peer::from(addr_from_buf(&slice));

        assert_eq!(
            peer,
            Peer {
                addr: SocketAddr::from((Ipv4Addr::new(10, 123, 0, 47), 6881)),
                id: None,
            }
        );
    }
}
