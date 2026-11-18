use std::io::BufRead;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::blockchain::transaction::{Address, Transaction};
use crate::blockchain::wallet::Wallet;
use crate::network::Network;

const HELP: &str = "Commands: mint <to> <amount> | transfer <to> <amount> | stake <amount> | balance [address] | address | status";

/// Reads commands from stdin. Transactions are sent from this node's wallet.
///
/// Stdin is read on a plain thread: a blocking read can not be cancelled, and
/// would prevent the runtime from shutting down.
pub(crate) async fn run(network: Arc<Network>, wallet: Arc<Wallet>) {
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

        match words.as_slice() {
            [] => {}
            ["address"] => info!(address = %wallet.address().0, "Address"),
            ["status"] => info!(
                height = ?network.height(),
                peers = network.get_connections_number(),
                mempool = network.mempool_len(),
                proposer = ?network.proposer().map(|address| address.0),
                "Status"
            ),
            ["balance"] => log_balance(&network, &wallet.address()),
            ["balance", address] => log_balance(&network, &Address((*address).into())),
            _ => {
                let Some(transaction) = parse_transaction(&words, &wallet.address()) else {
                    warn!("{HELP}");
                    continue;
                };

                match network.submit_transaction(transaction, &wallet).await {
                    Ok(transaction) => {
                        info!(transaction = ?transaction.transaction, "Transaction submitted")
                    }
                    Err(error) => error!(%error, "Failed to submit transaction"),
                }
            }
        }
    }
}

fn log_balance(network: &Network, address: &Address) {
    let (balance, stake) = network.account(address);
    info!(address = %address.0, balance, stake, "Balance");
}

/// The nonce is set when the transaction is submitted.
fn parse_transaction(words: &[&str], own: &Address) -> Option<Transaction> {
    let transaction = match words {
        ["mint", to, amount] => Transaction::Mint {
            to: Address((*to).into()),
            amount: amount.parse().ok()?,
            nonce: 0,
        },
        ["transfer", to, amount] => Transaction::Transfer {
            from: own.clone(),
            to: Address((*to).into()),
            amount: amount.parse().ok()?,
            nonce: 0,
        },
        ["stake", amount] => Transaction::Stake {
            from: own.clone(),
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
        let own = Address("alice".into());
        assert!(matches!(
            parse_transaction(&["transfer", "bob", "10"], &own),
            Some(Transaction::Transfer { amount: 10, .. })
        ));
        assert!(parse_transaction(&["mint", "alice", "ten"], &own).is_none());
        assert!(parse_transaction(&["stake", "0"], &own).is_none());
        assert!(parse_transaction(&["transfer", "alice", "1"], &own).is_none());
    }
}
