use std::fs;
use std::io;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use super::block::Block;
use super::transaction::{Address, SignedTransaction, Transaction};

/// The node's key pair. Its address is the hex encoded public key.
pub struct Wallet {
    key: SigningKey,
}

impl Wallet {
    pub fn generate() -> Self {
        Self {
            key: SigningKey::from_bytes(&rand::random()),
        }
    }

    /// Loads the secret key from `path`, or creates a new one there.
    pub fn load_or_create(path: &Path) -> io::Result<Self> {
        if path.exists() {
            let bytes = hex::decode(fs::read_to_string(path)?.trim())
                .map_err(io::Error::other)?
                .try_into()
                .map_err(|_| io::Error::other("invalid key length"))?;

            return Ok(Self {
                key: SigningKey::from_bytes(&bytes),
            });
        }

        let wallet = Self::generate();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, hex::encode(wallet.key.to_bytes()))?;

        Ok(wallet)
    }

    pub fn address(&self) -> Address {
        Address(hex::encode(self.key.verifying_key().to_bytes()))
    }

    fn sign(&self, message: &[u8]) -> String {
        hex::encode(self.key.sign(message).to_bytes())
    }

    pub fn sign_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<SignedTransaction, serde_json::Error> {
        let signature = self.sign(&serde_json::to_vec(&transaction)?);

        Ok(SignedTransaction {
            transaction,
            signer: self.address(),
            signature,
        })
    }

    /// Signs the block hash, so the hash must be computed first.
    pub fn sign_block(&self, block: &mut Block) {
        block.signature = self.sign(&block.hash);
    }
}

/// Checks that `signature` is a valid signature of `message` by `signer`.
pub fn verify(signer: &Address, message: &[u8], signature: &str) -> bool {
    let Some(key) = hex::decode(&signer.0)
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok())
    else {
        return false;
    };

    let Some(signature) = hex::decode(signature)
        .ok()
        .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
        .map(|bytes| Signature::from_bytes(&bytes))
    else {
        return false;
    };

    key.verify_strict(message, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mint(wallet: &Wallet) -> Transaction {
        Transaction::Mint {
            to: wallet.address(),
            amount: 10,
            nonce: 0,
        }
    }

    #[test]
    fn signed_transaction_is_verified() {
        let wallet = Wallet::generate();
        let signed = wallet.sign_transaction(mint(&wallet)).unwrap();
        assert!(signed.has_valid_signature());

        let mut tampered = signed.clone();
        tampered.transaction = tampered.transaction.with_nonce(1);
        assert!(!tampered.has_valid_signature());

        let mut stolen = signed;
        stolen.signer = Wallet::generate().address();
        assert!(!stolen.has_valid_signature());
    }

    #[test]
    fn signed_block_is_verified() {
        let wallet = Wallet::generate();
        let mut block = Block::new(0, [0; 32], vec![], wallet.address()).unwrap();
        assert!(!block.has_valid_signature());

        wallet.sign_block(&mut block);
        assert!(block.has_valid_signature());

        block.proposer = Wallet::generate().address();
        assert!(!block.has_valid_signature());
    }

    #[test]
    fn key_is_saved_and_loaded() {
        let path = std::env::temp_dir().join(format!("ferrune-{}.key", uuid::Uuid::new_v4()));
        let created = Wallet::load_or_create(&path).unwrap();
        let loaded = Wallet::load_or_create(&path).unwrap();
        assert_eq!(created.address(), loaded.address());
        fs::remove_file(path).unwrap();
    }
}
