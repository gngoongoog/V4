//! Immutable, read-only Expert Demo coordinator *decision-policy* study.
//! Evaluates the frozen coordinator+advisor weights on future completed bars.
//! This is NOT a second MT5 broker execution engine or a full risk-gate twin.
use anyhow::{ensure,Result};
use braintrade_domain::{execution::{self,BrainConfig,LearnedWeight,Signal},
    studio::{FrameBar,Genome},student::ExitStudy,trading::TradingMode,
    Config,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use std::collections::BTreeMap;

const POLICY:&str="frozen-expert-coordinator-decision-v1";
fn mode_key()->&'static str{"expert"}
fn context(t:&Telemetry)->String{
    crate::coordinator_learning::context(t)
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch("CREATE TABLE IF NOT EXISTS frozen_coordinator_runs(
        id TEXT PRIMARY KEY,context TEXT NOT NULL,mode TEXT NOT NULL,
        created_ms INTEGER NOT NULL,snapshot TEXT NOT NULL,
        active INTEGER NOT NULL CHECK(active IN (0,1)));
       CREATE INDEX IF NOT EXISTS frozen_coordinator_run_scope
          ON frozen_coordinator_runs(context,mode,created_ms DESC);
       CREATE TABLE IF NOT EXISTS frozen_coordinator_decisions(
          run_id TEXT NOT NULL,bar_time INTEGER NOT NULL,observed_ms INTEGER NOT NULL,
          frozen TEXT NOT NULL,adaptive TEXT NOT NULL,
          PRIMARY KEY(run_id,bar_time),
          FOREIGN KEY(run_id) REFERENCES frozen_coordinator_runs(id));")?;
    Ok(())
}
pub fn freeze(db:&Connection,t:&Telemetry,c:&Config,brains:&[BrainConfig],
    weights:&BTreeMap<String,LearnedWeight>,
    controls:&BTreeMap<String,crate::brain_control::Control>,
    policies:&BTreeMap<String,ExitStudy>,now:u64)->Result<Value>{
    ensure!(t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&c.trading.mode==TradingMode::Expert&&t.symbol==c.symbol,
        "تجميد منسق Expert للتدقيق مسموح على Demo فقط");
    ensure!(t.tick_age_ms.is_some_and(|ms|ms<=3000),
        "تحتاج قراءة أسعار Demo حديثة قبل التجميد");
    ensure!(brains.iter().any(|b|b.id=="system"),
        "منسق SYSTEM غير متاح");
    c.validate().map_err(anyhow::Error::msg)?;
    let key=context(t);
    let active:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_coordinator_runs
         WHERE context=?1 AND mode=?2 AND active=1)",
        params![key,mode_key()],|r|r.get(0))?;
    ensure!(!active,"توجد نسخة منسق مجمدة نشطة لهذا الحساب والرمز");
    let mut all=Vec::new();
    for b in brains {
        b.validate().map_err(anyhow::Error::msg)?;
        let g=if b.id=="system" {None} else {
            crate::coordinator_learning::genome(db,b,t)?
        };
        all.push(json!({"brain":b,"genome":g,"weight":weights.get(&b.id),
            "advisor_enabled":controls.get(&b.id).is_none_or(|x|x.advisor)
               && !b.enabled && b.id!="system"}));
    }
    let id=uuid::Uuid::new_v4().simple().to_string();
    let snapshot=json!({"policy":POLICY,"config":c,
        "members":all,"exit_studies":policies,
        "created_ms":now,"weights_frozen":true,"broker_permission":false,
        "included_layers":["Expert system base","frozen advisor signals",
           "frozen learned weights","frozen contextual Demo selection",
           "frozen adaptive exits"]});
    db.execute(
        "INSERT INTO frozen_coordinator_runs VALUES(?1,?2,?3,?4,?5,1)",
        params![id,key,mode_key(),now,snapshot.to_string()])?;
    Ok(json!({"ok":true,"run_id":id,"active":true,
        "no_broker_orders":true,"no_learning_effect":true,"policy":POLICY}))
}
pub fn stop(db:&Connection,key:&str)->Result<Value>{
    let count=db.execute(
        "UPDATE frozen_coordinator_runs SET active=0
         WHERE context=?1 AND mode=?2 AND active=1",
        params![key,mode_key()])?;
    Ok(json!({"ok":true,"stopped":count,"policy":POLICY}))
}
pub fn observe(db:&Connection,t:&Telemetry,c:&Config,adaptive:&Signal,
    frames:&BTreeMap<String,Vec<FrameBar>>,now:u64)->Result<()>{
    if c.trading.mode!=TradingMode::Expert||!t.demo||t.account_trade_mode!=Some(0)
        ||!t.connected||t.symbol!=c.symbol
        ||t.tick_age_ms.is_none_or(|ms|ms>3000){return Ok(())}
    let key=context(t);
    let found:Option<(String,u64,String)>=db.query_row(
        "SELECT id,created_ms,snapshot FROM frozen_coordinator_runs
         WHERE context=?1 AND mode=?2 AND active=1
         ORDER BY created_ms DESC LIMIT 1",
        params![key,mode_key()],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((id,created,raw))=found else{return Ok(())};
    let Some(last)=frames.get("M1").and_then(|v|v.last()) else{return Ok(())};
    let bar=last.candle.time;
    // Exactly one future COMPLETED one-minute bar, no historical backfill.
    let Some(closed)=bar.checked_add(60)else{return Ok(())};
    if bar.saturating_mul(1000)<=created || closed>now/1000
        ||(now/1000).saturating_sub(closed)>80{return Ok(())}
    let recorded:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_coordinator_decisions
         WHERE run_id=?1 AND bar_time=?2)",params![id,bar],|r|r.get(0))?;
    if recorded{return Ok(())}
    let snap:Value=serde_json::from_str(&raw)?;
    ensure!(snap["policy"]==POLICY,"frozen coordinator policy mismatch");
    let cfg:Config=serde_json::from_value(snap["config"].clone())?;
    ensure!(cfg.symbol==t.symbol&&cfg.trading.mode==TradingMode::Expert,
        "frozen policy context drift");
    let mut primary=None;
    let mut votes=Vec::new();
    for m in snap["members"].as_array().into_iter().flatten(){
        let b:BrainConfig=serde_json::from_value(m["brain"].clone())?;
        let genome:Option<Genome>=serde_json::from_value(m["genome"].clone())?;
        let weight:LearnedWeight=serde_json::from_value(
            m["weight"].as_object().map(|_|m["weight"].clone())
                .unwrap_or_else(||json!({})))?;
        if b.id=="system" {
            primary=Some((b.clone(),execution::evaluate(&b,t,&cfg,frames,None,now)));
        }else if m["advisor_enabled"]==true{
            let mut b=b;
            // Coordinator voting excludes SOLO. Frozen eligibility was captured
            // at freeze, not rebuilt from mutable per-brain control switches.
            b.enabled=false;
            let signal=execution::evaluate(&b,t,&cfg,frames,genome.as_ref(),now);
            votes.push((b,signal,weight));
        }
    }
    let Some((sys,base))=primary else{anyhow::bail!("no frozen coordinator")};
    let mut frozen=execution::coordinate_for_account(base,&cfg,t,votes);
    let studies:BTreeMap<String,ExitStudy>=serde_json::from_value(snap["exit_studies"].clone())?;
    crate::student_learning::adapt(&mut frozen,t,&cfg,&sys,&studies);
    // Record BEFORE any future simulated trade outcome. This table is neither
    // the broker-command ledger nor the adaptive learner's scored labels.
    db.execute(
        "INSERT OR IGNORE INTO frozen_coordinator_decisions
         (run_id,bar_time,observed_ms,frozen,adaptive)
         VALUES(?1,?2,?3,?4,?5)",
        params![id,bar,now,
            serde_json::to_string(&frozen)?,
            serde_json::to_string(adaptive)?])?;
    Ok(())
}
pub fn report(db:&Connection,key:&str)->Result<Value>{
    if key.is_empty(){return Ok(json!({"status":"unbound","policy":POLICY}))}
    let item:Option<(String,u64,bool)>=db.query_row(
        "SELECT id,created_ms,active FROM frozen_coordinator_runs
         WHERE context=?1 AND mode=?2 ORDER BY created_ms DESC,id DESC LIMIT 1",
        params![key,mode_key()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((id,created,active))=item else{
        return Ok(json!({"status":"not_started","policy":POLICY}))
    };
    let mut stmt=db.prepare(
        "SELECT frozen,adaptive FROM frozen_coordinator_decisions
         WHERE run_id=?1 ORDER BY bar_time")?;
    let rows=stmt.query_map([&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut total,mut both,mut same,mut different,mut frozen_only,mut adaptive_only)
       =(0u64,0u64,0u64,0u64,0u64,0u64);
    for (f,a) in rows{
        let f:Signal=serde_json::from_str(&f)?;
        let a:Signal=serde_json::from_str(&a)?;
        total+=1;
        if f.ready&&a.ready{
            both+=1;
            if f.side==a.side{same+=1}else{different+=1}
        }else if f.ready{frozen_only+=1}else if a.ready{adaptive_only+=1}
    }
    let paired=crate::coordinator_paper::summary(db,&id)?;
    Ok(json!({"status":if active{"active"}else{"stopped"},
        "run_id":id,"frozen_ms":created,"policy":POLICY,
        "bars_compared":total,"both_actionable":both,
        "paired_quote_paper":paired,
        "same_direction":same,"different_direction":different,
        "frozen_only_ready":frozen_only,"adaptive_only_ready":adaptive_only,
        "frozen_weights_immutable":true,"no_broker_orders":true,
        "no_training_updates":true,
        "evaluation_scope":"Expert coordinator base, frozen advisor configs/genomes, contextual weights and exit-study; compare raw decision before broker gates",
        "not_included":"Broker margin/spread/order checks, current open-position state, system execution timing, broker reconciliations and live Sniper/Scalping/Swing",
        "outcome":"Decision parity only; does not measure trade profitability or qualify REAL trading"
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->Connection{
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db
    }
    #[test]
    fn frozen_snapshot_persists_and_is_account_scoped(){
        let db=fixture();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            tick_age_ms:Some(0),server:"Demo".into(),login:123,
            symbol:"XAUUSD".into(),currency:"USD".into(),..Default::default()};
        let c=Config::default();
        // System-only fixture needs no genome/source database tables.
        let brains=vec![BrainConfig{id:"system".into(),..Default::default()}];
        let started=freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123456).unwrap();
        let id=started["run_id"].as_str().unwrap();
        assert_eq!(report(&db,&context(&t)).unwrap()["status"],"active");
        assert!(freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123457).is_err());
        let raw:String=db.query_row("SELECT snapshot FROM frozen_coordinator_runs WHERE id=?1",
            [id],|r|r.get(0)).unwrap();
        let saved:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(saved["policy"],POLICY);
        assert_eq!(saved["config"]["symbol"],"XAUUSD");
        let mut other=t.clone();other.login=456;
        assert_eq!(report(&db,&context(&other)).unwrap()["status"],"not_started");
        stop(&db,&context(&t)).unwrap();
        assert_eq!(report(&db,&context(&t)).unwrap()["status"],"stopped");
        let frozen:String=db.query_row("SELECT snapshot FROM frozen_coordinator_runs WHERE id=?1",
            [id],|r|r.get(0)).unwrap();
        assert_eq!(raw,frozen);
        let next=freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123458).unwrap();
        assert_ne!(next["run_id"],started["run_id"]);
    }
    #[test]
    fn observations_are_future_only_and_one_per_completed_bar(){
        use braintrade_domain::Candle;
        let db=fixture();
        let mut t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            tick_age_ms:Some(0),server:"Demo".into(),login:123,
            symbol:"XAUUSD".into(),currency:"USD".into(),
            bid:Some(100.),ask:Some(100.1),..Default::default()};
        let c=Config::default();
        let brains=vec![BrainConfig{id:"system".into(),..Default::default()}];
        freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        let sample=|bar|BTreeMap::from([("M1".into(),vec![FrameBar{
            candle:Candle{time:bar,open:100.,high:101.,low:99.,close:100.5,tick_volume:200},
            spread_points:20,real_volume:0
        }])]);
        let adaptive=Signal{brain:"system".into(),side:1,key:"bar:180".into(),
            bar_time:180,stop_distance:1.,target_distance:2.,hold_ms:60000,
            reason:"snapshot".into(),ready:true,coordination:None};
        // Bar 90 opened before freeze, even if it CLOSED later: not eligible.
        t.quote_time_ms=Some(151_000);
        observe(&db,&t,&c,&adaptive,&sample(90),151_000).unwrap();
        let count:usize=db.query_row("SELECT COUNT(*) FROM frozen_coordinator_decisions",
            [],|r|r.get(0)).unwrap();
        assert_eq!(count,0);
        // Bar 180 started after freeze and has completed at 240 seconds.
        t.quote_time_ms=Some(241_000);
        observe(&db,&t,&c,&adaptive,&sample(180),241_000).unwrap();
        observe(&db,&t,&c,&adaptive,&sample(180),242_000).unwrap();
        let status=report(&db,&context(&t)).unwrap();
        assert_eq!(status["bars_compared"],1);
        assert_eq!(status["adaptive_only_ready"],1);
        assert_eq!(status["no_broker_orders"],true);
        // Historical replay too late after close cannot create fabricated new rows.
        t.quote_time_ms=Some(999_000);
        observe(&db,&t,&c,&adaptive,&sample(240),999_000).unwrap();
        assert_eq!(report(&db,&context(&t)).unwrap()["bars_compared"],1);
    }
    #[test]
    fn real_or_non_expert_cannot_start_policy_trial(){
        let db=fixture();
        let mut t=Telemetry{connected:true,demo:false,account_trade_mode:Some(2),
            tick_age_ms:Some(0),symbol:"XAUUSD".into(),..Default::default()};
        let mut c=Config::default();
        let b=vec![BrainConfig{id:"system".into(),..Default::default()}];
        assert!(freeze(&db,&t,&c,&b,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),1).is_err());
        t.demo=true;t.account_trade_mode=Some(0);
        c.trading.mode=TradingMode::Scalping;
        assert!(freeze(&db,&t,&c,&b,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),1).is_err());
    }
}