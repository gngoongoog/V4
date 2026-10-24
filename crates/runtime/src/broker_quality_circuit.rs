//! Independent Rust broker-entry circuit for the authoritative Core journal.
//! Mirrors Gateway's REAL default: one unresolved broker submission OR three
//! broker-confirmed rejects in 120 s. Demo remains explicitly advisory here.
//! Every query is account+symbol scoped. Fast Pulse only reads cached Snapshot;
//! before-send authorization re-reads the durable Core journal.
//! Close intents and reconciliation must never be blocked by this entry circuit.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

const REJECT_WINDOW_MS:u64=120_000;
const REJECT_THRESHOLD:u32=3;
#[derive(Debug,Clone,Default)]
pub(crate) struct Snapshot {
    pub context:String,
    pub blocked:bool,
    pub reason:&'static str,
    pub unknown_count:u32,
    pub recent_broker_rejections:u32,
    pub evaluated_ms:u64,
    pub valid:bool,
}
impl Snapshot {
    pub(crate) fn unavailable(context:String,now:u64)->Self{
        Self{context,blocked:true,reason:"quality_evidence_unavailable",
             evaluated_ms:now,..Self::default()}
    }
    pub(crate) fn entry_allowed(&self,t:&Telemetry,live:bool)->bool{
        !live||(self.valid&&!self.blocked&&self.context==context(t)
            && self.evaluated_ms>0)
    }
    pub(crate) fn as_json(&self,live:bool)->Value{
        json!({"schema":"p0c-core-quality-v1",
              "policy":if live {"required_real"}else{"advisory_demo"},
              "state":if !self.valid {"unavailable"}else if !live {"demo_advisory"}
                   else if self.blocked {"blocked"}else{"ready"},
              "entry_allowed":self.valid&&(!live||!self.blocked),
              "reason":self.reason,
              "unknown_open_intents":self.unknown_count,
              "recent_broker_rejections":self.recent_broker_rejections,
              "reject_threshold":REJECT_THRESHOLD,
              "window_ms":REJECT_WINDOW_MS,
              "observed_ms":self.evaluated_ms,
              "never_blocks_close_or_reconcile":true,
              "read_only":true})
    }
}
pub(crate) fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap_or_default()
}
pub(crate) fn inspect(db:&Connection,t:&Telemetry,now:u64)->Result<Snapshot>{
    let key=context(t);
    let mut snapshot=Snapshot{context:key.clone(),evaluated_ms:now,..Snapshot::default()};
    if !t.connected||t.login==0||t.server.is_empty()||t.symbol.is_empty()||
        t.currency.is_empty()||!matches!(t.account_trade_mode,Some(0)|Some(2))||
        (t.demo && t.account_trade_mode!=Some(0))||
        (!t.demo && t.account_trade_mode!=Some(2)){
        snapshot.blocked=true;
        snapshot.reason="account_identity_unverified";
        return Ok(snapshot);
    }
    // Unknown is unresolved until a broker-confirmed terminal state is saved.
    // Never treat a two-minute timeout as reconciliation.
    let unknown:i64=db.query_row(
        "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='open' AND state='unknown'",
        [&key],|r|r.get(0))?;
    snapshot.unknown_count=unknown.clamp(0,i64::from(u32::MAX)) as u32;
    let since=now.saturating_sub(REJECT_WINDOW_MS);
    let mut stmt=db.prepare(
        "SELECT result FROM broker_commands WHERE context=?1 AND kind='open' AND state='rejected' AND created_ms>=?2"
    )?;
    let values=stmt.query_map(params![key,since],|r|r.get::<_,String>(0))?;
    for value in values {
        // Invalid evidence never counts as a confirmed broker rejection.
        // It cannot make a previously unresolved order resolved.
        if let Ok(v)=serde_json::from_str::<Value>(&value?) {
            if v.get("retcode").and_then(Value::as_i64).is_some(){
                snapshot.recent_broker_rejections=snapshot.recent_broker_rejections.saturating_add(1);
            }
        }
    }
    snapshot.valid=true;
    snapshot.reason=if snapshot.unknown_count>0 {"unresolved_broker_submission"}
        else if snapshot.recent_broker_rejections>=REJECT_THRESHOLD {"repeated_broker_rejections"}
        else {"within_quality_limits"};
    snapshot.blocked=snapshot.unknown_count>0||
        snapshot.recent_broker_rejections>=REJECT_THRESHOLD;
    Ok(snapshot)
}

#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->(Connection,Telemetry){
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,
          brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,
          state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');").unwrap();
        let t=Telemetry{connected:true,demo:false,account_trade_mode:Some(2),login:51,
            server:"RealBroker".into(),symbol:"XAUUSD".into(),currency:"USD".into(),
            equity:50.,..Default::default()};
        (db,t)
    }
    fn insert(db:&Connection,t:&Telemetry,id:&str,kind:&str,state:&str,ms:u64,result:&str){
        db.execute("INSERT INTO broker_commands VALUES(?1,?2,'system',?3,?4,?5,'{}',?6)",
           params![id,context(t),kind,ms,state,result]).unwrap();
    }
    #[test]fn blocks_unknown_until_reconciled_even_after_two_minutes_and_account_switch(){
        let (db,t)=fixture();
        insert(&db,&t,"uncertain","open","unknown",1,r#"{"reason":"timeout"}"#);
        let s=inspect(&db,&t,400_000).unwrap();
        assert!(s.blocked);
        assert_eq!(s.unknown_count,1);
        assert!(!s.entry_allowed(&t,true));
        assert_eq!(s.reason,"unresolved_broker_submission");
        assert!(s.entry_allowed(&t,false),"Demo's optional policy must remain advisory");
        let mut other=t.clone();other.login=52;
        assert!(inspect(&db,&other,400_000).unwrap().entry_allowed(&other,true));
        assert!(!s.entry_allowed(&other,true),"cached account cannot authorize a different login");
        db.execute("UPDATE broker_commands SET state='observed' WHERE id='uncertain'",[]).unwrap();
        assert!(inspect(&db,&t,400_001).unwrap().entry_allowed(&t,true));
    }
    #[test]fn three_broker_retcodes_trigger_window_but_local_rejects_do_not(){
        let (db,t)=fixture();
        insert(&db,&t,"a","open","rejected",100_001,r#"{"retcode":10019}"#);
        insert(&db,&t,"b","open","rejected",100_002,r#"{"retcode":10014}"#);
        insert(&db,&t,"local","open","rejected",100_003,r#"{"reason":"risk budget"}"#);
        insert(&db,&t,"close","close","rejected",100_003,r#"{"retcode":10016}"#);
        assert_eq!(inspect(&db,&t,100_010).unwrap().recent_broker_rejections,2);
        insert(&db,&t,"c","open","rejected",100_004,r#"{"retcode":10016}"#);
        let blocked=inspect(&db,&t,100_010).unwrap();
        assert_eq!(blocked.recent_broker_rejections,3);
        assert_eq!(blocked.reason,"repeated_broker_rejections");
        assert!(!blocked.entry_allowed(&t,true));
        assert!(inspect(&db,&t,240_010).unwrap().entry_allowed(&t,true));
        let mut other=t.clone();other.symbol="EURUSD".into();
        assert!(inspect(&db,&other,100_010).unwrap().entry_allowed(&other,true));
    }
    #[test]fn corrupt_or_missing_evidence_fails_closed_but_never_mutates_broker_journal(){
        let (db,t)=fixture();
        insert(&db,&t,"bad","open","rejected",50,r#"{invalid"#);
        assert!(inspect(&db,&t,100).unwrap().entry_allowed(&t,true),
           "corrupt local rejection cannot masquerade as confirmed broker retcode");
        let count:i64=db.query_row("SELECT COUNT(*) FROM broker_commands",[],|r|r.get(0)).unwrap();
        assert_eq!(count,1);
        let snapshot=Snapshot::unavailable(context(&t),100);
        assert!(!snapshot.entry_allowed(&t,true));
        let mut wrong=t.clone();wrong.account_trade_mode=Some(0);
        assert!(!inspect(&db,&wrong,100).unwrap().entry_allowed(&wrong,true));
        db.execute("DROP TABLE broker_commands",[]).unwrap_err();
        db.execute_batch("DROP TABLE broker_commands").unwrap();
        assert!(inspect(&db,&t,100).is_err());
    }
    #[test]fn snapshot_report_is_readonly_and_does_not_claim_order_authorization(){
        let (db,t)=fixture();
        let s=inspect(&db,&t,100).unwrap();
        let v=s.as_json(true);
        assert_eq!(v["state"],"ready");
        assert_eq!(v["recent_broker_rejections"],0);
        assert_eq!(v["never_blocks_close_or_reconcile"],true);
        assert_eq!(v["read_only"],true);
        assert_eq!(v["policy"],"required_real");
        assert_eq!(s.as_json(false)["policy"],"advisory_demo");
    }
}
