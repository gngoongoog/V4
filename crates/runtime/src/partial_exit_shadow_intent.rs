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
fn bound(t:&Telemetry,target:&str,now:u64)->bool{
    matches!((target,t.demo,t.account_trade_mode),
        ("demo",true,Some(0))|("live",false,Some(2)))&&t.connected
        &&t.margin_mode==Some(2)&&t.validate()&&t.login>0
        &&!t.server.is_empty()&&!t.symbol.is_empty()&&!t.currency.is_empty()
        &&t.quote_is_fresh(now,3_000)
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_shadow_close_intents(
            context TEXT NOT NULL,target TEXT NOT NULL CHECK(target IN ('demo','live')),
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
    crate::partial_exit_account_schema::allow_selected_account(
        db,"partial_exit_shadow_close_intents","CREATE INDEX IF NOT EXISTS tp_shadow_intent_recent ON partial_exit_shadow_close_intents(context,target,captured_ms DESC)")?;
    Ok(())
}
/// Runs only with complete reconciled MT5 snapshots, directly after the
/// shadow TP1 observer and before leaving the 1Hz history-audit branch.
pub fn observe(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<()>{
    if !bound(t,target,now){return Ok(())}
    let key=context(t);
    let shadow=crate::partial_exit_shadow_trigger::report(db,t,target)?;
    if shadow["status"]!="shadow_crossings_no_broker_orders"{return Ok(())}
    let proposals=crate::partial_exit_prices::report(db,t,target,now)?;
    if proposals["status"]!="operator_price_proposals_only"{return Ok(())}
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
             WHERE context=?1 AND target=?2 AND position_identifier=?3
             ORDER BY stage")?;
        let planned=stages.query_map(params![key,target,pid],|r|Ok((
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
             FROM partial_exit_drafts WHERE context=?1 AND target=?2
             AND position_identifier=?3",
            params![key,target,pid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,
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
             AND CASE
                 WHEN NOT json_valid(payload) THEN 1
                 WHEN COALESCE(json_type(payload,'$.position'),'missing')!='integer' THEN 1
                 WHEN CAST(json_extract(payload,'$.position') AS INTEGER)=?2 THEN 1
                 ELSE 0 END=1",
            params![key,ticket],|r|r.get(0))?;
        if prior!=0{continue}
        let candidate=json!({
            "schema":"tp1-shadow-close-intent-v1",
            "kind":"research_close_intent","target":target,
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
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![key,target,pid,plan,ticket,magic,lots,original,step,level,source,
                now,candidate.to_string()])?;
    }
    Ok(())
}
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    if !matches!((target,t.demo,t.account_trade_mode),
        ("demo",true,Some(0))|("live",false,Some(2)))||!t.connected||
        t.margin_mode!=Some(2)||!t.validate()||t.login==0||
        t.server.is_empty()||t.symbol.is_empty()||t.currency.is_empty(){
        return Ok(json!({"schema":"tp-shadow-close-intents-v1",
            "status":"unverified_selected_account","read_only":true,
            "orders_enabled":false,"rows":[]}))
    }
    let key=context(t);
    let mut statement=db.prepare(
        "SELECT position_identifier,plan_id,ticket,magic,lot_volume,
                full_position_volume,lot_step,target_price,source_quote_ms,
                captured_ms,payload
         FROM partial_exit_shadow_close_intents WHERE context=?1 AND target=?2
         ORDER BY captured_ms DESC LIMIT 50")?;
    let saved=statement.query_map(params![key,target],|r|Ok((
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
        // A TP1 shadow candidate is not a licence to reuse an altered MT5
        // position. Recheck the *saved* full-position TP and entry, rather
        // than accepting any farther-away newly configured broker TP.
        let capture_ok=db.query_row(
            "SELECT captured_json,prices_json FROM partial_exit_price_plans
             WHERE context=?1 AND target=?2 AND position_identifier=?3",
            params![key,target,pid],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))
        ).ok().is_some_and(|(raw,levels)|{
            let cap=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
            let levels=serde_json::from_str::<Value>(&levels).unwrap_or(Value::Null);
            let tick=cap["tick_size"].as_f64().unwrap_or(f64::NAN);
            crate::partial_exit_prices::capture_geometry_valid(&cap,&levels)
                &&p.len()==1
                &&t.tick_size.is_some_and(|x|near(x,tick,tick))
                &&near(p[0].tp,cap["original_broker_tp"].as_f64().unwrap_or(f64::NAN),tick)
                &&near(p[0].price_open,cap["entry"].as_f64().unwrap_or(f64::NAN),tick)
        });
        let recent_orders:i64=db.query_row(
            "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='close'
             AND CASE
                 WHEN NOT json_valid(payload) THEN 1
                 WHEN COALESCE(json_type(payload,'$.position'),'missing')!='integer' THEN 1
                 WHEN CAST(json_extract(payload,'$.position') AS INTEGER)=?2 THEN 1
                 ELSE 0 END=1",
            params![key,ticket],|r|r.get(0))?;
        let lineage:rusqlite::Result<(f64,String,u64,u64,f64,f64,String)>=
            db.query_row(
                "SELECT s.planned_volume,s.status,d.ticket,d.magic,
                        d.initial_volume,d.step,d.status
                 FROM partial_exit_stage_plans s
                 JOIN partial_exit_drafts d ON d.context=s.context
                     AND d.target=s.target
                     AND d.position_identifier=s.position_identifier
                 WHERE s.context=?1 AND s.target=?2
                     AND s.position_identifier=?3 AND s.stage=1",
                params![key,target,pid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,
                    r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)));
        let lineage_ok=lineage.is_ok_and(|(stage_lots,stage_state,orig_ticket,
            orig_magic,orig_volume,orig_step,draft_state)|
            stage_state=="prepared_only"&&draft_state=="draft_only"
            &&orig_ticket==ticket&&orig_magic==magic
            &&near(stage_lots,lots,step)&&near(orig_volume,full,step)
            &&near(orig_step,step,step));
        let broker_grid_ok=t.volume_step.is_some_and(|v|near(v,step,step))
            &&t.volume_min.is_some_and(|v|v.is_finite()&&v>0.&&
                lots>=v-step*1e-6)
            &&t.volume_max.is_some_and(|v|v.is_finite()&&v>=lots);
        let bound=stage_ok&&physical_ok&&capture_ok&&lineage_ok&&recent_orders==0&&broker_grid_ok&&
            candidate["schema"]=="tp1-shadow-close-intent-v1"&&
            candidate["kind"]=="research_close_intent"&&
            candidate["context"]==key&&candidate["target"]==target&&
            candidate["position_identifier"].as_u64()==Some(pid)&&
            candidate["ticket"].as_u64()==Some(ticket)&&
            candidate["magic"].as_u64()==Some(magic)&&
            candidate["symbol"]==t.symbol&&
            execution::brain_for_magic(magic).is_some_and(|brain|
                candidate["brain"]==brain)&&
            near(candidate["volume_step"].as_f64().unwrap_or(f64::NAN),step,step)&&
            candidate["broker_order_id"].is_null()&&
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
        "context":key,"target":target,"read_only":true,
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
        crate::partial_exit_progress::init(db).unwrap();
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
    fn tp1_core_contract_is_read_only_price_bound_and_not_an_executable_command(){
        let d=Connection::open_in_memory().unwrap();setup(&d);
        let t=crossed();tick(&d,&t,10_250);
        let view=crate::partial_exit_stage_contract::report(&d,&t,"demo",10_250).unwrap();
        assert_eq!(view["status"],"research_contracts_no_broker_commands");
        assert_eq!(view["rows"].as_array().unwrap().len(),1);
        let proposed=&view["rows"][0];
        assert_eq!(proposed["tp_stage"],1);
        assert_eq!(proposed["tp_target_price"],2001.0);
        assert_eq!(proposed["lots"],0.05);
        assert_eq!(proposed["expected_position_volume"],0.1);
        assert_eq!(proposed["expected_position_entry"],1999.5);
        assert_eq!(proposed["expected_broker_tp"],0.0);
        assert_eq!(proposed["authorized"],false);
        assert_eq!(proposed["orders_enabled"],false);
        assert!(proposed.get("id").is_none());
        assert!(proposed.get("kind").is_none());
        assert!(proposed["broker_command_id"].is_null());
        assert_eq!(d.query_row("SELECT COUNT(*) FROM broker_commands",[],
            |r|r.get::<_,u32>(0)).unwrap(),0);
        assert_eq!(crate::partial_exit_stage_contract::report(&d,&t,"demo",10_250)
            .unwrap(),view,"A report must not generate a unique order ID or write");
        let mut changed=t.clone();
        changed.positions[0].price_open=1998.0;
        let bad=crate::partial_exit_stage_contract::report(&d,&changed,"demo",10_250)
            .unwrap();
        assert!(bad["rows"].as_array().unwrap().is_empty());
        changed=t.clone();changed.tick_age_ms=Some(5_000);
        assert!(crate::partial_exit_stage_contract::report(&d,&changed,"demo",10_250)
            .unwrap()["rows"].as_array().unwrap().is_empty());
        assert!(crate::partial_exit_stage_contract::report(&d,&t,"live",10_250)
            .unwrap()["rows"].as_array().unwrap().is_empty());
    }
    #[test]
    fn selected_live_account_has_complete_research_tp1_contract_without_sending(){
        let db=Connection::open_in_memory().unwrap();
        setup(&db);
        let mut live=telemetry();
        live.demo=false;live.account_trade_mode=Some(2);
        crate::partial_exit_ledger::observe(&db,&live,"live",100).unwrap();
        crate::partial_exit_stage::observe(&db,&live,"live",100).unwrap();
        let plans=crate::partial_exit_stage::report(&db,&live,"live").unwrap();
        assert_eq!(plans["rows"].as_array().unwrap().len(),3);
        let ids=std::array::from_fn(|i|
            plans["rows"][i]["plan_id"].as_str().unwrap().to_string());
        crate::partial_exit_prices::save(&db,&live,"live",907,
            &ids,[2001.,2002.,2003.],10_000).unwrap();
        crate::partial_exit_stage_consent::init(&db).unwrap();
        let opted=crate::partial_exit_stage_consent::set(
            &db,&live,"live",907,&ids,true,7,10_000).unwrap();
        assert_eq!(opted["operator_approved"],true);
        assert_eq!(opted["orders_enabled"],false);
        assert_eq!(crate::partial_exit_stage_consent::report(
            &db,&live,"live").unwrap()["rows"][0]["state"],
            "operator_approved_not_dispatched");
        assert!(crate::partial_exit_stage_consent::report(
            &db,&telemetry(),"demo").unwrap()["rows"]
            .as_array().unwrap().is_empty());
        assert!(crate::partial_exit_stage_consent::set(
            &db,&live,"live",907,&ids,true,8,10_000).is_err(),
            "A changed Core revision must never silently inherit consent");
        assert_eq!(crate::partial_exit_stage_consent::set(
            &db,&live,"live",907,&ids,false,7,10_000).unwrap()
            ["operator_approved"],false);
        assert_eq!(crate::partial_exit_stage_consent::report(
            &db,&live,"live").unwrap()["rows"][0]["state"],
            "operator_disabled");
        live.bid=Some(2001.2);live.ask=Some(2001.4);
        live.quote_time_ms=Some(10_200);
        live.source_quote_time_ms=Some(10_200);
        crate::partial_exit_shadow_trigger::observe(&db,&live,"live",10_250).unwrap();
        observe(&db,&live,"live",10_250).unwrap();
        let intents=report(&db,&live,"live").unwrap();
        assert_eq!(intents["target"],"live");
        assert_eq!(intents["rows"].as_array().unwrap().len(),1);
        assert_eq!(intents["rows"][0]["state"],"shadow_close_intent_only");
        let contract=crate::partial_exit_stage_contract::report(
            &db,&live,"live",10_250).unwrap();
        assert_eq!(contract["rows"].as_array().unwrap().len(),1);
        assert_eq!(contract["rows"][0]["target"],"live");
        assert_eq!(contract["rows"][0]["tp_stage"],1);
        assert_eq!(contract["rows"][0]["tp_target_price"],2001.0);
        assert_eq!(contract["rows"][0]["orders_enabled"],false);
        assert!(contract["rows"][0].get("id").is_none());
        assert!(contract["rows"][0].get("kind").is_none());
        let shaped=crate::partial_exit_stage_producer::report(
            &db,&live,"live",10_250,7,20,true).unwrap();
        assert_eq!(shaped["status"],"broker_command_shapes_not_queued");
        assert_eq!(shaped["target"],"live");
        assert_eq!(shaped["broker_commands_created"],0);
        assert_eq!(shaped["orders_enabled"],false);
        assert_eq!(shaped["rows"].as_array().unwrap().len(),1);
        let command=&shaped["rows"][0]["broker_command_draft"];
        assert_eq!(command["kind"],"close");
        assert_eq!(command["target"],"live");
        assert_eq!(command["position"],87);
        assert_eq!(command["lots"],0.05);
        assert_eq!(command["revision"],7);
        assert_eq!(command["tp_stage"],1);
        assert_eq!(command["tp_stage_plan_id"],ids[0]);
        assert_eq!(command["expected_position_identifier"],907);
        assert_eq!(command["expected_position_volume"],0.1);
        assert_eq!(command["expected_position_entry"],1999.5);
        assert_eq!(command["expected_broker_tp"],0.0);
        assert_eq!(command["tp_target_price"],2001.0);
        crate::partial_exit_stage_reservation::init(&db).unwrap();
        let first=crate::partial_exit_stage_reservation::prepare(
            &db,&live,"live",10_250,7,20,true).unwrap();
        assert_eq!(first["prepared"],1);
        assert_eq!(first["idempotent"],0);
        assert_eq!(first["broker_commands_created"],0);
        let duplicate=crate::partial_exit_stage_reservation::prepare(
            &db,&live,"live",10_250,7,20,true).unwrap();
        assert_eq!(duplicate["prepared"],0);
        assert_eq!(duplicate["idempotent"],1);
        let reserved=crate::partial_exit_stage_reservation::report(
            &db,&live,"live").unwrap();
        assert_eq!(reserved["rows"].as_array().unwrap().len(),1);
        assert_eq!(reserved["rows"][0]["state"],"reserved_only_no_send");
        assert_eq!(reserved["rows"][0]["target_price"],2001.0);
        assert_eq!(reserved["rows"][0]["lots"],0.05);
        assert_eq!(reserved["rows"][0]["plan_id"],ids[0]);
        assert_eq!(reserved["orders_enabled"],false);
        let mut moved=live.clone();
        moved.source_quote_time_ms=Some(10_350);
        moved.quote_time_ms=Some(10_350);
        moved.bid=Some(2001.3);
        assert_eq!(crate::partial_exit_stage_reservation::prepare(
            &db,&moved,"live",10_400,7,20,true).unwrap()["idempotent"],1);
        assert_eq!(crate::partial_exit_stage_reservation::prepare(
            &db,&moved,"live",10_400,8,20,true).unwrap()["conflicting"],1,
            "A changed binding revision cannot rewrite an earlier stage intent");
        let original=db.query_row(
            "SELECT payload FROM partial_exit_stage_reservations
             WHERE position_identifier=907 AND stage=1",[],|r|r.get::<_,String>(0)).unwrap();
        let kept:Value=serde_json::from_str(&original).unwrap();
        assert_eq!(kept["revision"],7);
        let other=crate::partial_exit_stage_reservation::report(
            &db,&telemetry(),"demo").unwrap();
        assert!(other["rows"].as_array().unwrap().is_empty());
        assert!(command.get("id").is_none());
        assert!(command.get("state").is_none());
        assert_eq!(crate::partial_exit_stage_producer::report(
            &db,&telemetry(),"demo",10_250,7,20,true).unwrap()
            ["rows"].as_array().unwrap().len(),0);
        assert!(report(&db,&telemetry(),"demo").unwrap()["rows"]
            .as_array().unwrap().is_empty());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
            |r|r.get::<_,i64>(0)).unwrap(),0);
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
        changed=t.clone();changed.positions[0].tp=2050.;
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
    fn tampered_size_even_with_matching_payload_is_rejected_against_original_stage(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let t=crossed();tick(&db,&t,10_250);
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["state"],
            "shadow_close_intent_only");
        db.execute("UPDATE partial_exit_shadow_close_intents
            SET lot_volume=0.09,
                payload=json_set(payload,'$.close_lots',0.09,
                    '$.expected_remaining_volume',0.01)",[]).unwrap();
        assert_eq!(report(&db,&t,"demo").unwrap()["rows"][0]["state"],
            "needs_review");
    }
    #[test]
    fn corrupt_or_unattributable_prior_broker_close_blocks_research_candidate(){
        for raw in ["invalid-json","{}",r#"{"position":"unexpected"}"#] {
            let db=Connection::open_in_memory().unwrap();setup(&db);
            let t=crossed();
            db.execute("INSERT INTO broker_commands VALUES(?1,'close',?2)",
                params![context(&t),raw]).unwrap();
            tick(&db,&t,10_250);
            assert_eq!(count(&db),0,"ambiguous close record: {raw}");
        }
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
            let account=report(&db,&other,"live").unwrap();
            assert_eq!(account["status"],"research_close_intents_no_broker_commands");
            assert!(account["rows"].as_array().unwrap().is_empty());
            assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
                |r|r.get::<_,u32>(0)).unwrap(),0);
        }
        let _=std::fs::remove_file(path);
    }
}
