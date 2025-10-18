//! API layer scaffolding for bkg-db (HTTP, pgwire, gRPC).

#![allow(dead_code)]

#[derive(Debug, Default, Clone)]
pub struct ApiSurfaceBlueprint;

impl ApiSurfaceBlueprint {
    pub fn new() -> Self {
        Self
    }
}
