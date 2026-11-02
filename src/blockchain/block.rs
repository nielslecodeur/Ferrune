use crate::blockchain::transaction::{Address, Transaction};

#[derive(Debug, Clone)]
pub struct Block {
    pub height: u64,
    pub previous_hash: [u8; 32],
    pub transactions: Vec<Transaction>,
    pub proposer: Address,
    pub hash: [u8; 32],
}

impl Block {
    pub fn new(
        height: u64,
        previous_hash: [u8; 32],
        transactions: Vec<Transaction>,
        proposer: Address,
    ) -> Result<Self, serde_json::Error> {
        let mut block = Self {
            height,
            previous_hash,
            transactions,
            proposer,
            hash: [0; 32],
        };
        block.hash = block.calculate_hash()?;
        Ok(block)
    }

    pub fn calculate_hash(&self) -> Result<[u8; 32], serde_json::Error> {
        use sha2::{Digest, Sha256};

        let data = serde_json::to_vec(&(
            self.height,
            self.previous_hash,
            &self.transactions,
            &self.proposer,
        ))?;

        Ok(Sha256::digest(&data).into())
    }

    pub fn has_valid_hash(&self) -> Result<bool, serde_json::Error> {
        Ok(self.hash == self.calculate_hash()?)
    }
}
