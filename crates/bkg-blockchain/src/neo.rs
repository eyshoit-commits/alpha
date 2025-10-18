//! NEO blockchain specific functionality

use crate::account::{Account, KeyPair};
use crate::crypto::{sign_message, verify_signature};
use crate::error::BlockchainError;
use crate::{BlockchainAccount, BlockchainService, Signature};
use anyhow::Result;

/// NEO blockchain service implementation
pub struct NeoService;

impl NeoService {
    pub fn new() -> Self {
        NeoService
    }
}

impl Default for NeoService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl BlockchainService for NeoService {
    async fn create_account(&self) -> Result<BlockchainAccount> {
        let keypair = KeyPair::generate()?;
        let account = Account::from_keypair(&keypair);
        
        Ok(BlockchainAccount {
            address: account.address,
            public_key: account.public_key,
            private_key: account.private_key,
        })
    }
    
    async fn import_account(&self, private_key: &str) -> Result<BlockchainAccount> {
        let keypair = KeyPair::from_private_key(private_key)?;
        let account = Account::from_keypair(&keypair);
        
        Ok(BlockchainAccount {
            address: account.address,
            public_key: account.public_key,
            private_key: Some(private_key.to_string()),
        })
    }
    
    fn validate_address(&self, address: &str) -> Result<bool> {
        // Basic NEO address validation
        // NEO addresses start with 'N' and are base58 encoded
        if !address.starts_with('N') {
            return Ok(false);
        }
        
        // Try to decode base58
        match bs58::decode(address).into_vec() {
            Ok(decoded) => {
                // Should be 25 bytes (1 version + 20 hash + 4 checksum)
                Ok(decoded.len() == 25)
            }
            Err(_) => Ok(false),
        }
    }
    
    async fn sign_message(&self, message: &str, private_key: &str) -> Result<Signature> {
        let keypair = KeyPair::from_private_key(private_key)?;
        let signature = sign_message(message, private_key)?;
        
        Ok(Signature {
            signature,
            public_key: keypair.public_key_hex(),
        })
    }
    
    async fn verify_signature(&self, message: &str, signature: &str, public_key: &str) -> Result<bool> {
        verify_signature(message, signature, public_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_neo_create_account() {
        let service = NeoService::new();
        let account = service.create_account().await.unwrap();
        
        assert!(!account.address.is_empty());
        assert!(account.address.starts_with('N'));
        assert!(account.private_key.is_some());
    }

    #[tokio::test]
    async fn test_neo_sign_verify() {
        let service = NeoService::new();
        let account = service.create_account().await.unwrap();
        let message = "Test message";
        
        let signature = service
            .sign_message(message, &account.private_key.unwrap())
            .await
            .unwrap();
        
        let is_valid = service
            .verify_signature(message, &signature.signature, &signature.public_key)
            .await
            .unwrap();
        
        assert!(is_valid);
    }
}
