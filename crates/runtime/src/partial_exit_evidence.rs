//! Broker-deal-based partial-exit audit; informational only.
//!
//! Matching deal volume to a hypothetical TP split is NOT proof that a TP
//! strategy executed. No stage is advanced here. Only explicit future
//! stage->durable broker command->MT5 order/deal linkage may award that credit.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params,Connection};
use serde_json::{json,Value};
use std::collections::BTreeSet;

fn account_scope(t:&Telemetry,target:&str)->Option<String>{
    if !t.connected || !t.validate() || t.login==0 || t.server.is_empty()
        || t.symbol.is_empty() || t.currency.is_empty()
        || !matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2))) {return None}
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).ok()
}
fn number(d:&Value,k:&str)->Option<f64>{
    d[k].as_f64().filter(|x|x.is_finite())
}
#[derive(Default)]
struct Totals {
    open:f64,close:f64,net_cost:f64,
    entry_deals:u32,exit_deals:u32
}
fn calculate(deals:&[(u64,String)],pid:u64,magic:u64,symbol:&str,volume:f64,step:f64)
    -> Result<Value>
{
    let mut seen=BTreeSet::new();
    let mut sum=Totals::default();
    let mut bad=false;
    let mut first_side=None::<i64>;
    for (ticket,raw) in deals{
        let Ok(d)=serde_json::from_str::<Value>(raw) else {bad=true;break};
        if *ticket==0 || !seen.insert(*ticket)
            || d["ticket"].as_u64()!=Some(*ticket)
            || d["position_id"].as_u64()!=Some(pid)
            || d["symbol"].as_str()!=Some(symbol)
            || !matches!(d["type"].as_i64(),Some(0|1))
            || !d["time_msc"].as_u64().is_some_and(|n|n>0)
            || !number(&d,"price").is_some_and(|n|n>0.)
            || !number(&d,"volume").is_some_and(|n|n>0.)
            || ["profit","commission","swap","fee"].iter()
                .any(|k|number(&d,k).is_none())
        {bad=true;break}
        let kind=d["entry"].as_i64();
        let side=d["type"].as_i64().unwrap();
        let lots=number(&d,"volume").unwrap();
        match kind{
            Some(0)=>{
                // The physical position must be opened by this exact owned
                // brain; exits are allowed to have broker-side magic 0.
                if d["magic"].as_u64()!=Some(magic)
                    || first_side.is_some_and(|first|first!=side) {bad=true;break}
                first_side=Some(side);
                sum.open+=lots;sum.entry_deals+=1;
            }
            Some(1|3)=>{
                if first_side.is_some_and(|first|first==side){bad=true;break}
                sum.close+=lots;sum.exit_deals+=1;
            }
            // Netting INOUT reversal, adjustments and unknown deal kinds
            // cannot establish a single owned physical position history.
            _=>{bad=true;break}
        }
        sum.net_cost += ["profit","commission","swap","fee"].iter()
            .map(|k|number(&d,k).unwrap()).sum::<f64>();
        if !sum.open.is_finite() || !sum.close.is_finite() || !sum.net_cost.is_finite(){
            bad=true;break
        }
    }
    let tolerance=(step*1e-6).max(volume*1e-8);
    let origin_valid=!bad&&sum.entry_deals>0
        &&first_side.is_some()&&(sum.open-volume).abs()<=tolerance
        &&sum.close<=sum.open+tolerance;
    let state=if bad{"invalid_deal_history"}
        else if !origin_valid{"incomplete_or_ambiguous_open_history"}
        else if sum.exit_deals==0{"no_broker_exit_deals"}
        else if sum.close+ tolerance < sum.open{"broker_partial_exit_unattributed"}
        else{"broker_flat_by_deals_unattributed"};
    Ok(json!({
        "state":state,
        "opening_deals":sum.entry_deals,
        "exit_deals":sum.exit_deals,
        "broker_open_lots":if origin_valid{json!(sum.open)}else{Value::Null},
        "broker_closed_lots":if origin_valid{json!(sum.close)}else{Value::Null},
        "booked_net_account_currency":if origin_valid{json!(sum.net_cost)}else{Value::Null},
        "tp_stage_attribution_verified":false,
        "stage_completed":false,
        "note":"Historical exit volumes cannot prove which planned TP stage closed. No stage sends, profit credit, or automatic BE."
    }))
}
/// Read-only query for a verified, bound account. Never mutates journal or
/// silently marks stage plans filled. A synced watermark is metadata, not
/// proof that all late broker deals have arrived.
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    let Some(key)=account_scope(t,target) else {
        return Ok(json!({"schema":"partial-exit-deal-evidence-v1",
            "status":"unverified_account","read_only":true,
            "orders_enabled":false,"rows":[]}))
    };
    let sync=db.query_row(
        "SELECT until_ms,synced_ms FROM broker_history_sync WHERE context=?1 AND target=?2",
        params![key,target],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,u64>(1)?)));
    let watermark=match sync{
        Ok(v)=>Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows)=>None,
        Err(e)=>return Err(e.into())
    };
    let mut stmt=db.prepare(
        "SELECT position_identifier,magic,initial_volume,step,status
         FROM partial_exit_drafts WHERE context=?1 AND target=?2
         ORDER BY created_ms DESC,position_identifier DESC LIMIT 25")?;
    let drafts=stmt.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,
        r.get::<_,f64>(2)?,r.get::<_,f64>(3)?,
        r.get::<_,String>(4)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut rows=Vec::new();
    for (pid,magic,volume,step,revision) in drafts{
        let mut st=db.prepare(
            "SELECT ticket,payload FROM broker_deals
             WHERE context=?1 AND position_id=?2 ORDER BY ticket LIMIT 1001")?;
        let deals=st.query_map(params![key,pid],|r|Ok((
            r.get::<_,u64>(0)?,r.get::<_,String>(1)?
        )))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let evidence=if deals.len()>1000 {
            json!({"state":"too_many_deals_to_audit","stage_completed":false,
                "tp_stage_attribution_verified":false})
        }else if watermark.is_none(){
            json!({"state":"broker_history_not_synced",
                "stage_completed":false,"tp_stage_attribution_verified":false})
        }else if !step.is_finite() || step<=0. || !volume.is_finite() || volume<=0.{
            json!({"state":"invalid_draft_geometry","stage_completed":false,
                "tp_stage_attribution_verified":false})
        }else{
            calculate(&deals,pid,magic,&t.symbol,volume,step)?
        };
        rows.push(json!({
            "position_identifier":pid,
            "draft_state":revision,
            "evidence":evidence
        }));
    }
    Ok(json!({
        "schema":"partial-exit-deal-evidence-v1","status":"broker_history_audit_only",
        "context":key,"target":target,
        "history_until_ms":watermark.map(|x|x.0),
        "history_synced_ms":watermark.map(|x|x.1),
        "read_only":true,"orders_enabled":false,
        "tp_stage_execution_supported":false,
        "rows":rows,
        "note":"A broker-booked partial close may be observed without assigning it to TP1/2/3. The source includes only Core-reconciled real MT5 deals; never inferred fills."
    }))
}

#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    fn t()->Telemetry{
        Telemetry{connected:true,login:42,demo:true,account_trade_mode:Some(0),
            server:"D".into(),currency:"USD".into(),symbol:"XAUUSDm".into(),
            volume_min:Some(0.01),volume_step:Some(0.01),volume_max:Some(100.),
            positions:vec![Position{ticket:71,identifier:72,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.1,..Default::default()}],
            ..Default::default()}
    }
    fn fixture()->Connection{
        let d=Connection::open_in_memory().unwrap();
        crate::partial_exit_ledger::init(&d).unwrap();
        d.execute_batch(
            "CREATE TABLE broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,
             position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,
             PRIMARY KEY(context,ticket));
             CREATE TABLE broker_history_sync(context TEXT NOT NULL,target TEXT NOT NULL,
             until_ms INTEGER NOT NULL,synced_ms INTEGER NOT NULL,
             PRIMARY KEY(context,target));").unwrap();
        d
    }
    fn deal(ticket:u64,entry:i64,side:i64,magic:u64,volume:f64)->Value{
        json!({"ticket":ticket,"position_id":72,"symbol":"XAUUSDm",
            "magic":magic,"type":side,"entry":entry,"time_msc":100,
            "volume":volume,"price":2000.0,
            "profit":0.25,"commission":-0.07,"swap":0.0,"fee":0.0})
    }
    fn store(d:&Connection,t:&Telemetry,entry:Value){
        d.execute("INSERT INTO broker_deals VALUES(?1,?2,72,'gann',?3)",
            params![context_for(t),entry["ticket"].as_u64().unwrap(),entry.to_string()]).unwrap();
    }
    fn context_for(t:&Telemetry)->String{
        serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap()
    }
    fn init_evidence(d:&Connection,t:&Telemetry){
        crate::partial_exit_ledger::observe(d,t,"demo",120).unwrap();
        d.execute("INSERT INTO broker_history_sync VALUES(?1,'demo',200,201)",
            [context_for(t)]).unwrap();
    }
    #[test]
    fn unrelated_close_cannot_claim_any_tp_stage_while_verified_deal_volume_is_reported(){
        let d=fixture();let t=t();init_evidence(&d,&t);
        store(&d,&t,deal(100,0,0,410601,0.1));
        store(&d,&t,deal(101,1,1,0,0.04));
        let r=report(&d,&t,"demo").unwrap();
        let e=&r["rows"][0]["evidence"];
        assert_eq!(e["state"],"broker_partial_exit_unattributed");
        assert_eq!(e["broker_closed_lots"],0.04);
        assert_eq!(e["tp_stage_attribution_verified"],false);
        assert_eq!(e["stage_completed"],false);
        assert_eq!(r["orders_enabled"],false);
        store(&d,&t,deal(102,1,1,0,0.06));
        let e=report(&d,&t,"demo").unwrap()["rows"][0]["evidence"].clone();
        assert_eq!(e["state"],"broker_flat_by_deals_unattributed");
        assert_eq!(e["broker_closed_lots"],0.1);
        assert_eq!(e["stage_completed"],false);
    }
    #[test]
    fn wrong_origin_and_corrupt_receipts_never_create_confirmed_tp(){
        let d=fixture();let t=t();init_evidence(&d,&t);
        store(&d,&t,deal(100,0,0,410603,0.1));
        let x=report(&d,&t,"demo").unwrap();
        assert_eq!(x["rows"][0]["evidence"]["state"],"invalid_deal_history");
        assert_eq!(x["rows"][0]["evidence"]["broker_closed_lots"],Value::Null);
        d.execute("DELETE FROM broker_deals",[]).unwrap();
        let mut corrupt=deal(100,0,0,410601,0.1);
        corrupt.as_object_mut().unwrap().remove("commission");
        store(&d,&t,corrupt);
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["evidence"]["state"],
            "invalid_deal_history");
        d.execute("DELETE FROM broker_deals",[]).unwrap();
        store(&d,&t,deal(100,0,0,410601,0.1));
        store(&d,&t,deal(101,2,1,0,0.04));
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["evidence"]["state"],
            "invalid_deal_history");
    }
    #[test]
    fn missing_history_watermark_or_opening_evidence_is_unknown(){
        let d=fixture();let t=t();
        crate::partial_exit_ledger::observe(&d,&t,"demo",120).unwrap();
        store(&d,&t,deal(101,1,1,0,0.04));
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["evidence"]["state"],
            "broker_history_not_synced");
        d.execute("INSERT INTO broker_history_sync VALUES(?1,'demo',200,201)",
            [context_for(&t)]).unwrap();
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["evidence"]["state"],
            "incomplete_or_ambiguous_open_history");
    }
    #[test]
    fn cross_account_and_target_are_strictly_isolated(){
        let d=fixture();let t=t();init_evidence(&d,&t);
        let mut other=t.clone();other.login=90;
        assert_eq!(report(&d,&other,"demo").unwrap()["rows"].as_array().unwrap().len(),0);
        other.connected=false;
        assert_eq!(report(&d,&other,"demo").unwrap()["status"],"unverified_account");
        other=t.clone();other.demo=false;other.account_trade_mode=Some(2);
        assert_eq!(report(&d,&other,"live").unwrap()["rows"].as_array().unwrap().len(),0);
    }
}