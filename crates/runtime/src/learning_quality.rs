//! Read-only execution-learning evidence audit.
//! Broker outcomes are real closed MT5 DEMO deals after costs; paper, causal
//! forward bars, and unclosed/live executions NEVER become broker labels here.
//! "Recent" is a temporal monitoring window, NOT untouched out-of-sample:
//! online learning may already have consumed these outcomes.
use anyhow::Result;
use braintrade_domain::trading::TradingMode;
use rusqlite::{params,Connection};
use serde_json::{json,Value};
use std::collections::{BTreeSet,HashSet};

const MIN_TOTAL:usize=30;
const RECENT_TRADES:usize=12;
const MAX_HISTORY:usize=240;

#[derive(Clone,Debug)]
struct Sample {
    pos:u64,
    closed:u64,
    source:String,
    gross:f64,
    cost:f64,
    net:f64,
    volume:f64,
    r:Option<f64>,
}
fn table_exists(db:&Connection)->Result<bool>{
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='coordinator_broker_samples')",[],|r|r.get(0))?)
}
fn wilson_lower(wins:usize,n:usize)->Option<f64>{
    if n==0{return None}
    let n=n as f64;let p=wins as f64/n;let z=1.96;
    let denom=1.+z*z/n;
    Some(((p+z*z/(2.*n)-z*((p*(1.-p)+z*z/(4.*n))/n).sqrt())/denom).clamp(0.,1.))
}
fn summarize(rows:&[Sample])->Value {
    if rows.is_empty(){return json!({"samples":0,"wins_after_costs":0,"losses_after_costs":0,"breakeven_after_costs":0,"win_rate":null,"wilson_95_lower":null,"net_booked":0.,"mean_net_per_lot":null,"profit_factor":null,"mean_r_multiple":null,"cost_drag":0.,"gross_positive_but_net_nonpositive":0})}
    let n=rows.len();let wins=rows.iter().filter(|r|r.net>0.).count();
    let losses=rows.iter().filter(|r|r.net<0.).count();
    let booked:f64=rows.iter().map(|r|r.net).sum();
    let per_lot:f64=rows.iter().map(|r|r.net/r.volume).sum::<f64>()/n as f64;
    let gross_gain:f64=rows.iter().map(|r|r.net.max(0.)).sum();
    let gross_loss:f64=-rows.iter().map(|r|r.net.min(0.)).sum::<f64>();
    let sum_r:f64=rows.iter().filter_map(|r|r.r).sum();
    let nr=rows.iter().filter(|r|r.r.is_some()).count();
    json!({
        "samples":n,
        "wins_after_costs":wins,
        "losses_after_costs":losses,
        "breakeven_after_costs":n-wins-losses,
        "win_rate":wins as f64/n as f64,
        "wilson_95_lower":wilson_lower(wins,n),
        "net_booked":booked,
        "mean_net_per_lot":per_lot,
        "profit_factor":if gross_loss>0.{Some(gross_gain/gross_loss)}else{None},
        "profit_factor_note":if gross_loss==0.{"No losing trades observed; ratio undefined"}else{"Profit divided by abs(loss), after MT5 booked costs"},
        "mean_r_multiple":if nr>0 {Some(sum_r/nr as f64)}else{None},
        "r_sample_count":nr,
        "cost_drag":rows.iter().map(|r|r.cost).sum::<f64>(),
        "gross_positive_but_net_nonpositive":rows.iter().filter(|r|r.gross>0. && r.net<=0.).count(),
        "first_closed_ms":rows.first().map(|r|r.closed),
        "last_closed_ms":rows.last().map(|r|r.closed),
    })
}
fn read_sample(source:String,pos:u64,closed:u64,context:&str,mode:&str,brain:&str,p:&Value)->Option<Sample>{
    if pos==0||closed==0 ||p["context"]!=context||p["trading_mode"]!=mode
        ||p["brain"]!=brain||p["position_id"].as_u64()!=Some(pos)
        ||p["closed_ms"].as_u64()!=Some(closed){return None}
    // Performance and learner weights must use the SAME broker admission
    // contract. Financially plausible receipts with forged/missing reward or
    // risk cannot be represented as verified learning performance.
    let (net,net_per_lot,_reward)=
        crate::coordinator_learning::verified_broker_feedback(p)?;
    let g=p["gross"].as_f64()?;
    let fee=p["fee"].as_f64()?;
    let swap=p["swap"].as_f64()?;
    let com=p["commission"].as_f64()?;
    let volume=p["closed_volume"].as_f64()?;
    let reported_per_lot=net/volume;
    if (reported_per_lot-net_per_lot).abs()>1e-6_f64.max(net_per_lot.abs()*1e-8){
        return None
    }
    Some(Sample{pos,closed,source,gross:g,cost:com+fee+swap,net,volume,
        r:Some(p["r_multiple"].as_f64()?)})
}
pub fn audit(db:&Connection,context:&str,mode:TradingMode,brain:&str)->Result<Value>{
    let mode_key=serde_json::to_value(mode)?;
    let mode_key=mode_key.as_str().unwrap_or("");
    if context.is_empty()||!table_exists(db)?{
        return Ok(json!({"status":"insufficient_evidence","broker_samples":0,"invalid_samples":0,"policy":"read_only","note":"No current bound MT5 context or broker evidence table"}))
    }
    // Scope by broker account+symbol, trading mode and brain. Count only exact
    // closed Demo samples; never attribute paper/forward samples as real fills.
    let mut stmt=db.prepare(
      "SELECT source,position_id,closed_ms,payload FROM coordinator_broker_samples
       WHERE context=?1 AND brain=?2
         AND CASE WHEN json_valid(payload)
             THEN json_extract(payload,'$.trading_mode') ELSE NULL END=?3
       ORDER BY closed_ms DESC,position_id DESC LIMIT 240")?;
    let raw=stmt.query_map(params![context,brain,mode_key],|r|{
       Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,
           r.get::<_,u64>(2)?,r.get::<_,String>(3)?))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    // Corrupt JSON has no attestable trading mode. Count it separately in
    // this broker context/brain, never guess its mode or break the report.
    let malformed_unknown_mode:u64=db.query_row(
       "SELECT COUNT(*) FROM coordinator_broker_samples
        WHERE context=?1 AND brain=?2 AND NOT json_valid(payload)",
       params![context,brain],|r|r.get(0))?;
    let mut records=Vec::new();
    let mut invalid=0usize;let mut duplicates=0usize;
    let mut seen=HashSet::new();let mut sources=BTreeSet::new();
    for (source,pos,closed,payload) in raw.into_iter().rev(){
        // Reconciliation or policy revision cannot double-credit one position
        // for the same brain/mode. Other brains may be credited separately,
        // but those are correlated labels, not independent market outcomes.
        if !seen.insert(pos){duplicates+=1;continue}
        let parsed=serde_json::from_str::<Value>(&payload);
        let Some(row)=parsed.ok().and_then(|v|read_sample(source,pos,closed,context,mode_key,brain,&v)) else{
            invalid+=1;continue
        };
        sources.insert(row.source.clone());
        records.push(row);
    }
    let total=records.len();
    let split=total.saturating_sub(RECENT_TRADES);
    let earlier=&records[..split];let recent=&records[split..];
    let old=summarize(earlier);let new=summarize(recent);
    let all=summarize(&records);
    let warnings=if total<MIN_TOTAL{
       vec!["insufficient_closed_demo_broker_samples"]
    }else if sources.len()>1{
       vec!["mixed_strategy_source_or_timeframe"]
    }else if new["mean_net_per_lot"].as_f64().is_some_and(|v|v<=0.){
       vec!["recent_net_expectancy_nonpositive"]
    }else{vec![]};
    let status=if total<MIN_TOTAL{"insufficient_evidence"}
        else if sources.len()>1{"mixed_policy_monitor_only"}
        else if new["mean_net_per_lot"].as_f64().is_some_and(|v|v<=0.){"recent_deterioration"}
        else{"monitor_only"};
    Ok(json!({
       "schema":"closed-demo-learning-quality-v1",
       "status":status,"warnings":warnings,
       "policy":"read_only","broker_samples":total,
       "invalid_samples":invalid,"duplicate_position_samples":duplicates,
       "malformed_json_unknown_mode_rows":malformed_unknown_mode,
       "sample_cap":MAX_HISTORY,
       "distinct_strategy_sources":sources.len(),
       "minimum_samples_for_comparison":MIN_TOTAL,
       "recent_window":RECENT_TRADES,
       "all":all,"earlier":old,"recent":new,
       "source":"Closed MT5 DEMO deal records with verified reward/risk and row identity; net=gross+commission+swap+fee",
       "broker_admission_gate":"verified_broker_feedback + strict context/mode/brain/position/close identity",
       "learning_eligible_broker_samples_only":true,
       "legacy_unverified_samples_excluded_not_deleted":true,
       "source_scope":"bound account + symbol + trading mode + brain; strategy source revisions remain visible",
       "temporal_warning":"The recent chronological window is not independent out-of-sample: an online learner may already have used the results.",
       "no_auto_promotion":true,"no_execution_change":true,
       "paper_and_forward_excluded":true,
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup()->Connection {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE coordinator_broker_samples(source TEXT NOT NULL,position_id INTEGER NOT NULL,context TEXT NOT NULL,brain TEXT NOT NULL,closed_ms INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source,position_id));").unwrap();
        db
    }
    fn add(db:&Connection,context:&str,mode:&str,brain:&str,source:&str,id:u64,net:f64,gross:f64){
        let payload=json!({"evidence_source":"mt5_demo_broker","trading_mode":mode,
            "context":context,"position_id":id,"closed_ms":id*1000,"brain":brain,
            "net":net,"gross":gross,"commission":net-gross,"swap":0.,"fee":0.,"closed_volume":0.1,
            "net_per_lot":net/0.1,"risk_money":5.0,
            "r_multiple":net/5.0,"reward":(net/5.0).tanh()});
        db.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,?3,?4,?5,?6)",
            params![source,id,context,brain,id*1000,payload.to_string()]).unwrap();
    }
    #[test]
    fn empty_evidence_is_not_a_promotion_signal(){
        let db=setup();
        let v=audit(&db,"A",TradingMode::Expert,"trend").unwrap();
        assert_eq!(v["status"],"insufficient_evidence");
        assert_eq!(v["broker_samples"],0);
        assert_eq!(v["no_auto_promotion"],true);
    }
    #[test]
    fn strictly_scoped_costed_temporal_broker_evidence(){
        let db=setup();
        for id in 1..=34{
            let net=if id<=22 {2.}else{-1.};
            add(&db,"A","expert","trend","policy-m5",id,net,net+0.70);
        }
        add(&db,"B","expert","trend","elsewhere",700,20.,21.);
        add(&db,"A","sniper","trend","othermode",701,20.,21.);
        add(&db,"A","expert","wyckoff","different-brain",702,20.,21.);
        let v=audit(&db,"A",TradingMode::Expert,"trend").unwrap();
        assert_eq!(v["broker_samples"],34);
        assert_eq!(v["earlier"]["samples"],22);
        assert_eq!(v["recent"]["samples"],12);
        assert_eq!(v["status"],"recent_deterioration");
        assert_eq!(v["recent"]["win_rate"],0.);
        assert_eq!(v["all"]["gross_positive_but_net_nonpositive"],0);
        assert_eq!(v["no_auto_promotion"],true);
        assert_eq!(v["paper_and_forward_excluded"],true);
    }
    #[test]
    fn g5g_report_counts_only_verified_learning_receipts_not_forged_reward_or_missing_risk(){
        let db=setup();
        add(&db,"A","expert","trend","policy-m5",1,2.0,2.7);
        for (id,field) in [(2,"reward"),(3,"risk_money"),(4,"r_multiple"),
                           (5,"net_per_lot")] {
            let mut p=json!({"evidence_source":"mt5_demo_broker",
                "trading_mode":"expert","context":"A","brain":"trend",
                "position_id":id,"closed_ms":id*1000,"net":2.0,"gross":2.7,
                "commission":-0.7,"swap":0.0,"fee":0.0,
                "closed_volume":0.1,"net_per_lot":20.0,
                "risk_money":5.0,"r_multiple":0.4,"reward":0.4_f64.tanh()});
            if id==2 {p["reward"]=json!(0.99);}
            else {p.as_object_mut().unwrap().remove(field);}
            db.execute("INSERT INTO coordinator_broker_samples
                VALUES('policy-m5',?1,'A','trend',?2,?3)",
                params![id,id*1000,p.to_string()]).unwrap();
        }
        let view=audit(&db,"A",TradingMode::Expert,"trend").unwrap();
        assert_eq!(view["broker_samples"],1);
        assert_eq!(view["invalid_samples"],4);
        assert_eq!(view["all"]["net_booked"],2.0);
        assert_eq!(view["all"]["r_sample_count"],1);
        assert_eq!(view["learning_eligible_broker_samples_only"],true);
        assert_eq!(view["no_auto_promotion"],true);
    }
    #[test]
    fn g5g_malformed_legacy_json_cannot_crash_quality_or_fake_samples(){
        let db=setup();
        add(&db,"A","expert","trend","policy-m5",1,2.0,2.7);
        db.execute("INSERT INTO coordinator_broker_samples VALUES(
            'broken',2,'A','trend',2000,'{invalid-json')",[]).unwrap();
        let report=audit(&db,"A",TradingMode::Expert,"trend").unwrap();
        assert_eq!(report["broker_samples"],1);
        assert_eq!(report["all"]["net_booked"],2.0);
        assert_eq!(report["malformed_json_unknown_mode_rows"],1);
        assert_eq!(report["no_auto_promotion"],true);
    }
    #[test]
    fn g5g_mismatched_broker_payload_identity_is_never_brain_performance(){
        let db=setup();
        add(&db,"A","expert","trend","policy-m5",1,2.0,2.7);
        let variants=[("context",json!("B")),("brain",json!("sniper_learner")),
            ("position_id",json!(999_u64)),("closed_ms",json!(999_u64)),
            ("trading_mode",json!("scalping"))];
        for (n,(field,value)) in variants.iter().enumerate(){
            let pos=(n+2) as u64;
            let mut p=json!({"evidence_source":"mt5_demo_broker","context":"A",
                "trading_mode":"expert","brain":"trend","position_id":pos,
                "closed_ms":pos*1000,"gross":2.7,"commission":-0.7,
                "swap":0.0,"fee":0.0,"net":2.0,"closed_volume":0.1,
                "net_per_lot":20.0,"risk_money":5.0,"r_multiple":0.4,
                "reward":0.4_f64.tanh()});
            p[*field]=value.clone();
            db.execute("INSERT INTO coordinator_broker_samples VALUES(
                'policy-m5',?1,'A','trend',?2,?3)",
                params![pos,pos*1000,p.to_string()]).unwrap();
        }
        // Deliberately change an existing row after the original Demo label:
        // a correct net cannot authorize wrong broker-attribution provenance.
        let v=audit(&db,"A",TradingMode::Expert,"trend").unwrap();
        assert_eq!(v["broker_samples"],1);
        // mode-mismatched payload is excluded by the original SQL mode scope,
        // so only 4 corrupted rows were returned for read-time rejection.
        assert_eq!(v["invalid_samples"],4);
        assert_eq!(v["all"]["net_booked"],2.0);
    }
    #[test]
    fn mixed_sources_and_corrupt_broker_evidence_do_not_claim_quality(){
        let db=setup();
        for id in 1..=34{
            add(&db,"A","expert","trend",if id<=16{"v1"}else{"v2"},id,1.,1.7);
        }
        let bad=json!({"evidence_source":"paper","trading_mode":"expert","net":999.,
            "gross":999.,"commission":0.,"swap":0.,"fee":0.,"closed_volume":0.1});
        db.execute("INSERT INTO coordinator_broker_samples VALUES('bad',55,'A','trend',55000,?1)",
            [bad.to_string()]).unwrap();
        let result=audit(&db,"A",TradingMode::Expert,"trend").unwrap();
        assert_eq!(result["status"],"mixed_policy_monitor_only");
        assert_eq!(result["invalid_samples"],1);
        assert_eq!(result["distinct_strategy_sources"],2);
        assert_eq!(result["broker_samples"],34);
    }
}
