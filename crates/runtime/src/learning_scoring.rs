//! Out-of-sample scoring for read-only challenger predictions.
//! Outcomes are descriptive audit evidence only and never authorize promotion or trading influence.
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

const VERSION: &str = "prediction-outcome-scoring-v1";
const LABEL_SCHEMA: &str = "gold-m1-outcome-v1";
const CLASSES: [i64; 3] = [-1, 0, 1];

#[derive(Default)]
pub struct Service {
    last_error: String,
}

#[derive(Clone, Debug)]
struct ScoredRow {
    predicted: i64,
    realized: i64,
    confidence: f64,
    log_loss: f64,
    brier: f64,
}

fn class_index(value: i64) -> Result<usize> {
    CLASSES
        .iter()
        .position(|class| *class == value)
        .context("Class must be one of [-1,0,1]")
}

fn horizon_minutes(target: &str) -> Result<u64> {
    match target {
        "5m.direction_0_2atr" => Ok(5),
        "15m.direction_0_2atr" => Ok(15),
        "30m.direction_0_2atr" => Ok(30),
        _ => bail!("Unsupported prediction target"),
    }
}

fn finite(value: &Value, name: &str) -> Result<f64> {
    let number = value
        .as_f64()
        .with_context(|| format!("Label {name} missing or non-numeric"))?;
    if !number.is_finite() {
        bail!("Label {name} is not finite");
    }
    Ok(number)
}

fn score_one(
    target: &str,
    predicted_class: i64,
    probabilities: [f64; 3],
    confidence: f64,
    label_schema: &str,
    label: &Value,
    bar_time: u64,
) -> Result<Value> {
    if label_schema != LABEL_SCHEMA || label["schema"] != LABEL_SCHEMA {
        bail!("Prediction label schema mismatch");
    }
    if label["reference_bar_time"].as_u64() != Some(bar_time) {
        bail!("Prediction label reference bar does not match prediction");
    }
    let total = probabilities.iter().sum::<f64>();
    if probabilities
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0 || *value > 1.0)
        || !total.is_finite()
        || (total - 1.0).abs() > 1e-9
        || !confidence.is_finite()
        || !(0.0..=1.0).contains(&confidence)
    {
        bail!("Stored prediction probabilities are invalid");
    }
    let predicted_index = class_index(predicted_class)?;
    if probabilities[predicted_index] + 1e-12 < probabilities.iter().copied().fold(0.0, f64::max) {
        bail!("Stored predicted class is inconsistent with probabilities");
    }
    let minutes = horizon_minutes(target)?;
    let horizon = label["horizons"]
        .as_array()
        .context("Label horizons missing")?
        .iter()
        .find(|row| row["minutes"].as_u64() == Some(minutes))
        .with_context(|| format!("Exact {minutes}m label horizon missing"))?;
    let realized_class = horizon["direction_0_2atr"]
        .as_i64()
        .context("Realized direction class missing")?;
    let realized_index = class_index(realized_class)?;
    let return_atr = finite(&horizon["return_atr"], "return_atr")?;
    let mfe_atr = finite(&horizon["mfe_atr"], "mfe_atr")?;
    let mae_atr = finite(&horizon["mae_atr"], "mae_atr")?;
    let future_bar_time = horizon["future_bar_time"]
        .as_u64()
        .context("Future bar time missing")?;
    if future_bar_time != bar_time.saturating_add(minutes * 60) {
        bail!("Label horizon does not end at the exact requested future bar");
    }
    let true_probability = probabilities[realized_index];
    let log_loss = -true_probability.max(1e-15).ln();
    let brier = probabilities
        .iter()
        .enumerate()
        .map(|(index, probability)| {
            let expected = if index == realized_index { 1.0 } else { 0.0 };
            (probability - expected).powi(2)
        })
        .sum::<f64>()
        / 3.0;
    let correct = predicted_class == realized_class;
    let calibration_error = (confidence - if correct { 1.0 } else { 0.0 }).abs();
    if [true_probability, log_loss, brier, calibration_error]
        .iter()
        .any(|value| !value.is_finite())
    {
        bail!("Prediction scoring produced a non-finite metric");
    }
    Ok(json!({
        "horizon_minutes":minutes,
        "future_bar_time":future_bar_time,
        "realized_class":realized_class,
        "correct":correct,
        "true_probability":true_probability,
        "log_loss":log_loss,
        "brier":brier,
        "calibration_error":calibration_error,
        "return_atr":return_atr,
        "mfe_atr":mfe_atr,
        "mae_atr":mae_atr
    }))
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let rows = values.collect::<Vec<_>>();
    if rows.is_empty() {
        0.0
    } else {
        rows.iter().sum::<f64>() / rows.len() as f64
    }
}

fn metrics(rows: &[ScoredRow]) -> Value {
    let mut support = [0usize; 3];
    let mut predicted = [0usize; 3];
    let mut confusion = [[0usize; 3]; 3];
    let mut correct = 0usize;
    let mut bins = [(0usize, 0usize, 0.0f64); 10];
    for row in rows {
        let actual = class_index(row.realized).unwrap();
        let guess = class_index(row.predicted).unwrap();
        support[actual] += 1;
        predicted[guess] += 1;
        confusion[actual][guess] += 1;
        correct += usize::from(actual == guess);
        let bin = ((row.confidence * 10.0).floor() as usize).min(9);
        bins[bin].0 += 1;
        bins[bin].1 += usize::from(actual == guess);
        bins[bin].2 += row.confidence;
    }
    let accuracy = if rows.is_empty() {
        0.0
    } else {
        correct as f64 / rows.len() as f64
    };
    let recalls = (0..3)
        .filter(|index| support[*index] > 0)
        .map(|index| confusion[index][index] as f64 / support[index] as f64)
        .collect::<Vec<_>>();
    let balanced_accuracy = mean(recalls.into_iter());
    let f1 = (0..3)
        .filter_map(|index| {
            let tp = confusion[index][index] as f64;
            let fp = predicted[index].saturating_sub(confusion[index][index]) as f64;
            let fn_ = support[index].saturating_sub(confusion[index][index]) as f64;
            let denominator = 2.0 * tp + fp + fn_;
            (denominator > 0.0).then_some(2.0 * tp / denominator)
        })
        .collect::<Vec<_>>();
    let macro_f1 = mean(f1.into_iter());
    let ece = if rows.is_empty() {
        0.0
    } else {
        bins.iter()
            .filter(|bin| bin.0 > 0)
            .map(|(count, bin_correct, confidence_sum)| {
                let bin_accuracy = *bin_correct as f64 / *count as f64;
                let bin_confidence = *confidence_sum / *count as f64;
                *count as f64 / rows.len() as f64 * (bin_accuracy - bin_confidence).abs()
            })
            .sum::<f64>()
    };
    let class_floor = support.iter().copied().min().unwrap_or(0);
    json!({
        "samples":rows.len(),
        "accuracy":accuracy,
        "balanced_accuracy":balanced_accuracy,
        "macro_f1":macro_f1,
        "mean_log_loss":mean(rows.iter().map(|row|row.log_loss)),
        "mean_brier":mean(rows.iter().map(|row|row.brier)),
        "mean_confidence":mean(rows.iter().map(|row|row.confidence)),
        "ece_10_bin":ece,
        "class_support":{"down":support[0],"flat":support[1],"up":support[2]},
        "predicted_support":{"down":predicted[0],"flat":predicted[1],"up":predicted[2]},
        "confusion_realized_rows_predicted_columns":confusion,
        "small_sample_warning":rows.len()<300 || class_floor<30,
        "descriptive_sample_floor":300,
        "descriptive_per_class_floor":30,
        "promotion_eligible":false,
        "note":"These are descriptive out-of-sample audit metrics. Sample floors are reporting warnings, not promotion thresholds."
    })
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS learning_prediction_outcomes(
                prediction_id INTEGER PRIMARY KEY,
                snapshot_id INTEGER NOT NULL,
                label_matured_ms INTEGER NOT NULL,
                scored_ms INTEGER NOT NULL,
                label_schema TEXT NOT NULL,
                target TEXT NOT NULL,
                horizon_minutes INTEGER NOT NULL,
                future_bar_time INTEGER NOT NULL,
                realized_class INTEGER NOT NULL CHECK(realized_class IN (-1,0,1)),
                correct INTEGER NOT NULL CHECK(correct IN (0,1)),
                true_probability REAL NOT NULL,
                log_loss REAL NOT NULL,
                brier REAL NOT NULL,
                calibration_error REAL NOT NULL,
                return_atr REAL NOT NULL,
                mfe_atr REAL NOT NULL,
                mae_atr REAL NOT NULL,
                FOREIGN KEY(prediction_id) REFERENCES learning_predictions(id),
                FOREIGN KEY(snapshot_id) REFERENCES learning_snapshots(id)
            );
            CREATE INDEX IF NOT EXISTS learning_prediction_outcomes_scored_idx
                ON learning_prediction_outcomes(scored_ms DESC,prediction_id DESC);",
        )?;
        Ok(Self::default())
    }

    fn score_pending(&self, db: &Connection, now: u64) -> Result<usize> {
        let mut statement = db.prepare(
            "SELECT p.id,p.context,p.bar_time,p.target,p.predicted_class,p.confidence,
                    p.p_down,p.p_flat,p.p_up,s.id,l.matured_ms,l.label_schema,l.label
             FROM learning_predictions p
             JOIN learning_snapshots s ON s.context=p.context AND s.bar_time=p.bar_time
             JOIN learning_labels l ON l.snapshot_id=s.id
             LEFT JOIN learning_prediction_outcomes o ON o.prediction_id=p.id
             WHERE o.prediction_id IS NULL
             ORDER BY p.id LIMIT 500",
        )?;
        let pending = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, f64>(5)?,
                    [
                        row.get::<_, f64>(6)?,
                        row.get::<_, f64>(7)?,
                        row.get::<_, f64>(8)?,
                    ],
                    row.get::<_, i64>(9)?,
                    row.get::<_, u64>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        let tx = db.unchecked_transaction()?;
        let mut inserted = 0usize;
        for (
            prediction_id,
            bar_time,
            target,
            predicted_class,
            confidence,
            probabilities,
            snapshot_id,
            matured_ms,
            label_schema,
            label_raw,
        ) in pending
        {
            let label: Value = serde_json::from_str(&label_raw)?;
            let score = score_one(
                &target,
                predicted_class,
                probabilities,
                confidence,
                &label_schema,
                &label,
                bar_time,
            )?;
            inserted += tx.execute(
                "INSERT OR IGNORE INTO learning_prediction_outcomes(
                    prediction_id,snapshot_id,label_matured_ms,scored_ms,label_schema,target,
                    horizon_minutes,future_bar_time,realized_class,correct,true_probability,
                    log_loss,brier,calibration_error,return_atr,mfe_atr,mae_atr
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![
                    prediction_id,
                    snapshot_id,
                    matured_ms,
                    now,
                    label_schema,
                    target,
                    score["horizon_minutes"].as_u64().unwrap(),
                    score["future_bar_time"].as_u64().unwrap(),
                    score["realized_class"].as_i64().unwrap(),
                    i64::from(score["correct"] == true),
                    score["true_probability"].as_f64().unwrap(),
                    score["log_loss"].as_f64().unwrap(),
                    score["brier"].as_f64().unwrap(),
                    score["calibration_error"].as_f64().unwrap(),
                    score["return_atr"].as_f64().unwrap(),
                    score["mfe_atr"].as_f64().unwrap(),
                    score["mae_atr"].as_f64().unwrap(),
                ],
            )?;
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn observe(&mut self, db: &Connection, now: u64) {
        match self.score_pending(db, now) {
            Ok(_) => self.last_error.clear(),
            Err(error) => self.last_error = error.to_string().chars().take(240).collect(),
        }
    }

    fn groups(&self, db: &Connection) -> Result<Vec<Value>> {
        let mut group_statement = db.prepare(
            "SELECT DISTINCT p.model_id,p.target
             FROM learning_predictions p
             ORDER BY p.model_id,p.target",
        )?;
        let groups = group_statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out = Vec::with_capacity(groups.len());
        for (model_id, target) in groups {
            let total = db.query_row(
                "SELECT COUNT(*) FROM learning_predictions WHERE model_id=?1 AND target=?2",
                params![model_id, target],
                |row| row.get::<_, i64>(0),
            )?;
            let mut statement = db.prepare(
                "SELECT p.predicted_class,o.realized_class,p.confidence,o.log_loss,o.brier
                 FROM learning_prediction_outcomes o
                 JOIN learning_predictions p ON p.id=o.prediction_id
                 WHERE p.model_id=?1 AND p.target=?2 ORDER BY p.bar_time,p.id",
            )?;
            let rows = statement
                .query_map(params![model_id, target], |row| {
                    Ok(ScoredRow {
                        predicted: row.get(0)?,
                        realized: row.get(1)?,
                        confidence: row.get(2)?,
                        log_loss: row.get(3)?,
                        brier: row.get(4)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            out.push(json!({
                "model_id":model_id,
                "target":target,
                "predictions":total,
                "scored":rows.len(),
                "pending":total.saturating_sub(rows.len() as i64),
                "metrics":metrics(&rows),
                "promotion_eligible":false
            }));
        }
        Ok(out)
    }

    pub fn status(&self, db: &Connection) -> Result<Value> {
        let predictions = db.query_row("SELECT COUNT(*) FROM learning_predictions", [], |row| {
            row.get::<_, i64>(0)
        })?;
        let scored = db.query_row("SELECT COUNT(*) FROM learning_prediction_outcomes", [], |row| {
            row.get::<_, i64>(0)
        })?;
        Ok(json!({
            "version":VERSION,
            "predictions":predictions,
            "scored":scored,
            "pending":predictions-scored,
            "groups":self.groups(db)?,
            "last_error":self.last_error,
            "promotion_enabled":false,
            "shadow_effect":false,
            "broker_effect":false,
            "note":"Only exact matured future labels score stored read-only predictions. Metrics are descriptive audit evidence."
        }))
    }

    fn recent(&self, db: &Connection) -> Result<Value> {
        let mut statement = db.prepare(
            "SELECT p.model_id,p.context,p.bar_time,p.target,p.predicted_class,p.confidence,
                    o.scored_ms,o.horizon_minutes,o.future_bar_time,o.realized_class,o.correct,
                    o.true_probability,o.log_loss,o.brier,o.calibration_error,o.return_atr,o.mfe_atr,o.mae_atr,
                    p.review_record_id,p.dataset_identity
             FROM learning_prediction_outcomes o
             JOIN learning_predictions p ON p.id=o.prediction_id
             ORDER BY o.scored_ms DESC,o.prediction_id DESC LIMIT 100",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(json!({
                    "model_id":row.get::<_,String>(0)?,
                    "context":row.get::<_,String>(1)?,
                    "bar_time":row.get::<_,u64>(2)?,
                    "target":row.get::<_,String>(3)?,
                    "predicted_class":row.get::<_,i64>(4)?,
                    "confidence":row.get::<_,f64>(5)?,
                    "scored_ms":row.get::<_,u64>(6)?,
                    "horizon_minutes":row.get::<_,u64>(7)?,
                    "future_bar_time":row.get::<_,u64>(8)?,
                    "realized_class":row.get::<_,i64>(9)?,
                    "correct":row.get::<_,i64>(10)?==1,
                    "true_probability":row.get::<_,f64>(11)?,
                    "log_loss":row.get::<_,f64>(12)?,
                    "brier":row.get::<_,f64>(13)?,
                    "calibration_error":row.get::<_,f64>(14)?,
                    "return_atr":row.get::<_,f64>(15)?,
                    "mfe_atr":row.get::<_,f64>(16)?,
                    "mae_atr":row.get::<_,f64>(17)?,
                    "review_record_id":row.get::<_,String>(18)?,
                    "dataset_identity":row.get::<_,String>(19)?,
                    "promotion_eligible":false,
                    "shadow_effect":false
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({"outcomes":rows,"promotion_enabled":false,"shadow_effect":false,"broker_effect":false}))
    }

    pub fn route(&self, method: &str, path: &str, db: &Connection) -> Result<Value> {
        match (method, path) {
            ("GET", "/v1/learning/scoring") => self.status(db),
            ("GET", "/v1/learning/scoring/recent") => self.recent(db),
            _ => bail!("Unknown learning scoring route"),
        }
    }

    pub fn enrich_learning_status(&self, db: &Connection, mut base: Value) -> Result<Value> {
        if let Some(object) = base.as_object_mut() {
            object.insert("prediction_scoring".into(), self.status(db)?);
        }
        Ok(base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(bar_time: u64, include_15m: bool) -> Value {
        let horizons = [5u64, 15, 30]
            .into_iter()
            .filter(|minutes| include_15m || *minutes != 15)
            .map(|minutes| {
                json!({
                    "minutes":minutes,
                    "future_bar_time":bar_time+minutes*60,
                    "future_close":2000.0,
                    "return_atr":if minutes==15 {0.5} else {0.0},
                    "mfe_atr":0.7,
                    "mae_atr":-0.2,
                    "direction_0_2atr":if minutes==15 {1} else {0}
                })
            })
            .collect::<Vec<_>>();
        json!({
            "schema":LABEL_SCHEMA,
            "reference_bar_time":bar_time,
            "reference_close":1999.0,
            "reference_atr":2.0,
            "horizons":horizons
        })
    }

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE learning_snapshots(id INTEGER PRIMARY KEY,context TEXT NOT NULL,bar_time INTEGER NOT NULL,UNIQUE(context,bar_time));
             CREATE TABLE learning_labels(snapshot_id INTEGER PRIMARY KEY,matured_ms INTEGER NOT NULL,label_schema TEXT NOT NULL,label TEXT NOT NULL);
             CREATE TABLE learning_predictions(id INTEGER PRIMARY KEY,context TEXT NOT NULL,bar_time INTEGER NOT NULL,observed_ms INTEGER NOT NULL,model_id TEXT NOT NULL,dataset_identity TEXT NOT NULL,review_record_id TEXT NOT NULL,target TEXT NOT NULL,p_down REAL NOT NULL,p_flat REAL NOT NULL,p_up REAL NOT NULL,predicted_class INTEGER NOT NULL,confidence REAL NOT NULL,feature_schema TEXT NOT NULL,UNIQUE(model_id,context,bar_time));",
        )
        .unwrap();
        Service::new(&db).unwrap();
        db
    }

    fn prediction(db: &Connection, id: i64, bar_time: u64, probabilities: [f64; 3], class: i64) {
        db.execute(
            "INSERT INTO learning_snapshots(id,context,bar_time) VALUES(?1,'ctx',?2)",
            params![id,bar_time],
        )
        .unwrap();
        db.execute(
            "INSERT INTO learning_predictions(id,context,bar_time,observed_ms,model_id,dataset_identity,review_record_id,target,p_down,p_flat,p_up,predicted_class,confidence,feature_schema)
             VALUES(?1,'ctx',?2,10,'m1','sha','r1','15m.direction_0_2atr',?3,?4,?5,?6,?7,'gold-m1-feature-v1')",
            params![
                id,
                bar_time,
                probabilities[0],
                probabilities[1],
                probabilities[2],
                class,
                probabilities.iter().copied().fold(0.0, f64::max)
            ],
        )
        .unwrap();
    }

    #[test]
    fn prediction_stays_pending_until_exact_label_matures() {
        let db = db();
        prediction(&db, 1, 60_000, [0.1, 0.2, 0.7], 1);
        let mut service = Service::new(&db).unwrap();
        service.observe(&db, 20);
        assert_eq!(service.status(&db).unwrap()["pending"], 1);
        db.execute(
            "INSERT INTO learning_labels VALUES(1,30,?1,?2)",
            params![LABEL_SCHEMA,label(60_000,true).to_string()],
        )
        .unwrap();
        service.observe(&db, 40);
        let status=service.status(&db).unwrap();
        assert_eq!(status["scored"],1);
        assert_eq!(status["pending"],0);
        let recent=service.recent(&db).unwrap();
        let row=&recent["outcomes"][0];
        assert_eq!(row["realized_class"],1);
        assert_eq!(row["correct"],true);
        assert!((row["true_probability"].as_f64().unwrap()-0.7).abs()<1e-12);
        assert_eq!(row["future_bar_time"],60_000+15*60);
    }

    #[test]
    fn missing_exact_horizon_fails_closed_without_outcome() {
        let db = db();
        prediction(&db, 1, 60_000, [0.1, 0.2, 0.7], 1);
        db.execute(
            "INSERT INTO learning_labels VALUES(1,30,?1,?2)",
            params![LABEL_SCHEMA,label(60_000,false).to_string()],
        )
        .unwrap();
        let mut service=Service::new(&db).unwrap();
        service.observe(&db,40);
        assert!(service.status(&db).unwrap()["last_error"].as_str().unwrap().contains("15m"));
        assert_eq!(db.query_row("SELECT COUNT(*) FROM learning_prediction_outcomes",[],|row|row.get::<_,i64>(0)).unwrap(),0);
    }

    #[test]
    fn aggregate_metrics_are_deterministic_and_never_promotable() {
        let db=db();
        for (id,class,probs) in [
            (1,-1,[0.8,0.1,0.1]),
            (2,0,[0.1,0.8,0.1]),
            (3,1,[0.1,0.1,0.8]),
        ]{
            let bar=60_000+id as u64*3_600;
            prediction(&db,id,bar,probs,class);
            let mut value=label(bar,true);
            value["horizons"][1]["direction_0_2atr"]=json!(class);
            db.execute("INSERT INTO learning_labels VALUES(?1,30,?2,?3)",params![id,LABEL_SCHEMA,value.to_string()]).unwrap();
        }
        let mut service=Service::new(&db).unwrap();service.observe(&db,50);
        let group=&service.status(&db).unwrap()["groups"][0];
        assert_eq!(group["metrics"]["samples"],3);
        assert_eq!(group["metrics"]["accuracy"],1.0);
        assert_eq!(group["metrics"]["balanced_accuracy"],1.0);
        assert_eq!(group["metrics"]["macro_f1"],1.0);
        assert_eq!(group["metrics"]["promotion_eligible"],false);
        assert_eq!(group["metrics"]["small_sample_warning"],true);
    }
}
