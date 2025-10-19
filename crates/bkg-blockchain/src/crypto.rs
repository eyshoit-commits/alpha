//! Cryptographic utilities for blockchain operations

use crate::error::BlockchainError;
use crate::{from_hex, hash_sha256, to_hex};
use aes_gcm::{aead::Aead, aead::KeyInit, Aes256Gcm, Key, Nonce};
use anyhow::Result;
use pbkdf2::pbkdf2_hmac;
use rand::rngs::OsRng;
use rand::RngCore;
use secp256k1::{ecdsa::Signature as Secp256k1Signature, Message, PublicKey, Secp256k1, SecretKey};
use sha2::Sha256;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const PBKDF2_ITERATIONS: u32 = 100_000;

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

/// Encrypt a private key using AES-256-GCM with PBKDF2 key derivation.
pub fn encrypt_private_key(private_key: &str, passphrase: &str) -> Result<String> {
    if passphrase.is_empty() {
        return Err(
            BlockchainError::CryptoError("Passphrase must not be empty".to_string()).into(),
        );
    }

    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &salt, PBKDF2_ITERATIONS, &mut key);

    let cipher = Aes256Gcm::new(Key::from_slice(&key));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, private_key.as_bytes())
        .map_err(|_| BlockchainError::CryptoError("Encryption failed".to_string()))?;

    let mut output = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);

    Ok(base64::encode(output))
}

/// Decrypt a private key protected by [`encrypt_private_key`].
pub fn decrypt_private_key(encrypted: &str, passphrase: &str) -> Result<String> {
    if passphrase.is_empty() {
        return Err(
            BlockchainError::CryptoError("Passphrase must not be empty".to_string()).into(),
        );
    }

    let decoded =
        base64::decode(encrypted).map_err(|e| BlockchainError::EncodingError(e.to_string()))?;

    if decoded.len() <= SALT_LEN + NONCE_LEN {
        return Err(BlockchainError::CryptoError("Ciphertext too short".to_string()).into());
    }

    let (salt, rest) = decoded.split_at(SALT_LEN);
    let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);

    let cipher = Aes256Gcm::new(Key::from_slice(&key));
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| BlockchainError::CryptoError("Decryption failed".to_string()))?;

    let private_key =
        String::from_utf8(plaintext).map_err(|e| BlockchainError::EncodingError(e.to_string()))?;

    Ok(private_key)
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

    #[test]
    fn test_decrypt_with_wrong_passphrase_fails() {
        let private_key = "0123456789abcdef";
        let encrypted = encrypt_private_key(private_key, "correct-pass").unwrap();

        let err = decrypt_private_key(&encrypted, "wrong-pass").unwrap_err();
        let crypto_err = err.downcast_ref::<BlockchainError>().unwrap();
        assert!(
            matches!(crypto_err, BlockchainError::CryptoError(message) if message == "Decryption failed")
        );
    }

    #[test]
    fn test_encrypt_with_empty_passphrase_errors() {
        let err = encrypt_private_key("0123456789abcdef", "").unwrap_err();
        let crypto_err = err.downcast_ref::<BlockchainError>().unwrap();
        assert!(
            matches!(crypto_err, BlockchainError::CryptoError(message) if message == "Passphrase must not be empty")
        );
    }
}
