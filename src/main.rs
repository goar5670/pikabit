use clap::Parser;
use pikabit::client::Client;

#[derive(Parser)]
struct Args {
    filename: String,
}

#[tokio::main]
async fn main() {
    log4rs::init_file("log_config.yml", Default::default()).unwrap();

    let args = Args::parse();
    let client = Client::new(&args.filename, None);

    client.run().await;
}
