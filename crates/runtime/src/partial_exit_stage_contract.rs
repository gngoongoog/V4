//! Pure Core-owned draft of an MT5 DEMO staged-close command.
//!
//! This read-only projection joins independently verified TP1 shadow evidence
//! or later broker-sealed history with immutable price/volume lineage. It
//! NEVER creates a broker_commands row, order ID, authorization or send.
//! Deliberately omit both `id` and `kind="close"` from the draft: only an
//! as-yet-unimplemented execution producer could perform that conversion.
use anyhow::Result;
use braintrade_domain::{execution,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

fn scope(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn near(a:f64,b:f64,step:f64)->bool {
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
fn grid(v:f64,step:f64)->bool {
    v.is_finite()&&v>0.&&step.is_finite()&&step>0.
        &&(v/step).is_finite()&&((v/step)-(v/step).round()).abs()<=1e-6
}
fn draft(db:&Connection,t:&Telemetry,key:&str,now:u64,
    stage:u32,pid:u64,plan:&str,expected:f64,requested:f64,
    target:f64,source:u64,prior_seal:u64)->Result<Option<Value>>
{
    if !(1..=3).contains(&stage)||pid==0||plan.is_empty()||source==0
        ||source!=t.source_quote_time_ms.unwrap_or(0){return Ok(None)}
    let saved:Option<(u64,u64,String,String,String,u64)>=db.query_row(
        "SELECT ticket,magic,plan_ids_json,prices_json,captured_json,saved_ms
         FROM partial_exit_price_plans
         WHERE context=?1 AND target='demo' AND position_identifier=?2",
        params![key,pid],|r|Ok((
            r.get(0)?,r.get(1)?,r.get(2)?,
            r.get(3)?,r.get(4)?,r.get(5)?
        ))
    ).optional()?;
    let Some((ticket,magic,raw_ids,raw_prices,raw_cap,saved_at))=saved
        else{return Ok(None)};
    let ids=serde_json::from_str::<Value>(&raw_ids).unwrap_or(Value::Null);
    let prices=serde_json::from_str::<Value>(&raw_prices).unwrap_or(Value::Null);
    let cap=serde_json::from_str::<Value>(&raw_cap).unwrap_or(Value::Null);
    let (Some(ids),Some(prices))=(ids.as_array(),prices.as_array())
        else{return Ok(None)};
    if ids.len()!=3||prices.len()!=3||ticket==0||magic==0||saved_at==0
        ||cap["schema"]!="partial-exit-price-capture-v1"
        ||cap["context"]!=key||cap["target"]!="demo"
        ||cap["position_identifier"].as_u64()!=Some(pid)
        ||cap["ticket"].as_u64()!=Some(ticket)
        ||cap["magic"].as_u64()!=Some(magic)
        ||cap["symbol"]!=t.symbol||cap["saved_ms"].as_u64()!=Some(saved_at)
        ||cap["orders_enabled"]!=false||cap["trading_enabled"]!=false
        ||cap["breakeven_enabled"]!=false
        ||ids[(stage-1) as usize]!=plan
        ||!crate::partial_exit_prices::capture_geometry_valid(&cap,&Value::Array(prices.clone()))
    {return Ok(None)}
    let tick=cap["tick_size"].as_f64().unwrap_or(f64::NAN);
    let step=cap["lot_step"].as_f64().unwrap_or(f64::NAN);
    let initial=cap["original_volume"].as_f64().unwrap_or(f64::NAN);
    let entry=cap["entry"].as_f64().unwrap_or(f64::NAN);
    let broker_tp=cap["original_broker_tp"].as_f64().unwrap_or(f64::NAN);
    let price=prices[(stage-1) as usize].as_f64().unwrap_or(f64::NAN);
    let source_saved=cap["broker_quote_ms"].as_u64().unwrap_or(0);
    if !near(target,price,tick)||!near(t.tick_size.unwrap_or(f64::NAN),tick,tick)
        ||!near(t.volume_step.unwrap_or(f64::NAN),step,step)
        ||!initial.is_finite()||initial<=0.
        ||!entry.is_finite()||entry<=0.
        ||!broker_tp.is_finite()||broker_tp<0.
        ||source<=source_saved||source<=saved_at
        ||(stage>1&&(prior_seal==0||source<=prior_seal))
        ||now<=saved_at
    {return Ok(None)}
    let Some(brain)=execution::brain_for_magic(magic)else{return Ok(None)};
    let matching=t.positions.iter().filter(|p|
        p.identifier==pid||p.ticket==ticket).collect::<Vec<_>>();
    if matching.len()!=1{return Ok(None)}
    let p=matching[0];
    if p.identifier!=pid||p.ticket!=ticket||p.magic!=magic
        ||p.symbol!=t.symbol||cap["side"]!=p.side
        ||!matches!(p.side.as_str(),"BUY"|"SELL")
        ||!near(p.price_open,entry,tick)
        ||!near(p.tp,broker_tp,tick)
        ||!near(p.volume,expected,step)
    {return Ok(None)}
    let min=t.volume_min.unwrap_or(f64::NAN);
    let max=t.volume_max.unwrap_or(f64::NAN);
    let after=expected-requested;
    if !min.is_finite()||min<=0.||!max.is_finite()||max<min
        ||!grid(expected,step)||!grid(requested,step)
        ||!near(t.volume_min.unwrap_or(f64::NAN),min,step)
        ||requested<min-step*1e-6||requested>max+step*1e-6
        ||requested>expected+step*1e-6
        ||(stage<3&&(!grid(after,step)||after<min-step*1e-6))
        ||(stage==3&&!near(after,0.,step))
    {return Ok(None)}
    let mut st=db.prepare(
        "SELECT stage,plan_id,planned_volume,status
         FROM partial_exit_stage_plans
         WHERE context=?1 AND target='demo' AND position_identifier=?2
         ORDER BY stage")?;
    let stages=st.query_map(params![key,pid],|r|Ok((
        r.get::<_,u32>(0)?,r.get::<_,String>(1)?,
        r.get::<_,f64>(2)?,r.get::<_,String>(3)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if stages.len()!=3||stages.iter().enumerate().any(|(i,(number,id,volume,state))|
        *number!=(i+1) as u32||ids[i]!=*id||!grid(*volume,step)
        ||*state!=if stage==1 {"prepared_only"}else{"review_required"})
        ||!near(stages[(stage-1) as usize].2,requested,step)
        ||!near(stages.iter().map(|s|s.2).sum::<f64>(),initial,step)
    {return Ok(None)}
    let draft_status:Option<String>=db.query_row(
        "SELECT status FROM partial_exit_drafts
         WHERE context=?1 AND target='demo' AND position_identifier=?2
         AND ticket=?3 AND magic=?4",
        params![key,pid,ticket,magic],|r|r.get(0)
    ).optional()?;
    if draft_status.as_deref()!=Some(if stage==1 {"draft_only"}else{"volume_changed_unverified"})
    {return Ok(None)}
    let current=if p.side=="BUY"{t.bid}else{t.ask};
    let quote=current.unwrap_or(f64::NAN);
    if !quote.is_finite()||quote<=0.||!target.is_finite()||target<=0.
        ||(p.side=="BUY"&&quote<target)||(p.side=="SELL"&&quote>target)
    {return Ok(None)}
    Ok(Some(json!({
        "schema":"tp-stage-core-contract-preview-v1",
        "state":"validated_research_contract_only",
        "context":key,"target":"demo","login":t.login,
        "server":t.server,"currency":t.currency,"symbol":t.symbol,
        "brain":brain,"magic":magic,"position":ticket,
        "tp_stage":stage,"tp_stage_plan_id":plan,
        "expected_position_identifier":pid,"expected_position_side":p.side,
        "expected_position_volume":expected,"lots":requested,
        "expected_position_entry":entry,"expected_broker_tp":broker_tp,
        "tp_target_price":target,"volume_step":step,
        "quote_source_ms":source,"inspected_ms":now,
        "prior_stage_broker_sealed":stage>1,
        "orders_enabled":false,"authorized":false,
        "broker_command_id":Value::Null,"gateway_send_id":Value::Null,
        "broker_order_id":Value::Null,
        "breakeven_enabled":false
    })))
}

/// Read-only Core contract projection. No broker command ID, no 'close' kind,
/// no state transitions, no Gateway access. A price crossing is NOT a fill.
pub fn report(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<Value>{
    let empty=||json!({"schema":"tp-stage-core-contract-preview-v1",
        "status":"unverified_demo_hedging_or_quote",
        "rows":[],"read_only":true,"orders_enabled":false,
        "authorized":false,"breakeven_enabled":false});
    if target!="demo"||!t.connected||!t.demo||t.account_trade_mode!=Some(0)
        ||t.margin_mode!=Some(2)||t.login==0||!t.validate()
        ||t.server.is_empty()||t.symbol.is_empty()||t.currency.is_empty()
        ||!t.quote_is_fresh(now,3_000)
        ||!t.source_quote_time_ms.is_some_and(|ms|ms>0&&ms<=now.saturating_add(2_000))
        ||!t.bid.zip(t.ask).is_some_and(|(b,a)|b.is_finite()&&a.is_finite()&&b>0.&&a>=b)
    {return Ok(empty())}
    let key=scope(t);
    let mut rows=Vec::new();
    let first=crate::partial_exit_shadow_intent::report(db,t,target)?;
    if let Some(candidates)=first["rows"].as_array(){
        for p in candidates.iter().filter(|p|p["state"]=="shadow_close_intent_only"){
            let (Some(pid),Some(plan),Some(expected),Some(lots),Some(source))=(
                p["position_identifier"].as_u64(),p["plan_id"].as_str(),
                p["close_lots"].as_f64().and_then(|lots|{
                    p["expected_remaining_volume"].as_f64().map(|after|lots+after)
                }),p["close_lots"].as_f64(),t.source_quote_time_ms
            )else{continue};
            // Historical TP1 shadow crossing must also remain true against
            // today's executable broker Bid/Ask, not a stale past quote.
            let price_plan:Option<String>=db.query_row(
                "SELECT prices_json FROM partial_exit_price_plans
                 WHERE context=?1 AND target='demo' AND position_identifier=?2",
                params![key,pid],|r|r.get(0)
            ).optional()?;
            let target=price_plan.and_then(|raw|serde_json::from_str::<Value>(&raw).ok())
                .and_then(|v|v[0].as_f64());
            if let Some(target)=target {
                if let Some(row)=draft(db,t,&key,now,1,pid,plan,expected,lots,target,source,0)?{
                    rows.push(row);
                }
            }
        }
    }
    let next=crate::partial_exit_next_shadow::report(db,t,target,now)?;
    if let Some(candidates)=next["rows"].as_array(){
        for p in candidates.iter().filter(|p|
            p["state"]=="historical_prerequisites_met_shadow_only"){
            let (Some(stage),Some(pid),Some(plan),Some(expected),Some(lots),
                Some(target),Some(source),Some(seal))=(
                p["stage"].as_u64().and_then(|v|u32::try_from(v).ok()),
                p["position_identifier"].as_u64(),p["plan_id"].as_str(),
                p["expected_position_volume"].as_f64(),
                p["expected_close_lots"].as_f64(),
                p["target_price"].as_f64(),
                p["observed_quote_source_ms"].as_u64(),
                p["previous_broker_sealed_ms"].as_u64()
            )else{continue};
            if let Some(row)=draft(db,t,&key,now,stage,pid,plan,expected,lots,target,source,seal)?{
                rows.push(row);
            }
        }
    }
    Ok(json!({"schema":"tp-stage-core-contract-preview-v1",
        "status":"research_contracts_no_broker_commands",
        "context":key,"target":"demo","rows":rows,
        "read_only":true,"orders_enabled":false,
        "authorized":false,"breakeven_enabled":false,
        "note":"Exact Core-owned staged-close contract projections; no command IDs, order dispatch, authorization, fill, or stop modification."}))
}
