//! Manual challenger performance-review policy over immutable scored outcomes.
//! Observational only: this module never creates a Champion, changes Shadow/risk,
//! or enables broker execution.
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const POLICY: &str = "challenger-performance-review-v1";
const REVIEW_POLICY: &str = "manual-review-eligibility-v1";
const MIN_TOTAL_SAMPLES: usize = 600;
const MIN_PER_CLASS: usize = 60;
const MIN_CONTEXT_SAMPLES: usize = 150;
const MIN_WINDOW_SAMPLES: usize = 50;
const MIN_REGIME_SAMPLES: usize = 60;
const MIN_REGIME_SLICES: usize = 2;
const MIN_BALANCED_ACCURACY: f64 = 0.40;
const MIN_MACRO_F1: f64 = 0.38;
const MIN_CONTEXT_BALANCED_ACCURACY: f64 = 0.36;
const MIN_WINDOW_BALANCED_ACCURACY: f64 = 0.34;
const MAX_ECE: f64 = 0.12;
const MAX_BALANCED_ACCURACY_SPREAD: f64 = 0.18;
const DRIFT_WARNING: f64 = 0.20;
const DRIFT_REJECT: f64 = 0.35;
const MAJORITY_MARGIN: f64 = 0.01;
const UNIFORM_LOG_LOSS_MARGIN: f64 = 0.02;
const UNIFORM_BRIER_MARGIN: f64 = 0.005;
const CLASSES: [i64; 3] = [-1, 0, 1];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRequest {
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Subject {
    model_id: String,
    dataset_identity: String,
    review_record_id: String,
    target: String,
}

#[derive(Clone, Debug)]
struct Observation {
    prediction_id: i64,
    model_id: String,
    dataset_identity: String,
    review_record_id: String,
    target: String,
    context: String,
    bar_time: u64,
    predicted: i64,
    realized: i64,
    confidence: f64,
    log_loss: f64,
    brier: f64,
    regime: String,
}

fn id_ok(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn class_index(value: i64) -> Result<usize> {
    CLASSES
        .iter()
        .position(|class| *class == value)
        .context("Class must be one of [-1,0,1]")
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let rows = values.collect::<Vec<_>>();
    if rows.is_empty() { 0.0 } else { rows.iter().sum::<f64>() / rows.len() as f64 }
}

fn metrics(rows: &[Observation]) -> Result<Value> {
    let mut support = [0usize; 3];
    let mut predicted = [0usize; 3];
    let mut confusion = [[0usize; 3]; 3];
    let mut correct = 0usize;
    let mut bins = [(0usize, 0usize, 0.0f64); 10];
    for row in rows {
        let actual = class_index(row.realized)?;
        let guess = class_index(row.predicted)?;
        if !row.confidence.is_finite() || !(0.0..=1.0).contains(&row.confidence)
            || !row.log_loss.is_finite() || row.log_loss < 0.0
            || !row.brier.is_finite() || row.brier < 0.0 {
            bail!("Scored outcome contains invalid metrics");
        }
        support[actual] += 1; predicted[guess] += 1; confusion[actual][guess] += 1;
        correct += usize::from(actual == guess);
        let bin = ((row.confidence * 10.0).floor() as usize).min(9);
        bins[bin].0 += 1; bins[bin].1 += usize::from(actual == guess); bins[bin].2 += row.confidence;
    }
    let accuracy = if rows.is_empty() { 0.0 } else { correct as f64 / rows.len() as f64 };
    let balanced_accuracy = mean((0..3).filter(|i| support[*i] > 0).map(|i| confusion[i][i] as f64 / support[i] as f64));
    let macro_f1 = mean((0..3).filter_map(|i| {
        let tp = confusion[i][i] as f64;
        let fp = predicted[i].saturating_sub(confusion[i][i]) as f64;
        let fn_ = support[i].saturating_sub(confusion[i][i]) as f64;
        let d = 2.0 * tp + fp + fn_; (d > 0.0).then_some(2.0 * tp / d)
    }));
    let ece = if rows.is_empty() { 0.0 } else {
        bins.iter().filter(|b| b.0 > 0).map(|(count,ok,conf)| {
            *count as f64 / rows.len() as f64 * (*ok as f64 / *count as f64 - *conf / *count as f64).abs()
        }).sum::<f64>()
    };
    Ok(json!({
        "samples":rows.len(),"accuracy":accuracy,"balanced_accuracy":balanced_accuracy,"macro_f1":macro_f1,
        "mean_log_loss":mean(rows.iter().map(|r|r.log_loss)),"mean_brier":mean(rows.iter().map(|r|r.brier)),
        "mean_confidence":mean(rows.iter().map(|r|r.confidence)),"ece_10_bin":ece,
        "class_support":{"down":support[0],"flat":support[1],"up":support[2]},
        "predicted_support":{"down":predicted[0],"flat":predicted[1],"up":predicted[2]},
        "confusion_realized_rows_predicted_columns":confusion
    }))
}

fn distribution(rows: &[Observation]) -> Result<[f64; 3]> {
    if rows.is_empty() { return Ok([0.0; 3]); }
    let mut support=[0usize;3]; for row in rows { support[class_index(row.realized)?]+=1; }
    Ok([support[0] as f64/rows.len() as f64,support[1] as f64/rows.len() as f64,support[2] as f64/rows.len() as f64])
}
fn total_variation(a:[f64;3],b:[f64;3])->f64{0.5*a.iter().zip(b).map(|(x,y)|(x-y).abs()).sum::<f64>()}
fn metric_f64(value:&Value,key:&str)->Result<f64>{let n=value[key].as_f64().with_context(||format!("Metric {key} missing"))?;if !n.is_finite(){bail!("Metric {key} is non-finite");}Ok(n)}
fn regime_name(features:&Value)->String{let r=&features["regime"];if r["volatility_shock"]==true{"volatility_shock".into()}else if r["tradable_trend"]==true{"tradable_trend".into()}else{"range_or_mixed".into()}}

fn current_subject(db:&Connection)->Result<Option<Subject>>{
    let row:Option<(String,String,String)>=db.query_row(
        "SELECT m.model_id,m.dataset_identity,m.artifact FROM learning_slots s JOIN learning_models m ON m.model_id=s.model_id WHERE s.slot='challenger' AND s.model_id IS NOT NULL AND m.status='unreviewed_challenger'",[],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((model_id,dataset_identity,artifact_raw))=row else{return Ok(None)};
    let artifact:Value=serde_json::from_str(&artifact_raw)?;
    if artifact["dataset"]["sha256"].as_str()!=Some(dataset_identity.as_str()){bail!("Current challenger dataset identity does not match its immutable artifact");}
    let target=artifact["target"].as_str().context("Current challenger target missing")?.to_owned();
    let review:Option<(String,String,String)>=db.query_row(
        "SELECT id,policy,eligibility FROM learning_review_records WHERE model_id=?1 ORDER BY created_ms DESC,id DESC LIMIT 1",[&model_id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((review_record_id,policy,eligibility_raw))=review else{return Ok(None)};
    if policy!=REVIEW_POLICY{return Ok(None)};
    let eligibility:Value=serde_json::from_str(&eligibility_raw)?;
    if eligibility["eligible_for_manual_review"]!=true||eligibility["eligible_for_promotion"]!=false{return Ok(None)}
    Ok(Some(Subject{model_id,dataset_identity,review_record_id,target}))
}

fn collect_rows(db:&Connection,subject:&Subject)->Result<Vec<Observation>>{
    let mut stmt=db.prepare("SELECT p.id,p.model_id,p.dataset_identity,p.review_record_id,p.target,p.context,p.bar_time,p.predicted_class,o.realized_class,p.confidence,o.log_loss,o.brier,s.features FROM learning_prediction_outcomes o JOIN learning_predictions p ON p.id=o.prediction_id JOIN learning_snapshots s ON s.id=o.snapshot_id WHERE p.model_id=?1 AND p.dataset_identity=?2 AND p.review_record_id=?3 AND p.target=?4 AND o.target=p.target ORDER BY p.context,p.bar_time,p.id")?;
    let raw=stmt.query_map(params![&subject.model_id,&subject.dataset_identity,&subject.review_record_id,&subject.target],|r|Ok((
        r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,u64>(6)?,r.get::<_,i64>(7)?,r.get::<_,i64>(8)?,r.get::<_,f64>(9)?,r.get::<_,f64>(10)?,r.get::<_,f64>(11)?,r.get::<_,String>(12)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
    raw.into_iter().map(|(prediction_id,model_id,dataset_identity,review_record_id,target,context,bar_time,predicted,realized,confidence,log_loss,brier,features_raw)|{
        let features:Value=serde_json::from_str(&features_raw)?;Ok(Observation{prediction_id,model_id,dataset_identity,review_record_id,target,context,bar_time,predicted,realized,confidence,log_loss,brier,regime:regime_name(&features)})
    }).collect()
}

fn evidence_signature(subject:&Subject,rows:&[Observation])->String{
    let mut hash=0xcbf29ce484222325u64;let mut feed=|text:&str|{for byte in text.bytes(){hash^=byte as u64;hash=hash.wrapping_mul(0x100000001b3);}};
    for v in [POLICY,subject.model_id.as_str(),subject.dataset_identity.as_str(),subject.review_record_id.as_str(),subject.target.as_str()]{feed(v)}
    for r in rows{feed(&format!("{}|{}|{}|{}|{}|{:.12}|{:.12}|{:.12};",r.prediction_id,r.context,r.bar_time,r.predicted,r.realized,r.confidence,r.log_loss,r.brier));}
    format!("fnv64-v1:{hash:016x}")
}

fn evaluate(subject:&Subject,source_rows:&[Observation])->Result<Value>{
    let mut rows=source_rows.to_vec();rows.sort_by(|a,b|a.context.cmp(&b.context).then(a.bar_time.cmp(&b.bar_time)).then(a.prediction_id.cmp(&b.prediction_id)));
    let mut ids=BTreeSet::new();let mut bars=BTreeSet::new();
    for r in &rows{
        if r.model_id!=subject.model_id||r.dataset_identity!=subject.dataset_identity||r.review_record_id!=subject.review_record_id||r.target!=subject.target{bail!("Scored outcome context does not match the current immutable challenger review subject");}
        if !ids.insert(r.prediction_id){bail!("Duplicate scored outcome id detected");}
        if !bars.insert((r.context.clone(),r.bar_time)){bail!("Duplicate context/bar scored outcome detected");}
    }
    let overall=metrics(&rows)?;let samples=rows.len();
    let support=[overall["class_support"]["down"].as_u64().unwrap_or(0) as usize,overall["class_support"]["flat"].as_u64().unwrap_or(0) as usize,overall["class_support"]["up"].as_u64().unwrap_or(0) as usize];
    let majority_accuracy=if samples==0{0.0}else{*support.iter().max().unwrap_or(&0) as f64/samples as f64};let uniform_log_loss=3.0f64.ln();let uniform_brier=2.0/9.0;
    let mut insufficient=BTreeSet::new();let mut rejected=BTreeSet::new();let mut warnings=BTreeSet::new();
    if samples<MIN_TOTAL_SAMPLES{insufficient.insert("sample_floor");}if support.iter().copied().min().unwrap_or(0)<MIN_PER_CLASS{insufficient.insert("class_support_floor");}
    let mut by_context:BTreeMap<String,Vec<Observation>>=BTreeMap::new();for r in &rows{by_context.entry(r.context.clone()).or_default().push(r.clone());}
    let mut context_reports=Vec::new();let mut all_window_balanced=Vec::new();let mut worst_drift=0.0f64;
    for (context,cr) in &by_context{
        if cr.len()<MIN_CONTEXT_SAMPLES{insufficient.insert("context_sample_floor");}
        let cm=metrics(cr)?;if cr.len()>=MIN_CONTEXT_SAMPLES&&metric_f64(&cm,"balanced_accuracy")?<MIN_CONTEXT_BALANCED_ACCURACY{rejected.insert("context_underperformance");}
        let mut windows=Vec::new();let mut distributions=Vec::new();
        for i in 0..3usize{let start=cr.len()*i/3;let end=cr.len()*(i+1)/3;let slice=&cr[start..end];if slice.len()<MIN_WINDOW_SAMPLES{insufficient.insert("chronological_window_floor");}let wm=metrics(slice)?;if !slice.is_empty(){all_window_balanced.push(metric_f64(&wm,"balanced_accuracy")?);distributions.push(distribution(slice)?);}windows.push(json!({"index":i+1,"samples":slice.len(),"first_bar":slice.first().map(|r|r.bar_time),"last_bar":slice.last().map(|r|r.bar_time),"metrics":wm}));}
        let drift=if distributions.len()==3{total_variation(distributions[0],distributions[2])}else{0.0};worst_drift=worst_drift.max(drift);if drift>DRIFT_WARNING{warnings.insert("class_distribution_drift");}if drift>DRIFT_REJECT{rejected.insert("severe_distribution_drift");}
        context_reports.push(json!({"context":context,"samples":cr.len(),"first_bar":cr.first().map(|r|r.bar_time),"last_bar":cr.last().map(|r|r.bar_time),"metrics":cm,"chronological_windows":windows,"first_to_last_class_distribution_tv":drift}));
    }
    if let (Some(low),Some(high))=(all_window_balanced.iter().copied().reduce(f64::min),all_window_balanced.iter().copied().reduce(f64::max)){if low<MIN_WINDOW_BALANCED_ACCURACY{rejected.insert("chronological_window_underperformance");}if high-low>MAX_BALANCED_ACCURACY_SPREAD{rejected.insert("chronological_instability");}}
    let mut by_regime:BTreeMap<String,Vec<Observation>>=BTreeMap::new();for r in &rows{by_regime.entry(r.regime.clone()).or_default().push(r.clone());}
    let mut regime_reports=Vec::new();let mut eligible_regimes=0usize;for (regime,rr) in &by_regime{if rr.len()>=MIN_REGIME_SAMPLES{eligible_regimes+=1;}regime_reports.push(json!({"regime":regime,"samples":rr.len(),"metrics":metrics(rr)?}));}if eligible_regimes<MIN_REGIME_SLICES{insufficient.insert("regime_diversity_floor");}
    if samples>=MIN_TOTAL_SAMPLES&&support.iter().copied().min().unwrap_or(0)>=MIN_PER_CLASS{
        let accuracy=metric_f64(&overall,"accuracy")?;let balanced=metric_f64(&overall,"balanced_accuracy")?;let f1=metric_f64(&overall,"macro_f1")?;let logloss=metric_f64(&overall,"mean_log_loss")?;let brier=metric_f64(&overall,"mean_brier")?;let ece=metric_f64(&overall,"ece_10_bin")?;
        if accuracy<majority_accuracy+MAJORITY_MARGIN{rejected.insert("accuracy_not_above_majority_baseline");}if balanced<MIN_BALANCED_ACCURACY{rejected.insert("balanced_accuracy_below_floor");}if f1<MIN_MACRO_F1{rejected.insert("macro_f1_below_floor");}if logloss>uniform_log_loss-UNIFORM_LOG_LOSS_MARGIN{rejected.insert("log_loss_not_better_than_uniform");}if brier>uniform_brier-UNIFORM_BRIER_MARGIN{rejected.insert("brier_not_better_than_uniform");}if ece>MAX_ECE{rejected.insert("calibration_ece_too_high");}
    }
    let status=if !insufficient.is_empty(){"insufficient_evidence"}else if !rejected.is_empty(){"reject"}else{"review_candidate"};let signature=evidence_signature(subject,&rows);let outcome_ids=rows.iter().map(|r|r.prediction_id).collect::<Vec<_>>();let boundaries=context_reports.iter().map(|r|json!({"context":r["context"],"samples":r["samples"],"first_bar":r["first_bar"],"last_bar":r["last_bar"]})).collect::<Vec<_>>();
    Ok(json!({"policy":POLICY,"status":status,"model_id":subject.model_id,"dataset_identity":subject.dataset_identity,"review_record_id":subject.review_record_id,"target":subject.target,"samples":samples,"overall":overall,"baselines":{"majority_accuracy":majority_accuracy,"uniform_log_loss":uniform_log_loss,"uniform_brier":uniform_brier},"thresholds":{"min_total_samples":MIN_TOTAL_SAMPLES,"min_per_class":MIN_PER_CLASS,"min_context_samples":MIN_CONTEXT_SAMPLES,"min_window_samples":MIN_WINDOW_SAMPLES,"min_regime_samples":MIN_REGIME_SAMPLES,"min_regime_slices":MIN_REGIME_SLICES,"min_balanced_accuracy":MIN_BALANCED_ACCURACY,"min_macro_f1":MIN_MACRO_F1,"min_context_balanced_accuracy":MIN_CONTEXT_BALANCED_ACCURACY,"min_window_balanced_accuracy":MIN_WINDOW_BALANCED_ACCURACY,"max_ece":MAX_ECE,"max_balanced_accuracy_spread":MAX_BALANCED_ACCURACY_SPREAD,"drift_warning":DRIFT_WARNING,"drift_reject":DRIFT_REJECT,"majority_margin":MAJORITY_MARGIN,"uniform_log_loss_margin":UNIFORM_LOG_LOSS_MARGIN,"uniform_brier_margin":UNIFORM_BRIER_MARGIN},"contexts":context_reports,"regimes":regime_reports,"worst_class_distribution_drift":worst_drift,"insufficient_reason_codes":insufficient.into_iter().collect::<Vec<_>>(),"reject_reason_codes":rejected.into_iter().collect::<Vec<_>>(),"warning_codes":warnings.into_iter().collect::<Vec<_>>(),"evidence_signature":signature,"outcome_ids":outcome_ids,"boundaries":boundaries,"eligible_for_human_consideration":status=="review_candidate","promotion_enabled":false,"champion_created":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false,"note":"A review_candidate only means fixed manual research gates were met on immutable scored outcomes. It is not a Champion, execution authorization or profitability claim."}))
}

pub fn init(db:&Connection)->Result<()>{db.execute_batch("CREATE TABLE IF NOT EXISTS learning_performance_reviews(id TEXT PRIMARY KEY,model_id TEXT NOT NULL,review_record_id TEXT NOT NULL,dataset_identity TEXT NOT NULL,target TEXT NOT NULL,created_ms INTEGER NOT NULL,policy TEXT NOT NULL,status TEXT NOT NULL CHECK(status IN ('review_candidate','reject')),evidence_signature TEXT NOT NULL,outcome_ids TEXT NOT NULL,boundaries TEXT NOT NULL,report TEXT NOT NULL,reviewer_note TEXT NOT NULL,UNIQUE(model_id,review_record_id,target,policy,evidence_signature),FOREIGN KEY(model_id) REFERENCES learning_models(model_id),FOREIGN KEY(review_record_id) REFERENCES learning_review_records(id));CREATE INDEX IF NOT EXISTS learning_performance_reviews_model_idx ON learning_performance_reviews(model_id,created_ms DESC);")?;Ok(())}
pub fn status(db:&Connection)->Result<Value>{let Some(subject)=current_subject(db)? else{return Ok(json!({"policy":POLICY,"status":"insufficient_evidence","reason":"No current challenger with an immutable manual-review eligibility record is available","eligible_for_human_consideration":false,"promotion_enabled":false,"champion_created":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false}))};evaluate(&subject,&collect_rows(db,&subject)?)}
fn create(db:&mut Connection,request:CreateRequest,now:u64)->Result<Value>{if request.note.len()>500{bail!("Reviewer note is too long");}let subject=current_subject(db)?.context("No reviewable current challenger is available")?;let report=evaluate(&subject,&collect_rows(db,&subject)?)?;let status=report["status"].as_str().context("Performance review status missing")?;if status=="insufficient_evidence"{bail!("Insufficient evidence cannot be frozen as a challenger performance review");}let signature=report["evidence_signature"].as_str().context("Evidence signature missing")?;let existing:Option<String>=db.query_row("SELECT id FROM learning_performance_reviews WHERE model_id=?1 AND review_record_id=?2 AND target=?3 AND policy=?4 AND evidence_signature=?5",params![&subject.model_id,&subject.review_record_id,&subject.target,POLICY,signature],|r|r.get(0)).optional()?;if existing.is_some(){bail!("An immutable performance review already exists for this exact scored evidence set");}let id=format!("performance-review-{now}-{}",uuid::Uuid::new_v4().simple());let outcome_ids=serde_json::to_string(&report["outcome_ids"])?;let boundaries=serde_json::to_string(&report["boundaries"])?;let tx=db.transaction()?;tx.execute("INSERT INTO learning_performance_reviews(id,model_id,review_record_id,dataset_identity,target,created_ms,policy,status,evidence_signature,outcome_ids,boundaries,report,reviewer_note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![&id,&subject.model_id,&subject.review_record_id,&subject.dataset_identity,&subject.target,now,POLICY,status,signature,outcome_ids,boundaries,report.to_string(),request.note.trim()])?;tx.commit()?;Ok(json!({"ok":true,"id":id,"status":status,"evidence_signature":signature,"immutable":true,"eligible_for_human_consideration":status=="review_candidate","promotion_enabled":false,"champion_created":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false,"report":report}))}
fn list(db:&Connection)->Result<Value>{let mut stmt=db.prepare("SELECT id,model_id,review_record_id,dataset_identity,target,created_ms,policy,status,evidence_signature,reviewer_note FROM learning_performance_reviews ORDER BY created_ms DESC,id DESC LIMIT 50")?;let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"model_id":r.get::<_,String>(1)?,"review_record_id":r.get::<_,String>(2)?,"dataset_identity":r.get::<_,String>(3)?,"target":r.get::<_,String>(4)?,"created_ms":r.get::<_,u64>(5)?,"policy":r.get::<_,String>(6)?,"status":r.get::<_,String>(7)?,"evidence_signature":r.get::<_,String>(8)?,"reviewer_note":r.get::<_,String>(9)?,"immutable":true,"promotion_enabled":false,"champion_created":false,"shadow_effect":false,"broker_effect":false})))?.collect::<std::result::Result<Vec<_>,_>>()?;Ok(json!({"policy":POLICY,"records":rows,"promotion_enabled":false,"champion_created":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false}))}
fn report(db:&Connection,id:&str)->Result<Value>{if !id_ok(id){bail!("Invalid performance review record id");}type Row=(String,String,String,String,String,u64,String,String,String,String,String,String);let row:Option<Row>=db.query_row("SELECT model_id,review_record_id,dataset_identity,target,policy,created_ms,status,evidence_signature,outcome_ids,boundaries,report,reviewer_note FROM learning_performance_reviews WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?))).optional()?;let Some((model_id,review_record_id,dataset_identity,target,policy,created_ms,status,signature,outcome_ids,boundaries,raw_report,reviewer_note))=row else{bail!("Performance review record not found");};Ok(json!({"id":id,"model_id":model_id,"review_record_id":review_record_id,"dataset_identity":dataset_identity,"target":target,"policy":policy,"created_ms":created_ms,"status":status,"evidence_signature":signature,"outcome_ids":serde_json::from_str::<Value>(&outcome_ids)?,"boundaries":serde_json::from_str::<Value>(&boundaries)?,"report":serde_json::from_str::<Value>(&raw_report)?,"reviewer_note":reviewer_note,"immutable":true,"promotion_enabled":false,"champion_created":false,"shadow_effect":false,"risk_effect":false,"broker_effect":false}))}
pub fn enrich_learning_status(db:&Connection,mut base:Value)->Result<Value>{if let Some(obj)=base.as_object_mut(){obj.insert("challenger_performance_review".into(),status(db)?);obj.insert("challenger_performance_policy".into(),Value::String(POLICY.into()));}Ok(base)}
pub fn route(method:&str,path:&str,body:&[u8],db:&mut Connection,now:u64)->Result<Value>{match(method,path){("GET","/v1/learning/performance-review/status")=>status(db),("GET","/v1/learning/performance-review/records")=>list(db),("POST","/v1/learning/performance-review/create")=>create(db,serde_json::from_slice(body)?,now),("POST","/v1/learning/performance-review/report")=>{let request:Selection=serde_json::from_slice(body)?;report(db,&request.id)},_=>bail!("Unknown challenger performance-review route")}}

#[cfg(test)]
mod tests{
    use super::*;
    fn subject()->Subject{Subject{model_id:"m1".into(),dataset_identity:"dataset".into(),review_record_id:"review-1".into(),target:"15m.direction_0_2atr".into()}}
    fn rows(count:usize,contexts:usize,good:bool)->Vec<Observation>{let s=subject();(0..count).map(|i|{let realized=CLASSES[i%3];let predicted=if good{realized}else{CLASSES[(i+1)%3]};Observation{prediction_id:i as i64+1,model_id:s.model_id.clone(),dataset_identity:s.dataset_identity.clone(),review_record_id:s.review_record_id.clone(),target:s.target.clone(),context:format!("ctx-{}",i%contexts),bar_time:1_000_000+(i/contexts) as u64*60,predicted,realized,confidence:if good{0.95}else{0.80},log_loss:if good{-0.90f64.ln()}else{-0.10f64.ln()},brier:if good{0.01}else{0.40},regime:if(i/30)%2==0{"tradable_trend".into()}else{"range_or_mixed".into()}}}).collect()}
    #[test]fn insufficient_sample_is_not_a_candidate(){let r=evaluate(&subject(),&rows(300,1,true)).unwrap();assert_eq!(r["status"],"insufficient_evidence");assert_eq!(r["eligible_for_human_consideration"],false);assert_eq!(r["promotion_enabled"],false);}
    #[test]fn stable_large_scored_sample_becomes_review_candidate_only(){let r=evaluate(&subject(),&rows(660,2,true)).unwrap();assert_eq!(r["status"],"review_candidate");assert_eq!(r["eligible_for_human_consideration"],true);assert_eq!(r["promotion_enabled"],false);assert_eq!(r["champion_created"],false);assert_eq!(r["shadow_effect"],false);assert_eq!(r["broker_effect"],false);}
    #[test]fn large_but_bad_sample_is_rejected(){let r=evaluate(&subject(),&rows(660,2,false)).unwrap();assert_eq!(r["status"],"reject");assert!(!r["reject_reason_codes"].as_array().unwrap().is_empty());assert_eq!(r["promotion_enabled"],false);}
    #[test]fn duplicate_outcome_ids_fail_closed(){let mut v=rows(660,2,true);v[1].prediction_id=v[0].prediction_id;assert!(evaluate(&subject(),&v).unwrap_err().to_string().contains("Duplicate scored outcome"));}
    #[test]fn reordered_evidence_is_reproducible(){let v=rows(660,2,true);let a=evaluate(&subject(),&v).unwrap();let mut rev=v.clone();rev.reverse();let b=evaluate(&subject(),&rev).unwrap();assert_eq!(a["status"],b["status"]);assert_eq!(a["evidence_signature"],b["evidence_signature"]);assert_eq!(a["boundaries"],b["boundaries"]);}
    #[test]fn mismatched_context_identity_fails_closed(){let mut v=rows(660,2,true);v[20].review_record_id="other-review".into();assert!(evaluate(&subject(),&v).unwrap_err().to_string().contains("does not match"));}
}
