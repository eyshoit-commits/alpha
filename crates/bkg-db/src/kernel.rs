//! Kernel scaffolding for the future MVCC/WAL implementation.
//!
//! The actual MVCC engine, WAL writers and checkpoint managers will live here.
//! For jetzt dient das Modul als Platzhalter, damit andere Komponenten auf
//! eine stabile Schnittstelle verweisen können, während der detaillierte Entwurf
//! erfolgt.

#![allow(dead_code)]

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
