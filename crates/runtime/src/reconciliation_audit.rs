//! Read-only, account-aware explanation for execution mutation blocks.
//! Never settles, deletes, retries, or changes an existing broker intent.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::Connection;
use serde_json::{json,Value};
use std::collections::BTreeMap;

const PENDING: [&str; 6]=["queued","dispatched","unknown","placed","filled","partial"];

pub fn report(db:&Connection, t:Option<&Telemetry>, fresh:bool, now_ms:u64)->Result<Value>{
    let current=if fresh {
        t.filter(|t|t.connected&&t.login>0)
            .map(|t|serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)))
            .transpose()?
    }else{None};
    let sql=format!(
        "SELECT context,kind,CASE WHEN state='observed' THEN 'partial' ELSE state END AS visible_state,COUNT(*),MIN(created_ms),MAX(created_ms)
         FROM broker_commands
         WHERE {}
         GROUP BY context,kind,visible_state
         ORDER BY MAX(created_ms) DESC",crate::execution_legacy::PENDING_BROKER_SQL
    );
    let mut statement=db.prepare(&sql)?;
    let rows=statement.query_map([],|r|{
        Ok((
            r.get::<_,String>(0)?,r.get::<_,String>(1)?,
            r.get::<_,String>(2)?,r.get::<_,u64>(3)?,
            r.get::<_,i64>(4)?,r.get::<_,i64>(5)?
        ))
    })?;
    let mut count=0u64;
    let mut current_count=0u64;
    let mut other_count=0u64;
    let mut states=BTreeMap::<String,u64>::new();
    let mut last_groups=Vec::new();
    let mut oldest_age=0u64;
    for row in rows {
        let (context,kind,state,total,oldest,newest)=row?;
        debug_assert!(PENDING.contains(&state.as_str()));
        count=count.saturating_add(total);
        *states.entry(state.clone()).or_default()+=total;
        let is_current=current.as_ref().is_some_and(|c|*c==context);
        if is_current {current_count+=total;} else {other_count+=total;}
        let scope=if is_current{"current"}else if current.is_some(){"other"}else{"unverified"};
        let age=now_ms.saturating_sub(u64::try_from(newest).unwrap_or(0))/1000;
        oldest_age=oldest_age.max(now_ms.saturating_sub(u64::try_from(oldest).unwrap_or(0))/1000);
        // Display no login, broker server, symbol, secret, raw command or ID.
        if last_groups.len()<25 {
            last_groups.push(json!({"scope":scope,"kind":kind,"state":state,
                "count":total,"last_age_seconds":age}));
        }
    }
    Ok(json!({
        "schema":"broker-reconciliation-v1","read_only":true,
        "pending_all_accounts":true,"blocked_settings":count>0,
        "pending_total":count,"current_session_pending":current_count,
        "other_or_unverified_session_pending":other_count,
        "current_account_verified":current.is_some(),
        "oldest_age_seconds":oldest_age,
        "by_state":states,"recent_groups":last_groups,
        "action":"Never delete broker commands or reset journal to unlock saving. Reconcile exact broker history first.",
        "note":"STOP cancels unsent queued opens, not unknown or dispatched broker submissions."
    }))
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn pending_from_another_demo_session_is_visible_without_leaking_account_identity(){
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE broker_commands
            (id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,
             kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,
             payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}')").unwrap();
        let t=Telemetry{connected:true,login:17,server:"Demo-Private".into(),
            symbol:"XAUUSDr".into(),currency:"USD".into(),..Default::default()};
        let current=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap();
        let other=r#"["PreviousDemo",13,"XAUUSD","USD"]"#;
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload)
                    VALUES('a',?1,'sniper','open',1000,'unknown','{}')",[other]).unwrap();
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload)
                    VALUES('b',?1,'sniper','close',2000,'filled','{}')",[&current]).unwrap();
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload)
                    VALUES('c',?1,'sniper','open',3000,'observed','{}')",[&current]).unwrap();
        let value=report(&db,Some(&t),true,10000).unwrap();
        assert_eq!(value["pending_total"],2);
        assert_eq!(value["current_session_pending"],1);
        assert_eq!(value["other_or_unverified_session_pending"],1);
        assert_eq!(value["by_state"]["unknown"],1);
        assert_eq!(value["by_state"]["filled"],1);
        assert_eq!(value["blocked_settings"],true);
        let text=value.to_string();
        assert!(!text.contains("PreviousDemo"));
        assert!(!text.contains("Demo-Private"));
        assert!(!text.contains("XAUUSDr"));
        assert!(!text.contains("\"17\""));
        let unknown=report(&db,None,false,10000).unwrap();
        assert_eq!(unknown["current_session_pending"],0);
        assert_eq!(unknown["other_or_unverified_session_pending"],2);
        db.execute("UPDATE broker_commands SET state='observed' WHERE id='b'",[]).unwrap();
        db.execute("UPDATE broker_commands SET state='cancelled' WHERE id='a'",[]).unwrap();
        assert_eq!(report(&db,Some(&t),true,10000).unwrap()["pending_total"],0);
    }
}
