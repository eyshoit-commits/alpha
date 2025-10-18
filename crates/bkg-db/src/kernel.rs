//! Kernel scaffolding for the future MVCC/WAL implementation.
//!
//! The actual MVCC engine, WAL writers and checkpoint managers will live here.
//! For jetzt dient das Modul als Platzhalter, damit andere Komponenten auf
//! eine stabile Schnittstelle verweisen können, während der detaillierte Entwurf
//! erfolgt.

#![allow(dead_code)]

use anyhow::Result;

// TODO(bkg-db/kernel): Replace these placeholders with a fully fledged MVCC
// storage engine backed by WAL + checkpoints (siehe docs/bkg-db.md).

/// Transaction isolation levels supported by the future storage engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionMode {
    ReadOnly,
    ReadWrite,
}

/// Contract for MVCC storage engines.
pub trait StorageEngine {
    type Transaction: StorageTransaction;

    fn begin_transaction(&self, mode: TransactionMode) -> Result<Self::Transaction>;
    fn checkpoint(&self) -> Result<()>;
    fn recover(&self) -> Result<()>;
}

/// Trait modelling transactional behaviour on top of the storage engine.
pub trait StorageTransaction {
    fn commit(self) -> Result<()>;
    fn rollback(self) -> Result<()>;
}

/// WAL manager abstraction used by the kernel to append log entries.
pub trait WalManager {
    fn append(&self, bytes: &[u8]) -> Result<()>;
    fn flush(&self) -> Result<()>;
}

/// Checkpoint manager abstraction to persist consistent state snapshots.
pub trait CheckpointManager {
    fn create_checkpoint(&self) -> Result<()>;
    fn restore_latest(&self) -> Result<()>;
}

/// Draft structure representing the storage kernel blueprint.
#[derive(Debug, Default, Clone)]
pub struct KernelScaffold;

impl KernelScaffold {
    /// Creates a new scaffold instance. Future versions will accept config
    /// options (page size, cache limits, WAL directory, ...).
    pub fn new() -> Self {
        Self
    }

    /// Placeholder hook that will eventually bootstrap the MVCC runtime.
    pub fn initialize(&self) {}
}

/// Draft enum describing planned kernel lifecycle stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelStage {
    Prototype,
    Building,
    Ready,
}

impl Default for KernelStage {
    fn default() -> Self {
        KernelStage::Prototype
    }
}
