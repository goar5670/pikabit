use byteorder::{BigEndian, ByteOrder};
use log::{error, info};
use rand::Rng;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    io::AsyncWriteExt,
    net::UdpSocket,
    sync::mpsc::{Receiver, Sender},
    task::JoinHandle,
    time::{self, Duration},
};

use crate::{
    common::addr_from_buf,
    conc::{self, SharedMut},
    error::{self, Result},
};

const PROTOCOL_ID: u64 = 4497486125440;

const ACTION_CONNECT: u32 = 0;
const ACTION_ANNOUNCE: u32 = 1;

#[derive(Debug)]
pub struct UdpTracker {
    pub addr: SocketAddr,
    socket: SharedMut<UdpSocket>,
    rx: Receiver<Vec<u8>>,
}

impl UdpTracker {
    pub fn new(addr: SocketAddr, socket: SharedMut<UdpSocket>, rx: Receiver<Vec<u8>>) -> Self {
        Self { addr, socket, rx }
    }

    async fn send_recv(&mut self, buf: &[u8]) -> Result<Vec<u8>> {
        let _ = conc::timeout(5, self.socket.lock().await.send_to(&buf, &self.addr)).await?;
        conc::timeout(5, self.rx.recv()).await
    }

    async fn connect(&mut self) -> Result<u64> {
        let mut buf = vec![];
        let tid = rand::thread_rng().gen();

        buf.write_u64(PROTOCOL_ID).await.unwrap();
        buf.write_u32(ACTION_CONNECT).await.unwrap();
        buf.write_u32(tid).await.unwrap();

        let res = self.send_recv(&buf).await?;

        let action = BigEndian::read_u32(&res[..4]);
        let receieved_tid = BigEndian::read_u32(&res[4..8]);

        error::expect_eq(action, ACTION_CONNECT, "connect action")?;
        error::expect_eq(receieved_tid, tid, "connect tid")?;

        Ok(BigEndian::read_u64(&res[8..]))
    }

    async fn announce(
        &mut self,
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
        let _ = buf.write_u64(100).await; // left // todo: send real left
        let _ = buf.write_u64(0).await; // uploaded
        let _ = buf.write_u32(2).await; // event
        let _ = buf.write_u32(0).await; // ip address
        let _ = buf.write_u32(0).await; // key
        let _ = buf.write_i32(-1).await; // numwant
        let _ = buf.write_u16(port).await; // port

        assert_eq!(buf.len(), 98);

        let res = self.send_recv(&buf).await?;

        let action = BigEndian::read_u32(&res[..4]);
        error::expect_eq(action, ACTION_ANNOUNCE, "announce action")?;

        let received_tid = BigEndian::read_u32(&res[4..8]);
        error::expect_eq(received_tid, tid, "announce tid")?;

        Ok(res[20..].to_vec())
    }

    pub async fn get_peers(
        &mut self,
        info_hash: &Arc<[u8; 20]>,
        peer_id: &Arc<[u8; 20]>,
        port: u16,
    ) -> Result<Vec<SocketAddr>> {
        let cid = self.connect().await?;
        Ok(
            super::parse_peers(&self.announce(cid, info_hash, peer_id, port).await?)
                .iter()
                .map(|buf| addr_from_buf(buf))
                .collect(),
        )
    }
}

pub fn spawn_udp_rh(
    tracker_map: HashMap<SocketAddr, Sender<Vec<u8>>>,
    socket: SharedMut<UdpSocket>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        loop {
            let res = socket.lock().await.try_recv_from(&mut buf);
            match res {
                Ok((n, addr)) => {
                    if let Some(tx) = tracker_map.get(&addr) {
                        let _ = tx.send(buf[..n].to_vec()).await;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    time::sleep(Duration::from_millis(100)).await
                }
                Err(e) => {
                    error!("udp_rh error {:?}", e);
                    break;
                }
            }
        }
        info!("exiting udp_rh task");
    })
}
