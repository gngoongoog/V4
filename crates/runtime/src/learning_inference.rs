//! Read-only challenger inference audit.
//! Predictions are research telemetry only: they never alter Meta, Shadow, risk or broker execution.
use anyhow::{bail, Context, Result};
use braintrade_domain::{Analysis, Telemetry};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const FEATURE_SCHEMA: &str = "gold-m1-feature-v1";
const MODEL_SCHEMA: &str = "braintrade-linear-softmax-v1";
const REVIEW_POLICY: &str = "manual-review-eligibility-v1";
const CLASSES: [i64; 3] = [-1, 0, 1];
const TIMEFRAMES: [&str; 3] = ["M1", "M5", "M15"];
const PATTERNS: [&str; 6] = [
    "zone_rejection",
    "liquidity_sweep",
    "reversal",
    "breakout",
    "breakout_retest",
    "value_reclaim",
];

#[derive(Default)]
pub struct Service {
    last_error: String,
}

#[derive(Clone)]
struct Challenger {
    model_id: String,
    dataset_identity: String,
    artifact: Value,
    review_record_id: String,
}

fn context(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap_or_default()
}

fn finite(value: Option<&Value>, default: f64) -> f64 {
    let parsed = match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::Bool(flag)) => Some(if *flag { 1.0 } else { 0.0 }),
        Some(Value::String(text)) => text.parse::<f64>().ok(),
        _ => None,
    }
    .unwrap_or(default);
    if parsed.is_finite() { parsed } else { default }
}

fn clip(value: Option<&Value>, low: f64, high: f64) -> f64 {
    finite(value, 0.0).clamp(low, high)
}

fn flag(value: Option<&Value>) -> f64 {
    if value.is_some_and(|v| v == &Value::Bool(true)) { 1.0 } else { 0.0 }
}

fn object<'a>(root: &'a Value, name: &str) -> Option<&'a serde_json::Map<String, Value>> {
    root.get(name).and_then(Value::as_object)
}

fn item_by_timeframe<'a>(root: &'a Value, name: &str, timeframe: &str) -> Option<&'a Value> {
    root.get(name)
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .rev()
                .find(|row| row.get("timeframe").and_then(Value::as_str) == Some(timeframe))
        })
}

fn flatten(features: &Value) -> Result<BTreeMap<String, f64>> {
    if features.get("schema").and_then(Value::as_str) != Some(FEATURE_SCHEMA) {
        bail!("Unsupported runtime feature schema");
    }
    let mut out = BTreeMap::new();
    let mut put = |name: String, value: f64| {
        out.insert(name, if value.is_finite() { value } else { 0.0 });
    };

    put(
        "legacy_directional_score".into(),
        clip(features.get("legacy_directional_score"), -1.0, 1.0),
    );

    let trend = object(features, "trend");
    put(
        "trend.direction".into(),
        clip(trend.and_then(|row| row.get("direction")), -1.0, 1.0),
    );
    put(
        "trend.evidence".into(),
        clip(trend.and_then(|row| row.get("evidence")), 0.0, 1.0),
    );
    put(
        "trend.aligned".into(),
        flag(trend.and_then(|row| row.get("aligned"))),
    );

    let regime = object(features, "regime");
    for name in ["direction", "structure_direction", "meta_v2_direction"] {
        put(
            format!("regime.{name}"),
            clip(regime.and_then(|row| row.get(name)), -1.0, 1.0),
        );
    }
    for name in [
        "trend_evidence",
        "efficiency",
        "structure_evidence",
        "volatility_evidence",
        "meta_v2_evidence",
    ] {
        put(
            format!("regime.{name}"),
            clip(regime.and_then(|row| row.get(name)), 0.0, 1.0),
        );
    }
    put(
        "regime.volatility_ratio".into(),
        clip(regime.and_then(|row| row.get("volatility_ratio")), 0.0, 8.0),
    );
    for name in [
        "tradable_trend",
        "structure_consensus",
        "volatility_shock",
        "meta_v2_eligible",
    ] {
        put(
            format!("regime.{name}"),
            flag(regime.and_then(|row| row.get(name))),
        );
    }

    for timeframe in TIMEFRAMES {
        let row = item_by_timeframe(features, "momentum", timeframe);
        put(
            format!("momentum.{timeframe}.present"),
            if row.is_some() { 1.0 } else { 0.0 },
        );
        put(
            format!("momentum.{timeframe}.direction"),
            clip(row.and_then(|v| v.get("direction")), -1.0, 1.0),
        );
        put(
            format!("momentum.{timeframe}.roc3_atr"),
            clip(row.and_then(|v| v.get("roc3_atr")), -6.0, 6.0),
        );
        put(
            format!("momentum.{timeframe}.roc8_atr"),
            clip(row.and_then(|v| v.get("roc8_atr")), -10.0, 10.0),
        );
        put(
            format!("momentum.{timeframe}.acceleration_atr"),
            clip(row.and_then(|v| v.get("acceleration_atr")), -6.0, 6.0),
        );
        let rsi = finite(row.and_then(|v| v.get("rsi14")), 50.0);
        put(
            format!("momentum.{timeframe}.rsi_centered"),
            ((rsi - 50.0) / 25.0).clamp(-2.0, 2.0),
        );
        put(
            format!("momentum.{timeframe}.strength"),
            clip(row.and_then(|v| v.get("strength")), 0.0, 1.0),
        );
        put(
            format!("momentum.{timeframe}.divergence"),
            clip(row.and_then(|v| v.get("divergence")), -1.0, 1.0),
        );
    }

    for timeframe in TIMEFRAMES {
        let row = item_by_timeframe(features, "gann", timeframe);
        put(
            format!("gann.{timeframe}.available"),
            if row.and_then(|v| v.get("available")) == Some(&Value::Bool(true)) {
                1.0
            } else {
                0.0
            },
        );
        put(
            format!("gann.{timeframe}.direction"),
            clip(row.and_then(|v| v.get("direction")), -1.0, 1.0),
        );
        put(
            format!("gann.{timeframe}.strength"),
            clip(row.and_then(|v| v.get("strength")), 0.0, 1.0),
        );
        put(
            format!("gann.{timeframe}.atr_per_bar"),
            clip(row.and_then(|v| v.get("atr_per_bar")), -2.0, 2.0),
        );
        put(
            format!("gann.{timeframe}.distance_atr"),
            clip(row.and_then(|v| v.get("distance_atr")), 0.0, 10.0),
        );
        put(
            format!("gann.{timeframe}.eighth"),
            clip(row.and_then(|v| v.get("nearest_eighth")), 0.0, 8.0) / 8.0,
        );
    }

    let zones = object(features, "zones");
    put(
        "zones.active_count".into(),
        clip(zones.and_then(|row| row.get("active_count")), 0.0, 50.0) / 10.0,
    );
    put(
        "zones.nearest_present".into(),
        if zones
            .and_then(|row| row.get("nearest_side"))
            .is_some_and(|value| !value.is_null())
        {
            1.0
        } else {
            0.0
        },
    );
    put(
        "zones.nearest_side".into(),
        clip(zones.and_then(|row| row.get("nearest_side")), -1.0, 1.0),
    );
    put(
        "zones.nearest_strength".into(),
        clip(zones.and_then(|row| row.get("nearest_strength")), 0.0, 1.0),
    );
    put(
        "zones.nearest_mid_distance_atr".into(),
        clip(
            zones.and_then(|row| row.get("nearest_mid_distance_atr")),
            -10.0,
            10.0,
        ),
    );
    put(
        "zones.nearest_width_atr".into(),
        clip(zones.and_then(|row| row.get("nearest_width_atr")), 0.0, 10.0),
    );

    let setup = object(features, "setup");
    let pattern = setup
        .and_then(|row| row.get("top_pattern"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    put(
        "setup.count".into(),
        clip(setup.and_then(|row| row.get("count")), 0.0, 6.0) / 6.0,
    );
    put(
        "setup.top_present".into(),
        if pattern.is_some() { 1.0 } else { 0.0 },
    );
    put(
        "setup.top_side".into(),
        clip(setup.and_then(|row| row.get("top_side")), -1.0, 1.0),
    );
    put(
        "setup.top_evidence".into(),
        clip(setup.and_then(|row| row.get("top_evidence")), 0.0, 1.0),
    );
    for name in PATTERNS {
        put(
            format!("setup.pattern.{name}"),
            if pattern == Some(name) { 1.0 } else { 0.0 },
        );
    }

    let activity = object(features, "activity");
    let ratio = activity.and_then(|row| row.get("tick_activity_ratio"));
    put(
        "activity.tick_present".into(),
        if ratio.is_some_and(|value| !value.is_null()) { 1.0 } else { 0.0 },
    );
    put("activity.tick_ratio".into(), clip(ratio, 0.0, 5.0) / 5.0);
    put(
        "activity.profile_available".into(),
        flag(activity.and_then(|row| row.get("profile_available"))),
    );
    for name in [
        "poc_distance_atr",
        "value_low_distance_atr",
        "value_high_distance_atr",
    ] {
        put(
            format!("activity.{name}"),
            clip(activity.and_then(|row| row.get(name)), -10.0, 10.0),
        );
    }

    let session = object(features, "session");
    put(
        "session.covers_day_open".into(),
        flag(session.and_then(|row| row.get("covers_day_open"))),
    );
    for name in [
        "open_distance_atr",
        "high_distance_atr",
        "low_distance_atr",
        "vwap_distance_atr",
    ] {
        put(
            format!("session.{name}"),
            clip(session.and_then(|row| row.get(name)), -12.0, 12.0),
        );
    }
    put(
        "session.vwap_present".into(),
        if session
            .and_then(|row| row.get("vwap_distance_atr"))
            .is_some_and(|value| !value.is_null())
        {
            1.0
        } else {
            0.0
        },
    );

    Ok(out)
}

fn vec_f64(value: &Value, length: usize, positive: bool) -> Result<Vec<f64>> {
    let array = value.as_array().context("Model vector missing")?;
    if array.len() != length {
        bail!("Model vector dimension mismatch");
    }
    array
        .iter()
        .map(|value| {
            let number = value.as_f64().context("Model vector contains non-number")?;
            if !number.is_finite() || (positive && number <= 0.0) {
                bail!("Model vector contains invalid numeric value");
            }
            Ok(number)
        })
        .collect()
}

fn current_challenger(db: &Connection) -> Result<Option<Challenger>> {
    let row: Option<(String, String, String, String)> = db
        .query_row(
            "SELECT m.model_id,m.dataset_identity,m.status,m.artifact
             FROM learning_slots s JOIN learning_models m ON m.model_id=s.model_id
             WHERE s.slot='challenger' AND s.model_id IS NOT NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((model_id, dataset_identity, status, artifact_raw)) = row else {
        return Ok(None);
    };
    if status != "unreviewed_challenger" {
        return Ok(None);
    }
    let review: Option<(String, String, String)> = db
        .query_row(
            "SELECT id,policy,eligibility FROM learning_review_records
             WHERE model_id=?1 ORDER BY created_ms DESC,id DESC LIMIT 1",
            [&model_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((review_record_id, policy, eligibility_raw)) = review else {
        return Ok(None);
    };
    if policy != REVIEW_POLICY {
        return Ok(None);
    }
    let eligibility: Value = serde_json::from_str(&eligibility_raw)?;
    if eligibility["eligible_for_manual_review"] != true
        || eligibility["eligible_for_promotion"] != false
    {
        return Ok(None);
    }
    let artifact: Value = serde_json::from_str(&artifact_raw)?;
    Ok(Some(Challenger {
        model_id,
        dataset_identity,
        artifact,
        review_record_id,
    }))
}

fn probabilities(artifact: &Value, flat: &BTreeMap<String, f64>) -> Result<([f64; 3], i64)> {
    if artifact["schema"] != MODEL_SCHEMA
        || artifact["feature_schema"] != FEATURE_SCHEMA
        || artifact["status"] != "unreviewed_challenger"
        || artifact["eligible_for_promotion"] != false
    {
        bail!("Challenger artifact is not eligible for read-only inference");
    }
    let classes = artifact["model"]["classes"]
        .as_array()
        .context("Model classes missing")?;
    if classes.len() != 3
        || classes
            .iter()
            .zip(CLASSES)
            .any(|(value, expected)| value.as_i64() != Some(expected))
    {
        bail!("Model classes must be [-1,0,1]");
    }
    let names = artifact["model"]["feature_names"]
        .as_array()
        .context("Model feature names missing")?
        .iter()
        .map(|value| value.as_str().map(str::to_owned).context("Invalid feature name"))
        .collect::<Result<Vec<_>>>()?;
    let current_names = flat.keys().cloned().collect::<Vec<_>>();
    if names != current_names {
        bail!("Runtime feature contract differs from the trained artifact");
    }
    let dimension = names.len();
    let means = vec_f64(&artifact["model"]["means"], dimension, false)?;
    let stds = vec_f64(&artifact["model"]["stds"], dimension, true)?;
    let biases = vec_f64(&artifact["model"]["biases"], 3, false)?;
    let weight_rows = artifact["model"]["weights"]
        .as_array()
        .filter(|rows| rows.len() == 3)
        .context("Invalid model weight matrix")?;
    let weights = weight_rows
        .iter()
        .map(|row| vec_f64(row, dimension, false))
        .collect::<Result<Vec<_>>>()?;
    let standardized = names
        .iter()
        .enumerate()
        .map(|(index, name)| (flat[name] - means[index]) / stds[index])
        .collect::<Vec<_>>();
    if standardized.iter().any(|value| !value.is_finite()) {
        bail!("Standardized runtime features are not finite");
    }
    let mut scores = [0.0; 3];
    for class in 0..3 {
        scores[class] = biases[class]
            + weights[class]
                .iter()
                .zip(&standardized)
                .map(|(weight, value)| weight * value)
                .sum::<f64>();
        if !scores[class].is_finite() {
            bail!("Model score is not finite");
        }
    }
    let top = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut exp = [0.0; 3];
    for index in 0..3 {
        exp[index] = (scores[index] - top).clamp(-60.0, 60.0).exp();
    }
    let total = exp.iter().sum::<f64>();
    if !total.is_finite() || total <= 0.0 {
        bail!("Softmax normalization failed");
    }
    let probs = [exp[0] / total, exp[1] / total, exp[2] / total];
    let mut winner = 0usize;
    for index in 1..3 {
        if probs[index] > probs[winner] {
            winner = index;
        }
    }
    Ok((probs, CLASSES[winner]))
}

/// Labels used during training/validation/holdout extend up to 30 minutes beyond
/// their feature bars. Require a later bar in a known context before live scoring.
fn after_training_evidence(artifact: &Value, context: &str, bar_time: u64) -> Result<bool> {
    let splits=artifact["dataset"]["split"].as_array().context("Model chronological boundaries missing; retrain artifact")?;
    let mut latest=0u64; let mut known=false;
    for split in splits {
        known |= split["context"].as_str()==Some(context);
        let last=split["holdout_last_bar"].as_u64().context("Model holdout_last_bar missing; retrain artifact")?;
        latest=latest.max(last.checked_add(1800).context("Model timestamp overflow")?);
    }
    Ok(known && !splits.is_empty() && bar_time>latest)
}

fn infer_from_payload(
    db: &Connection,
    context: &str,
    bar_time: u64,
    features: &Value,
    now: u64,
) -> Result<bool> {
    let Some(challenger) = current_challenger(db)? else {
        return Ok(false);
    };
    if challenger.artifact["dataset"]["sha256"].as_str()
        != Some(challenger.dataset_identity.as_str())
    {
        bail!("Challenger dataset identity changed after registration");
    }
    if !after_training_evidence(&challenger.artifact,context,bar_time)? { return Ok(false); }
    let flat = flatten(features)?;
    let (probs, predicted_class) = probabilities(&challenger.artifact, &flat)?;
    let target = challenger.artifact["target"]
        .as_str()
        .context("Model target missing")?;
    if !matches!(
        target,
        "5m.direction_0_2atr" | "15m.direction_0_2atr" | "30m.direction_0_2atr"
    ) {
        bail!("Unsupported challenger target");
    }
    let confidence = probs.iter().copied().fold(0.0, f64::max);
    db.execute(
        "INSERT OR IGNORE INTO learning_predictions(
            context,bar_time,observed_ms,model_id,dataset_identity,review_record_id,target,
            p_down,p_flat,p_up,predicted_class,confidence,feature_schema
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            context,
            bar_time,
            now,
            challenger.model_id,
            challenger.dataset_identity,
            challenger.review_record_id,
            target,
            probs[0],
            probs[1],
            probs[2],
            predicted_class,
            confidence,
            FEATURE_SCHEMA
        ],
    )?;
    Ok(true)
}

#[cfg(test)]
fn fnv64(flat: &BTreeMap<String, f64>) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for (name, value) in flat {
        let line = format!("{name}={value:.12}\n");
        for byte in line.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS learning_predictions(
                id INTEGER PRIMARY KEY,
                context TEXT NOT NULL,
                bar_time INTEGER NOT NULL,
                observed_ms INTEGER NOT NULL,
                model_id TEXT NOT NULL,
                dataset_identity TEXT NOT NULL,
                review_record_id TEXT NOT NULL,
                target TEXT NOT NULL,
                p_down REAL NOT NULL,
                p_flat REAL NOT NULL,
                p_up REAL NOT NULL,
                predicted_class INTEGER NOT NULL CHECK(predicted_class IN (-1,0,1)),
                confidence REAL NOT NULL,
                feature_schema TEXT NOT NULL,
                UNIQUE(model_id,context,bar_time)
            );
            CREATE INDEX IF NOT EXISTS learning_predictions_recent_idx
                ON learning_predictions(observed_ms DESC,id DESC);",
        )?;
        Ok(Self::default())
    }

    pub fn observe(&mut self, db: &Connection, telemetry: &Telemetry, analysis: &Analysis, now: u64) {
        let result = (|| -> Result<()> {
            if !analysis.current || analysis.bar_time == 0 || analysis.symbol != telemetry.symbol {
                return Ok(());
            }
            let key = context(telemetry);
            let raw: Option<String> = db
                .query_row(
                    "SELECT features FROM learning_snapshots WHERE context=?1 AND bar_time=?2",
                    params![key, analysis.bar_time],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(raw) = raw else {
                return Ok(());
            };
            let features: Value = serde_json::from_str(&raw)?;
            infer_from_payload(db, &key, analysis.bar_time, &features, now)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.last_error.clear(),
            Err(error) => self.last_error = error.to_string().chars().take(240).collect(),
        }
    }

    pub fn status(&self, db: &Connection) -> Result<Value> {
        let count = db.query_row("SELECT COUNT(*) FROM learning_predictions", [], |row| {
            row.get::<_, i64>(0)
        })?;
        let latest: Option<String> = db
            .query_row(
                "SELECT json_object(
                    'model_id',model_id,'bar_time',bar_time,'observed_ms',observed_ms,
                    'target',target,'predicted_class',predicted_class,'confidence',confidence,
                    'p_down',p_down,'p_flat',p_flat,'p_up',p_up,'review_record_id',review_record_id
                 ) FROM learning_predictions ORDER BY observed_ms DESC,id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let gate = current_challenger(db)?;
        Ok(json!({
            "version":"challenger-read-only-inference-v1",
            "read_only_gate_ready":gate.is_some(),
            "current_model_id":gate.as_ref().map(|value|value.model_id.clone()),
            "review_record_id":gate.as_ref().map(|value|value.review_record_id.clone()),
            "predictions":count,
            "latest":latest.and_then(|raw|serde_json::from_str::<Value>(&raw).ok()),
            "last_error":self.last_error,
            "shadow_effect":false,
            "risk_effect":false,
            "broker_effect":false,
            "automatic_promotion":false,
            "note":"Read-only closed-bar challenger predictions are audit telemetry only."
        }))
    }

    fn recent(&self, db: &Connection) -> Result<Value> {
        let mut statement = db.prepare(
            "SELECT context,bar_time,observed_ms,model_id,dataset_identity,review_record_id,target,
                    p_down,p_flat,p_up,predicted_class,confidence
             FROM learning_predictions ORDER BY observed_ms DESC,id DESC LIMIT 100",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(json!({
                    "context":row.get::<_,String>(0)?,
                    "bar_time":row.get::<_,u64>(1)?,
                    "observed_ms":row.get::<_,u64>(2)?,
                    "model_id":row.get::<_,String>(3)?,
                    "dataset_identity":row.get::<_,String>(4)?,
                    "review_record_id":row.get::<_,String>(5)?,
                    "target":row.get::<_,String>(6)?,
                    "probabilities":{
                        "down":row.get::<_,f64>(7)?,
                        "flat":row.get::<_,f64>(8)?,
                        "up":row.get::<_,f64>(9)?
                    },
                    "predicted_class":row.get::<_,i64>(10)?,
                    "confidence":row.get::<_,f64>(11)?,
                    "shadow_effect":false
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({"predictions":rows,"shadow_effect":false,"broker_effect":false}))
    }

    pub fn route(&self, method: &str, path: &str, db: &Connection) -> Result<Value> {
        match (method, path) {
            ("GET", "/v1/learning/inference") => self.status(db),
            ("GET", "/v1/learning/inference/recent") => self.recent(db),
            _ => bail!("Unknown learning inference route"),
        }
    }

    pub fn enrich_learning_status(&self, db: &Connection, mut base: Value) -> Result<Value> {
        if let Some(object) = base.as_object_mut() {
            object.insert("read_only_inference".into(), self.status(db)?);
        }
        Ok(base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../tests/learning_feature_parity.json")).unwrap()
    }

    fn artifact(flat: &BTreeMap<String, f64>) -> Value {
        let names = flat.keys().cloned().collect::<Vec<_>>();
        let dimension = names.len();
        json!({
            "schema":MODEL_SCHEMA,
            "status":"unreviewed_challenger",
            "eligible_for_promotion":false,
            "feature_schema":FEATURE_SCHEMA,
            "target":"15m.direction_0_2atr",
            "dataset":{"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","split":[{"context":"ctx","holdout_last_bar":1}]},
            "model":{
                "classes":[-1,0,1],
                "feature_names":names,
                "means":vec![0.0;dimension],
                "stds":vec![1.0;dimension],
                "weights":[vec![0.0;dimension],vec![0.0;dimension],vec![0.0;dimension]],
                "biases":[-1.0,0.0,1.0]
            }
        })
    }

    fn db(flat: &BTreeMap<String, f64>) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_models(model_id TEXT PRIMARY KEY,dataset_identity TEXT NOT NULL,status TEXT NOT NULL,artifact TEXT NOT NULL);
             CREATE TABLE learning_slots(slot TEXT PRIMARY KEY,model_id TEXT);
             CREATE TABLE learning_review_records(id TEXT PRIMARY KEY,model_id TEXT NOT NULL,created_ms INTEGER NOT NULL,policy TEXT NOT NULL,evidence_ids TEXT NOT NULL,eligibility TEXT NOT NULL,reviewer_note TEXT NOT NULL);
             CREATE TABLE learning_snapshots(id INTEGER PRIMARY KEY,context TEXT NOT NULL,bar_time INTEGER NOT NULL,features TEXT NOT NULL,UNIQUE(context,bar_time));",
        )
        .unwrap();
        Service::new(&db).unwrap();
        db.execute(
            "INSERT INTO learning_models VALUES('m1',?1,'unreviewed_challenger',?2)",
            params!["a".repeat(64), artifact(flat).to_string()],
        )
        .unwrap();
        db.execute("INSERT INTO learning_slots VALUES('challenger','m1')", [])
            .unwrap();
        db
    }

    #[test]
    fn rust_flatten_matches_python_contract_fixture() {
        let fixture = fixture();
        let flat = flatten(&fixture["features"]).unwrap();
        assert_eq!(flat.len(), fixture["expected_feature_count"].as_u64().unwrap() as usize);
        assert_eq!(fnv64(&flat), fixture["expected_fnv64"].as_str().unwrap());
    }

    #[test]
    fn inference_requires_review_record_and_is_audit_only() {
        let fixture = fixture();
        let flat = flatten(&fixture["features"]).unwrap();
        let db = db(&flat);
        assert!(!infer_from_payload(&db, "ctx", 1860, &fixture["features"], 1000).unwrap());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM learning_predictions",[],|row|row.get::<_,i64>(0)).unwrap(),0);
        db.execute(
            "INSERT INTO learning_review_records VALUES('r1','m1',1,?1,'[]',?2,'')",
            params![REVIEW_POLICY,json!({"eligible_for_manual_review":true,"eligible_for_promotion":false}).to_string()],
        )
        .unwrap();
        assert!(infer_from_payload(&db, "ctx", 1860, &fixture["features"], 1000).unwrap());
        let (class, confidence):(i64,f64)=db.query_row("SELECT predicted_class,confidence FROM learning_predictions",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(class,1);
        assert!(confidence>0.65);
        assert!(infer_from_payload(&db, "ctx", 1860, &fixture["features"], 1001).unwrap());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM learning_predictions",[],|row|row.get::<_,i64>(0)).unwrap(),1);
    }

    #[test]
    fn inference_rejects_training_overlap_unknown_context_and_missing_bounds() {
        let mut a=json!({"dataset":{"split":[{"context":"ctx","holdout_last_bar":1000},{"context":"other","holdout_last_bar":2000}]}});
        assert!(!after_training_evidence(&a,"ctx",3800).unwrap());
        assert!(after_training_evidence(&a,"ctx",3801).unwrap());
        assert!(!after_training_evidence(&a,"unknown",3801).unwrap());
        a["dataset"]["split"][0].as_object_mut().unwrap().remove("holdout_last_bar");
        assert!(after_training_evidence(&a,"ctx",3801).is_err());
    }

    #[test]
    fn artifact_feature_name_drift_fails_closed() {
        let fixture = fixture();
        let flat = flatten(&fixture["features"]).unwrap();
        let mut artifact = artifact(&flat);
        artifact["model"]["feature_names"][0] = Value::String("drifted.feature".into());
        assert!(probabilities(&artifact, &flat).is_err());
    }
}
