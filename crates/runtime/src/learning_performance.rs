//! Conservative read-only performance review for challenger prediction outcomes.
//! The policy is scientific review telemetry only: it cannot mutate state, promote a model,
//! alter Shadow/risk decisions, or send broker instructions.
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "challenger-performance-review-v1";
const REVIEW_POLICY: &str = "manual-review-eligibility-v1";
const MIN_SAMPLES: usize = 900;
const MIN_CLASS_SUPPORT: usize = 90;
const WINDOW_SIZE: usize = 200;
const MIN_FULL_WINDOWS: usize = 3;
const MIN_BALANCED_ACCURACY: f64 = 0.38;
const MIN_MACRO_F1: f64 = 0.36;
const MAX_ECE: f64 = 0.12;
const MAX_OVERCONFIDENCE: f64 = 0.10;
const MIN_LOG_LOSS_GAIN: f64 = 0.02;
const MIN_BRIER_GAIN: f64 = 0.01;
const MIN_WINDOW_BALANCED_ACCURACY: f64 = 0.34;
const MAX_WINDOW_LOG_LOSS_MARGIN: f64 = 0.12;
const MAX_BALANCED_ACCURACY_SPREAD: f64 = 0.12;
const MAX_LOG_LOSS_SPREAD: f64 = 0.20;
const MAX_WINDOW_ECE: f64 = 0.15;
const MAX_CLASS_DISTRIBUTION_TV: f64 = 0.20;
const MAX_HALF_BALANCED_ACCURACY_DELTA: f64 = 0.10;
const MAX_HALF_LOG_LOSS_DELTA: f64 = 0.15;
const CLASSES: [i64; 3] = [-1, 0, 1];

#[derive(Clone, Debug)]
struct Row {
    prediction_id: i64,
    context: String,
    bar_time: u64,
    target: String,
    predicted: i64,
    realized: i64,
    confidence: f64,
    log_loss: f64,
    brier: f64,
    dataset_identity: String,
    review_record_id: String,
    review_policy: Option<String>,
    review_model_id: Option<String>,
}

#[derive(Clone, Debug)]
struct Summary {
    samples: usize,
    support: [usize; 3],
    predicted: [usize; 3],
    accuracy: f64,
    balanced_accuracy: f64,
    macro_f1: f64,
    mean_log_loss: f64,
    mean_brier: f64,
    mean_confidence: f64,
    ece: f64,
}

fn class_index(value: i64) -> Result<usize> {
    CLASSES
        .iter()
        .position(|candidate| *candidate == value)
        .context("class must be -1, 0 or 1")
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let rows = values.collect::<Vec<_>>();
    if rows.is_empty() {
        0.0
    } else {
        rows.iter().sum::<f64>() / rows.len() as f64
    }
}

fn summarize(rows: &[Row]) -> Result<Summary> {
    let mut support = [0usize; 3];
    let mut predicted = [0usize; 3];
    let mut confusion = [[0usize; 3]; 3];
    let mut bins = [(0usize, 0usize, 0.0f64); 10];
    let mut correct = 0usize;
    for row in rows {
        if !row.confidence.is_finite() || !row.log_loss.is_finite() || !row.brier.is_finite() {
            bail!("non-finite scored outcome metric");
        }
        if !(0.0..=1.0).contains(&row.confidence) || row.log_loss < 0.0 || row.brier < 0.0 {
            bail!("invalid scored outcome metric range");
        }
        let actual = class_index(row.realized)?;
        let guess = class_index(row.predicted)?;
        support[actual] += 1;
        predicted[guess] += 1;
        confusion[actual][guess] += 1;
        correct += usize::from(actual == guess);
        let bin = ((row.confidence * 10.0).floor() as usize).min(9);
        bins[bin].0 += 1;
        bins[bin].1 += usize::from(actual == guess);
        bins[bin].2 += row.confidence;
    }
    let samples = rows.len();
    let accuracy = if samples == 0 {
        0.0
    } else {
        correct as f64 / samples as f64
    };
    let balanced_accuracy = (0..3)
        .map(|index| {
            if support[index] == 0 {
                0.0
            } else {
                confusion[index][index] as f64 / support[index] as f64
            }
        })
        .sum::<f64>()
        / 3.0;
    let macro_f1 = (0..3)
        .map(|index| {
            let tp = confusion[index][index] as f64;
            let fp = predicted[index].saturating_sub(confusion[index][index]) as f64;
            let fn_ = support[index].saturating_sub(confusion[index][index]) as f64;
            let denominator = 2.0 * tp + fp + fn_;
            if denominator == 0.0 {
                0.0
            } else {
                2.0 * tp / denominator
            }
        })
        .sum::<f64>()
        / 3.0;
    let ece = if samples == 0 {
        0.0
    } else {
        bins.iter()
            .filter(|bin| bin.0 > 0)
            .map(|(count, hits, confidence_sum)| {
                let bin_accuracy = *hits as f64 / *count as f64;
                let bin_confidence = *confidence_sum / *count as f64;
                *count as f64 / samples as f64 * (bin_accuracy - bin_confidence).abs()
            })
            .sum()
    };
    Ok(Summary {
        samples,
        support,
        predicted,
        accuracy,
        balanced_accuracy,
        macro_f1,
        mean_log_loss: mean(rows.iter().map(|row| row.log_loss)),
        mean_brier: mean(rows.iter().map(|row| row.brier)),
        mean_confidence: mean(rows.iter().map(|row| row.confidence)),
        ece,
    })
}

fn summary_json(summary: &Summary) -> Value {
    json!({
        "samples":summary.samples,
        "class_support":{"down":summary.support[0],"flat":summary.support[1],"up":summary.support[2]},
        "predicted_support":{"down":summary.predicted[0],"flat":summary.predicted[1],"up":summary.predicted[2]},
        "accuracy":summary.accuracy,
        "balanced_accuracy":summary.balanced_accuracy,
        "macro_f1":summary.macro_f1,
        "mean_log_loss":summary.mean_log_loss,
        "mean_brier":summary.mean_brier,
        "mean_confidence":summary.mean_confidence,
        "ece_10_bin":summary.ece
    })
}

fn class_distribution(summary: &Summary) -> [f64; 3] {
    if summary.samples == 0 {
        return [0.0; 3];
    }
    [
        summary.support[0] as f64 / summary.samples as f64,
        summary.support[1] as f64 / summary.samples as f64,
        summary.support[2] as f64 / summary.samples as f64,
    ]
}

fn total_variation(a: [f64; 3], b: [f64; 3]) -> f64 {
    0.5 * a.iter().zip(b).map(|(left, right)| (left - right).abs()).sum::<f64>()
}

fn fnv1a64(rows: &[Row], model_id: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    let mut feed = |text: &str| {
        for byte in text.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    feed(POLICY);
    feed("\n");
    feed(model_id);
    feed("\n");
    for row in rows {
        feed(&format!(
            "{}|{}|{}|{}|{}|{}|{:.12}|{:.12}|{:.12}|{}|{}\n",
            row.prediction_id,
            row.context,
            row.bar_time,
            row.target,
            row.predicted,
            row.realized,
            row.confidence,
            row.log_loss,
            row.brier,
            row.dataset_identity,
            row.review_record_id
        ));
    }
    format!("fnv1a64:{hash:016x}")
}

fn check(name: &str, value: f64, relation: &str, threshold: f64, pass: bool) -> Value {
    json!({"name":name,"value":value,"relation":relation,"threshold":threshold,"pass":pass})
}

fn windows(rows: &[Row]) -> Result<(Vec<Value>, Vec<Summary>)> {
    let mut by_context: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        by_context.entry(&row.context).or_default().push(row);
    }
    let mut output = Vec::new();
    let mut summaries = Vec::new();
    for (context, mut context_rows) in by_context {
        context_rows.sort_by_key(|row| (row.bar_time, row.prediction_id));
        for (index, chunk) in context_rows.chunks(WINDOW_SIZE).enumerate() {
            if chunk.len() != WINDOW_SIZE {
                break;
            }
            let owned = chunk.iter().map(|row| (*row).clone()).collect::<Vec<_>>();
            let summary = summarize(&owned)?;
            output.push(json!({
                "context":context,
                "index":index,
                "first_bar_time":owned.first().map(|row|row.bar_time),
                "last_bar_time":owned.last().map(|row|row.bar_time),
                "metrics":summary_json(&summary)
            }));
            summaries.push(summary);
        }
    }
    Ok((output, summaries))
}

fn chronological_halves(rows: &[Row]) -> Result<(Summary, Summary)> {
    let mut by_context: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        by_context.entry(&row.context).or_default().push(row);
    }
    let mut early = Vec::new();
    let mut late = Vec::new();
    for (_, mut context_rows) in by_context {
        context_rows.sort_by_key(|row| (row.bar_time, row.prediction_id));
        let midpoint = context_rows.len() / 2;
        early.extend(context_rows[..midpoint].iter().map(|row| (*row).clone()));
        late.extend(context_rows[midpoint..].iter().map(|row| (*row).clone()));
    }
    Ok((summarize(&early)?, summarize(&late)?))
}

fn read_current(db: &Connection) -> Result<Option<(String, String, String)>> {
    db.query_row(
        "SELECT m.model_id,m.dataset_identity,m.artifact FROM learning_slots s JOIN learning_models m ON m.model_id=s.model_id WHERE s.slot='challenger' AND m.status='unreviewed_challenger'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .map_err(Into::into)
}

fn read_rows(db: &Connection, model_id: &str) -> Result<Vec<Row>> {
    let mut statement = db.prepare(
        "SELECT p.id,p.context,p.bar_time,p.target,p.predicted_class,o.realized_class,p.confidence,o.log_loss,o.brier,p.dataset_identity,p.review_record_id,r.policy,r.model_id
         FROM learning_prediction_outcomes o
         JOIN learning_predictions p ON p.id=o.prediction_id
         LEFT JOIN learning_review_records r ON r.id=p.review_record_id
         WHERE p.model_id=?1
         ORDER BY p.context,p.bar_time,p.id",
    )?;
    let rows = statement
        .query_map([model_id], |row| {
            Ok(Row {
                prediction_id: row.get(0)?,
                context: row.get(1)?,
                bar_time: row.get(2)?,
                target: row.get(3)?,
                predicted: row.get(4)?,
                realized: row.get(5)?,
                confidence: row.get(6)?,
                log_loss: row.get(7)?,
                brier: row.get(8)?,
                dataset_identity: row.get(9)?,
                review_record_id: row.get(10)?,
                review_policy: row.get(11)?,
                review_model_id: row.get(12)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn review(db: &Connection) -> Result<Value> {
    let Some((model_id, model_dataset_identity, artifact_raw)) = read_current(db)? else {
        return Ok(json!({
            "policy":POLICY,"status":"insufficient_evidence","reason":"No current unreviewed challenger",
            "promotion_eligible":false,"promotion_enabled":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false
        }));
    };
    let artifact: Value = serde_json::from_str(&artifact_raw).context("stored challenger artifact is invalid JSON")?;
    let model_target = artifact["target"].as_str().context("challenger target missing")?;
    let rows = read_rows(db, &model_id)?;
    let summary = summarize(&rows)?;
    let contexts = rows.iter().map(|row| row.context.clone()).collect::<BTreeSet<_>>();
    let datasets = rows.iter().map(|row| row.dataset_identity.clone()).collect::<BTreeSet<_>>();
    let review_ids = rows.iter().map(|row| row.review_record_id.clone()).collect::<BTreeSet<_>>();
    let targets = rows.iter().map(|row| row.target.clone()).collect::<BTreeSet<_>>();
    let broken_review_links = rows
        .iter()
        .filter(|row| {
            row.review_policy.as_deref() != Some(REVIEW_POLICY)
                || row.review_model_id.as_deref() != Some(model_id.as_str())
        })
        .count();
    let mismatched_dataset = rows.iter().filter(|row| row.dataset_identity != model_dataset_identity).count();
    let mismatched_target = rows.iter().filter(|row| row.target != model_target).count();
    let (window_json, window_summaries) = windows(&rows)?;
    let evidence = json!({
        "fingerprint":fnv1a64(&rows,&model_id),
        "model_id":model_id,
        "dataset_identity":model_dataset_identity,
        "target":model_target,
        "prediction_id_min":rows.iter().map(|row|row.prediction_id).min(),
        "prediction_id_max":rows.iter().map(|row|row.prediction_id).max(),
        "first_bar_time":rows.iter().map(|row|row.bar_time).min(),
        "last_bar_time":rows.iter().map(|row|row.bar_time).max(),
        "contexts":contexts,
        "dataset_identities":datasets,
        "review_record_ids":review_ids,
        "targets":targets,
        "broken_review_links":broken_review_links,
        "mismatched_dataset_rows":mismatched_dataset,
        "mismatched_target_rows":mismatched_target,
        "immutable_source":"learning_predictions + learning_prediction_outcomes"
    });
    let class_floor = summary.support.iter().copied().min().unwrap_or(0);
    let evidence_sufficient = summary.samples >= MIN_SAMPLES
        && class_floor >= MIN_CLASS_SUPPORT
        && window_summaries.len() >= MIN_FULL_WINDOWS;
    let integrity_ok = broken_review_links == 0
        && mismatched_dataset == 0
        && mismatched_target == 0
        && datasets.len() <= 1
        && targets.len() <= 1
        && !review_ids.is_empty();
    if !evidence_sufficient || !integrity_ok {
        let mut reasons = Vec::new();
        if summary.samples < MIN_SAMPLES {
            reasons.push(format!("need at least {MIN_SAMPLES} scored outcomes"));
        }
        if class_floor < MIN_CLASS_SUPPORT {
            reasons.push(format!("need at least {MIN_CLASS_SUPPORT} realized samples per class"));
        }
        if window_summaries.len() < MIN_FULL_WINDOWS {
            reasons.push(format!("need at least {MIN_FULL_WINDOWS} full {WINDOW_SIZE}-row chronological context windows"));
        }
        if !integrity_ok {
            reasons.push("immutable model/dataset/review/target linkage is inconsistent".into());
        }
        return Ok(json!({
            "policy":POLICY,"status":"insufficient_evidence","reasons":reasons,"evidence":evidence,
            "metrics":summary_json(&summary),"windows":window_json,
            "requirements":{"samples":MIN_SAMPLES,"per_class":MIN_CLASS_SUPPORT,"window_size":WINDOW_SIZE,"full_windows":MIN_FULL_WINDOWS},
            "promotion_eligible":false,"promotion_enabled":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false,
            "note":"Insufficient evidence is not a model failure and never authorizes promotion or trading influence."
        }));
    }

    let uniform_log_loss = 3.0_f64.ln();
    let uniform_brier = 2.0 / 9.0;
    let majority_accuracy = *summary.support.iter().max().unwrap_or(&0) as f64 / summary.samples as f64;
    let overconfidence = (summary.mean_confidence - summary.accuracy).max(0.0);
    let worst_window_balanced = window_summaries
        .iter().map(|summary| summary.balanced_accuracy).fold(f64::INFINITY, f64::min);
    let worst_window_log_loss = window_summaries
        .iter().map(|summary| summary.mean_log_loss).fold(f64::NEG_INFINITY, f64::max);
    let max_window_ece = window_summaries.iter().map(|summary| summary.ece).fold(0.0, f64::max);
    let min_ba = window_summaries
        .iter().map(|summary| summary.balanced_accuracy).fold(f64::INFINITY, f64::min);
    let max_ba = window_summaries
        .iter().map(|summary| summary.balanced_accuracy).fold(f64::NEG_INFINITY, f64::max);
    let min_ll = window_summaries
        .iter().map(|summary| summary.mean_log_loss).fold(f64::INFINITY, f64::min);
    let max_ll = window_summaries
        .iter().map(|summary| summary.mean_log_loss).fold(f64::NEG_INFINITY, f64::max);
    let ba_spread = max_ba - min_ba;
    let ll_spread = max_ll - min_ll;
    let (early, late) = chronological_halves(&rows)?;
    let distribution_tv = total_variation(class_distribution(&early), class_distribution(&late));
    let half_ba_delta = (late.balanced_accuracy - early.balanced_accuracy).abs();
    let half_ll_delta = (late.mean_log_loss - early.mean_log_loss).abs();

    let checks = vec![
        check("balanced_accuracy", summary.balanced_accuracy, ">=", MIN_BALANCED_ACCURACY, summary.balanced_accuracy >= MIN_BALANCED_ACCURACY),
        check("macro_f1", summary.macro_f1, ">=", MIN_MACRO_F1, summary.macro_f1 >= MIN_MACRO_F1),
        check("log_loss_vs_uniform", summary.mean_log_loss, "<=", uniform_log_loss - MIN_LOG_LOSS_GAIN, summary.mean_log_loss <= uniform_log_loss - MIN_LOG_LOSS_GAIN),
        check("brier_vs_uniform", summary.mean_brier, "<=", uniform_brier - MIN_BRIER_GAIN, summary.mean_brier <= uniform_brier - MIN_BRIER_GAIN),
        check("ece", summary.ece, "<=", MAX_ECE, summary.ece <= MAX_ECE),
        check("overconfidence", overconfidence, "<=", MAX_OVERCONFIDENCE, overconfidence <= MAX_OVERCONFIDENCE),
        check("worst_window_balanced_accuracy", worst_window_balanced, ">=", MIN_WINDOW_BALANCED_ACCURACY, worst_window_balanced >= MIN_WINDOW_BALANCED_ACCURACY),
        check("worst_window_log_loss", worst_window_log_loss, "<=", uniform_log_loss + MAX_WINDOW_LOG_LOSS_MARGIN, worst_window_log_loss <= uniform_log_loss + MAX_WINDOW_LOG_LOSS_MARGIN),
        check("balanced_accuracy_window_spread", ba_spread, "<=", MAX_BALANCED_ACCURACY_SPREAD, ba_spread <= MAX_BALANCED_ACCURACY_SPREAD),
        check("log_loss_window_spread", ll_spread, "<=", MAX_LOG_LOSS_SPREAD, ll_spread <= MAX_LOG_LOSS_SPREAD),
        check("max_window_ece", max_window_ece, "<=", MAX_WINDOW_ECE, max_window_ece <= MAX_WINDOW_ECE),
        check("class_distribution_drift_tv", distribution_tv, "<=", MAX_CLASS_DISTRIBUTION_TV, distribution_tv <= MAX_CLASS_DISTRIBUTION_TV),
        check("half_balanced_accuracy_delta", half_ba_delta, "<=", MAX_HALF_BALANCED_ACCURACY_DELTA, half_ba_delta <= MAX_HALF_BALANCED_ACCURACY_DELTA),
        check("half_log_loss_delta", half_ll_delta, "<=", MAX_HALF_LOG_LOSS_DELTA, half_ll_delta <= MAX_HALF_LOG_LOSS_DELTA),
    ];
    let pass = checks.iter().all(|value| value["pass"] == true);
    Ok(json!({
        "policy":POLICY,"status":if pass{"review_candidate"}else{"reject"},"evidence":evidence,
        "metrics":summary_json(&summary),"windows":window_json,
        "drift":{"early":summary_json(&early),"late":summary_json(&late),"class_distribution_tv":distribution_tv,"balanced_accuracy_delta":half_ba_delta,"log_loss_delta":half_ll_delta},
        "baselines":{"uniform_log_loss":uniform_log_loss,"uniform_brier":uniform_brier,"majority_accuracy":majority_accuracy},
        "checks":checks,
        "promotion_eligible":false,"promotion_enabled":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false,
        "note":"review_candidate means only that immutable scored evidence passed this conservative human-review screen. It is not promotion approval, a profitability claim, or permission to affect Shadow/broker execution."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_models(model_id TEXT PRIMARY KEY,status TEXT NOT NULL,dataset_identity TEXT NOT NULL,artifact TEXT NOT NULL);
             CREATE TABLE learning_slots(slot TEXT PRIMARY KEY,model_id TEXT);
             CREATE TABLE learning_review_records(id TEXT PRIMARY KEY,model_id TEXT NOT NULL,policy TEXT NOT NULL);
             CREATE TABLE learning_predictions(id INTEGER PRIMARY KEY,context TEXT NOT NULL,bar_time INTEGER NOT NULL,model_id TEXT NOT NULL,dataset_identity TEXT NOT NULL,review_record_id TEXT NOT NULL,target TEXT NOT NULL,predicted_class INTEGER NOT NULL,confidence REAL NOT NULL);
             CREATE TABLE learning_prediction_outcomes(prediction_id INTEGER PRIMARY KEY,realized_class INTEGER NOT NULL,log_loss REAL NOT NULL,brier REAL NOT NULL);",
        ).unwrap();
        db.execute(
            "INSERT INTO learning_models VALUES('m1','unreviewed_challenger','dataset-a',?1)",
            [json!({"target":"15m.direction_0_2atr"}).to_string()],
        ).unwrap();
        db.execute("INSERT INTO learning_slots VALUES('challenger','m1')", []).unwrap();
        db.execute(
            "INSERT INTO learning_review_records VALUES('r1','m1',?1)",
            [REVIEW_POLICY],
        ).unwrap();
        db
    }

    fn seed(db: &Connection, count: usize, strong: bool) {
        for index in 0..count {
            let actual = CLASSES[index % 3];
            let predicted = if strong { actual } else { 0 };
            let (confidence, log_loss, brier) = if strong {
                (0.95, -0.95_f64.ln(), 0.00125)
            } else if actual == 0 {
                (0.95, -0.95_f64.ln(), 0.00125)
            } else {
                (0.95, -0.025_f64.ln(), 0.451875)
            };
            let context = if index % 2 == 0 { "ctx-a" } else { "ctx-b" };
            db.execute(
                "INSERT INTO learning_predictions VALUES(?1,?2,?3,'m1','dataset-a','r1','15m.direction_0_2atr',?4,?5)",
                rusqlite::params![index as i64 + 1, context, 60_000u64 + index as u64 * 60, predicted, confidence],
            ).unwrap();
            db.execute(
                "INSERT INTO learning_prediction_outcomes VALUES(?1,?2,?3,?4)",
                rusqlite::params![index as i64 + 1, actual, log_loss, brier],
            ).unwrap();
        }
    }

    fn seed_extra(db: &Connection, id: usize) {
        let actual = CLASSES[id % 3];
        db.execute(
            "INSERT INTO learning_predictions VALUES(?1,'ctx-a',?2,'m1','dataset-a','r1','15m.direction_0_2atr',?3,0.95)",
            rusqlite::params![id as i64, 60_000u64 + id as u64 * 60, actual],
        ).unwrap();
        db.execute(
            "INSERT INTO learning_prediction_outcomes VALUES(?1,?2,?3,0.00125)",
            rusqlite::params![id as i64, actual, -0.95_f64.ln()],
        ).unwrap();
    }

    #[test]
    fn small_sample_is_insufficient() {
        let db = db();
        seed(&db, 300, true);
        let value = review(&db).unwrap();
        assert_eq!(value["status"], "insufficient_evidence");
        assert_eq!(value["promotion_eligible"], false);
    }

    #[test]
    fn weak_constant_model_is_rejected() {
        let db = db();
        seed(&db, 900, false);
        let value = review(&db).unwrap();
        assert_eq!(value["status"], "reject");
        assert_eq!(value["shadow_effect"], false);
    }

    #[test]
    fn strong_stable_model_becomes_review_candidate_only() {
        let db = db();
        seed(&db, 1200, true);
        let value = review(&db).unwrap();
        assert_eq!(value["status"], "review_candidate");
        assert_eq!(value["promotion_eligible"], false);
        assert_eq!(value["broker_effect"], false);
    }

    #[test]
    fn fingerprint_changes_with_evidence() {
        let db = db();
        seed(&db, 900, true);
        let first = review(&db).unwrap()["evidence"]["fingerprint"].clone();
        seed_extra(&db, 901);
        let second = review(&db).unwrap()["evidence"]["fingerprint"].clone();
        assert_ne!(first, second);
    }
}
