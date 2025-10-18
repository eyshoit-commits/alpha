//! Kernel scaffolding for the future MVCC/WAL implementation.
//!
//! The actual MVCC engine, WAL writers and checkpoint managers will live here.
//! For jetzt dient das Modul als Platzhalter, damit andere Komponenten auf
//! eine stabile Schnittstelle verweisen können, während der detaillierte Entwurf
//! erfolgt.

#![allow(dead_code)]

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
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
    file_wal: Option<Arc<FileWalManager>>,
}

impl InMemoryStorageEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file_wal<P: AsRef<Path>>(wal_path: P) -> Result<Self> {
        let manager = Arc::new(FileWalManager::new(wal_path.as_ref())?);
        let engine = Self {
            state: Arc::new(RwLock::new(InMemoryState::default())),
            file_wal: Some(manager.clone()),
        };

        let wal_entries = manager.load_entries()?;
        engine.state.write().wal = wal_entries;
        Ok(engine)
    }

    /// Helper for tests/metrics to inspect WAL length.
    pub fn wal_entries(&self) -> usize {
        self.state.read().wal.len()
    }

    /// Returns a clone of all WAL records (serialized events).
    pub fn wal_records(&self) -> Vec<Vec<u8>> {
        self.state.read().wal.clone()
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
            file_wal: self.file_wal.clone(),
        })
    }

    fn checkpoint(&self) -> Result<()> {
        if let Some(manager) = &self.file_wal {
            manager.create_checkpoint()?;
        }
        Ok(())
    }

    fn recover(&self) -> Result<()> {
        if let Some(manager) = &self.file_wal {
            manager.restore_latest()?;
            let entries = manager.load_entries()?;
            self.state.write().wal = entries;
        } else {
            self.state.write().wal.clear();
        }
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
    file_wal: Option<Arc<FileWalManager>>,
}

impl InMemoryTransaction {
    pub fn mode(&self) -> TransactionMode {
        self.mode
    }

    /// Append a WAL entry that will be flushed on commit.
    pub fn append_log(&mut self, entry: &[u8]) -> Result<()> {
        self.staged_log.push(entry.to_vec());
        Ok(())
    }
}

impl StorageTransaction for InMemoryTransaction {
    fn commit(mut self) -> Result<()> {
        let mut state = self.engine.state.write();
        state.wal.extend(self.staged_log.iter().cloned());
        if let Some(manager) = &self.file_wal {
            for entry in &self.staged_log {
                manager.append(entry)?;
            }
            manager.flush()?;
        }
        self.staged_log.clear();
        self.committed = true;
        Ok(())
    }

    fn rollback(mut self) -> Result<()> {
        self.staged_log.clear();
        self.committed = false;
        Ok(())
    }
}

/// File-backed WAL and checkpoint manager.
#[derive(Debug)]
pub struct FileWalManager {
    path: PathBuf,
    checkpoint_path: PathBuf,
}

impl FileWalManager {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating WAL directory {}", parent.display()))?;
        }
        if !path.exists() {
            File::create(path).with_context(|| format!("creating WAL file {}", path.display()))?;
        }
        let checkpoint_path = path.with_extension("checkpoint");
        Ok(Self {
            path: path.to_path_buf(),
            checkpoint_path,
        })
    }

    pub fn load_entries(&self) -> Result<Vec<Vec<u8>>> {
        let mut entries = Vec::new();
        let mut file = File::open(&self.path)
            .with_context(|| format!("opening WAL {}", self.path.display()))?;
        loop {
            let mut len_buf = [0u8; 4];
            if let Err(err) = file.read_exact(&mut len_buf) {
                if err.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(err).with_context(|| "reading WAL length prefix");
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            file.read_exact(&mut data)
                .with_context(|| "reading WAL entry")?;
            entries.push(data);
        }
        Ok(entries)
    }
}

impl WalManager for FileWalManager {
    fn append(&self, bytes: &[u8]) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("opening WAL {}", self.path.display()))?;
        let len = bytes.len() as u32;
        file.write_all(&len.to_le_bytes())
            .with_context(|| "writing WAL length")?;
        file.write_all(bytes)
            .with_context(|| "writing WAL payload")?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .with_context(|| format!("flushing WAL {}", self.path.display()))?;
        file.sync_all().with_context(|| "syncing WAL file")
    }
}

impl CheckpointManager for FileWalManager {
    fn create_checkpoint(&self) -> Result<()> {
        fs::copy(&self.path, &self.checkpoint_path).with_context(|| "writing checkpoint copy")?;
        Ok(())
    }

    fn restore_latest(&self) -> Result<()> {
        if self.checkpoint_path.exists() {
            fs::copy(&self.checkpoint_path, &self.path).with_context(|| "restoring checkpoint")?;
        }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KernelStage {
    #[default]
    Prototype,
    Building,
    Ready,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn in_memory_transaction_appends_wal() {
        let engine = InMemoryStorageEngine::new();
        assert_eq!(engine.wal_entries(), 0);

        let mut tx = engine
            .begin_transaction(TransactionMode::ReadWrite)
            .expect("begin tx");
        tx.append_log(b"INSERT INTO foo VALUES (1)").unwrap();
        tx.append_log(b"INSERT INTO foo VALUES (2)").unwrap();
        tx.commit().expect("commit");

        assert_eq!(engine.wal_entries(), 2);
    }

    #[test]
    fn rollback_discards_staged_entries() {
        let engine = InMemoryStorageEngine::new();
        let mut tx = engine
            .begin_transaction(TransactionMode::ReadWrite)
            .expect("begin tx");
        tx.append_log(b"UPDATE bar SET value = 42").unwrap();
        tx.rollback().expect("rollback");

        assert_eq!(engine.wal_entries(), 0);
    }

    #[test]
    fn file_wal_persists_across_recovery() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("test.wal");

        {
            let engine = InMemoryStorageEngine::with_file_wal(&wal_path).expect("engine");
            let mut tx = engine
                .begin_transaction(TransactionMode::ReadWrite)
                .expect("begin tx");
            tx.append_log(b"UPSERT foo 1").unwrap();
            tx.commit().expect("commit");
            engine.checkpoint().expect("checkpoint");
            assert_eq!(engine.wal_entries(), 1);
        }

        let engine = InMemoryStorageEngine::with_file_wal(&wal_path).expect("engine reload");
        engine.recover().expect("recover");
        assert_eq!(engine.wal_entries(), 1);
    }
}
