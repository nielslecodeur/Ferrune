use std::net::SocketAddr;
use std::sync::Arc;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

use crate::blockchain::block::Block;
use crate::blockchain::core::Blockchain;
use crate::blockchain::transaction::{Address, SignedTransaction, Transaction};
use crate::network::Network;

const PAGE: &str = include_str!("explorer.html");

/// Everything the explorer page shows, built from the node's current state.
#[derive(Serialize)]
struct Snapshot {
    address: String,
    listen_addr: SocketAddr,
    chain_id: u8,
    height: Option<u64>,
    proposer: Option<String>,
    mint_authority: String,
    peers: Vec<SocketAddr>,
    blocks: Vec<BlockView>,
    accounts: Vec<AccountView>,
    mempool: Vec<TransactionView>,
}

#[derive(Serialize)]
struct BlockView {
    height: u64,
    hash: String,
    previous_hash: String,
    proposer: String,
    transactions: Vec<TransactionView>,
}

#[derive(Serialize)]
struct TransactionView {
    kind: &'static str,
    from: String,
    to: Option<String>,
    amount: u64,
    nonce: u64,
}

#[derive(Serialize)]
struct AccountView {
    address: String,
    balance: u64,
    stake: u64,
    nonce: u64,
}

impl From<&SignedTransaction> for TransactionView {
    fn from(signed: &SignedTransaction) -> Self {
        let from = signed.signer.0.clone();

        match &signed.transaction {
            Transaction::Mint { to, amount, nonce } => Self {
                kind: "mint",
                from,
                to: Some(to.0.clone()),
                amount: *amount,
                nonce: *nonce,
            },
            Transaction::Transfer {
                to, amount, nonce, ..
            } => Self {
                kind: "transfer",
                from,
                to: Some(to.0.clone()),
                amount: *amount,
                nonce: *nonce,
            },
            Transaction::Stake { amount, nonce, .. } => Self {
                kind: "stake",
                from,
                to: None,
                amount: *amount,
                nonce: *nonce,
            },
        }
    }
}

impl From<&Block> for BlockView {
    fn from(block: &Block) -> Self {
        Self {
            height: block.height,
            hash: hex::encode(block.hash),
            previous_hash: hex::encode(block.previous_hash),
            proposer: block.proposer.0.clone(),
            transactions: block.transactions.iter().map(Into::into).collect(),
        }
    }
}

fn snapshot(network: &Network, own: &Address) -> Snapshot {
    let peers = network.peers();

    network.with_blockchain(|blockchain: &Blockchain| {
        let mut accounts: Vec<AccountView> = blockchain
            .state
            .accounts
            .iter()
            .map(|(address, account)| AccountView {
                address: address.0.clone(),
                balance: account.balance,
                stake: account.stake,
                nonce: account.nonce,
            })
            .collect();
        accounts.sort_by_key(|account| std::cmp::Reverse((account.stake, account.balance)));

        Snapshot {
            address: own.0.clone(),
            listen_addr: network.listen_addr(),
            chain_id: network.chain_id(),
            height: blockchain.height(),
            proposer: blockchain.proposer().map(|address| address.0),
            mint_authority: blockchain.state.mint_authority().0.clone(),
            peers,
            blocks: blockchain.blocks.iter().rev().map(Into::into).collect(),
            accounts,
            mempool: blockchain.mempool.iter().map(Into::into).collect(),
        }
    })
}

/// Serves the explorer page on `/` and the node's state as JSON on `/api`.
pub(crate) async fn run(addr: SocketAddr, network: Arc<Network>, own: Address) {
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(%addr, %error, "Failed to start the explorer");
            return;
        }
    };
    info!("Explorer on http://{addr}");

    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let network = network.clone();
        let own = own.clone();

        tokio::spawn(async move {
            if let Err(error) = handle(stream, &network, &own).await {
                debug!(%error, "Explorer request failed");
            }
        });
    }
}

async fn handle(mut stream: TcpStream, network: &Network, own: &Address) -> std::io::Result<()> {
    let mut buffer = [0; 4096];
    let read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request.split_whitespace().nth(1).unwrap_or("/");

    let (status, content_type, body) = match path {
        "/" => ("200 OK", "text/html; charset=utf-8", PAGE.to_owned()),
        "/api" => (
            "200 OK",
            "application/json",
            serde_json::to_string(&snapshot(network, own))?,
        ),
        _ => ("404 Not Found", "text/plain", "Not found".to_owned()),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}
