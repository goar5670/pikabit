use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
};

pub async fn new_file(filename: &str) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("resources/".to_owned() + filename)
        .await
        .unwrap()
}

pub async fn write_piece(file: &mut File, offset: u64, piece: &Vec<u8>) {
    file.seek(SeekFrom::Start(offset)).await.unwrap();
    let _ = file.write_all(piece).await;
}

#[derive(Debug)]
pub enum Cmd {
    WritePiece(u64, Vec<u8>),
}
