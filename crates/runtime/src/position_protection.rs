//! Account/symbol-scoped, durable stop-protection decisions for owned positions.
//! A queued modification is an intent, never proof that MT5 changed a stop.
use anyhow::{Context, Result};
use braintrade_domain::{execution, Position, Telemetry};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub break_even_enabled: bool,
    pub break_even_trigger_points: u32,
    pub break_even_offset_points: u32,
    pub trailing_enabled: bool,
    pub trailing_distance_points: u32,
    pub trailing_step_points: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self { break_even_enabled:false, break_even_trigger_points:10, break_even_offset_points:3,
            trailing_enabled:false, trailing_distance_points:15, trailing_step_points:5 }
    }
}
impl Policy {
    pub fn validate(&self)->Result<()> {
        anyhow::ensure!((1..=1_000_000).contains(&self.break_even_trigger_points)
            && self.break_even_offset_points<=self.break_even_trigger_points
            && (1..=1_000_000).contains(&self.trailing_distance_points)
            && (1..=1_000_000).contains(&self.trailing_step_points),"قيم التعادل أو الوقف المتحرك غير صالحة");
        Ok(())
    }
    fn active(&self)->bool {self.break_even_enabled || self.trailing_enabled}
}

pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS position_protection_settings(
        context TEXT NOT NULL,target TEXT NOT NULL,policy_id TEXT NOT NULL,payload TEXT NOT NULL,
        updated_ms INTEGER NOT NULL,PRIMARY KEY(context,target));
        CREATE TABLE IF NOT EXISTS position_protection_commands(
        id TEXT PRIMARY KEY,context TEXT NOT NULL,target TEXT NOT NULL,ticket INTEGER NOT NULL,
        created_ms INTEGER NOT NULL,last_poll_ms INTEGER NOT NULL DEFAULT 0,state TEXT NOT NULL,
        payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');
        CREATE INDEX IF NOT EXISTS position_protection_work ON position_protection_commands(context,target,state,last_poll_ms,created_ms);
        -- A restored Core backup does not restore the Gateway send journal.
        -- Never treat pre-restart work as a fresh authorization to send.
        UPDATE position_protection_commands SET state='cancelled' WHERE state='queued';
        UPDATE position_protection_commands SET state='unknown' WHERE state='dispatched';")?;
    Ok(())
}

fn context(t:&Telemetry)->String {serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap()}
fn mode(t:&Telemetry,target:&str)->bool {
    t.connected && t.login>0 && !t.server.is_empty() && !t.currency.is_empty()
        && matches!((target,t.demo,t.account_trade_mode),("demo",true,Some(0))|("live",false,Some(2)))
}
fn load(db:&Connection,t:&Telemetry,target:&str)->Result<(Policy,String)> {
    let row:Option<(String,String)>=db.query_row(
        "SELECT payload,policy_id FROM position_protection_settings WHERE context=?1 AND target=?2",
        params![context(t),target],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    match row {
        Some((raw,id))=>{let policy:Policy=serde_json::from_str(&raw)?;policy.validate()?;
            anyhow::ensure!(!id.is_empty(),"Protection policy identity missing");Ok((policy,id))}
        None=>Ok((Policy::default(),String::new()))
    }
}
pub fn save(db:&Connection,t:&Telemetry,target:&str,policy:&Policy,expected_id:&str,now:u64)->Result<Value> {
    anyhow::ensure!(mode(t,target),"حساب MT5 المرتبط غير متاح");policy.validate()?;
    anyhow::ensure!(load(db,t,target)?.1==expected_id,"تغيّرت إعدادات الحماية؛ حدّث الصفحة وأعد مراجعتها");
    let key=context(t);let id=uuid::Uuid::new_v4().simple().to_string();
    let tx=db.unchecked_transaction()?;
    tx.execute("INSERT OR REPLACE INTO position_protection_settings(context,target,policy_id,payload,updated_ms) VALUES(?1,?2,?3,?4,?5)",
        params![key,target,id,serde_json::to_string(policy)?,now])?;
    // In-flight modifications must pass the new policy identity on final
    // authorization; interrupted sends remain available for reconciliation.
    tx.execute("UPDATE position_protection_commands SET state='cancelled' WHERE context=?1 AND target=?2 AND state='queued'",params![key,target])?;
    tx.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,'position_protection_changed',?2)",
        params![now,json!({"context":key,"target":target,"policy":policy,"policy_id":id}).to_string()])?;
    tx.commit()?;
    Ok(json!({"ok":true,"policy":policy,"scope":"owned_positions_bound_account_symbol_and_mode"}))
}

/// Candidate SL uses executable exit quotes. Break-even is gross price only;
/// no unknown commissions, swaps or execution costs are presented as covered.
fn candidate(p:&Position,t:&Telemetry,policy:&Policy)->Option<f64> {
    let point=t.point.filter(|v|v.is_finite()&&*v>0.)?;
    let tick=t.tick_size.filter(|v|v.is_finite()&&*v>0.)?;
    let bid=t.bid.filter(|v|v.is_finite()&&*v>0.)?;
    let ask=t.ask.filter(|v|v.is_finite()&&*v>=bid)?;
    if !p.price_open.is_finite()||p.price_open<=0.||!p.sl.is_finite()||p.sl<0. || !p.tp.is_finite()||p.tp<0. {return None}
    let direction=match p.side.as_str(){"BUY"=>1.,"SELL"=>-1.,_=>return None};
    let exit=if direction>0. {bid}else{ask};
    let profit=(exit-p.price_open)*direction;
    let mut candidate:Option<f64>=None;
    if policy.break_even_enabled && profit+tick*1e-7>=policy.break_even_trigger_points as f64*point {
        let exact=p.price_open+direction*policy.break_even_offset_points as f64*point;
        let ticks=exact/tick;
        // A broker tick may be larger than its point. Rounding a requested
        // break-even offset away from profit could place the SL below entry;
        // leave it unchanged when the exact requested offset is not tradable.
        if ticks.is_finite() && (ticks-ticks.round()).abs()<=1e-6 {candidate=Some(ticks.round()*tick);}
    }
    if policy.trailing_enabled && profit+tick*1e-7>=policy.trailing_distance_points as f64*point {
        let raw=exit-direction*policy.trailing_distance_points as f64*point;
        let snapped=if direction>0. {(raw/tick+1e-8).floor()*tick}else{(raw/tick-1e-8).ceil()*tick};
        let enough_step=p.sl==0. || (snapped-p.sl)*direction+tick*1e-7>=policy.trailing_step_points as f64*point;
        if enough_step && candidate.is_none_or(|current|(snapped-current)*direction>0.) {candidate=Some(snapped);}
    }
    let raw=candidate?;
    let stop=if direction>0. {(raw/tick+1e-8).floor()*tick}else{(raw/tick-1e-8).ceil()*tick};
    if !stop.is_finite() || stop<=0. || (exit-stop)*direction<=tick*1e-7
        || p.sl>0. && (stop-p.sl)*direction<=tick*1e-7 {return None}
    Some(stop)
}

fn observed(t:&Telemetry,cmd:&Value)->bool {
    let Some(p)=t.positions.iter().find(|p|Some(p.ticket)==cmd["position"].as_u64()) else{return false};
    let Some(tick)=t.tick_size.filter(|n|n.is_finite()&&*n>0.) else{return false};
    let Some(wanted)=cmd["protect_stop"].as_f64() else{return false};
    let Some(tp)=cmd["expected_position_tp"].as_f64() else{return false};
    let Some(volume)=cmd["expected_position_volume"].as_f64() else{return false};
    p.symbol==t.symbol && Some(p.magic)==cmd["magic"].as_u64()
        && Some(p.identifier)==cmd["expected_position_identifier"].as_u64()
        && cmd["expected_position_side"].as_str()==Some(p.side.as_str())
        && (p.volume-volume).abs()<=1e-8 && (p.tp-tp).abs()<=tick*1e-7 && p.sl>0.
        && match p.side.as_str(){"BUY"=>p.sl>=wanted-tick*1e-7,"SELL"=>p.sl<=wanted+tick*1e-7,_=>false}
}

/// Called only for complete broker snapshots, not the latency-sensitive pulse.
/// At most one new modification is planned per snapshot.
pub fn observe(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<bool> {
    if !mode(t,target){return Ok(false)}
    let key=context(t);
    let mut st=db.prepare("SELECT id,payload FROM position_protection_commands WHERE context=?1 AND target=?2 AND state IN ('dispatched','unknown','placed','filled')")?;
    let rows=st.query_map(params![key,target],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;drop(st);
    for (id,raw) in rows {
        let command:Value=serde_json::from_str(&raw)?;
        if observed(t,&command) {
            db.execute("UPDATE position_protection_commands SET state='observed' WHERE id=?1",[id])?;
        } else if !t.positions.iter().any(|p|Some(p.ticket)==command["position"].as_u64()) {
            // The position disappeared. Preserve the uncertain result for the
            // audit rather than inventing an acknowledgement or exit deal.
            db.execute("UPDATE position_protection_commands SET state='position_absent' WHERE id=?1",[id])?;
        }
    }
    let (policy,policy_id)=load(db,t,target)?;
    let fresh=t.broker_quote_time_ms().is_some_and(|q|q<=now.saturating_add(2_000)&&now.saturating_sub(q)<=3_000);
    if policy.active() && fresh && t.trade_allowed && t.trade_expert {
        for p in &t.positions {
            let Some(brain)=execution::brain_for_magic(p.magic) else{continue};
            if p.symbol!=t.symbol || p.ticket==0 || p.identifier==0 || !p.volume.is_finite() || p.volume<=0. {continue}
            let busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM position_protection_commands
                WHERE context=?1 AND target=?2 AND ticket=?3 AND (state IN ('queued','dispatched','unknown','placed','filled') OR created_ms>?4))",
                params![key,target,p.ticket,now.saturating_sub(2_000)],|r|r.get(0))?;
            if busy {continue}
            let close_busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND kind='close'
                AND (state IN ('queued','dispatched','unknown','placed','filled','partial')
                     OR (state='observed' AND json_extract(result,'$.state')='partial' AND COALESCE(json_extract(result,'$.terminal_order.confirmed'),0)<>1))
                AND json_valid(payload) AND json_extract(payload,'$.position')=?2)",params![key,p.ticket],|r|r.get(0))?;
            if close_busy {continue}
            let Some(stop)=candidate(p,t,&policy) else{continue};
            let id=format!("protect-{}",uuid::Uuid::new_v4().simple());
            let command=json!({"id":id,"kind":"protect","context":key,"login":t.login,"server":t.server,
                "currency":t.currency,"target":target,"symbol":t.symbol,"brain":brain,"magic":p.magic,
                "position":p.ticket,"expected_position_identifier":p.identifier,"expected_position_side":p.side,
                "expected_position_volume":p.volume,"expected_position_open":p.price_open,"expected_position_sl":p.sl,
                "expected_position_tp":p.tp,"protect_stop":stop,"protect_policy_id":policy_id,
                "created_ms":now,"expires_ms":now.saturating_add(30_000),"max_quote_age_ms":3_000});
            db.execute("INSERT INTO position_protection_commands(id,context,target,ticket,created_ms,state,payload) VALUES(?1,?2,?3,?4,?5,'queued',?6)",
                params![id,key,target,p.ticket,now,command.to_string()])?;
            break;
        }
    }
    let ready:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM position_protection_commands WHERE context=?1 AND target=?2
        AND (state IN ('queued','dispatched') OR (state IN ('unknown','placed') AND last_poll_ms+3000<=?3)))",
        params![key,target,now],|r|r.get(0))?;
    Ok(ready)
}

pub fn next(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<Option<Value>> {
    if !mode(t,target){return Ok(None)}
    let key=context(t);
    db.execute("UPDATE position_protection_commands SET state='expired' WHERE context=?1 AND target=?2 AND state='queued' AND json_extract(payload,'$.expires_ms')<?3",params![key,target,now])?;
    let row:Option<(String,String,String)>=db.query_row("SELECT id,payload,state FROM position_protection_commands WHERE context=?1 AND target=?2
        AND (state IN ('queued','dispatched') OR (state IN ('unknown','placed') AND last_poll_ms+3000<=?3))
        ORDER BY CASE WHEN state IN ('queued','dispatched') THEN 0 ELSE 1 END,last_poll_ms,created_ms,id LIMIT 1",
        params![key,target,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((id,raw,state))=row else{return Ok(None)};
    let mut command:Value=serde_json::from_str(&raw)?;
    if state=="queued" {db.execute("UPDATE position_protection_commands SET state='dispatched',last_poll_ms=?2 WHERE id=?1 AND state='queued'",params![id,now])?;}
    else {db.execute("UPDATE position_protection_commands SET last_poll_ms=?2 WHERE id=?1",params![id,now])?;}
    command["state"]=json!(state);Ok(Some(command))
}

pub fn authorize(db:&Connection,t:&Telemetry,target:&str,id:&str,now:u64)->Result<bool> {
    if !mode(t,target){return Ok(false)}
    let row:Option<(String,String)>=db.query_row("SELECT payload,state FROM position_protection_commands WHERE id=?1 AND context=?2 AND target=?3",
        params![id,context(t),target],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((raw,state))=row else{return Ok(false)};
    let cmd:Value=serde_json::from_str(&raw)?;let (policy,policy_id)=load(db,t,target)?;
    if state!="dispatched" || !policy.active() || cmd["protect_policy_id"]!=policy_id
        || cmd["expires_ms"].as_u64().is_none_or(|n|now>n) {return Ok(false)}
    let close_busy:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND kind='close'
        AND (state IN ('queued','dispatched','unknown','placed','filled','partial')
             OR (state='observed' AND json_extract(result,'$.state')='partial' AND COALESCE(json_extract(result,'$.terminal_order.confirmed'),0)<>1)) AND json_valid(payload)
        AND json_extract(payload,'$.position')=?2)",params![context(t),cmd["position"].as_u64().unwrap_or(0)],|r|r.get(0))?;
    Ok(!close_busy && t.positions.iter().any(|p|Some(p.ticket)==cmd["position"].as_u64()
        && Some(p.identifier)==cmd["expected_position_identifier"].as_u64() && Some(p.magic)==cmd["magic"].as_u64()
        && p.symbol==t.symbol && cmd["expected_position_side"].as_str()==Some(p.side.as_str())
        && cmd["expected_position_volume"].as_f64().is_some_and(|v|(p.volume-v).abs()<=1e-8)
        && cmd["expected_position_sl"].as_f64()==Some(p.sl) && cmd["expected_position_tp"].as_f64()==Some(p.tp)))
}

pub fn result(db:&Connection,packet:&Value)->Result<bool> {
    let id=packet["id"].as_str().context("Protection result ID required")?;
    let state=packet["state"].as_str().context("Protection result state required")?;
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM position_protection_commands WHERE id=?1)",[id],|r|r.get(0))?;
    if !exists {return Ok(false)}
    anyhow::ensure!(matches!(state,"filled"|"rejected"|"unknown"|"placed"|"cancelled"),"Invalid protection result state");
    db.execute("UPDATE position_protection_commands SET state=?2,result=?3 WHERE id=?1 AND state IN ('dispatched','unknown','placed')",
        params![id,state,packet.to_string()])?;
    Ok(true)
}

pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value> {
    let (policy,policy_id)=load(db,t,target)?;
    let mut st=db.prepare("SELECT id,state,payload,result FROM position_protection_commands WHERE context=?1 AND target=?2 ORDER BY created_ms DESC,id DESC LIMIT 12")?;
    let rows=st.query_map(params![context(t),target],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?;
    let mut commands=Vec::new();for row in rows {let(id,state,payload,result)=row?;
        commands.push(json!({"id":id,"state":state,"request":serde_json::from_str::<Value>(&payload)?,"result":serde_json::from_str::<Value>(&result)?}));}
    Ok(json!({"policy":policy,"policy_id":policy_id,"commands":commands,"scope":"owned_positions_bound_account_symbol_and_mode",
        "note":"التعادل لسعر الدخول إجمالياً ولا يضمن تغطية العمولات أو الانزلاق. تعديل الوقف يعتمد على اتصال MT5 ورد الوسيط ويستمر أثناء STOP."}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup()->Connection {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE events(id INTEGER PRIMARY KEY,timestamp_ms INTEGER NOT NULL,kind TEXT NOT NULL,detail TEXT NOT NULL);
            CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,kind TEXT NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');").unwrap();
        init(&db).unwrap();db
    }
    fn telemetry(now:u64)->Telemetry {
        Telemetry {connected:true,login:17,server:"Demo".into(),currency:"USD".into(),demo:true,
            account_trade_mode:Some(0),trade_allowed:true,trade_expert:true,symbol:"XAUUSD".into(),
            bid:Some(2000.20),ask:Some(2000.22),point:Some(0.01),tick_size:Some(0.01),
            quote_time_ms:Some(now),source_quote_time_ms:Some(now),tick_age_ms:Some(0),
            positions:vec![Position {ticket:777,identifier:700,magic:410601,symbol:"XAUUSD".into(),side:"BUY".into(),
                volume:0.1,price_open:2000.,price_current:2000.2,sl:1999.,tp:2005.,..Position::default()}],..Telemetry::default()}
    }
    fn enabled()->Policy {Policy{break_even_enabled:true,trailing_enabled:true,..Policy::default()}}
    fn near(left:f64,right:f64){assert!((left-right).abs()<1e-7,"{left} != {right}")}

    #[test]
    fn buy_and_sell_candidates_only_improve_the_stop() {
        let mut t=telemetry(100_000);let p=&t.positions[0];
        near(candidate(p,&t,&enabled()).unwrap(),2000.05);
        let mut p=p.clone();p.sl=2000.05;
        assert!(candidate(&p,&t,&enabled()).is_none(),"No repeated stop at the same price");
        p.side="SELL".into();p.sl=2001.;p.tp=1995.;t.bid=Some(1999.78);t.ask=Some(1999.8);
        near(candidate(&p,&t,&enabled()).unwrap(),1999.95);
        p.sl=1999.90;
        assert!(candidate(&p,&t,&enabled()).is_none(),"Never loosen an already better stop");
    }

    #[test]
    fn break_even_never_rounds_an_unrepresentable_offset_into_a_loss() {
        let mut t=telemetry(100_000);t.tick_size=Some(0.05);
        let policy=Policy{break_even_enabled:true,trailing_enabled:false,..Policy::default()};
        assert!(candidate(&t.positions[0],&t,&policy).is_none());
        let representable=Policy{break_even_offset_points:5,..policy};
        near(candidate(&t.positions[0],&t,&representable).unwrap(),2000.05);
    }

    #[test]
    fn policy_is_scoped_and_persists_without_enabling_other_accounts() {
        let db=setup();let t=telemetry(100_000);
        assert!(!load(&db,&t,"demo").unwrap().0.active());
        save(&db,&t,"demo",&enabled(),"",100_000).unwrap();
        assert_eq!(load(&db,&t,"demo").unwrap().0,enabled());
        assert!(save(&db,&t,"demo",&Policy::default(),"",100_001).is_err(),"A stale draft must not overwrite the saved policy");
        let mut other=t.clone();other.login=18;
        assert!(!load(&db,&other,"demo").unwrap().0.active());
        let mut real=t.clone();real.demo=false;real.account_trade_mode=Some(2);
        assert!(!load(&db,&real,"live").unwrap().0.active());
        assert!(save(&db,&real,"demo",&enabled(),"",100_000).is_err());
        let mut invalid=enabled();invalid.break_even_offset_points=11;
        assert!(save(&db,&t,"demo",&invalid,&load(&db,&t,"demo").unwrap().1,100_001).is_err());
        assert_eq!(load(&db,&t,"demo").unwrap().0,enabled());
    }

    #[test]
    fn protection_requires_observed_position_and_no_duplicate_during_uncertainty() {
        let db=setup();let mut t=telemetry(100_000);
        save(&db,&t,"demo",&enabled(),"",100_000).unwrap();
        assert!(observe(&db,&t,"demo",100_000).unwrap());
        let cmd=next(&db,&t,"demo",100_001).unwrap().unwrap();
        let id=cmd["id"].as_str().unwrap();
        assert!(authorize(&db,&t,"demo",id,100_002).unwrap());
        assert!(result(&db,&json!({"id":id,"state":"placed"})).unwrap());
        observe(&db,&t,"demo",100_500).unwrap();
        let state:String=db.query_row("SELECT state FROM position_protection_commands WHERE id=?1",[id],|r|r.get(0)).unwrap();
        assert_eq!(state,"placed","Broker ACK alone is not an observed stop");
        assert_eq!(db.query_row("SELECT COUNT(*) FROM position_protection_commands",[],|r|r.get::<_,u32>(0)).unwrap(),1);
        t.positions[0].sl=cmd["protect_stop"].as_f64().unwrap();
        observe(&db,&t,"demo",100_750).unwrap();
        let state:String=db.query_row("SELECT state FROM position_protection_commands WHERE id=?1",[id],|r|r.get(0)).unwrap();
        assert_eq!(state,"observed");
    }

    #[test]
    fn changed_policy_and_pending_close_revoke_a_dispatched_modification() {
        let db=setup();let t=telemetry(100_000);
        save(&db,&t,"demo",&enabled(),"",100_000).unwrap();observe(&db,&t,"demo",100_000).unwrap();
        let cmd=next(&db,&t,"demo",100_001).unwrap().unwrap();let id=cmd["id"].as_str().unwrap();
        db.execute("INSERT INTO broker_commands(id,context,kind,state,payload) VALUES('close',?1,'close','queued',?2)",params![context(&t),json!({"position":777}).to_string()]).unwrap();
        assert!(!authorize(&db,&t,"demo",id,100_002).unwrap());
        db.execute("DELETE FROM broker_commands",[]).unwrap();
        assert!(authorize(&db,&t,"demo",id,100_002).unwrap());
        save(&db,&t,"demo",&Policy::default(),&load(&db,&t,"demo").unwrap().1,100_003).unwrap();
        assert!(!authorize(&db,&t,"demo",id,100_004).unwrap());
    }

    #[test]
    fn stale_quotes_and_foreign_positions_cannot_create_intents() {
        let db=setup();let mut t=telemetry(100_000);
        save(&db,&t,"demo",&enabled(),"",100_000).unwrap();
        assert!(!observe(&db,&t,"demo",104_000).unwrap());
        t.positions[0].magic=12345;
        assert!(!observe(&db,&t,"demo",100_000).unwrap());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM position_protection_commands",[],|r|r.get::<_,u32>(0)).unwrap(),0);
    }

    #[test]
    fn restart_never_authorizes_old_dispatched_work_as_a_fresh_send() {
        let db=setup();let t=telemetry(100_000);
        save(&db,&t,"demo",&enabled(),"",100_000).unwrap();observe(&db,&t,"demo",100_000).unwrap();
        let cmd=next(&db,&t,"demo",100_001).unwrap().unwrap();let id=cmd["id"].as_str().unwrap();
        assert!(authorize(&db,&t,"demo",id,100_002).unwrap());
        init(&db).unwrap();
        assert!(!authorize(&db,&t,"demo",id,100_003).unwrap());
        assert_eq!(next(&db,&t,"demo",103_002).unwrap().unwrap()["state"],"unknown");
    }
}
