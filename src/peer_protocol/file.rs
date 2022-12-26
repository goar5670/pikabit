use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};

pub struct FileHandler {
    file: File,
}

impl FileHandler {
    pub async fn new(filename: &str) -> (Sender<Cmd>, JoinHandle<()>) {
        let mut handler = Self {
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open("resources/".to_owned() + filename)
                .await
                .unwrap(),
        };
        let (tx, mut rx): (Sender<Cmd>, Receiver<Cmd>) = mpsc::channel(40);

        let join_handle = tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    Cmd::WritePiece(offset, piece) => handler.write_piece(offset, &piece).await,
                }
            }
        });

        (tx, join_handle)
    }

    async fn write_piece(&mut self, offset: u64, piece: &[u8]) {
        self.file.seek(SeekFrom::Start(offset)).await.unwrap();
        self.file.write_all(piece).await.unwrap();
    }
}

#[derive(Debug)]
pub enum Cmd {
    WritePiece(u64, Vec<u8>),
}
