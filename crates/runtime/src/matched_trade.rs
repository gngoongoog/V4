//! Paired prospective full-exit-rule Shadow paper study.
//! Both strategies enter on exactly the same observed MT5 Demo quote (0.01 lots
//! and same assumed costs). Each then independently exits on its own frozen
//! entry-time stop/target/holding rule. No broker commands or learner updates.
use anyhow::{Context,Result};
use braintrade_domain::{execution::Signal,Config,Telemetry};
use rusqlite::{params,Connection};
use serde_json::{json,Value};

const POLICY:&str="frozen-versus-adaptive-full-exit-paper-v1";
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
        "CREATE TABLE IF NOT EXISTS frozen_shadow_trade_pairs(
            run_id TEXT NOT NULL,
            bar_time INTEGER NOT NULL,
            opened_ms INTEGER NOT NULL,
            closed_ms INTEGER,
            state TEXT NOT NULL CHECK(state IN ('open','complete')),
            payload TEXT NOT NULL,
            PRIMARY KEY(run_id,bar_time),
            FOREIGN KEY(run_id) REFERENCES frozen_shadow_runs(id));
         CREATE INDEX IF NOT EXISTS frozen_shadow_trade_pairs_open_idx
            ON frozen_shadow_trade_pairs(run_id,state,opened_ms);"
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
        "SELECT bar_time,opened_ms,payload FROM frozen_shadow_trade_pairs
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
            "UPDATE frozen_shadow_trade_pairs
             SET state=?3,closed_ms=?4,payload=?5
             WHERE run_id=?1 AND bar_time=?2 AND state='open'",
            params![id,bar,if done{"complete"}else{"open"},
                if done{Some(q.at)}else{None},p.to_string()])?;
    }
    Ok(())
}
pub fn observe(db:&Connection,id:&str,created:u64,active:bool,
    frozen:&Signal,adaptive:&Signal,t:&Telemetry,c:&Config,now:u64)->Result<()>{
    let Some(q)=Quote::read(t,now)else{return Ok(())};
    settle(db,id,q)?;
    if !active || !valid_signal(frozen)||!valid_signal(adaptive)
        ||frozen.bar_time==0||frozen.bar_time!=adaptive.bar_time
        ||frozen.bar_time.checked_mul(1000).is_none_or(|ts|ts<=created||ts>now)
        ||!c.risk.commission_per_lot.is_finite()||c.risk.commission_per_lot<0.
        ||!c.risk.slippage_ticks.is_finite()||c.risk.slippage_ticks<0. {
        return Ok(())
    }
    // This is a strict shared opportunity cohort: no overlapping pairs per
    // frozen policy. Do not count another signal until BOTH exits are closed.
    let pending:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_shadow_trade_pairs
         WHERE run_id=?1 AND state='open')", [id],|r|r.get(0))?;
    if pending {return Ok(())}
    let payload=json!({
        "policy":POLICY,"observed_ms":q.at,"bar_time":frozen.bar_time,
        "source":"MT5 Demo BID/ASK paired prospective quote; no order_send",
        "frozen":leg(frozen,q),"adaptive":leg(adaptive,q),
        "lots":LOT,"tick":q.tick,"profit_tick":q.profit_tick,
        "loss_tick":q.loss_tick,"commission_per_lot":c.risk.commission_per_lot,
        "slippage_ticks":c.risk.slippage_ticks,
        "entry_bid":q.bid,"entry_ask":q.ask,
        "limitations":"Missing prices between samples can move exit beyond stop/target; same-quote entry does not model broker fills or latency",
    });
    db.execute(
        "INSERT OR IGNORE INTO frozen_shadow_trade_pairs
         (run_id,bar_time,opened_ms,closed_ms,state,payload)
         VALUES(?1,?2,?3,NULL,'open',?4)",
        params![id,frozen.bar_time,q.at,payload.to_string()])?;
    Ok(())
}
pub fn summary(db:&Connection,id:&str)->Result<Value>{
    let mut stmt=db.prepare(
        "SELECT state,payload FROM frozen_shadow_trade_pairs
         WHERE run_id=?1 ORDER BY opened_ms,bar_time")?;
    let rows=stmt.query_map([id],|r|Ok((
        r.get::<_,String>(0)?,r.get::<_,String>(1)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut completed,mut pending,mut won_f,mut won_a,mut better_f,mut ties)=(0usize,0usize,0usize,0usize,0usize,0usize);
    let (mut sum_f,mut sum_a,mut gaps)=(0.,0.,0usize);
    let (mut stop_f,mut stop_a,mut target_f,mut target_a)=(0usize,0usize,0usize,0usize);
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
        if f>0.{won_f+=1};if a>0.{won_a+=1}
        if f>a+1e-8{better_f+=1}else if (f-a).abs()<=1e-8{ties+=1}
        if p["frozen"]["result"]["gap_risk"]==true||p["adaptive"]["result"]["gap_risk"]==true{gaps+=1}
        if p["frozen"]["result"]["reason"]=="stop"{stop_f+=1}
        if p["adaptive"]["result"]["reason"]=="stop"{stop_a+=1}
        if p["frozen"]["result"]["reason"]=="target"{target_f+=1}
        if p["adaptive"]["result"]["reason"]=="target"{target_a+=1}
    }
    Ok(json!({
        "status":if completed<MIN_COMPLETE{"insufficient_complete_pairs"}else{"descriptive_only"},
        "policy":POLICY,"completed_pairs":completed,"open_pairs":pending,
        "frozen_wins_after_costs":won_f,"adaptive_wins_after_costs":won_a,
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
        "caveat":"Paired paper quotes, individually applied SL/TP/hold and same simulated costs; unobserved intratick events, fills, partials, swaps, bid/ask changes can make real outcomes different; not the global system/coordinator."
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn db()->Connection{
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE frozen_shadow_runs(id TEXT PRIMARY KEY);
            INSERT INTO frozen_shadow_runs VALUES('r1');").unwrap();
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
        observe(&db,"r1",created,true,&sig(1,bar,0.5,0.5,100000),
            &sig(1,bar,1.,2.,100000),&q(opened,100.,100.2),&c,opened).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["open_pairs"],1);
        observe(&db,"r1",created,true,&sig(1,bar,0.5,0.5,100000),
            &sig(1,bar,1.,2.,100000),&q(opened+1000,100.7,100.9),&c,opened+1000).unwrap();
        let part=summary(&db,"r1").unwrap();
        assert_eq!(part["completed_pairs"],0);
        assert_eq!(part["open_pairs"],1);
        observe(&db,"r1",created,false,&sig(1,bar,0.5,0.5,100000),
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
            "SELECT payload FROM frozen_shadow_trade_pairs",[],|r|r.get(0)).unwrap();
        let p:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(p["frozen"]["result"]["commission_assumed"],0.07);
        assert_eq!(p["adaptive"]["result"]["commission_assumed"],0.07);
    }
    #[test]
    fn stop_long_and_take_profit_short_are_independent(){
        let db=db();let c=Config::default();
        let opened=1_000_100_000;let bar=1_000_001;let created=1_000_000_000;
        observe(&db,"r1",created,true,&sig(1,bar,0.5,2.,100000),
            &sig(-1,bar,2.,0.5,100000),&q(opened,100.,100.2),&c,opened).unwrap();
        observe(&db,"r1",created,false,&sig(1,bar,0.5,2.,100000),
            &sig(-1,bar,2.,0.5,100000),&q(opened+1000,99.,99.2),&c,opened+1000).unwrap();
        let total=summary(&db,"r1").unwrap();
        assert_eq!(total["completed_pairs"],1);
        assert_eq!(total["frozen_stops"],1);
        assert_eq!(total["adaptive_targets"],1);
    }
    #[test]
    fn stale_quote_or_different_signal_bars_cannot_backfill_pair(){
        let db=db();let c=Config::default();
        let opened=1_000_100_000;let bar=1_000_001;let created=1_000_000_000;
        let mut stale=q(opened,100.,100.2);stale.tick_age_ms=Some(9000);
        observe(&db,"r1",created,true,&sig(1,bar,1.,2.,100000),
            &sig(1,bar,1.,2.,100000),&stale,&c,opened).unwrap();
        observe(&db,"r1",created,true,&sig(1,bar,1.,2.,100000),
            &sig(1,bar+60,1.,2.,100000),&q(opened,100.,100.2),&c,opened).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["open_pairs"],0);
        assert_eq!(summary(&db,"r1").unwrap()["completed_pairs"],0);
    }
    #[test]
    fn one_pending_run_survives_restart_and_stop_cannot_open_a_new_pair(){
        let db=db();let c=Config::default();let opened=1_000_100_000;let created=1_000_000_000;
        let bar=1_000_001;
        observe(&db,"r1",created,true,&sig(1,bar,1.,2.,1000),
            &sig(1,bar,1.,2.,1000),&q(opened,100.,100.2),&c,opened).unwrap();
        observe(&db,"r1",created,true,&sig(1,bar+60,1.,2.,1000),
            &sig(1,bar+60,1.,2.,1000),&q(opened+500,100.1,100.3),&c,opened+500).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["open_pairs"],1);
        observe(&db,"r1",created,false,&sig(1,bar,1.,2.,1000),
            &sig(1,bar,1.,2.,1000),&q(opened+2000,100.1,100.3),&c,opened+2000).unwrap();
        let r=summary(&db,"r1").unwrap();
        assert_eq!(r["completed_pairs"],1);
        assert_eq!(r["open_pairs"],0);
        observe(&db,"r1",created,false,&sig(1,bar+60,1.,2.,1000),
            &sig(1,bar+60,1.,2.,1000),&q(opened+3000,101.,101.2),&c,opened+3000).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["completed_pairs"],1);
    }
}