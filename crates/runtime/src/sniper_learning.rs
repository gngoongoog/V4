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
            if !reward.is_finite() || !net.is_finite() { continue; }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test] fn restart_deduplication_context_definition_and_evidence_isolation() {
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        let t=Telemetry{demo:true,account_trade_mode:Some(0),login:17,server:"Demo".into(),symbol:"XAUUSD".into(),..Default::default()};
        let context=crate::coordinator_learning::context(&t);
        for (i,brain,account,source) in [(1,ID,context.as_str(),"mt5_demo_broker"),(2,ID,context.as_str(),"virtual"),(3,"trend",context.as_str(),"mt5_demo_broker"),(4,ID,"another-account","mt5_demo_broker")] {
            let p=json!({"evidence_source":source,"trading_mode":"sniper","net":-2.,"reward":-0.5,
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
            let payload=json!({"evidence_source":source,"trading_mode":"sniper","net":1.2,"reward":0.4,
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
