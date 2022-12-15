use pikabit::{client::Client, constants};

#[tokio::main]
async fn main() {
    const TORRENT_FILENAME: &'static str = constants::torrents::FREE_BSD;

    let mut client = Client::new(TORRENT_FILENAME, None);

    let _ = client.run().await;
}
