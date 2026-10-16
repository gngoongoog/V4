//! Persisted holdout-only OOS evidence derived from immutable chronological evaluations.
//!
//! Research-only. No inference, automatic promotion, live signal, risk or broker-execution path
//! references this module.
use anyhow::{bail, Context, Result};
use braintrade_domain::{evaluation::ChronologicalEvaluation, oos::{self, OosEvidence}};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

const VERSION: &str = "oos-runtime-v1";
const PREFIX: &str = "oos-v1-";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationSelection {
    evaluation_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSelection {
    id: String,
}

#[derive(Clone)]
struct Source {
    evaluation_id: String,
    source_run_id: String,
    evaluation: ChronologicalEvaluation,
    canonical: String,
}

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS oos_evidence(
            id TEXT PRIMARY KEY,
            source_evaluation_id TEXT NOT NULL UNIQUE,
            source_run_id TEXT NOT NULL,
            created_ms INTEGER NOT NULL,
            source_evaluation TEXT NOT NULL,
            evidence TEXT NOT NULL,
            FOREIGN KEY(source_evaluation_id) REFERENCES chronological_evaluations(id) ON DELETE RESTRICT
        );
        CREATE INDEX IF NOT EXISTS oos_evidence_created_idx
            ON oos_evidence(created_ms DESC,id DESC);",
    )?;
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn source(db: &Connection, evaluation_id: &str) -> Result<Source> {
    if !valid_id(evaluation_id) {
        bail!("معرّف التقييم الزمني غير صالح");
    }
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT source_run_id,evaluation FROM chronological_evaluations WHERE id=?1",
            [evaluation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (source_run_id, raw) = row.context("دليل التقييم الزمني المصدر غير موجود")?;
    let evaluation: ChronologicalEvaluation = serde_json::from_str(&raw)
        .context("تعذر قراءة دليل التقييم الزمني المصدر")?;
    let canonical = serde_json::to_string(&evaluation)?;
    Ok(Source {
        evaluation_id: evaluation_id.into(),
        source_run_id,
        evaluation,
        canonical,
    })
}

fn deterministic_id(evaluation_id: &str) -> String {
    format!("{PREFIX}{evaluation_id}")
}

fn persist(db: &Connection, source: &Source, evidence: &OosEvidence, now: u64) -> Result<String> {
    let id = deterministic_id(&source.evaluation_id);
    let evidence_json = serde_json::to_string(evidence)?;
    let existing: Option<(String, String, String)> = db
        .query_row(
            "SELECT source_run_id,source_evaluation,evidence FROM oos_evidence WHERE id=?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((run_id, source_snapshot, saved_evidence)) = existing {
        if run_id != source.source_run_id
            || source_snapshot != source.canonical
            || saved_evidence != evidence_json
        {
            bail!("OOS evidence identity collision or mutated source detected");
        }
        return Ok(id);
    }
    db.execute(
        "INSERT INTO oos_evidence(id,source_evaluation_id,source_run_id,created_ms,source_evaluation,evidence)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            id,
            source.evaluation_id,
            source.source_run_id,
            now,
            source.canonical,
            evidence_json
        ],
    )?;
    Ok(deterministic_id(&source.evaluation_id))
}

fn analyze(db: &Connection, evaluation_id: &str, now: u64) -> Result<Value> {
    let source = source(db, evaluation_id)?;
    let evidence = oos::summarize(&source.evaluation).map_err(anyhow::Error::msg)?;
    let id = persist(db, &source, &evidence, now)?;
    Ok(json!({
        "ok": true,
        "id": id,
        "source_evaluation_id": source.evaluation_id,
        "source_run_id": source.source_run_id,
        "evidence": evidence,
        "immutable": true,
        "research_only": true,
        "automatic_promotion": false,
        "broker_effect": false
    }))
}

fn load(db: &Connection, id: &str) -> Result<(String, String, u64, String, OosEvidence)> {
    if !valid_id(id) || !id.starts_with(PREFIX) {
        bail!("معرّف OOS evidence غير صالح");
    }
    let row: Option<(String, String, u64, String, String)> = db
        .query_row(
            "SELECT source_evaluation_id,source_run_id,created_ms,source_evaluation,evidence
             FROM oos_evidence WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?;
    let (source_evaluation_id, source_run_id, created_ms, source_snapshot, raw) =
        row.context("OOS evidence غير موجود")?;
    let evidence: OosEvidence = serde_json::from_str(&raw)?;
    Ok((source_evaluation_id, source_run_id, created_ms, source_snapshot, evidence))
}

fn report(db: &Connection, id: &str) -> Result<Value> {
    let (source_evaluation_id, source_run_id, created_ms, _, evidence) = load(db, id)?;
    Ok(json!({
        "id": id,
        "source_evaluation_id": source_evaluation_id,
        "source_run_id": source_run_id,
        "created_ms": created_ms,
        "evidence": evidence,
        "immutable": true,
        "research_only": true,
        "automatic_promotion": false,
        "broker_effect": false
    }))
}

fn replay(db: &Connection, id: &str) -> Result<Value> {
    let (source_evaluation_id, source_run_id, created_ms, source_snapshot, saved) = load(db, id)?;
    let current = source(db, &source_evaluation_id)?;
    if current.source_run_id != source_run_id || current.canonical != source_snapshot {
        bail!("OOS source evaluation changed after evidence was saved");
    }
    let recomputed = oos::summarize(&current.evaluation).map_err(anyhow::Error::msg)?;
    if recomputed != saved {
        bail!("OOS replay mismatch; saved evidence is not reproducible from its source");
    }
    Ok(json!({
        "ok": true,
        "id": id,
        "source_evaluation_id": source_evaluation_id,
        "source_run_id": source_run_id,
        "created_ms": created_ms,
        "evidence": recomputed,
        "replay_verified": true,
        "research_only": true,
        "automatic_promotion": false,
        "broker_effect": false
    }))
}

fn sources(db: &Connection) -> Result<Value> {
    let mut statement = db.prepare(
        "SELECT id,source_run_id,created_ms,evaluation FROM chronological_evaluations
         ORDER BY created_ms DESC,id DESC LIMIT 50",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let values = rows
        .into_iter()
        .filter_map(|(id, source_run_id, created_ms, raw)| {
            let evaluation: ChronologicalEvaluation = serde_json::from_str(&raw).ok()?;
            let holdout_windows = evaluation
                .windows
                .iter()
                .filter(|row| row.window.role == braintrade_domain::evaluation::WindowRole::Holdout)
                .count();
            Some(json!({
                "id": id,
                "source_run_id": source_run_id,
                "created_ms": created_ms,
                "symbol": evaluation.symbol,
                "server": evaluation.server,
                "login": evaluation.login,
                "plan": evaluation.plan,
                "holdout_windows": holdout_windows,
                "eligible": holdout_windows > 0
            }))
        })
        .collect::<Vec<_>>();
    Ok(json!({"version": VERSION, "sources": values, "research_only": true}))
}

fn reports(db: &Connection) -> Result<Value> {
    let mut statement = db.prepare(
        "SELECT id,source_evaluation_id,source_run_id,created_ms,evidence FROM oos_evidence
         ORDER BY created_ms DESC,id DESC LIMIT 50",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let values = rows
        .into_iter()
        .filter_map(|(id, evaluation_id, run_id, created_ms, raw)| {
            let evidence: OosEvidence = serde_json::from_str(&raw).ok()?;
            Some(json!({
                "id": id,
                "source_evaluation_id": evaluation_id,
                "source_run_id": run_id,
                "created_ms": created_ms,
                "symbol": evidence.symbol,
                "holdout_windows": evidence.holdout_windows,
                "meta_v1_net_sum": evidence.meta_v1.net_sum,
                "meta_v2_net_sum": evidence.meta_v2.net_sum,
                "average_net_v2_minus_v1": evidence.delta.average_net_v2_minus_v1
            }))
        })
        .collect::<Vec<_>>();
    Ok(json!({"version": VERSION, "reports": values, "research_only": true}))
}

pub fn state(db: &Connection) -> Result<Value> {
    let latest: Option<String> = db
        .query_row(
            "SELECT id FROM oos_evidence ORDER BY created_ms DESC,id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(json!({
        "version": VERSION,
        "latest_id": latest,
        "research_only": true,
        "automatic_promotion": false,
        "broker_effect": false,
        "message": "Holdout-only OOS evidence derived from saved chronological evaluations"
    }))
}

pub fn route(method: &str, path: &str, body: &[u8], db: &Connection) -> Result<Value> {
    match (method, path) {
        ("GET", "/v1/research/oos") => state(db),
        ("GET", "/v1/research/oos/sources") => sources(db),
        ("GET", "/v1/research/oos/reports") => reports(db),
        ("POST", "/v1/research/oos/analyze") => {
            let request: EvaluationSelection = serde_json::from_slice(body)?;
            analyze(db, &request.evaluation_id, crate::now_ms())
        }
        ("POST", "/v1/research/oos/report") => {
            let request: ReportSelection = serde_json::from_slice(body)?;
            report(db, &request.id)
        }
        ("POST", "/v1/research/oos/replay") => {
            let request: ReportSelection = serde_json::from_slice(body)?;
            replay(db, &request.id)
        }
        _ => bail!("Unsupported OOS research route"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{
        backtest::{Metrics, Options, Strategy},
        evaluation::{
            ChronologicalEvaluation, EvaluationPlan, EvaluationWindow, StrategyAggregate,
            StrategyWindowResult, WindowEvaluation, WindowRole,
        },
        RiskConfig,
    };

    fn metrics(net: f64) -> Metrics {
        Metrics {
            trades: 5,
            wins: 3,
            losses: 2,
            breakeven: 0,
            net,
            return_percent: net / 100.0,
            profit_factor: Some(1.2),
            win_rate: Some(60.0),
            max_drawdown: 100.0,
            max_drawdown_percent: 1.0,
            commissions: 2.0,
            ambiguous_trades: 0,
            gap_exits: 0,
            tested_bars: 100,
            skipped: Default::default(),
        }
    }

    fn result(strategy: Strategy, net: f64) -> StrategyWindowResult {
        StrategyWindowResult {
            strategy,
            algorithm: strategy.algorithm().into(),
            metrics: metrics(net),
            first_test_bar: 60,
            last_test_bar: 119,
            gaps: 0,
        }
    }

    fn fixture() -> ChronologicalEvaluation {
        let dev = WindowEvaluation {
            window: EvaluationWindow { cycle: 0, role: WindowRole::Development, from: 60, to: 120 },
            meta_v1: result(Strategy::MetaV1, 999.0),
            meta_v2: result(Strategy::MetaV2, -999.0),
            delta_net_v2_minus_v1: -1998.0,
            delta_return_percent_v2_minus_v1: -19.98,
            delta_drawdown_percent_v2_minus_v1: 0.0,
        };
        let holdout = WindowEvaluation {
            window: EvaluationWindow { cycle: 0, role: WindowRole::Holdout, from: 120, to: 180 },
            meta_v1: result(Strategy::MetaV1, 10.0),
            meta_v2: result(Strategy::MetaV2, 12.0),
            delta_net_v2_minus_v1: 2.0,
            delta_return_percent_v2_minus_v1: 0.02,
            delta_drawdown_percent_v2_minus_v1: 0.0,
        };
        ChronologicalEvaluation {
            version: "chronological-evaluation-v1".into(),
            plan: EvaluationPlan::Holdout { holdout_percent: 30 },
            symbol: "XAUUSD".into(),
            server: "Demo".into(),
            login: 7,
            currency: "USD".into(),
            captured_ms: 1000,
            history_first: 1,
            history_last: 1000,
            history_bars: 100,
            options_template: Options {
                strategy: Strategy::MetaV1,
                from: 60,
                to: 180,
                initial_equity: 10_000.0,
                spread_floor: 0.02,
                risk: RiskConfig::default(),
            },
            windows: vec![dev, holdout],
            meta_v1: StrategyAggregate::default(),
            meta_v2: StrategyAggregate::default(),
            average_delta_net_v2_minus_v1: 0.0,
            average_delta_return_percent_v2_minus_v1: 0.0,
            note: String::new(),
        }
    }

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE chronological_evaluations(
                id TEXT PRIMARY KEY,
                source_run_id TEXT NOT NULL,
                created_ms INTEGER NOT NULL,
                plan TEXT NOT NULL,
                source_identity TEXT NOT NULL,
                evaluation TEXT NOT NULL
             );",
        ).unwrap();
        init(&db).unwrap();
        db.execute(
            "INSERT INTO chronological_evaluations(id,source_run_id,created_ms,plan,source_identity,evaluation)
             VALUES('eval-1','backtest-1',1,'{}','{}',?1)",
            [serde_json::to_string(&fixture()).unwrap()],
        ).unwrap();
        db
    }

    #[test]
    fn analysis_is_deterministic_and_deduplicated() {
        let db = database();
        let first = analyze(&db, "eval-1", 10).unwrap();
        let second = analyze(&db, "eval-1", 999).unwrap();
        assert_eq!(first["id"], second["id"]);
        let count: usize = db.query_row("SELECT COUNT(*) FROM oos_evidence", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        assert_eq!(first["evidence"]["meta_v1"]["net_sum"], 10.0);
        assert_eq!(first["evidence"]["meta_v2"]["net_sum"], 12.0);
    }

    #[test]
    fn replay_detects_mutated_source() {
        let db = database();
        let saved = analyze(&db, "eval-1", 10).unwrap();
        assert!(replay(&db, saved["id"].as_str().unwrap()).unwrap()["replay_verified"].as_bool().unwrap());
        let mut changed = fixture();
        changed.windows[1].meta_v2.metrics.net += 50.0;
        db.execute(
            "UPDATE chronological_evaluations SET evaluation=?1 WHERE id='eval-1'",
            [serde_json::to_string(&changed).unwrap()],
        ).unwrap();
        assert!(replay(&db, saved["id"].as_str().unwrap()).is_err());
    }

    #[test]
    fn evidence_survives_database_reopen() {
        let path = std::env::temp_dir().join(format!("braintrade-oos-{}.sqlite", uuid::Uuid::new_v4()));
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE chronological_evaluations(
                    id TEXT PRIMARY KEY,
                    source_run_id TEXT NOT NULL,
                    created_ms INTEGER NOT NULL,
                    plan TEXT NOT NULL,
                    source_identity TEXT NOT NULL,
                    evaluation TEXT NOT NULL
                 );",
            ).unwrap();
            init(&db).unwrap();
            db.execute(
                "INSERT INTO chronological_evaluations(id,source_run_id,created_ms,plan,source_identity,evaluation)
                 VALUES('eval-1','backtest-1',1,'{}','{}',?1)",
                [serde_json::to_string(&fixture()).unwrap()],
            ).unwrap();
            analyze(&db, "eval-1", 10).unwrap();
        }
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            init(&db).unwrap();
            assert!(replay(&db, "oos-v1-eval-1").unwrap()["replay_verified"].as_bool().unwrap());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn routes_expose_research_only_boundaries() {
        let db = database();
        let state = route("GET", "/v1/research/oos", b"", &db).unwrap();
        assert_eq!(state["research_only"], true);
        assert_eq!(state["automatic_promotion"], false);
        assert_eq!(state["broker_effect"], false);
        let analyzed = route(
            "POST",
            "/v1/research/oos/analyze",
            br#"{"evaluation_id":"eval-1"}"#,
            &db,
        ).unwrap();
        assert_eq!(analyzed["broker_effect"], false);
        assert_eq!(route("GET", "/v1/research/oos/reports", b"", &db).unwrap()["reports"].as_array().unwrap().len(), 1);
    }
}
