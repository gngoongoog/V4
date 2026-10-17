//! Exact, position-linked MT5 closed-deal feedback for the independent adaptive brain.
//! Never substitutes simulated shadow/backtest PnL for actual booked broker PnL.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub regime: String,
    pub policy: usize,
    pub reward_r: f64,
    pub net: f64,
}

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS adaptive_brain_broker_samples(\
         context TEXT NOT NULL, position_id INTEGER NOT NULL, command_id TEXT NOT NULL,\
         closed_ms INTEGER NOT NULL, regime TEXT NOT NULL, policy INTEGER NOT NULL,\
         payload TEXT NOT NULL, PRIMARY KEY(context,position_id));\
         CREATE UNIQUE INDEX IF NOT EXISTS adaptive_brain_broker_commands ON adaptive_brain_broker_samples(context,command_id);"
    )?;
    Ok(())
}
pub fn load(db: &Connection, context: &str) -> Result<Vec<Outcome>> {
    let mut stmt=db.prepare(
        "SELECT payload FROM adaptive_brain_broker_samples WHERE context=?1 ORDER BY closed_ms,position_id"
    )?;
    let rows=stmt.query_map([context],|r|r.get::<_,String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().map(|raw|serde_json::from_str(&raw)).collect::<serde_json::Result<Vec<_>>>()?)
}
pub fn sync(db: &Connection, t: &Telemetry) -> Result<Vec<Outcome>> {
    if !matches!(t.account_trade_mode,Some(0)|Some(2)) {return Ok(vec![]);}
    let ctx=crate::coordinator_learning::context(t);
    let mut statement=db.prepare(
        "SELECT b.id,b.payload,b.result FROM broker_commands b \
         WHERE b.context=?1 AND b.kind='open' AND b.brain IN ('adaptive','system') \
         AND CAST(json_extract(b.result,'$.deal') AS INTEGER)>0 \
         AND NOT EXISTS(SELECT 1 FROM adaptive_brain_broker_samples s \
           WHERE s.context=b.context AND s.command_id=b.id) \
         ORDER BY b.created_ms,b.id"
    )?;
    let commands=statement.query_map([&ctx],|r|Ok((
        r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut new_samples=Vec::new();
    for (command_id,raw_cmd,raw_result) in commands {
        let cmd:Value=serde_json::from_str(&raw_cmd)?;
        let result:Value=serde_json::from_str(&raw_result)?;
        let Some(open_deal)=result["deal"].as_u64().filter(|n|*n>0)else{continue};
        let pid:Option<u64>=db.query_row(
            "SELECT position_id FROM broker_deals WHERE context=?1 AND ticket=?2",
            params![&ctx,open_deal],|r|r.get(0)
        ).optional()?;
        let Some(position_id)=pid.filter(|id|*id>0) else{continue};
        // A partial close is NOT a fully closed training outcome.
        if t.positions.iter().any(|p|p.identifier==position_id||p.ticket==position_id){continue;}
        let signal_key=if cmd["brain"]=="adaptive" {
            cmd["signal"]["key"].as_str()
        } else {
            let executed_side=cmd["signal"]["side"].as_i64();
            cmd["signal"]["coordination"]["members"].as_array()
                .and_then(|members|members.iter().find(|member|
                    member["id"]=="adaptive" && member["ready"]==true
                    && member["side"].as_i64()==executed_side))
                .and_then(|member|member["signal_key"].as_str())
        };
        let Some(signal_key)=signal_key.filter(|key|key.starts_with("adaptive:M1:"))else{continue};
        let mut parts=signal_key.rsplit(':');
        let policy_name=parts.next().unwrap_or("");
        let regime=parts.next().unwrap_or("");
        let policy=match policy_name {"trend"=>0,"reversion"=>1,_=>continue};
        if regime.is_empty(){continue;}
        let mut deals_stmt=db.prepare(
            "SELECT payload FROM broker_deals WHERE context=?1 AND position_id=?2 ORDER BY ticket"
        )?;
        let deals=deals_stmt.query_map(params![&ctx,position_id],|r|r.get::<_,String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let(mut net,mut exit_lots,mut closed_ms,mut has_exit)=(0.0,0.0,0_u64,false);
        for raw in deals {
            let deal:Value=serde_json::from_str(&raw)?;
            net+=deal["profit"].as_f64().unwrap_or(0.)
                +deal["commission"].as_f64().unwrap_or(0.)
                +deal["swap"].as_f64().unwrap_or(0.)
                +deal["fee"].as_f64().unwrap_or(0.);
            if matches!(deal["entry"].as_i64(),Some(1)|Some(3)) {
                has_exit=true;
                let lots=deal["volume"].as_f64().unwrap_or(0.);
                if lots.is_finite() && lots>0. {exit_lots+=lots;}
                closed_ms=closed_ms.max(deal["time_msc"].as_u64().unwrap_or(0));
            }
        }
        if !has_exit||!net.is_finite()||!exit_lots.is_finite()||exit_lots<=0. {continue;}
        // MT5's booked PnL, fees and volume are authoritative; the risk
        // denominator is an estimate using the original frozen stop distance.
        let stop=cmd["signal"]["stop_distance"].as_f64().unwrap_or(0.);
        let risk=t.tick_size.zip(t.tick_value_loss)
            .filter(|(tick,loss)|tick.is_finite() && loss.is_finite() && *tick>0. && *loss>0.)
            .map(|(tick,loss)|stop/tick*loss*exit_lots).unwrap_or(0.);
        if !risk.is_finite()||risk<=0. {continue;}
        let o=Outcome{regime:regime.into(),policy,reward_r:(net/risk).clamp(-3.,3.),net};
        let source=if cmd["brain"]=="system" {"coordinator_supported"}else{"adaptive_solo"};
        let stored=json!({
            "regime":o.regime,"policy":o.policy,"reward_r":o.reward_r,"net":o.net,
            "source":"exact_mt5_closed_position","attribution":source,
            "position_id":position_id,"command_id":command_id,
            "opened_deal":open_deal,"closed_ms":closed_ms,"closed_lots":exit_lots,
            "estimated_stop_risk":risk,
            "costs":"MT5 booked profit+commission+swap+fee"
        });
        let inserted=db.execute(
            "INSERT OR IGNORE INTO adaptive_brain_broker_samples(context,position_id,command_id,closed_ms,regime,policy,payload) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![&ctx,position_id,&command_id,closed_ms,regime,policy as i64,stored.to_string()],
        )?;
        if inserted==1{new_samples.push(o);}
    }
    Ok(new_samples)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_broker_evidence_is_invented_without_deals() {
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        let t=Telemetry::default();
        assert!(sync(&db,&t).unwrap().is_empty());
    }
    #[test]
    fn exact_mt5_booked_trade_is_learned_once_with_all_fees() {
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');\
             CREATE TABLE broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,ticket));"
        ).unwrap();
        let t=Telemetry {
            connected:true,server:"DemoServer".into(),login:77,
            symbol:"XAUUSD".into(),currency:"USD".into(),
            account_trade_mode:Some(0),demo:true,
            tick_size:Some(0.01),tick_value_loss:Some(1.),
            ..Default::default()
        };
        let context=crate::coordinator_learning::context(&t);
        let cmd=json!({
            "id":"open1","brain":"adaptive","signal":{
                "key":"adaptive:M1:1800000000:impulse_normal_cost:trend",
                "side":1,"stop_distance":2.0
            }
        });
        db.execute(
            "INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result) VALUES('open1',?1,'adaptive','open',1,'observed',?2,?3)",
            params![&context,cmd.to_string(),json!({"deal":100}).to_string()]
        ).unwrap();
        for (ticket,entry,profit,commission,swap,fee) in [
            (100,0,0.0,-0.40,0.0,0.0),
            (101,1,4.0,-0.40,-0.10,-0.20),
        ] {
            let deal=json!({
                "ticket":ticket,"position_id":900,"entry":entry,
                "profit":profit,"commission":commission,"swap":swap,"fee":fee,
                "volume":0.1,"time_msc":ticket*1000
            });
            db.execute(
                "INSERT INTO broker_deals(context,ticket,position_id,brain,payload) VALUES(?1,?2,900,'adaptive',?3)",
                params![&context,ticket,deal.to_string()]
            ).unwrap();
        }
        let outcome=sync(&db,&t).unwrap();
        assert_eq!(outcome.len(),1);
        assert_eq!(outcome[0].policy,0);
        assert!((outcome[0].net-2.90).abs()<1e-9);
        assert!(outcome[0].reward_r.is_finite());
        assert!(sync(&db,&t).unwrap().is_empty());
        assert_eq!(load(&db,&context).unwrap().len(),1);
    }
}
