use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
    sync::mpsc::Receiver,
};

pub struct FileHandler {
    own_rx: Receiver<Cmd>,
    file: File,
}

impl FileHandler {
    pub async fn new(filename: &str, own_rx: Receiver<Cmd>) -> Self {
        Self {
            own_rx,
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open("resources/".to_owned() + filename)
                .await
                .unwrap(),
        }
    }

    pub async fn write_piece(self: &mut Self, offset: u64, piece: &[u8]) {
        self.file.seek(SeekFrom::Start(offset)).await.unwrap();
        self.file.write(piece).await.unwrap();
    }

    pub async fn run(mut self) {
        while let Some(cmd) = self.own_rx.recv().await {
            match cmd {
                Cmd::WritePiece(offset, piece) => self.write_piece(offset, &piece).await,
            }
        }
    }
}

#[derive(Debug)]
pub enum Cmd {
    WritePiece(u64, Vec<u8>),
}
