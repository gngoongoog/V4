use anyhow::{bail, Context, Result};
use braintrade_domain::{
    backtest::History,
    correlation::{self, CorrelationConfig, Input, Report},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

const MAX_SOURCES: usize = 8;
const MAX_LISTED_BACKTESTS: i64 = 100;
const MAX_LISTED_REPORTS: i64 = 50;
const EXPORT_FORMAT: &str = "correlation-export-v1";
const REPLAY_FLOAT_TOLERANCE: f64 = 1e-12;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalyzeRequest {
    source_ids: Vec<String>,
    #[serde(default = "default_min_overlap")]
    min_overlap: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
}

struct SavedSource {
    id: String,
    from: u64,
    to: u64,
    history: History,
}

struct StoredReport {
    created_ms: u64,
    report: Report,
    source_ids: Vec<String>,
}

fn default_min_overlap() -> usize {
    CorrelationConfig::default().min_overlap
}

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS correlation_reports(
            id TEXT PRIMARY KEY,
            created_ms INTEGER NOT NULL,
            min_overlap INTEGER NOT NULL,
            report TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS correlation_sources(
            report_id TEXT NOT NULL REFERENCES correlation_reports(id) ON DELETE CASCADE,
            ordinal INTEGER NOT NULL,
            backtest_id TEXT NOT NULL REFERENCES backtest_runs(id),
            PRIMARY KEY(report_id,ordinal),
            UNIQUE(report_id,backtest_id)
        );
        CREATE INDEX IF NOT EXISTS correlation_reports_created_idx
            ON correlation_reports(created_ms DESC,id DESC);
        CREATE INDEX IF NOT EXISTS correlation_sources_backtest_idx
            ON correlation_sources(backtest_id);",
    )?;
    Ok(())
}

fn validate_id(id: &str, label: &str) -> Result<()> {
    if id.trim().is_empty() || id.len() > 160 {
        bail!("{label} is empty or too long");
    }
    Ok(())
}

fn load_sources(db: &Connection, source_ids: &[String]) -> Result<Vec<SavedSource>> {
    if !(2..=MAX_SOURCES).contains(&source_ids.len()) {
        bail!("Correlation research requires 2..={MAX_SOURCES} saved backtests");
    }
    let mut unique = BTreeSet::new();
    let mut sources = Vec::with_capacity(source_ids.len());
    for id in source_ids {
        validate_id(id, "Backtest source ID")?;
        if !unique.insert(id.as_str()) {
            bail!("Duplicate backtest source ID");
        }
        let row: Option<(u64, u64, String)> = db
            .query_row(
                "SELECT json_extract(report,'$.options.from'),
                        json_extract(report,'$.options.to'),
                        history
                   FROM backtest_runs WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (from, to, history_json) = row.with_context(|| format!("Saved backtest not found: {id}"))?;
        let history: History = serde_json::from_str(&history_json)
            .with_context(|| format!("Saved backtest history is invalid: {id}"))?;
        history.validate().map_err(anyhow::Error::msg)?;
        sources.push(SavedSource { id: id.clone(), from, to, history });
    }
    Ok(sources)
}

fn calculate(saved: &[SavedSource], min_overlap: usize) -> Result<Report> {
    let config = CorrelationConfig { min_overlap };
    config.validate().map_err(anyhow::Error::msg)?;
    let inputs = saved
        .iter()
        .map(|source| Input {
            source_id: &source.id,
            from: source.from,
            to: source.to,
            history: &source.history,
        })
        .collect::<Vec<_>>();
    let report = correlation::analyze(inputs, config).map_err(anyhow::Error::msg)?;
    if report.execution_effect || report.forward_fill {
        bail!("Correlation research boundary violation");
    }
    Ok(report)
}

fn nearly_equal(left: f64, right: f64) -> bool {
    if left == right {
        return true;
    }
    if !left.is_finite() || !right.is_finite() {
        return false;
    }
    let scale = 1.0f64.max(left.abs()).max(right.abs());
    (left - right).abs() <= REPLAY_FLOAT_TOLERANCE * scale
}

fn optional_nearly_equal(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => nearly_equal(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn verify_replay(stored: &Report, replayed: &Report) -> Result<()> {
    if stored.algorithm != correlation::ALGORITHM || replayed.algorithm != correlation::ALGORITHM {
        bail!("Correlation deterministic replay mismatch: algorithm changed");
    }
    if stored.identity_algorithm != correlation::IDENTITY_ALGORITHM
        || replayed.identity_algorithm != correlation::IDENTITY_ALGORITHM
    {
        bail!("Correlation deterministic replay mismatch: identity algorithm changed");
    }
    if stored.identity != replayed.identity {
        bail!("Correlation deterministic replay mismatch: input identity changed");
    }
    if stored.from != replayed.from
        || stored.to != replayed.to
        || stored.min_overlap != replayed.min_overlap
        || stored.sources != replayed.sources
        || stored.execution_effect != replayed.execution_effect
        || stored.forward_fill != replayed.forward_fill
    {
        bail!("Correlation deterministic replay mismatch: report structure changed");
    }
    if stored.execution_effect || stored.forward_fill {
        bail!("Correlation deterministic replay mismatch: research boundary changed");
    }
    if stored.minutes.len() != replayed.minutes.len() {
        bail!("Correlation deterministic replay mismatch: aligned minute count changed");
    }
    for (stored_minute, replayed_minute) in stored.minutes.iter().zip(&replayed.minutes) {
        if stored_minute.time != replayed_minute.time
            || stored_minute.available != replayed_minute.available
            || stored_minute.returns.len() != replayed_minute.returns.len()
        {
            bail!("Correlation deterministic replay mismatch: aligned minute structure changed");
        }
        if stored_minute
            .returns
            .iter()
            .zip(&replayed_minute.returns)
            .any(|(left, right)| !optional_nearly_equal(*left, *right))
        {
            bail!("Correlation deterministic replay mismatch: aligned return changed beyond tolerance");
        }
    }
    if stored.pairs.len() != replayed.pairs.len() {
        bail!("Correlation deterministic replay mismatch: pair count changed");
    }
    for (stored_pair, replayed_pair) in stored.pairs.iter().zip(&replayed.pairs) {
        if stored_pair.left != replayed_pair.left
            || stored_pair.right != replayed_pair.right
            || stored_pair.overlap != replayed_pair.overlap
            || stored_pair.available != replayed_pair.available
            || stored_pair.reason != replayed_pair.reason
            || !optional_nearly_equal(stored_pair.correlation, replayed_pair.correlation)
            || !optional_nearly_equal(stored_pair.covariance, replayed_pair.covariance)
            || !optional_nearly_equal(stored_pair.left_stddev, replayed_pair.left_stddev)
            || !optional_nearly_equal(stored_pair.right_stddev, replayed_pair.right_stddev)
        {
            bail!("Correlation deterministic replay mismatch: pair statistics changed");
        }
    }
    let stored_portfolio = &stored.portfolio;
    let replayed_portfolio = &replayed.portfolio;
    if stored_portfolio.overlap != replayed_portfolio.overlap
        || stored_portfolio.available != replayed_portfolio.available
        || stored_portfolio.reason != replayed_portfolio.reason
        || !optional_nearly_equal(stored_portfolio.volatility, replayed_portfolio.volatility)
        || !optional_nearly_equal(
            stored_portfolio.diversification_ratio,
            replayed_portfolio.diversification_ratio,
        )
    {
        bail!("Correlation deterministic replay mismatch: portfolio statistics changed");
    }
    match (&stored_portfolio.covariance, &replayed_portfolio.covariance) {
        (None, None) => {}
        (Some(stored_matrix), Some(replayed_matrix)) => {
            if stored_matrix.len() != replayed_matrix.len() {
                bail!("Correlation deterministic replay mismatch: portfolio covariance shape changed");
            }
            for (stored_row, replayed_row) in stored_matrix.iter().zip(replayed_matrix) {
                if stored_row.len() != replayed_row.len()
                    || stored_row
                        .iter()
                        .zip(replayed_row)
                        .any(|(left, right)| !nearly_equal(*left, *right))
                {
                    bail!("Correlation deterministic replay mismatch: portfolio covariance changed");
                }
            }
        }
        _ => bail!("Correlation deterministic replay mismatch: portfolio covariance availability changed"),
    }
    Ok(())
}

fn summarize(report: &Report) -> Value {
    let symbols = report.sources.iter().map(|source| source.symbol.clone()).collect::<Vec<_>>();
    let available_pairs = report.pairs.iter().filter(|pair| pair.available).count();
    json!({
        "id": report.identity,
        "algorithm": report.algorithm,
        "from": report.from,
        "to": report.to,
        "min_overlap": report.min_overlap,
        "symbols": symbols,
        "pair_count": report.pairs.len(),
        "available_pairs": available_pairs,
        "portfolio": report.portfolio,
        "execution_effect": report.execution_effect,
        "forward_fill": report.forward_fill,
        "research_only": true
    })
}

fn stored_report(db: &Connection, id: &str) -> Result<StoredReport> {
    validate_id(id, "Correlation report ID")?;
    let row: Option<(u64, String)> = db
        .query_row(
            "SELECT created_ms,report FROM correlation_reports WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (created_ms, raw) = row.context("Correlation report not found")?;
    let report: Report = serde_json::from_str(&raw)
        .with_context(|| format!("Stored correlation report is invalid: {id}"))?;
    if report.identity != id {
        bail!("Stored correlation identity does not match its database key");
    }
    if report.execution_effect || report.forward_fill {
        bail!("Stored correlation report violates the research-only boundary");
    }
    let mut statement = db.prepare(
        "SELECT ordinal,backtest_id FROM correlation_sources
         WHERE report_id=?1 ORDER BY ordinal",
    )?;
    let source_ids = statement
        .query_map([id], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if source_ids.len() != report.sources.len() || source_ids.len() < 2 {
        bail!("Stored correlation provenance is incomplete");
    }
    Ok(StoredReport { created_ms, report, source_ids })
}

fn analyze(db: &Connection, body: &[u8], now_ms: u64) -> Result<Value> {
    let request: AnalyzeRequest = serde_json::from_slice(body)?;
    let saved = load_sources(db, &request.source_ids)?;
    let report = calculate(&saved, request.min_overlap)?;
    let payload = serde_json::to_string(&report)?;
    let tx = db.unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO correlation_reports(id,created_ms,min_overlap,report)
         VALUES(?1,?2,?3,?4)",
        params![report.identity, now_ms, report.min_overlap as u64, payload],
    )?;
    for (ordinal, source) in report.sources.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO correlation_sources(report_id,ordinal,backtest_id)
             VALUES(?1,?2,?3)",
            params![report.identity, ordinal as u64, source.source_id],
        )?;
    }
    tx.commit()?;
    let created_ms: u64 = db.query_row(
        "SELECT created_ms FROM correlation_reports WHERE id=?1",
        [&report.identity],
        |row| row.get(0),
    )?;
    Ok(json!({"ok":true,"created_ms":created_ms,"summary":summarize(&report)}))
}

fn sources(db: &Connection) -> Result<Value> {
    let mut statement = db.prepare(
        "SELECT id,created_ms,
                json_extract(report,'$.symbol'),
                json_extract(report,'$.options.from'),
                json_extract(report,'$.options.to'),
                json_extract(history,'$.server'),
                json_extract(history,'$.login'),
                json_extract(history,'$.currency')
           FROM backtest_runs
          ORDER BY created_ms DESC,id DESC LIMIT ?1",
    )?;
    let rows = statement
        .query_map([MAX_LISTED_BACKTESTS], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "created_ms": row.get::<_, u64>(1)?,
                "symbol": row.get::<_, String>(2)?,
                "from": row.get::<_, u64>(3)?,
                "to": row.get::<_, u64>(4)?,
                "server": row.get::<_, String>(5)?,
                "login": row.get::<_, u64>(6)?,
                "currency": row.get::<_, String>(7)?
            }))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(json!({
        "sources": rows,
        "research_only": true,
        "requires_saved_backtests": true,
        "live_feed_used": false
    }))
}

fn reports(db: &Connection) -> Result<Value> {
    let mut statement = db.prepare(
        "SELECT id,created_ms,report FROM correlation_reports
         ORDER BY created_ms DESC,id DESC LIMIT ?1",
    )?;
    let rows = statement
        .query_map([MAX_LISTED_REPORTS], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut items = Vec::with_capacity(rows.len());
    for (id, created_ms, raw) in rows {
        let report: Report = serde_json::from_str(&raw)
            .with_context(|| format!("Stored correlation report is invalid: {id}"))?;
        items.push(json!({"created_ms":created_ms,"summary":summarize(&report)}));
    }
    Ok(json!({"reports":items,"research_only":true}))
}

fn report(db: &Connection, body: &[u8]) -> Result<Value> {
    let selection: Selection = serde_json::from_slice(body)?;
    let stored = stored_report(db, &selection.id)?;
    let provenance = stored.source_ids.iter().enumerate()
        .map(|(ordinal, id)| json!({"ordinal":ordinal,"backtest_id":id}))
        .collect::<Vec<_>>();
    Ok(json!({
        "id": selection.id,
        "created_ms": stored.created_ms,
        "report": stored.report,
        "provenance": provenance,
        "research_only": true,
        "broker_effect": false
    }))
}

fn replay(db: &Connection, body: &[u8]) -> Result<Value> {
    let selection: Selection = serde_json::from_slice(body)?;
    let stored = stored_report(db, &selection.id)?;
    let saved = load_sources(db, &stored.source_ids)?;
    let replayed = calculate(&saved, stored.report.min_overlap)?;
    verify_replay(&stored.report, &replayed)?;
    Ok(json!({
        "ok": true,
        "id": stored.report.identity,
        "matches_stored_report": true,
        "summary": summarize(&replayed),
        "research_only": true,
        "broker_effect": false,
        "live_feed_used": false
    }))
}

fn csv_cell(value: &str) -> String {
    if value.contains(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn optional_number(value: Option<f64>) -> String {
    value.map(|number| number.to_string()).unwrap_or_default()
}

fn export(db: &Connection, body: &[u8]) -> Result<Value> {
    let selection: Selection = serde_json::from_slice(body)?;
    let stored = stored_report(db, &selection.id)?;
    let replayed = calculate(&load_sources(db, &stored.source_ids)?, stored.report.min_overlap)?;
    verify_replay(&stored.report, &replayed)
        .context("Correlation export refused because deterministic replay does not match the stored report")?;
    let provenance = stored.source_ids.iter().enumerate()
        .map(|(ordinal, id)| json!({"ordinal":ordinal,"backtest_id":id}))
        .collect::<Vec<_>>();
    let report_json = serde_json::to_string_pretty(&stored.report)? + "\n";
    let provenance_json = serde_json::to_string_pretty(&provenance)? + "\n";
    let mut pairs_csv = String::from("left,right,overlap,available,correlation,covariance,left_stddev,right_stddev,reason\n");
    for pair in &stored.report.pairs {
        pairs_csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            csv_cell(&pair.left), csv_cell(&pair.right), pair.overlap, pair.available,
            optional_number(pair.correlation), optional_number(pair.covariance),
            optional_number(pair.left_stddev), optional_number(pair.right_stddev),
            csv_cell(&pair.reason),
        ));
    }
    Ok(json!({
        "ok": true,
        "id": stored.report.identity,
        "format": EXPORT_FORMAT,
        "deterministic_replay_verified": true,
        "files": [
            {"name":"report.json","content_type":"application/json","content":report_json},
            {"name":"provenance.json","content_type":"application/json","content":provenance_json},
            {"name":"pairs.csv","content_type":"text/csv","content":pairs_csv}
        ],
        "research_only": true,
        "broker_effect": false,
        "live_feed_used": false
    }))
}

fn state(db: &Connection) -> Result<Value> {
    let reports: u64 = db.query_row("SELECT COUNT(*) FROM correlation_reports", [], |row| row.get(0))?;
    let sources: u64 = db.query_row("SELECT COUNT(*) FROM backtest_runs", [], |row| row.get(0))?;
    Ok(json!({
        "algorithm": correlation::ALGORITHM,
        "saved_reports": reports,
        "saved_backtests": sources,
        "research_only": true,
        "execution_effect": false,
        "forward_fill": false,
        "live_feed_used": false,
        "deterministic_replay": true,
        "export_format": EXPORT_FORMAT
    }))
}

pub fn route(method: &str, path: &str, body: &[u8], db: &Connection, now_ms: u64) -> Result<Value> {
    match (method, path) {
        ("GET", "/v1/research/correlation") => state(db),
        ("GET", "/v1/research/correlation/sources") => sources(db),
        ("GET", "/v1/research/correlation/reports") => reports(db),
        ("POST", "/v1/research/correlation/analyze") => analyze(db, body, now_ms),
        ("POST", "/v1/research/correlation/report") => report(db, body),
        ("POST", "/v1/research/correlation/replay") => replay(db, body),
        ("POST", "/v1/research/correlation/export") => export(db, body),
        _ => bail!("Unknown correlation research route"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{
        backtest::{Contract, HistoryBar},
        Candle,
    };

    const T0: u64 = 1_800_000_000;

    fn contract() -> Contract {
        Contract {
            point: 0.01,
            tick_size: 0.01,
            tick_loss: 1.0,
            tick_profit: 1.0,
            volume_min: 0.01,
            volume_step: 0.01,
            volume_max: 100.0,
        }
    }

    fn history(symbol: &str, login: u64, returns: &[f64]) -> History {
        let mut closes = vec![100.0];
        for value in returns {
            closes.push(*closes.last().unwrap() * value.exp());
        }
        let bars = closes
            .into_iter()
            .enumerate()
            .map(|(index, close)| HistoryBar {
                candle: Candle {
                    time: T0 + index as u64 * 60,
                    open: close,
                    high: close,
                    low: close,
                    close,
                    tick_volume: 100,
                },
                spread_points: 10,
            })
            .collect::<Vec<_>>();
        let last = T0 + returns.len() as u64 * 60;
        History {
            symbol: symbol.into(),
            server: "Demo".into(),
            login,
            currency: "USD".into(),
            captured_ms: (last + 60) * 1000,
            contract: contract(),
            bars,
        }
    }

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE backtest_runs(
                id TEXT PRIMARY KEY,
                created_ms INTEGER NOT NULL,
                report TEXT NOT NULL,
                history TEXT NOT NULL
             );",
        )
        .unwrap();
        init(&db).unwrap();
        db
    }

    fn file_database(path: &std::path::Path) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS backtest_runs(
                id TEXT PRIMARY KEY,
                created_ms INTEGER NOT NULL,
                report TEXT NOT NULL,
                history TEXT NOT NULL
             );",
        )
        .unwrap();
        init(&db).unwrap();
        db
    }

    fn save(db: &Connection, id: &str, symbol: &str, login: u64, returns: &[f64]) {
        let history = history(symbol, login, returns);
        let report = json!({
            "symbol": symbol,
            "options": {"from":T0,"to":T0+(returns.len() as u64+1)*60}
        });
        db.execute(
            "INSERT INTO backtest_runs(id,created_ms,report,history) VALUES(?1,?2,?3,?4)",
            params![id, 1u64, report.to_string(), serde_json::to_string(&history).unwrap()],
        )
        .unwrap();
    }

    fn analyze_fixture(db: &Connection) -> String {
        save(db, "a", "AAA", 7, &[0.01, 0.02, -0.01, 0.03]);
        save(db, "b", "BBB", 7, &[0.02, 0.04, -0.02, 0.06]);
        route(
            "POST",
            "/v1/research/correlation/analyze",
            json!({"source_ids":["a","b"],"min_overlap":4}).to_string().as_bytes(),
            db,
            100,
        )
        .unwrap()["summary"]["id"].as_str().unwrap().to_owned()
    }

    #[test]
    fn analysis_persists_report_and_exact_provenance() {
        let db = database();
        let id = analyze_fixture(&db);
        let count: u64 = db.query_row("SELECT COUNT(*) FROM correlation_reports", [], |row| row.get(0)).unwrap();
        let provenance: u64 = db.query_row("SELECT COUNT(*) FROM correlation_sources", [], |row| row.get(0)).unwrap();
        assert_eq!((count, provenance), (1, 2));
        let detail = route(
            "POST",
            "/v1/research/correlation/report",
            json!({"id":id}).to_string().as_bytes(),
            &db,
            0,
        )
        .unwrap();
        assert_eq!(detail["provenance"][0]["backtest_id"], "a");
        assert_eq!(detail["provenance"][1]["backtest_id"], "b");
        assert_eq!(detail["broker_effect"], false);
    }

    #[test]
    fn repeated_analysis_is_deduplicated_across_request_order() {
        let db = database();
        save(&db, "a", "AAA", 7, &[0.01, 0.02, -0.01, 0.03]);
        save(&db, "b", "BBB", 7, &[0.02, 0.04, -0.02, 0.06]);
        let first = route(
            "POST", "/v1/research/correlation/analyze",
            json!({"source_ids":["a","b"],"min_overlap":3}).to_string().as_bytes(), &db, 100,
        ).unwrap();
        let second = route(
            "POST", "/v1/research/correlation/analyze",
            json!({"source_ids":["b","a"],"min_overlap":3}).to_string().as_bytes(), &db, 999,
        ).unwrap();
        assert_eq!(first["summary"]["id"], second["summary"]["id"]);
        assert_eq!(second["created_ms"], 100);
        let count: u64 = db.query_row("SELECT COUNT(*) FROM correlation_reports", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn missing_source_and_mixed_account_are_rejected() {
        let db = database();
        save(&db, "a", "AAA", 7, &[0.01, 0.02, 0.03]);
        let missing = route(
            "POST", "/v1/research/correlation/analyze",
            json!({"source_ids":["a","missing"],"min_overlap":2}).to_string().as_bytes(), &db, 1,
        ).unwrap_err();
        assert!(missing.to_string().contains("not found"));
        save(&db, "b", "BBB", 8, &[0.01, 0.02, 0.03]);
        let mixed = route(
            "POST", "/v1/research/correlation/analyze",
            json!({"source_ids":["a","b"],"min_overlap":2}).to_string().as_bytes(), &db, 1,
        ).unwrap_err();
        assert!(mixed.to_string().contains("same account"));
    }

    #[test]
    fn source_listing_uses_saved_backtests_only() {
        let db = database();
        save(&db, "a", "AAA", 7, &[0.01, 0.02]);
        save(&db, "b", "BBB", 7, &[0.02, 0.01]);
        let value = route("GET", "/v1/research/correlation/sources", &[], &db, 0).unwrap();
        assert_eq!(value["sources"].as_array().unwrap().len(), 2);
        assert_eq!(value["live_feed_used"], false);
        assert_eq!(value["research_only"], true);
    }

    #[test]
    fn deterministic_replay_survives_database_restart() {
        let path = std::env::temp_dir().join(format!("braintrade-correlation-{}.sqlite", uuid::Uuid::new_v4()));
        let id = {
            let db = file_database(&path);
            analyze_fixture(&db)
        };
        {
            let db = file_database(&path);
            let result = route(
                "POST", "/v1/research/correlation/replay",
                json!({"id":id}).to_string().as_bytes(), &db, 0,
            ).unwrap();
            assert_eq!(result["matches_stored_report"], true);
            assert_eq!(result["broker_effect"], false);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn replay_detects_mutated_source_history() {
        let db = database();
        let id = analyze_fixture(&db);
        let mut changed = history("AAA", 7, &[0.01, 0.02, -0.01, 0.20]);
        changed.captured_ms += 1;
        db.execute("UPDATE backtest_runs SET history=?1 WHERE id='a'", [serde_json::to_string(&changed).unwrap()]).unwrap();
        let error = route(
            "POST", "/v1/research/correlation/replay",
            json!({"id":id}).to_string().as_bytes(), &db, 0,
        ).unwrap_err();
        assert!(error.to_string().contains("replay mismatch"));
    }

    #[test]
    fn export_is_deterministic_and_replay_verified() {
        let db = database();
        let id = analyze_fixture(&db);
        let request = json!({"id":id}).to_string();
        let first = route("POST", "/v1/research/correlation/export", request.as_bytes(), &db, 0).unwrap();
        let second = route("POST", "/v1/research/correlation/export", request.as_bytes(), &db, 999).unwrap();
        assert_eq!(first, second);
        assert_eq!(first["format"], EXPORT_FORMAT);
        assert_eq!(first["deterministic_replay_verified"], true);
        assert_eq!(first["files"][0]["name"], "report.json");
        assert_eq!(first["files"][1]["name"], "provenance.json");
        assert_eq!(first["files"][2]["name"], "pairs.csv");
        assert!(first["files"][2]["content"].as_str().unwrap().starts_with("left,right,overlap"));
        assert_eq!(first["broker_effect"], false);
    }
}
