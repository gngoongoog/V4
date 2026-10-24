//! P1-K5: Classic Sniper prospective full-snapshot study (NOT the Learner tick cohort).
//! A read-only research lane. No MT5 order, learner update, promotion or booked PnL.
//! Classic consults M1/market analysis, so accepted Fast Pulses alone are not
//! sufficient evidence for a policy comparison.
use anyhow::{ensure, Result};
use braintrade_domain::{
    execution::{self, BrainConfig},
    studio::FrameBar,
    trading::{SniperBrain, TradingMode},
    Config, Telemetry,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn identity(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap_or_default()
}
fn eligible(t: &Telemetry, c: &Config, now: u64) -> bool {
    t.connected && t.demo && t.account_trade_mode == Some(0) && t.login > 0
        && !t.server.is_empty() && !t.currency.is_empty() && t.symbol == c.symbol
        && c.trading.mode == TradingMode::Sniper
        && c.trading.sniper_brain == SniperBrain::Classic
        && t.source_quote_time_ms.is_some() && t.quote_is_fresh(now, 1500)
}
struct Trial {
    context: String, started_ms: u64, config: Config,
    active: bool, reason: String,
    last_event: Option<u64>, last_broker: Option<u64>,
    prospective_full_snapshots: u64, evaluated: u64, buy: u64, sell: u64,
    wait: u64, missing_market: u64, stale_or_duplicate: u64,
}
#[derive(Default)]
pub struct Study { trial: Option<Trial> }
impl Study {
    pub fn clear_on_mode_change(&mut self) { self.trial = None; }
    pub fn freeze(&mut self, t: &Telemetry, c: &Config, now: u64) -> Result<Value> {
        ensure!(eligible(t, c, now), "Classic research requires a fresh bound MT5 Demo quote");
        ensure!(self.trial.as_ref().is_none_or(|x| !x.active), "Stop existing Classic study first");
        c.validate().map_err(anyhow::Error::msg)?;
        self.trial = Some(Trial {
            context: identity(t), started_ms: now, config: c.clone(),
            active: true, reason: String::new(), last_event: None, last_broker: None,
            prospective_full_snapshots: 0, evaluated: 0, buy: 0, sell: 0, wait: 0,
            missing_market: 0, stale_or_duplicate: 0,
        });
        Ok(json!({"ok":true,"status":"active","context":identity(t),
            "policy":"p1k5-sniper-classic-forward-v1",
            "source":"reconciled_full_snapshot_and_m1",
            "no_broker_orders":true,"no_learning_effect":true,
            "persistence":"memory_only"}))
    }
    pub fn stop(&mut self, t: &Telemetry) -> Result<Value> {
        let x=self.trial.as_mut().filter(|x|x.context==identity(t))
            .ok_or_else(||anyhow::anyhow!("No matching Classic study"))?;
        x.active=false;x.reason="operator_stopped".into();
        Ok(json!({"ok":true,"status":"stopped","no_broker_orders":true}))
    }
    pub fn observe(&mut self,t:&Telemetry,c:&Config,
        frames:&BTreeMap<String,Vec<FrameBar>>,now:u64) {
        let Some(x)=self.trial.as_mut().filter(|x|x.active)else{return};
        if identity(t)!=x.context||!eligible(t,c,now){
            x.active=false;x.reason="broker_identity_mode_or_quote_stale".into();return;
        }
        if serde_json::to_value(c).ok()!=serde_json::to_value(&x.config).ok(){
            x.active=false;x.reason="classic_policy_changed_after_freeze".into();return;
        }
        let (Some(event),Some(broker))=(t.quote_time_ms,t.source_quote_time_ms)
        else{x.stale_or_duplicate+=1;return};
        // Broker clock is original source time; logical event keys may be
        // synthetic and > wall-clock during dense same-ms native ticks.
        if broker<=x.started_ms ||now<=x.started_ms
          ||x.last_event.is_some_and(|v|event<=v)
          ||x.last_broker.is_some_and(|v|broker<v) {
            x.stale_or_duplicate+=1;return;
        }
        x.last_event=Some(event);x.last_broker=Some(broker);
        x.prospective_full_snapshots+=1;
        // The Classic policy reads analyzed M1 evidence; missing market
        // snapshots must not be silently counted as successful WAIT signals.
        if t.market.is_none() || frames.get("M1").is_none_or(|v|v.is_empty()){
            x.missing_market+=1;return;
        }
        let signal=execution::evaluate(&BrainConfig::default(),t,&x.config,frames,None,now);
        x.evaluated+=1;
        match (signal.ready,signal.side){
            (true,1)=>x.buy+=1,
            (true,-1)=>x.sell+=1,
            _=>x.wait+=1,
        }
    }
    pub fn report(&self,t:Option<&Telemetry>,now:u64)->Value{
        let Some(x)=self.trial.as_ref()else{
            return json!({"schema":"p1k5-classic-forward-v1","status":"not_started",
                "no_broker_orders":true,"independent_oos":false});
        };
        let Some(t)=t.filter(|t|t.connected&&t.demo&&t.account_trade_mode==Some(0)
            &&identity(t)==x.context)else{
            return json!({"schema":"p1k5-classic-forward-v1","status":"unavailable",
                "no_broker_orders":true});
        };
        json!({"schema":"p1k5-classic-forward-v1",
            "status":if x.active{"active"}else{"stopped"},"reason":x.reason,
            "context":x.context,"mode":"sniper","sniper_brain":"classic",
            "started_ms":x.started_ms,"observed_ms":now,
            "prospective_full_snapshots":x.prospective_full_snapshots,
            "evaluated_classic_snapshots":x.evaluated,
            "raw_buy_signals":x.buy,"raw_sell_signals":x.sell,"raw_wait_signals":x.wait,
            "excluded_missing_m1_or_market":x.missing_market,
            "excluded_old_or_duplicate":x.stale_or_duplicate,
            "source":"reconciled_full_snapshot_and_m1_not_fast_pulse",
            "policy_snapshot_frozen":true,"forward_only":true,
            "no_broker_orders":true,"no_learning_effect":true,
            "independent_oos":false,"market_tick_complete":false,
            "broker_pnl":Value::Null,"paper_pnl":Value::Null,
            "strategy_win_rate":Value::Null,"persistence":"memory_only",
            "note":"Observed future Classic decision counts only; not Learner Tick parity, future-independent OOS, filled orders or strategy performance."})
    }
}
#[cfg(test)]
mod tests{
 use super::*;
 fn fixture(now:u64)->(Telemetry,Config){
   let mut c=Config::default();c.trading.mode=TradingMode::Sniper;
   c.trading.sniper_brain=SniperBrain::Classic;
   let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
     login:17,server:"Demo".into(),symbol:c.symbol.clone(),currency:"USD".into(),
     bid:Some(2500.),ask:Some(2500.2),tick_age_ms:Some(0),
     quote_time_ms:Some(now),source_quote_time_ms:Some(now),..Default::default()};
   (t,c)
 }
 #[test]
 fn classic_only_demo_only_no_pnl_or_backfill() {
   let (mut t,mut c)=fixture(100000);let mut s=Study::default();
   t.demo=false;assert!(s.freeze(&t,&c,100000).is_err());
   t.demo=true;c.trading.sniper_brain=SniperBrain::Learner;
   assert!(s.freeze(&t,&c,100000).is_err());
   c.trading.sniper_brain=SniperBrain::Classic;
   s.freeze(&t,&c,100000).unwrap();
   t.quote_time_ms=Some(100300);t.source_quote_time_ms=Some(100300);
   s.observe(&t,&c,&BTreeMap::new(),100301);
   let r=s.report(Some(&t),100301);
   assert_eq!(r["prospective_full_snapshots"],1);
   assert_eq!(r["evaluated_classic_snapshots"],0);
   assert_eq!(r["excluded_missing_m1_or_market"],1);
   assert_eq!(r["no_broker_orders"],true);
   assert_eq!(r["independent_oos"],false);
   assert!(r["broker_pnl"].is_null());
   s.observe(&t,&c,&BTreeMap::new(),100302);
   assert_eq!(s.report(Some(&t),100302)["prospective_full_snapshots"],1);
   let mut foreign=t.clone();foreign.login=99;
   assert_eq!(s.report(Some(&foreign),100303)["status"],"unavailable");
   assert!(s.stop(&foreign).is_err());
 }
 #[test]
 fn changing_classic_settings_or_real_account_ends_cohort() {
   let (mut t,mut c)=fixture(100000);let mut s=Study::default();
   s.freeze(&t,&c,100000).unwrap();
   c.trading.sniper_stop_points+=10.;
   t.quote_time_ms=Some(100250);t.source_quote_time_ms=Some(100250);
   s.observe(&t,&c,&BTreeMap::new(),100251);
   assert_eq!(s.report(Some(&t),100251)["status"],"stopped");
   assert_eq!(s.report(Some(&t),100251)["reason"],"classic_policy_changed_after_freeze");
   let (mut t,c)=fixture(200000);let mut s=Study::default();
   s.freeze(&t,&c,200000).unwrap();
   t.demo=false;t.account_trade_mode=Some(2);
   t.quote_time_ms=Some(200300);t.source_quote_time_ms=Some(200300);
   s.observe(&t,&c,&BTreeMap::new(),200301);
   assert_eq!(s.report(Some(&t),200302)["status"],"unavailable");
 }
}
