use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;

use crate::consts::{DEFAULT_BOOTSTRAP, MAIN_CHAIN_ID};
use crate::network::Network;

mod consts;
mod network;
mod node;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:8000")]
    listen: SocketAddr,

    #[arg(short, long)]
    root: bool,

    #[arg(short, long, conflicts_with = "root")]
    bootstrap: Vec<SocketAddr>,

    #[arg(short, long, default_value_t = MAIN_CHAIN_ID)]
    chain_id: u8,
}

impl Args {
    fn bootstrap_peers(&self) -> Vec<SocketAddr> {
        if self.root {
            Vec::new()
        } else if self.bootstrap.is_empty() {
            vec![DEFAULT_BOOTSTRAP]
        } else {
            self.bootstrap.clone()
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let network = Arc::new(Network::new(args.listen, args.chain_id));

    let listener = tokio::spawn(network.clone().listen());

    for peer in args.bootstrap_peers() {
        let network = network.clone();

        tokio::spawn(async move {
            if let Err(error) = network.connect_to(peer).await {
                eprintln!("Failed to connect to bootstrap peer {peer}: {error}");
            }
        });
    }

    tokio::spawn(network.clone().discover_peers());

    tokio::select! {
        result = listener => match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("Listener stopped: {error}"),
            Err(error) => eprintln!("Listener task panicked: {error}"),
        },
        _ = tokio::signal::ctrl_c() => println!("Shutting down"),
    }
}
