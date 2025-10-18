//! Row-Level Security scaffolding for bkg-db.

#![allow(dead_code)]

#[derive(Debug, Default, Clone)]
pub struct RlsEngineDraft;

impl RlsEngineDraft {
    pub fn new() -> Self {
        Self
    }
}
