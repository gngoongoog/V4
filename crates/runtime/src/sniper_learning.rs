//! Rebuild the small cached policy model from the existing, deduplicated complete
//! Demo broker ledger. No new schema, virtual fills or training on Fast Pulse.
use anyhow::Result;
use braintrade_domain::{sniper::{Engine, Models, TargetModels, ID, POLICIES, TARGET_ARMS}, Telemetry};
use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest,Sha256};

struct SniperBrokerRow {
    source:String, position:i64, context:String, brain:String, closed:i64,
    raw:String, admitted_context:Option<String>,admitted_brain:Option<String>,
    admitted_closed:Option<i64>,admitted_ms:Option<i64>,
    admitted_hash:Option<String>, protocol:Option<String>,
}

fn sniper_broker_rows(db:&Connection,context:&str)->Result<Vec<SniperBrokerRow>>{
    let mut st=db.prepare(
        "SELECT s.source,s.position_id,s.context,s.brain,s.closed_ms,s.payload,
                a.context,a.brain,a.closed_ms,a.admitted_ms,a.payload_sha256,a.protocol
         FROM coordinator_broker_samples s LEFT JOIN coordinator_broker_admissions a
           ON a.source=s.source AND a.position_id=s.position_id
         WHERE s.context=?1 AND s.brain='sniper_learner'
         ORDER BY s.closed_ms DESC,s.position_id DESC LIMIT 2000")?;
    Ok(st.query_map([context],|r|Ok(SniperBrokerRow{
        source:r.get(0)?,position:r.get::<_,i64>(1).unwrap_or(-1),
        context:r.get(2)?,brain:r.get(3)?,
        closed:r.get::<_,i64>(4).unwrap_or(-1),raw:r.get(5)?,
        admitted_context:r.get::<_,Option<String>>(6).unwrap_or(None),
        admitted_brain:r.get::<_,Option<String>>(7).unwrap_or(None),
        admitted_closed:r.get::<_,Option<i64>>(8).unwrap_or(None),
        admitted_ms:r.get::<_,Option<i64>>(9).unwrap_or(None),
        admitted_hash:r.get::<_,Option<String>>(10).unwrap_or(None),
        protocol:r.get::<_,Option<String>>(11).unwrap_or(None),
    }))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn verified_sniper_admission(row:&SniperBrokerRow,context:&str,now:u64)->Option<Value>{
    let pos=u64::try_from(row.position).ok().filter(|v|*v>0)?;
    let close=u64::try_from(row.closed).ok().filter(|v|*v>0)?;
    let admitted=row.admitted_ms.and_then(|v|u64::try_from(v).ok())?;
    if admitted<close || admitted>now || close>now
        || row.context!=context || row.brain!=ID
        || row.admitted_context.as_deref()!=Some(context)
        || row.admitted_brain.as_deref()!=Some(ID)
        || row.admitted_closed!=Some(row.closed)
        || row.protocol.as_deref()!=Some("g6g-runtime-same-transaction-v1")
        || row.admitted_hash.as_deref()!=
             Some(format!("{:x}",Sha256::digest(row.raw.as_bytes())).as_str())
    {return None;}
    let p:Value=serde_json::from_str(&row.raw).ok()?;
    if p["evidence_source"]!="mt5_demo_broker" || p["trading_mode"]!="sniper"
        || p["context"]!=context || p["brain"]!=ID
        || p["position_id"].as_u64()!=Some(pos)
        || p["closed_ms"].as_u64()!=Some(close) {return None;}
    let parts:Value=serde_json::from_str(&row.source).ok()?;
    let tuple=parts.as_array().filter(|a|a.len()==6)?;
    if tuple[0]!=context || tuple[1]!="adaptive-member-v1"
        || tuple[2]!="sniper" || tuple[3]!=ID
        || p["timeframe"]!=tuple[4] || p["report_id"]!=tuple[5]
        || crate::coordinator_learning::verified_broker_feedback(&p).is_none()
    {return None;}
    Some(p)
}

pub fn refresh(engine: &mut Engine, db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
    let context = crate::coordinator_learning::context(t);
    let mut models = Models::new();
    let mut targets=TargetModels::new();
    if t.demo && t.account_trade_mode == Some(0) {
        for row in sniper_broker_rows(db,&context)?.into_iter().rev(){
            let Some(closed)=u64::try_from(row.closed).ok().filter(|v|*v>0) else{continue;};
            if closed>now || now.saturating_sub(closed)>30*24*60*60*1000{continue;}
            // Never train a TP/policy from labels that did not exist at this
            // Core decision time. Corrupt JSON is skipped, not a fatal error.
            let Some(p)=verified_sniper_admission(&row,&context,now) else{continue;};
            let s = &p["selection"];
            if s["brain"] != ID || s["policy"] != "sniper-ticks-v1" { continue; }
            let Some(policy) = POLICIES.iter().position(|name| s["sniper_policy"] == *name) else { continue; };
            let (Some(definition),Some(regime),Some(reward),Some(net)) =
                (s["sniper_definition"].as_str(),s["sniper_regime"].as_str(),p["reward"].as_f64(),p["net"].as_f64()) else { continue; };
            // Reject legacy or malformed broker labels before they can
            // train the Sniper entry-policy or TP-arm. Historical $0 fee
            // assumptions never become a REAL MT5 success signal.
            let Some((verified_net,_,verified_reward))=
                crate::coordinator_learning::verified_broker_feedback(&p) else {continue;};
            if !reward.is_finite()||!net.is_finite()
                || (net-verified_net).abs()>1e-8 ||(reward-verified_reward).abs()>1e-8 {continue;}
            models.entry((definition.into(),regime.into())).or_default()[policy].learn(reward,net);
            // The chosen arm is frozen with the entry's signal; it is NOT
            // inferred from realized profit, MFE or an optimistic simulation.
            // No booked fill => no completed coordinator_broker_samples row.
            if s["sniper_target_mode"]=="adaptive" && p["sniper_tp_preserved"]==true {
                if let Some(arm)=s["sniper_target_arm"].as_u64().filter(|arm|(*arm as usize)<TARGET_ARMS) {
                    if s["sniper_target_pips"].as_f64().is_some_and(|p|p.is_finite()&&p>0.) {
                        targets.entry((definition.into(),regime.into(),POLICIES[policy].into()))
                            .or_default()[arm as usize].learn(reward,net);
                    }
                }
            }
        }
    }
    engine.set_models_with_targets(context, models, targets);
    Ok(())
}

/// Research-only, one-time broker-ledger provenance at STOP/freeze.
/// Mirrors refresh() replay rules, and rejects stale in-memory model counts.
pub fn frozen_training_checkpoint(db:&Connection,t:&Telemetry,model:&Engine,now:u64)->Result<Value>{
    anyhow::ensure!(t.connected&&t.demo&&t.account_trade_mode==Some(0)&&t.login>0
        &&!t.server.is_empty()&&!t.symbol.is_empty(),"Bound MT5 Demo required");
    let context=crate::coordinator_learning::context(t);
    anyhow::ensure!(model.training_account_matches(&context),
        "Frozen Learner cached policy belongs to a different Demo account");
    let rows=sniper_broker_rows(db,&context)?;
    let mut count=0u64;
    let mut targets=0u64;
    let mut latest=0u64;
    let mut earliest:Option<u64>=None;
    let scanned=rows.len();
    for row in rows {
        let Some(closed)=u64::try_from(row.closed).ok().filter(|v|*v>0) else{continue;};
        anyhow::ensure!(closed<=now,"Future-dated broker close cannot enter training cutoff");
        if now.saturating_sub(closed)>30*24*60*60*1000{continue;}
        let Some(p)=verified_sniper_admission(&row,&context,now) else{continue;};
        let s=&p["selection"];
        if p["evidence_source"]!="mt5_demo_broker"||p["trading_mode"]!="sniper"
            ||s["brain"]!=ID||s["policy"]!="sniper-ticks-v1"
            ||!POLICIES.iter().any(|item|s["sniper_policy"]==*item)
            ||s["sniper_definition"].as_str().is_none()
            ||s["sniper_regime"].as_str().is_none()
            ||crate::coordinator_learning::verified_broker_feedback(&p).is_none()
            {continue}
        count+=1;
        latest=latest.max(closed);
        earliest=Some(earliest.map_or(closed,|v|v.min(closed)));
        if s["sniper_target_mode"]=="adaptive"&&p["sniper_tp_preserved"]==true
            &&s["sniper_target_arm"].as_u64().is_some_and(|n|(n as usize)<TARGET_ARMS)
            &&s["sniper_target_pips"].as_f64().is_some_and(|p|p.is_finite()&&p>0.) {
            targets+=1;
        }
    }
    let model_count:u64=model.models.values().flat_map(|a|a.iter()).map(|p|p.samples).sum();
    let model_targets:u64=model.target_models.values().flat_map(|a|a.iter()).map(|p|p.samples).sum();
    anyhow::ensure!(model_count==count&&model_targets==targets,
        "Model scores and broker training ledger differ; refresh before freeze");
    Ok(serde_json::json!({
        "schema":"p1k5c-sniper-learner-training-cutoff-v1",
        "status":"captured","account_context":context,"cutoff_ms":now,
        "source":"deduplicated_complete_mt5_demo_broker_samples",
        "replay_limit":2000,"replay_window_days":30,
        "ledger_rows_scanned":scanned,"broker_training_samples":count,
        "broker_target_samples":targets,
        "cached_policy_samples":model_count,"cached_target_samples":model_targets,
        "latest_training_closed_ms":if count>0{Some(latest)}else{None},
        "earliest_training_closed_ms":earliest,
        "model_scores_frozen":true,"independent_oos":false,
        "broker_pnl":Value::Null,
        "note":"Ledger replay counts cross-checked at freeze, not a cryptographic model fingerprint, broker execution, or independent OOS certification."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Actual test receipts used to have arbitrary source names and no
    // physical MT5 identity. Normalize synthetic fixtures before admitting
    // them; invalid financial receipts are intentionally NOT stamped.
    fn stamp_fixture_sniper(db:&Connection,ctx:&str,now:u64){
        let mut st=db.prepare("SELECT source,position_id,closed_ms,payload
             FROM coordinator_broker_samples WHERE context=?1 AND brain='sniper_learner'").unwrap();
        let rows=st.query_map([ctx],|r|Ok((
            r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,
            r.get::<_,String>(3)?
        ))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        drop(st);
        let source=json!([ctx,"adaptive-member-v1","sniper",ID,"M1",""]).to_string();
        for(previous,pos,closed,raw) in rows{
            let Ok(mut p)=serde_json::from_str::<Value>(&raw) else{continue;};
            p["context"]=json!(ctx);p["brain"]=json!(ID);
            p["position_id"]=json!(pos);p["closed_ms"]=json!(closed);
            p["timeframe"]=json!("M1");p["report_id"]=json!("");
            let normalized=p.to_string();
            db.execute("UPDATE coordinator_broker_samples SET source=?1,payload=?2
                WHERE source=?3 AND position_id=?4",
                rusqlite::params![&source,&normalized,&previous,pos]).unwrap();
            if pos<=0||closed<=0||(closed as u64)>now
                ||p["evidence_source"]!="mt5_demo_broker"
                ||crate::coordinator_learning::verified_broker_feedback(&p).is_none()
            {continue;}
            let hash=format!("{:x}",Sha256::digest(normalized.as_bytes()));
            db.execute("INSERT OR IGNORE INTO coordinator_broker_admissions
                (source,position_id,context,brain,closed_ms,admitted_ms,payload_sha256,protocol)
                VALUES(?1,?2,?3,?4,?5,?6,?7,'g6g-runtime-same-transaction-v1')",
                rusqlite::params![&source,pos,ctx,ID,closed,now,hash]).unwrap();
        }
    }
    #[test] fn frozen_training_cutoff_rejects_unrefreshed_or_future_dated_ledger(){
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        let mut t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:17,server:"Demo".into(),symbol:"XAUUSD".into(),..Default::default()};
        let ctx=crate::coordinator_learning::context(&t);
        let record=json!({"evidence_source":"mt5_demo_broker","trading_mode":"sniper",
            "gross":2.0,"commission":0.0,"swap":0.0,"fee":0.0,
            "closed_volume":0.1,"net":2.,"net_per_lot":20.0,
            "risk_money":0.0,"r_multiple":0.0,"reward":0.25,"selection":{"brain":ID,"policy":"sniper-ticks-v1",
            "sniper_policy":"impulse","sniper_definition":"A","sniper_regime":"B"}});
        db.execute("INSERT INTO coordinator_broker_samples VALUES('first',1,?1,'sniper_learner',1000,?2)",
            rusqlite::params![ctx,record.to_string()]).unwrap();
        stamp_fixture_sniper(&db,&ctx,5000);
        let mut engine=Engine::default();
        assert!(frozen_training_checkpoint(&db,&t,&engine,5000).is_err());
        refresh(&mut engine,&db,&t,5000).unwrap();
        let checkpoint=frozen_training_checkpoint(&db,&t,&engine,5000).unwrap();
        assert_eq!(checkpoint["status"],"captured");
        assert_eq!(checkpoint["broker_training_samples"],1);
        assert_eq!(checkpoint["cached_policy_samples"],1);
        assert_eq!(checkpoint["latest_training_closed_ms"],1000);
        assert_eq!(checkpoint["independent_oos"],false);
        assert!(checkpoint["broker_pnl"].is_null());
        let mut different=t.clone();different.login=18;
        assert!(frozen_training_checkpoint(&db,&different,&engine,5000).is_err(),
            "Even an empty or equal-count ledger cannot certify a different account");
        t.demo=false;t.account_trade_mode=Some(2);
        assert!(frozen_training_checkpoint(&db,&t,&engine,5000).is_err());
        t.demo=true;t.account_trade_mode=Some(0);
        db.execute("INSERT INTO coordinator_broker_samples VALUES('future',2,?1,'sniper_learner',9000,?2)",
            rusqlite::params![ctx,record.to_string()]).unwrap();
        assert!(frozen_training_checkpoint(&db,&t,&engine,5000).is_err());
    }
    #[test] fn restart_deduplication_context_definition_and_evidence_isolation() {
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        let t=Telemetry{demo:true,account_trade_mode:Some(0),login:17,server:"Demo".into(),symbol:"XAUUSD".into(),..Default::default()};
        let context=crate::coordinator_learning::context(&t);
        for (i,brain,account,source) in [(1,ID,context.as_str(),"mt5_demo_broker"),(2,ID,context.as_str(),"virtual"),(3,"trend",context.as_str(),"mt5_demo_broker"),(4,ID,"another-account","mt5_demo_broker")] {
            let p=json!({"evidence_source":source,"trading_mode":"sniper",
                "gross":-2.0,"commission":0.0,"swap":0.0,"fee":0.0,
                "closed_volume":0.1,"net":-2.,"net_per_lot":-20.0,
                "risk_money":0.0,"r_multiple":0.0,"reward":-0.25,
                "selection":{"brain":ID,"policy":"sniper-ticks-v1","sniper_policy":"impulse","sniper_definition":"config-A","sniper_regime":"choppy"}});
            db.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,?3,?4,1000,?5)",rusqlite::params![i.to_string(),i,account,brain,p.to_string()]).unwrap();
        }
        let mut e=Engine::default();
        stamp_fixture_sniper(&db,&context,2000);
        for _ in 0..2 { refresh(&mut e,&db,&t,2000).unwrap(); assert_eq!(e.models.len(),1);assert_eq!(e.models.values().next().unwrap()[0].samples,1); }
        let mut restarted=Engine::default();refresh(&mut restarted,&db,&t,2000).unwrap();
        assert_eq!(restarted.models.values().next().unwrap()[0].reward,e.models.values().next().unwrap()[0].reward);
        let mut live=t.clone();live.demo=false;live.account_trade_mode=Some(2);refresh(&mut e,&db,&live,2000).unwrap();assert!(e.models.is_empty());
    }
    #[test]
    fn invalid_legacy_broker_fee_can_neither_train_sniper_nor_be_frozen(){
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:70,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),..Default::default()};
        let context=crate::coordinator_learning::context(&t);
        let valid=json!({"evidence_source":"mt5_demo_broker","trading_mode":"sniper",
            "gross":2.0,"commission":-0.5,"swap":0.0,"fee":0.0,
            "net":1.5,"net_per_lot":15.0,"closed_volume":0.1,
            "risk_money":0.0,"r_multiple":0.0,"reward":0.25,
            "selection":{"brain":ID,"policy":"sniper-ticks-v1",
                "sniper_policy":"impulse","sniper_definition":"A","sniper_regime":"B"}});
        let mut corrupt=valid.clone();
        corrupt.as_object_mut().unwrap().remove("swap");
        db.execute("INSERT INTO coordinator_broker_samples
            (source,position_id,context,brain,closed_ms,payload)
            VALUES('corrupt',111,?1,'sniper_learner',1000,?2)",
            rusqlite::params![context,corrupt.to_string()]).unwrap();
        let mut engine=Engine::default();
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert!(engine.models.is_empty(),
            "A legacy missing-swap label is not a completed broker training sample");
        let before=frozen_training_checkpoint(&db,&t,&engine,2000).unwrap();
        assert_eq!(before["broker_training_samples"],0);
        db.execute("UPDATE coordinator_broker_samples SET payload=?1 WHERE position_id=111",
            [valid.to_string()]).unwrap();
        stamp_fixture_sniper(&db,&context,2000);
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert_eq!(engine.models.values().next().unwrap()[0].samples,1);
        let after=frozen_training_checkpoint(&db,&t,&engine,2000).unwrap();
        assert_eq!(after["broker_training_samples"],1);
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert_eq!(engine.models.values().next().unwrap()[0].samples,1);
    }

    #[test]
    fn g5e_sniper_models_ignore_forged_reward_with_exact_broker_costs() {
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),login:19,
            server:"Demo".into(),symbol:"XAUUSD".into(),..Default::default()};
        let context=crate::coordinator_learning::context(&t);
        let valid=json!({
            "evidence_source":"mt5_demo_broker","trading_mode":"sniper",
            "gross":2.0,"commission":-0.5,"swap":0.0,"fee":0.0,
            "net":1.5,"net_per_lot":15.0,"closed_volume":0.1,
            "risk_money":5.0,"r_multiple":0.3,"reward":0.3_f64.tanh(),
            "sniper_tp_preserved":true,
            "selection":{"brain":ID,"policy":"sniper-ticks-v1",
              "sniper_policy":"impulse","sniper_definition":"G5E",
              "sniper_regime":"directional_normal",
              "sniper_target_mode":"adaptive","sniper_target_arm":2,
              "sniper_target_pips":6.0}
        });
        let mut false_reward=valid.clone();false_reward["reward"]=json!(0.99);
        db.execute("INSERT INTO coordinator_broker_samples
            VALUES('forged',1,?1,'sniper_learner',1000,?2)",
            rusqlite::params![context,false_reward.to_string()]).unwrap();
        let mut engine=Engine::default();
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert!(engine.models.is_empty());
        assert!(engine.target_models.is_empty());
        assert_eq!(frozen_training_checkpoint(&db,&t,&engine,2000).unwrap()
            ["broker_training_samples"],0);
        db.execute("UPDATE coordinator_broker_samples SET payload=?1
            WHERE position_id=1",[valid.to_string()]).unwrap();
        stamp_fixture_sniper(&db,&context,2000);
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert_eq!(engine.models.values().next().unwrap()[0].samples,1);
        assert_eq!(engine.target_models.values().next().unwrap()[2].samples,1);
        assert_eq!(frozen_training_checkpoint(&db,&t,&engine,2000).unwrap()
            ["broker_training_samples"],1);
    }

    #[test]
    fn target_learning_is_real_demo_only_account_scoped_replay_idempotent_and_arm_specific() {
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        let t=Telemetry{demo:true,account_trade_mode:Some(0),login:22,server:"Demo".into(),symbol:"XAUUSD".into(),..Default::default()};
        let context=crate::coordinator_learning::context(&t);
        for (id,brain,ctx,source,arm,mode) in [
            (1,ID,context.as_str(),"mt5_demo_broker",2,"adaptive"),
            (2,ID,context.as_str(),"virtual",3,"adaptive"),
            (3,"trend",context.as_str(),"mt5_demo_broker",3,"adaptive"),
            (4,ID,"other-account","mt5_demo_broker",3,"adaptive"),
            (5,ID,context.as_str(),"mt5_demo_broker",3,"fixed"),
            (6,ID,context.as_str(),"mt5_demo_broker",6,"adaptive")
        ] {
            let payload=json!({"evidence_source":source,"trading_mode":"sniper",
                "gross":1.2,"commission":0.0,"swap":0.0,"fee":0.0,
                "closed_volume":0.1,"net":1.2,"net_per_lot":12.0,
                "risk_money":0.0,"r_multiple":0.0,"reward":0.25,
                "sniper_tp_preserved":true,
                "selection":{"brain":ID,"policy":"sniper-ticks-v1","sniper_policy":"impulse",
                    "sniper_definition":"config-P","sniper_regime":"directional_normal",
                    "sniper_target_mode":mode,"sniper_target_arm":arm,"sniper_target_pips":6.0}});
            db.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,?3,?4,1000,?5)",
                rusqlite::params![format!("pos-{id}"),id,ctx,brain,payload.to_string()]).unwrap();
        }
        stamp_fixture_sniper(&db,&context,2000);
        let mut engine=Engine::default();
        for _ in 0..3 {
            refresh(&mut engine,&db,&t,2000).unwrap();
            assert_eq!(engine.target_models.len(),1);
            let arms=engine.target_models.values().next().unwrap();
            assert_eq!(arms[2].samples,1);
            assert!(arms.iter().enumerate().all(|(i,a)|i==2 || a.samples==0));
        }
        // The same historical booked net can remain strategy feedback while
        // a broker-widened TP is excluded from the exact pip-arm estimate.
        db.execute("UPDATE coordinator_broker_samples SET payload=json_set(payload,'$.sniper_tp_preserved',false) WHERE position_id=1",[]).unwrap();
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert_eq!(engine.target_models.len(),0);
        let mut live=t.clone();live.demo=false;live.account_trade_mode=Some(2);
        refresh(&mut engine,&db,&live,2000).unwrap();
        assert!(engine.target_models.is_empty());
    }

    #[test]
    fn g6i_sniper_never_trains_from_late_unstamped_or_changed_broker_results(){
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:70,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),..Default::default()};
        let context=crate::coordinator_learning::context(&t);
        let valid=json!({"evidence_source":"mt5_demo_broker","trading_mode":"sniper",
            "gross":2.0,"commission":-0.5,"swap":0.0,"fee":0.0,
            "net":1.5,"closed_volume":0.1,"net_per_lot":15.,
            "risk_money":5.,"r_multiple":0.3,"reward":0.3_f64.tanh(),
            "sniper_tp_preserved":true,
            "selection":{"brain":ID,"policy":"sniper-ticks-v1",
              "sniper_policy":"impulse","sniper_definition":"G6I",
              "sniper_regime":"directional_normal",
              "sniper_target_mode":"adaptive","sniper_target_arm":2,
              "sniper_target_pips":6.}
        });
        db.execute("INSERT INTO coordinator_broker_samples
            VALUES('pre-g6g',70,?1,'sniper_learner',1000,?2)",
            rusqlite::params![&context,valid.to_string()]).unwrap();
        let mut engine=Engine::default();
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert!(engine.models.is_empty(),"Unstamped Broker labels cannot train Sniper");
        stamp_fixture_sniper(&db,&context,2500);
        refresh(&mut engine,&db,&t,2499).unwrap();
        assert!(engine.models.is_empty(),"A close time is not the first Core admission");
        refresh(&mut engine,&db,&t,2500).unwrap();
        assert_eq!(engine.models.values().next().unwrap()[0].samples,1);
        assert_eq!(engine.target_models.values().next().unwrap()[2].samples,1);
        let checkpoint=frozen_training_checkpoint(&db,&t,&engine,2500).unwrap();
        assert_eq!(checkpoint["broker_training_samples"],1);
        db.execute("UPDATE coordinator_broker_samples
            SET payload=json_set(payload,'$.net',100000) WHERE position_id=70",[]).unwrap();
        assert!(frozen_training_checkpoint(&db,&t,&engine,2500).is_err(),
            "Frozen policy must fail if Broker receipt changes after stamped entry");
        refresh(&mut engine,&db,&t,2500).unwrap();
        assert!(engine.models.is_empty());
        // Restored original payload still cannot pass with a malformed
        // admission clock, but it must not crash a running Sniper refresh.
        let reconstructed=json!([context,"adaptive-member-v1","sniper",ID,"M1",""]).to_string();
        db.execute("UPDATE coordinator_broker_samples SET payload=?1 WHERE source=?2 AND position_id=70",
            rusqlite::params![{
                let mut p=valid.clone();
                p["context"]=json!(context);p["brain"]=json!(ID);
                p["position_id"]=json!(70);p["closed_ms"]=json!(1000);
                p["timeframe"]=json!("M1");p["report_id"]=json!("");
                p.to_string()
            },&reconstructed]).unwrap();
        db.execute("UPDATE coordinator_broker_admissions SET admitted_ms='invalid'
            WHERE source=?1 AND position_id=70",[&reconstructed]).unwrap();
        refresh(&mut engine,&db,&t,2500).unwrap();
        assert!(engine.models.is_empty());
    }

}
