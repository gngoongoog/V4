//! Chronological evaluation facade plus persisted holdout-only OOS evidence.
//!
//! The legacy chronological evaluation implementation is preserved byte-for-byte in
//! `evaluation/core.rs`. OOS is a research-only child route and never participates in live
//! analysis, risk, model promotion or broker execution.
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

#[path = "evaluation/core.rs"]
mod core;
#[path = "oos.rs"]
mod oos;

pub struct Service {
    inner: core::Service,
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        // The parent chronological_evaluations table must exist before the OOS foreign key.
        let inner = core::Service::new(db)?;
        oos::init(db)?;
        Ok(Self { inner })
    }

    pub fn tick(&mut self, db: &Connection) {
        self.inner.tick(db);
    }

    pub fn cancel(&mut self) {
        self.inner.cancel();
    }

    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        db: &Connection,
        dir: &Path,
    ) -> Result<Value> {
        if path == "/v1/evaluation/oos" || path.starts_with("/v1/evaluation/oos/") {
            let suffix = path.strip_prefix("/v1/evaluation/oos").unwrap_or("");
            let mapped = format!("/v1/research/oos{suffix}");
            return oos::route(method, &mapped, body, db);
        }
        self.inner.route(method, path, body, db, dir)
    }
}
