//! Rebuild the small cached policy model from the existing, deduplicated complete
//! Demo broker ledger. No new schema, virtual fills or training on Fast Pulse.
use anyhow::Result;
use braintrade_domain::{sniper::{Engine, Models, ID, POLICIES}, Telemetry};
use rusqlite::Connection;
use serde_json::Value;

pub fn refresh(engine: &mut Engine, db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
    let context = crate::coordinator_learning::context(t);
    let mut models = Models::new();
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
        }
    }
    engine.set_models(context, models);
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
}
