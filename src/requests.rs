use std::collections::{VecDeque, HashMap};
use tokio::{sync::mpsc::{self, Receiver, Sender}, task::JoinHandle};

use crate::{peer::PeerId, conc::SharedRw};

struct PieceRequest {
    ph_tx: Sender<String>,
    piece_index: u32,
}

pub struct RequestsQueue {
    queue: VecDeque<PieceRequest>,
    ph_tx_map: SharedRw<HashMap<PeerId, Sender<String>>>,
    piece_owners: HashMap<u32, Vec<PeerId>>,
    cap: u32,
}

impl RequestsQueue {
    pub fn new(ph_tx_map: SharedRw<HashMap<PeerId, Sender<String>>>) -> (Sender<String>, JoinHandle<()>) {
        let mut handler = Self {
            queue: VecDeque::new(),
            ph_tx_map,
            piece_owners: HashMap::new(),
            cap: 40,
        };

        let (tx, mut rx): (Sender<String>, Receiver<String>) = mpsc::channel(40);

        let join_handle = tokio::spawn(async move {
            
        });

        (tx, join_handle)
    }
}
