use super::transaction::{Address, Transaction, TransactionError};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    pub balance: u64,
    pub stake: u64,
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub accounts: HashMap<Address, Account>,
    mint_authority: Address,
}

impl State {
    pub fn new(mint_authority: Address) -> Self {
        Self {
            accounts: HashMap::new(),
            mint_authority,
        }
    }

    pub fn balance(&self, address: &Address) -> u64 {
        self.accounts
            .get(address)
            .map(|account| account.balance)
            .unwrap_or(0)
    }

    /// Le signataire doit provenir d'une verification cryptographique externe.
    pub fn apply_transaction(
        &mut self,
        transaction: &Transaction,
        verified_signer: &Address,
    ) -> Result<(), StateError> {
        transaction
            .validate_basic()
            .map_err(|transaction_error| StateError::InvalidTransaction { transaction_error })?;

        let (sender, nonce, amount) = match transaction {
            Transaction::Mint { nonce, amount, .. } => {
                if verified_signer != &self.mint_authority {
                    return Err(StateError::Unauthorized);
                }
                (&self.mint_authority, *nonce, *amount)
            }
            Transaction::Transfer {
                from,
                nonce,
                amount,
                ..
            }
            | Transaction::Stake {
                from,
                nonce,
                amount,
            } => {
                if verified_signer != from {
                    return Err(StateError::Unauthorized);
                }
                (from, *nonce, *amount)
            }
        };
        let current = self.accounts.get(sender).cloned().unwrap_or_default();
        if nonce != current.nonce {
            return Err(StateError::InvalidNonce {
                expected: current.nonce,
                received: nonce,
            });
        }
        let next_nonce = current.nonce.checked_add(1).ok_or(StateError::Overflow)?;
        let sender = sender.clone();

        match transaction {
            Transaction::Mint { to, .. } => {
                let new_balance = self
                    .balance(to)
                    .checked_add(amount)
                    .ok_or(StateError::Overflow)?;
                self.accounts.entry(to.clone()).or_default().balance = new_balance;
                self.accounts.entry(sender).or_default().nonce = next_nonce;
            }
            Transaction::Transfer { to, .. } => {
                let new_balance =
                    current
                        .balance
                        .checked_sub(amount)
                        .ok_or(StateError::InsufficientBalance {
                            balance: current.balance,
                        })?;
                let recipient_balance = self
                    .balance(to)
                    .checked_add(amount)
                    .ok_or(StateError::Overflow)?;
                let account = self.accounts.entry(sender).or_default();
                account.balance = new_balance;
                account.nonce = next_nonce;
                self.accounts.entry(to.clone()).or_default().balance = recipient_balance;
            }
            Transaction::Stake { .. } => {
                let new_balance =
                    current
                        .balance
                        .checked_sub(amount)
                        .ok_or(StateError::InsufficientBalance {
                            balance: current.balance,
                        })?;
                let new_stake = current
                    .stake
                    .checked_add(amount)
                    .ok_or(StateError::Overflow)?;
                let account = self.accounts.entry(sender).or_default();
                account.balance = new_balance;
                account.stake = new_stake;
                account.nonce = next_nonce;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum StateError {
    InvalidTransaction { transaction_error: TransactionError },
    InsufficientBalance { balance: u64 },
    InvalidNonce { expected: u64, received: u64 },
    Unauthorized,
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(name: &str) -> Address {
        Address(name.into())
    }

    fn funded() -> State {
        let mut state = State::new(address("admin"));
        state
            .apply_transaction(
                &Transaction::Mint {
                    to: address("alice"),
                    amount: 100,
                    nonce: 0,
                },
                &address("admin"),
            )
            .unwrap();
        state
    }

    fn assert_rejected(state: &mut State, tx: Transaction, signer: &str) -> StateError {
        let before = state.clone();
        let error = state.apply_transaction(&tx, &address(signer)).unwrap_err();
        assert_eq!(*state, before);
        error
    }

    #[test]
    fn transfer_and_stake_share_sender_nonce() {
        let mut state = funded();
        state
            .apply_transaction(
                &Transaction::Transfer {
                    from: address("alice"),
                    to: address("bob"),
                    amount: 30,
                    nonce: 0,
                },
                &address("alice"),
            )
            .unwrap();
        state
            .apply_transaction(
                &Transaction::Stake {
                    from: address("alice"),
                    amount: 20,
                    nonce: 1,
                },
                &address("alice"),
            )
            .unwrap();
        assert_eq!(state.balance(&address("alice")), 50);
        assert_eq!(state.balance(&address("bob")), 30);
        assert_eq!(state.accounts[&address("alice")].stake, 20);
        assert_eq!(state.accounts[&address("alice")].nonce, 2);
        assert_eq!(state.balance(&address("unknown")), 0);
    }

    #[test]
    fn mint_requires_authority_and_rejects_replay() {
        let mut state = funded();
        let mint = || Transaction::Mint {
            to: address("bob"),
            amount: 1,
            nonce: 0,
        };
        assert!(matches!(
            assert_rejected(&mut state, mint(), "alice"),
            StateError::Unauthorized
        ));
        assert!(matches!(
            assert_rejected(&mut state, mint(), "admin"),
            StateError::InvalidNonce { .. }
        ));
    }

    #[test]
    fn invalid_transactions_leave_state_intact() {
        let mut state = funded();
        for stake in [false, true] {
            let tx = |amount, nonce| {
                if stake {
                    Transaction::Stake {
                        from: address("alice"),
                        amount,
                        nonce,
                    }
                } else {
                    Transaction::Transfer {
                        from: address("alice"),
                        to: address("bob"),
                        amount,
                        nonce,
                    }
                }
            };
            assert!(matches!(
                assert_rejected(&mut state, tx(101, 0), "alice"),
                StateError::InsufficientBalance { .. }
            ));
            assert!(matches!(
                assert_rejected(&mut state, tx(1, 1), "alice"),
                StateError::InvalidNonce { .. }
            ));
            assert!(matches!(
                assert_rejected(&mut state, tx(1, 0), "bob"),
                StateError::Unauthorized
            ));
            assert!(matches!(
                assert_rejected(&mut state, tx(0, 0), "alice"),
                StateError::InvalidTransaction { .. }
            ));
        }
        assert!(matches!(
            assert_rejected(
                &mut state,
                Transaction::Transfer {
                    from: address("alice"),
                    to: address("alice"),
                    amount: 1,
                    nonce: 0,
                },
                "alice"
            ),
            StateError::InvalidTransaction { .. }
        ));
    }

    #[test]
    fn overflow_leaves_state_intact() {
        let mut state = funded();
        state.accounts.insert(
            address("bob"),
            Account {
                balance: u64::MAX,
                ..Account::default()
            },
        );
        assert!(matches!(
            assert_rejected(
                &mut state,
                Transaction::Transfer {
                    from: address("alice"),
                    to: address("bob"),
                    amount: 1,
                    nonce: 0,
                },
                "alice"
            ),
            StateError::Overflow
        ));
        assert!(matches!(
            assert_rejected(
                &mut state,
                Transaction::Mint {
                    to: address("bob"),
                    amount: 1,
                    nonce: 1,
                },
                "admin"
            ),
            StateError::Overflow
        ));
        state.accounts.get_mut(&address("alice")).unwrap().stake = u64::MAX;
        assert!(matches!(
            assert_rejected(
                &mut state,
                Transaction::Stake {
                    from: address("alice"),
                    amount: 1,
                    nonce: 0,
                },
                "alice"
            ),
            StateError::Overflow
        ));
        state.accounts.get_mut(&address("alice")).unwrap().nonce = u64::MAX;
        assert!(matches!(
            assert_rejected(
                &mut state,
                Transaction::Stake {
                    from: address("alice"),
                    amount: 1,
                    nonce: u64::MAX,
                },
                "alice"
            ),
            StateError::Overflow
        ));
    }

    #[test]
    fn mint_to_authority_updates_balance_and_nonce() {
        let mut state = State::new(address("admin"));
        state
            .apply_transaction(
                &Transaction::Mint {
                    to: address("admin"),
                    amount: 10,
                    nonce: 0,
                },
                &address("admin"),
            )
            .unwrap();
        assert_eq!(state.balance(&address("admin")), 10);
        assert_eq!(state.accounts[&address("admin")].nonce, 1);
    }
}
