//! Read-only scorecard for *closed, reconciled MT5 Demo* Sniper Learner policy
//! outcomes. Never aggregates Paper, live, open, unverified or shadow PnL.
//! Group identities include frozen strategy definition and regime: different
//! versions must not be compared as if they were the same trading policy.
use anyhow::Result;
use rusqlite::{params,Connection};
use serde_json::{json,Value};
use std::collections::{BTreeMap,HashSet};

const MAX_RECENT: i64 = 800;
const MIN_GROUP:usize=30;
const RECENT_WINDOW:usize=12;
const MAX_GROUPS:usize=16;
#[derive(Default)]
struct Group {
    positions:Vec<Entry>,
}
struct Entry {position:u64,closed:u64,net:f64,gross:f64,volume:f64,cost:f64,r:Option<f64>}
fn positive(v:&Value)->Option<f64>{
    v.as_f64().filter(|x|x.is_finite() && *x>0. && *x<1e10)
}
fn finite(v:&Value)->Option<f64>{
    v.as_f64().filter(|x|x.is_finite() && x.abs()<1e12)
}
fn score(rows:&[Entry])->Value{
    if rows.is_empty(){return json!({"samples":0,"win_rate":null,
        "net_booked":null,"mean_net_per_lot":null,"profit_factor":null,
        "gross_positive_net_nonpositive":0,"estimated_cost_drag":null,
        "mean_r_multiple":null});}
    let n=rows.len();
    let win=rows.iter().filter(|x|x.net>0.).count();
    let net:f64=rows.iter().map(|x|x.net).sum();
    let mean_per_lot:f64=rows.iter().map(|x|x.net/x.volume).sum::<f64>()/n as f64;
    let gains:f64=rows.iter().map(|x|x.net.max(0.)).sum();
    let losses:f64=rows.iter().map(|x|(-x.net).max(0.)).sum();
    let rs:Vec<f64>=rows.iter().filter_map(|x|x.r).collect();
    json!({
        "samples":n,
        "wins_after_costs":win,
        "win_rate":win as f64/n as f64,
        "net_booked":net,
        "mean_net_per_lot":mean_per_lot,
        "profit_factor":if losses>0.{Some(gains/losses)}else{None},
        "profit_factor_zero_losses_is_undefined":losses==0.,
        "gross_positive_net_nonpositive":rows.iter().filter(|x|x.gross>0.&&x.net<=0.).count(),
        "estimated_cost_drag":rows.iter().map(|x|x.cost).sum::<f64>(),
        "mean_r_multiple":if rs.is_empty(){None}else{Some(rs.iter().sum::<f64>()/rs.len() as f64)},
        "r_sample_count":rs.len(),
        "first_closed_ms":rows.first().map(|x|x.closed),
        "last_closed_ms":rows.last().map(|x|x.closed)
    })
}
fn bounded_key(v:&Value)->Option<String>{
    let name=v.as_str()?;
    if name.is_empty()||name.len()>256||name.chars().any(|c|c.is_control()){None}else{Some(name.to_owned())}
}
fn read(pos:u64,closed:u64,v:&Value)->Option<(String,String,String,Entry)>{
    if pos==0||closed==0||v["evidence_source"]!="mt5_demo_broker"
       ||v["trading_mode"]!="sniper"
       ||v["brain"]!="sniper_learner"
       ||v["selection"]["brain"]!="sniper_learner"
       ||v["selection"]["policy"]!="sniper-ticks-v1"
       ||v["position_id"].as_u64()!=Some(pos)
       ||v["closed_ms"].as_u64()!=Some(closed){return None;}
    let policy=bounded_key(&v["selection"]["sniper_policy"])?;
    if !matches!(policy.as_str(),"impulse"|"rebound"){return None;}
    let definition=bounded_key(&v["selection"]["sniper_definition"])?;
    let regime=bounded_key(&v["selection"]["sniper_regime"])?;
    // Match the actual Sniper/Coordinator training admission gate.
    // A financial sum alone does not attest that stored reward/R came from
    // the booked broker net and the frozen risk at entry.
    let (net,net_per_lot,_reward)=
        crate::coordinator_learning::verified_broker_feedback(v)?;
    let volume=positive(&v["closed_volume"])?;
    let gross=finite(&v["gross"])?;
    let commission=finite(&v["commission"])?;
    let swap=finite(&v["swap"])?;
    let fee=finite(&v["fee"])?;
    if (net/volume-net_per_lot).abs()>1e-6_f64.max(net_per_lot.abs()*1e-8){
        return None;
    }
    let e=Entry{position:pos,closed,net,gross,volume,
        cost:commission+swap+fee,
        r:Some(finite(&v["r_multiple"]) ?)};
    Some((definition,regime,policy,e))
}
pub fn report(db:&Connection,context:&str)->Result<Value>{
    if context.is_empty(){return Ok(json!({"status":"unbound",
        "closed_demo_positions":0,"no_execution_effect":true}));}
    let mut stmt=db.prepare(
        "SELECT source,position_id,closed_ms,payload FROM coordinator_broker_samples
         WHERE context=?1 AND brain='sniper_learner'
           AND json_extract(payload,'$.trading_mode')='sniper'
         ORDER BY closed_ms DESC,position_id DESC LIMIT ?2")?;
    let data=stmt.query_map(params![context,MAX_RECENT],
        |r|Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,
            r.get::<_,u64>(2)?,r.get::<_,String>(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let scanned=data.len();
    let mut groups=BTreeMap::<(String,String,String),Group>::new();
    let mut seen=HashSet::<u64>::new();
    let mut duplicate=0usize;let mut invalid=0usize;
    // newest row per broker position gets one vote; one broker position can
    // yield multiple advisor learning samples, but must not be counted twice.
    for (_source,pos,closed,raw) in data {
        if !seen.insert(pos){duplicate+=1;continue;}
        let parsed=serde_json::from_str::<Value>(&raw);
        let Some((definition,regime,policy,sample))=
            parsed.ok().and_then(|v|read(pos,closed,&v)) else {invalid+=1;continue;};
        groups.entry((definition,regime,policy)).or_default().positions.push(sample);
    }
    let count:usize=groups.values().map(|g|g.positions.len()).sum();
    let unique_definitions=groups.keys().map(|(d,_,_)|d.as_str())
        .collect::<std::collections::BTreeSet<_>>().len();
    let group_count=groups.len();
    let mut out=Vec::<Value>::new();
    for ((definition,regime,policy),mut group) in groups {
        group.positions.sort_by_key(|e|(e.closed,e.position));
        let n=group.positions.len();
        let old=n.saturating_sub(RECENT_WINDOW);
        let recent=&group.positions[old..];
        let metrics=score(&group.positions);
        let recent_metrics=score(recent);
        let status=if n<MIN_GROUP{"insufficient_demo_sample"}
            else if recent_metrics["mean_net_per_lot"].as_f64().is_some_and(|v|v<=0.){
                "recent_nonpositive_net_monitor"
            }else{"descriptive_monitor_only"};
        out.push(json!({
            "definition":definition,"regime":regime,"policy":policy,
            "status":status,"sample_limit":MAX_RECENT,
            "all":metrics,"recent":recent_metrics,
            "older":score(&group.positions[..old]),
            "recent_window":RECENT_WINDOW,
            "last_closed_ms":group.positions.last().map(|x|x.closed),
            "no_statistical_edge_certification":true,
        }));
    }
    out.sort_by(|a,b|{
        b["all"]["samples"].as_u64().cmp(&a["all"]["samples"].as_u64())
            .then_with(||b["last_closed_ms"].as_u64().cmp(&a["last_closed_ms"].as_u64()))
            .then_with(||a["policy"].as_str().cmp(&b["policy"].as_str()))
    });
    // A large total pooled across many tiny strategies is not adequate
    // evidence for any one strategy/market/revision cohort.
    let any_group_descriptive=out.iter().any(|v|
        v["all"]["samples"].as_u64().unwrap_or(0)>=MIN_GROUP as u64);
    out.truncate(MAX_GROUPS);
    Ok(json!({
        "status":if !any_group_descriptive{"insufficient_closed_demo_evidence"}
            else{"descriptive_broker_outcome_monitor_only"},
        "source":"coordinator_broker_samples: MT5 Demo closed positions with audited learner reward/risk",
        "broker_admission_gate":"coordinator_learning::verified_broker_feedback",
        "learning_eligible_broker_samples_only":true,
        "legacy_unverified_samples_excluded_not_deleted":true,
        "policy":"sniper_policy_demo_quality_v1",
        "account_scoped":true,
        "mode_scoped":"sniper",
        "sample_cap":MAX_RECENT,"sampled_rows":scanned,
        "closed_demo_positions":count,
        "invalid_or_unattributed_samples":invalid,
        "duplicate_position_samples":duplicate,
        "distinct_definitions":unique_definitions,
        "distinct_policy_regime_definition_groups":group_count,
        "shown_groups":out.len(),
        "groups":out,
        "paper_forward_live_open_excluded":true,
        "net_includes_booked_commission_swap_fee":true,
        "promotion_authorized":false,
        "no_execution_effect":true,
        "learning_effect":false,
        "note":"Each group is one fixed strategy-definition/regime/policy. Multiple strategy versions are never pooled. Recent monitoring may already influence the adaptive model: NOT untouched out-of-sample. No win-rate claim without sufficient unbiased forward evidence and real Demo broker test."
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db()->Connection{
        let d=Connection::open_in_memory().unwrap();
        d.execute_batch("CREATE TABLE coordinator_broker_samples(
            source TEXT NOT NULL,position_id INTEGER NOT NULL,context TEXT NOT NULL,
            brain TEXT NOT NULL,closed_ms INTEGER NOT NULL,payload TEXT NOT NULL);").unwrap();d
    }
    fn add(d:&Connection,ctx:&str,pos:u64,policy:&str,definition:&str,regime:&str,net:f64,source:&str){
        let p=json!({"evidence_source":source,"trading_mode":"sniper",
            "brain":"sniper_learner","position_id":pos,"closed_ms":pos*1000,
            "gross":net+0.70,"commission":-0.70,"fee":0.0,"swap":0.0,
            "net":net,"net_per_lot":net/0.10,"closed_volume":0.10,
            "risk_money":5.0,"r_multiple":net/5.0,
            "reward":(net/5.0).tanh(),
            "selection":{"brain":"sniper_learner","policy":"sniper-ticks-v1",
              "sniper_policy":policy,"sniper_definition":definition,"sniper_regime":regime}});
        d.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,?3,'sniper_learner',?4,?5)",
            params![format!("f{pos}"),pos,ctx,pos*1000,p.to_string()]).unwrap();
    }
    #[test]
    fn exact_policy_regime_and_definition_isolation_and_cost_net(){
        let d=db();
        for i in 1..=35 {add(&d,"a",i,"impulse","v1","trending",2.,"mt5_demo_broker");}
        add(&d,"a",100,"rebound","v1","trending",-1.,"mt5_demo_broker");
        add(&d,"a",101,"impulse","v2","trending",-1.,"mt5_demo_broker");
        add(&d,"a",102,"impulse","v1","choppy",-1.,"mt5_demo_broker");
        add(&d,"b",103,"impulse","v1","trending",500.,"mt5_demo_broker");
        add(&d,"a",104,"impulse","v1","trending",500.,"paper");
        let s=report(&d,"a").unwrap();
        assert_eq!(s["closed_demo_positions"],38);
        assert_eq!(s["invalid_or_unattributed_samples"],1);
        assert_eq!(s["distinct_policy_regime_definition_groups"],4);
        assert_eq!(s["groups"][0]["all"]["samples"],35);
        assert_eq!(s["groups"][0]["all"]["net_booked"],70.);
        assert!((s["groups"][0]["all"]["estimated_cost_drag"].as_f64().unwrap()+24.5).abs()<1e-8);
        assert_eq!(s["groups"][0]["all"]["win_rate"],1.);
        assert_eq!(s["groups"][0]["all"]["profit_factor"],Value::Null);
        assert_eq!(s["promotion_authorized"],false);
        assert_eq!(s["paper_forward_live_open_excluded"],true);
    }
    #[test]
    fn contaminated_fee_sum_and_duplicate_position_rejected(){
        let d=db();
        add(&d,"a",1,"impulse","v1","trending",1.,"mt5_demo_broker");
        add(&d,"a",1,"impulse","v1","trending",1.,"mt5_demo_broker");
        let corrupted=json!({"evidence_source":"mt5_demo_broker","trading_mode":"sniper",
            "brain":"sniper_learner","position_id":5,"closed_ms":5000,
            "gross":100.,"commission":-7.,"swap":0.,"fee":0.,"net":100.,
            "closed_volume":0.1,"selection":{"brain":"sniper_learner","policy":"sniper-ticks-v1",
            "sniper_policy":"impulse","sniper_definition":"v1","sniper_regime":"trending"}});
        d.execute("INSERT INTO coordinator_broker_samples VALUES('bad',5,'a','sniper_learner',5000,?1)",
            [corrupted.to_string()]).unwrap();
        let r=report(&d,"a").unwrap();
        assert_eq!(r["duplicate_position_samples"],1);
        assert_eq!(r["invalid_or_unattributed_samples"],1);
        assert_eq!(r["closed_demo_positions"],1);
    }
    #[test]
    fn g5g_sniper_scorecard_rejects_reward_poisoning_and_missing_risk(){
        let d=db();
        add(&d,"a",1,"impulse","v1","trending",2.0,"mt5_demo_broker");
        for (pos,variant) in [(2,"reward"),(3,"risk"),(4,"r"),(5,"volume")] {
            let mut p=json!({"evidence_source":"mt5_demo_broker",
                "trading_mode":"sniper","brain":"sniper_learner",
                "position_id":pos,"closed_ms":pos*1000,
                "gross":2.7,"commission":-0.7,"swap":0.0,"fee":0.0,
                "net":2.0,"net_per_lot":20.0,"closed_volume":0.1,
                "risk_money":5.0,"r_multiple":0.4,"reward":0.4_f64.tanh(),
                "selection":{"brain":"sniper_learner","policy":"sniper-ticks-v1",
                    "sniper_policy":"impulse","sniper_definition":"v1",
                    "sniper_regime":"trending"}});
            match variant {
                "reward"=>p["reward"]=json!(0.99),
                "risk"=>{p.as_object_mut().unwrap().remove("risk_money");},
                "r"=>p["r_multiple"]=json!(-7.0),
                "volume"=>p["net_per_lot"]=json!(999.0),
                _=>unreachable!()
            }
            d.execute("INSERT INTO coordinator_broker_samples
                VALUES(?1,?2,'a','sniper_learner',?3,?4)",
                params![format!("bad-{pos}"),pos,pos*1000,p.to_string()]).unwrap();
        }
        let report=report(&d,"a").unwrap();
        assert_eq!(report["sampled_rows"],5);
        assert_eq!(report["closed_demo_positions"],1);
        assert_eq!(report["invalid_or_unattributed_samples"],4);
        assert_eq!(report["groups"][0]["all"]["samples"],1);
        assert_eq!(report["groups"][0]["all"]["net_booked"],2.0);
        assert_eq!(report["learning_eligible_broker_samples_only"],true);
        assert_eq!(report["promotion_authorized"],false);
    }
    #[test]
    fn cost_drag_and_recent_deterioration_are_not_certified_edge(){
        let d=db();
        for i in 1..=40 {
            let net=if i<=28{1.}else{-2.};
            add(&d,"a",i,"impulse","v1","trending",net,"mt5_demo_broker");
        }
        let s=report(&d,"a").unwrap();
        assert_eq!(s["groups"][0]["status"],"recent_nonpositive_net_monitor");
        assert_eq!(s["groups"][0]["recent"]["win_rate"],0.);
        assert_eq!(s["groups"][0]["older"]["samples"],28);
        assert_eq!(s["groups"][0]["all"]["net_booked"],4.);
        assert_eq!(s["groups"][0]["no_statistical_edge_certification"],true);
    }
    #[test]
    fn pooled_small_cohorts_do_not_pass_sample_minimum(){
        let d=db();
        for i in 1..=20{add(&d,"a",i,"impulse","v1","trending",1.,"mt5_demo_broker");}
        for i in 21..=40{add(&d,"a",i,"rebound","v1","trending",1.,"mt5_demo_broker");}
        let r=report(&d,"a").unwrap();
        assert_eq!(r["closed_demo_positions"],40);
        assert_eq!(r["status"],"insufficient_closed_demo_evidence");
        assert_eq!(r["groups"][0]["status"],"insufficient_demo_sample");
        assert_eq!(r["groups"][1]["status"],"insufficient_demo_sample");
    }
    #[test]
    fn empty_context_and_no_samples_are_not_success(){
        let d=db();
        assert_eq!(report(&d,"") .unwrap()["status"],"unbound");
        let r=report(&d,"a").unwrap();
        assert_eq!(r["status"],"insufficient_closed_demo_evidence");
        assert_eq!(r["closed_demo_positions"],0);
    }
}
