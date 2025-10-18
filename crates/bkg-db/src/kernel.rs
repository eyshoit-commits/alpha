//! Kernel scaffolding for the future MVCC/WAL implementation.
//!
//! The actual MVCC engine, WAL writers and checkpoint managers will live here.
//! For jetzt dient das Modul als Platzhalter, damit andere Komponenten auf
//! eine stabile Schnittstelle verweisen können, während der detaillierte Entwurf
//! erfolgt.

#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use parking_lot::RwLock;

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

#[derive(Debug, Default)]
struct InMemoryState {
    wal: Vec<Vec<u8>>,
}

/// Minimal in-memory prototype that mimics a WAL-backed storage engine.
#[derive(Debug, Clone, Default)]
pub struct InMemoryStorageEngine {
    state: Arc<RwLock<InMemoryState>>,
}

impl InMemoryStorageEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Helper for tests/metrics to inspect WAL length.
    pub fn wal_entries(&self) -> usize {
        self.state.read().wal.len()
    }
}

impl StorageEngine for InMemoryStorageEngine {
    type Transaction = InMemoryTransaction;

    fn begin_transaction(&self, mode: TransactionMode) -> Result<Self::Transaction> {
        Ok(InMemoryTransaction {
            engine: self.clone(),
            mode,
            staged_log: Vec::new(),
            committed: false,
        })
    }

    fn checkpoint(&self) -> Result<()> {
        // TODO(bkg-db/kernel): Persist WAL to durable storage & produce snapshot.
        Ok(())
    }

    fn recover(&self) -> Result<()> {
        // TODO(bkg-db/kernel): Replay WAL and rebuild in-memory state.
        Ok(())
    }
}

/// Write-Ahead log transaction prototype.
#[derive(Debug)]
pub struct InMemoryTransaction {
    engine: InMemoryStorageEngine,
    mode: TransactionMode,
    staged_log: Vec<Vec<u8>>,
    committed: bool,
}

impl InMemoryTransaction {
    pub fn mode(&self) -> TransactionMode {
        self.mode
    }

    /// Append a WAL entry that will be flushed on commit.
    pub fn append_log(&mut self, entry: &[u8]) {
        self.staged_log.push(entry.to_vec());
    }
}

impl StorageTransaction for InMemoryTransaction {
    fn commit(mut self) -> Result<()> {
        let mut state = self.engine.state.write();
        state.wal.extend(self.staged_log.drain(..));
        self.committed = true;
        Ok(())
    }

    fn rollback(mut self) -> Result<()> {
        self.staged_log.clear();
        self.committed = false;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_transaction_appends_wal() {
        let engine = InMemoryStorageEngine::new();
        assert_eq!(engine.wal_entries(), 0);

        let mut tx = engine
            .begin_transaction(TransactionMode::ReadWrite)
            .expect("begin tx");
        tx.append_log(b"INSERT INTO foo VALUES (1)");
        tx.append_log(b"INSERT INTO foo VALUES (2)");
        tx.commit().expect("commit");

        assert_eq!(engine.wal_entries(), 2);
    }

    #[test]
    fn rollback_discards_staged_entries() {
        let engine = InMemoryStorageEngine::new();
        let mut tx = engine
            .begin_transaction(TransactionMode::ReadWrite)
            .expect("begin tx");
        tx.append_log(b"UPDATE bar SET value = 42");
        tx.rollback().expect("rollback");

        assert_eq!(engine.wal_entries(), 0);
    }
}
