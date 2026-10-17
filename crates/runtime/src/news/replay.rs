//! Causal historical replay for official-news observations.
//! Availability is anchored to the first time BrainTrade actually observed an item,
//! never to the publisher timestamp alone. Research only: no execution route.
use super::Headline;
use anyhow::{bail, Context, Result};
use braintrade_domain::backtest::History;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_HORIZONS: [u32; 5] = [1, 5, 15, 30, 60];
const MAX_HORIZONS: usize = 8;
const MAX_HORIZON_MINUTES: u32 = 240;
const REPORT_LIMIT: usize = 100;

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS news_observed_events(\
            source_id TEXT NOT NULL,\
            url TEXT NOT NULL,\
            title TEXT NOT NULL,\
            topic TEXT NOT NULL,\
            published_ms INTEGER,\
            first_observed_ms INTEGER NOT NULL,\
            last_observed_ms INTEGER NOT NULL,\
            PRIMARY KEY(source_id,url)\
        ) WITHOUT ROWID;\
        CREATE INDEX IF NOT EXISTS idx_news_observed_first ON news_observed_events(first_observed_ms);\
        CREATE TABLE IF NOT EXISTS news_replay_reports(\
            id TEXT PRIMARY KEY,\
            created_ms INTEGER NOT NULL,\
            source_id TEXT NOT NULL,\
            report TEXT NOT NULL\
        );\
        CREATE INDEX IF NOT EXISTS idx_news_replay_created ON news_replay_reports(created_ms DESC);",
    )?;
    Ok(())
}

pub fn observe(db: &Connection, source_id: &str, observed_ms: u64, items: &[Headline]) -> Result<()> {
    if observed_ms == 0 {
        bail!("Official-news observation time is missing");
    }
    let feed = super::FEEDS
        .iter()
        .find(|feed| feed.id == source_id)
        .context("Unknown official-news source")?;
    let observed_i64 = i64::try_from(observed_ms).context("Observation timestamp out of range")?;
    for item in items {
        if !super::safe_url(&item.url, feed.host) || item.title.trim().is_empty() {
            bail!("Invalid official-news observation");
        }
        let published = item
            .published_ms
            .map(i64::try_from)
            .transpose()
            .context("Published timestamp out of range")?;
        db.execute(
            "INSERT INTO news_observed_events(\
                source_id,url,title,topic,published_ms,first_observed_ms,last_observed_ms\
             ) VALUES(?1,?2,?3,?4,?5,?6,?6)\
             ON CONFLICT(source_id,url) DO UPDATE SET\
                title=excluded.title,\
                topic=excluded.topic,\
                published_ms=COALESCE(news_observed_events.published_ms,excluded.published_ms),\
                last_observed_ms=MAX(news_observed_events.last_observed_ms,excluded.last_observed_ms)",
            params![
                source_id,
                item.url,
                item.title,
                item.topic,
                published,
                observed_i64
            ],
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct Observation {
    source_id: String,
    url: String,
    title: String,
    topic: String,
    published_ms: Option<u64>,
    first_observed_ms: u64,
    last_observed_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalyzeRequest {
    source_id: String,
    #[serde(default)]
    horizons_minutes: Vec<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadRequest {
    id: String,
}

fn horizons(input: Vec<u32>) -> Result<Vec<u32>> {
    let values = if input.is_empty() {
        DEFAULT_HORIZONS.to_vec()
    } else {
        input
    };
    if values.len() > MAX_HORIZONS {
        bail!("Too many event-replay horizons");
    }
    let unique = values.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != values.len()
        || unique
            .iter()
            .any(|value| *value == 0 || *value > MAX_HORIZON_MINUTES)
    {
        bail!("Event-replay horizons must be unique and between 1 and 240 minutes");
    }
    Ok(unique.into_iter().collect())
}

fn load_history(db: &Connection, id: &str) -> Result<History> {
    if id.trim().is_empty() || id.len() > 128 {
        bail!("Saved backtest source id is required");
    }
    let raw: Option<String> = db
        .query_row(
            "SELECT history FROM backtest_runs WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    let raw = raw.context("Saved backtest source not found")?;
    let history: History = serde_json::from_str(&raw).context("Saved backtest history is invalid")?;
    history.validate().map_err(anyhow::Error::msg)?;
    Ok(history)
}

fn load_observations(db: &Connection, min_ms: u64, max_ms: u64) -> Result<Vec<Observation>> {
    let min_ms = i64::try_from(min_ms).context("Replay start out of range")?;
    let max_ms = i64::try_from(max_ms).context("Replay end out of range")?;
    let mut statement = db.prepare(
        "SELECT source_id,url,title,topic,published_ms,first_observed_ms,last_observed_ms\
         FROM news_observed_events\
         WHERE first_observed_ms>=?1 AND first_observed_ms<?2\
         ORDER BY first_observed_ms,source_id,url",
    )?;
    let rows = statement.query_map(params![min_ms, max_ms], |row| {
        Ok(Observation {
            source_id: row.get(0)?,
            url: row.get(1)?,
            title: row.get(2)?,
            topic: row.get(3)?,
            published_ms: row.get(4)?,
            first_observed_ms: row.get(5)?,
            last_observed_ms: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn next_eligible_bar(first_observed_ms: u64) -> u64 {
    // Strictly after the observation. An event seen at 10:00:00 is not allowed to
    // influence a bar that also opened at 10:00:00 because ordering is ambiguous.
    first_observed_ms / 60_000 * 60 + 60
}

fn exact_index(history: &History, time: u64) -> Option<usize> {
    history
        .bars
        .binary_search_by_key(&time, |bar| bar.candle.time)
        .ok()
}

fn horizon_sample(history: &History, entry_index: usize, horizon: u32) -> Value {
    let needed = horizon as usize;
    if needed == 0 || entry_index + needed > history.bars.len() {
        return json!({"available":false,"reason":"insufficient_future_bars"});
    }
    let entry_time = history.bars[entry_index].candle.time;
    let window = &history.bars[entry_index..entry_index + needed];
    if window
        .iter()
        .enumerate()
        .any(|(offset, bar)| bar.candle.time != entry_time + offset as u64 * 60)
    {
        return json!({"available":false,"reason":"m1_gap_no_forward_fill"});
    }
    let entry = window[0].candle.open;
    let exit = window.last().unwrap().candle.close;
    let high = window
        .iter()
        .map(|bar| bar.candle.high)
        .fold(entry, f64::max);
    let low = window
        .iter()
        .map(|bar| bar.candle.low)
        .fold(entry, f64::min);
    let signed_bps = (exit / entry - 1.0) * 10_000.0;
    json!({
        "available":true,
        "entry":entry,
        "exit":exit,
        "signed_bps":signed_bps,
        "absolute_bps":signed_bps.abs(),
        "up_excursion_bps":(high / entry - 1.0) * 10_000.0,
        "down_excursion_bps":(low / entry - 1.0) * 10_000.0,
        "exit_bar_time":window.last().unwrap().candle.time
    })
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let middle = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

fn fnv64(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn analyze(db: &Connection, request: AnalyzeRequest, now_ms: u64) -> Result<Value> {
    let horizons = horizons(request.horizons_minutes)?;
    let history = load_history(db, &request.source_id)?;
    let first_bar = history.bars.first().unwrap().candle.time;
    let last_bar = history.bars.last().unwrap().candle.time;
    let min_ms = first_bar.saturating_sub(60).saturating_mul(1000);
    let max_ms = last_bar.saturating_add(60).saturating_mul(1000);
    let observations = load_observations(db, min_ms, max_ms)?;

    let mut events = Vec::new();
    let mut aggregates: BTreeMap<u32, Vec<f64>> =
        horizons.iter().copied().map(|h| (h, Vec::new())).collect();
    let mut matched = 0usize;

    for observation in observations {
        let eligible_bar_time = next_eligible_bar(observation.first_observed_ms);
        let entry_index = exact_index(&history, eligible_bar_time);
        let mut samples = serde_json::Map::new();
        if let Some(index) = entry_index {
            matched += 1;
            for horizon in &horizons {
                let sample = horizon_sample(&history, index, *horizon);
                if let Some(value) = sample["absolute_bps"].as_f64() {
                    aggregates.get_mut(horizon).unwrap().push(value);
                }
                samples.insert(horizon.to_string(), sample);
            }
        }
        events.push(json!({
            "source_id":observation.source_id,
            "url":observation.url,
            "title":observation.title,
            "topic":observation.topic,
            "published_ms":observation.published_ms,
            "first_observed_ms":observation.first_observed_ms,
            "last_observed_ms":observation.last_observed_ms,
            "published_to_observed_ms":observation.published_ms.and_then(|p| observation.first_observed_ms.checked_sub(p)),
            "eligible_bar_time":eligible_bar_time,
            "entry_bar_available":entry_index.is_some(),
            "horizons":samples,
        }));
    }

    let summary = horizons
        .iter()
        .map(|horizon| {
            let values = aggregates.get(horizon).cloned().unwrap_or_default();
            let mean = (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
            let max = values.iter().copied().reduce(f64::max);
            json!({
                "horizon_minutes":horizon,
                "samples":values.len(),
                "mean_absolute_bps":mean,
                "median_absolute_bps":median(values.clone()),
                "max_absolute_bps":max,
            })
        })
        .collect::<Vec<_>>();

    let identity_payload = json!({
        "schema":"official-news-causal-replay-v1",
        "source_id":request.source_id,
        "history":{
            "symbol":history.symbol,
            "server":history.server,
            "login":history.login,
            "currency":history.currency,
            "captured_ms":history.captured_ms,
            "first_bar":first_bar,
            "last_bar":last_bar,
            "bars":history.bars.len()
        },
        "horizons":horizons,
        "events":events,
        "summary":summary,
    });
    let id = fnv64(&serde_json::to_vec(&identity_payload)?);
    let report = json!({
        "id":id,
        "schema":"official-news-causal-replay-v1",
        "research_only":true,
        "source_id":request.source_id,
        "history":identity_payload["history"],
        "horizons_minutes":horizons,
        "observed_events":events.len(),
        "matched_events":matched,
        "events":events,
        "summary":summary,
        "boundary":"First-observed timestamps only. Publisher timestamps never grant earlier availability. The first eligible M1 bar opens strictly after observation; gaps are unavailable and are never forward-filled. No BUY/SELL inference and no broker execution effect."
    });
    db.execute(
        "INSERT OR IGNORE INTO news_replay_reports(id,created_ms,source_id,report) VALUES(?1,?2,?3,?4)",
        params![id, i64::try_from(now_ms)?, request.source_id, report.to_string()],
    )?;
    Ok(report)
}

fn overview(db: &Connection) -> Result<Value> {
    let (count, first, last): (u64, Option<u64>, Option<u64>) = db.query_row(
        "SELECT COUNT(*),MIN(first_observed_ms),MAX(last_observed_ms) FROM news_observed_events",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mut statement = db.prepare(
        "SELECT id,created_ms,history FROM backtest_runs ORDER BY created_ms DESC LIMIT 100",
    )?;
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let created_ms: u64 = row.get(1)?;
        let raw: String = row.get(2)?;
        Ok((id, created_ms, raw))
    })?;
    let mut sources = Vec::new();
    for row in rows {
        let (id, created_ms, raw) = row?;
        let Ok(history) = serde_json::from_str::<History>(&raw) else {
            continue;
        };
        if history.validate().is_err() {
            continue;
        }
        sources.push(json!({
            "id":id,
            "created_ms":created_ms,
            "symbol":history.symbol,
            "server":history.server,
            "currency":history.currency,
            "captured_ms":history.captured_ms,
            "bars":history.bars.len(),
            "first_bar":history.bars.first().map(|bar|bar.candle.time),
            "last_bar":history.bars.last().map(|bar|bar.candle.time)
        }));
    }
    Ok(json!({
        "schema":"official-news-causal-replay-v1",
        "research_only":true,
        "observed_events":count,
        "first_observed_ms":first,
        "last_observed_ms":last,
        "sources":sources,
        "default_horizons_minutes":DEFAULT_HORIZONS,
        "boundary":"Historical availability starts at BrainTrade first_observed_ms, never retroactively at published_ms."
    }))
}

fn list_reports(db: &Connection) -> Result<Value> {
    let mut statement = db.prepare(
        "SELECT id,created_ms,source_id FROM news_replay_reports ORDER BY created_ms DESC,id LIMIT ?1",
    )?;
    let rows = statement.query_map([REPORT_LIMIT as i64], |row| {
        Ok(json!({
            "id":row.get::<_,String>(0)?,
            "created_ms":row.get::<_,u64>(1)?,
            "source_id":row.get::<_,String>(2)?
        }))
    })?;
    Ok(json!({"reports":rows.collect::<std::result::Result<Vec<_>,_>>()?}))
}

fn load_report(db: &Connection, request: LoadRequest) -> Result<Value> {
    if request.id.len() != 16 || !request.id.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid event-replay report id");
    }
    let raw: Option<String> = db
        .query_row(
            "SELECT report FROM news_replay_reports WHERE id=?1",
            [request.id],
            |row| row.get(0),
        )
        .optional()?;
    let raw = raw.context("Event-replay report not found")?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn route(method: &str, path: &str, body: &[u8], db: &Connection, now_ms: u64) -> Result<Value> {
    match (method, path) {
        ("GET", "/v1/news/replay") => overview(db),
        ("GET", "/v1/news/replay/reports") => list_reports(db),
        ("POST", "/v1/news/replay/analyze") => {
            analyze(db, serde_json::from_slice(body)?, now_ms)
        }
        ("POST", "/v1/news/replay/load") => load_report(db, serde_json::from_slice(body)?),
        _ => bail!("Unknown news replay route"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{
        backtest::{Contract, HistoryBar},
        Candle,
    };

    const START: u64 = 1_800_000_000;

    fn headline(published_ms: u64) -> Headline {
        Headline {
            title: "FOMC monetary policy statement".into(),
            url: "https://www.federalreserve.gov/a".into(),
            published_ms: Some(published_ms),
            fetched_ms: published_ms,
            topic: "سياسة نقدية / كلمات عنوان".into(),
        }
    }

    fn history(gap_at: Option<usize>) -> History {
        let mut bars = Vec::new();
        for index in 0..12usize {
            if gap_at == Some(index) {
                continue;
            }
            let time = START + index as u64 * 60;
            let open = 2000.0 + index as f64;
            bars.push(HistoryBar {
                candle: Candle {
                    time,
                    open,
                    high: open + 2.0,
                    low: open - 1.0,
                    close: open + 1.0,
                    tick_volume: 100 + index as u64,
                },
                spread_points: 20,
            });
        }
        History {
            symbol: "XAUUSD".into(),
            server: "Demo".into(),
            login: 42,
            currency: "USD".into(),
            captured_ms: (START + 13 * 60) * 1000,
            contract: Contract {
                point: 0.01,
                tick_size: 0.01,
                tick_loss: 1.0,
                tick_profit: 1.0,
                volume_min: 0.01,
                volume_step: 0.01,
                volume_max: 100.0,
            },
            bars,
        }
    }

    fn database(history: History) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db.execute_batch("CREATE TABLE backtest_runs(id TEXT PRIMARY KEY,created_ms INTEGER NOT NULL,report TEXT NOT NULL,history TEXT NOT NULL);").unwrap();
        db.execute(
            "INSERT INTO backtest_runs(id,created_ms,report,history) VALUES('run',1,'{}',?1)",
            [serde_json::to_string(&history).unwrap()],
        )
        .unwrap();
        db
    }

    #[test]
    fn first_observed_is_append_only_and_cannot_be_backdated() {
        let db = database(history(None));
        let first = START * 1000 + 30_000;
        observe(&db, "fed", first, &[headline(first - 600_000)]).unwrap();
        observe(&db, "fed", first - 120_000, &[headline(first - 600_000)]).unwrap();
        let (stored_first, stored_last): (u64, u64) = db
            .query_row(
                "SELECT first_observed_ms,last_observed_ms FROM news_observed_events WHERE source_id='fed'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored_first, first);
        assert_eq!(stored_last, first);
    }

    #[test]
    fn replay_uses_first_observed_not_published_and_waits_for_next_bar() {
        let db = database(history(None));
        let first = START * 1000 + 30_000;
        observe(&db, "fed", first, &[headline(first - 600_000)]).unwrap();
        let one = analyze(
            &db,
            AnalyzeRequest {
                source_id: "run".into(),
                horizons_minutes: vec![1, 5],
            },
            first + 1,
        )
        .unwrap();
        assert_eq!(one["events"][0]["eligible_bar_time"], START + 60);
        assert_eq!(one["events"][0]["horizons"]["1"]["available"], true);
        db.execute(
            "UPDATE news_observed_events SET published_ms=?1 WHERE source_id='fed'",
            [i64::try_from(first - 3_600_000).unwrap()],
        )
        .unwrap();
        let two = analyze(
            &db,
            AnalyzeRequest {
                source_id: "run".into(),
                horizons_minutes: vec![1, 5],
            },
            first + 2,
        )
        .unwrap();
        assert_eq!(two["events"][0]["eligible_bar_time"], START + 60);
        assert_eq!(one["events"][0]["horizons"]["5"], two["events"][0]["horizons"]["5"]);
        assert_eq!(two["research_only"], true);
    }

    #[test]
    fn replay_never_forward_fills_m1_gaps() {
        let db = database(history(Some(3)));
        let first = START * 1000 + 30_000;
        observe(&db, "fed", first, &[headline(first - 60_000)]).unwrap();
        let report = analyze(
            &db,
            AnalyzeRequest {
                source_id: "run".into(),
                horizons_minutes: vec![5],
            },
            first + 1,
        )
        .unwrap();
        assert_eq!(report["events"][0]["horizons"]["5"]["available"], false);
        assert_eq!(report["events"][0]["horizons"]["5"]["reason"], "m1_gap_no_forward_fill");
        assert_eq!(report["summary"][0]["samples"], 0);
    }

    #[test]
    fn replay_report_round_trip_is_deterministic() {
        let db = database(history(None));
        let first = START * 1000 + 30_000;
        observe(&db, "fed", first, &[headline(first - 60_000)]).unwrap();
        let report = analyze(
            &db,
            AnalyzeRequest {
                source_id: "run".into(),
                horizons_minutes: vec![1, 15],
            },
            first + 1,
        )
        .unwrap();
        let loaded = load_report(
            &db,
            LoadRequest {
                id: report["id"].as_str().unwrap().into(),
            },
        )
        .unwrap();
        assert_eq!(loaded, report);
        assert!(report["boundary"].as_str().unwrap().contains("First-observed"));
    }
}
