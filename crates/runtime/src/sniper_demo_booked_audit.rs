//! P1-K5H: read-only, exact booked MT5 Demo broker close audit.
//! These are ACTUAL closed trades from the execution coordinator's broker
//! deal reconciliation, never fills of the frozen Sniper research study.
//! DO NOT pair them with 5-second research markouts or present one as the
//! other's net PnL / Win Rate. No orders or SQLite writes here.
use anyhow::Result;
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use std::collections::BTreeSet;

const SCHEMA:&str="p1k5h-demo-booked-trade-audit-v1";
const MAX_ROWS:usize=2000;
fn empty(reason:&str)->Value {
    json!({"schema":SCHEMA,"status":"insufficient_broker_evidence",
        "reason":reason,"source":"reconciled_mt5_demo_broker_deals",
        "physical_broker_trades":0,"independent_oos":false,
        "frozen_study_sent_orders":false,"matched_with_frozen_markouts":false,
        "comparison_with_five_second_markout_permitted":false,
        "net_demo_account_currency":Value::Null,
        "strategy_win_rate":Value::Null,
        "real_account_data":false})
}
fn exists(db:&Connection)->Result<bool>{
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='coordinator_broker_samples')",
        [],|r|r.get(0))?)
}
/// Re-derive every booked amount from the per-ticket original MT5 Demo
/// receipts and the observed open command, not merely a JSON aggregate.
/// Missing history, partial close or corrupt receipt => no financial total.
/// This does NOT independently attest the gateway's interaction with MT5.
fn receipts_reconcile(db:&Connection,account:&str,position:u64,sample:&Value)
    ->Result<bool>{
    let present:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='broker_deals')",
        [],|r|r.get(0))?;
    if !present{return Ok(false)}
    let commands:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='broker_commands')",
        [],|r|r.get(0))?;
    if !commands{return Ok(false)}
    let command_id=sample["open_command_id"].as_str().unwrap_or("");
    let cmd:Option<(String,String,String)>=db.query_row(
        "SELECT brain,payload,result FROM broker_commands
         WHERE context=?1 AND id=?2 AND kind='open'",
        params![account,command_id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))
    ).optional()?;
    let Some((brain,cmd,reply))=cmd else{return Ok(false)};
    let Ok(cmd)=serde_json::from_str::<Value>(&cmd) else{return Ok(false)};
    let Ok(reply)=serde_json::from_str::<Value>(&reply) else{return Ok(false)};
    let open_ticket=sample["open_deal"].as_u64().unwrap_or(0);
    if brain!=sample["trade_owner"].as_str().unwrap_or("")
       ||reply["deal"].as_u64()!=Some(open_ticket)
       ||cmd["signal"]["coordination"]["selection"]["brain"]!="sniper_learner"
       ||cmd["signal"]["coordination"]["selection"]["policy"]!="sniper-ticks-v1"
       ||cmd["signal"]["side"].as_i64().is_none_or(|side|!matches!(side,-1|1)){
        return Ok(false)
    }
    let mut stmt=db.prepare(
        "SELECT ticket,payload FROM broker_deals
         WHERE context=?1 AND position_id=?2 ORDER BY ticket LIMIT 1001")?;
    let rows=stmt.query_map(params![account,position],
        |r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len()<2||rows.len()>1000{return Ok(false)}
    let (mut buy_in,mut closed,mut gross,mut comm,mut swap,mut fee,mut latest)=
        (0.,0.,0.,0.,0.,0.,0u64);
    let mut original_open=false;
    for (ticket,raw) in rows {
        let Ok(d)=serde_json::from_str::<Value>(&raw)else{return Ok(false)};
        let float=|key:&str|d[key].as_f64().filter(|v|v.is_finite());
        let (Some(volume),Some(profit),Some(c),Some(sw),Some(f))=(
            float("volume"),float("profit"),float("commission"),
            float("swap"),float("fee"))else{return Ok(false)};
        if d["ticket"].as_u64()!=Some(ticket)
            ||d["position_id"].as_u64()!=Some(position)
            ||volume<=0.||volume>100.{
            return Ok(false)
        }
        match d["entry"].as_i64(){
            Some(0)=>{buy_in+=volume;if ticket==open_ticket{original_open=true}},
            Some(1)|Some(3)=>{
                closed+=volume;
                let Some(ts)=d["time_msc"].as_u64().filter(|ts|*ts>0)else{return Ok(false)};
                latest=latest.max(ts);
            },
            // Netting reversal/unknown entry creates ambiguous attribution.
            _=>return Ok(false)
        }
        gross+=profit;comm+=c;swap+=sw;fee+=f;
    }
    let approx=|a:f64,b:f64|{
        a.is_finite()&&b.is_finite()
        &&(a-b).abs()<=1e-6*(1.+a.abs().max(b.abs()))
    };
    Ok(original_open&&latest==sample["closed_ms"].as_u64().unwrap_or(0)
        &&buy_in>0.&&closed>0.
        &&approx(buy_in,closed)
        &&approx(closed,sample["closed_volume"].as_f64().unwrap_or(f64::NAN))
        &&approx(gross,sample["gross"].as_f64().unwrap_or(f64::NAN))
        &&approx(comm,sample["commission"].as_f64().unwrap_or(f64::NAN))
        &&approx(swap,sample["swap"].as_f64().unwrap_or(f64::NAN))
        &&approx(fee,sample["fee"].as_f64().unwrap_or(f64::NAN))
        &&approx(gross+comm+swap+fee,
                  sample["net"].as_f64().unwrap_or(f64::NAN)))
}

/// Account context and cutoff MUST be taken from a verified frozen research
/// artifact. Frozen research places ZERO orders and therefore must not be
/// mislabeled as matching these separately executed coordinator trades.
pub fn inspect(db:&Connection,account:&str,artifact_id:&str,hash:&str)->Result<Value>{
    let proof=crate::sniper_research_artifact::verify(
        db,account,artifact_id,Some(hash))?;
    let cutoff=proof["cutoff_ms"].as_u64().unwrap_or(u64::MAX);
    if !exists(db)?{return Ok(empty("no_reconciled_broker_ledger"))}
    let total:i64=db.query_row(
        "SELECT COUNT(*) FROM coordinator_broker_samples
         WHERE context=?1 AND brain='sniper_learner' AND closed_ms>?2",
        params![account,cutoff],|r|r.get(0))?;
    if total<0 || total as usize>MAX_ROWS{
        return Ok(empty("broker_history_exceeds_bounded_audit_limit"))
    }
    let mut stmt=db.prepare(
        "SELECT source,position_id,closed_ms,payload FROM coordinator_broker_samples
         WHERE context=?1 AND brain='sniper_learner' AND closed_ms>?2
         ORDER BY closed_ms,position_id,source LIMIT 2000")?;
    let rows=stmt.query_map(params![account,cutoff],|r|
        Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,
            r.get::<_,u64>(2)?,r.get::<_,String>(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut positions=BTreeSet::new();
    let (mut gross,mut commission,mut swap,mut fee,mut net,mut volume)=
        (0.,0.,0.,0.,0.,0.);
    let mut accepted=0u64;
    let mut refused=0u64;
    let mut rejected_reasons=BTreeSet::new();
    for (source,position_id,closed,raw) in rows {
        let Ok(v)=serde_json::from_str::<Value>(&raw)else {
            refused+=1;rejected_reasons.insert("bad_payload_json");continue
        };
        let finite=|key:&str|v[key].as_f64().filter(|x|x.is_finite());
        let (Some(g),Some(c),Some(s),Some(f),Some(n),Some(vol))=(
            finite("gross"),finite("commission"),finite("swap"),
            finite("fee"),finite("net"),finite("closed_volume"))
        else {refused+=1;rejected_reasons.insert("missing_booked_costs");continue};
        let valid=v["evidence_source"]=="mt5_demo_broker"
            &&v["context"]==account
            &&v["brain"]=="sniper_learner"
            &&v["trading_mode"]=="sniper"
            &&v["position_id"].as_u64()==Some(position_id)
            &&v["closed_ms"].as_u64()==Some(closed)
            &&v["selection"]["brain"]=="sniper_learner"
            &&v["open_command_id"].as_str().is_some_and(|x|!x.is_empty())
            &&v["trade_owner"].as_str().is_some_and(|x|!x.is_empty())
            &&v["open_deal"].as_u64().is_some_and(|x|x>0)
            &&!source.is_empty()&&position_id>0&&closed>cutoff
            &&vol>0.&&vol<=100.
            &&(g+c+s+f-n).abs()<=0.000001*(1.+n.abs());
        if !valid {
            refused+=1;rejected_reasons.insert("wrong_broker_identity_or_unbalanced_costs");
            continue
        }
        if !receipts_reconcile(db,account,position_id,&v)?{
            refused+=1;
            rejected_reasons.insert("missing_or_inconsistent_original_broker_deals");
            continue
        }
        // The physical position can be attributed to another specialist as
        // well as system; count exact Position ID ONCE, never double count.
        if !positions.insert(position_id) {
            refused+=1;rejected_reasons.insert("duplicate_physical_position");
            continue
        }
        gross+=g;commission+=c;swap+=s;fee+=f;net+=n;volume+=vol;
        accepted+=1;
    }
    // An invalid/tampered record contaminates the entire sample set:
    // refuse sums rather than quietly dropping losing/invalid positions.
    if refused>0{
        let mut v=empty("broker_history_contains_inconsistent_rows");
        v["rejected_records"]=json!(refused);
        v["rejection_reasons"]=json!(rejected_reasons);
        return Ok(v)
    }
    if accepted==0{return Ok(empty("no_booked_demo_closes_after_model_freeze"))}
    let all_finite=[gross,commission,swap,fee,net,volume]
        .iter().all(|x|x.is_finite());
    if !all_finite{return Ok(empty("aggregated_booked_costs_overflow"))}
    Ok(json!({
        "schema":SCHEMA,"status":"separate_broker_cohort_observed",
        "source":"reconciled_mt5_demo_broker_deals",
        "frozen_artifact_id":artifact_id,
        "training_cutoff_ms":cutoff,
        "account_context":account,
        "physical_broker_trades":accepted,
        "closed_volume_lots_sum":volume,
        "booked_gross_account_currency":gross,
        "booked_commission_account_currency":commission,
        "booked_swap_account_currency":swap,
        "booked_fee_account_currency":fee,
        "net_demo_account_currency":net,
        "booked_costs_reconcile":true,
        "local_broker_deal_receipts_reconciled":true,
        "broker_connection_tested_here":false,
        "frozen_study_sent_orders":false,
        "matched_with_frozen_markouts":false,
        "comparison_with_five_second_markout_permitted":false,
        "independent_oos":false,
        "strategy_win_rate":Value::Null,
        "real_account_data":false,
        "warning":"Real MT5 Demo trades from the separate executing coordinator only. Not fills from the frozen five-second research study; holding period, selected entries, risk, lot sizes and realized costs differ. No inferred research PnL or performance attribution."
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::{Config,Telemetry,sniper::Engine};
    fn fixture()->(Connection,String,String,String,u64){
        let db=Connection::open_in_memory().unwrap();
        crate::coordinator_learning::Learner::new(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT,brain TEXT,
               kind TEXT,created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
             CREATE TABLE broker_deals(context TEXT,ticket INTEGER,position_id INTEGER,
               brain TEXT,payload TEXT,PRIMARY KEY(context,ticket));"
        ).unwrap();
        let config=Config::default();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:25,server:"Demo".into(),currency:"USD".into(),
            symbol:config.symbol.clone(),..Default::default()};
        let account=crate::coordinator_learning::context(&t);
        let mut model=Engine::default();
        model.set_models(account.clone(),Default::default());
        let cutoff=100_000;
        let checkpoint=json!({"status":"captured","account_context":account,
            "cutoff_ms":cutoff,"independent_oos":false,"broker_pnl":Value::Null,
            "broker_training_samples":0,"cached_policy_samples":0,
            "broker_target_samples":0,"cached_target_samples":0});
        let seal=crate::sniper_research_artifact::persist(
            &db,&t,&config,&model,&checkpoint,cutoff).unwrap();
        (db,account,seal.id,seal.sha256,cutoff)
    }
    fn payload(account:&str,position:u64,closed:u64)->Value{
        json!({"evidence_source":"mt5_demo_broker","context":account,
            "position_id":position,"brain":"sniper_learner","trading_mode":"sniper",
            "trade_owner":"system","selection":{"brain":"sniper_learner",
                "policy":"sniper-ticks-v1"},
            "open_command_id":format!("confirmed-core-open-{position}"),
            "open_deal":position*100+1,
            "closed_ms":closed,"closed_volume":0.1,
            "gross":1.5,"commission":-0.7,"swap":0.0,"fee":-0.1,
            "net":0.7,"net_per_lot":7.0})
    }
    fn insert(db:&Connection,account:&str,source:&str,position:u64,
              closed:u64,payload:Value){
        let command_id=payload["open_command_id"].as_str().unwrap();
        let open_ticket=payload["open_deal"].as_u64().unwrap();
        let cmd=json!({"id":command_id,"brain":"system","signal":{
            "side":1,"coordination":{"selection":{
                "brain":"sniper_learner","policy":"sniper-ticks-v1"}}}});
        let result=json!({"deal":open_ticket});
        db.execute(
            "INSERT OR IGNORE INTO broker_commands VALUES(?1,?2,'system','open',1,
             'observed',?3,?4)",
            params![command_id,account,cmd.to_string(),result.to_string()]
        ).unwrap();
        for (ticket,entry,volume,profit,commission,fee,stamp) in [
            (open_ticket,0,0.1,0.,-0.35,0.,closed.saturating_sub(1000)),
            (open_ticket+1,1,0.1,1.5,-0.35,-0.1,closed)
        ]{
            let deal=json!({"ticket":ticket,"position_id":position,
                "entry":entry,"volume":volume,"profit":profit,
                "commission":commission,"swap":0.0,"fee":fee,"time_msc":stamp});
            db.execute("INSERT OR IGNORE INTO broker_deals VALUES(?1,?2,?3,'system',?4)",
                params![account,ticket,position,deal.to_string()]).unwrap();
        }
        db.execute("INSERT INTO coordinator_broker_samples
           (source,position_id,context,brain,closed_ms,payload)
           VALUES(?1,?2,?3,'sniper_learner',?4,?5)",
           params![source,position,account,closed,payload.to_string()]).unwrap();
    }
    #[test]fn booked_costs_are_real_separate_cohort_not_frozen_executions(){
        let (db,account,id,hash,cutoff)=fixture();
        assert_eq!(inspect(&db,&account,&id,&hash).unwrap()["net_demo_account_currency"],Value::Null);
        let record=payload(&account,11,cutoff+20000);
        insert(&db,&account,"system/11",11,cutoff+20000,record);
        let got=inspect(&db,&account,&id,&hash).unwrap();
        assert_eq!(got["physical_broker_trades"],1);
        assert_eq!(got["booked_commission_account_currency"],-0.7);
        assert_eq!(got["net_demo_account_currency"],0.7);
        assert_eq!(got["matched_with_frozen_markouts"],false);
        assert_eq!(got["strategy_win_rate"],Value::Null);
        let mut another=account.clone();another.push('x');
        assert!(inspect(&db,&another,&id,&hash).is_err());
    }
    #[test]fn corrupt_booked_costs_or_duplicate_position_fail_closed(){
        let (db,account,id,hash,cutoff)=fixture();
        let stamp=cutoff+20_000;
        let a=payload(&account,11,stamp);
        insert(&db,&account,"sys1",11,stamp,a);
        insert(&db,&account,"sys2",11,stamp,payload(&account,11,stamp));
        let got=inspect(&db,&account,&id,&hash).unwrap();
        assert_eq!(got["status"],"insufficient_broker_evidence");
        assert!(got["net_demo_account_currency"].is_null());
    }
    #[test]fn missing_partial_or_changed_original_deal_refuses_sum(){
        let (db,account,id,hash,cutoff)=fixture();
        let close=cutoff+20_000;
        insert(&db,&account,"system-receipts",11,close,payload(&account,11,close));
        assert_eq!(inspect(&db,&account,&id,&hash).unwrap()["physical_broker_trades"],1);
        let exit=11*100+2;
        let raw:String=db.query_row(
            "SELECT payload FROM broker_deals WHERE context=?1 AND ticket=?2",
            params![&account,exit],|r|r.get(0)).unwrap();
        db.execute("DELETE FROM broker_deals WHERE context=?1 AND ticket=?2",
            params![&account,exit]).unwrap();
        let partial=inspect(&db,&account,&id,&hash).unwrap();
        assert_eq!(partial["status"],"insufficient_broker_evidence");
        assert!(partial["net_demo_account_currency"].is_null());
        db.execute("INSERT INTO broker_deals VALUES(?1,?2,11,'system',?3)",
            params![&account,exit,raw]).unwrap();
        assert_eq!(inspect(&db,&account,&id,&hash).unwrap()["physical_broker_trades"],1);
        let mut changed:Value=serde_json::from_str(&raw).unwrap();
        changed["commission"]=json!(-0.99);
        db.execute("UPDATE broker_deals SET payload=?3 WHERE context=?1 AND ticket=?2",
            params![&account,exit,changed.to_string()]).unwrap();
        let corrupted=inspect(&db,&account,&id,&hash).unwrap();
        assert_eq!(corrupted["status"],"insufficient_broker_evidence");
        assert_eq!(corrupted["rejection_reasons"][0],
            "missing_or_inconsistent_original_broker_deals");
        assert!(corrupted["net_demo_account_currency"].is_null());
    }
    #[test]fn raw_open_order_identity_cannot_be_relabelled_as_verified_demo(){
        let (db,account,id,hash,cutoff)=fixture();
        insert(&db,&account,"system-1",12,cutoff+30_000,
            payload(&account,12,cutoff+30_000));
        let cmd="confirmed-core-open-12";
        db.execute("UPDATE broker_commands SET result='{\"deal\":123456}'
                   WHERE context=?1 AND id=?2",params![&account,cmd]).unwrap();
        let changed=inspect(&db,&account,&id,&hash).unwrap();
        assert_eq!(changed["status"],"insufficient_broker_evidence");
        assert!(changed["net_demo_account_currency"].is_null());
    }
    #[test]fn no_broker_closes_at_training_cutoff_or_wrong_financial_identity(){
        let (db,account,id,hash,cutoff)=fixture();
        insert(&db,&account,"before",9,cutoff,payload(&account,9,cutoff));
        assert_eq!(inspect(&db,&account,&id,&hash).unwrap()["physical_broker_trades"],0);
        let mut tampered=payload(&account,10,cutoff+10_000);
        tampered["net"]=json!(10.0);
        insert(&db,&account,"tampered",10,cutoff+10_000,tampered);
        assert!(inspect(&db,&account,&id,&hash).unwrap()["net_demo_account_currency"].is_null());
    }
}
