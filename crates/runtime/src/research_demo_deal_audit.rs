//! Read-only comparison of synthetic entry/exit volumes against *caller supplied*
//! normalized MT5 Demo deal records. Caller-provided data is NOT authenticated
//! broker history; matching volumes is not execution or PnL verification.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_BYTES:usize=3_000_000;
const MAX_DEALS:usize=5000;
const EPS:f64=1e-8;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    bridge:Value,
    evidence:Evidence,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    source:String,
    account:String,
    server:String,
    symbol:String,
    currency:String,
    demo:bool,
    captured_ms:u64,
    complete_history:bool,
    deals:Vec<Deal>,
}
#[derive(Deserialize,Clone,Copy)]
#[serde(rename_all="snake_case")]
enum Direction {In,Out,Inout}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Deal {
    ticket:u64,
    position_id:u64,
    #[serde(default)]
    intent_id:Option<String>,
    direction:Direction,
    volume:f64,
    price:f64,
    time_msc:u64,
    profit:f64,
    commission:f64,
    swap:f64,
    fee:f64,
}
#[derive(Default)]
struct Observed {
    entered:f64,
    exited:f64,
    net:f64,
    tickets:u64,
    positions:BTreeSet<u64>,
    ambiguous:u64,
}
fn identifier(s:&str)->bool {
    !s.is_empty() && s.len()<=100 && s.bytes().all(|c|
        c.is_ascii_alphanumeric() || matches!(c,b'_'|b'-'|b':'|b'.'))
}
fn same(a:f64,b:f64)->bool {(a-b).abs()<=EPS.max(a.abs().max(b.abs())*1e-8)}

pub fn audit(body:&[u8])->Result<Value>{
    ensure!(body.len()<=MAX_BYTES,"Evidence-audit scenario exceeds 3 MB");
    let req:Request=serde_json::from_slice(body)?;
    let ev=&req.evidence;
    ensure!(ev.source=="caller_supplied_mt5_demo_deals",
        "Source must disclose caller-supplied, unverified MT5 Demo records");
    ensure!(ev.demo && identifier(&ev.account) && identifier(&ev.server)
        && identifier(&ev.symbol) && identifier(&ev.currency)
        && ev.captured_ms>0 && ev.deals.len()<=MAX_DEALS,
        "Invalid or non-Demo supplied broker evidence");
    let bridge=crate::research_execution_bridge::compare(
        serde_json::to_vec(&req.bridge)?.as_slice()
    ).context("Independent synthetic bridge rejected the scenario")?;
    ensure!(bridge["account"].as_str()==Some(ev.account.as_str())
        && bridge["symbol"].as_str()==Some(ev.symbol.as_str()),
        "Demo evidence account/symbol mismatch with synthetic scenario");
    let synthetic=bridge["synthetic_order_lifecycle"]["orders"].as_array()
        .context("Missing synthetic order ledger")?;
    let mut by_id=BTreeMap::<String,Observed>::new();
    let mut requested=BTreeMap::<String,(f64,f64)>::new();
    for order in synthetic {
        let id=order["id"].as_str().context("Missing synthetic order ID")?;
        let fills=order["filled_lots"].as_f64().context("Missing synthetic fill total")?;
        let closes=order["closed_lots"].as_f64().context("Missing synthetic close total")?;
        ensure!(identifier(id) && fills.is_finite() && closes.is_finite()
            && fills>=0. && closes>=0.,"Invalid synthetic order ledger");
        ensure!(requested.insert(id.into(),(fills,closes)).is_none(),
            "Duplicate synthetic order ID");
        by_id.insert(id.into(),Observed::default());
    }
    let mut seen=BTreeSet::<u64>::new();
    let mut unlinked=0u64;
    let mut unsupported=0u64;
    let mut unmatched=0u64;
    let mut total_net=0.;
    let mut count=0u64;
    for deal in &ev.deals {
        ensure!(deal.ticket>0 && deal.position_id>0 && deal.time_msc>0
            && deal.time_msc<=ev.captured_ms
            && deal.volume.is_finite() && deal.volume>0.
            && deal.price.is_finite() && deal.price>0.
            && [deal.profit,deal.commission,deal.swap,deal.fee]
                .iter().all(|x|x.is_finite()),
            "Malformed or future-dated MT5 Demo deal evidence");
        ensure!(seen.insert(deal.ticket),"Duplicate MT5 deal ticket in evidence");
        let Some(intent)=deal.intent_id.as_deref() else {
            unlinked+=1;
            continue
        };
        ensure!(identifier(intent),"Malformed supplied intent identity");
        let Some(row)=by_id.get_mut(intent) else {
            unmatched+=1;
            continue
        };
        row.tickets+=1;
        row.positions.insert(deal.position_id);
        if matches!(deal.direction,Direction::Inout) {
            row.ambiguous+=1;
            unsupported+=1;
            continue
        }
        match deal.direction {
            Direction::In=>row.entered+=deal.volume,
            Direction::Out=>row.exited+=deal.volume,
            Direction::Inout=>unreachable!()
        }
        let net=deal.profit+deal.commission+deal.swap+deal.fee;
        row.net+=net;
        total_net+=net;
        count+=1;
    }
    let mut discrepancies=0usize;
    let mut unknown=0usize;
    let mut matches=0usize;
    let mut rows=Vec::new();
    for (id,(synthetic_in,synthetic_out)) in requested {
        let seen=&by_id[&id];
        let over_seen=seen.entered>synthetic_in+EPS
            ||seen.exited>synthetic_out+EPS
            ||seen.exited>seen.entered+EPS;
        let exact=same(seen.entered,synthetic_in)&&same(seen.exited,synthetic_out);
        let state=if seen.ambiguous>0 || seen.positions.len()>1 {
                unknown+=1;"unresolved_ambiguous_position"
            } else if over_seen || (ev.complete_history && !exact) {
                discrepancies+=1;"volume_discrepancy"
            } else if !ev.complete_history || seen.tickets==0 {
                unknown+=1;"insufficient_deal_evidence"
            } else {
                matches+=1;"volume_parity_from_supplied_records"
            };
        rows.push(json!({
            "id":id,
            "state":state,
            "synthetic_filled_lots":synthetic_in,
            "synthetic_closed_lots":synthetic_out,
            "supplied_demo_entry_lots":seen.entered,
            "supplied_demo_exit_lots":seen.exited,
            "supplied_deal_count":seen.tickets,
            "supplied_position_ids":seen.positions.iter().copied().collect::<Vec<_>>(),
            "supplied_net_of_linked_non_inout_deals":seen.net,
            "note":"Volume-only comparison, not proof that events happened at the same price/time. Reported deal rows are caller-supplied."
        }));
    }
    let status=if discrepancies>0 || unmatched>0 {
        "discrepancy"
    } else if unknown>0 || unlinked>0 || unsupported>0 || rows.is_empty() {
        "insufficient_or_ambiguous_evidence"
    } else {
        "volume_parity_claimed_unverified"
    };
    Ok(json!({
        "policy":"mt5-demo-deal-evidence-audit-v1",
        "status":status,
        "source_provenance":"caller supplied; NOT verified against MT5, not a broker attestation",
        "account":ev.account,"server":ev.server,"symbol":ev.symbol,"currency":ev.currency,
        "snapshot_captured_ms":ev.captured_ms,
        "complete_history_claimed_by_caller":ev.complete_history,
        "deals_in_supplied_snapshot":ev.deals.len(),
        "linked_unambiguous_deals":count,
        "unlinked_deals":unlinked,
        "deals_linked_to_unknown_orders":unmatched,
        "unsupported_inout_netting_deals":unsupported,
        "volume_parity_rows":matches,
        "discrepancy_rows":discrepancies,
        "insufficient_rows":unknown,
        "orders":rows,
        "net_booked_from_caller_supplied_linked_deals_only":total_net,
        "synthetic_bridge":bridge,
        "actual_broker_verified":false,
        "profitability_verified":false,
        "mt5_commands":0,
        "broker_reads":0,
        "learning_effect":false,
        "persistent_mutation":false,
        "limitations":"Intent-to-deal mapping and history completeness are supplied by the caller, not authenticated. Fee/profit fields cover only provided linked non-INOUT deals. Volume parity alone cannot validate real prices, slippage, executions, swaps, missing tickets or broker acceptance. Do not use for live risk approval."
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn admission()->Value{json!({
        "account":"demo_77","symbol":"XAUUSD",
        "quote":{"bid":5000.,"ask":5000.2,"age_ms":50},
        "contract":{"volume_min":0.01,"volume_max":1.,"volume_step":0.01,
            "tick_size":0.1,"tick_value_loss":1.,"estimated_margin_per_lot":100.},
        "limits":{"equity":500.,"free_margin":300.,"max_positions":2,
            "max_total_lots":0.1,"max_stop_risk_percent":2.,
            "commission_roundtrip_per_lot":7.,"slippage_ticks_per_side":1.},
        "requests":[{"id":"A","side":"buy","lots":0.04,"stop_price":4999.2}]
    })}
    fn event_deals()->Value{json!([
        {"event":"submit","id":"A","lots":0.04},
        {"event":"dispatch","id":"A"},
        {"event":"fill","id":"A","deal_id":"synthetic1","lots":0.04},
        {"event":"close","id":"A","deal_id":"synthetic2","lots":0.04}
    ])}
    fn deal(ticket:u64,direction:&str,volume:f64)->Value{
        json!({"ticket":ticket,"position_id":77,"intent_id":"A",
            "direction":direction,"volume":volume,"price":5000.,
            "time_msc":1500+ticket,
            "profit":if direction=="out"{2.}else{0.},
            "commission":-0.14,"swap":0.,"fee":0.})
    }
    fn request(deals:Value,complete:bool)->Value {json!({
        "bridge":{"admission":admission(),"lifecycle":{
            "account":"demo_77","symbol":"XAUUSD","events":event_deals()}},
        "evidence":{"source":"caller_supplied_mt5_demo_deals",
            "account":"demo_77","server":"demo_server","symbol":"XAUUSD",
            "currency":"USD","demo":true,"captured_ms":10_000,
            "complete_history":complete,"deals":deals}
    })}
    fn run(v:Value)->Result<Value>{audit(v.to_string().as_bytes())}

    #[test]
    fn matching_supplied_demo_deals_only_claim_unverified_volume_parity(){
        let v=run(request(json!([deal(1,"in",0.04),deal(2,"out",0.04)]),true)).unwrap();
        assert_eq!(v["status"],"volume_parity_claimed_unverified");
        assert_eq!(v["orders"][0]["supplied_demo_entry_lots"],0.04);
        assert_eq!(v["orders"][0]["supplied_demo_exit_lots"],0.04);
        assert_eq!(v["actual_broker_verified"],false);
        assert_eq!(v["profitability_verified"],false);
        assert_eq!(v["mt5_commands"],0);
    }
    #[test]
    fn incomplete_supplied_history_cannot_fake_a_missing_fill(){
        let v=run(request(json!([deal(1,"in",0.04)]),false)).unwrap();
        assert_eq!(v["status"],"insufficient_or_ambiguous_evidence");
        assert_eq!(v["orders"][0]["state"],"insufficient_deal_evidence");
        let v=run(request(json!([deal(1,"in",0.04)]),true)).unwrap();
        assert_eq!(v["status"],"discrepancy");
        assert_eq!(v["orders"][0]["state"],"volume_discrepancy");
    }
    #[test]
    fn real_or_different_account_and_duplicate_ticket_are_rejected(){
        let mut v=request(json!([deal(1,"in",0.04),deal(2,"out",0.04)]),true);
        v["evidence"]["demo"]=json!(false);
        assert!(run(v).is_err());
        let mut v=request(json!([]),false);
        v["evidence"]["account"]=json!("demo_99");
        assert!(run(v).is_err());
        assert!(run(request(json!([deal(1,"in",0.04),deal(1,"out",0.04)]),true)).is_err());
    }
    #[test]
    fn broker_reverse_and_unlinked_deals_are_not_mislabeled_complete(){
        let v=run(request(json!([deal(1,"inout",0.04)]),true)).unwrap();
        assert_eq!(v["status"],"insufficient_or_ambiguous_evidence");
        assert_eq!(v["unsupported_inout_netting_deals"],1);
        let mut unlinked=deal(9,"out",0.03);
        unlinked["intent_id"]=Value::Null;
        let v=run(request(json!([deal(1,"in",0.04),deal(2,"out",0.04),unlinked]),true)).unwrap();
        assert_eq!(v["status"],"insufficient_or_ambiguous_evidence");
        assert_eq!(v["unlinked_deals"],1);
    }
    #[test]
    fn copied_intent_mapping_to_unknown_order_is_a_discrepancy(){
        let mut x=deal(1,"in",0.04);
        x["intent_id"]=json!("B");
        let v=run(request(json!([x]),true)).unwrap();
        assert_eq!(v["status"],"discrepancy");
        assert_eq!(v["deals_linked_to_unknown_orders"],1);
    }
}
