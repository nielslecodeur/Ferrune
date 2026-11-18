use super::block::Block;
use super::mempool::Mempool;
use super::state::State;
use super::state::StateError;
use super::transaction::{Address, SignedTransaction, Transaction};
use super::wallet::Wallet;
use crate::consts::MAX_TRANSACTIONS_PER_BLOCK;

#[derive(Debug, Clone)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub state: State,
    pub mempool: Mempool,
}

#[derive(Debug)]
pub enum BlockchainError {
    InvalidHeight { expected: u64, received: u64 },
    InvalidPreviousHash,
    InvalidHash,
    HeightOverflow,
    HashCalculation(serde_json::Error),
    InvalidTransaction(StateError),
    InvalidSignature,
    InvalidProposer,
}

impl Blockchain {
    pub fn new(state: State) -> Self {
        Self {
            blocks: Vec::new(),
            state,
            mempool: Mempool::default(),
        }
    }

    pub fn height(&self) -> Option<u64> {
        self.blocks.last().map(|block| block.height)
    }

    pub fn blocks_from(&self, from: u64, limit: usize) -> Vec<Block> {
        let start = usize::try_from(from).unwrap_or(usize::MAX);

        self.blocks
            .iter()
            .skip(start)
            .take(limit)
            .cloned()
            .collect()
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
        F: Fn(&SignedTransaction) -> Result<Address, BlockchainError>,
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

            bis.apply_transaction(&transaction.transaction, &signer)
                .map_err(BlockchainError::InvalidTransaction)?;
        }
        // 4. Remplacer self.state par l'état temporaire.
        self.state = bis;
        // 5. Ajouter le bloc à self.blocks.
        self.mempool.remove_included(&block.transactions);
        let state = &self.state;
        self.mempool.retain(|pending| {
            pending.transaction.nonce() >= state.nonce(sender(state, &pending.transaction))
        });
        self.blocks.push(block);

        Ok(())
    }

    /// Next nonce of the transaction's sender, counting its pending transactions.
    pub fn next_nonce(&self, transaction: &Transaction) -> u64 {
        let sender = sender(&self.state, transaction);
        let pending = self
            .mempool
            .iter()
            .filter(|pending| self::sender(&self.state, &pending.transaction) == sender)
            .count();

        self.state.nonce(sender) + pending as u64
    }

    /// The signature is valid and comes from the sender of the transaction.
    pub fn is_valid_transaction(&self, transaction: &SignedTransaction) -> bool {
        transaction.transaction.validate_basic().is_ok()
            && transaction.has_valid_signature()
            && &transaction.signer == sender(&self.state, &transaction.transaction)
    }

    /// Adds a transaction to the mempool, unless it is already known or
    /// already applied. Returns `false` in those cases.
    pub fn add_transaction(
        &mut self,
        transaction: SignedTransaction,
    ) -> Result<bool, serde_json::Error> {
        let sender = sender(&self.state, &transaction.transaction);

        if transaction.transaction.nonce() < self.state.nonce(sender) {
            return Ok(false);
        }

        self.mempool.insert(transaction)
    }

    /// Validator expected to propose the next block: picked among the accounts
    /// with a stake, with a probability proportional to it. The previous hash
    /// is used as the random seed, so every node picks the same one.
    ///
    /// Until someone stakes, the mint authority proposes every block.
    pub fn proposer(&self) -> Option<Address> {
        let previous = self.blocks.last()?;

        let mut validators: Vec<(&Address, u64)> = self
            .state
            .accounts
            .iter()
            .filter(|(_, account)| account.stake > 0)
            .map(|(address, account)| (address, account.stake))
            .collect();

        if validators.is_empty() {
            return Some(self.state.mint_authority().clone());
        }

        validators.sort_by(|a, b| a.0.0.cmp(&b.0.0));

        let total: u128 = validators.iter().map(|(_, stake)| u128::from(*stake)).sum();
        let seed = u64::from_le_bytes(previous.hash[..8].try_into().ok()?);
        let mut target = u128::from(seed) % total;

        for (address, stake) in validators {
            if target < u128::from(stake) {
                return Some(address.clone());
            }
            target -= u128::from(stake);
        }

        None
    }

    /// First block of the chain. Its proposer becomes the mint authority.
    pub fn genesis(wallet: &Wallet) -> Result<Block, serde_json::Error> {
        let mut block = Block::new(0, [0; 32], Vec::new(), wallet.address())?;
        wallet.sign_block(&mut block);
        Ok(block)
    }

    /// Builds the next block from the mempool if `wallet` is the expected
    /// proposer and there is something to include.
    pub fn build_block(&self, wallet: &Wallet) -> Result<Option<Block>, serde_json::Error> {
        let Some(previous) = self.blocks.last() else {
            return Ok(None);
        };

        if self.proposer() != Some(wallet.address()) {
            return Ok(None);
        }

        let mut pending: Vec<&SignedTransaction> = self.mempool.iter().collect();
        pending.sort_by_key(|transaction| transaction.transaction.nonce());

        // Only keeps the transactions that apply on the current state.
        let mut state = self.state.clone();
        let mut transactions = Vec::new();

        for transaction in pending {
            if transactions.len() == MAX_TRANSACTIONS_PER_BLOCK {
                break;
            }

            if self.is_valid_transaction(transaction)
                && state
                    .apply_transaction(&transaction.transaction, &transaction.signer)
                    .is_ok()
            {
                transactions.push(transaction.clone());
            }
        }

        if transactions.is_empty() {
            return Ok(None);
        }

        let mut block = Block::new(
            previous.height + 1,
            previous.hash,
            transactions,
            wallet.address(),
        )?;
        wallet.sign_block(&mut block);

        Ok(Some(block))
    }

    /// Applies a block received from the network, or produced by this node.
    pub fn add_block(&mut self, block: Block) -> Result<(), BlockchainError> {
        self.validate_block(&block)?;

        if !block.has_valid_signature() {
            return Err(BlockchainError::InvalidSignature);
        }

        let previous_state = self.state.clone();

        if block.height == 0 {
            self.state = State::new(block.proposer.clone());
        } else if self.proposer() != Some(block.proposer.clone()) {
            return Err(BlockchainError::InvalidProposer);
        }

        let result = self.apply_finalized_block(block, |transaction| {
            if transaction.has_valid_signature() {
                Ok(transaction.signer.clone())
            } else {
                Err(BlockchainError::InvalidSignature)
            }
        });

        if result.is_err() {
            self.state = previous_state;
        }

        result
    }
}

fn sender<'a>(state: &'a State, transaction: &'a Transaction) -> &'a Address {
    match transaction {
        Transaction::Mint { .. } => state.mint_authority(),
        Transaction::Transfer { from, .. } | Transaction::Stake { from, .. } => from,
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
        let transactions = transactions.into_iter().map(unsigned).collect();
        Block::new(height, previous, transactions, address("validator")).unwrap()
    }
    fn unsigned(transaction: Transaction) -> SignedTransaction {
        SignedTransaction {
            signer: match &transaction {
                Transaction::Mint { .. } => address("admin"),
                Transaction::Transfer { from, .. } | Transaction::Stake { from, .. } => {
                    from.clone()
                }
            },
            transaction,
            signature: String::new(),
        }
    }
    // Simule un verificateur uniquement pour ces tests, sans authentification reelle.
    fn signer(tx: &SignedTransaction) -> Result<Address, BlockchainError> {
        Ok(match &tx.transaction {
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
            |tx| match &tx.transaction {
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

    /// Chain whose genesis was proposed by `admin`, so `admin` is the mint
    /// authority and proposes the next blocks.
    fn signed_chain(admin: &Wallet) -> Blockchain {
        let mut chain = Blockchain::new(State::new(address("nobody")));
        chain
            .add_block(Blockchain::genesis(admin).unwrap())
            .unwrap();
        chain
    }
    fn signed_mint(admin: &Wallet, to: &Wallet, amount: u64, nonce: u64) -> SignedTransaction {
        admin
            .sign_transaction(Transaction::Mint {
                to: to.address(),
                amount,
                nonce,
            })
            .unwrap()
    }
    fn signed_stake(wallet: &Wallet, amount: u64) -> SignedTransaction {
        wallet
            .sign_transaction(Transaction::Stake {
                from: wallet.address(),
                amount,
                nonce: 0,
            })
            .unwrap()
    }
    fn produce(chain: &mut Blockchain, proposer: &Wallet) {
        let block = chain.build_block(proposer).unwrap().unwrap();
        chain.add_block(block).unwrap();
    }

    #[test]
    fn genesis_proposer_becomes_mint_authority() {
        let admin = Wallet::generate();
        let chain = signed_chain(&admin);
        assert_eq!(chain.state.mint_authority(), &admin.address());
        assert_eq!(chain.proposer(), Some(admin.address()));
    }

    #[test]
    fn produced_block_is_applied_and_clears_mempool() {
        let admin = Wallet::generate();
        let alice = Wallet::generate();
        let mut chain = signed_chain(&admin);

        assert!(
            chain
                .add_transaction(signed_mint(&admin, &alice, 100, 0))
                .unwrap()
        );
        assert!(
            chain
                .add_transaction(signed_mint(&admin, &alice, 5, 2))
                .unwrap()
        );
        produce(&mut chain, &admin);

        assert_eq!(chain.height(), Some(1));
        assert_eq!(chain.state.balance(&alice.address()), 100);
        // The transaction with a future nonce stays in the mempool.
        assert_eq!(chain.mempool.len(), 1);
        assert!(
            !chain
                .add_transaction(signed_mint(&admin, &alice, 100, 0))
                .unwrap()
        );
    }

    #[test]
    fn only_the_expected_proposer_can_build_or_add_a_block() {
        let admin = Wallet::generate();
        let other = Wallet::generate();
        let mut chain = signed_chain(&admin);
        chain
            .add_transaction(signed_mint(&admin, &other, 100, 0))
            .unwrap();

        assert!(chain.build_block(&other).unwrap().is_none());

        let previous = chain.blocks.last().unwrap();
        let mut block = Block::new(
            1,
            previous.hash,
            chain.mempool.iter().cloned().collect(),
            other.address(),
        )
        .unwrap();
        other.sign_block(&mut block);
        assert!(matches!(
            chain.add_block(block),
            Err(BlockchainError::InvalidProposer)
        ));
    }

    #[test]
    fn rejects_unsigned_blocks_and_forged_transactions() {
        let admin = Wallet::generate();
        let alice = Wallet::generate();
        let mut chain = signed_chain(&admin);

        let unsigned_block = Block::new(1, chain.blocks[0].hash, vec![], admin.address()).unwrap();
        assert!(matches!(
            chain.add_block(unsigned_block),
            Err(BlockchainError::InvalidSignature)
        ));

        // Alice can not mint, even with a valid signature.
        let forged = alice
            .sign_transaction(Transaction::Mint {
                to: alice.address(),
                amount: 100,
                nonce: 0,
            })
            .unwrap();
        assert!(!chain.is_valid_transaction(&forged));
        assert!(chain.is_valid_transaction(&signed_mint(&admin, &alice, 100, 0)));
    }

    #[test]
    fn stakers_become_the_proposers() {
        let admin = Wallet::generate();
        let alice = Wallet::generate();
        let mut chain = signed_chain(&admin);

        chain
            .add_transaction(signed_mint(&admin, &alice, 100, 0))
            .unwrap();
        produce(&mut chain, &admin);
        chain.add_transaction(signed_stake(&alice, 10)).unwrap();
        produce(&mut chain, &admin);

        assert_eq!(chain.proposer(), Some(alice.address()));
        assert!(chain.build_block(&admin).unwrap().is_none());
    }

    #[test]
    fn next_nonce_counts_pending_transactions() {
        let admin = Wallet::generate();
        let alice = Wallet::generate();
        let mut chain = signed_chain(&admin);
        let mint = |to: &Wallet| Transaction::Mint {
            to: to.address(),
            amount: 1,
            nonce: 0,
        };

        assert_eq!(chain.next_nonce(&mint(&alice)), 0);
        chain
            .add_transaction(signed_mint(&admin, &alice, 1, 0))
            .unwrap();
        assert_eq!(chain.next_nonce(&mint(&alice)), 1);
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
