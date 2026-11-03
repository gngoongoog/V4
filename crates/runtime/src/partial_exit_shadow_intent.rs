//! TP1 research close-intent contract, NEVER a broker command.
//!
//! Only a complete post-save MT5 source quote can cause a single durable
//! research candidate. This table is structurally separate from broker_commands
//! and gateway sends. Core's existing TP-stage authorization deny stays on.
//! This module does not authorize orders or alter position protection.
use anyhow::Result;
use braintrade_domain::{execution,Telemetry};
use rusqlite::{params,Connection};
use serde_json::{json,Value};

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn near(a:f64,b:f64,step:f64)->bool{
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
fn demo(t:&Telemetry,target:&str,now:u64)->bool{
    target=="demo"&&t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&t.margin_mode==Some(2)&&t.validate()&&t.login>0
        &&!t.server.is_empty()&&!t.symbol.is_empty()&&!t.currency.is_empty()
        &&t.quote_is_fresh(now,3_000)
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_shadow_close_intents(
            context TEXT NOT NULL,target TEXT NOT NULL CHECK(target='demo'),
            position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
            plan_id TEXT NOT NULL,
            ticket INTEGER NOT NULL CHECK(ticket>0),
            magic INTEGER NOT NULL CHECK(magic>0),
            lot_volume REAL NOT NULL CHECK(lot_volume>0),
            full_position_volume REAL NOT NULL CHECK(full_position_volume>0),
            lot_step REAL NOT NULL CHECK(lot_step>0),
            target_price REAL NOT NULL CHECK(target_price>0),
            source_quote_ms INTEGER NOT NULL CHECK(source_quote_ms>0),
            captured_ms INTEGER NOT NULL CHECK(captured_ms>0),
            payload TEXT NOT NULL,
            PRIMARY KEY(context,target,position_identifier),
            UNIQUE(context,target,plan_id)
        );
        CREATE INDEX IF NOT EXISTS tp_shadow_intent_recent
          ON partial_exit_shadow_close_intents(context,target,captured_ms DESC);")?;
    Ok(())
}
/// Runs only with complete reconciled MT5 snapshots, directly after the
/// shadow TP1 observer and before leaving the 1Hz history-audit branch.
pub fn observe(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<()>{
    if !demo(t,target,now){return Ok(())}
    let key=context(t);
    let shadow=crate::partial_exit_shadow_trigger::report(db,t,target)?;
    if shadow["status"]!="shadow_crossings_no_broker_orders"{return Ok(())}
    let proposals=crate::partial_exit_prices::report(db,t,target,now)?;
    if proposals["status"]!="demo_operator_price_proposals_only"{return Ok(())}
    let Some(shadows)=shadow["rows"].as_array()else{return Ok(())};
    let Some(prices)=proposals["rows"].as_array()else{return Ok(())};
    for evidence in shadows {
        if evidence["state"]!="historical_shadow_crossing_only"||
            evidence["observed_ms"].as_u64()!=Some(now)||
            evidence["stage"].as_u64()!=Some(1)||
            evidence["broker_fill_verified"]!=false||
            evidence["orders_enabled"]!=false||
            evidence["next_stage_authorized"]!=false {continue}
        let Some(pid)=evidence["position_identifier"].as_u64().filter(|id|*id>0)
            else{continue};
        let Some(plan)=evidence["plan_id"].as_str().filter(|id|
            id.starts_with("tp-plan-")&&id.len()==40
            &&id[8..].bytes().all(|b|b.is_ascii_hexdigit()))else{continue};
        let Some(source)=evidence["source_quote_time_ms"].as_u64().filter(|n|
            Some(*n)==t.source_quote_time_ms&&*n>0)else{continue};
        let Some(price)=prices.iter().find(|p|
            p["position_identifier"].as_u64()==Some(pid)&&
            p["state"]=="price_levels_saved_no_execution"&&
            p["plan_ids"][0]==plan&&p["quote_crossing_only"][0]==true)else{continue};
        let Some(level)=price["prices"][0].as_f64().filter(|x|x.is_finite()&&*x>0.)
            else{continue};
        if evidence["target_price"].as_f64()!=Some(level){continue}
        let mut stages=db.prepare(
            "SELECT stage,plan_id,planned_volume,status FROM partial_exit_stage_plans
             WHERE context=?1 AND target='demo' AND position_identifier=?2
             ORDER BY stage")?;
        let planned=stages.query_map(params![key,pid],|r|Ok((
            r.get::<_,u32>(0)?,r.get::<_,String>(1)?,
            r.get::<_,f64>(2)?,r.get::<_,String>(3)?
        )))?.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stages);
        if planned.len()!=3||planned.iter().enumerate().any(|(i,(stage,id,v,state))|
            *stage!=(i+1) as u32||*state!="prepared_only"||
            id.as_str()!=price["plan_ids"][i].as_str().unwrap_or("")||
            !v.is_finite()||*v<=0.) {continue}
        let (ticket,magic,original,step,status,raw):(
            u64,u64,f64,f64,String,String
        )=match db.query_row(
            "SELECT ticket,magic,initial_volume,step,status,payload
             FROM partial_exit_drafts WHERE context=?1 AND target='demo'
             AND position_identifier=?2",
            params![key,pid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,
                r.get(3)?,r.get(4)?,r.get(5)?))
        ){Ok(row)=>row,Err(_)=>continue};
        if status!="draft_only"||ticket==0||magic==0||!step.is_finite()||step<=0.
            ||!original.is_finite()||original<=0. {continue}
        let Ok(draft)=serde_json::from_str::<Value>(&raw)else{continue};
        let Some(brain)=execution::brain_for_magic(magic)else{continue};
        if draft["schema"]!="partial-exit-draft-v1"||
            draft["position_identifier"].as_u64()!=Some(pid)||
            draft["ticket"].as_u64()!=Some(ticket)||
            draft["magic"].as_u64()!=Some(magic)||
            draft["symbol"]!=t.symbol||
            draft["broker_orders_created"]!=false||
            draft["side"].as_str().is_none_or(|s|s!="BUY"&&s!="SELL")||
            !near(draft["entry_volume"].as_f64().unwrap_or(f64::NAN),original,step)||
            !near(draft["volume_step"].as_f64().unwrap_or(f64::NAN),step,step)
        {continue}
        let physical=t.positions.iter().filter(|p|p.identifier==pid||p.ticket==ticket)
            .collect::<Vec<_>>();
        if physical.len()!=1{continue}
        let p=physical[0];
        if p.identifier!=pid||p.ticket!=ticket||p.magic!=magic||
            p.symbol!=t.symbol||draft["side"]!=p.side||
            !near(p.volume,original,step) {continue}
        let minimum=t.volume_min.unwrap_or(f64::NAN);
        let maximum=t.volume_max.unwrap_or(f64::NAN);
        let current_step=t.volume_step.unwrap_or(f64::NAN);
        let lots=planned[0].2;
        let residual=original-lots;
        let all_lots=planned.iter().map(|x|x.2).sum::<f64>();
        let on_step=|v:f64|v.is_finite()&&v>0.&&
            (v/step).is_finite()&&(v/step-(v/step).round()).abs()<=1e-6;
        if !minimum.is_finite()||!maximum.is_finite()||minimum<=0.||
            maximum<minimum||!near(current_step,step,step)||
            !near(all_lots,original,step)||!on_step(lots)||
            lots<minimum-step*1e-6||lots>maximum+step*1e-6||
            !on_step(residual)||residual<minimum-step*1e-6 {continue}
        // Any previously journaled close on this physical ticket is a
        // competing economic event until independently reconciled. Do not
        // create a new "ready" candidate from an ambiguous position.
        let prior:i64=db.query_row(
            "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='close'
             AND CASE WHEN json_valid(payload) THEN
                CAST(json_extract(payload,'$.position') AS INTEGER) ELSE 0 END=?2",
            params![key,ticket],|r|r.get(0))?;
        if prior!=0{continue}
        let candidate=json!({
            "schema":"tp1-shadow-close-intent-v1",
            "kind":"research_close_intent","target":"demo",
            "context":key,"position_identifier":pid,
            "ticket":ticket,"magic":magic,"symbol":t.symbol,
            "brain":brain,"side":p.side,
            "tp_stage":1,"tp_stage_plan_id":plan,
            "expected_position_volume":original,
            "close_lots":lots,"expected_remaining_volume":residual,
            "volume_step":step,"volume_min":minimum,"volume_max":maximum,
            "target_price":level,"quote_source_ms":source,
            "observed_ms":now,
            "orders_enabled":false,"broker_command_id":Value::Null,
            "gateway_send_id":Value::Null,"broker_order_id":Value::Null,
            "broker_fill_verified":false,"next_stage_authorized":false,
            "breakeven_enabled":false
        });
        db.execute(
            "INSERT OR IGNORE INTO partial_exit_shadow_close_intents(
               context,target,position_identifier,plan_id,ticket,magic,
               lot_volume,full_position_volume,lot_step,target_price,
               source_quote_ms,captured_ms,payload)
             VALUES(?1,'demo',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![key,pid,plan,ticket,magic,lots,original,step,level,source,
                now,candidate.to_string()])?;
    }
    Ok(())
}
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    if target!="demo"||!t.connected||!t.demo||t.account_trade_mode!=Some(0)||
        t.margin_mode!=Some(2)||!t.validate()||t.login==0||
        t.server.is_empty()||t.symbol.is_empty()||t.currency.is_empty(){
        return Ok(json!({"schema":"tp-shadow-close-intents-v1",
            "status":"unverified_demo_account","read_only":true,
            "orders_enabled":false,"rows":[]}))
    }
    let key=context(t);
    let mut statement=db.prepare(
        "SELECT position_identifier,plan_id,ticket,magic,lot_volume,
                full_position_volume,lot_step,target_price,source_quote_ms,
                captured_ms,payload
         FROM partial_exit_shadow_close_intents WHERE context=?1 AND target='demo'
         ORDER BY captured_ms DESC LIMIT 50")?;
    let saved=statement.query_map(params![key],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,String>(1)?,
        r.get::<_,u64>(2)?,r.get::<_,u64>(3)?,
        r.get::<_,f64>(4)?,r.get::<_,f64>(5)?,
        r.get::<_,f64>(6)?,r.get::<_,f64>(7)?,
        r.get::<_,u64>(8)?,r.get::<_,u64>(9)?,
        r.get::<_,String>(10)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let shadow=crate::partial_exit_shadow_trigger::report(db,t,target)?;
    let current=shadow["rows"].as_array();
    let mut rows=Vec::new();
    for (pid,plan,ticket,magic,lots,full,step,level,quote_at,observed,raw) in saved{
        let candidate=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
        let stage_ok=current.and_then(|array|array.iter().find(|r|
            r["position_identifier"].as_u64()==Some(pid)
                &&r["plan_id"]==plan
                &&r["state"]=="historical_shadow_crossing_only")).is_some();
        let p=t.positions.iter().filter(|p|p.identifier==pid||p.ticket==ticket)
            .collect::<Vec<_>>();
        let physical_ok=p.len()==1&&p[0].identifier==pid&&p[0].ticket==ticket
            &&p[0].magic==magic&&p[0].symbol==t.symbol
            &&candidate["side"]==p[0].side
            &&near(p[0].volume,full,step);
        let recent_orders:i64=db.query_row(
            "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='close'
             AND CASE WHEN json_valid(payload) THEN
                  CAST(json_extract(payload,'$.position') AS INTEGER) ELSE 0 END=?2",
            params![key,ticket],|r|r.get(0))?;
        let broker_grid_ok=t.volume_step.is_some_and(|v|near(v,step,step))
            &&t.volume_min.is_some_and(|v|v.is_finite()&&v>0.&&
                lots>=v-step*1e-6)
            &&t.volume_max.is_some_and(|v|v.is_finite()&&v>=lots);
        let bound=stage_ok&&physical_ok&&recent_orders==0&&broker_grid_ok&&
            candidate["schema"]=="tp1-shadow-close-intent-v1"&&
            candidate["kind"]=="research_close_intent"&&
            candidate["context"]==key&&candidate["target"]=="demo"&&
            candidate["position_identifier"].as_u64()==Some(pid)&&
            candidate["ticket"].as_u64()==Some(ticket)&&
            candidate["magic"].as_u64()==Some(magic)&&
            candidate["tp_stage"].as_u64()==Some(1)&&candidate["tp_stage_plan_id"]==plan&&
            near(candidate["close_lots"].as_f64().unwrap_or(f64::NAN),lots,step)&&
            near(candidate["expected_position_volume"].as_f64().unwrap_or(f64::NAN),full,step)&&
            near(candidate["expected_remaining_volume"].as_f64().unwrap_or(f64::NAN),full-lots,step)&&
            candidate["target_price"].as_f64()==Some(level)&&
            candidate["quote_source_ms"].as_u64()==Some(quote_at)&&
            candidate["observed_ms"].as_u64()==Some(observed)&&
            candidate["orders_enabled"]==false&&
            candidate["broker_command_id"].is_null()&&candidate["gateway_send_id"].is_null()&&
            candidate["broker_fill_verified"]==false&&
            candidate["next_stage_authorized"]==false&&
            candidate["breakeven_enabled"]==false;
        // This is only a recorded research candidate. The currently
        // configured account and mode must still own the same lot grid.
        // No broker send may ever be resumed from this row.
        rows.push(json!({
            "position_identifier":pid,"stage":1,"plan_id":plan,
            "ticket":ticket,"observed_ms":observed,
            "close_lots":if bound{json!(lots)}else{Value::Null},
            "expected_remaining_volume":if bound{json!(full-lots)}else{Value::Null},
            "state":if bound{"shadow_close_intent_only"}else{"needs_review"},
            "read_only":true,"orders_enabled":false,
            "broker_fill_verified":false,"next_stage_authorized":false,
            "breakeven_enabled":false
        }));
    }
    Ok(json!({"schema":"tp-shadow-close-intents-v1",
        "status":"research_close_intents_no_broker_commands",
        "context":key,"target":"demo","read_only":true,
        "orders_enabled":false,"next_stage_authorized":false,
        "breakeven_enabled":false,"rows":rows,
        "note":"A durable research-only close-lot snapshot, not a dispatched command or a broker fill."}))
}


#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    fn telemetry()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:222,server:"DemoA".into(),
            currency:"USD".into(),symbol:"XAUUSDm".into(),
            balance:1000.,equity:1000.,volume_min:Some(0.01),
            volume_step:Some(0.01),volume_max:Some(100.),
            tick_size:Some(0.01),bid:Some(2000.),ask:Some(2000.2),
            tick_age_ms:Some(100),quote_time_ms:Some(9900),
            source_quote_time_ms:Some(9900),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.10,
                price_open:1999.5,..Default::default()}],..Default::default()}
    }
    fn setup(db:&Connection){
        crate::partial_exit_ledger::init(db).unwrap();
        crate::partial_exit_stage::init(db).unwrap();
        crate::partial_exit_prices::init(db).unwrap();
        crate::partial_exit_shadow_trigger::init(db).unwrap();
        init(db).unwrap();
        db.execute_batch(
            "CREATE TABLE broker_commands(context TEXT,kind TEXT,payload TEXT);").unwrap();
        let t=telemetry();
        crate::partial_exit_ledger::observe(db,&t,"demo",100).unwrap();
        crate::partial_exit_stage::observe(db,&t,"demo",100).unwrap();
        let plans=crate::partial_exit_stage::report(db,&t,"demo").unwrap();
        let ids=std::array::from_fn(|i|plans["rows"][i]["plan_id"].as_str().unwrap().to_string());
        crate::partial_exit_prices::save(db,&t,"demo",907,
            &ids,[2001.,2002.,2003.],10_000).unwrap();
    }
    fn crossed()->Telemetry{
        let mut t=telemetry();
        t.bid=Some(2001.2);t.ask=Some(2001.4);
        t.quote_time_ms=Some(10_200);
        t.source_quote_time_ms=Some(10_200);
        t
    }
    fn tick(db:&Connection,t:&Telemetry,now:u64){
        crate::partial_exit_shadow_trigger::observe(db,t,"demo",now).unwrap();
        observe(db,t,"demo",now).unwrap();
    }
    fn count(db:&Connection)->u32{
        db.query_row("SELECT COUNT(*) FROM partial_exit_shadow_close_intents",[],
            |r|r.get(0)).unwrap()
    }
    #[test]
    fn tp1_crossing_creates_one_non_executable_grid_aligned_close_lot_snapshot(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let initial=telemetry();tick(&db,&initial,10_050);
        assert_eq!(count(&db),0);
        let t=crossed();tick(&db,&t,10_250);
        assert_eq!(count(&db),1);
        let v=report(&db,&t,"demo").unwrap();
        assert_eq!(v["status"],"research_close_intents_no_broker_commands");
        assert_eq!(v["rows"][0]["state"],"shadow_close_intent_only");
        assert_eq!(v["rows"][0]["close_lots"],0.05);
        assert_eq!(v["rows"][0]["expected_remaining_volume"],0.05);
        assert_eq!(v["orders_enabled"],false);
        assert_eq!(v["next_stage_authorized"],false);
        assert_eq!(v["breakeven_enabled"],false);
        assert_eq!(v["rows"].as_array().unwrap().len(),1);
        let raw:String=db.query_row(
            "SELECT payload FROM partial_exit_shadow_close_intents",[],
            |r|r.get(0)).unwrap();
        let candidate:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(candidate["kind"],"research_close_intent");
        assert_eq!(candidate["broker_command_id"],Value::Null);
        assert_eq!(candidate["gateway_send_id"],Value::Null);
        assert_eq!(candidate["broker_order_id"],Value::Null);
        assert_eq!(candidate["broker_fill_verified"],false);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
            |r|r.get::<_,u32>(0)).unwrap(),0);
        tick(&db,&t,10_300);
        assert_eq!(count(&db),1,"Repeated full snapshots may not duplicate TP1 intent");
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["close_lots"],0.05);
    }
    #[test]
    fn unknown_quote_old_shadow_and_competing_broker_close_never_create_intent(){
        for index in 0..5 {
            let db=Connection::open_in_memory().unwrap();setup(&db);
            let mut t=crossed();
            match index{
                0=>t.source_quote_time_ms=None,
                1=>{t.source_quote_time_ms=Some(9950);t.quote_time_ms=Some(9950);},
                2=>t.tick_age_ms=Some(9_000),
                3=>{t.margin_mode=Some(0);},
                _=>{
                    db.execute("INSERT INTO broker_commands VALUES(?1,'close',?2)",
                        params![context(&t),json!({"position":87}).to_string()]).unwrap();
                }
            }
            tick(&db,&t,10_250);
            assert_eq!(count(&db),0,"case {index}");
        }
    }
    #[test]
    fn damaged_or_moved_position_lot_grid_and_stages_revoke_candidate_display(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let t=crossed();tick(&db,&t,10_250);
        let mut changed=t.clone();changed.positions[0].volume=0.07;
        assert_eq!(report(&db,&changed,"demo").unwrap()["rows"][0]["state"],"needs_review");
        changed=t.clone();changed.positions[0].magic=999;
        assert_eq!(report(&db,&changed,"demo").unwrap()["rows"][0]["state"],"needs_review");
        changed=t.clone();changed.volume_step=Some(0.02);
        assert_eq!(report(&db,&changed,"demo").unwrap()["rows"][0]["state"],"needs_review");
        db.execute("UPDATE partial_exit_shadow_close_intents
           SET payload=json_set(payload,'$.broker_fill_verified',json('true'))",[]).unwrap();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["state"],"needs_review");
    }
    #[test]
    fn damaged_plan_or_later_physical_close_never_resumes_saved_shadow_candidate(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let t=crossed();tick(&db,&t,10_250);
        let plan:String=db.query_row("SELECT plan_id FROM partial_exit_shadow_close_intents",
            [],|r|r.get(0)).unwrap();
        db.execute("INSERT INTO broker_commands VALUES(?1,'close',?2)",
            params![context(&t),json!({"position":87,"tp_stage_plan_id":plan}).to_string()]).unwrap();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["state"],"needs_review");
        db.execute("DELETE FROM broker_commands",[]).unwrap();
        db.execute("UPDATE partial_exit_stage_plans SET status='review_required'
            WHERE stage=1",[]).unwrap();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["state"],"needs_review");
        tick(&db,&t,10_350);
        assert_eq!(count(&db),1);
    }
    #[test]
    fn restart_preserves_only_research_intent_and_isolates_broker_accounts(){
        let path=std::env::temp_dir().join(format!("bt-tp1-intent-{}.sqlite",
            uuid::Uuid::new_v4().simple()));
        let t=crossed();let first;
        {
            let db=Connection::open(&path).unwrap();setup(&db);
            tick(&db,&t,10_250);
            first=report(&db,&t,"demo").unwrap()["rows"][0].clone();
        }
        {
            let db=Connection::open(&path).unwrap();init(&db).unwrap();
            tick(&db,&t,10_350);
            assert_eq!(count(&db),1);
            assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0],first);
            let mut other=t.clone();other.login+=1;
            assert!(report(&db,&other,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
            other=t.clone();other.demo=false;other.account_trade_mode=Some(2);
            assert_eq!(report(&db,&other,"live").unwrap()["status"],
                "unverified_demo_account");
            assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
                |r|r.get::<_,u32>(0)).unwrap(),0);
        }
        let _=std::fs::remove_file(path);
    }
}
