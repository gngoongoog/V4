//! Read-only audit of a TP stage -> durable Core command -> exact MT5 order
//! -> immutable broker Deal tickets. This does NOT queue, send or advance TP.
//! A plan, an ACK or matching exit volume alone is never proof of TP execution.
use anyhow::Result;
use braintrade_domain::{execution,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use std::collections::BTreeSet;

fn ctx(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn bound(t:&Telemetry,target:&str)->bool{
    t.connected&&t.validate()&&t.login>0&&!t.server.is_empty()&&
    !t.symbol.is_empty()&&!t.currency.is_empty()&&
    matches!((target,t.demo,t.account_trade_mode),
        ("demo",true,Some(0))|("live",false,Some(2)))
}
fn positive(v:&Value)->Option<f64>{
    v.as_f64().filter(|n|n.is_finite()&&*n>0.)
}
fn close_to(a:f64,b:f64,step:f64)->bool{
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
// Exact MT5 Order+Deal matching is not sufficient evidence that the order
// belongs to the operator's *immutable original price ladder*. Require
// all three stage IDs, captured quote geometry, original position entry/TP,
// and the intended stage price. Historical proof is read-only and fail-closed.
fn price_plan_matches(db:&Connection,t:&Telemetry,target:&str,key:&str,
    pid:u64,stage:u32,plan:&str,ticket:u64,magic:u64,
    initial:f64,step:f64,cmd:&Value)->Result<bool>
{
    if !matches!((target,t.demo,t.account_trade_mode),
        ("demo",true,Some(0))|("live",false,Some(2)))
        ||!(1..=3).contains(&stage){return Ok(false)}
    let saved:Option<(String,String,String,u64,u64,u64)>=db.query_row(
        "SELECT plan_ids_json,prices_json,captured_json,ticket,magic,saved_ms
         FROM partial_exit_price_plans
         WHERE context=?1 AND target=?2 AND position_identifier=?3",
        params![key,target,pid],|r|Ok((
            r.get(0)?,r.get(1)?,r.get(2)?,
            r.get(3)?,r.get(4)?,r.get(5)?
        ))
    ).optional()?;
    let Some((raw_ids,raw_prices,raw_capture,stored_ticket,stored_magic,saved_ms))=saved
        else{return Ok(false)};
    let ids=serde_json::from_str::<Value>(&raw_ids).unwrap_or(Value::Null);
    let prices=serde_json::from_str::<Value>(&raw_prices).unwrap_or(Value::Null);
    let cap=serde_json::from_str::<Value>(&raw_capture).unwrap_or(Value::Null);
    let Some(id_array)=ids.as_array().filter(|a|a.len()==3)else{return Ok(false)};
    let Some(level_array)=prices.as_array().filter(|a|a.len()==3)else{return Ok(false)};
    let mut st=db.prepare(
        "SELECT stage,plan_id FROM partial_exit_stage_plans
         WHERE context=?1 AND target=?2 AND position_identifier=?3 ORDER BY stage")?;
    let stages=st.query_map(params![key,target,pid],|r|Ok((
        r.get::<_,u32>(0)?,r.get::<_,String>(1)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if stages.len()!=3 || stages.iter().enumerate().any(|(i,(n,id))|
        *n!=(i+1) as u32||id_array[i]!=*id){return Ok(false)}
    let tick=cap["tick_size"].as_f64().unwrap_or(f64::NAN);
    let entry=cap["entry"].as_f64().unwrap_or(f64::NAN);
    let full_tp=cap["original_broker_tp"].as_f64().unwrap_or(f64::NAN);
    let level=level_array[(stage-1) as usize].as_f64().unwrap_or(f64::NAN);
    Ok(crate::partial_exit_prices::capture_geometry_valid(&cap,&prices)
        &&id_array[(stage-1) as usize]==plan
        &&stored_ticket==ticket&&stored_magic==magic
        &&saved_ms>0&&cmd["created_ms"].as_u64().is_some_and(|ms|ms>=saved_ms)
        &&cap["schema"]=="partial-exit-price-capture-v1"
        &&cap["context"]==key&&cap["target"]==target
        &&cap["position_identifier"].as_u64()==Some(pid)
        &&cap["ticket"].as_u64()==Some(ticket)
        &&cap["magic"].as_u64()==Some(magic)
        &&cap["symbol"]==t.symbol
        &&cap["saved_ms"].as_u64()==Some(saved_ms)
        &&cap["side"]==cmd["expected_position_side"]
        &&cap["orders_enabled"]==false&&cap["breakeven_enabled"]==false
        &&close_to(cap["original_volume"].as_f64().unwrap_or(f64::NAN),initial,step)
        &&close_to(cap["lot_step"].as_f64().unwrap_or(f64::NAN),step,step)
        &&close_to(cmd["tp_target_price"].as_f64().unwrap_or(f64::NAN),level,tick)
        &&close_to(cmd["expected_position_entry"].as_f64().unwrap_or(f64::NAN),entry,tick)
        &&close_to(cmd["expected_broker_tp"].as_f64().unwrap_or(f64::NAN),full_tp,tick))
}
fn evaluate(db:&Connection,t:&Telemetry,target:&str,key:&str,pid:u64,stage:u32,
    plan:&str,plan_lots:f64,draft:&Value,ticket:u64,magic:u64,
    initial:f64,step:f64)->Result<Value>
{
    let prior:f64=db.query_row(
        "SELECT COALESCE(SUM(planned_volume),0) FROM partial_exit_stage_plans
         WHERE context=?1 AND target=?2 AND position_identifier=?3 AND stage<?4",
        params![key,target,pid,stage],|r|r.get(0))?;
    let expected=initial-prior;
    if ticket==0||magic==0||!close_to(draft["entry_volume"].as_f64().unwrap_or(f64::NAN),initial,step)
       ||draft["schema"]!="partial-exit-draft-v1"||
       draft["ticket"].as_u64()!=Some(ticket)||draft["position_identifier"].as_u64()!=Some(pid)
       ||draft["magic"].as_u64()!=Some(magic)||draft["symbol"]!=t.symbol||
       draft["broker_orders_created"]!=false||!positive(&json!(plan_lots)).is_some()||
       !close_to(draft["volume_step"].as_f64().unwrap_or(f64::NAN),step,step)
    {
        return Ok(json!({"state":"invalid_original_plan"}))
    }
    let side=match draft["side"].as_str(){Some("BUY")=>0,Some("SELL")=>1,
        _=>return Ok(json!({"state":"invalid_original_side"}))};
    let mut statement=db.prepare(
        "SELECT id,state,payload,result FROM broker_commands
         WHERE context=?1 AND kind='close' AND json_valid(payload)
         AND json_extract(payload,'$.tp_stage_plan_id')=?2
         ORDER BY created_ms,id LIMIT 3")?;
    let commands=statement.query_map(params![key,plan],|r|Ok((
        r.get::<_,String>(0)?,r.get::<_,String>(1)?,
        r.get::<_,String>(2)?,r.get::<_,String>(3)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if commands.is_empty(){return Ok(json!({"state":"no_core_stage_command"}))}
    if commands.len()!=1 {return Ok(json!({"state":"conflicting_stage_commands"}))}
    let (id,command_state,raw,raw_result)=&commands[0];
    let Ok(cmd)=serde_json::from_str::<Value>(raw) else{
        return Ok(json!({"state":"corrupt_core_command"}))
    };
    let owner=execution::brain_for_magic(magic);
    if owner.is_none()||cmd["id"]!=*id||cmd["kind"]!="close"||
        cmd["context"]!=key||cmd["target"]!=target||
        cmd["login"].as_u64()!=Some(t.login)||cmd["server"]!=t.server||
        cmd["currency"]!=t.currency||cmd["symbol"]!=t.symbol||
        cmd["magic"].as_u64()!=Some(magic)||cmd["brain"].as_str()!=owner||
        cmd["tp_stage_plan_id"]!=plan||cmd["tp_stage"].as_u64()!=Some(stage as u64)||
        cmd["position"].as_u64()!=Some(ticket)||
        cmd["expected_position_identifier"].as_u64()!=Some(pid)||
        cmd["expected_position_side"]!=draft["side"]||
        !cmd["created_ms"].as_u64().is_some_and(|ms|ms>0)||
        !close_to(cmd["lots"].as_f64().unwrap_or(f64::NAN),plan_lots,step)||
        !close_to(cmd["expected_position_volume"].as_f64().unwrap_or(f64::NAN),expected,step)
    {return Ok(json!({"state":"command_identity_mismatch"}))}
    if !price_plan_matches(db,t,target,key,pid,stage,plan,ticket,magic,
        initial,step,&cmd)?{
        return Ok(json!({"state":"original_tp_price_lineage_not_verified"}))
    }
    if !matches!(command_state.as_str(),"filled"|"partial"|"observed"){
        return Ok(json!({"state":"command_not_broker_reconciled"}))
    }
    let Ok(receipt)=serde_json::from_str::<Value>(raw_result) else{
        return Ok(json!({"state":"corrupt_gateway_receipt"}))
    };
    if receipt["recovered_from_history"]!=true||receipt["state"]!="filled"||
        !close_to(receipt["volume"].as_f64().unwrap_or(f64::NAN),plan_lots,step)
    {return Ok(json!({"state":"gateway_history_proof_missing"}))}
    let Some(order)=receipt["order"].as_u64().filter(|n|*n>0) else {
        return Ok(json!({"state":"gateway_order_ticket_missing"}))
    };
    if receipt["position_id"].as_u64()!=Some(pid)||
       !close_to(receipt["request"]["volume"].as_f64().unwrap_or(f64::NAN),plan_lots,step)||
       receipt["request"]["position"].as_u64()!=Some(ticket)||
       receipt["request"]["magic"].as_u64()!=Some(magic)||
       receipt["request"]["symbol"]!=t.symbol||
       receipt["request"]["type"].as_i64()!=Some(1-side)
    {return Ok(json!({"state":"gateway_request_not_matching_stage"}))}
    let other_claims:u64=db.query_row(
        "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND id<>?2
         AND kind='close'
         AND CAST(CASE WHEN json_valid(result) THEN json_extract(result,'$.order') ELSE NULL END AS INTEGER)=?3
         AND CASE WHEN json_valid(payload) THEN json_extract(payload,'$.tp_stage_plan_id') ELSE NULL END IS NOT NULL",
        params![key,id,order],|r|r.get(0))?;
    if other_claims>0{return Ok(json!({"state":"broker_order_claimed_by_multiple_stages"}))}
    let Some(tickets)=receipt["deals"].as_array().filter(|v|!v.is_empty()&&v.len()<=50) else{
        return Ok(json!({"state":"gateway_deal_ticket_list_missing"}))
    };
    let watermark:rusqlite::Result<(u64,u64)>=db.query_row(
        "SELECT until_ms,synced_ms FROM broker_history_sync
         WHERE context=?1 AND target=?2",params![key,target],
        |r|Ok((r.get(0)?,r.get(1)?)));
    let (until,synced)=match watermark{
        Ok(x) if x.0>0&&x.1>0=>x,_=>return Ok(json!({"state":"broker_deal_history_not_synced"}))
    };
    let Some(primary_deal)=receipt["deal"].as_u64().filter(|n|*n>0) else{
        return Ok(json!({"state":"gateway_primary_deal_ticket_missing"}))
    };
    if !tickets.iter().any(|d|d.as_u64()==Some(primary_deal)){
        return Ok(json!({"state":"gateway_primary_deal_not_in_fill_batch"}))
    }
    let mut seen=BTreeSet::new();
    let mut booked=0.0_f64;
    let mut lots=0.0_f64;
    for ticket_id in tickets{
        let Some(deal_ticket)=ticket_id.as_u64().filter(|n|*n>0) else{
            return Ok(json!({"state":"invalid_gateway_deal_ticket"}))
        };
        if !seen.insert(deal_ticket){return Ok(json!({"state":"duplicate_gateway_deal_ticket"}))}
        let deal:rusqlite::Result<String>=db.query_row(
            "SELECT payload FROM broker_deals
             WHERE context=?1 AND position_id=?2 AND ticket=?3",
            params![key,pid,deal_ticket],|r|r.get(0));
        let Ok(raw)=deal else{return Ok(json!({"state":"matching_broker_deal_not_yet_synced"}))};
        let Ok(d)=serde_json::from_str::<Value>(&raw) else{
            return Ok(json!({"state":"invalid_matching_broker_deal"}))
        };
        if d["ticket"].as_u64()!=Some(deal_ticket)||
           d["order"].as_u64()!=Some(order)||d["position_id"].as_u64()!=Some(pid)||
           d["symbol"]!=t.symbol||
           // Staged TP uses a normal DEAL close. OUT_BY (3) is a
           // separate broker close-by operation and must not be attributed
           // to this exact market-close stage even with matching order ID.
           d["entry"].as_i64()!=Some(1)||
           d["type"].as_i64()!=Some(1-side)||
           !d["magic"].as_u64().is_some_and(|n|n==0||n==magic)||
           !d["time_msc"].as_u64().is_some_and(|n|
               n>0&&n<=until.saturating_add(2000)&&
               n.saturating_add(2000)>=cmd["created_ms"].as_u64().unwrap_or(u64::MAX))||
           !d["price"].as_f64().is_some_and(|p|p.is_finite()&&p>0.)
        {return Ok(json!({"state":"broker_deal_order_or_identity_mismatch"}))}
        let Some(v)=positive(&d["volume"])else{return Ok(json!({"state":"invalid_broker_fill_volume"}))};
        lots+=v;
        for name in ["profit","commission","swap","fee"]{
            let Some(value)=d[name].as_f64().filter(|v|v.is_finite())else{
                return Ok(json!({"state":"broker_booked_costs_missing"}))
            };
            booked+=value;
        }
        if !lots.is_finite()||!booked.is_finite(){
            return Ok(json!({"state":"invalid_broker_totals"}))
        }
    }
    if !close_to(lots,plan_lots,step){
        return Ok(json!({"state":"broker_fill_lots_do_not_cover_stage"}))
    }
    Ok(json!({
        "state":"broker_order_deals_exactly_linked",
        "core_command_id":id,"broker_order_ticket":order,
        "broker_deal_tickets":seen,
        "matched_broker_lots":lots,"booked_net_account_currency":booked,
        "broker_history_until_ms":until,"broker_history_synced_ms":synced,
        "stage_completed":false,"orders_enabled":false,
        "note":"Historical proof is read-only; it never advances stage sequence, enables another stage or triggers breakeven."
    }))
}

/// No stage transitions or broker sends. Exact one-command, one-order and
/// persisted Deal proof is independently visible for future reconciliation.
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    if !bound(t,target) {
        return Ok(json!({"schema":"partial-exit-stage-command-links-v1",
            "status":"unverified_account","read_only":true,
            "orders_enabled":false,"rows":[]}))
    }
    let key=ctx(t);
    let mut statement=db.prepare(
        "SELECT s.position_identifier,s.stage,s.plan_id,s.planned_volume,s.status,
                d.ticket,d.magic,d.initial_volume,d.step,d.payload
         FROM partial_exit_stage_plans s
         LEFT JOIN partial_exit_drafts d ON d.context=s.context AND d.target=s.target
            AND d.position_identifier=s.position_identifier
         WHERE s.context=?1 AND s.target=?2
         ORDER BY s.created_ms DESC,s.position_identifier DESC,s.stage ASC LIMIT 75")?;
    let stages=statement.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u32>(1)?,r.get::<_,String>(2)?,
        r.get::<_,f64>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<u64>>(5)?,
        r.get::<_,Option<u64>>(6)?,r.get::<_,Option<f64>>(7)?,
        r.get::<_,Option<f64>>(8)?,r.get::<_,Option<String>>(9)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let mut rows=Vec::new();
    for (pid,stage,plan,lots,state,ticket,magic,initial,step,raw) in stages{
        let evidence=if let (Some(ticket),Some(magic),Some(initial),Some(step),Some(raw))=
            (ticket,magic,initial,step,raw){
            let draft=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
            evaluate(db,t,target,&key,pid,stage,&plan,lots,&draft,
                ticket,magic,initial,step)?
        } else {json!({"state":"missing_original_draft"})};
        rows.push(json!({"position_identifier":pid,"stage":stage,"plan_id":plan,
            "planned_volume":lots,"plan_state":state,"evidence":evidence}));
    }
    Ok(json!({"schema":"partial-exit-stage-command-links-v1",
        "status":"broker_stage_links_read_only","context":key,"target":target,
        "read_only":true,"orders_enabled":false,"stage_execution_supported":false,
        "rows":rows}))
}

#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    fn sample_telemetry()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),login:222,
            server:"DemoA".into(),currency:"USD".into(),symbol:"XAUUSDm".into(),
            volume_min:Some(0.01),volume_step:Some(0.01),volume_max:Some(100.),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.10,
                ..Default::default()}],..Default::default()
        }
    }
    fn db()->Connection{
        let d=Connection::open_in_memory().unwrap();
        crate::partial_exit_ledger::init(&d).unwrap();
        crate::partial_exit_stage::init(&d).unwrap();
        crate::partial_exit_prices::init(&d).unwrap();
        d.execute_batch("CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT,
            brain TEXT,kind TEXT,created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
            CREATE TABLE broker_deals(context TEXT,ticket INTEGER,position_id INTEGER,
                brain TEXT,payload TEXT,PRIMARY KEY(context,ticket));
            CREATE TABLE broker_history_sync(context TEXT,target TEXT,until_ms INTEGER,
                synced_ms INTEGER,PRIMARY KEY(context,target));").unwrap();
        let tt=sample_telemetry();
        crate::partial_exit_ledger::observe(&d,&tt,"demo",100).unwrap();
        crate::partial_exit_stage::observe(&d,&tt,"demo",100).unwrap();
        let plans=crate::partial_exit_stage::report(&d,&tt,"demo").unwrap();
        let ids=(0..3).map(|i|plans["rows"][i]["plan_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let prices=json!([2000.3,2000.6,2000.9]);
        let captured=json!({"schema":"partial-exit-price-capture-v1",
            "context":ctx(&tt),"target":"demo","position_identifier":907,
            "ticket":87,"magic":410601,"side":"BUY","symbol":"XAUUSDm",
            "entry":1999.5,"original_volume":0.10,"lot_step":0.01,
            "tick_size":0.01,"quote_reference":2000.0,
            "saved_bid":2000.0,"saved_ask":2000.1,
            "original_broker_tp":0.0,"saved_ms":110,
            "orders_enabled":false,"breakeven_enabled":false});
        d.execute("INSERT INTO partial_exit_price_plans VALUES(?1,'demo',907,87,
            410601,?2,?3,?4,110)",
            params![ctx(&tt),json!(ids).to_string(),
                prices.to_string(),captured.to_string()]).unwrap();
        d
    }
    fn seed(d:&Connection,t:&Telemetry){
        let v=crate::partial_exit_stage::report(d,t,"demo").unwrap();
        let plan=v["rows"][0]["plan_id"].as_str().unwrap();
        let command=json!({"id":"core-close-1","kind":"close","context":ctx(t),
            "target":"demo","login":222,"server":"DemoA","currency":"USD",
            "symbol":"XAUUSDm","brain":"gann","magic":410601,
            "position":87,"tp_stage":1,"tp_stage_plan_id":plan,
            "lots":0.05,"created_ms":120,"expected_position_identifier":907,
            "expected_position_volume":0.10,"expected_position_side":"BUY",
            "tp_target_price":2000.3,"expected_position_entry":1999.5,
            "expected_broker_tp":0.0});
        let receipt=json!({"state":"filled","recovered_from_history":true,
            "volume":0.05,"position_id":907,"order":777,"deal":901,"deals":[901],
            "request":{"volume":0.05,"position":87,"type":1,
                "magic":410601,"symbol":"XAUUSDm"}});
        d.execute("INSERT INTO broker_commands VALUES (?1,?2,'gann','close',120,'filled',?3,?4)",
            params!["core-close-1",ctx(t),command.to_string(),receipt.to_string()]).unwrap();
        d.execute("INSERT INTO broker_history_sync VALUES (?1,'demo',1000,1001)",
            [ctx(t)]).unwrap();
        let deal=json!({"ticket":901,"order":777,"position_id":907,
            "symbol":"XAUUSDm","magic":410601,"entry":1,"type":1,
            "time_msc":200,"volume":0.05,"price":2000.0,"profit":1.0,
            "commission":-0.03,"swap":0.0,"fee":0.0});
        d.execute("INSERT INTO broker_deals VALUES (?1,901,907,'gann',?2)",
            params![ctx(t),deal.to_string()]).unwrap();
    }
    fn e(d:&Connection,t:&Telemetry)->Value{
        report(d,t,"demo").unwrap()["rows"][0]["evidence"].clone()
    }
    #[test]
    fn exact_original_command_and_mt5_order_and_deals_can_be_audited_without_executing(){
        let d=db();let t=sample_telemetry();
        assert_eq!(e(&d,&t)["state"],"no_core_stage_command");
        seed(&d,&t);
        let x=e(&d,&t);
        assert_eq!(x["state"],"broker_order_deals_exactly_linked");
        assert_eq!(x["matched_broker_lots"],0.05);
        assert_eq!(x["booked_net_account_currency"],0.97);
        assert_eq!(x["stage_completed"],false);
        assert_eq!(report(&d,&t,"demo").unwrap()["orders_enabled"],false);
        assert_eq!(e(&d,&t),x,"Repeated audits cannot mutate broker state");
    }
    #[test]
    fn command_and_saved_original_tp_ladder_must_be_identical(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        assert_eq!(e(&d,&t)["state"],"broker_order_deals_exactly_linked");
        for field in ["tp_target_price","expected_position_entry","expected_broker_tp"] {
            let original:String=d.query_row(
                "SELECT payload FROM broker_commands WHERE id='core-close-1'",[],
                |r|r.get(0)).unwrap();
            d.execute("UPDATE broker_commands SET payload=json_set(payload,?1,2999.0)
                       WHERE id='core-close-1'",
                [format!("$.{field}")]).unwrap();
            assert_eq!(e(&d,&t)["state"],"original_tp_price_lineage_not_verified",
                "Mutated {field} cannot count as original stage execution");
            d.execute("UPDATE broker_commands SET payload=?1
                WHERE id='core-close-1'",[original]).unwrap();
        }
        let raw:String=d.query_row(
            "SELECT prices_json FROM partial_exit_price_plans",[],|r|r.get(0)).unwrap();
        d.execute("UPDATE partial_exit_price_plans
            SET prices_json=json_set(prices_json,'$[0]',2000.4)",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"original_tp_price_lineage_not_verified");
        d.execute("UPDATE partial_exit_price_plans SET prices_json=?1",[raw]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_order_deals_exactly_linked");
        d.execute("DELETE FROM partial_exit_price_plans",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"original_tp_price_lineage_not_verified");
    }
    #[test]
    fn ack_only_wrong_order_or_missing_deal_cannot_verify_stage(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        d.execute("UPDATE broker_commands SET result=json_set(result,'$.recovered_from_history',false)",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"gateway_history_proof_missing");
        d.execute("UPDATE broker_commands SET result=json_set(result,'$.recovered_from_history',json('true'))",[]).unwrap();
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.order',999)",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_deal_order_or_identity_mismatch");
        d.execute("DELETE FROM broker_deals",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"matching_broker_deal_not_yet_synced");
    }
    #[test]
    fn close_by_deal_never_counts_as_ordinary_staged_tp_market_close(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        assert_eq!(e(&d,&t)["state"],"broker_order_deals_exactly_linked");
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.entry',3)",
            []).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_deal_order_or_identity_mismatch");
        // A normal MT5 DEAL_ENTRY_OUT receipt restores historical linkage.
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.entry',1)",
            []).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_order_deals_exactly_linked");
    }
    #[test]
    fn missing_deal_price_wrong_primary_ticket_and_ancient_order_are_rejected(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        d.execute("UPDATE broker_deals SET payload=json_remove(payload,'$.price')",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_deal_order_or_identity_mismatch");
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.price',2000.0)",[]).unwrap();
        d.execute("UPDATE broker_commands SET result=json_set(result,'$.deal',999)",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"gateway_primary_deal_not_in_fill_batch");
        d.execute("UPDATE broker_commands SET result=json_set(result,'$.deal',901)",[]).unwrap();
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.time_msc',1)",[]).unwrap();
        d.execute("UPDATE broker_commands SET payload=json_set(payload,'$.created_ms',10000)",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_deal_order_or_identity_mismatch");
    }
    #[test]
    fn altered_core_command_or_competing_command_cannot_claim_tp(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        let original:String=d.query_row("SELECT payload FROM broker_commands",[],|r|r.get(0)).unwrap();
        d.execute("UPDATE broker_commands SET payload=json_set(payload,'$.lots',0.06)",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"command_identity_mismatch");
        d.execute("UPDATE broker_commands SET payload=?1",[original.clone()]).unwrap();
        let plan=serde_json::from_str::<Value>(&original).unwrap()["tp_stage_plan_id"].as_str().unwrap().to_string();
        let other=json!({"tp_stage_plan_id":plan,"kind":"close"});
        d.execute("INSERT INTO broker_commands VALUES ('other',?1,'gann','close',130,'filled',?2,'{}')",
            params![ctx(&t),other.to_string()]).unwrap();
        assert_eq!(e(&d,&t)["state"],"conflicting_stage_commands");
    }
    #[test]
    fn broker_volume_and_duplicate_ticket_proof_are_not_inferred(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        d.execute("UPDATE broker_commands SET result=json_set(result,'$.deals',json_array(901,901))",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"duplicate_gateway_deal_ticket");
        d.execute("UPDATE broker_commands SET result=json_set(result,'$.deals',json_array(901))",[]).unwrap();
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.volume',0.04)",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_fill_lots_do_not_cover_stage");
    }
    #[test]
    fn corrupt_unrelated_command_journal_does_not_crash_stage_proof(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        d.execute("INSERT INTO broker_commands VALUES ('bad',?1,'gann','close',140,'unknown',
            'not valid json','not valid json')",[ctx(&t)]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_order_deals_exactly_linked");
    }
    #[test]
    fn older_broker_history_without_order_id_cannot_be_claimed_as_tp_proof(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        d.execute("UPDATE broker_deals SET payload=json_remove(payload,'$.order')",[]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_deal_order_or_identity_mismatch");
    }
    #[test]
    fn two_stages_cannot_claim_the_same_mt5_order(){
        let d=db();let t=sample_telemetry();seed(&d,&t);
        let plan=crate::partial_exit_stage::report(&d,&t,"demo").unwrap()["rows"][1]["plan_id"]
            .as_str().unwrap().to_string();
        let mut cmd:Value=serde_json::from_str(&d.query_row(
            "SELECT payload FROM broker_commands WHERE id='core-close-1'",[],
            |r|r.get::<_,String>(0)).unwrap()).unwrap();
        cmd["id"]=json!("second-command");
        cmd["tp_stage_plan_id"]=json!(plan);
        cmd["tp_stage"]=json!(2);
        let result:String=d.query_row(
            "SELECT result FROM broker_commands WHERE id='core-close-1'",[],
            |r|r.get(0)).unwrap();
        d.execute("INSERT INTO broker_commands VALUES ('second-command',?1,'gann',
            'close',140,'filled',?2,?3)",
            params![ctx(&t),cmd.to_string(),result]).unwrap();
        assert_eq!(e(&d,&t)["state"],"broker_order_claimed_by_multiple_stages");
    }
    #[test]
    fn foreign_account_or_real_mode_does_not_inherit_stage_proof(){
        let d=db();let mut t=sample_telemetry();seed(&d,&t);
        t.login+=1;
        assert!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
        t.connected=false;
        assert_eq!(report(&d,&t,"demo").unwrap()["status"],"unverified_account");
        t=sample_telemetry();
        t.demo=false;t.account_trade_mode=Some(2);
        assert!(report(&d,&t,"live").unwrap()["rows"].as_array().unwrap().is_empty());
    }
}
