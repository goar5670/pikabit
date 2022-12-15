use super::*;
use super::bitfield::*;
use crate::constants::message_ids::*;

async fn _send(stream_ref: &SharedRef<TcpStream>, length: u32, id: Option<u8>) {
    let mut buf: Vec<u8> = vec![];
    buf.write_u32(length).await.unwrap();
    if length != 0 {
        let message_id = id.unwrap();
        buf.push(message_id);
    }

    let _ = stream_ref.get_handle().await.write(&mut buf).await;
    println!("sent message, id {:?}, buffer: {:?}", id, buf);
}

async fn _recv_len(stream_ref: &SharedRef<TcpStream>) -> u32 {
    let mut buf = [0u8; 4];
    let _ = stream_ref.get_handle().await.read_exact(&mut buf).await;
    BigEndian::read_u32(&buf)
}

async fn _recv(
    stream_ref: &SharedRef<TcpStream>,
    buf: &mut [u8],
    n: u32,
) -> Result<(), std::io::Error> {
    let _ = stream_ref.get_handle().await.read_exact(buf).await;

    if buf[0] != PIECE {
        println!("recieved message, len: {} {:?}", n, buf.to_vec());
    } else {
        println!(
            "received piece, len: {}, message_id: {}, index: {}, offset: {}",
            n,
            buf[0],
            BigEndian::read_u32(&buf[1..]),
            BigEndian::read_u32(&buf[5..]),
        )
    }

    Ok(())
}

pub async fn send(
    stream_ref: SharedRef<TcpStream>,
    length: u32,
    id: Option<u8>,
    // payload: Option<&mut Vec<u8>>,
) {
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
        let _ = _recv(&stream_ref, &mut buf, n).await.unwrap();

        let message_id = buf[0];
        if message_id == UNCHOKE {
            peer_ref.get_handle().await.unchoke_me();
        } else if message_id == HAVE {
            piece_handler
                .queue_piece(BigEndian::read_u32(&buf[1..]))
                .await;
        } else if message_id == BITFIELD {
            let piece_handler = Arc::clone(&piece_handler);
            let bitfield = Bitfield::from((&buf[1..], piece_handler.info.num_pieces()));
            tokio::spawn(async move {
                for i in 0..bitfield.len() {
                    if bitfield.get(i) == true {
                        piece_handler.queue_piece(i).await;
                        sleep(Duration::from_millis(2)).await;
                    }
                }
            });
        } else if message_id == PIECE {
            *piece_handler.num_requests.get_handle().await -= 1;
            // let piece_index = BigEndian::read_u32(&buf[1..5]);
            // let piece_bitfield = piece_handler.downloaded_blocks.get_mut(piece_index);
        }

        sleep(Duration::from_millis(1)).await;
    }
}

pub async fn keep_alive(stream_ref: SharedRef<TcpStream>) {
    loop {
        _send(&stream_ref, 0, None).await;
        sleep(Duration::from_secs(60)).await;
    }
}

#[derive(Debug)]
struct BlockRequest(u32, u32, u32);

impl BlockRequest {
    pub async fn send(self: &Self, handler: &SharedRef<TcpStream>) {
        let mut buf: Vec<u8> = vec![];
        buf.write_u32(13).await.unwrap();
        buf.push(6);
        buf.write_u32(self.0).await.unwrap();
        buf.write_u32(self.1).await.unwrap();
        buf.write_u32(self.2).await.unwrap();

        let _ = handler.get_handle().await.write(&mut buf).await;
        println!("sent request piece message, {:?}", self);
    }
}

pub struct PieceHandler {
    requests_queue: SharedRef<VecDeque<BlockRequest>>,
    info: Arc<Info>,
    downloaded_pieces: Bitfield,
    // downloaded_blocks: Vec<Bitfield>,
    // requested_pieces: Bitfield,
    num_requests: SharedRef<u32>,
    requests_cap: u32,
    block_size: u32,
}

impl PieceHandler {
    pub fn new(metadata: &Metadata) -> Self {
        let info = Arc::clone(&metadata.info);
        let num_pieces = info.num_pieces();
        let block_size = 16 * 1024;
        // let num_blocks = info.num_blocks(0, block_size);
        println!("number of pieces: {}", num_pieces);
        Self {
            requests_queue: SharedRef::new(Some(VecDeque::new())),
            info,
            downloaded_pieces: Bitfield::new(num_pieces),
            // downloaded_blocks: vec![Bitfield::new(num_blocks); num_pieces as usize],
            // requested_pieces: Bitfield::new(num_pieces),
            num_requests: SharedRef::new(Some(0)),
            requests_cap: 10,
            block_size,
        }
    }

    async fn _full(self: &Self) -> bool {
        return *self.num_requests.get_handle().await > self.requests_cap
    }

    pub async fn queue_piece(self: &Self, piece_index: u32) {
        // println!("{}", piece_index);
        let mut cur_offset: u32 = 0;
        let piece_len = self.info.piece_len(piece_index);
        // println!("piece length: {}", piece_len);
        while cur_offset != piece_len {
            let block_size = cmp::min(self.block_size, piece_len - cur_offset);
            let request = BlockRequest(piece_index, cur_offset, block_size);
            self.requests_queue.get_handle().await.push_back(request);
            cur_offset += block_size;
        }
    }

    pub async fn request_loop(self: &Self, stream_ref: SharedRef<TcpStream>, peer_ref: SharedRef<Peer>) {
        while self.downloaded_pieces.rem() > 0 {
            if self._full().await || peer_ref.get_handle().await.state.client_choked {
                // println!("{}", cnt);
                // println!("{}", peer_ref.get_handle().await.state.client_choked);
                sleep(Duration::from_millis(2)).await;
            } else {
                let mut q = self.requests_queue.get_handle().await;
                if q.len() > 0 {
                    q.front().unwrap().send(&stream_ref).await;
                    q.pop_front();
                    *self.num_requests.get_handle().await += 1;
                }
            }
        }
    }
}
