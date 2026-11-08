use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Transaction {
    Mint {
        to: Address,
        amount: u64,
        nonce: u64,
    },
    Transfer {
        from: Address,
        to: Address,
        amount: u64,
        nonce: u64,
    },
    Stake {
        from: Address,
        nonce: u64,
        amount: u64,
    },
}

impl Transaction {
    pub fn validate_basic(&self) -> Result<(), TransactionError> {
        match self {
            Transaction::Mint { amount, .. }
            | Transaction::Stake { amount, .. }
            | Transaction::Transfer { amount, .. } => {
                if *amount == 0 {
                    return Err(TransactionError::ZeroAmount);
                }
            }
        }
        if let Transaction::Transfer { from, to, .. } = self {
            if from == to {
                return Err(TransactionError::SelfTransfer);
            }
        }

        Ok(())
    }

    pub fn hash(&self) -> Result<[u8; 32], serde_json::Error> {
        use sha2::{Digest, Sha256};

        Ok(Sha256::digest(serde_json::to_vec(self)?).into())
    }
}

#[derive(Debug)]
pub enum TransactionError {
    ZeroAmount,
    SelfTransfer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub String);
