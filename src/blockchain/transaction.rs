use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
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
}

#[derive(Debug)]
pub enum TransactionError {
    ZeroAmount,
    SelfTransfer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Address(pub String);
