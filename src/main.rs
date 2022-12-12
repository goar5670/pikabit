use pikabit::{
  client::Client,
  constants
};


fn main() {
  const TORRENT_FILENAME: &'static str = constants::torrents::UBUNTU22;
  
  let mut client = Client::new(TORRENT_FILENAME, None);

  let _ = client.run();
}
