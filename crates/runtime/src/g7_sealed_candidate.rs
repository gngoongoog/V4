//! G7-F: explicitly sealed policy candidate, evaluated as research-only M1
//! decisions. Adaptive broker policy and every learning path remain separate.
//! No sealed candidate broker execution, booked PnL or independent OOS.
use anyhow::{ensure,Result};
use braintrade_domain::{execution::{BrainConfig,LearnedWeight},student::ExitStudy,
    trading::TradingMode,Config,Telemetry,studio::FrameBar};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::collections::BTreeMap;

const PROTOCOL:&str="g7f-core-sealed-policy-candidate-v1";
fn sha(s:&str)->String {format!("{:x}",Sha256::digest(s.as_bytes()))}
fn mode_key(m:TradingMode)->&'static str {
    match m {TradingMode::Expert=>"expert",TradingMode::Scalping=>"scalping",
             TradingMode::Swing=>"swing",TradingMode::Sniper=>"sniper"}
}
pub(crate) fn init(db:&Connection)->Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS g7f_sealed_candidates(
           run_id TEXT PRIMARY KEY,context TEXT NOT NULL,mode TEXT NOT NULL,
           sealed_ms INTEGER NOT NULL,checkpoint TEXT NOT NULL,
           checkpoint_sha256 TEXT NOT NULL,protocol TEXT NOT NULL,
           FOREIGN KEY(run_id) REFERENCES g7_pair_registrations(run_id));
         CREATE TABLE IF NOT EXISTS g7f_sealed_decisions(
           run_id TEXT NOT NULL,bar_time INTEGER NOT NULL,observed_ms INTEGER NOT NULL,
           payload TEXT NOT NULL,payload_sha256 TEXT NOT NULL,
           PRIMARY KEY(run_id,bar_time),
           FOREIGN KEY(run_id) REFERENCES g7f_sealed_candidates(run_id));")?;
    Ok(())
}

/// Only called by explicit stopped/flat Demo Core action. Captures the
/// current adaptive policy ONCE but never consumes future learner updates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn seal(db:&Connection,t:&Telemetry,c:&Config,brains:&[BrainConfig],
    weights:&BTreeMap<String,LearnedWeight>,
    controls:&BTreeMap<String,crate::brain_control::Control>,
    policies:&BTreeMap<String,ExitStudy>,now:u64)->Result<Value>{
    ensure!(t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&t.symbol==c.symbol&&t.tick_age_ms.is_some_and(|a|a<=3000)
        &&matches!(c.trading.mode,TradingMode::Expert|TradingMode::Scalping|TradingMode::Swing),
        "G7-F sealed candidate requires connected fresh MT5 Demo M1 coordinator");
    let context=crate::coordinator_learning::context(t);
    let mode=mode_key(c.trading.mode);
    let row:Option<(String,u64,String,i64)>=db.query_row(
        "SELECT f.id,g.registered_ms,f.snapshot,f.active
         FROM frozen_coordinator_runs f JOIN g7_pair_registrations g ON g.run_id=f.id
         WHERE f.context=?1 AND f.mode=?2 AND f.active=1
         ORDER BY f.created_ms DESC,f.id DESC LIMIT 1",
        params![context,mode],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let (run,registered,original,active)=row.ok_or_else(||
        anyhow::anyhow!("G7-F requires an active G7-preregistered Frozen study"))?;
    ensure!(active==1&&now>registered,"G7-F seal must follow original G7 freeze");
    let previously:i64=db.query_row(
        "SELECT COUNT(*) FROM frozen_coordinator_decisions WHERE run_id=?1",
        [&run],|r|r.get(0))?;
    ensure!(previously==0,"G7-F cannot seal after any prospective decision");
    // Refuse an edited original G7 snapshot even if it parses normally.
    let recorded_hash:String=db.query_row(
        "SELECT frozen_snapshot_sha256 FROM g7_pair_registrations WHERE run_id=?1",
        [&run],|r|r.get(0))?;
    ensure!(sha(&original)==recorded_hash,"G7-F original checkpoint has changed");
    c.validate().map_err(anyhow::Error::msg)?;
    let checkpoint=crate::frozen_coordinator::checkpoint(
        db,t,c,brains,weights,controls,policies,now)?.to_string();
    let digest=sha(&checkpoint);
    let inserted=db.execute(
        "INSERT INTO g7f_sealed_candidates
          (run_id,context,mode,sealed_ms,checkpoint,checkpoint_sha256,protocol)
          VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![run,context,mode,now,checkpoint,digest,PROTOCOL])?;
    ensure!(inserted==1,"G7-F candidate seal was not persisted");
    Ok(json!({"ok":true,"run_id":run,"sealed_ms":now,
        "checkpoint_sha256":digest,"research_only":true,
        "candidate_uses_future_learner_updates":false,
        "adaptive_live_training_unchanged":true,
        "candidate_broker_permission":false,"independent_oos":false}))
}

/// Save an actual prospective, completed-M1 signal generated using the
/// SEALED candidate parameters only; cannot write broker_commands or learner.
#[allow(clippy::too_many_arguments)]
pub(crate) fn observe(db:&Connection,run:&str,context:&str,mode:&str,
    t:&Telemetry,c:&Config,frames:&BTreeMap<String,Vec<FrameBar>>,
    bar:u64,now:u64)->Result<()>{
    let item:Option<(String,String,u64,String,String,String)>=db.query_row(
        "SELECT context,mode,sealed_ms,checkpoint,checkpoint_sha256,protocol
         FROM g7f_sealed_candidates WHERE run_id=?1",
        [run],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))
    ).optional()?;
    let Some((saved_context,saved_mode,sealed,raw,digest,protocol))=item else{return Ok(())};
    ensure!(protocol==PROTOCOL&&saved_context==context&&saved_mode==mode
        &&sha(&raw)==digest,"G7-F candidate seal changed or wrong account/mode");
    if bar.saturating_mul(1000)<=sealed{return Ok(())}
    ensure!(t.demo&&t.account_trade_mode==Some(0)
        &&crate::coordinator_learning::context(t)==context,
        "G7-F candidate changed broker identity");
    let decision=crate::frozen_coordinator::evaluate_checkpoint(&raw,t,c,frames,now)?;
    let prior:Option<(u64,String)>=db.query_row(
        "SELECT observed_ms,adaptive FROM frozen_coordinator_decisions
         WHERE run_id=?1 AND bar_time=?2",
        params![run,bar],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (observed,adaptive)=prior.ok_or_else(||
        anyhow::anyhow!("G7-F missing original same-bar Adaptive decision"))?;
    ensure!(observed==now,"G7-F candidate must share original decision observation time");
    let original:Value=serde_json::from_str(&adaptive)?;
    let packet=json!({"protocol":PROTOCOL,"run_id":run,"context":context,"mode":mode,
        "sealed_ms":sealed,"checkpoint_sha256":digest,
        "bar_time":bar,"observed_ms":now,
        "sealed_candidate":decision,
        "sealed_candidate_signal_sha256":sha(&serde_json::to_string(&decision)?),
        "online_adaptive":original,
        "online_adaptive_signal_sha256":sha(&adaptive),
        "source":"Core/frozen_coordinator.observe/g7f",
        "candidate_training_effect":false,"candidate_broker_orders":false,
        "candidate_booked_pnl":Value::Null,
        "online_adaptive_policy_sealed":false});
    let raw_packet=packet.to_string();
    db.execute(
        "INSERT OR IGNORE INTO g7f_sealed_decisions
         (run_id,bar_time,observed_ms,payload,payload_sha256)
         VALUES (?1,?2,?3,?4,?5)",
        params![run,bar,now,raw_packet,sha(&raw_packet)])?;
    Ok(())
}

pub(crate) fn status(db:&Connection,context:&str,mode:TradingMode)->Result<Value>{
    let found:Option<(String,u64,String,String)>=db.query_row(
        "SELECT c.run_id,c.sealed_ms,c.checkpoint,c.checkpoint_sha256
         FROM g7f_sealed_candidates c JOIN frozen_coordinator_runs f ON f.id=c.run_id
         WHERE c.context=?1 AND c.mode=?2
         ORDER BY c.sealed_ms DESC LIMIT 1",
        params![context,mode_key(mode)],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((run,sealed,raw,hash))=found else{return Ok(json!({
        "status":"not_sealed","research_only":true,"candidate_broker_permission":false}))};
    let tampered=sha(&raw)!=hash;
    let count:i64=db.query_row(
        "SELECT COUNT(*) FROM g7f_sealed_decisions WHERE run_id=?1",
        [&run],|r|r.get(0))?;
    Ok(json!({"status":if tampered {"seal_integrity_failed"}else{"sealed_candidate_research_only"},
        "run_id":run,"sealed_ms":sealed,"checkpoint_sha256":hash,
        "future_candidate_decisions":count,"candidate_policy_immutable_source":!tampered,
        "online_adaptive_policy_sealed":false,"broker_comparator_executed":false,
        "independent_oos":false,"real_authorized":false}))
}

#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::{Candle,execution::Signal};
    fn fixture()->(Connection,Telemetry,Config,Vec<BrainConfig>,String){
        let db=Connection::open_in_memory().unwrap();
        crate::frozen_coordinator::init(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            tick_age_ms:Some(0),quote_time_ms:Some(241_000),
            bid:Some(100.),ask:Some(100.1),
            login:17,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),..Default::default()};
        let c=Config::default();
        let brains=vec![BrainConfig{id:"system".into(),..Default::default()}];
        let id=crate::frozen_coordinator::freeze(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),120_000).unwrap();
        (db,t,c,brains,id["run_id"].as_str().unwrap().into())
    }
    #[test]
    fn sealed_candidate_is_one_time_and_never_creates_broker_commands(){
        let (db,t,c,brains,run)=fixture();
        let key=crate::coordinator_learning::context(&t);
        let sealed=seal(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        assert_eq!(sealed["run_id"],run);
        assert_eq!(sealed["candidate_broker_permission"],false);
        assert!(seal(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),124_000).is_err());
        assert_eq!(status(&db,&key,c.trading.mode).unwrap()["future_candidate_decisions"],0);
        let count:i64=db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='broker_commands'",
            [],|r|r.get(0)).unwrap();
        assert_eq!(count,0,"sealing must not create brokerage tables");
    }
    #[test]
    fn live_adaptive_signal_can_differ_without_changing_sealed_policy(){
        let (db,t,c,brains,run)=fixture();
        seal(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        let frames=BTreeMap::from([("M1".into(),vec![FrameBar{
            candle:Candle{time:180,open:100.,high:101.,low:99.,close:100.5,
                tick_volume:100},spread_points:10,real_volume:0
        }])]);
        let adaptive=Signal{brain:"system".into(),side:1,key:"bar:180".into(),
            bar_time:180,stop_distance:1.,target_distance:2.,hold_ms:60_000,
            reason:"live adaptive policy".into(),ready:true,coordination:None};
        crate::frozen_coordinator::observe(&db,&t,&c,&adaptive,&frames,241_000).unwrap();
        let proof:String=db.query_row("SELECT payload FROM g7f_sealed_decisions WHERE run_id=?1",
            [&run],|r|r.get(0)).unwrap();
        let value:Value=serde_json::from_str(&proof).unwrap();
        assert_eq!(value["online_adaptive"]["reason"],"live adaptive policy");
        assert_eq!(value["online_adaptive_policy_sealed"],false);
        assert_eq!(value["candidate_training_effect"],false);
        assert!(value["candidate_booked_pnl"].is_null());
        crate::frozen_coordinator::observe(&db,&t,&c,&adaptive,&frames,242_000).unwrap();
        let count:i64=db.query_row(
            "SELECT COUNT(*) FROM g7f_sealed_decisions",[],|r|r.get(0)).unwrap();
        assert_eq!(count,1);
    }
    #[test]
    fn changed_seal_fails_and_preserves_prior_g7_pair(){
        let (db,t,c,brains,run)=fixture();
        seal(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),123_000).unwrap();
        db.execute("UPDATE g7f_sealed_candidates SET checkpoint='{}' WHERE run_id=?1",
            [&run]).unwrap();
        let frames=BTreeMap::from([("M1".into(),vec![FrameBar{
            candle:Candle{time:180,open:100.,high:101.,low:99.,close:100.5,
                tick_volume:100},spread_points:10,real_volume:0
        }])]);
        let signal=Signal{brain:"system".into(),side:1,key:"bar:180".into(),
            bar_time:180,stop_distance:1.,target_distance:2.,hold_ms:60_000,
            reason:"live".into(),ready:true,coordination:None};
        assert!(crate::frozen_coordinator::observe(&db,&t,&c,&signal,&frames,241_000).is_err());
        let original:i64=db.query_row("SELECT COUNT(*) FROM frozen_coordinator_decisions",
            [],|r|r.get(0)).unwrap();
        assert_eq!(original,1,"G7-A source must survive sealed-candidate rejection");
        assert_eq!(status(&db,&crate::coordinator_learning::context(&t),
            c.trading.mode).unwrap()["status"],"seal_integrity_failed");
    }
    #[test]
    fn no_postdecision_registration(){
        let (db,t,c,brains,_)=fixture();
        let frames=BTreeMap::from([("M1".into(),vec![FrameBar{
            candle:Candle{time:180,open:100.,high:101.,low:99.,close:100.5,
                tick_volume:100},spread_points:10,real_volume:0
        }])]);
        let signal=Signal{brain:"system".into(),side:1,key:"bar:180".into(),
            bar_time:180,stop_distance:1.,target_distance:2.,hold_ms:60_000,
            reason:"live".into(),ready:true,coordination:None};
        crate::frozen_coordinator::observe(&db,&t,&c,&signal,&frames,241_000).unwrap();
        assert!(seal(&db,&t,&c,&brains,&BTreeMap::new(),
            &BTreeMap::new(),&BTreeMap::new(),242_000).is_err());
    }
}