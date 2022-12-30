use byteorder::{BigEndian, ByteOrder};
use rand::Rng;
use tokio::{io::AsyncWriteExt, net::UdpSocket};

use super::metadata::Info;

const PROTOCOL_ID: u64 = 4497486125440;

const ACTION_CONNECT: u32 = 0;
const ACTION_ANNOUNCE: u32 = 1;

pub struct Tracker {
    url: String,
    socket: UdpSocket,
    transaction_id: u32,
}

impl Tracker {
    pub fn new(socket: UdpSocket, url: String) -> Self {
        let mut rng = rand::thread_rng();

        Self {
            url,
            socket,
            transaction_id: rng.gen(),
        }
    }

    pub async fn connect_request(&self) {
        let mut buf = vec![];

        buf.write_u64(PROTOCOL_ID).await.unwrap();
        buf.write_u32(ACTION_CONNECT).await.unwrap();
        buf.write_u32(self.transaction_id).await.unwrap();

        let res = self.socket.send_to(&buf, &self.url).await;

        println!("{:?} {}", res, self.transaction_id);
    }

    pub async fn connect_response(&mut self) -> u64 {
        let mut buf = [0u8; 16];
        self.socket.recv_from(&mut buf).await.unwrap();

        let tid = BigEndian::read_u32(&buf[4..8]);
        let cid = BigEndian::read_u64(&buf[8..]);
        println!(
            "{} {} {}",
            BigEndian::read_u32(&buf[..4]), tid, cid,
        );

        cid
    }

    pub async fn announce(&mut self, cid: u64, info_hash: &[u8; 20], peer_id: &[u8; 20], left: u64) {
        // let tid = rand::thread_rng().gen();
        let mut buf = vec![];
        let _ = buf.write_u64(cid).await;
        let _ = buf.write_u32(ACTION_ANNOUNCE).await;
        let _ = buf.write_u32(self.transaction_id).await;
        buf.extend(info_hash);
        buf.extend(peer_id);
        let _ = buf.write_u64(0).await; // downloaded
        let _ = buf.write_u64(left).await; // left
        let _ = buf.write_u64(0).await; // uploaded
        let _ = buf.write_u32(2).await; // event
        let _ = buf.write_u32(0).await; // ip address
        let _ = buf.write_u32(55234512).await; // key
        let _ = buf.write_i32(-1).await;
        let _ = buf.write_u16(6881).await;

        assert_eq!(buf.len(), 98);

        let _ = self.socket.send_to(&buf, &self.url).await;

        let mut buf = [0u8; 1024];
        let (n, _) = self.socket.recv_from(&mut buf).await.unwrap();

        println!("{}", n);

        let action = BigEndian::read_u32(&buf[..4]);
        let recieved_tid = BigEndian::read_u32(&buf[4..8]);
        let intervel = BigEndian::read_u32(&buf[8..12]);
        let leechers = BigEndian::read_u32(&buf[12..16]);
        let seeders = BigEndian::read_u32(&buf[16..20]);

        let mut cur: usize = 20;
        let mut peers = vec![];
        while cur < n {
            peers.push(buf[cur..cur+6].to_vec());
            cur += 6;
        }

        println!("{:?}", peers);
    }
}
