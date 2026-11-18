use std::sync::Arc;

use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use crate::blockchain::wallet::Wallet;
use crate::consts::BLOCK_INTERVAL;
use crate::network::Network;

/// Produces a block from the mempool whenever this node is the expected proposer.
pub(crate) async fn run(network: Arc<Network>, wallet: Arc<Wallet>) {
    let mut interval = tokio::time::interval(BLOCK_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        interval.tick().await;

        let block = match network.build_block(&wallet) {
            Ok(Some(block)) => block,
            Ok(None) => continue,
            Err(error) => {
                warn!(%error, "Failed to build block");
                continue;
            }
        };

        if let Err(error) = network.add_block(block.clone()) {
            warn!(%error, "Failed to add produced block");
            continue;
        }

        info!(
            height = block.height,
            transactions = block.transactions.len(),
            "Block produced"
        );

        network.broadcast_block(&block, None).await;
    }
}
