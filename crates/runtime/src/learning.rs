//! Leakage-aware learning dataset foundation.
//! Captures immutable closed-bar features and writes labels only after future bars exist.
//! No trained model, prediction, promotion or live self-modification exists in v1.
use anyhow::{bail, Result};
use braintrade_domain::{Analysis, Candle, Telemetry};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{fs, io::Write, path::Path};

const FEATURE_SCHEMA: &str = "gold-m1-feature-v1";
const LABEL_SCHEMA: &str = "gold-m1-outcome-v1";
const HORIZONS: [u64; 3] = [5, 15, 30];

#[derive(Default)]
pub struct Service {
    last_error: String,
}

fn context(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap_or_default()
}

fn directional_score(a: &Analysis) -> f64 {
    let weight = a.votes.iter().map(|v| v.evidence.max(0.0)).sum::<f64>();
    if weight <= f64::EPSILON {
        0.0
    } else {
        (a.votes
            .iter()
            .map(|v| v.direction as f64 * v.evidence.max(0.0))
            .sum::<f64>()
            / weight)
            .clamp(-1.0, 1.0)
    }
}

fn feature_payload(t: &Telemetry, a: &Analysis) -> Option<(f64, f64, Value)> {
    if !a.current || a.bar_time == 0 || a.symbol != t.symbol {
        return None;
    }
    let market = t.market.as_ref()?;
    let last = market.m1.last()?;
    if last.time != a.bar_time || !last.close.is_finite() || last.close <= 0.0 {
        return None;
    }
    let atr = a.atr.filter(|value| value.is_finite() && *value > 0.0)?;
    let opportunities = a.opportunities.as_ref();
    let nearest_zone = opportunities.and_then(|opps| {
        opps.zones
            .iter()
            .filter(|zone| !zone.broken)
            .min_by(|left, right| {
                let l = ((left.low + left.high) * 0.5 - last.close).abs();
                let r = ((right.low + right.high) * 0.5 - last.close).abs();
                l.total_cmp(&r)
            })
    });
    let top_setup = opportunities.and_then(|opps| opps.setups.first());
    let gann = opportunities
        .map(|opps| {
            opps.gann
                .iter()
                .map(|frame| {
                    json!({
                        "timeframe":frame.timeframe,
                        "available":frame.available,
                        "direction":frame.direction,
                        "strength":frame.strength,
                        "atr_per_bar":frame.atr_per_bar,
                        "nearest_eighth":frame.nearest_eighth,
                        "distance_atr":frame.distance_atr
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let momentum = opportunities
        .map(|opps| {
            opps.momentum
                .iter()
                .map(|m| {
                    json!({
                        "timeframe":m.timeframe,
                        "direction":m.direction,
                        "roc3_atr":m.roc3_atr,
                        "roc8_atr":m.roc8_atr,
                        "acceleration_atr":m.acceleration_atr,
                        "rsi14":m.rsi14,
                        "strength":m.strength,
                        "divergence":m.divergence
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let active_zones = opportunities
        .map(|opps| opps.zones.iter().filter(|zone| !zone.broken).count())
        .unwrap_or(0);
    let profile = opportunities.map(|opps| &opps.profile);
    let session = opportunities.map(|opps| &opps.session);

    let payload = json!({
        "schema":FEATURE_SCHEMA,
        "scope":"closed-bars-only; no quote projection, DOM or news in v1",
        "symbol":t.symbol,
        "bar_time":a.bar_time,
        "close":last.close,
        "atr":atr,
        "legacy_directional_score":directional_score(a),
        "trend":{
            "direction":a.trend_brain.direction,
            "evidence":a.trend_brain.evidence,
            "aligned":a.trend_brain.aligned,
            "frames":a.trend_brain.frames
        },
        "regime":{
            "label":a.regime_brain.label,
            "direction":a.regime_brain.direction,
            "trend_evidence":a.regime_brain.trend_evidence,
            "efficiency":a.regime_brain.efficiency,
            "volatility_ratio":a.regime_brain.volatility_ratio,
            "tradable_trend":a.regime_brain.tradable_trend,
            "structure_direction":a.regime_brain.structure_brain.direction,
            "structure_evidence":a.regime_brain.structure_brain.evidence,
            "structure_consensus":a.regime_brain.structure_brain.consensus,
            "volatility_state":a.regime_brain.volatility_brain.state,
            "volatility_shock":a.regime_brain.volatility_brain.shock,
            "volatility_evidence":a.regime_brain.volatility_brain.evidence,
            "meta_v2_direction":a.regime_brain.meta_v2.direction,
            "meta_v2_evidence":a.regime_brain.meta_v2.evidence,
            "meta_v2_eligible":a.regime_brain.meta_v2.eligible
        },
        "momentum":momentum,
        "gann":gann,
        "zones":{
            "active_count":active_zones,
            "nearest_side":nearest_zone.map(|zone|zone.side),
            "nearest_strength":nearest_zone.map(|zone|zone.strength),
            "nearest_mid_distance_atr":nearest_zone.map(|zone|(((zone.low+zone.high)*0.5-last.close)/atr)),
            "nearest_width_atr":nearest_zone.map(|zone|(zone.high-zone.low)/atr)
        },
        "setup":{
            "count":opportunities.map(|opps|opps.setups.len()).unwrap_or(0),
            "top_pattern":top_setup.map(|setup|setup.pattern.clone()),
            "top_side":top_setup.map(|setup|setup.side),
            "top_evidence":top_setup.map(|setup|setup.evidence)
        },
        "activity":{
            "tick_activity_ratio":opportunities.and_then(|opps|opps.tick_activity_ratio),
            "profile_available":profile.map(|p|p.available).unwrap_or(false),
            "poc_distance_atr":profile.filter(|p|p.available).map(|p|(p.poc-last.close)/atr),
            "value_low_distance_atr":profile.filter(|p|p.available).map(|p|(p.value_low-last.close)/atr),
            "value_high_distance_atr":profile.filter(|p|p.available).map(|p|(p.value_high-last.close)/atr)
        },
        "session":{
            "covers_day_open":session.map(|s|s.covers_day_open).unwrap_or(false),
            "open_distance_atr":session.map(|s|(last.close-s.open)/atr),
            "high_distance_atr":session.map(|s|(s.high-last.close)/atr),
            "low_distance_atr":session.map(|s|(last.close-s.low)/atr),
            "vwap_distance_atr":session.and_then(|s|s.vwap_proxy).map(|v|(last.close-v)/atr)
        }
    });
    Some((last.close, atr, payload))
}

fn exact_window<'a>(series: &'a [Candle], start: u64, minutes: u64) -> Option<&'a [Candle]> {
    let first = series.iter().position(|bar| bar.time == start + 60)?;
    let last_time = start.checked_add(minutes.checked_mul(60)?)?;
    let last = series.iter().position(|bar| bar.time == last_time)?;
    if last < first || last - first + 1 != minutes as usize {
        return None;
    }
    let window = &series[first..=last];
    if window.windows(2).any(|pair| pair[1].time != pair[0].time + 60) {
        return None;
    }
    Some(window)
}

fn outcome(reference: f64, atr: f64, window: &[Candle], minutes: u64) -> Value {
    let last = window.last().unwrap();
    let final_atr = (last.close - reference) / atr;
    let mfe_atr = window
        .iter()
        .map(|bar| (bar.high - reference) / atr)
        .fold(f64::NEG_INFINITY, f64::max);
    let mae_atr = window
        .iter()
        .map(|bar| (bar.low - reference) / atr)
        .fold(f64::INFINITY, f64::min);
    let direction = if final_atr > 0.20 {
        1
    } else if final_atr < -0.20 {
        -1
    } else {
        0
    };
    json!({
        "minutes":minutes,
        "future_bar_time":last.time,
        "future_close":last.close,
        "return_atr":final_atr,
        "mfe_atr":mfe_atr,
        "mae_atr":mae_atr,
        "direction_0_2atr":direction
    })
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS learning_snapshots(
                id INTEGER PRIMARY KEY,
                context TEXT NOT NULL,
                bar_time INTEGER NOT NULL,
                captured_ms INTEGER NOT NULL,
                feature_schema TEXT NOT NULL,
                reference_close REAL NOT NULL,
                atr REAL NOT NULL,
                features TEXT NOT NULL,
                UNIQUE(context,bar_time)
            );
            CREATE TABLE IF NOT EXISTS learning_labels(
                snapshot_id INTEGER PRIMARY KEY,
                matured_ms INTEGER NOT NULL,
                label_schema TEXT NOT NULL,
                label TEXT NOT NULL,
                FOREIGN KEY(snapshot_id) REFERENCES learning_snapshots(id)
            );
            CREATE TABLE IF NOT EXISTS learning_models(
                model_id TEXT PRIMARY KEY,
                created_ms INTEGER NOT NULL,
                family TEXT NOT NULL,
                feature_schema TEXT NOT NULL,
                dataset_identity TEXT NOT NULL,
                status TEXT NOT NULL,
                metrics TEXT NOT NULL,
                artifact_path TEXT NOT NULL,
                artifact_sha256 TEXT NOT NULL,
                note TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS learning_slots(
                slot TEXT PRIMARY KEY CHECK(slot IN ('champion','challenger')),
                model_id TEXT,
                assigned_ms INTEGER NOT NULL,
                note TEXT NOT NULL,
                FOREIGN KEY(model_id) REFERENCES learning_models(model_id)
            );",
        )?;
        Ok(Self::default())
    }

    pub fn observe(&mut self, db: &mut Connection, t: &Telemetry, a: &Analysis, now: u64) {
        match self.observe_inner(db, t, a, now) {
            Ok(()) => self.last_error.clear(),
            Err(error) => self.last_error = error.to_string().chars().take(240).collect(),
        }
    }

    fn observe_inner(&mut self, db: &mut Connection, t: &Telemetry, a: &Analysis, now: u64) -> Result<()> {
        let Some((reference_close, atr, payload)) = feature_payload(t, a) else {
            return Ok(());
        };
        let market = t.market.as_ref().unwrap();
        let key = context(t);
        let tx = db.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO learning_snapshots(context,bar_time,captured_ms,feature_schema,reference_close,atr,features)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![key, a.bar_time, now, FEATURE_SCHEMA, reference_close, atr, payload.to_string()],
        )?;

        let cutoff = market.m1.last().map(|bar| bar.time.saturating_sub(30 * 60)).unwrap_or(0);
        let pending = {
            let mut statement = tx.prepare(
                "SELECT s.id,s.bar_time,s.reference_close,s.atr
                 FROM learning_snapshots s LEFT JOIN learning_labels l ON l.snapshot_id=s.id
                 WHERE s.context=?1 AND l.snapshot_id IS NULL AND s.bar_time<=?2
                 ORDER BY s.bar_time LIMIT 180",
            )?;
            let rows = statement
                .query_map(params![key, cutoff], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        for (id, bar_time, reference, snapshot_atr) in pending {
            if !snapshot_atr.is_finite() || snapshot_atr <= 0.0 {
                continue;
            }
            let Some(w5) = exact_window(&market.m1, bar_time, HORIZONS[0]) else { continue; };
            let Some(w15) = exact_window(&market.m1, bar_time, HORIZONS[1]) else { continue; };
            let Some(w30) = exact_window(&market.m1, bar_time, HORIZONS[2]) else { continue; };
            let label = json!({
                "schema":LABEL_SCHEMA,
                "reference_bar_time":bar_time,
                "reference_close":reference,
                "reference_atr":snapshot_atr,
                "horizons":[
                    outcome(reference,snapshot_atr,w5,HORIZONS[0]),
                    outcome(reference,snapshot_atr,w15,HORIZONS[1]),
                    outcome(reference,snapshot_atr,w30,HORIZONS[2])
                ],
                "note":"Label written only after exact future M1 bars exist; missing/gapped horizons are not shifted or fabricated."
            });
            tx.execute(
                "INSERT OR IGNORE INTO learning_labels(snapshot_id,matured_ms,label_schema,label) VALUES(?1,?2,?3,?4)",
                params![id, now, LABEL_SCHEMA, label.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn status(&self, db: &Connection) -> Result<Value> {
        let snapshots = db.query_row("SELECT COUNT(*) FROM learning_snapshots", [], |row| row.get::<_, i64>(0))?;
        let labels = db.query_row("SELECT COUNT(*) FROM learning_labels", [], |row| row.get::<_, i64>(0))?;
        let models = db.query_row("SELECT COUNT(*) FROM learning_models", [], |row| row.get::<_, i64>(0))?;
        let latest: Option<(u64, u64, String)> = db
            .query_row(
                "SELECT bar_time,captured_ms,feature_schema FROM learning_snapshots ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let champion: Option<String> = db
            .query_row("SELECT model_id FROM learning_slots WHERE slot='champion'", [], |row| row.get(0))
            .optional()?
            .flatten();
        let challenger: Option<String> = db
            .query_row("SELECT model_id FROM learning_slots WHERE slot='challenger'", [], |row| row.get(0))
            .optional()?
            .flatten();
        Ok(json!({
            "version":"learning-foundation-v1",
            "feature_schema":FEATURE_SCHEMA,
            "label_schema":LABEL_SCHEMA,
            "snapshots":snapshots,
            "matured_labels":labels,
            "pending_labels":snapshots-labels,
            "models":models,
            "champion":champion,
            "challenger":challenger,
            "latest":latest.map(|(bar_time,captured_ms,schema)|json!({"bar_time":bar_time,"captured_ms":captured_ms,"schema":schema})),
            "last_error":self.last_error,
            "training_enabled":false,
            "prediction_enabled":false,
            "live_self_modification":false,
            "note":"Immutable closed-bar dataset only. No model is trained or allowed to influence Shadow in this version."
        }))
    }

    pub fn export(&self, db: &Connection, dir: &Path, now: u64) -> Result<Value> {
        let name = format!("learning-dataset-{now}");
        let staging = dir.join("exports").join(format!(".{name}.tmp"));
        let destination = dir.join("exports").join(&name);
        if staging.exists() || destination.exists() {
            bail!("Learning export path already exists");
        }
        fs::create_dir_all(&staging)?;
        let mut output = fs::File::create(staging.join("dataset.jsonl"))?;
        let mut statement = db.prepare(
            "SELECT s.id,s.context,s.bar_time,s.captured_ms,s.feature_schema,s.features,l.matured_ms,l.label_schema,l.label
             FROM learning_snapshots s JOIN learning_labels l ON l.snapshot_id=s.id ORDER BY s.context,s.bar_time,s.id",
        )?;
        let mut rows = statement.query([])?;
        let mut count = 0usize;
        while let Some(row) = rows.next()? {
            let features: Value = serde_json::from_str(&row.get::<_, String>(5)?)?;
            let label: Value = serde_json::from_str(&row.get::<_, String>(8)?)?;
            writeln!(output,"{}",json!({
                "snapshot_id":row.get::<_,i64>(0)?,
                "context":row.get::<_,String>(1)?,
                "bar_time":row.get::<_,u64>(2)?,
                "captured_ms":row.get::<_,u64>(3)?,
                "feature_schema":row.get::<_,String>(4)?,
                "features":features,
                "matured_ms":row.get::<_,u64>(6)?,
                "label_schema":row.get::<_,String>(7)?,
                "label":label
            }))?;
            count += 1;
        }
        output.sync_all()?;
        fs::write(
            staging.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "version":"learning-foundation-v1",
                "feature_schema":FEATURE_SCHEMA,
                "label_schema":LABEL_SCHEMA,
                "rows":count,
                "created_ms":now,
                "ordering":"context,bar_time,snapshot_id",
                "scope":"matured labels only; no quote/DOM/news features in v1",
                "trained_model_included":false
            }))?,
        )?;
        drop(rows);
        drop(statement);
        fs::rename(staging, &destination)?;
        Ok(json!({"path":destination,"rows":count,"note":"Deterministic matured learning dataset export; training is not implemented yet."}))
    }
}
