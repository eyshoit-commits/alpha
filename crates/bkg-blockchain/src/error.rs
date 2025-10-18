//! Blockchain error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockchainError {
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),
    
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
    
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    
    #[error("Cryptographic error: {0}")]
    CryptoError(String),
    
    #[error("Encoding error: {0}")]
    EncodingError(String),
    
    #[error("Account not found")]
    AccountNotFound,
    
    #[error("Transaction error: {0}")]
    TransactionError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Other error: {0}")]
    Other(String),
}

impl From<anyhow::Error> for BlockchainError {
    fn from(err: anyhow::Error) -> Self {
        BlockchainError::Other(err.to_string())
    }
}

impl From<hex::FromHexError> for BlockchainError {
    fn from(err: hex::FromHexError) -> Self {
        BlockchainError::EncodingError(err.to_string())
    }
}
