use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

use crate::blockchain::block::Block;
use crate::blockchain::core::Blockchain;
use crate::blockchain::transaction::{Address, SignedTransaction, Transaction};
use crate::blockchain::wallet::Wallet;
use crate::network::Network;
use crate::node::cli::parse_transaction;

const PAGE: &str = include_str!("explorer.html");
const MAX_REQUEST_SIZE: usize = 16 * 1024;

/// Everything the explorer page shows, built from the node's current state.
#[derive(Serialize)]
struct Snapshot {
    address: String,
    /// What this node can still spend, mempool included.
    spendable: u64,
    /// Transactions can only be sent from the computer running the node.
    can_submit: bool,
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

/// Transaction sent by the page. It is signed with the node's wallet.
#[derive(Deserialize)]
struct TransactionRequest {
    kind: String,
    #[serde(default)]
    to: String,
    amount: u64,
}

#[derive(Serialize)]
struct TransactionResponse {
    ok: bool,
    message: String,
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

fn snapshot(network: &Network, own: &Address, can_submit: bool) -> Snapshot {
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
            spendable: blockchain.spendable(own),
            can_submit,
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

/// Serves the explorer page on `/`, the node's state as JSON on `/api`, and
/// receives transactions on `POST /api/transaction`.
pub async fn run(addr: SocketAddr, network: Arc<Network>, wallet: Arc<Wallet>) {
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
        let wallet = wallet.clone();

        tokio::spawn(async move {
            if let Err(error) = handle(stream, &network, &wallet).await {
                debug!(%error, "Explorer request failed");
            }
        });
    }
}

struct Request {
    method: String,
    path: String,
    is_json: bool,
    body: Vec<u8>,
}

/// Reads the request line, the headers and the body (up to `MAX_REQUEST_SIZE`).
async fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    let mut data = Vec::new();
    let mut buffer = [0; 4096];

    let header_end = loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        data.extend_from_slice(&buffer[..read]);

        if let Some(position) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if data.len() > MAX_REQUEST_SIZE {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
    };

    let head = String::from_utf8_lossy(&data[..header_end]).to_string();
    let mut lines = head.lines();
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let path = request_line.next().unwrap_or("/").to_owned();

    let mut content_length = 0;
    let mut is_json = false;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();

        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.parse().unwrap_or(0),
            "content-type" => is_json = value.starts_with("application/json"),
            _ => {}
        }
    }

    if content_length > MAX_REQUEST_SIZE {
        return Err(std::io::ErrorKind::InvalidData.into());
    }

    let mut body = data[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
    }
    body.truncate(content_length);

    Ok(Request {
        method,
        path,
        is_json,
        body,
    })
}

async fn submit(request: &Request, network: &Network, wallet: &Wallet) -> TransactionResponse {
    let fail = |message: &str| TransactionResponse {
        ok: false,
        message: message.to_owned(),
    };

    let Ok(input) = serde_json::from_slice::<TransactionRequest>(&request.body) else {
        return fail("Invalid request");
    };

    let amount = input.amount.to_string();
    let words: Vec<&str> = match input.kind.as_str() {
        "mint" | "transfer" => vec![input.kind.as_str(), input.to.trim(), &amount],
        "stake" => vec!["stake", &amount],
        _ => return fail("Unknown transaction type"),
    };

    let Some(transaction) = parse_transaction(&words, &wallet.address()) else {
        return fail("Invalid transaction: check the address and the amount");
    };

    match network.submit_transaction(transaction, wallet).await {
        Ok(transaction) => {
            info!(transaction = ?transaction.transaction, "Transaction submitted from the explorer");
            TransactionResponse {
                ok: true,
                message: "Transaction sent, it will be in the next block".to_owned(),
            }
        }
        Err(error) => fail(&error.to_string()),
    }
}

async fn handle(mut stream: TcpStream, network: &Network, wallet: &Wallet) -> std::io::Result<()> {
    let request = read_request(&mut stream).await?;
    // The page signs with this node's wallet: only the computer running the
    // node may use it, even if the explorer is open to the network.
    let is_local = stream.peer_addr()?.ip().is_loopback();

    let (status, content_type, body) = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => ("200 OK", "text/html; charset=utf-8", PAGE.to_owned()),
        ("GET", "/api") => (
            "200 OK",
            "application/json",
            serde_json::to_string(&snapshot(network, &wallet.address(), is_local))?,
        ),
        // Requiring JSON makes browsers send a preflight request first, which
        // we never accept: other websites can not send transactions.
        ("POST", "/api/transaction") if is_local && request.is_json => (
            "200 OK",
            "application/json",
            serde_json::to_string(&submit(&request, network, wallet).await)?,
        ),
        ("POST", "/api/transaction") => ("403 Forbidden", "text/plain", "Forbidden".to_owned()),
        _ => ("404 Not Found", "text/plain", "Not found".to_owned()),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}
