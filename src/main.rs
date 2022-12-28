use pikabit::{client::Client, constants};

#[tokio::main]
async fn main() {
    log4rs::init_file("log_config.yml", Default::default()).unwrap();

    const TORRENT_FILENAME: &str = constants::torrents::FREE_BSD;

    let client = Client::new(TORRENT_FILENAME, None);

    client.run().await;
}
