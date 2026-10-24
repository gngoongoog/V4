//! P1-K3: forward-only, in-memory frozen Sniper Learner comparison.
//! Observes the *same accepted Core pulses*; NEVER submits broker orders,
//! learns from future closed deals, builds synthetic PnL, or backfills ticks.
//! Classic Sniper is explicitly unsupported by this Learner-only snapshot.
use anyhow::{ensure,Result};
use braintrade_domain::{
    execution::Signal,sniper::Engine,trading::{SniperBrain,TradingMode},
    Config,Telemetry,
};
use serde_json::{json,Value};

const GAP_MS:u64=1500;
const WARMUP_MS:u64=2100;

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap_or_default()
}
fn eligible(t:&Telemetry,c:&Config,now:u64)->bool{
    t.connected&&t.demo&&t.account_trade_mode==Some(0)&&t.login>0
    &&!t.server.is_empty()&&!t.currency.is_empty()
    &&t.symbol==c.symbol&&c.trading.mode==TradingMode::Sniper
    &&c.trading.sniper_brain==SniperBrain::Learner
    &&t.source_quote_time_ms.is_some()
    &&t.quote_is_fresh(now,1500)
}
struct Trial{
    context:String,started_ms:u64,config:Config,
    frozen:Engine,active:bool,reason:String,
    first_future_ms:Option<u64>,last_event:Option<u64>,
    last_broker:Option<u64>,last_origin:String,
    last_dropped:Option<u64>,
    accepted: u64,comparable:u64,
    exact:u64,side_disagreements:u64,readiness_disagreements:u64,
    target_disagreements:u64,both_waiting:u64,
    excluded_legacy:u64,excluded_switch:u64,excluded_gap:u64,
    excluded_overflow:u64,excluded_warmup:u64,
}
#[derive(Default)]
pub struct Study{trial:Option<Trial>}
impl Study{
    pub fn clear_on_mode_change(&mut self){self.trial=None;}
    pub fn freeze(&mut self,t:&Telemetry,c:&Config,adaptive:&Engine,now:u64)->Result<Value>{
        ensure!(eligible(t,c,now),
            "Sniper Learner study needs a fresh bound MT5 Demo quote with original broker timestamp");
        ensure!(self.trial.as_ref().is_none_or(|x|!x.active),
            "Frozen Sniper Tick study is already active; stop it first");
        c.validate().map_err(anyhow::Error::msg)?;
        let frozen=adaptive.frozen_study_copy();
        self.trial=Some(Trial{
            context:context(t),started_ms:now,config:c.clone(),frozen,
            active:true,reason:String::new(),
            first_future_ms:None,last_event:None,last_broker:None,
            last_origin:String::new(),last_dropped:None,
            accepted:0,comparable:0,exact:0,side_disagreements:0,
            readiness_disagreements:0,target_disagreements:0,
            both_waiting:0,excluded_legacy:0,excluded_switch:0,
            excluded_gap:0,excluded_overflow:0,excluded_warmup:0,
        });
        Ok(json!({"ok":true,"status":"active","context":context(t),
            "frozen_ms":now,"policy":"p1k3-sniper-learner-forward-tick-v1",
            "model_scores_frozen":true,"prior_tick_window_discarded":true,
            "no_broker_orders":true,"no_learning_effect":true,
            "persistence":"memory_only"}))
    }
    pub fn stop(&mut self,t:&Telemetry)->Result<Value>{
        let trial=self.trial.as_mut().filter(|v|v.context==context(t))
            .ok_or_else(||anyhow::anyhow!("No matching Sniper study"))?;
        trial.active=false;
        trial.reason="operator_stopped".into();
        Ok(json!({"ok":true,"status":"stopped","no_broker_orders":true}))
    }
    pub fn observe(&mut self,t:&Telemetry,c:&Config,adaptive:Option<&Signal>,
        payload:&[u8],now:u64){
        let Some(trial)=self.trial.as_mut().filter(|v|v.active)else{return};
        if context(t)!=trial.context||!eligible(t,c,now) {
            // Never resume an outdated/foreign account trial.
            trial.active=false;trial.reason="broker_identity_or_mode_changed".into();return
        }
        if serde_json::to_value(c).ok()!=serde_json::to_value(&trial.config).ok(){
            trial.active=false;trial.reason="policy_config_changed_after_freeze".into();return
        }
        let meta:Value=serde_json::from_slice(payload).unwrap_or(Value::Null);
        let src=meta["quote_source"].as_str().unwrap_or("");
        let drop=meta["native_dropped_total"].as_u64();
        let (Some(event),Some(broker))=(t.quote_time_ms,t.source_quote_time_ms)
        else {trial.excluded_legacy+=1;return};
        if broker<=trial.started_ms || now<=trial.started_ms
           ||trial.last_event.is_some_and(|p|event<=p)
           ||trial.last_broker.is_some_and(|p|broker<p) {
            trial.excluded_legacy+=1;return
        }
        let gap=trial.last_broker.map(|p|broker-p).unwrap_or(0);
        let changed=!trial.last_origin.is_empty()&&src!=trial.last_origin;
        let overflow=trial.last_dropped.zip(drop)
            .is_some_and(|(p,n)|n!=p);
        // Any source jump/gap/mailbox loss makes the signal trajectory
        // ambiguous: restart ONLY the frozen study's tick window, and wait
        // for a fresh warmup. The executing engine is never modified.
        let bad_source=!matches!(src,"native"|"poll")||drop.is_none();
        if bad_source||changed||gap>GAP_MS||overflow {
            trial.frozen.reset_quotes();
            trial.first_future_ms=None;
            if bad_source{trial.excluded_legacy+=1}
            if changed{trial.excluded_switch+=1}
            if gap>GAP_MS{trial.excluded_gap+=1}
            if overflow{trial.excluded_overflow+=1}
        }
        trial.last_event=Some(event);trial.last_broker=Some(broker);
        trial.last_origin=src.to_owned();trial.last_dropped=drop;
        trial.accepted+=1;
        if bad_source||changed||gap>GAP_MS||overflow {return}
        let frozen=trial.frozen.evaluate(t,&trial.config,now);
        let start=*trial.first_future_ms.get_or_insert(broker);
        // The executing learner can retain pre-freeze quotes for <=2 seconds.
        // The first 2.1 seconds of prospective quotes are therefore warmup
        // and explicitly excluded from comparison (no historical backfill).
        if broker.saturating_sub(start)<WARMUP_MS {
            trial.excluded_warmup+=1;return
        }
        let Some(adaptive)=adaptive.filter(|x|x.key==frozen.key)
        else {trial.excluded_legacy+=1;return};
        trial.comparable+=1;
        if !adaptive.ready&&!frozen.ready{trial.both_waiting+=1}
        else if adaptive.ready!=frozen.ready{trial.readiness_disagreements+=1}
        else if adaptive.side!=frozen.side{trial.side_disagreements+=1}
        else if (adaptive.target_distance-frozen.target_distance).abs()>1e-8
            ||(adaptive.stop_distance-frozen.stop_distance).abs()>1e-8 {
            trial.target_disagreements+=1
        }else{trial.exact+=1}
    }
    pub fn report(&self,t:Option<&Telemetry>,now:u64)->Value {
        let Some(trial)=self.trial.as_ref()else{return json!({
            "schema":"p1k3-sniper-frozen-tick-v1","status":"not_started",
            "no_broker_orders":true,"independent_oos":false})};
        let Some(t)=t.filter(|t|t.connected&&t.demo
            &&t.account_trade_mode==Some(0)&&context(t)==trial.context)
        else{return json!({"schema":"p1k3-sniper-frozen-tick-v1",
            "status":"unavailable","no_broker_orders":true})};
        json!({
            "schema":"p1k3-sniper-frozen-tick-v1",
            "status":if trial.active{"active"}else{"stopped"},
            "reason":trial.reason,
            "context":trial.context,
            "mode":"sniper","sniper_brain":"learner",
            "account":{"login":t.login,"server":t.server,"symbol":t.symbol,"currency":t.currency},
            "frozen_ms":trial.started_ms,"observed_ms":now,
            "accepted_prospective_pulses":trial.accepted,
            "comparable_future_tick_decisions":trial.comparable,
            "exact_decisions":trial.exact,
            "same_direction_both_waiting":trial.both_waiting,
            "readiness_disagreements":trial.readiness_disagreements,
            "side_disagreements":trial.side_disagreements,
            "target_or_stop_disagreements":trial.target_disagreements,
            "excluded_unknown_or_legacy":trial.excluded_legacy,
            "excluded_source_switch":trial.excluded_switch,
            "excluded_broker_clock_gap":trial.excluded_gap,
            "excluded_native_queue_counter_change":trial.excluded_overflow,
            "excluded_early_warmup":trial.excluded_warmup,
            "minimum_warmup_ms":WARMUP_MS,
            "forward_only":true,
            "model_scores_frozen":true,"prior_tick_window_discarded":true,
            "persistence":"memory_only","read_only":true,
            "no_broker_orders":true,"no_learning_effect":true,
            "independent_oos":false,"paper_pnl":Value::Null,"broker_pnl":Value::Null,
            "strategy_win_rate":Value::Null,
            "note":"Sniper Learner raw signal vs immutable frozen policy on accepted future Native/Poll quotes; not Classic Sniper, no market-tick completeness, no broker gates/fills or OOS profitability certification."
        })
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn fixture(now:u64)->(Telemetry,Config,Engine){
        let mut c=Config::default();
        c.trading.mode=TradingMode::Sniper;
        c.trading.sniper_brain=SniperBrain::Learner;
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:17,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),bid:Some(2500.),ask:Some(2500.2),
            quote_time_ms:Some(now),source_quote_time_ms:Some(now),
            tick_age_ms:Some(0),point:Some(0.01),tick_size:Some(0.01),
            tick_value_profit:Some(1.),tick_value_loss:Some(1.),
            ..Default::default()};
        (t,c,Engine::default())
    }
    fn meta(src:&str,drops:u64)->Vec<u8>{
        json!({"quote_source":src,"native_dropped_total":drops}).to_string().into_bytes()
    }
    #[test]
    fn account_real_classic_and_policy_drift_never_create_forward_profit(){
        let (mut t,mut c,e)=fixture(100_000);
        let mut s=Study::default();
        t.demo=false;assert!(s.freeze(&t,&c,&e,100_000).is_err());
        t.demo=true;c.trading.sniper_brain=SniperBrain::Classic;
        assert!(s.freeze(&t,&c,&e,100_000).is_err());
        c.trading.sniper_brain=SniperBrain::Learner;
        s.freeze(&t,&c,&e,100_000).unwrap();
        c.trading.sniper_min_target_pips+=1.;
        t.source_quote_time_ms=Some(101_000);t.quote_time_ms=Some(101_000);
        s.observe(&t,&c,None,&meta("native",0),101_001);
        assert_eq!(s.report(Some(&t),101_001)["status"],"stopped");
        assert_eq!(s.report(Some(&t),101_001)["reason"],"policy_config_changed_after_freeze");
        assert!(s.report(Some(&t),101_001)["broker_pnl"].is_null());
    }
    #[test]
    fn future_tick_warmup_and_source_gap_do_not_fabricate_advantage(){
        let (mut t,c,e)=fixture(100_000);
        let mut s=Study::default();
        s.freeze(&t,&c,&e,100_000).unwrap();
        for (source,broker,event,drops) in [
            ("native",100_100,100_100,0),
            ("native",101_000,101_000,0),
            ("poll",101_500,101_500,0),
            ("poll",103_900,103_900,0),
            ("poll",104_000,104_000,1),
            ("poll",104_500,104_500,1),
        ]{
            t.source_quote_time_ms=Some(broker);
            t.quote_time_ms=Some(event);
            s.observe(&t,&c,None,&meta(source,drops),broker+1);
        }
        let r=s.report(Some(&t),105_000);
        assert_eq!(r["accepted_prospective_pulses"],6);
        assert_eq!(r["excluded_source_switch"],1);
        assert_eq!(r["excluded_broker_clock_gap"],1);
        assert_eq!(r["excluded_native_queue_counter_change"],1);
        assert_eq!(r["comparable_future_tick_decisions"],0);
        assert!(r["paper_pnl"].is_null());
        assert_eq!(r["independent_oos"],false);
        assert_eq!(r["no_broker_orders"],true);
    }
    #[test]
    fn frozen_study_does_not_inherit_old_account_or_live_quotes(){
        let (mut t,c,e)=fixture(100_000);
        let mut s=Study::default();s.freeze(&t,&c,&e,100_000).unwrap();
        let mut other=t.clone();other.login=99;
        assert_eq!(s.report(Some(&other),100_500)["status"],"unavailable");
        assert!(s.stop(&other).is_err());
        assert_eq!(s.report(Some(&t),100_500)["status"],"active");
        t.source_quote_time_ms=Some(100_900);t.quote_time_ms=Some(100_900);
        s.observe(&t,&c,None,&meta("native",0),100_901);
        assert_eq!(s.report(Some(&t),100_901)["excluded_early_warmup"],1);
        s.stop(&t).unwrap();
        assert_eq!(s.report(Some(&t),100_901)["status"],"stopped");
    }
}
