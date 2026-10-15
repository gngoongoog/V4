//! Leakage-aware learning dataset and immutable research-model registry.
//! No prediction, champion promotion or live self-modification exists in v1.
use anyhow::{bail, Result};
use braintrade_domain::{Analysis, Candle, Telemetry};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{collections::HashSet, fs, io::Write, path::Path};

const FEATURE_SCHEMA: &str = "gold-m1-feature-v1";
const LABEL_SCHEMA: &str = "gold-m1-outcome-v1";
const MODEL_SCHEMA: &str = "braintrade-linear-softmax-v1";
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
    if weight <= f64::EPSILON { 0.0 } else {
        (a.votes.iter().map(|v| v.direction as f64 * v.evidence.max(0.0)).sum::<f64>() / weight).clamp(-1.0, 1.0)
    }
}
fn feature_payload(t: &Telemetry, a: &Analysis) -> Option<(f64, f64, Value)> {
    if !a.current || a.bar_time == 0 || a.symbol != t.symbol { return None; }
    let market=t.market.as_ref()?; let last=market.m1.last()?;
    if last.time!=a.bar_time || !last.close.is_finite() || last.close<=0.0 {return None;}
    let atr=a.atr.filter(|v|v.is_finite()&&*v>0.0)?;
    let opportunities=a.opportunities.as_ref();
    let nearest_zone=opportunities.and_then(|opps|opps.zones.iter().filter(|z|!z.broken).min_by(|l,r|(((l.low+l.high)*0.5-last.close).abs()).total_cmp(&(((r.low+r.high)*0.5-last.close).abs()))));
    let top_setup=opportunities.and_then(|opps|opps.setups.first());
    let gann=opportunities.map(|opps|opps.gann.iter().map(|f|json!({"timeframe":f.timeframe,"available":f.available,"direction":f.direction,"strength":f.strength,"atr_per_bar":f.atr_per_bar,"nearest_eighth":f.nearest_eighth,"distance_atr":f.distance_atr})).collect::<Vec<_>>()).unwrap_or_default();
    let momentum=opportunities.map(|opps|opps.momentum.iter().map(|m|json!({"timeframe":m.timeframe,"direction":m.direction,"roc3_atr":m.roc3_atr,"roc8_atr":m.roc8_atr,"acceleration_atr":m.acceleration_atr,"rsi14":m.rsi14,"strength":m.strength,"divergence":m.divergence})).collect::<Vec<_>>()).unwrap_or_default();
    let active_zones=opportunities.map(|opps|opps.zones.iter().filter(|z|!z.broken).count()).unwrap_or(0);
    let profile=opportunities.map(|opps|&opps.profile); let session=opportunities.map(|opps|&opps.session);
    let payload=json!({
        "schema":FEATURE_SCHEMA,"scope":"closed-bars-only; no quote projection, DOM or news in v1","symbol":t.symbol,"bar_time":a.bar_time,"close":last.close,"atr":atr,
        "legacy_directional_score":directional_score(a),
        "trend":{"direction":a.trend_brain.direction,"evidence":a.trend_brain.evidence,"aligned":a.trend_brain.aligned,"frames":a.trend_brain.frames},
        "regime":{"label":a.regime_brain.label,"direction":a.regime_brain.direction,"trend_evidence":a.regime_brain.trend_evidence,"efficiency":a.regime_brain.efficiency,"volatility_ratio":a.regime_brain.volatility_ratio,"tradable_trend":a.regime_brain.tradable_trend,"structure_direction":a.regime_brain.structure_brain.direction,"structure_evidence":a.regime_brain.structure_brain.evidence,"structure_consensus":a.regime_brain.structure_brain.consensus,"volatility_state":a.regime_brain.volatility_brain.state,"volatility_shock":a.regime_brain.volatility_brain.shock,"volatility_evidence":a.regime_brain.volatility_brain.evidence,"meta_v2_direction":a.regime_brain.meta_v2.direction,"meta_v2_evidence":a.regime_brain.meta_v2.evidence,"meta_v2_eligible":a.regime_brain.meta_v2.eligible},
        "momentum":momentum,"gann":gann,
        "zones":{"active_count":active_zones,"nearest_side":nearest_zone.map(|z|z.side),"nearest_strength":nearest_zone.map(|z|z.strength),"nearest_mid_distance_atr":nearest_zone.map(|z|((z.low+z.high)*0.5-last.close)/atr),"nearest_width_atr":nearest_zone.map(|z|(z.high-z.low)/atr)},
        "setup":{"count":opportunities.map(|o|o.setups.len()).unwrap_or(0),"top_pattern":top_setup.map(|s|s.pattern.clone()),"top_side":top_setup.map(|s|s.side),"top_evidence":top_setup.map(|s|s.evidence)},
        "activity":{"tick_activity_ratio":opportunities.and_then(|o|o.tick_activity_ratio),"profile_available":profile.map(|p|p.available).unwrap_or(false),"poc_distance_atr":profile.filter(|p|p.available).map(|p|(p.poc-last.close)/atr),"value_low_distance_atr":profile.filter(|p|p.available).map(|p|(p.value_low-last.close)/atr),"value_high_distance_atr":profile.filter(|p|p.available).map(|p|(p.value_high-last.close)/atr)},
        "session":{"covers_day_open":session.map(|s|s.covers_day_open).unwrap_or(false),"open_distance_atr":session.map(|s|(last.close-s.open)/atr),"high_distance_atr":session.map(|s|(s.high-last.close)/atr),"low_distance_atr":session.map(|s|(last.close-s.low)/atr),"vwap_distance_atr":session.and_then(|s|s.vwap_proxy).map(|v|(last.close-v)/atr)}
    });
    Some((last.close,atr,payload))
}
fn exact_window<'a>(series:&'a [Candle],start:u64,minutes:u64)->Option<&'a [Candle]>{
    let first=series.iter().position(|b|b.time==start+60)?;let last_time=start.checked_add(minutes.checked_mul(60)?)?;let last=series.iter().position(|b|b.time==last_time)?;
    if last<first||last-first+1!=minutes as usize{return None;}let window=&series[first..=last];if window.windows(2).any(|p|p[1].time!=p[0].time+60){return None;}Some(window)
}
fn outcome(reference:f64,atr:f64,window:&[Candle],minutes:u64)->Value{
    let last=window.last().unwrap();let final_atr=(last.close-reference)/atr;
    let mfe_atr=window.iter().map(|b|(b.high-reference)/atr).fold(f64::NEG_INFINITY,f64::max);let mae_atr=window.iter().map(|b|(b.low-reference)/atr).fold(f64::INFINITY,f64::min);
    let direction=if final_atr>0.20{1}else if final_atr< -0.20{-1}else{0};json!({"minutes":minutes,"future_bar_time":last.time,"future_close":last.close,"return_atr":final_atr,"mfe_atr":mfe_atr,"mae_atr":mae_atr,"direction_0_2atr":direction})
}
fn finite_number(v:&Value)->bool{v.as_f64().is_some_and(f64::is_finite)}
fn number_array(v:&Value,len:usize,positive:bool)->bool{v.as_array().is_some_and(|a|a.len()==len&&a.iter().all(|n|finite_number(n)&&(!positive||n.as_f64().unwrap()>0.0)))}
fn valid_sha256(value:&str)->bool{value.len()==64&&value.bytes().all(|b|b.is_ascii_hexdigit()&&(!b.is_ascii_alphabetic()||b.is_ascii_lowercase()))}
fn metric_ok(v:&Value)->bool{["accuracy","balanced_accuracy","macro_f1","log_loss","mean_confidence"].iter().all(|k|finite_number(&v[*k]))&&v["rows"].as_u64().is_some_and(|n|n>0)}
fn validate_artifact(a:&Value)->Result<(String,String,String)>{
    if a["schema"]!=MODEL_SCHEMA{bail!("Unsupported model schema");}
    if a["feature_schema"]!=FEATURE_SCHEMA||a["label_schema"]!=LABEL_SCHEMA{bail!("Model feature/label schema mismatch");}
    if a["status"]!="unreviewed_challenger"||a["eligible_for_promotion"]!=false{bail!("Only unreviewed non-promotable challenger artifacts may be registered");}
    let target=a["target"].as_str().unwrap_or("");if !matches!(target,"5m.direction_0_2atr"|"15m.direction_0_2atr"|"30m.direction_0_2atr"){bail!("Unsupported model target");}
    let sha=a["dataset"]["sha256"].as_str().unwrap_or("");if !valid_sha256(sha){bail!("Invalid dataset SHA-256 identity");}
    if a["dataset"]["used_rows"].as_u64().unwrap_or(0)<600{bail!("Registered baseline must contain at least 600 post-purge rows");}
    let splits=a["dataset"]["split"].as_array().filter(|v|!v.is_empty()).ok_or_else(||anyhow::anyhow!("Missing chronological split metadata"))?;
    if splits.iter().any(|s|s["purge_rows_each_side"].as_u64().unwrap_or(0)<30){bail!("Split purge must be at least 30 rows per side");}
    if splits.iter().any(|s|s["holdout_last_bar"].as_u64().is_none_or(|last|last<s["holdout_first_bar"].as_u64().unwrap_or(u64::MAX))){bail!("Model requires an exact holdout_last_bar boundary; re-export/retrain older artifacts");}
    if a["training"]["random_split"]!=false||a["training"]["purged_chronological_split"]!=true||a["training"]["automatic_promotion"]!=false{bail!("Training contract violates chronological/manual-promotion policy");}
    let classes=a["model"]["classes"].as_array().ok_or_else(||anyhow::anyhow!("Missing classes"))?;
    if classes.len()!=3||classes[0].as_i64()!=Some(-1)||classes[1].as_i64()!=Some(0)||classes[2].as_i64()!=Some(1){bail!("Model classes must be [-1,0,1]");}
    let names=a["model"]["feature_names"].as_array().ok_or_else(||anyhow::anyhow!("Missing feature names"))?;let d=names.len();
    if !(16..=256).contains(&d){bail!("Unexpected feature dimension");}let mut unique=HashSet::new();
    for n in names{let Some(s)=n.as_str()else{bail!("Feature names must be strings");};if s.len()>100||!unique.insert(s){bail!("Duplicate/invalid feature name");}}
    if !number_array(&a["model"]["means"],d,false)||!number_array(&a["model"]["stds"],d,true)||!number_array(&a["model"]["biases"],3,false){bail!("Invalid model vector dimensions or values");}
    let weights=a["model"]["weights"].as_array().filter(|w|w.len()==3).ok_or_else(||anyhow::anyhow!("Invalid weight matrix"))?;
    if weights.iter().any(|row|!number_array(row,d,false)){bail!("Invalid model weight matrix");}
    for split in ["train","validation","holdout"]{if !metric_ok(&a["metrics"][split]){bail!("Invalid {split} metrics");}}
    Ok((sha.into(),target.into(),a["metrics"].to_string()))
}

impl Service{
    pub fn new(db:&Connection)->Result<Self>{
        db.execute_batch("CREATE TABLE IF NOT EXISTS learning_snapshots(id INTEGER PRIMARY KEY,context TEXT NOT NULL,bar_time INTEGER NOT NULL,captured_ms INTEGER NOT NULL,feature_schema TEXT NOT NULL,reference_close REAL NOT NULL,atr REAL NOT NULL,features TEXT NOT NULL,UNIQUE(context,bar_time));
            CREATE TABLE IF NOT EXISTS learning_labels(snapshot_id INTEGER PRIMARY KEY,matured_ms INTEGER NOT NULL,label_schema TEXT NOT NULL,label TEXT NOT NULL,FOREIGN KEY(snapshot_id) REFERENCES learning_snapshots(id));
            CREATE TABLE IF NOT EXISTS learning_models(model_id TEXT PRIMARY KEY,created_ms INTEGER NOT NULL,family TEXT NOT NULL,feature_schema TEXT NOT NULL,dataset_identity TEXT NOT NULL,status TEXT NOT NULL,metrics TEXT NOT NULL,artifact_path TEXT NOT NULL,artifact_sha256 TEXT NOT NULL,note TEXT NOT NULL,artifact TEXT NOT NULL DEFAULT '{}');
            CREATE TABLE IF NOT EXISTS learning_slots(slot TEXT PRIMARY KEY CHECK(slot IN ('champion','challenger')),model_id TEXT,assigned_ms INTEGER NOT NULL,note TEXT NOT NULL,FOREIGN KEY(model_id) REFERENCES learning_models(model_id));")?;
        let has_artifact={
            let mut s=db.prepare("PRAGMA table_info(learning_models)")?;
            let columns=s.query_map([],|r|r.get::<_,String>(1))?.collect::<std::result::Result<Vec<_>,_>>()?;
            columns.iter().any(|n|n=="artifact")
        };
        if !has_artifact{db.execute("ALTER TABLE learning_models ADD COLUMN artifact TEXT NOT NULL DEFAULT '{}'",[])?;}
        Ok(Self::default())
    }
    pub fn observe(&mut self,db:&mut Connection,t:&Telemetry,a:&Analysis,now:u64){match self.observe_inner(db,t,a,now){Ok(())=>self.last_error.clear(),Err(e)=>self.last_error=e.to_string().chars().take(240).collect()}}
    fn observe_inner(&mut self,db:&mut Connection,t:&Telemetry,a:&Analysis,now:u64)->Result<()>{
        let Some((reference_close,atr,payload))=feature_payload(t,a)else{return Ok(());};let market=t.market.as_ref().unwrap();let key=context(t);let tx=db.transaction()?;
        tx.execute("INSERT OR IGNORE INTO learning_snapshots(context,bar_time,captured_ms,feature_schema,reference_close,atr,features) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![key,a.bar_time,now,FEATURE_SCHEMA,reference_close,atr,payload.to_string()])?;
        let cutoff=market.m1.last().map(|b|b.time.saturating_sub(1800)).unwrap_or(0);
        let pending={
            let mut s=tx.prepare("SELECT s.id,s.bar_time,s.reference_close,s.atr FROM learning_snapshots s LEFT JOIN learning_labels l ON l.snapshot_id=s.id WHERE s.context=?1 AND l.snapshot_id IS NULL AND s.bar_time<=?2 ORDER BY s.bar_time LIMIT 180")?;
            let rows=s.query_map(params![key,cutoff],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,u64>(1)?,r.get::<_,f64>(2)?,r.get::<_,f64>(3)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
            rows
        };
        for(id,bar_time,reference,snapshot_atr)in pending{if !snapshot_atr.is_finite()||snapshot_atr<=0.0{continue;}let Some(w5)=exact_window(&market.m1,bar_time,5)else{continue;};let Some(w15)=exact_window(&market.m1,bar_time,15)else{continue;};let Some(w30)=exact_window(&market.m1,bar_time,30)else{continue;};let label=json!({"schema":LABEL_SCHEMA,"reference_bar_time":bar_time,"reference_close":reference,"reference_atr":snapshot_atr,"horizons":[outcome(reference,snapshot_atr,w5,5),outcome(reference,snapshot_atr,w15,15),outcome(reference,snapshot_atr,w30,30)],"note":"Label written only after exact future M1 bars exist; missing/gapped horizons are not shifted or fabricated."});tx.execute("INSERT OR IGNORE INTO learning_labels(snapshot_id,matured_ms,label_schema,label) VALUES(?1,?2,?3,?4)",params![id,now,LABEL_SCHEMA,label.to_string()])?;}
        tx.commit()?;Ok(())
    }
    pub fn register(&self,db:&mut Connection,artifact:Value,now:u64)->Result<Value>{
        let(dataset,target,metrics)=validate_artifact(&artifact)?;let model_id=format!("linear-{now}-{}",uuid::Uuid::new_v4().simple());let serialized=serde_json::to_string(&artifact)?;let tx=db.transaction()?;
        tx.execute("INSERT INTO learning_models(model_id,created_ms,family,feature_schema,dataset_identity,status,metrics,artifact_path,artifact_sha256,note,artifact) VALUES(?1,?2,'linear-softmax-v1',?3,?4,'unreviewed_challenger',?5,'','','Stored immutable JSON artifact; inference disabled',?6)",params![model_id,now,FEATURE_SCHEMA,dataset,metrics,serialized])?;
        tx.execute("INSERT INTO learning_slots(slot,model_id,assigned_ms,note) VALUES('challenger',?1,?2,?3) ON CONFLICT(slot) DO UPDATE SET model_id=excluded.model_id,assigned_ms=excluded.assigned_ms,note=excluded.note",params![model_id,now,"Manual registration selected this artifact as the current challenger; no prediction or champion promotion enabled"])?;tx.commit()?;
        Ok(json!({"ok":true,"model_id":model_id,"slot":"challenger","target":target,"dataset_identity":dataset,"prediction_enabled":false,"eligible_for_promotion":false,"note":"Artifact registered immutably as challenger only."}))
    }
    pub fn status(&self,db:&Connection)->Result<Value>{
        let snapshots=db.query_row("SELECT COUNT(*) FROM learning_snapshots",[],|r|r.get::<_,i64>(0))?;let labels=db.query_row("SELECT COUNT(*) FROM learning_labels",[],|r|r.get::<_,i64>(0))?;let models=db.query_row("SELECT COUNT(*) FROM learning_models",[],|r|r.get::<_,i64>(0))?;
        let latest:Option<(u64,u64,String)>=db.query_row("SELECT bar_time,captured_ms,feature_schema FROM learning_snapshots ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let champion:Option<String>=db.query_row("SELECT model_id FROM learning_slots WHERE slot='champion'",[],|r|r.get::<_,Option<String>>(0)).optional()?.flatten();let challenger:Option<String>=db.query_row("SELECT model_id FROM learning_slots WHERE slot='challenger'",[],|r|r.get::<_,Option<String>>(0)).optional()?.flatten();
        let recent={
            let mut s=db.prepare("SELECT model_id,created_ms,family,dataset_identity,status,metrics FROM learning_models ORDER BY created_ms DESC LIMIT 10")?;
            let rows=s.query_map([],|r|{let metrics:String=r.get(5)?;Ok(json!({"model_id":r.get::<_,String>(0)?,"created_ms":r.get::<_,u64>(1)?,"family":r.get::<_,String>(2)?,"dataset_identity":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"metrics":serde_json::from_str::<Value>(&metrics).unwrap_or(Value::Null)}))})?.collect::<std::result::Result<Vec<_>,_>>()?;
            rows
        };
        Ok(json!({"version":"learning-foundation-v1","feature_schema":FEATURE_SCHEMA,"label_schema":LABEL_SCHEMA,"snapshots":snapshots,"matured_labels":labels,"pending_labels":snapshots-labels,"models":models,"champion":champion,"challenger":challenger,"recent_models":recent,"latest":latest.map(|(bar_time,captured_ms,schema)|json!({"bar_time":bar_time,"captured_ms":captured_ms,"schema":schema})),"last_error":self.last_error,"training_enabled":false,"prediction_enabled":false,"champion_promotion_enabled":false,"live_self_modification":false,"note":"Registry accepts validated unreviewed challengers only. No model can influence Shadow."}))
    }
    pub fn export(&self,db:&Connection,dir:&Path,now:u64)->Result<Value>{
        let name=format!("learning-dataset-{now}");let staging=dir.join("exports").join(format!(".{name}.tmp"));let destination=dir.join("exports").join(&name);if staging.exists()||destination.exists(){bail!("Learning export path already exists");}fs::create_dir_all(&staging)?;let mut output=fs::File::create(staging.join("dataset.jsonl"))?;
        let mut statement=db.prepare("SELECT s.id,s.context,s.bar_time,s.captured_ms,s.feature_schema,s.features,l.matured_ms,l.label_schema,l.label FROM learning_snapshots s JOIN learning_labels l ON l.snapshot_id=s.id ORDER BY s.context,s.bar_time,s.id")?;let mut rows=statement.query([])?;let mut count=0usize;
        while let Some(r)=rows.next()?{let features:Value=serde_json::from_str(&r.get::<_,String>(5)?)?;let label:Value=serde_json::from_str(&r.get::<_,String>(8)?)?;writeln!(output,"{}",json!({"snapshot_id":r.get::<_,i64>(0)?,"context":r.get::<_,String>(1)?,"bar_time":r.get::<_,u64>(2)?,"captured_ms":r.get::<_,u64>(3)?,"feature_schema":r.get::<_,String>(4)?,"features":features,"matured_ms":r.get::<_,u64>(6)?,"label_schema":r.get::<_,String>(7)?,"label":label}))?;count+=1;}output.sync_all()?;
        fs::write(staging.join("manifest.json"),serde_json::to_vec_pretty(&json!({"version":"learning-foundation-v1","feature_schema":FEATURE_SCHEMA,"label_schema":LABEL_SCHEMA,"rows":count,"created_ms":now,"ordering":"context,bar_time,snapshot_id","scope":"matured labels only; no quote/DOM/news features in v1","trained_model_included":false}))?)?;drop(rows);drop(statement);fs::rename(staging,&destination)?;Ok(json!({"path":destination,"rows":count,"note":"Deterministic matured learning dataset export; training is offline only."}))
    }
}
