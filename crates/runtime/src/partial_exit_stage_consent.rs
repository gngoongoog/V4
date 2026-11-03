//! Explicit operator approval for one immutable TP1/TP2/TP3 price plan.
//!
//! Existing saved prices are research-only: an upgrade MUST NOT silently
//! turn those stored prices into broker closes. This is an account-scoped
//! precondition for a separately audited TP close outbox, NOT an MT5 sender.
use anyhow::{ensure,Context,Result};
use braintrade_domain::Telemetry;
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn account_ok(t:&Telemetry,target:&str)->bool{
    t.connected&&t.login>0&&t.margin_mode==Some(2)&&t.validate()
        &&matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
}
fn plan_ids_valid(ids:&[String;3])->bool{
    ids.iter().all(|id|id.len()==40&&id.starts_with("tp-plan-")
        &&id[8..].bytes().all(|v|v.is_ascii_hexdigit()&&!v.is_ascii_uppercase()))
        &&ids[0]!=ids[1]&&ids[0]!=ids[2]&&ids[1]!=ids[2]
}

fn near(a:f64,b:f64,step:f64)->bool{
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
/// Stored consent is a historical operator action, NOT necessarily a current
/// authorization. Never show it as currently approved after a position closes,
/// a broker ticket changes, or the immutable stage/price lineage is revoked.
fn current_lineage(db:&Connection,t:&Telemetry,target:&str,key:&str,
    pid:u64,ticket:u64,ids:&[String;3],approved_ms:u64)->bool{
    let matching=t.positions.iter().filter(|p|
        p.identifier==pid||p.ticket==ticket).collect::<Vec<_>>();
    if matching.len()!=1{return false}
    let p=matching[0];
    if p.identifier!=pid||p.ticket!=ticket||p.magic==0||p.symbol!=t.symbol
        ||!matches!(p.side.as_str(),"BUY"|"SELL"){return false}
    let saved=db.query_row(
        "SELECT ticket,magic,plan_ids_json,prices_json,captured_json,saved_ms
         FROM partial_exit_price_plans WHERE context=?1 AND target=?2
         AND position_identifier=?3",
        params![key,target,pid],|r|Ok((
            r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,
            r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,u64>(5)?
        ))
    ).optional();
    let Ok(Some((price_ticket,magic,raw_ids,raw_prices,raw_capture,saved_ms)))=saved
        else{return false};
    if price_ticket!=ticket||magic!=p.magic||saved_ms==0||approved_ms<saved_ms
        ||serde_json::from_str::<Value>(&raw_ids).ok()!=Some(json!(ids))
    {return false}
    let Ok(cap)=serde_json::from_str::<Value>(&raw_capture)else{return false};
    let Ok(prices)=serde_json::from_str::<Value>(&raw_prices)else{return false};
    if !crate::partial_exit_prices::capture_geometry_valid(&cap,&prices){
        return false
    }
    let tick=t.tick_size.unwrap_or(f64::NAN);
    let step=t.volume_step.unwrap_or(f64::NAN);
    if cap["schema"]!="partial-exit-price-capture-v1"
        ||cap["context"]!=key||cap["target"]!=target
        ||cap["position_identifier"].as_u64()!=Some(pid)
        ||cap["ticket"].as_u64()!=Some(ticket)
        ||cap["magic"].as_u64()!=Some(magic)
        ||cap["symbol"]!=t.symbol||cap["side"]!=p.side
        ||cap["saved_ms"].as_u64()!=Some(saved_ms)
        ||cap["orders_enabled"]!=false||cap["trading_enabled"]!=false
        ||cap["breakeven_enabled"]!=false
        ||!near(cap["tick_size"].as_f64().unwrap_or(f64::NAN),tick,tick)
        ||!near(cap["lot_step"].as_f64().unwrap_or(f64::NAN),step,step)
        ||!near(cap["entry"].as_f64().unwrap_or(f64::NAN),p.price_open,tick)
        ||!near(cap["original_broker_tp"].as_f64().unwrap_or(f64::NAN),p.tp,tick)
        ||!near(cap["original_volume"].as_f64().unwrap_or(f64::NAN),p.volume,step)
    {return false}
    let Ok(mut stmt)=db.prepare(
        "SELECT stage,plan_id,status FROM partial_exit_stage_plans
         WHERE context=?1 AND target=?2 AND position_identifier=?3
         ORDER BY stage")else{return false};
    let plans=stmt.query_map(params![key,target,pid],|r|Ok((
        r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?
    )));
    let Ok(plans)=plans else{return false};
    let Ok(plans)=plans.collect::<rusqlite::Result<Vec<_>>>()else{return false};
    plans.len()==3&&plans.iter().enumerate().all(|(i,(stage,id,status))|
        *stage==(i+1) as u32&&id==&ids[i]&&status=="prepared_only")
}

pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS partial_exit_stage_consents(
        context TEXT NOT NULL,
        target TEXT NOT NULL CHECK(target IN ('demo','live')),
        position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
        position_ticket INTEGER NOT NULL CHECK(position_ticket>0),
        plan_ids TEXT NOT NULL,
        approved_revision INTEGER NOT NULL,
        approved_ms INTEGER NOT NULL CHECK(approved_ms>0),
        enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
        purpose TEXT NOT NULL CHECK(purpose='preparation_only'),
        PRIMARY KEY(context,target,position_identifier)
    );")?;
    Ok(())
}
/// Reading a stored consent is not proof of a fill or permission to send.
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    let empty=||json!({"schema":"tp-stage-operator-consent-v1",
        "status":"no_verified_bound_account","rows":[],"read_only":true,
        "automatic_tp_execution_enabled":false,"orders_created":0});
    if !account_ok(t,target){return Ok(empty())}
    let key=context(t);
    let mut statement=db.prepare("SELECT position_identifier,position_ticket,
            plan_ids,approved_revision,approved_ms,enabled,purpose
        FROM partial_exit_stage_consents WHERE context=?1 AND target=?2
        ORDER BY approved_ms DESC LIMIT 50")?;
    let rows=statement.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,
        r.get::<_,u64>(3)?,r.get::<_,u64>(4)?,r.get::<_,i64>(5)?,r.get::<_,String>(6)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let rows=rows.into_iter().map(|(pid,ticket,raw,rev,at,enabled,purpose)|{
        let ids=serde_json::from_str::<[String;3]>(&raw).ok();
        let intact=ids.as_ref().is_some_and(plan_ids_valid)
            &&rev>0&&at>0&&ticket>0
            &&matches!(enabled,0|1)&&purpose=="preparation_only";
        let current=intact&&ids.as_ref().is_some_and(|ids|
            current_lineage(db,t,target,&key,pid,ticket,ids,at));
        json!({"position_identifier":pid,"position_ticket":ticket,
            "plan_ids":if intact{json!(ids)}else{Value::Null},
            "approved_revision":rev,"approved_ms":at,
            "purpose":purpose,
            "operator_approved":current&&enabled==1,
            "state":if !intact {"invalid_record_requires_review"}
                else if enabled==0 {"operator_disabled"}
                else if current {"operator_approved_not_dispatched"}
                else {"operator_approval_stale_needs_review"},
            "read_only":true,"orders_enabled":false,
            "broker_command_id":Value::Null})
    }).collect::<Vec<_>>();
    Ok(json!({"schema":"tp-stage-operator-consent-v1",
        "status":"account_scoped_operator_consent_not_execution",
        "context":key,"target":target,"rows":rows,
        "purpose":"preparation_only",
        "read_only":true,"orders_enabled":false,
        "automatic_tp_execution_enabled":false,"orders_created":0,
        "note":"Per-position operator authorization stored for a future separately verified stage executor. No MT5 close is sent."}))
}
/// The caller must additionally require STOP, no pending broker intents and
/// matching MT5 session; Live requires typed login at the Core route.
pub fn set(db:&Connection,t:&Telemetry,target:&str,pid:u64,
    ids:&[String;3],enabled:bool,revision:u64,now:u64)->Result<Value>{
    ensure!(account_ok(t,target),"حساب MT5 Hedging المختار غير مطابق");
    ensure!(pid>0&&revision>0&&now>0&&plan_ids_valid(ids),
        "هوية خطة TP أو مراجعة الحساب غير صالحة");
    let key=context(t);
    let old:Option<(u64,String,u64,i64,String)>=db.query_row(
        "SELECT position_ticket,plan_ids,approved_revision,enabled,purpose
         FROM partial_exit_stage_consents
         WHERE context=?1 AND target=?2 AND position_identifier=?3",
        params![key,target,pid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))
    ).optional()?;
    if enabled {
        ensure!(t.quote_is_fresh(now,3_000),"يجب توفر اقتباس MT5 حديث عند اعتماد خطة TP");
        let positions=t.positions.iter().filter(|p|p.identifier==pid).collect::<Vec<_>>();
        ensure!(positions.len()==1&&positions[0].ticket>0,
            "المركز غير موجود أو ليس له تذكرة واحدة مؤكدة");
        let p=positions[0];
        let prices=crate::partial_exit_prices::report(db,t,target,now)?;
        let eligible=prices["status"]=="operator_price_proposals_only"
            &&prices["context"]==key&&prices["target"]==target
            &&prices["rows"].as_array().is_some_and(|rows|rows.iter().any(|row|
                row["position_identifier"].as_u64()==Some(pid)
                &&row["ticket"].as_u64()==Some(p.ticket)
                &&row["state"]=="price_levels_saved_no_execution"
                &&row["plan_ids"]==json!(ids)));
        ensure!(eligible,"أسعار وخطط TP لا تطابق المركز الحالي أو أصبحت غير موثوقة");
        // Old consent is immutable for ticket, plan and Core revision.
        if let Some((old_ticket,old_ids,old_revision,_,purpose))=&old {
            ensure!(purpose=="preparation_only"&&*old_ticket==p.ticket&&*old_revision==revision
                &&serde_json::from_str::<Value>(old_ids).ok()==Some(json!(ids)),
                "موافقة TP قديمة تختلف عن الهوية الحالية؛ لا تعيد تفعيلها تلقائياً");
            db.execute("UPDATE partial_exit_stage_consents SET enabled=1
                WHERE context=?1 AND target=?2 AND position_identifier=?3",
                params![key,target,pid])?;
        }else{
            db.execute("INSERT INTO partial_exit_stage_consents(
                context,target,position_identifier,position_ticket,plan_ids,
                approved_revision,approved_ms,enabled,purpose)
                VALUES(?1,?2,?3,?4,?5,?6,?7,1,'preparation_only')",
                params![key,target,pid,p.ticket,json!(ids).to_string(),revision,now])?;
        }
    }else{
        let (old_ticket,old_ids,_,_,purpose)=old.context("لا توجد خطة TP معتمدة لتعطيلها")?;
        ensure!(purpose=="preparation_only"&&old_ticket>0&&serde_json::from_str::<Value>(&old_ids).ok()==Some(json!(ids)),
            "معرّف الخطة تغيّر؛ لا يمكن تعديل موافقة مختلفة");
        db.execute("UPDATE partial_exit_stage_consents SET enabled=0
            WHERE context=?1 AND target=?2 AND position_identifier=?3",
            params![key,target,pid])?;
    }
    Ok(json!({"ok":true,"operator_approved":enabled,
        "context":key,"target":target,"position_identifier":pid,
        "plan_ids":ids,"purpose":"preparation_only","orders_enabled":false,
        "automatic_tp_execution_enabled":false,"broker_commands_created":0,
        "state":if enabled{"operator_approved_not_dispatched"}else{"operator_disabled"}}))
}
#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    #[test]
    fn historical_approval_never_survives_missing_position_or_plan(){
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        crate::partial_exit_prices::init(&db).unwrap();
        crate::partial_exit_stage::init(&db).unwrap();
        let mut t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:222,server:"DemoA".into(),
            symbol:"XAUUSDm".into(),currency:"USD".into(),
            volume_step:Some(0.01),tick_size:Some(0.01),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.10,
                price_open:2000.,tp:0.,..Default::default()}],
            ..Default::default()};
        let key=context(&t);
        let ids=[
            format!("tp-plan-{}","a".repeat(32)),
            format!("tp-plan-{}","b".repeat(32)),
            format!("tp-plan-{}","c".repeat(32))
        ];
        let capture=json!({"schema":"partial-exit-price-capture-v1",
            "context":key,"target":"demo","position_identifier":907,
            "ticket":87,"magic":410601,"symbol":"XAUUSDm","side":"BUY",
            "entry":2000.,"original_volume":0.10,"lot_step":0.01,
            "tick_size":0.01,"original_broker_tp":0.,"saved_ms":1000,
            "saved_bid":2000.5,"saved_ask":2001.0,"quote_reference":2000.5,
            "orders_enabled":false,"trading_enabled":false,"breakeven_enabled":false});
        db.execute("INSERT INTO partial_exit_price_plans
            (context,target,position_identifier,ticket,magic,plan_ids_json,
             prices_json,captured_json,saved_ms)
            VALUES(?1,'demo',907,87,410601,?2,'[2010,2020,2030]',?3,1000)",
            params![key,json!(ids).to_string(),capture.to_string()]).unwrap();
        for (i,id) in ids.iter().enumerate(){
            db.execute("INSERT INTO partial_exit_stage_plans
                (context,target,position_identifier,stage,plan_id,planned_volume,
                 created_ms,status) VALUES(?1,'demo',907,?2,?3,0.03,900,'prepared_only')",
                params![key,i+1,id]).unwrap();
        }
        db.execute("INSERT INTO partial_exit_stage_consents
            (context,target,position_identifier,position_ticket,plan_ids,
             approved_revision,approved_ms,enabled,purpose)
            VALUES(?1,'demo',907,87,?2,5,1200,1,'preparation_only')",
            params![key,json!(ids).to_string()]).unwrap();
        let row=||report(&db,&t,"demo").unwrap()["rows"][0].clone();
        assert_eq!(row()["state"],"operator_approved_not_dispatched");
        t.positions.clear();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["state"],
            "operator_approval_stale_needs_review");
        t.positions.push(Position{ticket:87,identifier:907,magic:410601,
            symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.10,
            price_open:2000.,tp:0.,..Default::default()});
        db.execute("UPDATE partial_exit_stage_plans
            SET status='review_required' WHERE stage=2",[]).unwrap();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["operator_approved"],false);
        db.execute("UPDATE partial_exit_stage_plans
            SET status='prepared_only' WHERE stage=2",[]).unwrap();
        db.execute("UPDATE partial_exit_price_plans
            SET prices_json='[2010,2020,2020]' ",[]).unwrap();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["operator_approved"],false);
        db.execute("DELETE FROM partial_exit_price_plans",[]).unwrap();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["operator_approved"],false);
    }
    #[test]
    fn no_implicit_or_cross_account_consent(){
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:222,server:"DemoA".into(),
            symbol:"XAUUSDm".into(),currency:"USD".into(),
            positions:vec![Position{ticket:87,identifier:907,..Default::default()}],
            ..Default::default()};
        assert!(report(&db,&t,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
        assert_eq!(report(&db,&t,"live").unwrap()["status"],"no_verified_bound_account");
        let ids=std::array::from_fn(|_|"tp-plan-12345678901234567890123456789012".to_string());
        assert!(set(&db,&t,"live",907,&ids,true,1,1000).is_err());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM partial_exit_stage_consents",[],
            |r|r.get::<_,i64>(0)).unwrap(),0);
    }
}
