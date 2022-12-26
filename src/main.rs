use pikabit::{client::Client, constants};

#[tokio::main]
async fn main() {
    env_logger::init();

    const TORRENT_FILENAME: &'static str = constants::torrents::FREE_BSD;

    let client = Client::new(TORRENT_FILENAME, None);

    let _ = client.run().await;
}
