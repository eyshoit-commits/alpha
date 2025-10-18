//! Blockchain account management

use crate::error::BlockchainError;
use crate::{from_hex, to_hex};
use anyhow::Result;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};

/// Key pair for blockchain accounts
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub secret_key: SecretKey,
    pub public_key: PublicKey,
}

impl KeyPair {
    /// Generate a new random key pair
    pub fn generate() -> Result<Self> {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        
        Ok(KeyPair {
            secret_key,
            public_key,
        })
    }
    
    /// Create key pair from private key hex string
    pub fn from_private_key(private_key_hex: &str) -> Result<Self> {
        let private_key_bytes = from_hex(private_key_hex)
            .map_err(|e| BlockchainError::InvalidPrivateKey(e.to_string()))?;
        
        let secret_key = SecretKey::from_slice(&private_key_bytes)
            .map_err(|e| BlockchainError::InvalidPrivateKey(e.to_string()))?;
        
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        
        Ok(KeyPair {
            secret_key,
            public_key,
        })
    }
    
    /// Get private key as hex string
    pub fn private_key_hex(&self) -> String {
        to_hex(&self.secret_key.secret_bytes())
    }
    
    /// Get public key as hex string
    pub fn public_key_hex(&self) -> String {
        to_hex(&self.public_key.serialize())
    }
}

/// Blockchain account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub address: String,
    pub public_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
}

impl Account {
    /// Create a new account from key pair
    pub fn from_keypair(keypair: &KeyPair) -> Self {
        let address = derive_address_from_public_key(&keypair.public_key);
        
        Account {
            address,
            public_key: keypair.public_key_hex(),
            private_key: Some(keypair.private_key_hex()),
        }
    }
    
    /// Create account from address and public key (no private key)
    pub fn from_public(address: String, public_key: String) -> Self {
        Account {
            address,
            public_key,
            private_key: None,
        }
    }
}

/// Derive address from public key (simplified NEO-style)
fn derive_address_from_public_key(public_key: &PublicKey) -> String {
    use sha2::{Digest, Sha256};
    use ripemd::Ripemd160;
    
    // Serialize public key
    let pubkey_bytes = public_key.serialize();
    
    // SHA-256 hash
    let sha256_hash = Sha256::digest(&pubkey_bytes);
    
    // RIPEMD-160 hash
    let ripemd_hash = Ripemd160::digest(&sha256_hash);
    
    // Add version byte (0x17 for NEO)
    let mut versioned = vec![0x17];
    versioned.extend_from_slice(&ripemd_hash);
    
    // Double SHA-256 for checksum
    let checksum = Sha256::digest(&Sha256::digest(&versioned));
    versioned.extend_from_slice(&checksum[..4]);
    
    // Base58 encode
    bs58::encode(versioned).into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = KeyPair::generate().unwrap();
        assert_eq!(keypair.secret_key.secret_bytes().len(), 32);
        assert!(keypair.public_key.serialize().len() > 0);
    }

    #[test]
    fn test_account_from_keypair() {
        let keypair = KeyPair::generate().unwrap();
        let account = Account::from_keypair(&keypair);
        
        assert!(!account.address.is_empty());
        assert!(!account.public_key.is_empty());
        assert!(account.private_key.is_some());
    }
}
