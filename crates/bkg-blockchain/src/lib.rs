//! BKG Blockchain Integration
//! 
//! Provides blockchain functionality for the BKG platform, including:
//! - NEO blockchain account management
//! - Cryptographic signing and verification
//! - Address validation
//! - Transaction creation and signing

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub mod account;
pub mod crypto;
pub mod error;
pub mod neo;

pub use account::{Account, KeyPair};
pub use error::BlockchainError;

/// Blockchain account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainAccount {
    /// Public address
    pub address: String,
    /// Public key (hex encoded)
    pub public_key: String,
    /// Private key (hex encoded, should be kept secure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
}

/// Signature result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// Signature data (hex encoded)
    pub signature: String,
    /// Public key used for signing (hex encoded)
    pub public_key: String,
}

/// Blockchain service trait
#[async_trait::async_trait]
pub trait BlockchainService: Send + Sync {
    /// Create a new account
    async fn create_account(&self) -> Result<BlockchainAccount>;
    
    /// Import account from private key
    async fn import_account(&self, private_key: &str) -> Result<BlockchainAccount>;
    
    /// Validate an address
    fn validate_address(&self, address: &str) -> Result<bool>;
    
    /// Sign a message
    async fn sign_message(&self, message: &str, private_key: &str) -> Result<Signature>;
    
    /// Verify a signature
    async fn verify_signature(&self, message: &str, signature: &str, public_key: &str) -> Result<bool>;
}

/// Hash data using SHA-256
pub fn hash_sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Convert bytes to hex string
pub fn to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Convert hex string to bytes
pub fn from_hex(hex: &str) -> Result<Vec<u8>> {
    hex::decode(hex).context("Invalid hex string")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_sha256() {
        let data = b"Hello, BKG!";
        let hash = hash_sha256(data);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_hex_conversion() {
        let data = vec![0x01, 0x02, 0x03, 0xff];
        let hex = to_hex(&data);
        assert_eq!(hex, "010203ff");
        
        let decoded = from_hex(&hex).unwrap();
        assert_eq!(decoded, data);
    }
}
