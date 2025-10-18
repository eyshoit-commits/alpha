//! Objekt-Storage scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;

// TODO(bkg-db/storage): Implementiere pluggable Storage Layer (lokal, S3, RLS auf Buckets).

pub trait ObjectStorage {
    fn put_object(&self, bucket: &str, key: &str, bytes: &[u8]) -> Result<()>;
    fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>>;
    fn delete_object(&self, bucket: &str, key: &str) -> Result<()>;
    fn presign_url(&self, bucket: &str, key: &str) -> Result<String>;
}

#[derive(Debug, Default, Clone)]
pub struct StorageBlueprint;

impl StorageBlueprint {
    pub fn new() -> Self {
        Self
    }
}
