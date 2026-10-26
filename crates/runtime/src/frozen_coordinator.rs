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
const POLICY_MULTI:&str="frozen-scalping-swing-coordinator-decision-v1";
// Sniper's price-triggered Fast Pulse is NOT an M1-close coordinator;
// its frozen research requires a separate tick-specific experiment.
fn supported(mode:TradingMode)->bool{
    matches!(mode,TradingMode::Expert|TradingMode::Scalping|TradingMode::Swing)
}
fn mode_key(mode:TradingMode)->&'static str{
    match mode{
        TradingMode::Expert=>"expert",TradingMode::Scalping=>"scalping",
        TradingMode::Swing=>"swing",TradingMode::Sniper=>"sniper"
    }
}
fn policy(mode:TradingMode)->&'static str{
    if mode==TradingMode::Expert{POLICY}else{POLICY_MULTI}
}
fn context(t:&Telemetry)->String{
    crate::coordinator_learning::context(t)
}

/// Immutable research-only policy snapshot, also reused for opt-in sealed
/// candidate replay. The live Coordinator continues learning separately.
pub(crate) fn checkpoint(db:&Connection,t:&Telemetry,c:&Config,brains:&[BrainConfig],
    weights:&BTreeMap<String,LearnedWeight>,
    controls:&BTreeMap<String,crate::brain_control::Control>,
    policies:&BTreeMap<String,ExitStudy>,now:u64)->Result<Value>{
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
    Ok(json!({"policy":policy(c.trading.mode),"mode":mode_key(c.trading.mode),"config":c,
        "members":all,"exit_studies":policies,
        "created_ms":now,"weights_frozen":true,"broker_permission":false,
        "included_layers":["Expert system base","frozen advisor signals",
           "frozen learned weights","frozen contextual Demo selection",
           "frozen adaptive exits"]}))
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
    // Keep direct frozen_coordinator init, test fixtures and full Service init in sync.
    crate::g7_capture::init(db)?;
    crate::g7_sealed_candidate::init(db)?;
    crate::coordinator_paper::init(db)?;
    Ok(())
}
pub fn freeze(db:&Connection,t:&Telemetry,c:&Config,brains:&[BrainConfig],
    weights:&BTreeMap<String,LearnedWeight>,
    controls:&BTreeMap<String,crate::brain_control::Control>,
    policies:&BTreeMap<String,ExitStudy>,now:u64)->Result<Value>{
    ensure!(t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&t.login>0 && !t.server.is_empty() && !t.currency.is_empty()
        &&supported(c.trading.mode)&&t.symbol==c.symbol,
        "تجميد منسق Expert / Scalping / Swing للتدقيق مسموح على Demo فقط؛ Sniper يحتاج رصد Tick مستقل");
    ensure!(t.tick_age_ms.is_some_and(|ms|ms<=3000),
        "تحتاج قراءة أسعار Demo حديثة قبل التجميد");
    ensure!(brains.iter().any(|b|b.id=="system"),
        "منسق SYSTEM غير متاح");
    c.validate().map_err(anyhow::Error::msg)?;
    let key=context(t);
    let active:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_coordinator_runs
         WHERE context=?1 AND mode=?2 AND active=1)",
        params![key,mode_key(c.trading.mode)],|r|r.get(0))?;
    ensure!(!active,"توجد نسخة منسق مجمدة نشطة لهذا الحساب والرمز");
    let id=uuid::Uuid::new_v4().simple().to_string();
    let snapshot=checkpoint(db,t,c,brains,weights,controls,policies,now)?;
    let tx=db.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO frozen_coordinator_runs VALUES(?1,?2,?3,?4,?5,1)",
        params![id,key,mode_key(c.trading.mode),now,snapshot.to_string()])?;
    // Register the research cohort BEFORE any new completed-bar evidence.
    crate::g7_capture::preregister(&tx,&id,&key,mode_key(c.trading.mode),
        &snapshot.to_string(),now)?;
    tx.commit()?;
    Ok(json!({"ok":true,"run_id":id,"active":true,
        "no_broker_orders":true,"no_learning_effect":true,
        "mode":mode_key(c.trading.mode),"policy":policy(c.trading.mode)}))
}
pub fn stop(db:&Connection,key:&str,mode:TradingMode)->Result<Value>{
    ensure!(supported(mode),"Sniper frozen coordinator requires a distinct tick study");
    let count=db.execute(
        "UPDATE frozen_coordinator_runs SET active=0
         WHERE context=?1 AND mode=?2 AND active=1",
        params![key,mode_key(mode)])?;
    Ok(json!({"ok":true,"stopped":count,
        "mode":mode_key(mode),"policy":policy(mode)}))
}
/// Execute only a captured policy against future bars; no current learner
/// weights, Student policies, trading commands, broker results or mutation.
pub(crate) fn evaluate_checkpoint(raw:&str,t:&Telemetry,c:&Config,
    frames:&BTreeMap<String,Vec<FrameBar>>,now:u64)->Result<Signal>{
    let snap:Value=serde_json::from_str(raw)?;
    ensure!(snap["policy"]==policy(c.trading.mode),
        "frozen coordinator policy mismatch");
    let cfg:Config=serde_json::from_value(snap["config"].clone())?;
    // Existing Expert V24/P1-I snapshots did not persist the redundant
    // mode field; their immutable configuration already binds the mode.
    let snapshot_mode_matches=snap["mode"]==mode_key(c.trading.mode)
        ||(c.trading.mode==TradingMode::Expert && snap["mode"].is_null());
    ensure!(cfg.symbol==t.symbol&&cfg.trading.mode==c.trading.mode
        &&snapshot_mode_matches,"frozen policy context drift");
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
    Ok(frozen)
}
pub fn observe(db:&Connection,t:&Telemetry,c:&Config,adaptive:&Signal,
    frames:&BTreeMap<String,Vec<FrameBar>>,now:u64)->Result<()>{
    if !supported(c.trading.mode)||!t.demo||t.account_trade_mode!=Some(0)
        ||!t.connected||t.symbol!=c.symbol
        ||t.tick_age_ms.is_none_or(|ms|ms>3000){return Ok(())}
    let key=context(t);
    let found:Option<(String,u64,String)>=db.query_row(
        "SELECT id,created_ms,snapshot FROM frozen_coordinator_runs
         WHERE context=?1 AND mode=?2 AND active=1
         ORDER BY created_ms DESC LIMIT 1",
        params![key,mode_key(c.trading.mode)],
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
    let frozen=evaluate_checkpoint(&raw,t,c,frames,now)?;
    // Record BEFORE any future simulated trade outcome. This table is neither
    // the broker-command ledger nor the adaptive learner's scored labels.
    let tx=db.unchecked_transaction()?;
    let inserted=tx.execute(
        "INSERT OR IGNORE INTO frozen_coordinator_decisions
         (run_id,bar_time,observed_ms,frozen,adaptive)
         VALUES(?1,?2,?3,?4,?5)",
        params![id,bar,now,
            serde_json::to_string(&frozen)?,
            serde_json::to_string(adaptive)?])?;
    if inserted==1 && crate::g7_capture::has_registration(&tx,&id)? {
        // Pre-G7 frozen studies remain LEGACY, not retroactively preregistered.
        // Registered G7 receipts are atomic with the paired decision.
        crate::g7_capture::record_pair(&tx,&id,&key,mode_key(c.trading.mode),
            &raw,bar,now,t,&frozen,adaptive)?;
    }
    tx.commit()?;
    // An optional sealed Candidate is evaluated on the SAME future M1 bar.
    // A failed research replay cannot roll back accepted G7-A pair evidence or
    // interfere with already handled broker positions.
    crate::g7_sealed_candidate::observe(db,&id,&key,mode_key(c.trading.mode),
        t,c,frames,bar,now)?;
    Ok(())
}
pub fn report(db:&Connection,key:&str,mode:TradingMode)->Result<Value>{
    ensure!(supported(mode),"Sniper frozen coordinator is NOT available from completed M1 observations");
    if key.is_empty(){return Ok(json!({"status":"unbound","policy":policy(mode)}))}
    let item:Option<(String,u64,bool)>=db.query_row(
        "SELECT id,created_ms,active FROM frozen_coordinator_runs
         WHERE context=?1 AND mode=?2 ORDER BY created_ms DESC,id DESC LIMIT 1",
        params![key,mode_key(mode)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((id,created,active))=item else{
        return Ok(json!({"status":"not_started","mode":mode_key(mode),
            "context":key,"policy":policy(mode),"no_broker_orders":true}))
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
    // The Expert paper simulator is NOT valid for Scalping/Swing.
    // Their forward results are prospective *decision parity only*.
    let paired=if mode==TradingMode::Expert{
        Some(crate::coordinator_paper::summary(db,&id)?)
    }else{None};
    Ok(json!({"status":if active{"active"}else{"stopped"},
        "mode":mode_key(mode),"context":key,"run_id":id,
        "frozen_ms":created,"policy":policy(mode),
        "bars_compared":total,"both_actionable":both,
        "paired_quote_paper":paired,
        "same_direction":same,"different_direction":different,
        "frozen_only_ready":frozen_only,"adaptive_only_ready":adaptive_only,
        "frozen_weights_immutable":true,"no_broker_orders":true,
        "no_training_updates":true,
        "evaluation_scope":"Selected mode coordinator base, frozen advisor configs/genomes, contextual weights and exit-study; compare raw decision before broker gates",
        "not_included":"Broker margin/spread/order checks, current open-position state, order fills, broker reconciliation, and Sniper tick Fast Pulse. Paired quote-paper outcomes are Expert-only.",
        "outcome":"Prospective M1-completed decision parity only for this exact mode; Scalping/Swing have no paper-PnL twin. Does not qualify REAL trading",
        "independent_oos":false,"mode_isolated":true,"sniper_tick_coverage":false
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
        assert_eq!(report(&db,&context(&t),TradingMode::Expert).unwrap()["status"],"active");
        assert!(freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123457).is_err());
        let raw:String=db.query_row("SELECT snapshot FROM frozen_coordinator_runs WHERE id=?1",
            [id],|r|r.get(0)).unwrap();
        let saved:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(saved["policy"],POLICY);
        assert_eq!(saved["config"]["symbol"],"XAUUSD");
        let mut other=t.clone();other.login=456;
        assert_eq!(report(&db,&context(&other),TradingMode::Expert).unwrap()["status"],"not_started");
        stop(&db,&context(&t),TradingMode::Expert).unwrap();
        assert_eq!(report(&db,&context(&t),TradingMode::Expert).unwrap()["status"],"stopped");
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
        let status=report(&db,&context(&t),TradingMode::Expert).unwrap();
        assert_eq!(status["bars_compared"],1);
        assert_eq!(status["adaptive_only_ready"],1);
        assert_eq!(status["no_broker_orders"],true);
        // The actual future completed-bar path writes exactly one G7 receipt,
        // sharing the transaction with the original Frozen/Adaptive pair.
        let receipt:String=db.query_row(
            "SELECT payload FROM g7_paired_decision_evidence",[],|r|r.get(0)).unwrap();
        let proof:Value=serde_json::from_str(&receipt).unwrap();
        assert_eq!(proof["protocol"],"g7-local-paired-decisions-v1");
        assert_eq!(proof["coverage"],"local_quote_observed");
        assert_eq!(proof["execution"],"decision_only_neither_arm_broker_executed");
        assert!(proof["booked_pnl"].is_null());
        let n:i64=db.query_row("SELECT COUNT(*) FROM g7_paired_decision_evidence",[],|r|r.get(0)).unwrap();
        assert_eq!(n,1);
        // Historical replay too late after close cannot create fabricated new rows.
        t.quote_time_ms=Some(999_000);
        observe(&db,&t,&c,&adaptive,&sample(240),999_000).unwrap();
        assert_eq!(report(&db,&context(&t),TradingMode::Expert).unwrap()["bars_compared"],1);
    }
    #[test]
    fn two_non_sniper_modes_have_independent_future_decisions_and_stop_scope(){
        use braintrade_domain::Candle;
        let db=fixture();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:321,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),tick_age_ms:Some(0),..Default::default()};
        let brains=vec![BrainConfig{id:"system".into(),..Default::default()}];
        let mut scalp=Config::default();scalp.trading.mode=TradingMode::Scalping;
        let mut swing=scalp.clone();swing.trading.mode=TradingMode::Swing;
        let sr=freeze(&db,&t,&scalp,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        let wr=freeze(&db,&t,&swing,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        assert_ne!(sr["run_id"],wr["run_id"]);
        assert_eq!(sr["mode"],"scalping");
        assert_eq!(wr["mode"],"swing");
        assert_eq!(report(&db,&context(&t),TradingMode::Scalping).unwrap()["status"],"active");
        assert_eq!(report(&db,&context(&t),TradingMode::Swing).unwrap()["status"],"active");
        let frames=BTreeMap::from([("M1".into(),vec![FrameBar{
            candle:Candle{time:180,open:100.,high:101.,low:99.,close:100.5,
                tick_volume:200},spread_points:20,real_volume:0
        }])]);
        let actual=Signal{brain:"system".into(),side:1,key:"next:180".into(),
            bar_time:180,stop_distance:1.,target_distance:2.,hold_ms:60_000,
            reason:"synthetic actual signal".into(),ready:true,coordination:None};
        // At 241s bar 180 is completed. Only the selected mode records.
        observe(&db,&t,&scalp,&actual,&frames,241_000).unwrap();
        assert_eq!(report(&db,&context(&t),TradingMode::Scalping).unwrap()["bars_compared"],1);
        assert_eq!(report(&db,&context(&t),TradingMode::Swing).unwrap()["bars_compared"],0);
        // Replay same M1 is idempotent.
        observe(&db,&t,&scalp,&actual,&frames,242_000).unwrap();
        assert_eq!(report(&db,&context(&t),TradingMode::Scalping).unwrap()["bars_compared"],1);
        observe(&db,&t,&swing,&actual,&frames,241_000).unwrap();
        let w=report(&db,&context(&t),TradingMode::Swing).unwrap();
        assert_eq!(w["bars_compared"],1);
        assert_eq!(w["paired_quote_paper"],Value::Null);
        assert_eq!(w["broker_permission"],Value::Null);
        assert_eq!(w["no_broker_orders"],true);
        assert_eq!(w["sniper_tick_coverage"],false);
        assert_eq!(w["independent_oos"],false);
        // Stopping scalp does not stop swing or rewrite either snapshot.
        stop(&db,&context(&t),TradingMode::Scalping).unwrap();
        assert_eq!(report(&db,&context(&t),TradingMode::Scalping).unwrap()["status"],"stopped");
        assert_eq!(report(&db,&context(&t),TradingMode::Swing).unwrap()["status"],"active");
        // Sniper must NOT use an M1-close trial or return made-up parity.
        assert!(report(&db,&context(&t),TradingMode::Sniper).is_err());
        let mut sniper=scalp.clone();sniper.trading.mode=TradingMode::Sniper;
        assert!(freeze(&db,&t,&sniper,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),243_000).is_err());
    }
    #[test]
    fn different_demo_account_has_no_forward_trial_or_inherited_counts(){
        let db=fixture();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            tick_age_ms:Some(0),server:"Demo".into(),login:17,
            symbol:"XAUUSD".into(),currency:"USD".into(),..Default::default()};
        let mut c=Config::default();c.trading.mode=TradingMode::Swing;
        let brains=vec![BrainConfig{id:"system".into(),..Default::default()}];
        freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        let mut different=t.clone();different.login=18;
        assert_eq!(report(&db,&context(&different),TradingMode::Swing).unwrap()["status"],"not_started");
        different=t.clone();different.symbol="EURUSD".into();
        assert_eq!(report(&db,&context(&different),TradingMode::Swing).unwrap()["status"],"not_started");
    }
    #[test]
    fn legacy_expert_snapshots_without_mode_still_evaluate_future_bars(){
        use braintrade_domain::Candle;
        let db=fixture();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:17,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),tick_age_ms:Some(0),..Default::default()};
        let c=Config::default();
        let brains=vec![BrainConfig{id:"system".into(),..Default::default()}];
        let trial=freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        let id=trial["run_id"].as_str().unwrap();
        // Simulate a genuine pre-G7 frozen run: old studies had no G7
        // registration. Never silently re-sign an edited G7 checkpoint.
        db.execute("DELETE FROM g7_pair_registrations WHERE run_id=?1",[id]).unwrap();
        let raw:String=db.query_row(
            "SELECT snapshot FROM frozen_coordinator_runs WHERE id=?1",
            [id],|r|r.get(0)).unwrap();
        let mut old:Value=serde_json::from_str(&raw).unwrap();
        old.as_object_mut().unwrap().remove("mode");
        assert_eq!(old["policy"],POLICY);
        db.execute("UPDATE frozen_coordinator_runs SET snapshot=?2 WHERE id=?1",
            params![id,old.to_string()]).unwrap();
        let frames=BTreeMap::from([("M1".into(),vec![FrameBar{
            candle:Candle{time:180,open:100.,high:101.,low:99.,
                close:100.5,tick_volume:100},spread_points:10,real_volume:0
        }])]);
        let signal=Signal{brain:"system".into(),side:1,key:"legacy:180".into(),
            bar_time:180,stop_distance:1.,target_distance:2.,
            hold_ms:60000,reason:"legacy report".into(),ready:true,coordination:None};
        observe(&db,&t,&c,&signal,&frames,241_000).unwrap();
        let r=report(&db,&context(&t),TradingMode::Expert).unwrap();
        assert_eq!(r["bars_compared"],1);
        let receipts:i64=db.query_row("SELECT COUNT(*) FROM g7_paired_decision_evidence",[],|r|r.get(0)).unwrap();
        assert_eq!(receipts,0,"Legacy pairs must not receive invented G7 provenance");
        assert_eq!(r["mode"],"expert");
        assert_eq!(r["no_broker_orders"],true);
    }
    #[test]
    fn real_or_sniper_cannot_start_m1_frozen_policy_trial(){
        let db=fixture();
        let mut t=Telemetry{connected:true,demo:false,account_trade_mode:Some(2),
            login:17,server:"Demo".into(),currency:"USD".into(),
            tick_age_ms:Some(0),symbol:"XAUUSD".into(),..Default::default()};
        let mut c=Config::default();
        let b=vec![BrainConfig{id:"system".into(),..Default::default()}];
        assert!(freeze(&db,&t,&c,&b,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),1).is_err());
        t.demo=true;t.account_trade_mode=Some(0);
        c.trading.mode=TradingMode::Sniper;
        assert!(freeze(&db,&t,&c,&b,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),1).is_err());
    }
}