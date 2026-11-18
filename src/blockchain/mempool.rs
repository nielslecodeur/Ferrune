use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::transaction::SignedTransaction;

/// Transactions received but not yet included in a block, keyed by their hash.
#[derive(Debug, Clone, Default)]
pub struct Mempool {
    transactions: HashMap<[u8; 32], SignedTransaction>,
}

impl Mempool {
    /// Returns `false` if the transaction was already known.
    pub fn insert(&mut self, transaction: SignedTransaction) -> Result<bool, serde_json::Error> {
        match self.transactions.entry(transaction.hash()?) {
            Entry::Vacant(entry) => {
                entry.insert(transaction);
                Ok(true)
            }
            Entry::Occupied(_) => Ok(false),
        }
    }

    pub fn remove_included(&mut self, transactions: &[SignedTransaction]) {
        for transaction in transactions {
            if let Ok(hash) = transaction.hash() {
                self.transactions.remove(&hash);
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &SignedTransaction> {
        self.transactions.values()
    }

    pub fn retain(&mut self, keep: impl Fn(&SignedTransaction) -> bool) {
        self.transactions.retain(|_, transaction| keep(transaction));
    }

    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.transactions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::transaction::{Address, Transaction};

    fn mint(nonce: u64) -> SignedTransaction {
        SignedTransaction {
            transaction: Transaction::Mint {
                to: Address("alice".into()),
                amount: 10,
                nonce,
            },
            signer: Address("admin".into()),
            signature: String::new(),
        }
    }

    #[test]
    fn ignores_duplicates_and_removes_included() {
        let mut mempool = Mempool::default();
        assert!(mempool.insert(mint(0)).unwrap());
        assert!(!mempool.insert(mint(0)).unwrap());
        assert!(mempool.insert(mint(1)).unwrap());
        assert_eq!(mempool.len(), 2);

        mempool.remove_included(&[mint(0)]);
        assert_eq!(mempool.len(), 1);
    }
}
