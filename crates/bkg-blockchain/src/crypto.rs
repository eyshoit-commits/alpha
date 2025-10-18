//! Cryptographic utilities for blockchain operations

use crate::error::BlockchainError;
use crate::{from_hex, hash_sha256, to_hex};
use anyhow::Result;
use secp256k1::{ecdsa::Signature as Secp256k1Signature, Message, PublicKey, Secp256k1, SecretKey};

/// Sign a message with a private key
pub fn sign_message(message: &str, private_key_hex: &str) -> Result<String> {
    let private_key_bytes = from_hex(private_key_hex)?;
    let secret_key = SecretKey::from_slice(&private_key_bytes)
        .map_err(|e| BlockchainError::InvalidPrivateKey(e.to_string()))?;
    
    // Hash the message
    let message_hash = hash_sha256(message.as_bytes());
    let message = Message::from_digest_slice(&message_hash)
        .map_err(|e| BlockchainError::CryptoError(e.to_string()))?;
    
    // Sign
    let secp = Secp256k1::new();
    let signature = secp.sign_ecdsa(&message, &secret_key);
    
    Ok(to_hex(&signature.serialize_compact()))
}

/// Verify a signature
pub fn verify_signature(message: &str, signature_hex: &str, public_key_hex: &str) -> Result<bool> {
    let signature_bytes = from_hex(signature_hex)?;
    let public_key_bytes = from_hex(public_key_hex)?;
    
    let signature = Secp256k1Signature::from_compact(&signature_bytes)
        .map_err(|e| BlockchainError::InvalidSignature(e.to_string()))?;
    
    let public_key = PublicKey::from_slice(&public_key_bytes)
        .map_err(|e| BlockchainError::InvalidPublicKey(e.to_string()))?;
    
    // Hash the message
    let message_hash = hash_sha256(message.as_bytes());
    let message = Message::from_digest_slice(&message_hash)
        .map_err(|e| BlockchainError::CryptoError(e.to_string()))?;
    
    // Verify
    let secp = Secp256k1::new();
    Ok(secp.verify_ecdsa(&message, &signature, &public_key).is_ok())
}

/// Encrypt data using AES-256-GCM (placeholder - implement proper encryption)
pub fn encrypt_private_key(private_key: &str, passphrase: &str) -> Result<String> {
    // TODO: Implement proper AES-256-GCM encryption
    // For now, this is a placeholder
    let combined = format!("{}:{}", passphrase, private_key);
    let encrypted = base64::encode(combined.as_bytes());
    Ok(encrypted)
}

/// Decrypt data (placeholder - implement proper decryption)
pub fn decrypt_private_key(encrypted: &str, passphrase: &str) -> Result<String> {
    // TODO: Implement proper AES-256-GCM decryption
    let decoded = base64::decode(encrypted)
        .map_err(|e| BlockchainError::EncodingError(e.to_string()))?;
    let combined = String::from_utf8(decoded)
        .map_err(|e| BlockchainError::EncodingError(e.to_string()))?;
    
    if let Some(private_key) = combined.strip_prefix(&format!("{}:", passphrase)) {
        Ok(private_key.to_string())
    } else {
        Err(BlockchainError::CryptoError("Invalid passphrase".to_string()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::KeyPair;

    #[test]
    fn test_sign_and_verify() {
        let keypair = KeyPair::generate().unwrap();
        let message = "Hello, BKG Blockchain!";
        
        let signature = sign_message(message, &keypair.private_key_hex()).unwrap();
        let is_valid = verify_signature(message, &signature, &keypair.public_key_hex()).unwrap();
        
        assert!(is_valid);
    }

    #[test]
    fn test_encrypt_decrypt() {
        let private_key = "0123456789abcdef";
        let passphrase = "my-secret-password";
        
        let encrypted = encrypt_private_key(private_key, passphrase).unwrap();
        let decrypted = decrypt_private_key(&encrypted, passphrase).unwrap();
        
        assert_eq!(decrypted, private_key);
    }
}
