//! Read-only, same-entry-time 5-minute directional markouts for a frozen
//! advisor and current adaptive advisor. Both arms see identical future quotes,
//! lot size, costs, and latency assumptions. This is NOT broker execution or
//! a profit estimate for stop/TP-based MT5 orders.
use anyhow::{Context,Result};
use braintrade_domain::{execution::Signal,Config,Telemetry};
use rusqlite::{params,Connection};
use serde_json::{json,Value};

const POLICY:&str="frozen-matched-five-minute-forward-v1";
const HORIZON:u64=300_000;
const GRACE:u64=30_000;
const MIN_SCORED:usize=30;
#[derive(Clone,Copy)]
struct Quote{bid:f64,ask:f64,at:u64,tick:f64,profit:f64,loss:f64}
impl Quote {
 fn current(t:&Telemetry,now:u64)->Option<Self>{
    if !t.connected||!t.demo||t.account_trade_mode!=Some(0)
        ||t.tick_age_ms.is_none_or(|x|x>3000){return None}
    let (bid,ask)=t.bid.zip(t.ask)?;
    let at=t.quote_time_ms?;
    let tick=t.tick_size?;
    let profit=t.tick_value_profit?;
    let loss=t.tick_value_loss?;
    if [bid,ask,tick,profit,loss].iter().any(|v|!v.is_finite()||*v<=0.)
        ||bid>ask||at>now.saturating_add(2000)||now.saturating_sub(at)>3000{return None}
    Some(Self{bid,ask,at,tick,profit,loss})
 }
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
      "CREATE TABLE IF NOT EXISTS frozen_shadow_markouts(
         run_id TEXT NOT NULL,
         bar_time INTEGER NOT NULL,
         opened_ms INTEGER NOT NULL,
         due_ms INTEGER NOT NULL,
         closed_ms INTEGER,
         state TEXT NOT NULL CHECK(state IN ('pending','scored','expired')),
         payload TEXT NOT NULL,
         PRIMARY KEY(run_id,bar_time),
         FOREIGN KEY(run_id) REFERENCES frozen_shadow_runs(id));
       CREATE INDEX IF NOT EXISTS frozen_shadow_markouts_pending
         ON frozen_shadow_markouts(run_id,state,due_ms);"
    )?;
    Ok(())
}
fn markout(side:i8,entry:f64,exit:f64,lot:f64,tick:f64,profit:f64,loss:f64,
    commission:f64,slippage:f64)->Option<Value>{
    if !matches!(side,-1|1) || [entry,exit,lot,tick,profit,loss,commission,slippage]
        .iter().any(|v|!v.is_finite()||*v<0.)||lot<=0.||tick<=0.||profit<=0.||loss<=0.{
        return None
    }
    let change=(exit-entry)*f64::from(side);
    let effective_ticks=change/tick-2.*slippage;
    let tick_value=if effective_ticks>=0.{profit}else{loss};
    let gross=effective_ticks*tick_value*lot;
    let cost=commission*lot;
    let net=gross-cost;
    if ![gross,cost,net].iter().all(|v|v.is_finite()){return None}
    Some(json!({"side":side,"entry":entry,"exit":exit,"movement_ticks":change/tick,
        "gross":gross,"commission_assumed":cost,
        "roundtrip_slippage_ticks":2.*slippage,"net_paper":net,
        "net_per_lot":net/lot}))
}
fn settle(db:&Connection,id:&str,q:Quote)->Result<()>{
    let mut query=db.prepare(
        "SELECT bar_time,due_ms,payload FROM frozen_shadow_markouts
         WHERE run_id=?1 AND state='pending' ORDER BY due_ms,bar_time")?;
    let all=query.query_map([id],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(query);
    for(bar,due,raw) in all {
        if q.at<due {continue}
        let mut state:Value=serde_json::from_str(&raw)?;
        if q.at>due.saturating_add(GRACE) {
            state["expiry"]=json!("No fresh quote inside 30s of five-minute maturity; no return imputed");
            db.execute(
                "UPDATE frozen_shadow_markouts SET state='expired',closed_ms=?3,payload=?4
                 WHERE run_id=?1 AND bar_time=?2 AND state='pending'",
                params![id,bar,q.at,state.to_string()])?;
            continue;
        }
        let lot=state["lots"].as_f64().context("matched lot")?;
        let tick=state["tick"].as_f64().context("matched tick")?;
        let profit=state["profit_tick"].as_f64().context("matched profit tick")?;
        let loss=state["loss_tick"].as_f64().context("matched loss tick")?;
        let commission=state["commission_per_lot"].as_f64().context("matched commission")?;
        let slip=state["slippage_ticks"].as_f64().context("matched slippage")?;
        let frozen_side=state["frozen"]["side"].as_i64().context("matched frozen direction")? as i8;
        let adaptive_side=state["adaptive"]["side"].as_i64().context("matched adaptive direction")? as i8;
        let frozen_entry=state["frozen"]["entry"].as_f64().context("matched frozen entry")?;
        let adaptive_entry=state["adaptive"]["entry"].as_f64().context("matched adaptive entry")?;
        let frozen_exit=if frozen_side==1{q.bid}else{q.ask};
        let adaptive_exit=if adaptive_side==1{q.bid}else{q.ask};
        let (Some(f),Some(a))=(
            markout(frozen_side,frozen_entry,frozen_exit,lot,tick,profit,loss,commission,slip),
            markout(adaptive_side,adaptive_entry,adaptive_exit,lot,tick,profit,loss,commission,slip)
        )else{anyhow::bail!("Invalid stored matched-forward evidence")};
        let delta=f["net_per_lot"].as_f64().unwrap()-a["net_per_lot"].as_f64().unwrap();
        state["result"]=json!({"frozen":f,"adaptive":a,
            "delta_frozen_minus_adaptive_per_lot":delta,
            "exit_bid":q.bid,"exit_ask":q.ask,"closed_ms":q.at});
        db.execute(
            "UPDATE frozen_shadow_markouts SET state='scored',closed_ms=?3,payload=?4
             WHERE run_id=?1 AND bar_time=?2 AND state='pending'",
            params![id,bar,q.at,state.to_string()])?;
    }
    Ok(())
}
pub fn observe(db:&Connection,id:&str,created:u64,active:bool,
    frozen:&Signal,adaptive:&Signal,t:&Telemetry,c:&Config,now:u64)->Result<()>{
    let Some(q)=Quote::current(t,now) else{return Ok(())};
    settle(db,id,q)?;
    if !active||!frozen.ready||!adaptive.ready||frozen.bar_time==0
        ||frozen.bar_time!=adaptive.bar_time||!matches!(frozen.side,-1|1)
        ||!matches!(adaptive.side,-1|1)
        ||frozen.bar_time.checked_mul(1000).is_none_or(|ts|ts<=created||ts>now)
        ||!c.risk.commission_per_lot.is_finite()||c.risk.commission_per_lot<0.
        ||!c.risk.slippage_ticks.is_finite()||c.risk.slippage_ticks<0.
        {return Ok(())}
    // No overlapping comparisons: one fresh entry decision per trial,
    // with a single future horizon and equal costs in both arms.
    let pending:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_shadow_markouts WHERE run_id=?1 AND state='pending')",
        [id],|r|r.get(0))?;
    if pending{return Ok(())}
    let entry=|side:i8|if side==1{q.ask}else{q.bid};
    let payload=json!({
        "policy":POLICY,"source":"MT5 Demo fresh BID/ASK quote, NO broker order",
        "bar_time":frozen.bar_time,"opened_ms":q.at,
        "entry_bid":q.bid,"entry_ask":q.ask,
        "frozen":{"side":frozen.side,"entry":entry(frozen.side),"key":frozen.key},
        "adaptive":{"side":adaptive.side,"entry":entry(adaptive.side),"key":adaptive.key},
        "lots":0.01,"tick":q.tick,"profit_tick":q.profit,"loss_tick":q.loss,
        "commission_per_lot":c.risk.commission_per_lot,
        "slippage_ticks":c.risk.slippage_ticks,
        "evaluation":"Five-minute FIXED horizon, no interim stop/TP; NOT the original strategy's PnL",
    });
    let due=q.at.saturating_add(HORIZON);
    db.execute(
        "INSERT OR IGNORE INTO frozen_shadow_markouts
         (run_id,bar_time,opened_ms,due_ms,closed_ms,state,payload)
         VALUES(?1,?2,?3,?4,NULL,'pending',?5)",
         params![id,frozen.bar_time,q.at,due,payload.to_string()])?;
    Ok(())
}
pub fn summary(db:&Connection,id:&str)->Result<Value>{
    let mut stmt=db.prepare(
        "SELECT state,payload FROM frozen_shadow_markouts
         WHERE run_id=?1 ORDER BY bar_time")?;
    let raw=stmt.query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut scored,mut expired,mut pending)=(0usize,0usize,0usize);
    let (mut fsum,mut asum,mut better,mut equal)=(0f64,0f64,0usize,0usize);
    for(state,raw)in raw{
        match state.as_str(){
            "pending"=>pending+=1,
            "expired"=>expired+=1,
            "scored"=>{
                let p:Value=serde_json::from_str(&raw)?;
                let Some(f)=p["result"]["frozen"]["net_per_lot"].as_f64() else{continue};
                let Some(a)=p["result"]["adaptive"]["net_per_lot"].as_f64() else{continue};
                if !f.is_finite()||!a.is_finite(){continue}
                scored+=1;fsum+=f;asum+=a;
                if f>a+1e-8{better+=1}else if (f-a).abs()<=1e-8{equal+=1}
            }
            _=>{}
        }
    }
    Ok(json!({
        "policy":POLICY,"source":"paired forward Demo BID/ASK quotes",
        "matched_five_minute_samples":scored,"pending":pending,"expired_no_quote":expired,
        "frozen_mean_net_per_lot":if scored>0{Some(fsum/scored as f64)}else{None},
        "adaptive_mean_net_per_lot":if scored>0{Some(asum/scored as f64)}else{None},
        "mean_delta_frozen_minus_adaptive_per_lot":if scored>0{Some((fsum-asum)/scored as f64)}else{None},
        "frozen_outperforms_pairs":better,"exact_ties":equal,
        "minimum_descriptive_samples":MIN_SCORED,
        "status":if scored<MIN_SCORED{"insufficient_matched_evidence"}else{"descriptive_only"},
        "no_broker_orders":true,"no_learning_effect":true,
        "limitations":"Five-minute sampled directional markout, NOT actual stops, targets or MT5 fills. Both-ready-only selection and overlapping market regimes bias any performance inference. Costs/slippage assumed identically.",
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::trading::TradingMode;
    fn db()->Connection{
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE frozen_shadow_runs(id TEXT PRIMARY KEY);").unwrap();
        db.execute("INSERT INTO frozen_shadow_runs VALUES('r1')",[]).unwrap();
        init(&db).unwrap();db
    }
    fn signal(side:i8,bar:u64)->Signal{
        Signal{brain:"trend".into(),side,key:format!("side{side}-bar{bar}"),
            bar_time:bar,stop_distance:1.,target_distance:2.,hold_ms:300000,
            reason:"test".into(),ready:true,coordination:None}
    }
    fn quote(at:u64,bid:f64,ask:f64)->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            symbol:"XAUUSD".into(),bid:Some(bid),ask:Some(ask),
            quote_time_ms:Some(at),tick_age_ms:Some(0),
            tick_size:Some(0.1),tick_value_profit:Some(1.),
            tick_value_loss:Some(1.),..Default::default()}
    }
    #[test]
    fn identical_future_quotes_fees_and_signal_directions_yield_exact_tie(){
        let db=db();
        let c=Config::default();
        let at=1_000_100_000u64;let bar=1_000_001u64;
        observe(&db,"r1",1_000_000_000,true,&signal(1,bar),
            &signal(1,bar),&quote(at,100.,100.2),&c,at).unwrap();
        let before=summary(&db,"r1").unwrap();
        assert_eq!(before["pending"],1);
        assert_eq!(before["matched_five_minute_samples"],0);
        observe(&db,"r1",1_000_000_000,true,&signal(1,bar),
            &signal(1,bar),&quote(at+HORIZON,100.5,100.7),&c,at+HORIZON).unwrap();
        let finished=summary(&db,"r1").unwrap();
        assert_eq!(finished["matched_five_minute_samples"],1);
        assert_eq!(finished["pending"],0);
        assert_eq!(finished["mean_delta_frozen_minus_adaptive_per_lot"],0.);
        assert_eq!(finished["exact_ties"],1);
        assert_eq!(finished["status"],"insufficient_matched_evidence");
        observe(&db,"r1",1_000_000_000,true,&signal(1,bar),
            &signal(1,bar),&quote(at+HORIZON+1000,100.5,100.7),&c,at+HORIZON+1000).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["matched_five_minute_samples"],1);
    }
    #[test]
    fn opposite_signals_have_equal_cost_assumptions_and_detect_direction(){
        let db=db();let mut c=Config::default();c.risk.commission_per_lot=7.;
        let at=1_000_100_000;let bar=1_000_001;
        observe(&db,"r1",1_000_000_000,true,&signal(1,bar),
            &signal(-1,bar),&quote(at,100.,100.2),&c,at).unwrap();
        observe(&db,"r1",1_000_000_000,false,&signal(1,bar),
            &signal(-1,bar),&quote(at+HORIZON,101.,101.2),&c,at+HORIZON).unwrap();
        let s=summary(&db,"r1").unwrap();
        assert_eq!(s["matched_five_minute_samples"],1);
        assert!(s["mean_delta_frozen_minus_adaptive_per_lot"].as_f64().unwrap()>0.);
        let raw:String=db.query_row(
            "SELECT payload FROM frozen_shadow_markouts WHERE run_id='r1'",
            [],|r|r.get(0)).unwrap();
        let p:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(p["result"]["frozen"]["commission_assumed"],0.07);
        assert_eq!(p["result"]["adaptive"]["commission_assumed"],0.07);
    }
    #[test]
    fn misses_quote_window_does_not_impute_a_winning_return(){
        let db=db();let c=Config::default();
        let at=1_000_100_000;let bar=1_000_001;
        observe(&db,"r1",1_000_000_000,true,&signal(1,bar),
            &signal(-1,bar),&quote(at,100.,100.2),&c,at).unwrap();
        observe(&db,"r1",1_000_000_000,false,&signal(1,bar),
            &signal(-1,bar),&quote(at+HORIZON+GRACE+5000,999.,999.2),&c,
                at+HORIZON+GRACE+5000).unwrap();
        let s=summary(&db,"r1").unwrap();
        assert_eq!(s["expired_no_quote"],1);
        assert_eq!(s["matched_five_minute_samples"],0);
        assert_eq!(s["frozen_mean_net_per_lot"],Value::Null);
    }
    #[test]
    fn mismatched_bar_or_stale_broker_quote_is_never_matched(){
        let db=db();let c=Config::default();let at=1_000_100_000;let bar=1_000_001;
        observe(&db,"r1",1_000_000_000,true,&signal(1,bar),
            &signal(1,bar+60),&quote(at,100.,100.2),&c,at).unwrap();
        let mut stale=quote(at,100.,100.2);stale.tick_age_ms=Some(5000);
        observe(&db,"r1",1_000_000_000,true,&signal(1,bar),
            &signal(1,bar),&stale,at).unwrap();
        assert_eq!(summary(&db,"r1").unwrap()["pending"],0);
    }
}