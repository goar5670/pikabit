use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
};

pub struct FileHandler {
    file: File,
}

impl FileHandler {
    pub async fn new(filename: &str) -> Self {
        Self {
            file: OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(filename)
                .await
                .unwrap(),
        }
    }

    pub async fn write_piece(self: &mut Self, offset: u64, piece: &[u8]) {
        self.file.seek(SeekFrom::Start(offset)).await.unwrap();
        self.file.write(piece).await.unwrap();
    }
}
