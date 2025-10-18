//! API layer scaffolding for bkg-db (HTTP, pgwire, gRPC).

#![allow(dead_code)]

use anyhow::Result;
use serde_json::Value;

use crate::executor::ExecutionResult;

// TODO(bkg-db/api): Implement REST, pgwire und gRPC Server inkl. Auth & RLS Hooks.

pub trait RestApiServer {
    fn handle_query(&self, body: Value) -> Result<ExecutionResult>;
    fn handle_auth(&self, body: Value) -> Result<Value>;
    fn handle_policy(&self, body: Value) -> Result<Value>;
    fn handle_schema(&self) -> Result<Value>;
}

pub trait PgWireServer {
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}

pub trait GrpcApiServer {
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}

#[derive(Debug, Default, Clone)]
pub struct ApiSurfaceBlueprint;

impl ApiSurfaceBlueprint {
    pub fn new() -> Self {
        Self
    }
}
