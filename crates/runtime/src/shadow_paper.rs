//! Independent, quote-observed paper outcomes for non-selected brains.
//! No order_send, broker intent, or live coordinator learner updates.
use anyhow::Result;
use braintrade_domain::{execution::Signal, execution::BrainConfig, Config, Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS paper_brain_positions(\
        context TEXT NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,\
        PRIMARY KEY(context,brain));\
        CREATE TABLE IF NOT EXISTS paper_brain_outcomes(\
        context TEXT NOT NULL,brain TEXT NOT NULL,opened_ms INTEGER NOT NULL,\
        closed_ms INTEGER NOT NULL,payload TEXT NOT NULL,\
        PRIMARY KEY(context,brain,opened_ms));\
        CREATE INDEX IF NOT EXISTS paper_brain_outcome_idx ON paper_brain_outcomes(context,brain,closed_ms);")?;
    Ok(())
}
/// Continuous paper observation is independent of the global START switch.
/// A trade is resolved only on a later distinct quote. Missing quotes do not
/// fabricate profits and never close the virtual position.
pub fn observe(db:&Connection,context:&str,brain:&BrainConfig,signal:&Signal,t:&Telemetry,c:&Config,now:u64)->Result<()> {
    if brain.id=="system" || !t.connected || t.symbol!=c.symbol || t.tick_age_ms.is_none_or(|v|v>3000) {
        return Ok(());
    }
    let Some((bid,ask))=t.bid.zip(t.ask) else {return Ok(())};
    let Some(quote_ms)=t.quote_time_ms else{return Ok(())};
    let Some((tick,profit,loss))=t.tick_size.zip(t.tick_value_profit).zip(t.tick_value_loss).map(|((a,b),c)|(a,b,c)) else{return Ok(())};
    if ![bid,ask,tick,profit,loss].iter().all(|x|x.is_finite()&&*x>0.) || ask<bid
        || quote_ms>now+2000 || now.saturating_sub(quote_ms)>3000 {return Ok(())}
    let raw: Option<String>=db.query_row("SELECT payload FROM paper_brain_positions WHERE context=?1 AND brain=?2",
        params![context,brain.id],|r|r.get(0)).optional()?;
    if let Some(raw)=raw {
        let pos:Value=serde_json::from_str(&raw)?;
        let opened=pos["opened_ms"].as_u64().unwrap_or(0);
        if quote_ms<=opened {return Ok(());}
        let side=pos["side"].as_i64().unwrap_or(0) as f64;
        let entry=pos["entry"].as_f64().unwrap_or(0.);
        let exit=if side>0.{bid}else{ask};
        let movement=(exit-entry)*side;
        let stop=pos["stop_distance"].as_f64().unwrap_or(0.);
        let target=pos["target_distance"].as_f64().unwrap_or(0.);
        let hold=pos["hold_ms"].as_u64().unwrap_or(0);
        let reason=if movement<=-stop {"paper_stop"}else if movement>=target {"paper_target"}
            else if hold>0 && quote_ms.saturating_sub(opened)>=hold {"paper_time"}else{""};
        if !reason.is_empty() {
            let volume=pos["lots"].as_f64().unwrap_or(0.);
            let gain_tick=pos["profit_tick_value"].as_f64().unwrap_or(profit);
            let lose_tick=pos["loss_tick_value"].as_f64().unwrap_or(loss);
            let used_tick=pos["tick_size"].as_f64().unwrap_or(tick);
            let execution_slip=pos["slippage_ticks"].as_f64().unwrap_or(0.)*2.;
            let adjusted=movement / used_tick - execution_slip;
            let gross=adjusted * if adjusted>=0.{gain_tick}else{lose_tick} * volume;
            let fees=pos["commission_per_lot"].as_f64().unwrap_or(0.)*volume;
            let net=gross-fees;
            if net.is_finite() {
                let result=json!({"context":context,"brain":brain.id,"trading_mode_at_open":pos["mode"],
                    "opened_ms":opened,"closed_ms":quote_ms,"entry":entry,"exit":exit,
                    "lots":volume,"reason":reason,"gross":gross,"commission_assumed":fees,
                    "net_paper":net,"side":pos["side"],"signal_reason":pos["signal_reason"],
                    "source":"SHADOW / PRICE-SAMPLED — NO MT5 FILL",
                    "limitations":"Skipped ticks, spread changes, partial fills and overnight costs may differ from actual execution"});
                let tx=db.unchecked_transaction()?;
                tx.execute("INSERT OR IGNORE INTO paper_brain_outcomes VALUES (?1,?2,?3,?4,?5)",
                    params![context,brain.id,opened,quote_ms,result.to_string()])?;
                tx.execute("DELETE FROM paper_brain_positions WHERE context=?1 AND brain=?2",params![context,brain.id])?;
                tx.commit()?;
            }
        }
        return Ok(());
    }
    if !signal.ready || !matches!(signal.side,-1|1)
        || !signal.stop_distance.is_finite() || signal.stop_distance<=0.
        || !signal.target_distance.is_finite() || signal.target_distance<=0.
        || !brain.lots.is_finite() || brain.lots<=0. {return Ok(())}
    // Exclude stale signals: only a fresh broker quote can open a virtual trade.
    let state=json!({"mode":c.trading.mode,"opened_ms":quote_ms,"brain":brain.id,
        "lots":brain.lots,"side":signal.side,"entry":if signal.side>0{ask}else{bid},
        "stop_distance":signal.stop_distance,"target_distance":signal.target_distance,
        "hold_ms":signal.hold_ms,"tick_size":tick,
        "profit_tick_value":profit,"loss_tick_value":loss,
        "commission_per_lot":c.risk.commission_per_lot,
        "slippage_ticks":c.risk.slippage_ticks,"signal_reason":signal.reason});
    db.execute("INSERT OR IGNORE INTO paper_brain_positions VALUES (?1,?2,?3)",
        params![context,brain.id,state.to_string()])?;
    Ok(())
}
pub fn summary(db:&Connection,context:&str)->Result<Value> {
    let mut stmt=db.prepare("SELECT brain,COUNT(*),SUM(CASE WHEN CAST(json_extract(payload,'$.net_paper') AS REAL)>0 THEN 1 ELSE 0 END),SUM(CAST(json_extract(payload,'$.net_paper') AS REAL)) \
        FROM paper_brain_outcomes WHERE context=?1 GROUP BY brain ORDER BY brain")?;
    let mut rows=stmt.query([context])?;
    let mut result=Vec::new();
    while let Some(row)=rows.next()? {
        result.push(json!({"brain":row.get::<_,String>(0)?,
            "closed":row.get::<_,u64>(1)?,
            "wins":row.get::<_,u64>(2)?,
            "net_paper":row.get::<_,f64>(3)?}));
    }
    let open:u64=db.query_row("SELECT COUNT(*) FROM paper_brain_positions WHERE context=?1",
        [context],|r|r.get(0))?;
    Ok(json!({"open":open,"brains":result,"source":"independent shadow price-sampled outcomes; no MT5 orders; no promotion to broker learner"}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_quote_outcomes_are_separate_from_real_broker_deals() {
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        let mut c=Config::default();
        c.risk.commission_per_lot=7.;
        c.risk.slippage_ticks=0.;
        let b=BrainConfig{id:"trend".into(),lots:0.1,..Default::default()};
        let mut t=Telemetry{connected:true,symbol:c.symbol.clone(),bid:Some(2000.),ask:Some(2000.1),
            quote_time_ms:Some(1000),tick_age_ms:Some(0),tick_size:Some(0.01),
            tick_value_profit:Some(1.),tick_value_loss:Some(1.),..Default::default()};
        let signal=Signal {brain:b.id.clone(),side:1,ready:true,target_distance:0.5,stop_distance:0.2,
            hold_ms:10_000,key:"bar:1".into(),bar_time:1,reason:"test".into(),coordination:None};
        observe(&db,"acct",&b,&signal,&t,&c,1000).unwrap();
        t.bid=Some(2000.7);t.ask=Some(2000.8);t.quote_time_ms=Some(1200);
        observe(&db,"acct",&b,&signal,&t,&c,1200).unwrap();
        let result=summary(&db,"acct").unwrap();
        assert_eq!(result["brains"][0]["closed"],1);
        assert_eq!(result["brains"][0]["wins"],1);
        assert!(result["brains"][0]["net_paper"].as_f64().unwrap()>0.);
        let n:u64=db.query_row("SELECT COUNT(*) FROM paper_brain_outcomes",[],|r|r.get(0)).unwrap();
        assert_eq!(n,1);
    }
}
