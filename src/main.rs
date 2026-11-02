use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::blockchain::core::Blockchain;
use crate::blockchain::state::State;
use crate::blockchain::transaction::Address;
use crate::consts::{DEFAULT_BOOTSTRAP, MAIN_CHAIN_ID, MINT_AUTHORITY};
use crate::network::Network;

mod blockchain;
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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let blockchain = Arc::new(RwLock::new(Blockchain::new(State::new(Address(
        MINT_AUTHORITY.into(),
    )))));
    let network = Arc::new(Network::new(args.listen, args.chain_id, blockchain));

    let listener = tokio::spawn(network.clone().listen());

    for peer in args.bootstrap_peers() {
        let network = network.clone();

        tokio::spawn(async move {
            if let Err(error) = network.connect_to(peer).await {
                error!(%peer, %error, "Failed to connect to bootstrap peer");
            }
        });
    }

    tokio::spawn(network.clone().discover_peers());

    tokio::select! {
        result = listener => match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => error!(%error, "Listener stopped"),
            Err(error) => error!(%error, "Listener task panicked"),
        },
        _ = tokio::signal::ctrl_c() => info!("Shutting down"),
    }
}
