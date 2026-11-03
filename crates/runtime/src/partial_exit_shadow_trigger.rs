//! Shadow-only first TP price-crossing receipt on complete reconciled MT5 snapshots.
//!
//! This is NOT a broker order, command authorization, TP fill or profit.
//! It only proves that a FRESH later MT5 source quote crossed the immutable
//! operator-saved TP1 price for the same owned physical position. Subsequent
//! stages require durable actual broker fills, so no TP2/TP3 shadow dispatch
//! is manufactured while Core stage-close production remains disabled.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params,Connection};
use serde_json::{json,Value};

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn bound(t:&Telemetry,target:&str,now:u64)->bool {
    target=="demo"&&t.connected&&t.demo&&t.account_trade_mode==Some(0)
    &&t.margin_mode==Some(2)&&t.validate()&&t.login>0
    &&!t.server.is_empty()&&!t.symbol.is_empty()&&!t.currency.is_empty()
    &&t.source_quote_time_ms.is_some_and(|source|source>0)
    &&t.quote_is_fresh(now,3_000)
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_shadow_tp1(
            context TEXT NOT NULL,target TEXT NOT NULL CHECK(target='demo'),
            position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
            plan_id TEXT NOT NULL,
            quote_source_ms INTEGER NOT NULL CHECK(quote_source_ms>0),
            observed_ms INTEGER NOT NULL CHECK(observed_ms>0),
            broker_price REAL NOT NULL CHECK(broker_price>0),
            target_price REAL NOT NULL CHECK(target_price>0),
            evidence_json TEXT NOT NULL,
            PRIMARY KEY(context,target,position_identifier),
            UNIQUE(context,target,plan_id)
        );")?;
    Ok(())
}
/// Called no faster than every 1s on full broker snapshots. No API read or
/// Fast Pulse may insert these records. Never issues a broker command.
pub fn observe(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<()>{
    if !bound(t,target,now){return Ok(())}
    let source=t.source_quote_time_ms.unwrap_or(0);
    let key=context(t);
    let proposals=crate::partial_exit_prices::report(db,t,target,now)?;
    if proposals["status"]!="demo_operator_price_proposals_only"{return Ok(())}
    let Some(rows)=proposals["rows"].as_array()else{return Ok(())};
    for r in rows {
        if r["state"]!="price_levels_saved_no_execution"
            ||r["quote_crossing_only"][0]!=true {continue}
        let Some(pid)=r["position_identifier"].as_u64().filter(|id|*id>0)else{continue};
        let Some(ticket)=r["ticket"].as_u64().filter(|id|*id>0)else{continue};
        let Some(plan)=r["plan_ids"][0].as_str().filter(|s|
            s.starts_with("tp-plan-")&&s.len()==40)else{continue};
        let Some(target_price)=r["prices"][0].as_f64().filter(|v|v.is_finite()&&*v>0.)
            else{continue};
        let original:rusqlite::Result<String>=db.query_row(
            "SELECT captured_json FROM partial_exit_price_plans WHERE
                context=?1 AND target='demo' AND position_identifier=?2 AND ticket=?3",
            params![key,pid,ticket],|row|row.get(0));
        let Ok(raw)=original else{continue};
        let Ok(capture)=serde_json::from_str::<Value>(&raw)else{continue};
        let saved_quote=capture["broker_quote_ms"].as_u64().unwrap_or(0);
        let saved_ms=capture["saved_ms"].as_u64().unwrap_or(0);
        if saved_quote==0||source<=saved_quote||now<=saved_ms
           ||capture["orders_enabled"]!=false
           ||capture["breakeven_enabled"]!=false
           ||capture["side"]!="BUY"&&capture["side"]!="SELL"{continue}
        let matches=t.positions.iter().filter(|p|p.identifier==pid&&p.ticket==ticket
            &&p.magic==capture["magic"].as_u64().unwrap_or(0)
            &&p.symbol==t.symbol&&capture["side"]==p.side).count();
        if matches!=1 {continue}
        let broker_price=if capture["side"]=="BUY"{t.bid}else{t.ask};
        let Some(price)=broker_price.filter(|v|v.is_finite()&&*v>0.)
            else{continue};
        if (capture["side"]=="BUY"&&price<target_price)||
            (capture["side"]=="SELL"&&price>target_price){continue}
        // Never retain a TP1 "intent" if there is ANY already submitted
        // Core close tied to the plan. The stage-close authorizer itself
        // stays locked until real broker-DEMO testing is complete.
        let existing:i64=db.query_row(
            "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='close'
             AND CASE WHEN json_valid(payload) THEN
                  json_extract(payload,'$.tp_stage_plan_id') ELSE NULL END=?2",
            params![key,plan],|row|row.get(0))?;
        if existing!=0{continue}
        let proof=json!({
            "schema":"partial-exit-shadow-price-v1",
            "position_identifier":pid,"ticket":ticket,
            "plan_id":plan,"stage":1,
            "source_quote_time_ms":source,"saved_quote_time_ms":saved_quote,
            "bid":t.bid,"ask":t.ask,
            "execution_price_side":if capture["side"]=="BUY"{"bid"}else{"ask"},
            "price":price,"target_price":target_price,
            "orders_enabled":false,"core_command_queued":false,
            "broker_fill_verified":false,"next_stage_authorized":false,
            "breakeven_enabled":false
        });
        db.execute(
            "INSERT OR IGNORE INTO partial_exit_shadow_tp1(
             context,target,position_identifier,plan_id,quote_source_ms,
             observed_ms,broker_price,target_price,evidence_json)
             VALUES(?1,'demo',?2,?3,?4,?5,?6,?7,?8)",
            params![key,pid,plan,source,now,price,target_price,proof.to_string()])?;
    }
    Ok(())
}
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    if target!="demo"||!t.connected||!t.demo||t.account_trade_mode!=Some(0)
        ||t.margin_mode!=Some(2)||!t.validate()||t.login==0
        ||t.server.is_empty()||t.currency.is_empty()||t.symbol.is_empty(){
        return Ok(json!({"schema":"partial-exit-shadow-tp1-v1",
            "status":"unverified_demo_account","rows":[],
            "read_only":true,"orders_enabled":false}))
    }
    let key=context(t);
    let mut st=db.prepare(
        "SELECT q.position_identifier,q.plan_id,q.quote_source_ms,q.observed_ms,
            q.broker_price,q.target_price,q.evidence_json,
            p.plan_id,p.status
         FROM partial_exit_shadow_tp1 q
         LEFT JOIN partial_exit_stage_plans p ON p.context=q.context
            AND p.target=q.target AND p.position_identifier=q.position_identifier
            AND p.stage=1
         WHERE q.context=?1 AND q.target='demo'
         ORDER BY q.observed_ms DESC,q.position_identifier DESC LIMIT 50")?;
    let records=st.query_map(params![key],|row|Ok((
        row.get::<_,u64>(0)?,row.get::<_,String>(1)?,
        row.get::<_,u64>(2)?,row.get::<_,u64>(3)?,
        row.get::<_,f64>(4)?,row.get::<_,f64>(5)?,
        row.get::<_,String>(6)?,row.get::<_,Option<String>>(7)?,
        row.get::<_,Option<String>>(8)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let rows=records.into_iter().map(|(pid,id,source,at,price,target_price,raw,current,status)|{
        let v=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
        let valid=v["schema"]=="partial-exit-shadow-price-v1"
            &&v["position_identifier"].as_u64()==Some(pid)
            &&v["plan_id"]==id&&v["stage"]==1
            &&v["source_quote_time_ms"].as_u64()==Some(source)
            &&v["price"].as_f64()==Some(price)
            &&v["target_price"].as_f64()==Some(target_price)
            &&v["orders_enabled"]==false
            &&v["broker_fill_verified"]==false
            &&v["core_command_queued"]==false
            &&v["next_stage_authorized"]==false
            &&v["breakeven_enabled"]==false;
        let intact=valid&&current.as_deref()==Some(id.as_str())
            &&status.as_deref()==Some("prepared_only");
        json!({"position_identifier":pid,"stage":1,"plan_id":id,
            "observed_ms":at,"source_quote_time_ms":source,
            "observed_price":price,"target_price":target_price,
            "state":if intact{"historical_shadow_crossing_only"}else{"needs_review"},
            "broker_fill_verified":false,"orders_enabled":false,
            "next_stage_authorized":false,"breakeven_enabled":false})
    }).collect::<Vec<_>>();
    Ok(json!({"schema":"partial-exit-shadow-tp1-v1",
        "status":"shadow_crossings_no_broker_orders",
        "context":key,"target":"demo","rows":rows,
        "read_only":true,"orders_enabled":false,
        "next_stage_authorized":false,"breakeven_enabled":false,
        "note":"A historic verified source quote crossed TP1. This is not order placement, a fill, booked profit or permission for TP2."}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::Position;
    fn snapshot()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:222,server:"DemoA".into(),
            currency:"USD".into(),symbol:"XAUUSDm".into(),
            balance:1000.,equity:1000.,volume_min:Some(0.01),
            volume_step:Some(0.01),volume_max:Some(100.),
            tick_size:Some(0.01),bid:Some(2000.),ask:Some(2000.2),
            tick_age_ms:Some(100),quote_time_ms:Some(9900),
            source_quote_time_ms:Some(9900),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.1,
                price_open:1999.5,..Default::default()}],..Default::default()}
    }
    fn fixture()->Connection{
        let db=Connection::open_in_memory().unwrap();
        crate::partial_exit_ledger::init(&db).unwrap();
        crate::partial_exit_stage::init(&db).unwrap();
        crate::partial_exit_prices::init(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE broker_commands(context TEXT,kind TEXT,payload TEXT);").unwrap();
        init(&db).unwrap();
        let t=snapshot();
        crate::partial_exit_ledger::observe(&db,&t,"demo",100).unwrap();
        crate::partial_exit_stage::observe(&db,&t,"demo",100).unwrap();
        let plans=crate::partial_exit_stage::report(&db,&t,"demo").unwrap();
        let ids=std::array::from_fn(|i|plans["rows"][i]["plan_id"].as_str().unwrap().to_string());
        crate::partial_exit_prices::save(&db,&t,"demo",907,
            &ids,[2001.,2002.,2003.],10_000).unwrap();
        db
    }
    fn count(db:&Connection)->u32{
        db.query_row("SELECT COUNT(*) FROM partial_exit_shadow_tp1",[],|r|r.get(0)).unwrap()
    }
    #[test]
    fn delayed_authentic_quote_records_one_shadow_crossing_only(){
        let db=fixture();let mut t=snapshot();
        observe(&db,&t,"demo",10_050).unwrap();
        assert_eq!(count(&db),0);
        t.bid=Some(2001.2);t.ask=Some(2001.4);
        t.quote_time_ms=Some(10_200);t.source_quote_time_ms=Some(10_200);
        observe(&db,&t,"demo",10_250).unwrap();
        assert_eq!(count(&db),1);
        let report=report(&db,&t,"demo").unwrap();
        assert_eq!(report["rows"][0]["state"],"historical_shadow_crossing_only");
        assert_eq!(report["rows"][0]["orders_enabled"],false);
        assert_eq!(report["rows"][0]["broker_fill_verified"],false);
        assert_eq!(report["rows"][0]["next_stage_authorized"],false);
        assert_eq!(report["rows"][0]["breakeven_enabled"],false);
        assert!(report["rows"].as_array().unwrap().len()==1);
        observe(&db,&t,"demo",10_300).unwrap();
        assert_eq!(count(&db),1);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
            |r|r.get::<_,u32>(0)).unwrap(),0);
        // One TP1 crossing never invents TP2 / TP3 closing events.
    }
    #[test]
    fn no_quote_provenance_unchanged_timestamp_or_stale_quote_never_records(){
        let db=fixture();
        for ix in 0..5 {
            let mut t=snapshot();
            t.bid=Some(2001.2);t.ask=Some(2001.4);
            match ix {
                0=>{t.source_quote_time_ms=None;t.quote_time_ms=Some(10_200);}
                1=>{t.quote_time_ms=Some(9_900);t.source_quote_time_ms=Some(9_900);}
                2=>{t.quote_time_ms=Some(10_200);t.source_quote_time_ms=Some(10_200);
                    t.tick_age_ms=Some(3_001);}
                3=>{t.quote_time_ms=Some(10_200);t.source_quote_time_ms=Some(10_200);
                    t.demo=false;t.account_trade_mode=Some(2);}
                _=>{t.quote_time_ms=Some(10_200);t.source_quote_time_ms=Some(10_200);
                    t.margin_mode=Some(0);}
            }
            observe(&db,&t,"demo",10_250).unwrap();
            assert_eq!(count(&db),0,"variant {ix}");
        }
    }
    #[test]
    fn stale_or_manipulated_position_and_stage_price_are_not_shadow_fills(){
        let db=fixture();let mut t=snapshot();
        t.quote_time_ms=Some(10_200);t.source_quote_time_ms=Some(10_200);
        t.bid=Some(2001.2);t.ask=Some(2001.4);
        t.positions[0].volume=0.07;
        observe(&db,&t,"demo",10_250).unwrap();
        assert_eq!(count(&db),0);
        t.positions[0].volume=0.1;
        db.execute("UPDATE partial_exit_stage_plans SET status='review_required' WHERE stage=1",[]).unwrap();
        observe(&db,&t,"demo",10_250).unwrap();
        assert_eq!(count(&db),0);
    }
}
