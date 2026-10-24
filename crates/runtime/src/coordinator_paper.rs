//! Paired prospective Expert-coordinator Shadow paper outcomes, NOT broker simulation.
//! Frozen and adaptive coordinator arms enter on one sampled MT5 Demo quote (0.01 lots
//! and same assumed costs). Each then independently exits on its own frozen
//! entry-time stop/target/holding rule. No broker commands or learner updates.
use anyhow::{Context,Result};
use braintrade_domain::{execution::Signal,Config,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

const POLICY:&str="frozen-expert-coordinator-paired-paper-v1";
const LOT:f64=0.01;
const MIN_COMPLETE:usize=30;
const GAP_WARNING_MS:u64=15_000;

#[derive(Clone,Copy)]
struct Quote{bid:f64,ask:f64,at:u64,tick:f64,profit_tick:f64,loss_tick:f64}
impl Quote {
    fn read(t:&Telemetry,now:u64)->Option<Self>{
        if !t.connected||!t.demo||t.account_trade_mode!=Some(0)
            ||t.tick_age_ms.is_none_or(|v|v>3000){return None}
        let (bid,ask)=t.bid.zip(t.ask)?;
        let at=t.quote_time_ms?;
        let tick=t.tick_size?;
        let profit_tick=t.tick_value_profit?;
        let loss_tick=t.tick_value_loss?;
        if [bid,ask,tick,profit_tick,loss_tick].iter().any(|x|!x.is_finite()||*x<=0.)
            ||bid>ask||at>now.saturating_add(2000)||now.saturating_sub(at)>3000{
            return None
        }
        Some(Self{bid,ask,at,tick,profit_tick,loss_tick})
    }
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS frozen_coordinator_trade_pairs(
            run_id TEXT NOT NULL,
            bar_time INTEGER NOT NULL,
            opened_ms INTEGER NOT NULL,
            closed_ms INTEGER,
            state TEXT NOT NULL CHECK(state IN ('open','complete')),
            payload TEXT NOT NULL,
            PRIMARY KEY(run_id,bar_time),
            FOREIGN KEY(run_id) REFERENCES frozen_coordinator_runs(id));
         CREATE INDEX IF NOT EXISTS frozen_coordinator_trade_pairs_open_idx
            ON frozen_coordinator_trade_pairs(run_id,state,opened_ms);"
    )?;
    Ok(())
}
fn valid_signal(s:&Signal)->bool{
    s.ready && matches!(s.side,-1|1)
      && s.stop_distance.is_finite() && s.stop_distance>0.
      && s.target_distance.is_finite() && s.target_distance>0.
      && s.hold_ms>0
}
fn leg(s:&Signal,q:Quote)->Value{
    let entry=if s.side==1{q.ask}else{q.bid};
    json!({
        "state":"open","side":s.side,"entry":entry,"stop_distance":s.stop_distance,
        "target_distance":s.target_distance,"hold_ms":s.hold_ms,
        "signal_key":s.key,"last_quote_ms":q.at,"max_gap_ms":0
    })
}
fn update_leg(v:&mut Value,q:Quote,opened:u64,tick:f64,
    profit_tick:f64,loss_tick:f64,commission:f64,slippage:f64)->Result<()>{
    if v["state"]!="open"||q.at<=opened{return Ok(())}
    let side=v["side"].as_i64().context("side")?;
    let entry=v["entry"].as_f64().context("entry")?;
    let stop=v["stop_distance"].as_f64().context("stop")?;
    let target=v["target_distance"].as_f64().context("target")?;
    let hold=v["hold_ms"].as_u64().context("hold")?;
    anyhow::ensure!(matches!(side,-1|1) && entry>0. && stop>0. && target>0.
        && entry.is_finite() && stop.is_finite() && target.is_finite() && hold>0,
        "Corrupt frozen pair exit policy");
    let last=v["last_quote_ms"].as_u64().unwrap_or(opened);
    if q.at<=last{return Ok(())}
    let gap=q.at-last;
    let max_gap=v["max_gap_ms"].as_u64().unwrap_or(0).max(gap);
    v["last_quote_ms"]=json!(q.at);
    v["max_gap_ms"]=json!(max_gap);
    // Bid for long exit and Ask for short exit, including spread.
    let exit=if side==1{q.bid}else{q.ask};
    let movement=(exit-entry)*(side as f64);
    let reason=if movement<=-stop{"stop"}else if movement>=target{"target"}
        else if q.at.saturating_sub(opened)>=hold{"time"}else{""};
    if reason.is_empty(){return Ok(())}
    let adjusted_ticks=movement/tick-2.*slippage;
    let gross=adjusted_ticks*if adjusted_ticks>=0.{profit_tick}else{loss_tick}*LOT;
    let fee=commission*LOT;
    let net=gross-fee;
    anyhow::ensure!([exit,gross,net,fee].iter().all(|v|v.is_finite()),
        "Invalid full-exit result");
    v["state"]=json!("closed");
    v["result"]=json!({
        "exit_ms":q.at,"exit":exit,"reason":reason,
        "gross":gross,"commission_assumed":fee,"net_paper":net,
        "net_per_lot":net/LOT,"max_quote_gap_ms":max_gap,
        "gap_risk":max_gap>GAP_WARNING_MS,
        "provenance":"sampled MT5 Demo bid/ask; NOT a broker fill",
    });
    Ok(())
}
fn settle(db:&Connection,id:&str,q:Quote)->Result<()>{
    let mut stmt=db.prepare(
        "SELECT bar_time,opened_ms,payload FROM frozen_coordinator_trade_pairs
         WHERE run_id=?1 AND state='open' ORDER BY opened_ms,bar_time")?;
    let pending=stmt.query_map([id],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for(bar,opened,raw)in pending{
        if q.at<=opened {continue}
        let mut p:Value=serde_json::from_str(&raw)?;
        let tick=p["tick"].as_f64().context("tick")?;
        let profit=p["profit_tick"].as_f64().context("profit tick")?;
        let loss=p["loss_tick"].as_f64().context("loss tick")?;
        let comm=p["commission_per_lot"].as_f64().context("commission")?;
        let slip=p["slippage_ticks"].as_f64().context("slippage")?;
        anyhow::ensure!(tick>0. && profit>0. && loss>0. && comm>=0. && slip>=0.
            && [tick,profit,loss,comm,slip].iter().all(|v|v.is_finite()),
            "Invalid stored entry cost assumptions");
        update_leg(&mut p["frozen"],q,opened,tick,profit,loss,comm,slip)?;
        update_leg(&mut p["adaptive"],q,opened,tick,profit,loss,comm,slip)?;
        let done=p["frozen"]["state"]=="closed"&&p["adaptive"]["state"]=="closed";
        db.execute(
            "UPDATE frozen_coordinator_trade_pairs
             SET state=?3,closed_ms=?4,payload=?5
             WHERE run_id=?1 AND bar_time=?2 AND state='open'",
            params![id,bar,if done{"complete"}else{"open"},
                if done{Some(q.at)}else{None},p.to_string()])?;
    }
    Ok(())
}
/// Recorded, conservative quote-time *research hint*. This never calls
/// MT5 order_check/order_calc_margin and is NOT a broker admission authority.
/// Missing contract data is UNKNOWN rather than silently approved.
fn indicative_contract(t:&Telemetry,c:&Config,f:&Signal,a:&Signal,q:Quote)->Value{
    const LOT:f64=0.01;
    let volume=match (t.volume_min,t.volume_step,t.volume_max){
        (Some(min),Some(step),Some(max))
          if [min,step,max].iter().all(|x|x.is_finite()&&*x>0.)&&min<=max=>{
            let n=(LOT-min)/step;
            if LOT+1e-9>=min && LOT<=max+1e-9
                && n>=-1e-9 && (n-n.round()).abs()<1e-6{"pass"}
            else{"fail"}
        },
        _=>"unknown"
    };
    // This is a proportional *indication*, not true per-order margin because
    // hedged positions, pricing, account leverage and conversion can differ.
    let estimated_margin=match (t.minimum_lot_margin,t.volume_min){
        (Some(m),Some(min)) if m.is_finite()&&m>0.&&min.is_finite()&&min>0.=>{
            let v=m*LOT/min;
            if v.is_finite()&&v>0.{Some(v)}else{None}
        },_=>None
    };
    let margin=match (estimated_margin,t.margin_free){
        (Some(needed),Some(free)) if free.is_finite()&&free>=0.=>{
            if free+1e-9>=needed{"pass"}else{"fail"}
        },_=>"unknown"
    };
    let permissions=if t.trade_allowed&&t.trade_expert{"pass"}else{"fail"};
    let budget=if t.equity.is_finite()&&t.equity>0.
        &&c.risk.risk_percent.is_finite()&&c.risk.risk_percent>0.{
            Some(t.equity*c.risk.risk_percent/100.)
        }else{None};
    let risk=|signal:&Signal|{
        let risk=(signal.stop_distance/q.tick
            *q.loss_tick.max(q.profit_tick)*LOT)
            +LOT*c.risk.commission_per_lot
            +2.*c.risk.slippage_ticks*q.loss_tick.max(q.profit_tick)*LOT;
        let valid=risk.is_finite()&&risk>=0.
            &&signal.stop_distance.is_finite()&&signal.stop_distance>0.
            &&c.risk.commission_per_lot.is_finite()&&c.risk.commission_per_lot>=0.
            &&c.risk.slippage_ticks.is_finite()&&c.risk.slippage_ticks>=0.;
        let state=match (valid,budget){
            (true,Some(max))=>if risk<=max+1e-9{"pass"}else{"fail"},
            _=>"unknown"
        };
        json!({"status":state,"estimated_loss_at_stop":if valid{Some(risk)}else{None},
            "account_risk_budget":budget})
    };
    let fr=risk(f);
    let ar=risk(a);
    let parts=[volume,margin,permissions,
        fr["status"].as_str().unwrap_or("unknown"),
        ar["status"].as_str().unwrap_or("unknown")];
    let status=if parts.contains(&"fail"){"fail"}
        else if parts.contains(&"unknown"){"unknown"}
        else{"pass"};
    json!({"status":status,"volume":volume,"margin":margin,
        "permissions":permissions,"frozen_stop_risk":fr,"adaptive_stop_risk":ar,
        "requested_paper_lots":LOT,
        "broker_min_volume":t.volume_min,"broker_volume_step":t.volume_step,
        "broker_max_volume":t.volume_max,
        "broker_margin_free":t.margin_free,
        "estimated_margin_linear":estimated_margin,
        "assumption":"Informational conservative signal-contract check only. Does NOT include full broker order_check, current order price normalization, exact order_calc_margin/order_calc_profit, portfolio slots, pending intents, cooldown, market execution, or fills.",
        "broker_effect":false})
}

fn observe_trial(db:&Connection,id:&str,created:u64,active:bool,decision_bar:u64,
    frozen:&Signal,adaptive:&Signal,t:&Telemetry,c:&Config,now:u64)->Result<()>{
    let Some(q)=Quote::read(t,now)else{return Ok(())};
    settle(db,id,q)?;
    if !active || !valid_signal(frozen)||!valid_signal(adaptive)
        ||decision_bar==0
        ||decision_bar.checked_mul(1000).is_none_or(|ts|ts<=created||ts>now)
        ||!c.risk.commission_per_lot.is_finite()||c.risk.commission_per_lot<0.
        ||!c.risk.slippage_ticks.is_finite()||c.risk.slippage_ticks<0. {
        return Ok(())
    }
    // This is a strict shared opportunity cohort: no overlapping pairs per
    // frozen policy. Do not count another signal until BOTH exits are closed.
    let pending:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_coordinator_trade_pairs
         WHERE run_id=?1 AND state='open')", [id],|r|r.get(0))?;
    if pending{return Ok(())}
    // A ready advisor signal may remain true across several M1 observations.
    // Broker seen-keys would not execute it again: never manufacture another
    // profitable paper pair from that SAME underlying frozen or adaptive key.
    if frozen.key.trim().is_empty()||adaptive.key.trim().is_empty(){return Ok(())}
    let replayed:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_coordinator_trade_pairs
         WHERE run_id=?1 AND
          (json_extract(payload,'$.frozen.signal_key')=?2
           OR json_extract(payload,'$.adaptive.signal_key')=?3))",
        params![id,frozen.key,adaptive.key],|r|r.get(0))?;
    if replayed{return Ok(())}
    let payload=json!({
        "policy":POLICY,"observed_ms":q.at,"bar_time":decision_bar,
        "source":"MT5 Demo BID/ASK paired prospective quote; no order_send",
        "frozen":leg(frozen,q),"adaptive":leg(adaptive,q),
        "lots":LOT,"tick":q.tick,"profit_tick":q.profit_tick,
        "loss_tick":q.loss_tick,"commission_per_lot":c.risk.commission_per_lot,
        "slippage_ticks":c.risk.slippage_ticks,
        "entry_bid":q.bid,"entry_ask":q.ask,
        "indicative_contract":indicative_contract(t,c,frozen,adaptive,q),
        "limitations":"Missing prices between samples can move exit beyond stop/target; same-quote entry does not model broker fills or latency",
    });
    db.execute(
        "INSERT OR IGNORE INTO frozen_coordinator_trade_pairs
         (run_id,bar_time,opened_ms,closed_ms,state,payload)
         VALUES(?1,?2,?3,NULL,'open',?4)",
        params![id,decision_bar,q.at,payload.to_string()])?;
    Ok(())
}

/** Called after frozen_coordinator::observe. Existing pairs continue to settle
 *  after Stop/mode changes; only freshly persisted same-cycle, completed-M1
 *  policy decisions can open new paper pairs. No MT5 Gateway call is possible.
 */
pub fn observe(db:&Connection,t:&Telemetry,c:&Config,now:u64)->Result<()>{
    let Some(q)=Quote::read(t,now) else{return Ok(())};
    let key=crate::coordinator_learning::context(t);
    let mut pending=db.prepare(
        "SELECT DISTINCT p.run_id FROM frozen_coordinator_trade_pairs p
         JOIN frozen_coordinator_runs r ON r.id=p.run_id
         WHERE r.context=?1 AND p.state='open'")?;
    let ids=pending.query_map([&key],|r|r.get::<_,String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(pending);
    for id in ids {settle(db,&id,q)?}
    if c.trading.mode!=braintrade_domain::trading::TradingMode::Expert
       ||t.symbol!=c.symbol {return Ok(())}
    let trial:Option<(String,u64)>=db.query_row(
        "SELECT id,created_ms FROM frozen_coordinator_runs
         WHERE context=?1 AND mode='expert' AND active=1
         ORDER BY created_ms DESC LIMIT 1",
        [&key],|r|Ok((r.get(0)?,r.get(1)?)))
        .optional()?;
    let Some((id,created))=trial else{return Ok(())};
    // A previous-cycle policy observation must NEVER open a delayed/backfilled
    // paired trade at a later, more favorable quote.
    let pair:Option<(u64,String,String)>=db.query_row(
        "SELECT bar_time,frozen,adaptive FROM frozen_coordinator_decisions
         WHERE run_id=?1 AND observed_ms=?2 ORDER BY bar_time DESC LIMIT 1",
        params![id,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))
        .optional()?;
    let Some((bar,f,a))=pair else{return Ok(())};
    let f:Signal=serde_json::from_str(&f)?;
    let a:Signal=serde_json::from_str(&a)?;
    observe_trial(db,&id,created,true,bar,&f,&a,t,c,now)
}

pub fn summary(db:&Connection,id:&str)->Result<Value>{
    let mut stmt=db.prepare(
        "SELECT state,payload FROM frozen_coordinator_trade_pairs
         WHERE run_id=?1 ORDER BY opened_ms,bar_time")?;
    let rows=stmt.query_map([id],|r|Ok((
        r.get::<_,String>(0)?,r.get::<_,String>(1)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut completed,mut pending,mut won_f,mut won_a,mut better_f,mut ties)=(0usize,0usize,0usize,0usize,0usize,0usize);
    let (mut sum_f,mut sum_a,mut gaps)=(0.,0.,0usize);
    let (mut stop_f,mut stop_a,mut target_f,mut target_a)=(0usize,0usize,0usize,0usize);
    let (mut contract_pass,mut contract_fail,mut contract_unknown)=(0usize,0usize,0usize);
    let (mut pass_f,mut pass_a)=(0.0,0.0);
    // Acceptance-quality prospective evidence requires BOTH an indicative
    // entry-time contract PASS and no sampled quote gap on EITHER exit leg.
    // A PASS remains a heuristic: never call it an MT5 broker approval.
    let (mut clean,mut dirty,mut unknown_gap,mut clean_feasible)=(0usize,0usize,0usize,0usize);
    let (mut feasible_f,mut feasible_a,mut feasible_wins_f,mut feasible_wins_a)=
        (0.,0.,0usize,0usize);
    for(state,raw)in rows{
        if state=="open"{pending+=1;continue}
        if state!="complete"{continue}
        let p:Value=serde_json::from_str(&raw)?;
        let (Some(f),Some(a))=(
            p["frozen"]["result"]["net_per_lot"].as_f64(),
            p["adaptive"]["result"]["net_per_lot"].as_f64()
        )else{continue};
        if !f.is_finite()||!a.is_finite(){continue}
        completed+=1;sum_f+=f;sum_a+=a;
        let contract=p["indicative_contract"]["status"].as_str().unwrap_or("unknown");
        match contract{
            "pass"=>{contract_pass+=1;pass_f+=f;pass_a+=a;},
            "fail"=>contract_fail+=1,
            _=>contract_unknown+=1
        }
        // Unknown/corrupt historical gap metadata must not turn into
        // 'clean' evidence. The stored flag and recorded maximum must agree.
        let fgap=p["frozen"]["result"]["max_quote_gap_ms"].as_u64();
        let agap=p["adaptive"]["result"]["max_quote_gap_ms"].as_u64();
        let fflag=p["frozen"]["result"]["gap_risk"].as_bool();
        let aflag=p["adaptive"]["result"]["gap_risk"].as_bool();
        match (fgap,agap,fflag,aflag){
            (Some(fg),Some(ag),Some(ff),Some(af))
                if ff==(fg>GAP_WARNING_MS) && af==(ag>GAP_WARNING_MS)=>{
                if ff||af{dirty+=1}
                else{
                    clean+=1;
                    if contract=="pass"{
                        clean_feasible+=1;feasible_f+=f;feasible_a+=a;
                        if f>0.{feasible_wins_f+=1}
                        if a>0.{feasible_wins_a+=1}
                    }
                }
            }
            _=>unknown_gap+=1
        }
        if f>0.{won_f+=1};if a>0.{won_a+=1}
        if f>a+1e-8{better_f+=1}else if (f-a).abs()<=1e-8{ties+=1}
        if p["frozen"]["result"]["gap_risk"]==true||p["adaptive"]["result"]["gap_risk"]==true{gaps+=1}
        if p["frozen"]["result"]["reason"]=="stop"{stop_f+=1}
        if p["adaptive"]["result"]["reason"]=="stop"{stop_a+=1}
        if p["frozen"]["result"]["reason"]=="target"{target_f+=1}
        if p["adaptive"]["result"]["reason"]=="target"{target_a+=1}
    }
    Ok(json!({
        "status":if completed<MIN_COMPLETE{"insufficient_complete_pairs"}
            else if clean_feasible<MIN_COMPLETE{"insufficient_clean_feasible_pairs"}
            else{"descriptive_only"},
        "policy":POLICY,"completed_pairs":completed,"open_pairs":pending,
        "frozen_wins_after_costs":won_f,"adaptive_wins_after_costs":won_a,
        "indicative_contract_pass_pairs":contract_pass,
        "indicative_contract_fail_pairs":contract_fail,
        "indicative_contract_unknown_pairs":contract_unknown,
        "quote_clean_complete_pairs":clean,
        "quote_gap_affected_pairs":dirty,
        "unknown_quote_gap_evidence_pairs":unknown_gap,
        "clean_feasible_complete_pairs":clean_feasible,
        "clean_feasible_frozen_wins_after_costs":feasible_wins_f,
        "clean_feasible_adaptive_wins_after_costs":feasible_wins_a,
        "clean_feasible_frozen_mean_net_per_lot":
            if clean_feasible>0{Some(feasible_f/clean_feasible as f64)}else{None},
        "clean_feasible_adaptive_mean_net_per_lot":
            if clean_feasible>0{Some(feasible_a/clean_feasible as f64)}else{None},
        "clean_feasible_frozen_minus_adaptive_mean_net_per_lot":
            if clean_feasible>0{Some((feasible_f-feasible_a)/clean_feasible as f64)}else{None},
        "minimum_clean_feasible_pairs":MIN_COMPLETE,
        "coverage_gate":"both_arms_max_gap_le_15000ms AND consistent_flags AND indicative_contract_pass",
        // Backward-compatible legacy contract-PASS means deliberately still
        // include pairs with quote gaps; it must not be used for qualification.
        "indicative_pass_frozen_mean_net_per_lot":
          if contract_pass>0{Some(pass_f/contract_pass as f64)}else{None},
        "indicative_pass_adaptive_mean_net_per_lot":
          if contract_pass>0{Some(pass_a/contract_pass as f64)}else{None},
        "contract_hints_are_not_broker_approvals":true,
        "frozen_stops":stop_f,"adaptive_stops":stop_a,
        "frozen_targets":target_f,"adaptive_targets":target_a,
        "frozen_outperforms_pairs":better_f,"exact_ties":ties,
        "pairs_with_observation_gaps":gaps,"gap_warning_ms":GAP_WARNING_MS,
        "mean_frozen_net_per_lot":if completed>0{Some(sum_f/completed as f64)}else{None},
        "mean_adaptive_net_per_lot":if completed>0{Some(sum_a/completed as f64)}else{None},
        "mean_difference_frozen_minus_adaptive_per_lot":
            if completed>0{Some((sum_f-sum_a)/completed as f64)}else{None},
        "minimum_descriptive_pairs":MIN_COMPLETE,
        "broker_effect":false,"learning_effect":false,
        "caveat":"Expert coordinator paired quote-sampled PAPER with independent SL/TP/hold, both-ready-only selection and assumed costs. Clean+feasible excludes >15s/unknown quote gaps and entry hints failing or unknown; PASS is NOT order_check or broker approval. Even clean sampled quotes miss intratick SL/TP paths, broker fills, latency, partials, swaps and full system PnL. NOT Sniper, Scalping or Swing."
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn db()->Connection{
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE frozen_coordinator_runs(id TEXT PRIMARY KEY);
            INSERT INTO frozen_coordinator_runs VALUES('r1');").unwrap();
        init(&db).unwrap();db
    }
    fn sig(side:i8,bar:u64,stop:f64,tp:f64,hold:u64)->Signal{
        Signal{brain:"trend".into(),side,key:format!("{side}:{bar}"),bar_time:bar,
            stop_distance:stop,target_distance:tp,hold_ms:hold,
            reason:"synthetic".into(),ready:true,coordination:None}
    }
    fn q(at:u64,bid:f64,ask:f64)->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            symbol:"XAUUSD".into(),bid:Some(bid),ask:Some(ask),
            quote_time_ms:Some(at),tick_age_ms:Some(0),
            tick_size:Some(0.1),tick_value_profit:Some(1.),
            tick_value_loss:Some(1.),..Default::default()}
    }
    #[test]
    fn two_exit_policies_only_score_when_both_legs_finish(){
        let db=db();let mut c=Config::default();c.risk.commission_per_lot=7.;
        let opened=1_000_100_000u64;let bar=1_000_001u64;let created=1_000_000_000u64;
        observe_trial(&db,"r1",created,true,bar,&sig(1,bar,0.5,0.5,100000),
            &sig(1,bar,1.,2.,100000),&q(opened,100.,100.2),&c,opened).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["open_pairs"],1);
        observe_trial(&db,"r1",created,true,bar,&sig(1,bar,0.5,0.5,100000),
            &sig(1,bar,1.,2.,100000),&q(opened+1000,100.7,100.9),&c,opened+1000).unwrap();
        let part=summary(&db,"r1").unwrap();
        assert_eq!(part["completed_pairs"],0);
        assert_eq!(part["open_pairs"],1);
        observe_trial(&db,"r1",created,false,bar,&sig(1,bar,0.5,0.5,100000),
            &sig(1,bar,1.,2.,100000),&q(opened+2000,102.4,102.6),&c,opened+2000).unwrap();
        let total=summary(&db,"r1").unwrap();
        assert_eq!(total["completed_pairs"],1);
        assert_eq!(total["frozen_targets"],1);
        assert_eq!(total["adaptive_targets"],1);
        assert_eq!(total["open_pairs"],0);
        assert!(total["mean_frozen_net_per_lot"].as_f64().unwrap()<0.,
            "The 0.5 USD target is not profitable at 0.01 lots with $0.07 roundtrip fees");
        assert_eq!(total["frozen_wins_after_costs"],0);
        // Exact full round-trip $7/lot for each of the 0.01 lot paper arms.
        let raw:String=db.query_row(
            "SELECT payload FROM frozen_coordinator_trade_pairs",[],|r|r.get(0)).unwrap();
        let p:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(p["frozen"]["result"]["commission_assumed"],0.07);
        assert_eq!(p["adaptive"]["result"]["commission_assumed"],0.07);
    }
    fn synthetic_complete(db:&Connection,bar:u64,contract:&str,
        gap:Option<u64>,frozen_net:f64,adaptive_net:f64){
        let leg=|net:f64|{
            let mut result=json!({"net_per_lot":net,"reason":"target"});
            if let Some(ms)=gap{
                result["max_quote_gap_ms"]=json!(ms);
                result["gap_risk"]=json!(ms>GAP_WARNING_MS);
            }
            json!({"state":"closed","result":result})
        };
        let payload=json!({
            "frozen":leg(frozen_net),"adaptive":leg(adaptive_net),
            "indicative_contract":{"status":contract}
        });
        db.execute(
            "INSERT INTO frozen_coordinator_trade_pairs
             (run_id,bar_time,opened_ms,closed_ms,state,payload)
             VALUES('r1',?1,1000,2000,'complete',?2)",
            params![bar,payload.to_string()]).unwrap();
    }

    #[test]
    fn thirty_paper_winners_during_quote_blackouts_cannot_pass_prospective_gate(){
        let db=db();
        // All historical pairs show unrealistic +100 per lot and a contract
        // indication of PASS. A 25-second sampled quote gap on BOTH sides
        // makes the claimed SL/TP path unknowable.
        for id in 1..=30u64{
            synthetic_complete(&db,id,"pass",Some(25_000),100.,50.);
        }
        let r=summary(&db,"r1").unwrap();
        assert_eq!(r["completed_pairs"],30);
        assert_eq!(r["indicative_contract_pass_pairs"],30);
        assert_eq!(r["quote_gap_affected_pairs"],30);
        assert_eq!(r["quote_clean_complete_pairs"],0);
        assert_eq!(r["clean_feasible_complete_pairs"],0);
        assert_eq!(r["clean_feasible_frozen_mean_net_per_lot"],Value::Null);
        assert_eq!(r["status"],"insufficient_clean_feasible_pairs");
        assert_eq!(r["mean_frozen_net_per_lot"],100.,
            "Legacy all-pairs figures stay visible but cannot qualify a model");
        assert_eq!(r["contract_hints_are_not_broker_approvals"],true);
        assert_eq!(r["broker_effect"],false);

        // A clean quote path, but one failing the indicative minimum lot or
        // margin check, is still not evidence for an executable strategy.
        for id in 31..=60u64{
            synthetic_complete(&db,id,"fail",Some(1000),500.,500.);
        }
        let r=summary(&db,"r1").unwrap();
        assert_eq!(r["quote_clean_complete_pairs"],30);
        assert_eq!(r["indicative_contract_fail_pairs"],30);
        assert_eq!(r["clean_feasible_complete_pairs"],0);
        assert_eq!(r["status"],"insufficient_clean_feasible_pairs");

        // Only the separate clean+feasible descriptive cohort qualifies.
        for id in 61..=90u64{
            synthetic_complete(&db,id,"pass",Some(1000),-2.,-1.);
        }
        let r=summary(&db,"r1").unwrap();
        assert_eq!(r["completed_pairs"],90);
        assert_eq!(r["clean_feasible_complete_pairs"],30);
        assert_eq!(r["clean_feasible_frozen_mean_net_per_lot"],-2.);
        assert_eq!(r["clean_feasible_adaptive_mean_net_per_lot"],-1.);
        assert_eq!(r["clean_feasible_frozen_minus_adaptive_mean_net_per_lot"],-1.);
        assert_eq!(r["clean_feasible_frozen_wins_after_costs"],0);
        assert_eq!(r["status"],"descriptive_only");
    }

    #[test]
    fn incomplete_or_contradictory_coverage_and_unknown_margin_never_qualify(){
        let db=db();
        for id in 1..=30u64 {
            synthetic_complete(&db,id,"pass",None,10.,20.);
        }
        for id in 31..=60u64 {
            synthetic_complete(&db,id,"unknown",Some(500),10.,20.);
        }
        let p=json!({
            "frozen":{"result":{"net_per_lot":10.,"gap_risk":false,
                "max_quote_gap_ms":40_000}},
            "adaptive":{"result":{"net_per_lot":20.,"gap_risk":false,
                "max_quote_gap_ms":40_000}},
            "indicative_contract":{"status":"pass"}
        });
        db.execute(
            "INSERT INTO frozen_coordinator_trade_pairs
             (run_id,bar_time,opened_ms,closed_ms,state,payload)
             VALUES('r1',61,1000,2000,'complete',?1)",
            [p.to_string()]).unwrap();
        let r=summary(&db,"r1").unwrap();
        assert_eq!(r["unknown_quote_gap_evidence_pairs"],31);
        assert_eq!(r["quote_clean_complete_pairs"],30);
        assert_eq!(r["indicative_contract_unknown_pairs"],30);
        assert_eq!(r["clean_feasible_complete_pairs"],0);
        assert_eq!(r["clean_feasible_frozen_mean_net_per_lot"],Value::Null);
        assert_eq!(r["status"],"insufficient_clean_feasible_pairs");
    }

    #[test]
    fn actual_coordinator_paper_observe_marks_blackout_as_unqualified(){
        let db=db();let c=Config::default();
        let opened=1_000_100_000u64;let bar=1_000_001u64;let created=1_000_000_000u64;
        let signal=sig(1,bar,0.5,0.5,60_000);
        observe_trial(&db,"r1",created,true,bar,&signal,&signal,
            &q(opened,100.,100.2),&c,opened).unwrap();
        observe_trial(&db,"r1",created,false,bar,&signal,&signal,
            &q(opened+25_000,101.,101.2),&c,opened+25_000).unwrap();
        let r=summary(&db,"r1").unwrap();
        assert_eq!(r["completed_pairs"],1);
        assert_eq!(r["quote_gap_affected_pairs"],1);
        assert_eq!(r["clean_feasible_complete_pairs"],0);
        assert_eq!(r["clean_feasible_frozen_mean_net_per_lot"],Value::Null);
    }
    #[test]
    fn stop_long_and_take_profit_short_are_independent(){
        let db=db();let c=Config::default();
        let opened=1_000_100_000;let bar=1_000_001;let created=1_000_000_000;
        observe_trial(&db,"r1",created,true,bar,&sig(1,bar,0.5,2.,100000),
            &sig(-1,bar,2.,0.5,100000),&q(opened,100.,100.2),&c,opened).unwrap();
        observe_trial(&db,"r1",created,false,bar,&sig(1,bar,0.5,2.,100000),
            &sig(-1,bar,2.,0.5,100000),&q(opened+1000,99.,99.2),&c,opened+1000).unwrap();
        let total=summary(&db,"r1").unwrap();
        assert_eq!(total["completed_pairs"],1);
        assert_eq!(total["frozen_stops"],1);
        assert_eq!(total["adaptive_targets"],1);
    }
    #[test]
    fn stale_quote_or_invalid_observation_bar_cannot_backfill_pair(){
        let db=db();let c=Config::default();
        let opened=1_000_100_000;let bar=1_000_001;let created=1_000_000_000;
        let mut stale=q(opened,100.,100.2);stale.tick_age_ms=Some(9000);
        observe_trial(&db,"r1",created,true,bar,&sig(1,bar,1.,2.,100000),
            &sig(1,bar,1.,2.,100000),&stale,&c,opened).unwrap();
        observe_trial(&db,"r1",created,true,0,&sig(1,bar,1.,2.,100000),
            &sig(1,bar+60,1.,2.,100000),&q(opened,100.,100.2),&c,opened).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["open_pairs"],0);
        assert_eq!(summary(&db,"r1").unwrap()["completed_pairs"],0);
    }
    #[test]
    fn one_pending_run_survives_restart_and_stop_cannot_open_a_new_pair(){
        let db=db();let c=Config::default();let opened=1_000_100_000;let created=1_000_000_000;
        let bar=1_000_001;
        observe_trial(&db,"r1",created,true,bar,&sig(1,bar,1.,2.,1000),
            &sig(1,bar,1.,2.,1000),&q(opened,100.,100.2),&c,opened).unwrap();
        observe_trial(&db,"r1",created,true,bar,&sig(1,bar+60,1.,2.,1000),
            &sig(1,bar+60,1.,2.,1000),&q(opened+500,100.1,100.3),&c,opened+500).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["open_pairs"],1);
        observe_trial(&db,"r1",created,false,bar,&sig(1,bar,1.,2.,1000),
            &sig(1,bar,1.,2.,1000),&q(opened+2000,100.1,100.3),&c,opened+2000).unwrap();
        let r=summary(&db,"r1").unwrap();
        assert_eq!(r["completed_pairs"],1);
        assert_eq!(r["open_pairs"],0);
        observe_trial(&db,"r1",created,false,bar,&sig(1,bar+60,1.,2.,1000),
            &sig(1,bar+60,1.,2.,1000),&q(opened+3000,101.,101.2),&c,opened+3000).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["completed_pairs"],1);
    }
    #[test]
    fn only_same_cycle_coordinator_decisions_open_and_closed_runs_still_settle(){
        let db=Connection::open_in_memory().unwrap();
        crate::frozen_coordinator::init(&db).unwrap();
        init(&db).unwrap();
        let c=Config::default();
        let opened=1_000_100_000u64;
        let created=1_000_000_000u64;
        let t=q(opened,100.,100.2);
        let key=crate::coordinator_learning::context(&t);
        db.execute(
            "INSERT INTO frozen_coordinator_runs
             (id,context,mode,created_ms,snapshot,active)
             VALUES('coordinator-test',?1,'expert',?2,'{}',1)",
            params![key,created]).unwrap();
        // Coordinator can select M1 and M5 advisors on the SAME observation
        // minute, so unlike an individual advisor trial signal bar times differ.
        let first=sig(1,1_000_001,1.,2.,20_000);
        let second=sig(1,999_960,1.,2.,20_000);
        db.execute(
            "INSERT INTO frozen_coordinator_decisions
             (run_id,bar_time,observed_ms,frozen,adaptive)
             VALUES('coordinator-test',?1,?2,?3,?4)",
            params![1_000_001u64,opened-1000,
                serde_json::to_string(&first).unwrap(),
                serde_json::to_string(&second).unwrap()]).unwrap();
        // A delayed API observation may not invent a new entry.
        observe(&db,&t,&c,opened).unwrap();
        assert_eq!(summary(&db,"coordinator-test").unwrap()["open_pairs"],0);
        db.execute(
            "UPDATE frozen_coordinator_decisions SET observed_ms=?1",
            [opened]).unwrap();
        observe(&db,&t,&c,opened).unwrap();
        assert_eq!(summary(&db,"coordinator-test").unwrap()["open_pairs"],1);
        db.execute(
            "UPDATE frozen_coordinator_runs SET active=0 WHERE id='coordinator-test'",
            []).unwrap();
        let closed=q(opened+2000,103.,103.2);
        observe(&db,&closed,&c,opened+2000).unwrap();
        let r=summary(&db,"coordinator-test").unwrap();
        assert_eq!(r["completed_pairs"],1);
        assert_eq!(r["open_pairs"],0);
        observe(&db,&closed,&c,opened+2000).unwrap();
        assert_eq!(summary(&db,"coordinator-test").unwrap()["completed_pairs"],1);
    }

    #[test]
    fn same_signal_keys_cannot_be_reentered_on_later_observation_bars(){
        let db=db();let c=Config::default();let opened=1_000_100_000u64;
        let created=1_000_000_000u64;let bar=1_000_001u64;
        let same=sig(1,bar,0.5,0.5,3000);
        observe_trial(&db,"r1",created,true,bar,&same,&same,
            &q(opened,100.,100.2),&c,opened).unwrap();
        observe_trial(&db,"r1",created,false,bar,&same,&same,
            &q(opened+1000,101.0,101.2),&c,opened+1000).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["completed_pairs"],1);
        observe_trial(&db,"r1",created,true,bar+60,&same,&same,
            &q(opened+3000,101.1,101.3),&c,opened+3000).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["completed_pairs"],1);
        assert_eq!(summary(&db,"r1").unwrap()["open_pairs"],0);
    }

    #[test]
    fn paper_profit_does_not_mean_broker_lot_is_allowed(){
        let mut t=q(1_000_100_000,100.,100.2);
        t.volume_min=Some(0.1);t.volume_step=Some(0.01);t.volume_max=Some(20.);
        t.minimum_lot_margin=Some(40.);t.margin_free=Some(200.);
        t.trade_allowed=true;t.trade_expert=true;t.equity=100.;
        let c=Config::default();
        let q=Quote::read(&t,1_000_100_000).unwrap();
        let s=sig(1,1_000_001,1.,2.,5000);
        let evidence=indicative_contract(&t,&c,&s,&s,q);
        assert_eq!(evidence["volume"],"fail");
        assert_eq!(evidence["status"],"fail");
        assert_eq!(evidence["broker_effect"],false);
    }
    #[test]
    fn contract_check_is_unknown_without_real_broker_volume_or_margin_data(){
        let mut t=q(1_000_100_000,100.,100.2);
        t.trade_allowed=true;t.trade_expert=true;t.equity=100.;
        let c=Config::default();
        let q=Quote::read(&t,1_000_100_000).unwrap();
        let s=sig(1,1_000_001,1.,2.,5000);
        let evidence=indicative_contract(&t,&c,&s,&s,q);
        assert_eq!(evidence["volume"],"unknown");
        assert_eq!(evidence["margin"],"unknown");
        assert_eq!(evidence["status"],"unknown");
    }
    #[test]
    fn stop_risk_is_evaluated_independently_for_two_exit_policies(){
        let mut t=q(1_000_100_000,100.,100.2);
        t.trade_allowed=true;t.trade_expert=true;t.equity=100.;
        t.volume_min=Some(0.01);t.volume_step=Some(0.01);
        t.volume_max=Some(20.);t.minimum_lot_margin=Some(1.);
        t.margin_free=Some(100.);
        let mut c=Config::default();
        c.risk.risk_percent=2.;
        c.risk.commission_per_lot=7.;
        let q=Quote::read(&t,1_000_100_000).unwrap();
        let small=sig(1,1_000_001,1.,2.,5000);
        let big=sig(1,1_000_001,50.,2.,5000);
        let evidence=indicative_contract(&t,&c,&small,&big,q);
        assert_eq!(evidence["volume"],"pass");
        assert_eq!(evidence["margin"],"pass");
        assert_eq!(evidence["frozen_stop_risk"]["status"],"pass");
        assert_eq!(evidence["adaptive_stop_risk"]["status"],"fail");
        assert_eq!(evidence["status"],"fail");
    }

}