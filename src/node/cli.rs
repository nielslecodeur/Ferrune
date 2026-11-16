use std::io::BufRead;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::blockchain::transaction::{Address, Transaction};
use crate::network::Network;

const HELP: &str =
    "Commands: mint <to> <amount> | transfer <from> <to> <amount> | stake <from> <amount> | status";

/// Reads commands from stdin.
///
/// Stdin is read on a plain thread: a blocking read can not be cancelled, and
/// would prevent the runtime from shutting down.
pub(crate) async fn run(network: Arc<Network>) {
    let (sender, mut receiver) = mpsc::unbounded_channel();

    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };

            if sender.send(line).is_err() {
                break;
            }
        }
    });

    while let Some(line) = receiver.recv().await {
        let words: Vec<&str> = line.split_whitespace().collect();

        if words.is_empty() {
            continue;
        }

        if words == ["status"] {
            info!(
                height = ?network.height(),
                peers = network.get_connections_number(),
                mempool = network.mempool_len(),
                "Status"
            );
            continue;
        }

        let Some(transaction) = parse_transaction(&words) else {
            warn!("{HELP}");
            continue;
        };

        match network.submit_transaction(transaction).await {
            Ok(transaction) => info!(?transaction, "Transaction submitted"),
            Err(error) => error!(%error, "Failed to submit transaction"),
        }
    }
}

/// The nonce is set when the transaction is submitted.
fn parse_transaction(words: &[&str]) -> Option<Transaction> {
    let address = |name: &str| Address(name.into());

    let transaction = match words {
        ["mint", to, amount] => Transaction::Mint {
            to: address(to),
            amount: amount.parse().ok()?,
            nonce: 0,
        },
        ["transfer", from, to, amount] => Transaction::Transfer {
            from: address(from),
            to: address(to),
            amount: amount.parse().ok()?,
            nonce: 0,
        },
        ["stake", from, amount] => Transaction::Stake {
            from: address(from),
            amount: amount.parse().ok()?,
            nonce: 0,
        },
        _ => return None,
    };

    transaction.validate_basic().ok()?;
    Some(transaction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert!(matches!(
            parse_transaction(&["transfer", "alice", "bob", "10"]),
            Some(Transaction::Transfer { amount: 10, .. })
        ));
        assert!(parse_transaction(&["mint", "alice", "ten"]).is_none());
        assert!(parse_transaction(&["stake", "alice", "0"]).is_none());
        assert!(parse_transaction(&["transfer", "alice", "alice", "1"]).is_none());
    }
}
