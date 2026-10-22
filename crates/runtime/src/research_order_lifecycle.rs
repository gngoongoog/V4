//! Offline deterministic replay of *supplied synthetic* broker lifecycle events.
//! No MT5 access, no database writes, no gateway commands, no inferred fills.
//! A foundation for later independent execution simulation, NOT a production twin.
use anyhow::{bail,ensure,Result};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::collections::BTreeMap;

const MAX_EVENTS:usize=5000;
const EPS:f64=1e-8;

#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    account:String,
    symbol:String,
    events:Vec<Event>,
}

#[derive(Debug,Deserialize)]
#[serde(tag="event",rename_all="snake_case",deny_unknown_fields)]
enum Event {
    Submit{id:String,lots:f64},
    Dispatch{id:String},
    Unknown{id:String},
    Fill{id:String,deal_id:String,lots:f64},
    Close{id:String,deal_id:String,lots:f64},
    Reject_remaining{id:String},
    Cancel_unsent{id:String},
}

#[derive(Clone,Debug,Serialize)]
struct Order {
    id:String,
    requested_lots:f64,
    filled_lots:f64,
    closed_lots:f64,
    dispatched:bool,
    uncertain:bool,
    remainder_rejected:bool,
    cancelled:bool,
}

impl Order {
    fn remaining_to_fill(&self)->f64{(self.requested_lots-self.filled_lots).max(0.)}
    fn open_lots(&self)->f64{(self.filled_lots-self.closed_lots).max(0.)}
    fn state(&self)->&'static str {
        if self.cancelled{"cancelled"}
        else if self.remainder_rejected && self.open_lots()>EPS{"partial_terminal_open"}
        else if self.remainder_rejected && self.filled_lots>EPS{"partial_terminal_flat"}
        else if self.remainder_rejected{"rejected_remaining"}
        else if self.filled_lots+EPS>=self.requested_lots && self.open_lots()<=EPS{"closed"}
        else if self.filled_lots+EPS>=self.requested_lots{"filled"}
        else if self.filled_lots>EPS{"partial"}
        else if self.uncertain{"unknown"}
        else if self.dispatched{"dispatched"}
        else{"queued"}
    }
}

#[derive(Clone,Debug,PartialEq)]
struct Deal {
    kind:&'static str,
    order_id:String,
    lots:f64,
}
fn identifier(s:&str)->bool {
    !s.is_empty()&&s.len()<=80&&s.bytes().all(|v|
        v.is_ascii_alphanumeric()||matches!(v,b'_'|b'-'|b'.'|b':'))
}
fn positive(v:f64)->bool{v.is_finite()&&v>0.&&v<=10000.}
fn order<'a>(orders:&'a mut BTreeMap<String,Order>,id:&str)->Result<&'a mut Order>{
    orders.get_mut(id).ok_or_else(||anyhow::anyhow!("Unknown synthetic order id: {id}"))
}
fn seen_deal(seen:&BTreeMap<String,Deal>,deal_id:&str,expected:&Deal)->Result<bool>{
    ensure!(identifier(deal_id),"Invalid deal identity");
    if let Some(prior)=seen.get(deal_id){
        ensure!(prior==expected,"Deal ID reused with different contents");
        return Ok(true)
    }
    Ok(false)
}

/// Strictly pure response from a bounded synthetic event stream.
pub fn replay(body:&[u8])->Result<Value>{
    ensure!(body.len()<=1_000_000,"Scenario exceeds 1 MB");
    let scenario:Scenario=serde_json::from_slice(body)?;
    ensure!(identifier(&scenario.account)&&identifier(&scenario.symbol),
        "Non-empty alphanumeric account and symbol identities are required");
    ensure!(scenario.events.len()<=MAX_EVENTS,"Too many lifecycle events");
    let mut orders=BTreeMap::<String,Order>::new();
    let mut seen=BTreeMap::<String,Deal>::new();
    let mut duplicates=0u64;
    for (index,event) in scenario.events.iter().enumerate(){
        // No partial results are returned when a hypothetical event is invalid.
        let step=(||->Result<()>{
            match event {
                Event::Submit{id,lots}=>{
                    ensure!(identifier(id)&&positive(*lots),"Invalid order ID or lots");
                    ensure!(!orders.contains_key(id),"Synthetic order ID already submitted");
                    orders.insert(id.clone(),Order{
                        id:id.clone(),requested_lots:*lots,filled_lots:0.,closed_lots:0.,
                        dispatched:false,uncertain:false,remainder_rejected:false,cancelled:false
                    });
                }
                Event::Dispatch{id}=>{
                    let o=order(&mut orders,id)?;
                    ensure!(!o.cancelled&&!o.remainder_rejected&&o.filled_lots<=EPS,
                        "Cannot dispatch a cancelled, finalized or already-filled order");
                    // Duplicate dispatch is harmless, never a second fill.
                    if o.dispatched {duplicates+=1;} else {o.dispatched=true;}
                }
                Event::Unknown{id}=>{
                    let o=order(&mut orders,id)?;
                    ensure!(o.dispatched&&!o.cancelled&&!o.remainder_rejected&&o.remaining_to_fill()>EPS,
                        "Uncertain send requires an unresolved dispatched entry");
                    if o.uncertain{duplicates+=1;}else{o.uncertain=true;}
                }
                Event::Fill{id,deal_id,lots}=>{
                    ensure!(positive(*lots),"Invalid reported fill volume");
                    let d=Deal{kind:"fill",order_id:id.clone(),lots:*lots};
                    if seen_deal(&seen,deal_id,&d)? {duplicates+=1;}
                    else {
                        let o=order(&mut orders,id)?;
                        ensure!(o.dispatched&&!o.cancelled&&!o.remainder_rejected,
                            "Cannot fill without dispatch or after final rejection");
                        ensure!(*lots<=o.remaining_to_fill()+EPS,
                            "Simulated cumulative fill would exceed requested lots");
                        o.filled_lots=(o.filled_lots+*lots).min(o.requested_lots);
                        if o.remaining_to_fill()<=EPS{o.uncertain=false;}
                        seen.insert(deal_id.clone(),d);
                    }
                }
                Event::Close{id,deal_id,lots}=>{
                    ensure!(positive(*lots),"Invalid reported close volume");
                    let d=Deal{kind:"close",order_id:id.clone(),lots:*lots};
                    if seen_deal(&seen,deal_id,&d)? {duplicates+=1;}
                    else {
                        let o=order(&mut orders,id)?;
                        ensure!(o.filled_lots>EPS && !o.cancelled,
                            "No reported fill available for closing");
                        ensure!(*lots<=o.open_lots()+EPS,
                            "Close exceeds broker-reported still-open volume");
                        o.closed_lots=(o.closed_lots+*lots).min(o.filled_lots);
                        seen.insert(deal_id.clone(),d);
                    }
                }
                Event::Reject_remaining{id}=>{
                    let o=order(&mut orders,id)?;
                    ensure!(o.dispatched&&!o.cancelled,"Only dispatched entries may be rejected by broker");
                    ensure!(o.remaining_to_fill()>EPS,"Already fully filled");
                    if o.remainder_rejected {duplicates+=1;}
                    else {o.remainder_rejected=true;o.uncertain=false;}
                }
                Event::Cancel_unsent{id}=>{
                    let o=order(&mut orders,id)?;
                    ensure!(!o.dispatched && o.filled_lots<=EPS && !o.remainder_rejected,
                        "Cannot cancel an already-dispatched or uncertain broker send locally");
                    if o.cancelled{duplicates+=1;}else{o.cancelled=true;}
                }
            }
            Ok(())
        })();
        if let Err(e)=step {bail!("Synthetic event #{} invalid: {e}",index+1)}
    }
    let mut unresolved=0u64;
    let mut open_lots=0.;
    let mut open_orders=0u64;
    let mut view=Vec::new();
    for o in orders.values(){
        let remains=o.remaining_to_fill();
        let open=o.open_lots();
        if o.dispatched && remains>EPS && !o.remainder_rejected {
            unresolved+=1;
        }
        if open>EPS{open_orders+=1;}
        open_lots+=open;
        view.push(json!({"id":o.id,"state":o.state(),
            "requested_lots":o.requested_lots,"filled_lots":o.filled_lots,
            "closed_lots":o.closed_lots,"still_open_lots":open,
            "unresolved_entry_lots":if o.dispatched&&!o.remainder_rejected {remains}else{0.},
            "uncertain_send":o.uncertain && remains>EPS,
            "broker_event_only":true}));
    }
    Ok(json!({
        "status":"synthetic_only","account":scenario.account,"symbol":scenario.symbol,
        "event_count":scenario.events.len(),"idempotent_replays":duplicates,
        "orders":view,"open_orders":open_orders,"open_lots":open_lots,
        "unresolved_dispatched_entries":unresolved,
        "profit_loss":null,
        "result_explanation":"Only explicit synthetic fill and close events alter lot balances. Unknown sends never imply fills or local cancellation. No real broker or price data.",
        "no_mt5_commands":true,"no_broker_reads":true,
        "no_portfolio_or_margin_admission":true,"no_learning_updates":true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(events:&str)->Result<Value>{
        replay(format!(r#"{{"account":"demo_77","symbol":"XAUUSD","events":{events}}}"#).as_bytes())
    }
    #[test]
    fn unknown_send_is_never_an_implied_fill_or_cancellable(){
        let v=run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"unknown","id":"A"}]"#).unwrap();
        assert_eq!(v["orders"][0]["state"],"unknown");
        assert_eq!(v["orders"][0]["filled_lots"],0.);
        assert_eq!(v["unresolved_dispatched_entries"],1);
        assert!(run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"unknown","id":"A"},
            {"event":"cancel_unsent","id":"A"}]"#).is_err());
    }
    #[test]
    fn broker_reported_partial_fill_then_close_keeps_unresolved_remainder(){
        let v=run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"unknown","id":"A"},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.04},
            {"event":"close","id":"A","deal_id":"d2","lots":0.04}]"#).unwrap();
        assert_eq!(v["orders"][0]["state"],"partial");
        assert_eq!(v["open_lots"],0.);
        assert_eq!(v["unresolved_dispatched_entries"],1);
        let v=run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.04},
            {"event":"reject_remaining","id":"A"},
            {"event":"close","id":"A","deal_id":"d2","lots":0.04}]"#).unwrap();
        assert_eq!(v["orders"][0]["state"],"partial_terminal_flat");
        assert_eq!(v["unresolved_dispatched_entries"],0);
    }
    #[test]
    fn idempotent_replay_by_exact_deal_id_does_not_multiply_fill_or_close(){
        let v=run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.1},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.1},
            {"event":"close","id":"A","deal_id":"d2","lots":0.1},
            {"event":"close","id":"A","deal_id":"d2","lots":0.1}]"#).unwrap();
        assert_eq!(v["orders"][0]["state"],"closed");
        assert_eq!(v["idempotent_replays"],2);
        assert_eq!(v["open_lots"],0.);
    }
    #[test]
    fn detects_overfill_and_cross_order_deal_reuse(){
        assert!(run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.2}]"#).is_err());
        assert!(run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"submit","id":"B","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"dispatch","id":"B"},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.1},
            {"event":"fill","id":"B","deal_id":"d1","lots":0.1}]"#).is_err());
        assert!(run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.1}]"#).is_err());
    }
    #[test]
    fn prevents_overclose_and_preserves_zero_profit_claim(){
        assert!(run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"dispatch","id":"A"},
            {"event":"fill","id":"A","deal_id":"d1","lots":0.1},
            {"event":"close","id":"A","deal_id":"d2","lots":0.2}]"#).is_err());
        let v=run(r#"[
            {"event":"submit","id":"A","lots":0.1},
            {"event":"cancel_unsent","id":"A"}]"#).unwrap();
        assert_eq!(v["orders"][0]["state"],"cancelled");
        assert!(v["profit_loss"].is_null());
        assert_eq!(v["no_mt5_commands"],true);
    }
}
