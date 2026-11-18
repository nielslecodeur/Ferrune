use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use clap::Parser;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::blockchain::core::Blockchain;
use crate::blockchain::state::State;
use crate::blockchain::storage;
use crate::blockchain::transaction::Address;
use crate::blockchain::wallet::Wallet;
use crate::consts::{DEFAULT_BOOTSTRAP, MAIN_CHAIN_ID};
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

    /// Where the wallet and the chain are saved. Defaults to `data/<port>`.
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// Address of the web explorer. Defaults to `127.0.0.1:<port + 1000>`.
    #[arg(short, long)]
    web: Option<SocketAddr>,
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

    fn web(&self) -> SocketAddr {
        self.web.unwrap_or_else(|| {
            SocketAddr::from(([127, 0, 0, 1], self.listen.port().wrapping_add(1000)))
        })
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("data").join(self.listen.port().to_string()))
    }
}

/// Replays the saved blocks, stopping at the first invalid one.
fn load_chain(path: &std::path::Path) -> Blockchain {
    // The mint authority is set by the genesis block.
    let mut blockchain = Blockchain::new(State::new(Address(String::new())));

    let blocks = match storage::load(path) {
        Ok(blocks) => blocks,
        Err(error) => {
            warn!(%error, "Failed to load the chain, starting from scratch");
            return blockchain;
        }
    };

    for block in blocks {
        let height = block.height;

        if let Err(error) = blockchain.add_block(block) {
            warn!(
                height,
                %error,
                "Invalid saved block, ignoring the next ones"
            );
            break;
        }
    }

    blockchain
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let data_dir = args.data_dir();
    let chain_file = data_dir.join("chain.jsonl");

    let wallet = match Wallet::load_or_create(&data_dir.join("wallet.key")) {
        Ok(wallet) => Arc::new(wallet),
        Err(error) => {
            error!(%error, "Failed to load the wallet");
            return;
        }
    };
    info!(address = %wallet.address().0, "Wallet loaded");

    let blockchain = load_chain(&chain_file);
    info!(height = ?blockchain.height(), "Chain loaded");

    let blockchain = Arc::new(RwLock::new(blockchain));
    let network = Arc::new(Network::new(
        args.listen,
        args.chain_id,
        blockchain,
        chain_file,
    ));

    if args.root && network.height().is_none() {
        let result = Blockchain::genesis(&wallet)
            .map_err(|error| format!("{error}"))
            .and_then(|block| network.add_block(block).map_err(|error| format!("{error}")));

        match result {
            Ok(()) => info!("Genesis block created"),
            Err(error) => error!(%error, "Failed to create the genesis block"),
        }
    }

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
    tokio::spawn(network.clone().check_peers());
    tokio::spawn(node::cli::run(network.clone(), wallet.clone()));
    tokio::spawn(node::explorer::run(
        args.web(),
        network.clone(),
        wallet.address(),
    ));
    tokio::spawn(node::producer::run(network.clone(), wallet));

    tokio::select! {
        result = listener => match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => error!(%error, "Listener stopped"),
            Err(error) => error!(%error, "Listener task panicked"),
        },
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down");
            network.disconnect_all().await;
        }
    }
}
