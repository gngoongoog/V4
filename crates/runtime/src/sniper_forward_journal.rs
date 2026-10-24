//! P1-K5E: append-only evidence journal for frozen prospective signals.
//! On Fast Pulse Tick: only bounded in-memory events. On slower full MT5
//! reconciliation: transactional short batch INSERT; failure never blocks
//! broker supervision and unpublished events stay in memory. Crash between
//! pulses and the next reconciliation can lose up to one buffer window.
use anyhow::{ensure,Context,Result};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};

const EVENT_SCHEMA:&str="p1k5e-frozen-future-event-v1";
const MAX_READ:usize=20_000;
fn digest(raw:&[u8])->String{format!("{:x}",Sha256::digest(raw))}
fn exists(db:&Connection)->Result<bool>{
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sniper_future_research_events')",
        [],|r|r.get(0))?)
}
fn setup(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS sniper_future_research_events(
          artifact_id TEXT NOT NULL,
          account_context TEXT NOT NULL,
          event_seq INTEGER NOT NULL,
          payload TEXT NOT NULL,
          sha256 TEXT NOT NULL,
          PRIMARY KEY(artifact_id,event_seq),
          FOREIGN KEY(artifact_id) REFERENCES sniper_freeze_research_artifacts(id)
        );
        CREATE INDEX IF NOT EXISTS sniper_future_research_account
          ON sniper_future_research_events(account_context,artifact_id,event_seq);
        CREATE TRIGGER IF NOT EXISTS sniper_future_events_no_update
          BEFORE UPDATE ON sniper_future_research_events
          BEGIN SELECT RAISE(ABORT,'Frozen future events are append only'); END;
        CREATE TRIGGER IF NOT EXISTS sniper_future_events_no_delete
          BEFORE DELETE ON sniper_future_research_events
          BEGIN SELECT RAISE(ABORT,'Frozen future events are append only'); END;")?;
    Ok(())
}
/// Atomic whole-batch commit: duplicate event sequences are safe to retry
/// only if byte-for-byte identical; conflicting retries fail closed.
pub fn append(db:&Connection,account:&str,artifact_id:&str,
    artifact_hash:&str,events:&[Value])->Result<usize>{
    if events.is_empty(){return Ok(0)}
    ensure!(events.len()<=256,"Research event batch exceeds buffered budget");
    let proof=crate::sniper_research_artifact::verify(
        db,account,artifact_id,Some(artifact_hash))?;
    ensure!(proof["integrity"]=="verified","Frozen research archive corrupted");
    let cutoff=proof["cutoff_ms"].as_u64().context("Missing freeze cutoff")?;
    let mut last=0u64;
    for (i,e) in events.iter().enumerate(){
        let seq=e["sequence"].as_u64().context("Missing research event sequence")?;
        ensure!(seq>0&&(i==0||seq>last),"Unordered prospective event sequence");
        ensure!(e["schema"]==EVENT_SCHEMA,"Unknown research event schema");
        let kind=e["kind"].as_str().context("Missing research event kind")?;
        ensure!(matches!(kind,"candidate"|"matured"|"censored"),
            "Unsupported research event");
        let opened=e["opened_broker_ms"].as_u64().context("Missing entry broker time")?;
        ensure!(opened>cutoff,"Training-time quote in frozen future journal");
        ensure!(matches!(e["side"].as_i64(),Some(-1|1)),"Invalid frozen research direction");
        if kind=="matured"{
            ensure!(e["closed_broker_ms"].as_u64().is_some_and(|v|v>=opened.saturating_add(5_000)),
                "Research label leaked before the fixed horizon");
            ensure!(matches!(e["outcome"].as_str(),Some("correct"|"opposite"|"flat")),
                "Unknown future directional class");
        }else if kind=="candidate"{
            ensure!(e["entry_midquote"].as_f64().is_some_and(f64::is_finite),
                "Nonfinite research source");
        }
        last=seq;
    }
    setup(db)?;
    let tx=db.unchecked_transaction()?;
    for e in events {
        let raw=serde_json::to_string(e)?;
        let sha=digest(raw.as_bytes());
        let seq=e["sequence"].as_u64().context("Missing sequence")?;
        let old:Option<(String,String)>=tx.query_row(
            "SELECT payload,sha256 FROM sniper_future_research_events
             WHERE artifact_id=?1 AND event_seq=?2",
             params![artifact_id,seq],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((existing,hash))=old{
            ensure!(existing==raw&&hash==sha,
                "Conflicting repeated research event sequence");
            continue;
        }
        tx.execute(
            "INSERT INTO sniper_future_research_events
             (artifact_id,account_context,event_seq,payload,sha256)
             VALUES(?1,?2,?3,?4,?5)",
            params![artifact_id,account,seq,raw,sha])?;
    }
    tx.commit()?;
    Ok(events.len())
}
/// Verify complete event order and one-to-one candidate/terminal pairing.
/// A missing/unknown event, tampered digest, unpaired label or gap marks
/// INTEGRITY_WARNING, not a favorable outcome. Until STOP, unresolved
/// candidate remains pending and post-crash results cannot be assumed.
pub fn report(db:&Connection,account:&str,artifact_id:&str,
    artifact_sha:&str)->Result<Value>{
    let proof=crate::sniper_research_artifact::verify(
        db,account,artifact_id,Some(artifact_sha))?;
    if !exists(db)?{
        return Ok(json!({"schema":"p1k5e-forward-research-journal-v1",
          "artifact_id":artifact_id,"integrity":"empty",
          "recorded_events":0,"resolved_labels":0,
          "complete_trace":false,"historical_oos":false,
          "broker_pnl":Value::Null,"no_broker_orders":true}));
    }
    let count:i64=db.query_row(
        "SELECT COUNT(*) FROM sniper_future_research_events
         WHERE artifact_id=?1 AND account_context=?2",
        params![artifact_id,account],|r|r.get(0))?;
    ensure!(count>=0,"Invalid research journal count");
    let mut stmt=db.prepare(
        "SELECT event_seq,payload,sha256 FROM sniper_future_research_events
         WHERE artifact_id=?1 AND account_context=?2
         ORDER BY event_seq LIMIT ?3")?;
    let rows=stmt.query_map(params![artifact_id,account,(MAX_READ+1) as i64],
        |r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seq=0u64;
    let mut integrity=true;
    let mut count_candidate=0u64;
    let mut resolved=0u64;
    let mut censored=0u64;
    let mut correct=0u64;
    let mut opposite=0u64;
    let mut flat=0u64;
    let mut pending:Option<(u64,i64)>=None;
    for (n,raw,sha) in &rows {
        let v:Value=serde_json::from_str(raw).unwrap_or(Value::Null);
        if *n!=seq+1||v["sequence"].as_u64()!=Some(*n)
            ||digest(raw.as_bytes())!=*sha||v["schema"]!=EVENT_SCHEMA{
            integrity=false;break;
        }
        seq=*n;
        let opened=v["opened_broker_ms"].as_u64();
        let side=v["side"].as_i64();
        if !opened.is_some_and(|v|v>proof["cutoff_ms"].as_u64().unwrap_or(u64::MAX))
            ||!matches!(side,Some(-1|1)){integrity=false;break}
        match v["kind"].as_str(){
            Some("candidate")=>{
                if pending.is_some(){integrity=false;break}
                pending=opened.zip(side);count_candidate+=1;
            }
            Some("matured")=>{
                if pending!=opened.zip(side)
                    ||!v["closed_broker_ms"].as_u64()
                        .is_some_and(|ts|ts>=opened.unwrap_or(u64::MAX).saturating_add(5_000)){
                    integrity=false;break;
                }
                pending=None;resolved+=1;
                match v["outcome"].as_str(){
                    Some("correct")=>correct+=1,
                    Some("opposite")=>opposite+=1,
                    Some("flat")=>flat+=1,
                    _=>{integrity=false;break}
                }
            }
            Some("censored")=>{
                if pending!=opened.zip(side){integrity=false;break}
                pending=None;censored+=1;
            }
            _=>{integrity=false;break}
        }
    }
    let truncated=rows.len()>MAX_READ ||count as usize>MAX_READ;
    let safe=integrity&&!truncated;
    Ok(json!({"schema":"p1k5e-forward-research-journal-v1",
        "artifact_id":artifact_id,
        "integrity":if safe{"verified_local"}else{"warning"},
        "recorded_events":count,"journal_cap_for_read":MAX_READ,
        "truncated":truncated,"resolved_labels":if safe{Some(resolved)}else{None},
        "direction_correct":if safe{Some(correct)}else{None},
        "direction_opposite":if safe{Some(opposite)}else{None},
        "direction_flat":if safe{Some(flat)}else{None},
        "censored":if safe{Some(censored)}else{None},
        "opened_candidates":if safe{Some(count_candidate)}else{None},
        "pending_unresolved":if safe{Some(u64::from(pending.is_some()))}else{None},
        "verified_local_event_pairing":safe&&pending.is_none()
            &&resolved+censored==count_candidate&&count>0,
        // A Core crash may lose a complete unflushed in-memory session;
        // an archive cannot assert complete market-tick coverage.
        "complete_trace":false,
        "historical_oos":false,"independent_oos":false,"broker_pnl":Value::Null,
        "no_broker_orders":true,"no_learning_effect":true,
        "note":"Bounded future Native/Poll event journal tied to archived frozen weights. A crash can lose unflushed memory events; complete_trace only describes local event pairing, not exhaustive market ticks or independent OOS."}))
}
#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::{Config,Telemetry,sniper::Engine};
    fn fixture()->(Connection,Telemetry,Config,Engine,Value,String,String){
        let db=Connection::open_in_memory().unwrap();
        let c=Config::default();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:99,server:"Demo".into(),symbol:c.symbol.clone(),
            currency:"USD".into(),..Default::default()};
        let mut e=Engine::default();
        let ctx=crate::coordinator_learning::context(&t);
        e.set_models(ctx,Default::default());
        let proof=json!({"status":"captured",
            "account_context":crate::coordinator_learning::context(&t),
            "cutoff_ms":100_000,"broker_training_samples":0,"cached_policy_samples":0,
            "broker_target_samples":0,"cached_target_samples":0,
            "independent_oos":false,"broker_pnl":Value::Null});
        let seal=crate::sniper_research_artifact::persist(
            &db,&t,&c,&e,&proof,100_000).unwrap();
        let result=(seal.id,seal.sha256);
        (db,t,c,e,proof,result.0,result.1)
    }
    fn candidate(seq:u64,ts:u64)->Value{
        json!({"schema":EVENT_SCHEMA,"sequence":seq,"kind":"candidate",
            "opened_broker_ms":ts,"side":1,"entry_midquote":2500.,"tick_size":0.01})
    }
    fn matured(seq:u64,ts:u64)->Value{
        json!({"schema":EVENT_SCHEMA,"sequence":seq,"kind":"matured",
            "opened_broker_ms":ts,"closed_broker_ms":ts+5000,"side":1,
            "outcome":"correct","horizon_ms":5000})
    }
    #[test]fn future_only_idempotent_and_account_bound_with_recovery(){
        let (db,t,_,_,_,id,hash)=fixture();let ctx=crate::coordinator_learning::context(&t);
        let entries=[candidate(1,101_000),matured(2,101_000)];
        assert_eq!(append(&db,&ctx,&id,&hash,&entries).unwrap(),2);
        assert_eq!(append(&db,&ctx,&id,&hash,&entries).unwrap(),2);
        let good=report(&db,&ctx,&id,&hash).unwrap();
        assert_eq!(good["integrity"],"verified_local");
        assert_eq!(good["resolved_labels"],1);
        assert_eq!(good["verified_local_event_pairing"],true);
        assert_eq!(good["complete_trace"],false);
        assert_eq!(good["broker_pnl"],Value::Null);
        let other=serde_json::to_string(&("Demo",100,"XAUUSD","USD")).unwrap();
        assert!(report(&db,&other,&id,&hash).is_err());
        let conflicting=candidate(1,101_100);
        assert!(append(&db,&ctx,&id,&hash,&[conflicting]).is_err());
    }
    #[test]fn missing_censored_label_and_training_leak_fail_closed(){
        let (db,t,_,_,_,id,hash)=fixture();let ctx=crate::coordinator_learning::context(&t);
        assert!(append(&db,&ctx,&id,&hash,&[candidate(1,99_999)]).is_err());
        append(&db,&ctx,&id,&hash,&[candidate(1,101_000)]).unwrap();
        append(&db,&ctx,&id,&hash,&[matured(3,101_000)]).unwrap();
        assert_eq!(report(&db,&ctx,&id,&hash).unwrap()["integrity"],"warning");
        let row:Value=report(&db,&ctx,&id,&hash).unwrap();
        assert_eq!(row["resolved_labels"],Value::Null);
    }
    #[test]fn digest_tamper_is_not_a_verified_success(){
        let (db,t,_,_,_,id,hash)=fixture();let ctx=crate::coordinator_learning::context(&t);
        append(&db,&ctx,&id,&hash,&[candidate(1,101_000)]).unwrap();
        assert!(db.execute("UPDATE sniper_future_research_events SET sha256='bad' WHERE artifact_id=?1",
            [&id]).is_err());
        db.execute_batch("DROP TRIGGER sniper_future_events_no_update").unwrap();
        db.execute("UPDATE sniper_future_research_events SET sha256='bad' WHERE artifact_id=?1",
            [&id]).unwrap();
        assert_eq!(report(&db,&ctx,&id,&hash).unwrap()["integrity"],"warning");
    }
}