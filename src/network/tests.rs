//! Runs real nodes on localhost. Each test uses its own ports.

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use uuid::Uuid;

use super::Network;
use crate::blockchain::core::Blockchain;
use crate::blockchain::state::State;
use crate::blockchain::transaction::{Address, Transaction};
use crate::blockchain::wallet::Wallet;

fn empty_chain() -> Blockchain {
    Blockchain::new(State::new(Address(String::new())))
}

/// Chain with a genesis block from `admin`, and a block minting 100 to `admin`.
fn chain_with_blocks(admin: &Wallet) -> Blockchain {
    let mut chain = empty_chain();
    chain
        .add_block(Blockchain::genesis(admin).unwrap())
        .unwrap();

    let mint = admin
        .sign_transaction(Transaction::Mint {
            to: admin.address(),
            amount: 100,
            nonce: 0,
        })
        .unwrap();
    chain.add_transaction(mint).unwrap();

    let block = chain.build_block(admin).unwrap().unwrap();
    chain.add_block(block).unwrap();
    chain
}

async fn node(port: u16, chain_id: u8, blockchain: Blockchain) -> Arc<Network> {
    let chain_file = std::env::temp_dir().join(format!("ferrune-{}.jsonl", Uuid::new_v4()));
    let network = Arc::new(Network::new(
        SocketAddr::from(([127, 0, 0, 1], port)),
        chain_id,
        Arc::new(RwLock::new(blockchain)),
        chain_file,
    ));

    tokio::spawn(network.clone().listen());
    tokio::time::sleep(Duration::from_millis(50)).await;

    network
}

async fn connect(from: &Arc<Network>, to: &Arc<Network>) {
    from.clone().connect_to(to.listen_addr()).await.unwrap();
}

/// Waits up to 2s for `condition` to become true.
async fn eventually(condition: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    false
}

#[tokio::test]
async fn both_sides_register_the_connection() {
    let a = node(47100, 0, empty_chain()).await;
    let b = node(47101, 0, empty_chain()).await;

    connect(&b, &a).await;

    assert!(
        eventually(|| a.get_connections_number() == 1 && b.get_connections_number() == 1).await
    );
    assert!(a.is_connected(&b.listen_addr()));
    assert!(b.is_connected(&a.listen_addr()));
}

#[tokio::test]
async fn a_different_chain_is_rejected() {
    let a = node(47110, 0, empty_chain()).await;
    let b = node(47111, 1, empty_chain()).await;

    connect(&b, &a).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(a.get_connections_number(), 0);
    assert_eq!(b.get_connections_number(), 0);
}

#[tokio::test]
async fn a_new_node_syncs_the_chain_on_connection() {
    let admin = Wallet::generate();
    let a = node(47120, 0, chain_with_blocks(&admin)).await;
    let b = node(47121, 0, empty_chain()).await;

    connect(&b, &a).await;

    assert!(eventually(|| b.height() == Some(1)).await);
    assert_eq!(b.account(&admin.address()), (100, 0));
}

#[tokio::test]
async fn transactions_are_gossiped_across_the_network() {
    let admin = Wallet::generate();
    let a = node(47130, 0, chain_with_blocks(&admin)).await;
    let b = node(47131, 0, empty_chain()).await;
    let c = node(47132, 0, empty_chain()).await;

    // a <-> b <-> c: c only gets the transaction through b.
    connect(&b, &a).await;
    connect(&c, &b).await;
    assert!(eventually(|| c.height() == Some(1)).await);

    let transfer = Transaction::Transfer {
        from: admin.address(),
        to: Wallet::generate().address(),
        amount: 10,
        nonce: 0,
    };
    a.submit_transaction(transfer, &admin).await.unwrap();

    assert!(eventually(|| c.mempool_len() == 1).await);
    assert_eq!(b.mempool_len(), 1);
}

#[tokio::test]
async fn blocks_are_gossiped_and_unsigned_ones_ignored() {
    let admin = Wallet::generate();
    let mut chain = empty_chain();
    chain
        .add_block(Blockchain::genesis(&admin).unwrap())
        .unwrap();

    let a = node(47140, 0, chain).await;
    let b = node(47141, 0, empty_chain()).await;
    let c = node(47142, 0, empty_chain()).await;

    connect(&b, &a).await;
    connect(&c, &b).await;
    assert!(eventually(|| c.height() == Some(0)).await);

    // A block that is not signed by its proposer is refused.
    let genesis_hash = a.blocks_from(0)[0].hash;
    let unsigned =
        crate::blockchain::block::Block::new(1, genesis_hash, vec![], admin.address()).unwrap();
    a.broadcast_block(&unsigned, None).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(b.height(), Some(0));

    // A valid one goes through b to reach c.
    let mint = Transaction::Mint {
        to: admin.address(),
        amount: 5,
        nonce: 0,
    };
    a.submit_transaction(mint, &admin).await.unwrap();
    let block = a.build_block(&admin).unwrap().unwrap();
    a.add_block(block.clone()).unwrap();
    a.broadcast_block(&block, None).await;

    assert!(eventually(|| c.height() == Some(1)).await);
    assert_eq!(c.account(&admin.address()), (5, 0));
}
