use super::block::Block;
use super::state::State;
use super::state::StateError;
use super::transaction::{Address, Transaction};

#[derive(Debug, Clone)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub state: State,
}

#[derive(Debug)]
pub enum BlockchainError {
    InvalidHeight { expected: u64, received: u64 },
    InvalidPreviousHash,
    InvalidHash,
    HeightOverflow,
    HashCalculation(serde_json::Error),
    InvalidTransaction(StateError),
}

impl Blockchain {
    pub fn new(state: State) -> Self {
        Self {
            blocks: Vec::new(),
            state,
        }
    }

    pub fn validate_block(&self, block: &Block) -> Result<(), BlockchainError> {
        let valid_height = match self.blocks.last() {
            Some(previous_block) => previous_block
                .height
                .checked_add(1)
                .ok_or(BlockchainError::HeightOverflow)?,
            None => 0,
        };

        if block.height != valid_height {
            return Err(BlockchainError::InvalidHeight {
                expected: valid_height,
                received: block.height,
            });
        }

        let expected_previous_hash = match self.blocks.last() {
            Some(previous_block) => previous_block.hash,
            None => [0; 32],
        };

        if block.previous_hash != expected_previous_hash {
            return Err(BlockchainError::InvalidPreviousHash);
        }

        let valid_hash = block
            .has_valid_hash()
            .map_err(BlockchainError::HashCalculation)?;

        if !valid_hash {
            return Err(BlockchainError::InvalidHash);
        }

        Ok(())
    }

    pub fn apply_finalized_block<F>(
        &mut self,
        block: Block,
        verify_signer: F,
    ) -> Result<(), BlockchainError>
    where
        F: Fn(&Transaction) -> Result<Address, BlockchainError>,
    {
        // 1. Vérifier le bloc avec validate_block.
        self.validate_block(&block)?;

        // 2. Cloner self.state dans un état temporaire.
        let mut bis = self.state.clone();
        // 3. Pour chaque transaction :
        //    - Obtenir son signataire avec verify_signer.
        //    - L'appliquer sur l'état temporaire.
        //    - Convertir les erreurs StateError en BlockchainError.
        for transaction in &block.transactions {
            let signer = verify_signer(transaction)?;

            bis.apply_transaction(transaction, &signer)
                .map_err(BlockchainError::InvalidTransaction)?;
        }
        // 4. Remplacer self.state par l'état temporaire.
        self.state = bis;
        // 5. Ajouter le bloc à self.blocks.
        self.blocks.push(block);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(name: &str) -> Address {
        Address(name.into())
    }
    fn chain() -> Blockchain {
        Blockchain::new(State::new(address("admin")))
    }
    fn block(height: u64, previous: [u8; 32], transactions: Vec<Transaction>) -> Block {
        Block::new(height, previous, transactions, address("validator")).unwrap()
    }
    // Simule un verificateur uniquement pour ces tests, sans authentification reelle.
    fn signer(tx: &Transaction) -> Result<Address, BlockchainError> {
        Ok(match tx {
            Transaction::Mint { .. } => address("admin"),
            Transaction::Transfer { from, .. } | Transaction::Stake { from, .. } => from.clone(),
        })
    }

    #[test]
    fn validates_genesis_and_successor_and_rejects_replay() {
        let mut chain = chain();
        let genesis = block(0, [0; 32], vec![]);
        chain
            .apply_finalized_block(genesis.clone(), signer)
            .unwrap();
        let next = block(1, genesis.hash, vec![]);
        chain.validate_block(&next).unwrap();
        chain.apply_finalized_block(next.clone(), signer).unwrap();
        assert!(matches!(
            chain.apply_finalized_block(next, signer),
            Err(BlockchainError::InvalidHeight { .. })
        ));
        assert_eq!(chain.blocks.len(), 2);
    }

    #[test]
    fn rejects_bad_height_previous_hash_and_tampered_content() {
        let chain = chain();
        assert!(matches!(
            chain.validate_block(&block(1, [0; 32], vec![])),
            Err(BlockchainError::InvalidHeight {
                expected: 0,
                received: 1
            })
        ));
        assert!(matches!(
            chain.validate_block(&block(0, [1; 32], vec![])),
            Err(BlockchainError::InvalidPreviousHash)
        ));
        let mut tampered = block(0, [0; 32], vec![]);
        tampered.proposer = address("other");
        assert!(matches!(
            chain.validate_block(&tampered),
            Err(BlockchainError::InvalidHash)
        ));
    }

    #[test]
    fn rejects_invalid_successor_and_height_overflow() {
        let mut chain = chain();
        let genesis = block(0, [0; 32], vec![]);
        chain.apply_finalized_block(genesis, signer).unwrap();
        assert!(matches!(
            chain.validate_block(&block(1, [0; 32], vec![])),
            Err(BlockchainError::InvalidPreviousHash)
        ));
        chain.blocks[0].height = u64::MAX;
        assert!(matches!(
            chain.validate_block(&block(0, [0; 32], vec![])),
            Err(BlockchainError::HeightOverflow)
        ));
    }

    fn mint() -> Transaction {
        Transaction::Mint {
            to: address("alice"),
            amount: 100,
            nonce: 0,
        }
    }
    fn transfer(amount: u64) -> Transaction {
        Transaction::Transfer {
            from: address("alice"),
            to: address("bob"),
            amount,
            nonce: 0,
        }
    }

    #[test]
    fn applies_all_transactions_before_committing_block() {
        let mut chain = chain();
        chain
            .apply_finalized_block(block(0, [0; 32], vec![mint(), transfer(30)]), signer)
            .unwrap();
        assert_eq!(chain.blocks.len(), 1);
        assert_eq!(chain.state.balance(&address("alice")), 70);
        assert_eq!(chain.state.balance(&address("bob")), 30);
    }

    #[test]
    fn transaction_failure_discards_all_changes() {
        let mut chain = chain();
        let before = chain.state.clone();
        let result =
            chain.apply_finalized_block(block(0, [0; 32], vec![mint(), transfer(101)]), signer);
        assert!(matches!(
            result,
            Err(BlockchainError::InvalidTransaction(
                StateError::InsufficientBalance { .. }
            ))
        ));
        assert_eq!(chain.state, before);
        assert!(chain.blocks.is_empty());
    }

    #[test]
    fn signer_failure_discards_previous_transaction() {
        let mut chain = chain();
        let before = chain.state.clone();
        let result = chain.apply_finalized_block(
            block(0, [0; 32], vec![mint(), transfer(1)]),
            |tx| match tx {
                Transaction::Mint { .. } => signer(tx),
                _ => Err(BlockchainError::InvalidTransaction(
                    StateError::Unauthorized,
                )),
            },
        );
        assert!(matches!(
            result,
            Err(BlockchainError::InvalidTransaction(
                StateError::Unauthorized
            ))
        ));
        assert_eq!(chain.state, before);
        assert!(chain.blocks.is_empty());
    }

    #[test]
    fn invalid_block_never_reaches_signer_or_changes_state() {
        let mut chain = chain();
        let before = chain.state.clone();
        let result = chain.apply_finalized_block(block(1, [0; 32], vec![mint()]), |_| {
            panic!("invalid block must be rejected before signature verification")
        });
        assert!(matches!(result, Err(BlockchainError::InvalidHeight { .. })));
        assert_eq!(chain.state, before);
        assert!(chain.blocks.is_empty());
    }
}
