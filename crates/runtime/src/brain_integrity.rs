//! P1-A: read-only evidence integrity for the 17 configured research and execution brains.
//!
//! This is PRESENTATION, not an entry signal, learning update, or permission to trade.
//! Every performance figure is restricted to CLOSED, mode-matched MT5 DEMO
//! broker evidence. Advisor / paper signals NEVER become a broker success rate.
use anyhow::{bail, Result};
use braintrade_domain::{execution::BRAINS, trading::TradingMode, Status};
use rusqlite::Connection;
use serde_json::{json,Value};

pub(crate) fn enrich(view:&mut Value,db:&Connection,status:&Status,mode:TradingMode)->Result<()>{
    // Read account metadata BEFORE borrowing the array mutably.
    let context=view["context"].as_str().unwrap_or("").to_string();
    let account_matches=view["account_matches"]==true;
    let target_is_demo=view["settings"]["target"]=="demo";
    let broker=status.telemetry.as_ref();
    let exact_demo=broker.is_some_and(|t|t.connected && t.demo && t.account_trade_mode==Some(0))
        && status.gateway_fresh && account_matches && target_is_demo && !context.is_empty();
    let current_mode=broker.and_then(|t|t.account_trade_mode);
    let no_account=!status.gateway_fresh||!account_matches;
    let expected:BTreeSet<String>=BRAINS.iter().map(|(id,_)|id.to_string()).collect();
    let rows=view["brains"].as_array_mut().ok_or_else(||anyhow::anyhow!("Brain list is missing"))?;
    let got:BTreeSet<String>=rows.iter().filter_map(|row|row["config"]["id"].as_str().map(str::to_owned)).collect();
    if rows.len()!=BRAINS.len() || got!=expected {
        bail!("Incomplete or duplicate brain inventory; refuse to report brain integrity");
    }
    let mut sampled=0u64;
    for row in rows {
        let id=row["config"]["id"].as_str().unwrap_or_default().to_string();
        let role=if id=="system"{"selected_mode_coordinator"}else{"advisor_or_research"};
        let evidence=if exact_demo {
            let audited=crate::learning_quality::audit(db,&context,mode,&id)?;
            let n=audited["broker_samples"].as_u64().unwrap_or(0);
            sampled+=n;
            let sufficient=n>=audited["minimum_samples_for_comparison"].as_u64().unwrap_or(30);
            let status_label=if sufficient {audited["status"].as_str().unwrap_or("insufficient_evidence")}else{"insufficient_evidence"};
            json!({
                "status":status_label,"sample_count":n,"minimum_samples":30,
                "invalid_samples":audited["invalid_samples"],"duplicates":audited["duplicate_position_samples"],
                "win_rate":if sufficient {audited["all"]["win_rate"].clone()}else{Value::Null},
                "net_booked":if n>0 {audited["all"]["net_booked"].clone()}else{Value::Null},
                "source":"closed_costed_mt5_demo_deals_only","scope":"current_demo_account_symbol_mode_brain",
                "warning":"Historical or online sample, NOT independent forward OOS proof",
                "no_auto_promotion":true,"broker_execution_permission":false
            })
        } else {
            json!({"status":"insufficient_evidence","sample_count":0,"minimum_samples":30,
                "win_rate":null,"net_booked":null,
                "reason":if no_account{"broker_unavailable_or_stale"}else{"verified_demo_evidence_unavailable_for_this_account"},
                "source":"none","no_auto_promotion":true,"broker_execution_permission":false})
        };
        row["integrity"]=json!({
            "role":role,
            "broker_evidence":evidence,
            "signal":if row["latest"].is_null(){"no_fresh_signal"}else{"recent_signal_present_not_a_fill"},
            "learned_weight_is_not_win_probability":true,
            "broker_ownership":if id=="system"{"selected_mode_coordinator_only"}else{"research_only"},
            "promotion_enabled":false
        });
    }
    view["brain_integrity"]=json!({
        "schema":"p1-brain-integrity-v1",
        "brains_accounted_for":BRAINS.len(),
        "inventory_complete":true,
        "demo_account_verified":exact_demo,
        "account_trade_mode":current_mode,
        "trading_mode":mode,
        "broker_sample_rows_total":sampled,
        "status":"research_monitor_only",
        "unverified_live_win_rate":null,
        "read_only":true,
        "changes_execution":false,
        "automatic_promotion":false,
        "note":"17 configured brains do not imply 17 independent broker strategies or 17 independent profit histories; no fabricated wins."
    });
    Ok(())
}

use std::collections::BTreeSet;

#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Telemetry;

    fn view()->Value {
        let rows=BRAINS.iter().map(|(id,_)|
            json!({"config":{"id":id},"latest":null,"stats":{"closed_positions":1000,"wins":999,"net_booked":999999}})
        ).collect::<Vec<_>>();
        json!({"brains":rows,"account_matches":true,"context":"[\"Demo\",17,\"XAUUSD\",\"USD\"]","settings":{"target":"demo"}})
    }
    fn status(demo:bool,fresh:bool)->Status{
        let mut s=Status::default();
        s.gateway_fresh=fresh;
        s.telemetry=Some(Telemetry{connected:true,login:17,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),demo,account_trade_mode:Some(if demo{0}else{2}),..Default::default()});
        s
    }
    #[test]
    fn all_seventeen_brains_have_clear_roles_and_no_fictional_metrics(){
        let db=Connection::open_in_memory().unwrap();
        let mut data=view();
        enrich(&mut data,&db,&status(true,true),TradingMode::Sniper).unwrap();
        assert_eq!(data["brain_integrity"]["brains_accounted_for"],17);
        assert_eq!(data["brain_integrity"]["read_only"],true);
        assert_eq!(data["brains"][0]["integrity"]["role"],"selected_mode_coordinator");
        assert_eq!(data["brains"][1]["integrity"]["role"],"advisor_or_research");
        assert_eq!(data["brains"][0]["integrity"]["broker_evidence"]["win_rate"],Value::Null);
        assert_eq!(data["brains"][0]["integrity"]["broker_evidence"]["net_booked"],Value::Null);
        assert_eq!(data["brains"][0]["integrity"]["broker_evidence"]["status"],"insufficient_evidence");
    }
    #[test]
    fn real_account_and_stale_snapshot_never_inherit_demo_statistics(){
        let db=Connection::open_in_memory().unwrap();
        for (demo,fresh) in [(false,true),(true,false),(false,false)]{
            let mut data=view();
            enrich(&mut data,&db,&status(demo,fresh),TradingMode::Expert).unwrap();
            assert_eq!(data["brain_integrity"]["demo_account_verified"],false);
            for brain in data["brains"].as_array().unwrap(){
                assert!(brain["integrity"]["broker_evidence"]["win_rate"].is_null());
                assert!(brain["integrity"]["broker_evidence"]["net_booked"].is_null());
            }
        }
    }
    #[test]
    fn incomplete_and_duplicate_brain_inventory_fails_closed(){
        let db=Connection::open_in_memory().unwrap();
        let mut missing=view();missing["brains"].as_array_mut().unwrap().pop();
        assert!(enrich(&mut missing,&db,&status(true,true),TradingMode::Sniper).is_err());
        let mut dup=view();
        let first=dup["brains"][0].clone();
        dup["brains"][1]=first;
        assert!(enrich(&mut dup,&db,&status(true,true),TradingMode::Sniper).is_err());
    }
}
