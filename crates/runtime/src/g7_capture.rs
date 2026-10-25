//! G7 passive, locally attested coordinator-pair evidence.
//! There is NO broker route in this module. Its pairs are decision-only,
//! not executed Frozen vs Adaptive trials, booked P&L, or independent OOS.
use anyhow::{ensure, Result};
use braintrade_domain::{execution::Signal, Telemetry};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const PROTOCOL: &str = "g7-local-paired-decisions-v1";

fn hash(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS g7_pair_registrations(
            run_id TEXT PRIMARY KEY,
            context TEXT NOT NULL,
            mode TEXT NOT NULL,
            registered_ms INTEGER NOT NULL,
            cohort_start_ms INTEGER NOT NULL,
            frozen_snapshot_sha256 TEXT NOT NULL,
            protocol TEXT NOT NULL,
            FOREIGN KEY(run_id) REFERENCES frozen_coordinator_runs(id));
         CREATE TABLE IF NOT EXISTS g7_paired_decision_evidence(
            run_id TEXT NOT NULL,
            bar_time INTEGER NOT NULL,
            observed_ms INTEGER NOT NULL,
            payload TEXT NOT NULL,
            payload_sha256 TEXT NOT NULL,
            PRIMARY KEY(run_id,bar_time),
            FOREIGN KEY(run_id) REFERENCES g7_pair_registrations(run_id));",
    )?;
    Ok(())
}

/// Call in the SAME SQLite transaction as creation of the frozen run,
/// before any post-freeze completed bar can be observed.
pub fn preregister(
    db: &Connection,
    run_id: &str,
    context: &str,
    mode: &str,
    frozen_snapshot: &str,
    registered_ms: u64,
) -> Result<()> {
    ensure!(!run_id.is_empty() && !context.is_empty() && !mode.is_empty(), "missing G7 identity");
    let start = registered_ms.checked_add(1).ok_or_else(|| anyhow::anyhow!("G7 clock overflow"))?;
    db.execute(
        "INSERT INTO g7_pair_registrations
        (run_id,context,mode,registered_ms,cohort_start_ms,frozen_snapshot_sha256,protocol)
        VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![run_id, context, mode, registered_ms, start, hash(frozen_snapshot), PROTOCOL],
    )?;
    Ok(())
}

/// Call in the SAME transaction as frozen_coordinator_decisions insertion.
/// Fail closed on missing/changed run snapshot. An error is isolated by
/// execution::Service::observe and cannot block broker exits or entries.
#[allow(clippy::too_many_arguments)]
pub fn record_pair(
    db: &Connection,
    run_id: &str,
    context: &str,
    mode: &str,
    frozen_snapshot: &str,
    bar_time: u64,
    observed_ms: u64,
    t: &Telemetry,
    frozen: &Signal,
    adaptive: &Signal,
) -> Result<()> {
    let registered: Option<(String,String,u64,u64,String,String)> = db.query_row(
        "SELECT context,mode,registered_ms,cohort_start_ms,frozen_snapshot_sha256,protocol
         FROM g7_pair_registrations WHERE run_id=?1",
        [run_id],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)),
    ).optional()?;
    let (saved_context,saved_mode,registered_ms,start,expected_hash,protocol) =
        registered.ok_or_else(|| anyhow::anyhow!("G7 registration missing"))?;
    ensure!(protocol == PROTOCOL && saved_context == context && saved_mode == mode,
        "G7 protocol/account/mode mismatch");
    ensure!(hash(frozen_snapshot) == expected_hash, "G7 frozen checkpoint changed");
    ensure!(observed_ms >= start && bar_time.saturating_mul(1000) > registered_ms,
        "G7 nonprospective decision");
    ensure!(t.connected && t.demo && t.account_trade_mode == Some(0),
        "G7 evidence must be Demo");
    ensure!(crate::coordinator_learning::context(t) == context, "G7 broker identity changed");
    let quote_ok = t.bid.zip(t.ask).is_some_and(|(b,a)|
        b.is_finite() && a.is_finite() && b > 0.0 && a >= b)
        && t.quote_time_ms.is_some_and(|ms| ms <= observed_ms
            && observed_ms - ms <= 3_000);
    let coverage = if quote_ok { "local_quote_observed" } else { "quote_missing_or_stale" };
    let receipt: Value = json!({
        "protocol": PROTOCOL, "run_id":run_id, "context":context, "mode":mode,
        "bar_time":bar_time, "observed_ms":observed_ms,
        "registered_ms":registered_ms, "cohort_start_ms":start,
        "frozen_checkpoint_sha256":expected_hash,
        "adaptive_checkpoint_source":"observed_signal_only_not_pre_frozen",
        "frozen_signal_sha256":hash(&serde_json::to_string(frozen)?),
        "adaptive_signal_sha256":hash(&serde_json::to_string(adaptive)?),
        "frozen":frozen, "adaptive":adaptive,
        "quote":{"bid":t.bid,"ask":t.ask,"quote_time_ms":t.quote_time_ms,
                 "tick_age_ms":t.tick_age_ms},
        "coverage":coverage,
        "source":"Core/frozen_coordinator.observe",
        "execution":"decision_only_neither_arm_broker_executed",
        "booked_pnl":Value::Null
    });
    let payload = serde_json::to_string(&receipt)?;
    db.execute(
        "INSERT INTO g7_paired_decision_evidence
        (run_id,bar_time,observed_ms,payload,payload_sha256)
        VALUES (?1,?2,?3,?4,?5)",
        params![run_id,bar_time,observed_ms,payload,hash(&payload)],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{Config, Telemetry};
    use crate::frozen_coordinator;

    fn setup() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        frozen_coordinator::init(&db).unwrap();
        db
    }

    #[test]
    fn registration_precedes_events_and_is_unique() {
        let db = setup();
        db.execute("INSERT INTO frozen_coordinator_runs VALUES (?1,?2,?3,?4,?5,1)",
            params!["id1","ctx","expert",1000,"{}"]).unwrap();
        preregister(&db,"id1","ctx","expert","{}",1000).unwrap();
        assert!(preregister(&db,"id1","ctx","expert","{}",1000).is_err());
        let start:u64=db.query_row(
            "SELECT cohort_start_ms FROM g7_pair_registrations WHERE run_id='id1'",[],
            |r|r.get(0)).unwrap();
        assert_eq!(start,1001);
    }

    #[test]
    fn tampered_freeze_is_rejected_before_any_pair() {
        let db=setup();
        let t=Telemetry::default();
        let sig=braintrade_domain::execution::Signal {
            brain:"system".into(), side:1,key:"key".into(),reason:"test".into(),
            ready:true,stop_distance:1.,target_distance:2.,hold_ms:10_000,
            coordination:None,
        };
        db.execute("INSERT INTO frozen_coordinator_runs VALUES (?1,?2,?3,?4,?5,1)",
            params!["id2","ctx","expert",1000,"{}"]).unwrap();
        preregister(&db,"id2","ctx","expert","{}",1000).unwrap();
        assert!(record_pair(&db,"id2","ctx","expert","{\"modified\":true}",
            2,2000,&t,&sig,&sig).is_err());
        let n:i64=db.query_row("SELECT count(*) FROM g7_paired_decision_evidence",[],|r|r.get(0)).unwrap();
        assert_eq!(n,0);
    }
}
