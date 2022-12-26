use pikabit::{client::Client, constants};

#[tokio::main]
async fn main() {
    env_logger::init();

    const TORRENT_FILENAME: &str = constants::torrents::FREE_BSD;

    let client = Client::new(TORRENT_FILENAME, None);

    client.run().await;
}
