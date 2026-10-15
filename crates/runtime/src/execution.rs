//! Execution semantics wrapper: coordinated main brain by default, per-brain SOLO override on demand.
//! The previous 0.6 WIP engine is preserved verbatim in execution_legacy.rs.
#[path = "execution_legacy.rs"]
mod legacy;

use anyhow::Result;
use braintrade_domain::{
    execution::BrainConfig,
    studio::FrameBar,
    Config, Status, Telemetry,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Service {
    inner: legacy::Service,
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        let mut inner = legacy::Service::new(db)?;
        // `system` is the coordinated main brain. It is controlled by global START/STOP,
        // not by the per-brain SOLO switch. Keep it enabled in memory even when an older
        // 0.6 checkpoint persisted it as false.
        if let Some(system) = inner.brains.iter_mut().find(|brain| brain.id == "system") {
            system.enabled = true;
        }
        Ok(Self { inner })
    }

    fn account_matches(&self, status: &Status) -> bool {
        status.telemetry.as_ref().is_some_and(|t| {
            t.connected
                && t.login == self.inner.settings.login
                && t.server == self.inner.settings.server
                && t.currency == self.inner.settings.currency
                && t.account_trade_mode
                    == Some(if self.inner.settings.target == "demo" { 0 } else { 2 })
        })
    }

    /// Global broker execution is armed by START + a fresh bound account.
    /// Secondary-brain SOLO switches are optional overrides and are not the master arm.
    pub fn enabled(&self, status: &Status) -> bool {
        status.requested_run && status.gateway_fresh && self.account_matches(status)
    }

    pub fn pending(&self, db: &Connection) -> Result<bool> {
        self.inner.pending(db)
    }

    pub fn cancel_entries(&self, db: &Connection) -> Result<()> {
        self.inner.cancel_entries(db)
    }

    pub fn observe(
        &mut self,
        db: &Connection,
        telemetry: &Telemetry,
        config: &Config,
        run: bool,
        frames: &BTreeMap<String, Vec<FrameBar>>,
        now: u64,
    ) -> Result<()> {
        if let Some(system) = self.inner.brains.iter_mut().find(|brain| brain.id == "system") {
            system.enabled = true;
        }
        // Legacy engine still owns durable broker intents/reconciliation. With `system`
        // forced on, it represents coordinated execution; every other `enabled` flag now
        // means SOLO override only.
        self.inner.observe(db, telemetry, config, run, frames, now)
    }

    fn enrich_roles(mut value: Value) -> Value {
        if let Some(brains) = value.get_mut("brains").and_then(Value::as_array_mut) {
            for brain in brains {
                let id = brain["config"]["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let solo = id != "system" && brain["config"]["enabled"] == true;
                brain["role"] = json!(if id == "system" {
                    "coordinator"
                } else if solo {
                    "solo"
                } else {
                    "advisor"
                });
                brain["solo_enabled"] = json!(solo);
                brain["participates_in_coordinator"] = json!(id != "system" && !solo);
                if id == "system" {
                    brain["config"]["enabled"] = json!(true);
                }
            }
        }
        value["execution_model"] = json!("coordinator-with-solo-overrides-v1");
        value["note"] = json!("العقل الرئيسي يعمل مع START. العقول الثانوية تشارك كمستشارين افتراضياً؛ تفعيل SOLO يفصل العقل المختار فقط، وإطفاؤه يعيده للمنظومة الجماعية. صفقات SOLO تبقى مملوكة للعقل عبر Magic مستقل.");
        value
    }

    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        db: &Connection,
        status: &Status,
        now: u64,
    ) -> Result<Value> {
        // System is not a SOLO strategy. Preserve its numeric settings if the UI saves
        // them, but force the coordinator bit on and let global START/STOP own activation.
        if method == "POST" && path == "/v1/execution/brain" {
            let mut config: BrainConfig = serde_json::from_slice(body)?;
            if config.id == "system" {
                config.enabled = true;
                let encoded = serde_json::to_vec(&config)?;
                let mut result = self.inner.route(method, path, &encoded, db, status, now)?;
                result["start"] = json!(false);
                result["role"] = json!("coordinator");
                result["note"] = json!("العقل الرئيسي يتبع START/STOP ولا يحتاج زر تداول منفرد.");
                return Ok(result);
            }
        }

        let value = self.inner.route(method, path, body, db, status, now)?;
        if method == "GET" && path == "/v1/execution" {
            Ok(Self::enrich_roles(value))
        } else {
            Ok(value)
        }
    }
}
