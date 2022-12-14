use serde_bencode;
use std::{fs, error::Error, string::ToString};

use crate::metadata::Metadata;
use crate::peer_protocol::{Peer, PeerHandler, PeerId};
use crate::tracker_protocol::http::{Event, Request, Response};

pub struct Client {
    peer_id: PeerId,
    torrent: Metadata,
    port: u16,
}

impl Client {
    pub fn new(filename: &str, port: Option<u16>) -> Self {
        let file: Vec<u8> = fs::read(filename).unwrap();
        let torrent: Metadata = serde_bencode::from_bytes(&file).unwrap();

        let default_port = 6881;

        Self {
            peer_id: PeerId::new(),
            torrent,
            port: match port {
                Some(p) => p,
                None => default_port,
            },
        }
    }

    fn tracker_start_request(self: &Self) -> Request {
        Request::new(
            self.torrent.get_tracker_url(),
            self.torrent.get_info_hash(),
            self.peer_id.to_string(),
            self.port,
            0,
            0,
            1024,
            1,
            0,
            Some(Event::Started),
            None,
            Some(50),
            None,
            None,
        )
    }

    // todo: implement stop, resume functionality | priority: high

    // todo: implement non compact response format | priority: high

    async fn request_peers(self: &mut Self) -> Result<Vec<[u8; 6]>, Box<dyn Error>> {
        let started_request: Request = self.tracker_start_request();
        let mut iters = 0;
        // todo: move this to config
        let max_iters = 100;

        println!("Fetching peers from tracker...");

        while iters < max_iters {
            let response: Response =
                serde_bencode::from_bytes(&started_request.get().await).unwrap();
            let peers = response.get_peers();

            if peers.len() != 0 {
                return Ok(peers);
            }
            iters += 1;
        }

        panic!("Couldn't get peers from the tracker");
    }

    pub async fn run(self: &mut Self) {
        let peers = self.request_peers().await.unwrap();
        let mut peer_handler = PeerHandler::new(Peer::from(peers[0]));

        peer_handler.run(&self.torrent, &self.peer_id).await;
    }
}
