//! Core-owned staged-close command construction. This is the *last* pure,
//! read-only boundary before implementing a separately reviewed durable queue.
//!
//! Each draft is assembled only from the validated TP price/volume/Deal-lineage
//! contract for the currently selected MT5 Demo or Live account. No generated
//! ID, broker_commands row, authorization, send journal or Gateway call exists
//! here; a draft is intentionally NOT executable by Gateway.
use anyhow::Result;
use braintrade_domain::{execution,Telemetry};
use rusqlite::Connection;
use serde_json::{json,Value};
use std::collections::BTreeMap;

fn positive(v:&Value,key:&str)->Option<f64>{
    v[key].as_f64().filter(|n|n.is_finite()&&*n>0.)
}
fn exact_grid(v:f64,step:f64)->bool{
    v.is_finite()&&v>0.&&step.is_finite()&&step>0.
        &&(v/step).is_finite()&&((v/step)-(v/step).round()).abs()<=1e-6
}
fn close(a:f64,b:f64,step:f64)->bool{
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
fn candidate(row:&Value,t:&Telemetry,target:&str,now:u64,
    revision:u64,deviation_points:u32,risk_master_enabled:bool)->Option<Value>
{
    let context=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).ok()?;
    if row["schema"]!="tp-stage-core-contract-preview-v1"
        ||row["state"]!="validated_research_contract_only"
        ||row["context"]!=context||row["target"]!=target
        ||row["login"].as_u64()!=Some(t.login)
        ||row["server"]!=t.server||row["currency"]!=t.currency
        ||row["symbol"]!=t.symbol
        ||row["orders_enabled"]!=false||row["authorized"]!=false
        ||row["breakeven_enabled"]!=false
        ||!row["broker_command_id"].is_null()
        ||!row["gateway_send_id"].is_null()
        ||!row["broker_order_id"].is_null()
        ||deviation_points>100_000
    {return None}
    let plan=row["tp_stage_plan_id"].as_str()?;
    if !plan.starts_with("tp-plan-")||plan.len()!=40
        ||!plan[8..].bytes().all(|c|c.is_ascii_hexdigit()) {return None}
    let stage=row["tp_stage"].as_u64()?;
    let position=row["position"].as_u64().filter(|p|*p>0)?;
    let pid=row["expected_position_identifier"].as_u64().filter(|p|*p>0)?;
    let magic=row["magic"].as_u64().filter(|p|*p>0)?;
    let brain=row["brain"].as_str()?;
    if !(1..=3).contains(&stage)||execution::brain_for_magic(magic)!=Some(brain)
        ||row["prior_stage_broker_sealed"]!=(stage>1)
        ||row["quote_source_ms"].as_u64()!=t.source_quote_time_ms
    {return None}
    let side=row["expected_position_side"].as_str()?;
    if !matches!(side,"BUY"|"SELL"){return None}
    let expected=positive(row,"expected_position_volume")?;
    let lots=positive(row,"lots")?;
    let entry=positive(row,"expected_position_entry")?;
    let price=positive(row,"tp_target_price")?;
    let step=positive(row,"volume_step")?;
    let minimum=t.volume_min?;
    let maximum=t.volume_max?;
    let tick=t.tick_size?;
    let original_tp=row["expected_broker_tp"].as_f64()
        .filter(|n|n.is_finite()&&*n>=0.)?;
    let fresh_quote=if side=="BUY"{t.bid?}else{t.ask?};
    let expires_ms=now.checked_add(3_000)?;
    if !close(t.volume_step?,step,step)
        ||!exact_grid(lots,step)||!exact_grid(expected,step)
        ||!exact_grid(price,tick)
        ||!minimum.is_finite()||minimum<=0.||!maximum.is_finite()||maximum<minimum
        ||lots<minimum-step*1e-6||lots>maximum+step*1e-6
        ||lots>expected+step*1e-6
        ||(stage<3&&(!exact_grid(expected-lots,step)
            ||expected-lots<minimum-step*1e-6))
    {return None}
    if !fresh_quote.is_finite()||fresh_quote<=0.
        ||(side=="BUY"&&(price<=entry+tick*0.5||fresh_quote<price))
        ||(side=="SELL"&&(price+tick*0.5>=entry||fresh_quote>price))
        ||(stage==3&&!close(expected,lots,step))
    {return None}
    // The draft intentionally omits "id" and "state". Gateway cannot
    // interpret it as a queued durable intent even if a caller forwards JSON.
    Some(json!({
        "schema":"tp-stage-broker-command-shape-v1",
        "context":context,"brain":brain,"kind":"close",
        "login":t.login,"server":t.server,"currency":t.currency,
        "target":target,"symbol":t.symbol,"magic":magic,
        "position":position,"lots":lots,"revision":revision,
        "deviation_points":deviation_points,
        "created_ms":now,"expires_ms":expires_ms,
        "max_quote_age_ms":3000,
        "manual_close":false,"manual_open":false,"demo_flexible":false,
        "risk_master_enabled":risk_master_enabled,"signal":Value::Null,
        "tp_stage_plan_id":plan,"tp_stage":stage,
        "expected_position_identifier":pid,
        "expected_position_side":side,
        "expected_position_volume":expected,
        "expected_position_entry":entry,"expected_broker_tp":original_tp,
        "tp_target_price":price,
        "price_source_quote_ms":row["quote_source_ms"]
    }))
}
pub fn report(db:&Connection,t:&Telemetry,target:&str,now:u64,
    revision:u64,deviation_points:u32,risk_master_enabled:bool)->Result<Value>
{
    let empty=||json!({
        "schema":"tp-stage-broker-command-drafts-v1",
        "status":"unverified_selected_account_or_price_lineage",
        "rows":[],"read_only":true,"orders_enabled":false,
        "authorized":false,"broker_commands_created":0
    });
    if !matches!((target,t.demo,t.account_trade_mode),
        ("demo",true,Some(0))|("live",false,Some(2)))
        ||!t.connected||t.margin_mode!=Some(2)||!t.validate()
        ||t.login==0||deviation_points>100_000
        ||!t.quote_is_fresh(now,3_000)
    {return Ok(empty())}
    let contracts=crate::partial_exit_stage_contract::report(db,t,target,now)?;
    if contracts["schema"]!="tp-stage-core-contract-preview-v1"
        ||contracts["status"]!="research_contracts_no_broker_commands"
        ||contracts["context"]!=serde_json::to_string(
            &(&t.server,t.login,&t.symbol,&t.currency))?
        ||contracts["target"]!=target||contracts["read_only"]!=true
        ||contracts["orders_enabled"]!=false||contracts["authorized"]!=false
    {return Ok(empty())}
    let Some(source)=contracts["rows"].as_array()else{return Ok(empty())};
    // Ambiguous stages on one physical position are dropped in their
    // entirety rather than picking the first plausible command.
    let mut counts=BTreeMap::<u64,u32>::new();
    for row in source {
        if let Some(pid)=row["expected_position_identifier"].as_u64(){
            *counts.entry(pid).or_default()+=1;
        }
    }
    let rows=source.iter().filter_map(|r|{
        let pid=r["expected_position_identifier"].as_u64()?;
        if counts.get(&pid)!=Some(&1){return None}
        let draft=candidate(r,t,target,now,revision,deviation_points,
            risk_master_enabled)?;
        Some(json!({
            "stage":r["tp_stage"],
            "position_identifier":pid,
            "plan_id":r["tp_stage_plan_id"],
            "broker_command_draft":draft,
            "source":"Core immutable account/price/volume lineage",
            "read_only":true,"orders_enabled":false,
            "broker_command_id":Value::Null
        }))
    }).collect::<Vec<_>>();
    Ok(json!({
        "schema":"tp-stage-broker-command-drafts-v1",
        "status":"broker_command_shapes_not_queued",
        "context":contracts["context"],"target":target,
        "rows":rows,"read_only":true,
        "orders_enabled":false,"authorized":false,
        "broker_commands_created":0,
        "note":"Gateway-shaped economic contract only. No ID, no queue, no authorization, no broker order."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fake_or_ambiguous_stage_drafts_never_form_a_sendable_command(){
        let mut t=Telemetry::default();
        t.connected=true;t.demo=false;t.account_trade_mode=Some(2);
        t.margin_mode=Some(2);t.login=50;t.server="LiveBroker".into();
        t.currency="USD".into();t.symbol="XAUUSDm".into();
        t.volume_step=Some(0.01);t.volume_min=Some(0.01);
        t.tick_size=Some(0.01);t.bid=Some(2001.5);
        t.source_quote_time_ms=Some(10_100);
        let mut r=json!({
            "schema":"tp-stage-core-contract-preview-v1",
            "state":"validated_research_contract_only",
            "context":serde_json::to_string(
                &(&t.server,t.login,&t.symbol,&t.currency)).unwrap(),
            "target":"live","login":50,"server":"LiveBroker","currency":"USD",
            "symbol":"XAUUSDm","brain":"trend","magic":410601,"position":87,
            "tp_stage":1,"tp_stage_plan_id":format!("tp-plan-{}","a".repeat(32)),
            "expected_position_identifier":907,"expected_position_side":"BUY",
            "expected_position_volume":0.1,"lots":0.05,
            "expected_position_entry":1999.5,"expected_broker_tp":0.0,
            "tp_target_price":2001.0,"volume_step":0.01,
            "quote_source_ms":10100,"prior_stage_broker_sealed":false,
            "orders_enabled":false,"authorized":false,"breakeven_enabled":false,
            "broker_command_id":null,"gateway_send_id":null,"broker_order_id":null
        });
        // Known strategy magic is mandatory; do not allow a forged brain.
        r["brain"]=json!("bad");
        assert!(candidate(&r,&t,"live",10_150,3,20,true).is_none());
        r["brain"]=json!(execution::brain_for_magic(410601).unwrap());
        let valid=candidate(&r,&t,"live",10_150,3,20,true).unwrap();
        assert_eq!(valid["target"],"live");
        assert_eq!(valid["kind"],"close");
        assert_eq!(valid["tp_stage"],1);
        assert_eq!(valid["lots"],0.05);
        assert_eq!(valid["max_quote_age_ms"],3000);
        assert!(valid.get("id").is_none());
        assert!(valid.get("state").is_none());
        assert_eq!(valid["expected_position_entry"],1999.5);
        assert_eq!(valid["expected_broker_tp"],0.0);
        for name in ["tp_stage_plan_id","tp_target_price",
            "expected_position_identifier","expected_position_volume",
            "expected_position_entry","expected_broker_tp"] {
            let mut altered=r.clone();
            altered[name]=Value::Null;
            assert!(candidate(&altered,&t,"live",10_150,3,20,true).is_none(),
                "Missing immutable economic field {name} must invalidate draft");
        }
        let mut altered=r.clone();
        altered["target"]=json!("demo");
        assert!(candidate(&altered,&t,"live",10_150,3,20,true).is_none());
        let mut altered=r.clone();
        altered["prior_stage_broker_sealed"]=json!(true);
        assert!(candidate(&altered,&t,"live",10_150,3,20,true).is_none());
        let mut altered=r.clone();
        altered["tp_target_price"]=json!(1998.);
        assert!(candidate(&altered,&t,"live",10_150,3,20,true).is_none());
    }
}
