//! P1-K5D: durable, tamper-evident *research-only* freeze checkpoint.
//! This stores frozen scores and cutoff once on operator STOP. It is never
//! queried or written by the Fast Pulse, MT5 send or learning update path.
//! SHA-256 detects accidental changes, not adversarial DB+hash replacement.
use anyhow::{ensure,Context,Result};
use braintrade_domain::{sniper::Engine,Config,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};

const SCHEMA:&str="p1k5d-sniper-freeze-artifact-v1";
const LIMIT:usize=8;
fn sha(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
pub struct Seal{pub id:String,pub sha256:String}
fn context(t:&Telemetry)->String{
    crate::coordinator_learning::context(t)
}
fn setup(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS sniper_freeze_research_artifacts(
          id TEXT PRIMARY KEY,
          account_context TEXT NOT NULL,
          cutoff_ms INTEGER NOT NULL,
          payload TEXT NOT NULL,
          sha256 TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS sniper_freeze_artifacts_account_time
          ON sniper_freeze_research_artifacts(account_context,cutoff_ms DESC,id DESC);
         CREATE TRIGGER IF NOT EXISTS sniper_artifact_no_update
          BEFORE UPDATE ON sniper_freeze_research_artifacts
          BEGIN SELECT RAISE(ABORT,'Frozen research artifacts cannot be updated'); END;
         CREATE TRIGGER IF NOT EXISTS sniper_artifact_no_delete
          BEFORE DELETE ON sniper_freeze_research_artifacts
          BEGIN SELECT RAISE(ABORT,'Frozen research artifacts cannot be deleted'); END;")?;
    Ok(())
}
/// Source-only artifact, NOT a complete tick tape or the strategy's broker
/// PnL. Config is hashed (not stored raw, to avoid private app settings).
/// All actual frozen model score fields are saved for later inspection.
pub fn persist(db:&Connection,t:&Telemetry,c:&Config,adaptive:&Engine,
    checkpoint:&Value,now:u64)->Result<Seal>{
    ensure!(t.connected&&t.demo&&t.account_trade_mode==Some(0)&&t.login>0
        &&!t.server.is_empty()&&!t.symbol.is_empty()&&!t.currency.is_empty(),
        "Research artifact requires a verified bound MT5 Demo account");
    ensure!(adaptive.training_account_matches(&context(t)),
        "Frozen model account differs from the selected Demo account");
    ensure!(checkpoint["status"]=="captured"
        &&checkpoint["account_context"]==context(t)
        &&checkpoint["cutoff_ms"].as_u64()==Some(now)
        &&checkpoint["independent_oos"]==false
        &&checkpoint["broker_pnl"].is_null(),
        "Unverified training checkpoint cannot be persisted");
    setup(db)?;
    let scores=adaptive.frozen_research_scores();
    ensure!(scores["context"]==context(t),
        "Serialized policy scores belong to a different account");
    let model_sha=sha(&serde_json::to_vec(&scores)?);
    let config_sha=sha(&serde_json::to_vec(c)?);
    let id=uuid::Uuid::new_v4().to_string();
    let payload=json!({
        "schema":SCHEMA,"id":id,"account_context":context(t),"cutoff_ms":now,
        "model_scores_sha256":model_sha,"config_sha256":config_sha,
        "model_scores":scores,"training_checkpoint":checkpoint,
        "forward_labels":Value::Null,
        "no_broker_orders":true,"independent_oos":false,
        "durability":"sqlite_local_append_only_checkpoint",
        "note":"Frozen research policy weights and cutoff. No tick tape, validated OOS, fills, execution costs or broker PnL. SHA256 detects edits unless both payload and hash are maliciously replaced."
    });
    let bytes=serde_json::to_vec(&payload)?;
    let digest=sha(&bytes);
    let text=String::from_utf8(bytes)?;
    db.execute("INSERT INTO sniper_freeze_research_artifacts
       (id,account_context,cutoff_ms,payload,sha256)
       VALUES(?1,?2,?3,?4,?5)",
       params![id,context(t),now,text,digest])?;
    Ok(Seal{id,sha256:digest})
}
/// Verifies stored JSON bytes, independent model-score hash, original
/// context and ID. On mismatch, caller withholds research stats.
pub fn verify(db:&Connection,account:&str,id:&str,expected_sha:Option<&str>)
    ->Result<Value>{
    ensure!(!account.is_empty()&&!id.is_empty(),"Missing research account/artifact");
    let row:Option<(String,u64,String,String)>=db.query_row(
        "SELECT account_context,cutoff_ms,payload,sha256
         FROM sniper_freeze_research_artifacts WHERE id=?1",
        [id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let (actual,cutoff,raw,stored)=row.context("Research checkpoint not found")?;
    ensure!(actual==account,"Research checkpoint account mismatch");
    ensure!(sha(raw.as_bytes())==stored,"Research checkpoint SHA-256 mismatch");
    if let Some(expected)=expected_sha{
        ensure!(expected==stored,"Research checkpoint changed since freeze");
    }
    let payload:Value=serde_json::from_str(&raw)?;
    ensure!(payload["schema"]==SCHEMA
        &&payload["id"]==id
        &&payload["account_context"]==account
        &&payload["cutoff_ms"].as_u64()==Some(cutoff)
        &&payload["no_broker_orders"]==true
        &&payload["independent_oos"]==false
        &&payload["forward_labels"].is_null(),"Research checkpoint envelope altered");
    let scores=&payload["model_scores"];
    let actual_model_sha=sha(&serde_json::to_vec(scores)?);
    ensure!(scores["context"]==account
        &&scores["schema"]=="p1k5d-sniper-policy-scores-v1"
        &&payload["model_scores_sha256"].as_str()==Some(actual_model_sha.as_str()),
        "Frozen model scores checksum mismatch");
    let training=&payload["training_checkpoint"];
    ensure!(training["status"]=="captured"
        &&training["account_context"]==account
        &&training["cutoff_ms"].as_u64()==Some(cutoff)
        &&training["independent_oos"]==false
        &&training["broker_pnl"].is_null(),
        "Training cutoff cannot be independently reconciled");
    Ok(json!({
        "schema":SCHEMA,"id":id,"cutoff_ms":cutoff,
        "sha256":stored,"config_sha256":payload["config_sha256"],
        "model_scores_sha256":payload["model_scores_sha256"],
        "broker_training_samples":training["broker_training_samples"],
        "integrity":"verified","stored_locally":true,
        "immutable_checkpoint":true,"no_broker_orders":true,
        "independent_oos":false,"broker_pnl":Value::Null,
        "note":"Locally checksum-verified frozen scores/cutoff. Not cryptographically signed, independently held, a tick tape or executed trading PnL."
    }))
}
/// Account-scoped recent artifacts stay discoverable *after a Core restart*.
/// Invalid/corrupted payloads are marked rather than silently disappearing.
pub fn recent(db:&Connection,account:&str)->Result<Value>{
    ensure!(!account.is_empty(),"Missing verified research account");
    let found:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sniper_freeze_research_artifacts')",
        [],|row|row.get(0))?;
    if !found{
        return Ok(json!({"schema":"p1k5d-research-artifact-history-v1",
            "account_context":account,"artifacts":[],
            "max_items":LIMIT,"no_broker_orders":true,"independent_oos":false}));
    }
    let mut stmt=db.prepare(
        "SELECT id FROM sniper_freeze_research_artifacts
         WHERE account_context=?1 ORDER BY cutoff_ms DESC,id DESC LIMIT 8")?;
    let ids=stmt.query_map([account],|r|r.get::<_,String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out=Vec::with_capacity(ids.len());
    for id in ids.into_iter().take(LIMIT){
        match verify(db,account,&id,None){
            Ok(summary)=>out.push(summary),
            Err(_)=>out.push(json!({"id":id,"integrity":"failed",
                "independent_oos":false,"broker_pnl":Value::Null})),
        }
    }
    Ok(json!({"schema":"p1k5d-research-artifact-history-v1",
        "account_context":account,"artifacts":out,
        "max_items":LIMIT,"no_broker_orders":true,"independent_oos":false}))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->(Telemetry,Config,Engine,Value){
        let mut config=Config::default();
        config.trading.mode=braintrade_domain::trading::TradingMode::Sniper;
        config.trading.sniper_brain=braintrade_domain::trading::SniperBrain::Learner;
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:17,server:"Demo".into(),symbol:config.symbol.clone(),
            currency:"USD".into(),..Default::default()};
        let mut model=Engine::default();
        let ctx=context(&t);
        model.set_models(ctx.clone(),Default::default());
        let checkpoint=json!({"status":"captured","account_context":ctx,
            "cutoff_ms":100000,"independent_oos":false,"broker_pnl":Value::Null,
            "broker_training_samples":0,"cached_policy_samples":0,
            "broker_target_samples":0,"cached_target_samples":0});
        (t,config,model,checkpoint)
    }
    #[test]fn write_read_reject_wrong_account_and_detect_payload_or_model_digest_tamper(){
        let db=Connection::open_in_memory().unwrap();
        let (t,c,e,ch)=fixture();
        let seal=persist(&db,&t,&c,&e,&ch,100000).unwrap();
        let good=verify(&db,&context(&t),&seal.id,Some(&seal.sha256)).unwrap();
        assert_eq!(good["integrity"],"verified");
        assert_eq!(good["broker_pnl"],Value::Null);
        assert_eq!(good["independent_oos"],false);
        assert_eq!(recent(&db,&context(&t)).unwrap()["artifacts"][0]["id"],seal.id);
        // Application-level append-only guard prevents accidental edits.
        assert!(db.execute(
            "UPDATE sniper_freeze_research_artifacts SET sha256='bad' WHERE id=?1",
            [&seal.id]).is_err());
        assert!(db.execute(
            "DELETE FROM sniper_freeze_research_artifacts WHERE id=?1",
            [&seal.id]).is_err());
        let mut foreign=t.clone();foreign.login=99;
        assert!(verify(&db,&context(&foreign),&seal.id,None).is_err());
        assert!(recent(&db,&context(&foreign)).unwrap()["artifacts"].as_array().unwrap().is_empty());
        // A privileged editor can DROP triggers; checksum still detects
        // corruption if it doesn't also intentionally replace every hash.
        db.execute_batch("DROP TRIGGER sniper_artifact_no_update;").unwrap();
        db.execute("UPDATE sniper_freeze_research_artifacts SET payload=payload||' ' WHERE id=?1",
            [&seal.id]).unwrap();
        assert!(verify(&db,&context(&t),&seal.id,None).is_err());
        assert_eq!(recent(&db,&context(&t)).unwrap()["artifacts"][0]["integrity"],"failed");
    }
    #[test]fn durable_checkpoint_survives_db_reopen_no_real_or_future_cutoff(){
        let tmp=std::env::temp_dir().join(format!("sniper-artifact-{}.sqlite",uuid::Uuid::new_v4()));
        let (t,c,e,checkpoint)=fixture();
        let (id,hash) = {
            let db=Connection::open(&tmp).unwrap();
            let seal=persist(&db,&t,&c,&e,&checkpoint,100000).unwrap();
            (seal.id,seal.sha256)
        };
        {
            let db=Connection::open(&tmp).unwrap();
            assert_eq!(verify(&db,&context(&t),&id,Some(&hash)).unwrap()["integrity"],"verified");
        }
        let _=std::fs::remove_file(tmp);
        let db=Connection::open_in_memory().unwrap();
        let mut real=t.clone();real.demo=false;real.account_trade_mode=Some(2);
        assert!(persist(&db,&real,&c,&e,&checkpoint,100000).is_err());
        assert!(persist(&db,&t,&c,&e,&checkpoint,100001).is_err());
    }
}