use clap::Parser;
use pikabit::client::Client;
use std::fs::File;

#[derive(Parser)]
struct Args {
    filename: String,
    // Listening port
    #[arg(short, long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() {
    File::create("log/.log").unwrap();
    log4rs::init_file("log_config.yml", Default::default()).unwrap();

    let args = Args::parse();
    let client = Client::new(&args.filename, args.port);

    client.run().await;
}
