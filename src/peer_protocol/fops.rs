use futures::future::join_all;
use std::cmp;
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom},
};

use crate::tracker_protocol::metadata::Info;

async fn new_file(base_dir: &str, path: &Vec<String>) -> File {
    let filename = path.last().unwrap();
    let dir = &path[..path.len() as usize - 1];

    let mut dir_path = "resources/".to_owned() + base_dir + "/";
    if !dir.is_empty() {
        dir_path = dir_path + &dir.join("/") + "/";
    }

    let _ = std::fs::create_dir_all(&dir_path);

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(dir_path + filename)
        .await
        .unwrap()
}

struct FileTracker {
    inner: File,
    length: u64,
}

pub struct FileManager {
    files: Vec<FileTracker>,
}

impl FileManager {
    pub async fn new(info: &Info) -> Self {
        if let Some(files) = info.files.as_ref() {
            return Self {
                files: join_all(files.iter().map(|f| async {
                    FileTracker {
                        inner: new_file(&info.name, &f.path).await,
                        length: f.length,
                    }
                }))
                .await,
            };
        } else {
            return Self {
                files: vec![FileTracker {
                    inner: new_file(".", &vec![info.name.clone()]).await,
                    length: info.length.unwrap(),
                }],
            };
        }
    }

    fn total_len(&self) -> u64 {
        self.files.iter().fold(0, |acc, f| acc + f.length)
    }

    fn get_relative_offset(&self, offset: u64) -> (usize, u64) {
        let mut cur_offset = 0;
        for (i, ft) in self.files.iter().enumerate() {
            if cur_offset + ft.length > offset {
                return (i, offset - cur_offset);
            } else {
                cur_offset += ft.length;
            }
        }
        (
            self.files.len() - 1,
            offset - self.total_len() + self.files.last().unwrap().length,
        )
    }

    fn get_length_to_add(&self, file_index: usize, offset: u64, length: u32) -> u32 {
        let rem = self.files.get(file_index).unwrap().length - offset;
        cmp::min(rem, length as u64) as u32
    }

    async fn write_to_file(&mut self, file_index: usize, relative_offset: u64, bytes: &[u8]) {
        let file = &mut self.files.get_mut(file_index).unwrap().inner;
        file.seek(SeekFrom::Start(relative_offset)).await.unwrap();
        let _ = file.write_all(bytes).await;
    }

    async fn read_from_file(
        &mut self,
        file_index: usize,
        relative_offset: u64,
        length: u32,
    ) -> Vec<u8> {
        let file = &mut self.files.get_mut(file_index).unwrap().inner;
        file.seek(SeekFrom::Start(relative_offset)).await.unwrap();
        let mut buf: Vec<u8> = vec![0; length as usize];
        let _ = file.read_exact(&mut buf).await;
        buf
    }

    pub async fn write_piece(&mut self, offset: u64, bytes: &Vec<u8>) {
        let piece_length = bytes.len() as u32;
        let mut bytes_written = 0;
        let (mut cur_file_index, mut cur_offset) = self.get_relative_offset(offset);
        while bytes_written < piece_length {
            let length =
                self.get_length_to_add(cur_file_index, cur_offset, piece_length - bytes_written);
            let (s, e) = (bytes_written as usize, (bytes_written + length) as usize);
            self.write_to_file(cur_file_index, cur_offset, &bytes[s..e])
                .await;
            bytes_written += length;
            cur_file_index += 1;
            cur_offset = 0;
        }
    }

    pub async fn read_block(&mut self, offset: u64, length: u32) -> Vec<u8> {
        let (mut cur_file_index, mut cur_offset) = self.get_relative_offset(offset);
        let mut bytes_read = 0;
        let mut buf = vec![0; length as usize];
        while bytes_read < length {
            let l = self.get_length_to_add(cur_file_index, cur_offset, length - bytes_read);
            let mut bytes = self.read_from_file(cur_file_index, cur_offset, l).await;
            buf.append(&mut bytes);
            bytes_read += l;
            cur_file_index += 1;
            cur_offset = 0;
        }
        buf
    }
}
