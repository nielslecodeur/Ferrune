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
        if let Transaction::Transfer { from, to, .. } = self
            && from == to
        {
            return Err(TransactionError::SelfTransfer);
        }

        Ok(())
    }

    pub fn nonce(&self) -> u64 {
        match self {
            Transaction::Mint { nonce, .. }
            | Transaction::Transfer { nonce, .. }
            | Transaction::Stake { nonce, .. } => *nonce,
        }
    }

    pub fn with_nonce(mut self, new_nonce: u64) -> Self {
        match &mut self {
            Transaction::Mint { nonce, .. }
            | Transaction::Transfer { nonce, .. }
            | Transaction::Stake { nonce, .. } => *nonce = new_nonce,
        }

        self
    }
}

/// A transaction with the signature of its sender.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub signer: Address,
    pub signature: String,
}

impl SignedTransaction {
    pub fn has_valid_signature(&self) -> bool {
        serde_json::to_vec(&self.transaction)
            .is_ok_and(|message| super::wallet::verify(&self.signer, &message, &self.signature))
    }

    pub fn hash(&self) -> Result<[u8; 32], serde_json::Error> {
        use sha2::{Digest, Sha256};

        Ok(Sha256::digest(serde_json::to_vec(self)?).into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("the amount is zero")]
    ZeroAmount,
    #[error("the sender and the recipient are the same")]
    SelfTransfer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub String);
