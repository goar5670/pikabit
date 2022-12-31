use byteorder::{BigEndian, ByteOrder};
use rand::Rng;
use std::sync::Arc;
use tokio::{io::AsyncWriteExt, net::UdpSocket};

use crate::conc::SharedMut;
use crate::error::{self, Result};

const PROTOCOL_ID: u64 = 4497486125440;

const ACTION_CONNECT: u32 = 0;
const ACTION_ANNOUNCE: u32 = 1;

pub struct UdpTracker {
    url: String,
    socket: SharedMut<UdpSocket>,
}

impl UdpTracker {
    pub async fn new(url: &str, socket: &SharedMut<UdpSocket>) -> Self {
        Self {
            url: url.to_owned(),
            socket: socket.clone(),
        }
    }

    async fn send_recv(&self, buf: &[u8]) -> Result<Vec<u8>> {
        let _ = self.socket.lock().await.send_to(&buf, &self.url).await?;

        let mut buf = [0u8; 512];
        let (n, _) = self.socket.lock().await.recv_from(&mut buf).await?;

        Ok(buf[..n].to_vec())
    }

    async fn connect(&self) -> Result<u64> {
        let mut buf = vec![];
        let tid = rand::thread_rng().gen();

        buf.write_u64(PROTOCOL_ID).await.unwrap();
        buf.write_u32(ACTION_CONNECT).await.unwrap();
        buf.write_u32(tid).await.unwrap();

        let res = self.send_recv(&buf).await?;

        let action = BigEndian::read_u32(&res[..4]);
        let receieved_tid = BigEndian::read_u32(&res[4..8]);

        error::expect_eq(action, ACTION_CONNECT)?;
        error::expect_eq(receieved_tid, tid)?;

        Ok(BigEndian::read_u64(&res[8..]))
    }

    async fn announce(
        &self,
        cid: u64,
        info_hash: &Arc<[u8; 20]>,
        peer_id: &Arc<[u8; 20]>,
        port: u16,
    ) -> Result<Vec<u8>> {
        let tid = rand::thread_rng().gen();
        let mut buf = vec![];
        let _ = buf.write_u64(cid).await;
        let _ = buf.write_u32(ACTION_ANNOUNCE).await;
        let _ = buf.write_u32(tid).await;
        buf.extend(info_hash.iter());
        buf.extend(peer_id.iter());
        let _ = buf.write_u64(0).await; // downloaded
        // todo: send real left
        let _ = buf.write_u64(100).await; // left
        let _ = buf.write_u64(0).await; // uploaded
        let _ = buf.write_u32(2).await; // event
        let _ = buf.write_u32(0).await; // ip address
        let _ = buf.write_u32(0).await; // key
        let _ = buf.write_i32(-1).await; // numwant
        let _ = buf.write_u16(port).await; // port

        assert_eq!(buf.len(), 98);

        let res = self.send_recv(&buf).await?;

        let action = BigEndian::read_u32(&res[..4]);
        error::expect_eq(action, ACTION_ANNOUNCE)?;

        let received_tid = BigEndian::read_u32(&res[4..8]);
        error::expect_eq(received_tid, tid)?;

        Ok(res[20..].to_vec())
    }

    pub async fn get_peers(
        &self,
        info_hash: &Arc<[u8; 20]>,
        peer_id: &Arc<[u8; 20]>,
        port: u16,
    ) -> Result<Vec<[u8; 6]>> {
        let cid = self.connect().await?;
        Ok(super::parse_peers(
            &self.announce(cid, info_hash, peer_id, port).await?,
        ))
    }
}
