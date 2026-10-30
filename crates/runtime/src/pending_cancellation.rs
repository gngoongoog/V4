//! Explicit one-ticket cancellation of an owned MT5 pending order.
//! Disappearance from active orders is not proof of cancellation.
use anyhow::{Context,Result};
use braintrade_domain::{execution,PendingOrder,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

fn context(t:&Telemetry)->String {serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap()}
fn mode(t:&Telemetry,target:&str)->bool {
    t.connected && t.login>0 && matches!((target,t.demo,t.account_trade_mode),("demo",true,Some(0))|("live",false,Some(2)))
}
pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS pending_cancel_commands(
        id TEXT PRIMARY KEY,context TEXT NOT NULL,target TEXT NOT NULL,ticket INTEGER NOT NULL,
        created_ms INTEGER NOT NULL,last_poll_ms INTEGER NOT NULL DEFAULT 0,state TEXT NOT NULL,
        payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');
        CREATE INDEX IF NOT EXISTS pending_cancel_work ON pending_cancel_commands(context,target,state,last_poll_ms,created_ms);
        UPDATE pending_cancel_commands SET state='cancelled' WHERE state='queued';
        UPDATE pending_cancel_commands SET state='unknown' WHERE state='dispatched';")?;Ok(())
}
pub fn pending(db:&Connection)->Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM pending_cancel_commands WHERE state IN ('queued','dispatched','unknown','placed'))",[],|r|r.get(0))?)
}
pub fn queue(db:&Connection,t:&Telemetry,target:&str,selected:&PendingOrder,now:u64)->Result<Value> {
    anyhow::ensure!(mode(t,target),"الحساب المرتبط غير متاح");
    let rows=t.pending_order_details.as_ref().context("تفاصيل أوامر MT5 غير مكتملة؛ حدّث البيانات")?;
    let matching:Vec<_>=rows.iter().filter(|o|o.ticket==selected.ticket).collect();
    anyhow::ensure!(matching.len()==1 && matching[0]==selected,"الأمر تغيّر أو اختفى؛ أعد مراجعة MT5");
    anyhow::ensure!((2..=7).contains(&selected.order_type),"اختر أمراً معلّقاً فقط");
    let brain=execution::brain_for_magic(selected.magic).context("الأمر ليس ضمن هوية BrainTrade")?;
    let key=context(t);
    let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM pending_cancel_commands WHERE context=?1 AND target=?2 AND ticket=?3 AND state IN ('queued','dispatched','unknown','placed'))",
        params![key,target,selected.ticket],|r|r.get(0))?;
    anyhow::ensure!(!busy,"طلب إلغاء هذا الأمر قيد التسوية بالفعل");
    let id=format!("pending-cancel-{}",uuid::Uuid::new_v4().simple());
    let cmd=json!({"id":id,"kind":"cancel_pending","context":key,"target":target,
        "login":t.login,"server":t.server,"currency":t.currency,"symbol":selected.symbol,
        "magic":selected.magic,"brain":brain,"order":selected.ticket,"expected_order_type":selected.order_type,
        "expected_volume_initial":selected.volume_initial,"expected_volume_current":selected.volume_current,
        "expected_price_open":selected.price_open,"expected_sl":selected.sl,"expected_tp":selected.tp,
        "expected_stoplimit":selected.price_stoplimit,"expected_setup_ms":selected.time_setup_ms,
        "expected_expiration":selected.expiration,"created_ms":now,"expires_ms":now.saturating_add(30_000)});
    let tx=db.unchecked_transaction()?;
    tx.execute("INSERT INTO pending_cancel_commands(id,context,target,ticket,created_ms,state,payload) VALUES(?1,?2,?3,?4,?5,'queued',?6)",
        params![id,key,target,selected.ticket,now,cmd.to_string()])?;
    tx.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,'pending_cancel_requested',?2)",
        params![now,json!({"context":key,"target":target,"ticket":selected.ticket,"id":id}).to_string()])?;
    tx.commit()?;Ok(json!({"ok":true,"queued":true,"id":id,"ticket":selected.ticket,
        "note":"الطابور لا يثبت الإلغاء؛ راجع سجل الوسيط فقد يكون الأمر قد نُفذ كلياً أو جزئياً."}))
}
pub fn ready(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<bool> {
    if !mode(t,target){return Ok(false)}
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM pending_cancel_commands WHERE context=?1 AND target=?2
        AND (state IN ('queued','dispatched') OR (state IN ('unknown','placed') AND last_poll_ms+3000<=?3)))",
        params![context(t),target,now],|r|r.get(0))?)
}
pub fn next(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<Option<Value>> {
    if !mode(t,target){return Ok(None)}
    let key=context(t);
    db.execute("UPDATE pending_cancel_commands SET state='expired' WHERE context=?1 AND target=?2 AND state='queued' AND json_extract(payload,'$.expires_ms')<?3",params![key,target,now])?;
    let row:Option<(String,String,String)>=db.query_row("SELECT id,payload,state FROM pending_cancel_commands WHERE context=?1 AND target=?2
        AND (state IN ('queued','dispatched') OR (state IN ('unknown','placed') AND last_poll_ms+3000<=?3))
        ORDER BY CASE WHEN state IN ('queued','dispatched') THEN 0 ELSE 1 END,last_poll_ms,created_ms,id LIMIT 1",
        params![key,target,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((id,raw,state))=row else{return Ok(None)};let mut cmd:Value=serde_json::from_str(&raw)?;
    if state=="queued" {db.execute("UPDATE pending_cancel_commands SET state='dispatched',last_poll_ms=?2 WHERE id=?1 AND state='queued'",params![id,now])?;}
    else {db.execute("UPDATE pending_cancel_commands SET last_poll_ms=?2 WHERE id=?1",params![id,now])?;}
    cmd["state"]=json!(state);Ok(Some(cmd))
}
pub fn authorize(db:&Connection,t:&Telemetry,target:&str,id:&str,now:u64)->Result<bool> {
    if !mode(t,target){return Ok(false)}
    let row:Option<(String,String)>=db.query_row("SELECT payload,state FROM pending_cancel_commands WHERE id=?1 AND context=?2 AND target=?3",
        params![id,context(t),target],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((raw,state))=row else{return Ok(false)};let cmd:Value=serde_json::from_str(&raw)?;
    if state!="dispatched"||cmd["expires_ms"].as_u64().is_none_or(|v|now>v){return Ok(false)}
    Ok(t.pending_order_details.as_ref().is_some_and(|orders|orders.iter().any(|o|
        Some(o.ticket)==cmd["order"].as_u64() && Some(o.magic)==cmd["magic"].as_u64()
        && cmd["symbol"].as_str()==Some(o.symbol.as_str()) && cmd["expected_order_type"].as_i64()==Some(o.order_type as i64)
        && cmd["expected_volume_initial"].as_f64()==Some(o.volume_initial) && cmd["expected_volume_current"].as_f64()==Some(o.volume_current)
        && cmd["expected_price_open"].as_f64()==Some(o.price_open) && cmd["expected_sl"].as_f64()==Some(o.sl)
        && cmd["expected_tp"].as_f64()==Some(o.tp) && cmd["expected_stoplimit"].as_f64()==Some(o.price_stoplimit)
        && cmd["expected_setup_ms"].as_u64()==Some(o.time_setup_ms) && cmd["expected_expiration"].as_u64()==Some(o.expiration))))
}
pub fn result(db:&Connection,packet:&Value)->Result<bool> {
    let id=packet["id"].as_str().context("Cancellation ID required")?;
    let state=packet["state"].as_str().context("Cancellation state required")?;
    let row:Option<String>=db.query_row("SELECT payload FROM pending_cancel_commands WHERE id=?1",[id],|r|r.get(0)).optional()?;
    let Some(raw)=row else{return Ok(false)};let command:Value=serde_json::from_str(&raw)?;
    anyhow::ensure!(matches!(state,"cancelled"|"filled"|"expired"|"rejected"|"unknown"|"placed"),"Invalid cancellation result");
    let proof=&packet["terminal_order"];
    if !proof.is_null() {
        let initial=proof["volume_initial"].as_f64().unwrap_or(f64::NAN);
        let remaining=proof["volume_current"].as_f64().unwrap_or(f64::NAN);
        anyhow::ensure!(proof["confirmed"]==true && proof["ticket"]==command["order"] && proof["state"]==state
            && initial.is_finite() && remaining.is_finite() && remaining>=0. && remaining<=initial+1e-8
            && command["expected_volume_initial"].as_f64().is_some_and(|v|(v-initial).abs()<=1e-8),"Invalid terminal-order evidence");
    } else {anyhow::ensure!(!matches!(state,"filled"|"expired"),"Terminal order history required");}
    db.execute("UPDATE pending_cancel_commands SET state=?2,result=?3 WHERE id=?1 AND state IN ('dispatched','unknown','placed')",
        params![id,state,packet.to_string()])?;Ok(true)
}
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Vec<Value>> {
    let mut st=db.prepare("SELECT state,payload,result FROM pending_cancel_commands WHERE context=?1 AND target=?2 ORDER BY created_ms DESC,id DESC LIMIT 12")?;
    let rows=st.query_map(params![context(t),target],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?;
    let mut out=Vec::new();for row in rows {let(state,raw,result)=row?;
        out.push(json!({"state":state,"request":serde_json::from_str::<Value>(&raw)?,"result":serde_json::from_str::<Value>(&result)?}));}Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup()->Connection {let db=Connection::open_in_memory().unwrap();db.execute_batch("CREATE TABLE events(id INTEGER PRIMARY KEY,timestamp_ms INTEGER NOT NULL,kind TEXT NOT NULL,detail TEXT NOT NULL);").unwrap();init(&db).unwrap();db}
    fn order()->PendingOrder {PendingOrder{ticket:42,symbol:"EURUSD".into(),magic:410618,order_type:2,volume_initial:0.1,volume_current:0.1,price_open:1.1,sl:1.,tp:1.2,price_stoplimit:0.,time_setup_ms:123000,expiration:0}}
    fn telemetry()->Telemetry {Telemetry{connected:true,demo:true,login:17,server:"Demo".into(),currency:"USD".into(),symbol:"XAUUSD".into(),account_trade_mode:Some(0),trade_allowed:true,trade_expert:true,pending_orders:1,pending_order_details:Some(vec![order()]),..Telemetry::default()}}
    #[test]
    fn exact_ticket_is_queued_and_duplicate_or_foreign_requests_are_rejected() {
        let db=setup();let t=telemetry();let queued=queue(&db,&t,"demo",&order(),100_000).unwrap();assert_eq!(queued["queued"],true);
        assert!(queue(&db,&t,"demo",&order(),100_001).is_err());let mut foreign=order();foreign.magic=99;
        let mut other=t.clone();other.pending_order_details=Some(vec![foreign.clone()]);assert!(queue(&db,&other,"demo",&foreign,100_001).is_err());
        let mut changed=order();changed.volume_current=0.05;assert!(queue(&db,&t,"demo",&changed,100_001).is_err());
        assert!(queue(&db,&t,"live",&order(),100_001).is_err());let mut unavailable=t.clone();unavailable.pending_order_details=None;
        assert!(queue(&db,&unavailable,"demo",&order(),100_001).is_err());
    }
    #[test]
    fn authorization_revokes_on_partial_fill_and_history_proves_terminal_outcome() {
        let db=setup();let mut t=telemetry();queue(&db,&t,"demo",&order(),100_000).unwrap();
        let command=next(&db,&t,"demo",100_001).unwrap().unwrap();let id=command["id"].as_str().unwrap();
        assert!(authorize(&db,&t,"demo",id,100_002).unwrap());t.pending_order_details.as_mut().unwrap()[0].volume_current=0.05;
        assert!(!authorize(&db,&t,"demo",id,100_003).unwrap());assert!(result(&db,&json!({"id":id,"state":"filled"})).is_err());
        let evidence=json!({"confirmed":true,"ticket":42,"state":"filled","volume_initial":0.1,"volume_current":0.0});
        result(&db,&json!({"id":id,"state":"filled","terminal_order":evidence})).unwrap();assert!(!pending(&db).unwrap());
        assert_eq!(report(&db,&t,"demo").unwrap()[0]["state"],"filled");
    }
    #[test]
    fn restart_demotes_dispatched_work_and_never_reauthorizes_it() {
        let db=setup();let t=telemetry();queue(&db,&t,"demo",&order(),100_000).unwrap();
        let command=next(&db,&t,"demo",100_001).unwrap().unwrap();let id=command["id"].as_str().unwrap();
        assert!(authorize(&db,&t,"demo",id,100_002).unwrap());init(&db).unwrap();assert!(!authorize(&db,&t,"demo",id,100_003).unwrap());
        assert_eq!(next(&db,&t,"demo",103_002).unwrap().unwrap()["state"],"unknown");
    }
}
