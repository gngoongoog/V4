use super::*;
use crate::{DomLevel, Status};

fn setup() -> Setup {
    Setup {id:"fixture".into(),pattern:"zone_rejection".into(),name:"fixture".into(),side:1,
        signal_bar:60,expires_ms:180_000,zone_id:"z".into(),zone_low:98.5,zone_high:99.5,
        reference_close:100.0,atr:1.0,structural_stop:99.0,obstacle:None,evidence:0.8,
        reasons:vec!["fixture".into()],signal_gates:vec![],gates:vec![],state:"DETECTED".into(),
        entry:None,stop:None,targets:vec![],lots:None,estimated_risk:None,net_rr:None}
}
fn quote()->Telemetry {
    Telemetry {connected:true,bid:Some(100.0),ask:Some(100.05),tick_age_ms:Some(0),
        tick_size:Some(0.01),tick_value_loss:Some(1.0),tick_value_profit:Some(1.0),
        volume_min:Some(0.01),volume_step:Some(0.01),volume_max:Some(10.0),..Default::default()}
}
fn opportunities()->MarketBrains {
    MarketBrains {current:true,setups:vec![setup()],..Default::default()}
}
#[test]
fn quote_plan_respects_budget_rounding_and_expiry() {
    let mut map=opportunities();let risk=RiskConfig::default();
    map.price(&quote(),&risk,5.0,&[],120_000);
    let s=&map.setups[0];assert_eq!(s.state,"READY");
    assert!(s.estimated_risk.unwrap()<=5.0+1e-10);
    assert!(s.stop.unwrap()<s.entry.unwrap() && s.targets[0]>s.entry.unwrap());
    assert!(s.targets.windows(2).all(|pair|pair[1]>pair[0]));
    map.expire(180_000,true);
    assert_eq!(map.setups[0].state,"EXPIRED");assert!(map.setups[0].entry.is_none());
}
#[test]
fn stop_withdraws_ready_plan_immediately_without_gateway_update() {
    let mut map=opportunities();map.price(&quote(),&RiskConfig::default(),5.0,&[],120_000);
    let mut t=quote();t.market=Some(MarketData{captured_ms:120_000,..Default::default()});
    let mut status=Status{requested_run:false,last_received_ms:Some(120_000),telemetry:Some(t),..Default::default()};
    status.analysis.entry=Some(100.0);status.analysis.lots=Some(0.1);
    status.analysis.opportunities=Some(map);status.refresh(120_100);
    assert!(status.analysis.entry.is_none() && status.analysis.lots.is_none());
    let s=&status.analysis.opportunities.as_ref().unwrap().setups[0];
    assert_eq!(s.state,"BLOCKED");assert!(s.entry.is_none() && s.targets.is_empty() && s.lots.is_none());
}
#[test]
fn stale_quote_and_withdrawal_do_not_display_old_levels() {
    let mut map=opportunities();map.price(&quote(),&RiskConfig::default(),5.0,&[],120_000);
    map.expire(120_100,false);assert!(map.quote_ms.is_none());assert!(map.setups[0].entry.is_none());
    map.withdraw("stale");assert!(!map.map_text().contains("RSI"));assert!(!map.signals_text().contains("98.50000"));
    map.price(&quote(),&RiskConfig::default(),5.0,&[],120_200);
    assert!(map.setups[0].entry.is_none());
}
#[test]
fn spread_and_opposing_obstacle_block_opportunity() {
    let mut map=opportunities();let mut t=quote();t.ask=Some(100.3);
    map.price(&t,&RiskConfig::default(),5.0,&[],120_000);assert_eq!(map.setups[0].state,"BLOCKED");
    map.setups[0].obstacle=Some(101.0);map.price(&quote(),&RiskConfig::default(),5.0,&[],120_000);
    assert!(map.setups[0].entry.is_none());assert!(!map.setups[0].gates.is_empty());
}
#[test]
fn only_one_ready_candidate_and_risk_gate_never_bypassed() {
    let mut map=opportunities();map.setups.push(setup());
    map.price(&quote(),&RiskConfig::default(),5.0,&[],120_000);
    assert_eq!(map.setups.iter().filter(|s|s.state=="READY").count(),1);
    map.price(&quote(),&RiskConfig::default(),5.0,&["daily limit".into()],120_000);
    assert!(map.setups.iter().all(|s|s.state=="BLOCKED" && s.gates.contains(&"daily limit".into())));
}
#[test]
fn dom_requires_two_valid_uncrossed_sides() {
    let mut t=quote();t.dom_available=true;
    t.dom=vec![DomLevel{side:"buy".into(),price:100.0,volume:5.0},DomLevel{side:"sell".into(),price:100.1,volume:1.0}];
    assert!(dom::assess(&t).available);assert_eq!(dom::assess(&t).direction,1);
    t.dom[1].price=99.0;assert!(!dom::assess(&t).available);
    t.dom.pop();assert!(!dom::assess(&t).available);
}
#[test]
fn short_series_never_fabricates_a_scan() {
    let out=scan(&MarketData::default(),"TEST",&TrendBrain::default(),&RegimeBrain::default(),120_000);
    assert!(!out.current && out.setups.is_empty() && out.gann.is_empty());
}
