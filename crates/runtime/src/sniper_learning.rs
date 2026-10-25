//! Rebuild the small cached policy model from the existing, deduplicated complete
//! Demo broker ledger. No new schema, virtual fills or training on Fast Pulse.
use anyhow::Result;
use braintrade_domain::{sniper::{Engine, Models, TargetModels, ID, POLICIES, TARGET_ARMS}, Telemetry};
use rusqlite::Connection;
use serde_json::Value;

pub fn refresh(engine: &mut Engine, db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
    let context = crate::coordinator_learning::context(t);
    let mut models = Models::new();
    let mut targets=TargetModels::new();
    if t.demo && t.account_trade_mode == Some(0) {
        let mut st = db.prepare("SELECT closed_ms,payload FROM coordinator_broker_samples WHERE context=?1 AND brain='sniper_learner' ORDER BY closed_ms DESC,position_id DESC LIMIT 2000")?;
        let rows = st.query_map([&context], |r| Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (closed, raw) in rows.into_iter().rev() {
            if closed == 0 || closed > now || now.saturating_sub(closed) > 30*24*60*60*1000 { continue; }
            let p: Value = serde_json::from_str(&raw)?;
            if p["evidence_source"] != "mt5_demo_broker" || p["trading_mode"] != "sniper" { continue; }
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
    let mut query=db.prepare(
        "SELECT closed_ms,payload FROM coordinator_broker_samples
         WHERE context=?1 AND brain='sniper_learner'
         ORDER BY closed_ms DESC,position_id DESC LIMIT 2000")?;
    let rows=query.query_map([&context],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut count=0u64;
    let mut targets=0u64;
    let mut latest=0u64;
    let mut earliest:Option<u64>=None;
    let scanned=rows.len();
    for (closed,raw) in rows {
        anyhow::ensure!(closed<=now,"Future-dated broker close cannot enter training cutoff");
        if closed==0||now.saturating_sub(closed)>30*24*60*60*1000{continue}
        let p:Value=serde_json::from_str(&raw)?;
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
        db.execute("UPDATE coordinator_broker_samples SET payload=?1 WHERE source='corrupt'",
            [valid.to_string()]).unwrap();
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
            WHERE source='forged'",[valid.to_string()]).unwrap();
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
        db.execute("UPDATE coordinator_broker_samples SET payload=json_set(payload,'$.sniper_tp_preserved',false) WHERE source='pos-1'",[]).unwrap();
        refresh(&mut engine,&db,&t,2000).unwrap();
        assert_eq!(engine.target_models.len(),0);
        let mut live=t.clone();live.demo=false;live.account_trade_mode=Some(2);
        refresh(&mut engine,&db,&live,2000).unwrap();
        assert!(engine.target_models.is_empty());
    }

}
