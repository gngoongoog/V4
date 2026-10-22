//! Read-only causal audit of the registered IMMUTABLE research Challenger.
//! This does NOT evaluate or freeze the adaptive MT5 trading brains; it audits
//! prospective, post-training direction forecasts only. No broker side effects.
use anyhow::Result;
use rusqlite::{Connection,OptionalExtension};
use serde_json::{json,Value};
use std::collections::{BTreeMap,BTreeSet};

const POLICY:&str="frozen-challenger-prequential-audit-v1";
const REVIEW:&str="manual-review-eligibility-v1";
const MAX_ROWS:usize=4000;
const MIN_SCORED:usize=100;

fn table(db:&Connection,name:&str)->Result<bool>{
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],|r|r.get(0))?)
}
fn unavailable(reason:&str)->Value {
    json!({"policy":POLICY,"status":"insufficient_evidence","reason":reason,
        "promotion_enabled":false,"execution_changed":false,
        "audits_adaptive_brains":false,"read_only":true})
}
fn num(v:&Value)->Option<f64>{v.as_f64().filter(|v|v.is_finite())}
fn selected(db:&Connection)->Result<Option<(String,String,u64,Value,String,u64)>> {
    if ["learning_slots","learning_models","learning_review_records"].iter()
        .any(|t|!table(db,t).unwrap_or(false)) {return Ok(None)}
    let model:Option<(String,String,u64,String,String)>=db.query_row(
        "SELECT m.model_id,m.dataset_identity,m.created_ms,m.status,m.artifact
         FROM learning_slots s JOIN learning_models m ON m.model_id=s.model_id
         WHERE s.slot='challenger' AND s.model_id IS NOT NULL",[],|r|{
            Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
        }).optional()?;
    let Some((id,dataset,created,status,artifact_raw))=model else{return Ok(None)};
    if status!="unreviewed_challenger" {return Ok(None)}
    let review:Option<(String,u64,String,String)>=db.query_row(
        "SELECT id,created_ms,policy,eligibility FROM learning_review_records
         WHERE model_id=?1 ORDER BY created_ms DESC,id DESC LIMIT 1",
        [&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((review_id,review_created,policy,eligibility))=review else{return Ok(None)};
    if policy!=REVIEW {return Ok(None)}
    let eligibility:Value=serde_json::from_str(&eligibility)?;
    if eligibility["eligible_for_manual_review"]!=true
        ||eligibility["eligible_for_promotion"]!=false{return Ok(None)}
    let artifact:Value=serde_json::from_str(&artifact_raw)?;
    if artifact["dataset"]["sha256"].as_str()!=Some(dataset.as_str()) {return Ok(None)}
    Ok(Some((id,dataset,created,artifact,review_id,review_created)))
}
pub fn audit(db:&Connection)->Result<Value>{
    let Some((model_id,dataset,created_ms,artifact,review_id,review_ms))=selected(db)? else{
        return Ok(unavailable("No registered, manually reviewed frozen Challenger model"))
    };
    let target=artifact["target"].as_str().unwrap_or("");
    let horizon:u64=match target{
        "5m.direction_0_2atr"=>5,"15m.direction_0_2atr"=>15,
        "30m.direction_0_2atr"=>30,_=>return Ok(unavailable("Unsupported frozen Challenger target"))
    };
    let Some(splits)=artifact["dataset"]["split"].as_array().filter(|v|!v.is_empty()) else{
        return Ok(unavailable("Missing immutable chronological training split"))
    };
    let mut contexts=BTreeSet::new();let mut last_training_bar=0u64;
    for s in splits{
        let Some(ctx)=s["context"].as_str().filter(|v|!v.is_empty()) else{
            return Ok(unavailable("Invalid training context"))
        };
        let Some(last)=s["holdout_last_bar"].as_u64() else{
            return Ok(unavailable("Missing holdout boundary"))
        };
        last_training_bar=last_training_bar.max(last);
        contexts.insert(ctx.to_string());
    }
    let Some(embargo)=last_training_bar.checked_add(1800) else{
        return Ok(unavailable("Overflowed holdout boundary"))
    };
    if !table(db,"learning_predictions")?||!table(db,"learning_snapshots")?
      ||!table(db,"learning_prediction_outcomes")?{
        return Ok(unavailable("No saved research prediction/label history"))
    }
    let mut stmt=db.prepare(
        "SELECT json_object(
        'id',p.id,'context',p.context,'bar_time',p.bar_time,
        'observed_ms',p.observed_ms,'dataset',p.dataset_identity,
        'review_id',p.review_record_id,'target',p.target,
        'predicted',p.predicted_class,'confidence',p.confidence,
        'p_down',p.p_down,'p_flat',p.p_flat,'p_up',p.p_up,
        'captured_ms',s.captured_ms,'feature_schema',s.feature_schema,
        'future_bar_time',o.future_bar_time,'label_matured_ms',o.label_matured_ms,
        'scored_ms',o.scored_ms,'outcome_target',o.target,'realized',o.realized_class,
        'log_loss',o.log_loss,'brier',o.brier,'correct',o.correct
        ) FROM learning_predictions p
        LEFT JOIN learning_snapshots s ON s.context=p.context AND s.bar_time=p.bar_time
        LEFT JOIN learning_prediction_outcomes o ON o.prediction_id=p.id
        WHERE p.model_id=?1 ORDER BY p.bar_time DESC,p.id DESC LIMIT 4000"
    )?;
    let raws=stmt.query_map([&model_id],|r|r.get::<_,String>(0))?
       .collect::<rusqlite::Result<Vec<_>>>()?;
    let scanned=raws.len();let mut valid=0usize;let mut invalid=0usize;
    let mut pending=0usize;let mut scored=0usize;let mut correct=0usize;
    let mut loss_sum=0.;let mut brier_sum=0.;
    let mut reasons:BTreeMap<String,usize>=BTreeMap::new();
    let mut by_context:BTreeMap<String,(usize,usize)>=BTreeMap::new();
    let mut identities=BTreeSet::new();
    for raw in raws.into_iter().rev(){
        let record:Value=serde_json::from_str(&raw)?;
        let check=||->std::result::Result<(String,Option<(bool,f64,f64)>),&'static str>{
            let context=record["context"].as_str().ok_or("missing_context")?;
            if !contexts.contains(context){return Err("unknown_training_context")}
            if record["dataset"]!=dataset||record["review_id"]!=review_id||record["target"]!=target{
                return Err("stale_or_mismatched_model_identity")}
            let bar=record["bar_time"].as_u64().ok_or("missing_bar")?;
            let observed=record["observed_ms"].as_u64().ok_or("missing_observed_time")?;
            let close=bar.checked_add(horizon*60).ok_or("time_overflow")?;
            let future_ms=close.checked_mul(1000).ok_or("time_overflow")?;
            if bar<=embargo{return Err("overlaps_training_or_embargo")}
            if observed<=created_ms||observed<=review_ms{return Err("before_model_review")}
            if observed<bar.checked_mul(1000).ok_or("time_overflow")?{
                return Err("prediction_before_input_bar")}
            if observed>=future_ms{return Err("prediction_after_target_was_known")}
            let captured=record["captured_ms"].as_u64().ok_or("missing_source_snapshot")?;
            if captured>observed{return Err("prediction_before_source_snapshot")}
            if record["feature_schema"]!=artifact["feature_schema"]{
                return Err("wrong_feature_schema")}
            if !identities.insert((context.to_string(),bar)){return Err("duplicate_context_bar")}
            let mut probs=[0.;3];
            for (i,key) in ["p_down","p_flat","p_up"].iter().enumerate(){
                probs[i]=num(&record[*key]).ok_or("invalid_probability")?;
            }
            if probs.iter().any(|p|!(0.0..=1.).contains(p))
               ||(probs.iter().sum::<f64>()-1.).abs()>1e-9 {
                return Err("invalid_probability")}
            let predicted=record["predicted"].as_i64().ok_or("invalid_prediction")?;
            let idx=match predicted{-1=>0,0=>1,1=>2,_=>return Err("invalid_prediction")};
            if probs[idx]+1e-12<probs.iter().copied().fold(0.,f64::max)
                ||(num(&record["confidence"]).ok_or("invalid_confidence")?
                    -probs[idx]).abs()>1e-8 {return Err("invalid_confidence")}
            if record["future_bar_time"].is_null(){return Ok((context.to_string(),None))}
            let future=record["future_bar_time"].as_u64().ok_or("invalid_outcome")?;
            let matured=record["label_matured_ms"].as_u64().ok_or("invalid_outcome")?;
            let scored_at=record["scored_ms"].as_u64().ok_or("invalid_outcome")?;
            let actual=record["realized"].as_i64().ok_or("invalid_outcome")?;
            let actual_idx=match actual{-1=>0,0=>1,1=>2,_=>return Err("invalid_outcome")};
            if future!=close||matured<future_ms||matured<=observed
                ||scored_at<matured||record["outcome_target"]!=target{
                return Err("leaked_or_mismatched_label")}
            let ll=-probs[actual_idx].max(1e-15).ln();
            let brier=probs.iter().enumerate().map(|(i,p)|{
                let y=if i==actual_idx{1.}else{0.};(p-y)*(p-y)
            }).sum::<f64>()/3.;
            let stored_ll=num(&record["log_loss"]).ok_or("invalid_scored_metric")?;
            let stored_brier=num(&record["brier"]).ok_or("invalid_scored_metric")?;
            let win=predicted==actual;
            if (ll-stored_ll).abs()>1e-7||(brier-stored_brier).abs()>1e-7
               ||record["correct"].as_i64()!=Some(if win{1}else{0}){
                return Err("invalid_scored_metric")}
            Ok((context.to_string(),Some((win,ll,brier))))
        };
        match check(){
            Ok((context,outcome))=>{
                valid+=1;
                let entry=by_context.entry(context).or_default();
                entry.0+=1;
                if let Some((win,ll,brier))=outcome{
                    scored+=1;if win {correct+=1}
                    loss_sum+=ll;brier_sum+=brier;entry.1+=1;
                }else{pending+=1}
            }
            Err(reason)=>{invalid+=1;*reasons.entry(reason.to_string()).or_default()+=1;}
        }
    }
    let status=if invalid>0{"integrity_warning"}
        else if scored<MIN_SCORED{"insufficient_evidence"}
        else{"forward_monitor_only"};
    Ok(json!({
        "policy":POLICY,"status":status,"model_id":model_id,
        "dataset_identity":dataset,"review_record_id":review_id,"target":target,
        "training_end_bar":last_training_bar,"embargo_end_bar":embargo,
        "model_registered_ms":created_ms,"review_created_ms":review_ms,
        "history_cap":MAX_ROWS,"scanned":scanned,"valid_predictions":valid,
        "invalid_predictions":invalid,"invalid_reasons":reasons,
        "pending_future_labels":pending,"scored_future_labels":scored,
        "correct":correct,
        "accuracy":if scored>0{Some(correct as f64/scored as f64)}else{None},
        "mean_log_loss":if scored>0{Some(loss_sum/scored as f64)}else{None},
        "mean_brier":if scored>0{Some(brier_sum/scored as f64)}else{None},
        "context_counts":by_context.iter().map(|(k,(all,done))|json!({"context":k,"predictions":all,"scored":done})).collect::<Vec<_>>(),
        "minimum_scored_for_monitoring":MIN_SCORED,
        "registered_model_frozen":true,
        "audits_adaptive_brains":false,"read_only":true,
        "promotion_enabled":false,"broker_effect":false,
        "unseen_future_bars_required":true,
        "notes":"Audits immutable research Challenger direction forecasts emitted after registration, manual review and training embargo, before realized future labels. NOT a test of continuously learning MT5 trading brains and NOT a trading profit or execution-quality estimate."
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->Connection{
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("
        CREATE TABLE learning_slots(slot TEXT,model_id TEXT);
        CREATE TABLE learning_models(model_id TEXT,dataset_identity TEXT,created_ms INTEGER,status TEXT,artifact TEXT);
        CREATE TABLE learning_review_records(id TEXT,model_id TEXT,created_ms INTEGER,policy TEXT,eligibility TEXT);
        CREATE TABLE learning_snapshots(context TEXT,bar_time INTEGER,captured_ms INTEGER,feature_schema TEXT);
        CREATE TABLE learning_predictions(id INTEGER,model_id TEXT,context TEXT,bar_time INTEGER,observed_ms INTEGER,dataset_identity TEXT,review_record_id TEXT,target TEXT,p_down REAL,p_flat REAL,p_up REAL,predicted_class INTEGER,confidence REAL);
        CREATE TABLE learning_prediction_outcomes(prediction_id INTEGER,future_bar_time INTEGER,label_matured_ms INTEGER,scored_ms INTEGER,target TEXT,realized_class INTEGER,log_loss REAL,brier REAL,correct INTEGER);").unwrap();
        let artifact=json!({"target":"5m.direction_0_2atr","feature_schema":"gold-m1-feature-v1",
            "dataset":{"sha256":"dataset1","split":[{"context":"A","holdout_last_bar":990000}]}});
        db.execute("INSERT INTO learning_slots VALUES('challenger','m1')",[]).unwrap();
        db.execute("INSERT INTO learning_models VALUES('m1','dataset1',999990000,'unreviewed_challenger',?1)",
            [artifact.to_string()]).unwrap();
        let eligibility=json!({"eligible_for_manual_review":true,"eligible_for_promotion":false});
        db.execute("INSERT INTO learning_review_records VALUES('review1','m1',999995000,?1,?2)",
            rusqlite::params![REVIEW,eligibility.to_string()]).unwrap();
        db
    }
    fn row(db:&Connection,id:u64,late:bool,invalid_score:bool){
        let bar=1_000_000+(id-1)*60;
        let observed=if late{(bar+300)*1000}else{(bar+60)*1000};
        db.execute("INSERT INTO learning_snapshots VALUES('A',?1,?2,'gold-m1-feature-v1')",
            rusqlite::params![bar,bar*1000]).unwrap();
        db.execute("INSERT INTO learning_predictions VALUES(?1,'m1','A',?2,?3,'dataset1','review1','5m.direction_0_2atr',0.1,0.1,0.8,1,0.8)",
            rusqlite::params![id,bar,observed]).unwrap();
        let brier=if invalid_score{0.5}else{0.02};
        db.execute("INSERT INTO learning_prediction_outcomes VALUES(?1,?2,?3,?4,'5m.direction_0_2atr',1,?5,?6,1)",
            rusqlite::params![id,bar+300,(bar+360)*1000,(bar+361)*1000,-0.8f64.ln(),brier]).unwrap();
    }
    #[test]
    fn no_challenger_means_insufficient_and_no_execution_effect(){
        let db=Connection::open_in_memory().unwrap();
        let v=audit(&db).unwrap();
        assert_eq!(v["status"],"insufficient_evidence");
        assert_eq!(v["broker_effect"],Value::Null);
        assert_eq!(v["promotion_enabled"],false);
    }
    #[test]
    fn genuine_post_registration_future_timestamps_audited_without_promotion(){
        let db=fixture();
        for id in 1..=120{row(&db,id,false,false)}
        let v=audit(&db).unwrap();
        assert_eq!(v["status"],"forward_monitor_only");
        assert_eq!(v["scored_future_labels"],120);
        assert_eq!(v["invalid_predictions"],0);
        assert_eq!(v["accuracy"],1.);
        assert_eq!(v["promotion_enabled"],false);
        assert_eq!(v["audits_adaptive_brains"],false);
    }
    #[test]
    fn leaks_and_tampered_metrics_are_rejected_not_scored(){
        let db=fixture();
        for id in 1..=101{row(&db,id,id==80,id==81)}
        let v=audit(&db).unwrap();
        assert_eq!(v["status"],"integrity_warning");
        assert_eq!(v["scored_future_labels"],99);
        assert_eq!(v["invalid_predictions"],2);
        assert_eq!(v["invalid_reasons"]["prediction_after_target_was_known"],1);
        assert_eq!(v["invalid_reasons"]["invalid_scored_metric"],1);
    }
}