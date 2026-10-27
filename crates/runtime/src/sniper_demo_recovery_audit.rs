//! P1-K5J: read-only, account-bound broker intent recovery diagnostic.
//! A pending/unknown receipt is NOT evidence that MT5 rejected an order.
//! This module never retries, releases, resends, cancels, or acknowledges an
//! order. Its absence of local pending rows never proves the broker flat.
use anyhow::{ensure,Result};
use braintrade_domain::Telemetry;
use rusqlite::{params,Connection};
use serde_json::{json,Value};
use std::collections::BTreeMap;

const SCHEMA:&str="p1k5j-demo-recovery-audit-v1";
const MAX_ROWS:u64=1000;
fn has_table(db:&Connection,name:&str)->Result<bool>{
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],|r|r.get(0))?)
}
fn unavailable(reason:&str)->Value{
    json!({"schema":SCHEMA,"status":"not_verified","reason":reason,
        "pending_intents":Value::Null,"unresolved_new_open_risk":true,
        "new_orders_authorized":false,
        "requires_live_broker_reconciliation":true,
        "automatic_retry_or_release":false,
        "broker_connected_verified_here":false,"independent_oos":false})
}
/// Only a currently bound MT5 Demo telemetry may view this report. The
/// frozen model artifact is used for account provenance, not as evidence the
/// frozen study executed a broker trade (it never submits orders).
pub fn inspect(db:&Connection,t:&Telemetry,artifact_id:&str,hash:&str)->Result<Value>{
    ensure!(t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&t.login>0&&!t.server.is_empty()&&!t.symbol.is_empty()
        &&!t.currency.is_empty(),"Verified current MT5 Demo required");
    let key=crate::coordinator_learning::context(t);
    let proof=crate::sniper_research_artifact::verify(
        db,&key,artifact_id,Some(hash))?;
    ensure!(proof["integrity"]=="verified","Frozen account evidence invalid");
    if !has_table(db,"broker_commands")?{
        return Ok(unavailable("missing_persistent_broker_command_journal"))
    }
    let count:u64=db.query_row(
        "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND
           state IN ('queued','dispatched','unknown','placed','filled','partial')",
        [&key],|r|r.get(0))?;
    let global_count:u64=db.query_row(
        "SELECT COUNT(*) FROM broker_commands WHERE
           state IN ('queued','dispatched','unknown','placed','filled','partial')",
        [],|r|r.get(0))?;
    if count>MAX_ROWS ||global_count>MAX_ROWS{
        return Ok(unavailable("unresolved_broker_intent_count_exceeds_bounded_audit"))
    }
    let mut stmt=db.prepare(
        "SELECT id,kind,state,created_ms,payload,result
           FROM broker_commands WHERE context=?1 AND
           state IN ('queued','dispatched','unknown','placed','filled','partial')
           ORDER BY created_ms,id LIMIT 1000")?;
    let rows=stmt.query_map([&key],|r|Ok(
        (r.get::<_,String>(0)?,r.get::<_,String>(1)?,
         r.get::<_,String>(2)?,r.get::<_,u64>(3)?,
         r.get::<_,String>(4)?,r.get::<_,String>(5)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut states:BTreeMap<String,u64>=BTreeMap::new();
    let mut unresolved_ambiguous=0u64;
    let mut locally_queued=0u64;
    let mut unknown_broker_outcome=0u64;
    let mut invalid=0u64;
    for (id,kind,state,created,payload,result) in rows {
        let p=serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null);
        let r=serde_json::from_str::<Value>(&result).unwrap_or(Value::Null);
        let valid=created>0&&!id.is_empty()
            &&matches!(kind.as_str(),"open"|"close"|"modify"|"cancel")
            &&p["id"]==id&&p["context"]==key
            &&p["kind"]==kind&&p["created_ms"].as_u64()==Some(created)
            &&r.is_object();
        if !valid{invalid+=1;continue}
        *states.entry(state.clone()).or_default()+=1;
        if state=="queued"{
            locally_queued+=1;
        }else{
            unresolved_ambiguous+=1;
            if state=="unknown" ||state=="dispatched" ||state=="placed"{
                unknown_broker_outcome+=1;
            }
        }
    }
    if invalid>0{
        let mut v=unavailable("unreconciled_broker_command_row_inconsistent");
        v["invalid_rows"]=json!(invalid);
        return Ok(v)
    }
    let foreign_pending=global_count.saturating_sub(count);
    Ok(json!({
        "schema":SCHEMA,
        "status":if global_count>0{"blocked_pending_broker_reconciliation"}
                 else{"local_journal_has_no_unresolved_intents"},
        "artifact_id":artifact_id,"account_context":key,
        "pending_intents":count,
        "locally_queued":locally_queued,
        "unresolved_sent_or_partially_filled":unresolved_ambiguous,
        "unknown_broker_outcome":unknown_broker_outcome,
        "states":states,
        // Existing legacy Core uses a global pending gate (including other
        // accounts). Preserve that visibility without leaking identity.
        "other_account_pending_intents":foreign_pending,
        "systemwide_pending_intents":global_count,
        "unresolved_new_open_risk":global_count>0,
        // Local absence of a row CANNOT verify broker-side flat/zero orders.
        "new_orders_authorized":false,
        "requires_live_broker_reconciliation":true,
        "automatic_retry_or_release":false,
        "broker_connected_verified_here":false,
        "frozen_study_sent_orders":false,
        "independent_oos":false,
        "note":"Persistent account-bound command journal only. A dispatched/unknown/filled/partial intent may be live at MT5. No resubmission permitted based on this report. Zero rows is not terminal/broker-side proof of a flat position."
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::{Config,Telemetry,sniper::Engine};
    fn fixture(path:Option<&std::path::Path>)->(Connection,Telemetry,String,String){
        let db=match path{Some(p)=>Connection::open(p).unwrap(),
                         None=>Connection::open_in_memory().unwrap()};
        db.execute_batch(
            "CREATE TABLE broker_commands(
                 id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,
                 created_ms INTEGER,state TEXT,payload TEXT,result TEXT
             );").unwrap();
        let c=Config::default();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:34,server:"BrokerDemo".into(),symbol:c.symbol.clone(),
            currency:"USD".into(),..Default::default()};
        let ctx=crate::coordinator_learning::context(&t);
        let mut model=Engine::default();
        model.set_models(ctx.clone(),Default::default());
        let checkpoint=json!({"status":"captured","account_context":ctx,
            "cutoff_ms":100_000,"broker_training_samples":0,"cached_policy_samples":0,
            "broker_target_samples":0,"cached_target_samples":0,
            "independent_oos":false,"broker_pnl":Value::Null});
        let seal=crate::sniper_research_artifact::persist(
            &db,&t,&c,&model,&checkpoint,100_000).unwrap();
        (db,t,seal.id,seal.sha256)
    }
    fn put(db:&Connection,t:&Telemetry,id:&str,state:&str,ts:u64){
        let ctx=crate::coordinator_learning::context(t);
        let p=json!({"id":id,"context":ctx,"brain":"system",
            "kind":"open","created_ms":ts,
            "lots":0.01,"expires_ms":ts+3000});
        db.execute("INSERT INTO broker_commands
              VALUES(?1,?2,'system','open',?3,?4,?5,'{}')",
              params![id,ctx,ts,state,p.to_string()]).unwrap();
    }
    #[test]fn unknown_dispatched_and_partial_never_authorize_retry(){
        let (db,t,id,hash)=fixture(None);
        put(&db,&t,"a","queued",110_000);
        put(&db,&t,"b","dispatched",111_000);
        put(&db,&t,"c","unknown",112_000);
        put(&db,&t,"d","partial",113_000);
        let v=inspect(&db,&t,&id,&hash).unwrap();
        assert_eq!(v["pending_intents"],4);
        assert_eq!(v["locally_queued"],1);
        assert_eq!(v["unresolved_sent_or_partially_filled"],3);
        assert_eq!(v["unknown_broker_outcome"],2);
        assert_eq!(v["new_orders_authorized"],false);
        assert_eq!(v["automatic_retry_or_release"],false);
        assert_eq!(v["requires_live_broker_reconciliation"],true);
    }
    #[test]fn account_isolation_zero_rows_and_inconsistent_payload_fail_closed(){
        let (db,t,id,hash)=fixture(None);
        let mut foreign=t.clone();foreign.login=35;
        put(&db,&foreign,"foreign","unknown",110_000);
        let v=inspect(&db,&t,&id,&hash).unwrap();
        assert_eq!(v["pending_intents"],0);
        assert_eq!(v["other_account_pending_intents"],1);
        assert_eq!(v["unresolved_new_open_risk"],true);
        assert_eq!(v["new_orders_authorized"],false);
        assert!(inspect(&db,&foreign,&id,&hash).is_err());
        put(&db,&t,"bad","unknown",110_000);
        db.execute("UPDATE broker_commands SET payload='{}' WHERE id='bad'",[])
            .unwrap();
        let not=inspect(&db,&t,&id,&hash).unwrap();
        assert_eq!(not["status"],"not_verified");
        assert!(not["pending_intents"].is_null());
    }
    #[test]fn unresolved_intents_are_not_lost_on_sqlite_reopen(){
        let path=std::env::temp_dir().join(format!(
            "v25-recovery-{}.sqlite",uuid::Uuid::new_v4()));
        let (t,id,hash)={
            let (db,t,id,hash)=fixture(Some(&path));
            put(&db,&t,"restart","dispatched",110_000);
            (t,id,hash)
        };
        let db=Connection::open(&path).unwrap();
        let v=inspect(&db,&t,&id,&hash).unwrap();
        assert_eq!(v["states"]["dispatched"],1);
        assert_eq!(v["new_orders_authorized"],false);
        drop(db);
        let _=std::fs::remove_file(path);
    }
    #[test]fn real_and_unbound_telemetry_are_not_privileged(){
        let (db,t,id,hash)=fixture(None);
        let mut real=t.clone();real.demo=false;real.account_trade_mode=Some(2);
        assert!(inspect(&db,&real,&id,&hash).is_err());
        let mut offline=t.clone();offline.connected=false;
        assert!(inspect(&db,&offline,&id,&hash).is_err());
    }
}