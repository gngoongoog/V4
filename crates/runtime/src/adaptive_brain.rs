//! Persisted, account-scoped adaptive advisor. Historical/forward M1 samples are
//! conservative simulated outcomes, never MT5 deals or broker execution proof.
//! The learner is driven only by reconciled completed-bar snapshots (never Fast Pulse).
use anyhow::Result;
use braintrade_domain::{
    execution::Signal,
    studio::FrameBar,
    trading,
    Config, Telemetry,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const HORIZON: usize = 5;
const MIN_HISTORY: usize = 35;
const ALPHA: f64 = 0.12;
const EXPLORATION_MODULUS: u64 = 5;
const MAX_SCORE_R: f64 = 3.0;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Policy {
    samples: u64,
    wins: u64,
    score: f64,
    last_net_r: f64,
}
#[derive(Clone, Debug, Default)]
struct State {
    last_exit: u64,
    next_experiment: u64,
    overall: [Policy; 2],
    regimes: BTreeMap<String, [Policy; 2]>,
}
#[derive(Default)]
pub struct Learner {
    cached: BTreeMap<String, State>,
    last_view: BTreeMap<String, Value>,
}
#[derive(Clone, Debug)]
struct Features {
    trend: i8,
    atr: f64,
    regime: String,
    momentum_atr: f64,
    spread_atr: f64,
}
fn features(bars: &[FrameBar], end: usize, spread: f64) -> Option<Features> {
    if end < MIN_HISTORY || !bars[end-30..end].windows(2).all(|w| w.candle.time + 60 == w.candle.time) {
        return None;
    }
    let series: Vec<_> = bars[end-15..end].iter().map(|v| v.candle.clone()).collect();
    let atr = trading::volatility(&series);
    if !atr.is_finite() || atr <= 0. { return None; }
    let change = bars[end-1].candle.close - bars[end-4].candle.close;
    let momentum_atr = change / atr;
    if momentum_atr.abs() < 0.05 { return None; }
    let spread_atr = spread / atr;
    let regime = format!("{}_{}", if momentum_atr.abs() >= 0.65 { "impulse" } else { "range" },
        if spread_atr >= 0.2 { "high_cost" } else { "normal_cost" });
    Some(Features {
        trend: if change > 0. { 1 } else { -1 },
        atr, regime, momentum_atr, spread_atr,
    })
}
fn policy_name(i: usize) -> &'static str { if i == 0 { "trend" } else { "reversion" } }
fn choose(state: &State, regime: &str) -> (usize, &'static str) {
    let scoped = state.regimes.get(regime).unwrap_or(&state.overall);
    let adjusted = |p: &Policy| p.score * p.samples as f64 / (p.samples as f64 + 12.);
    let favorite = if adjusted(&scoped[1]) > adjusted(&scoped[0]) { 1 } else { 0 };
    if state.next_experiment % EXPLORATION_MODULUS == EXPLORATION_MODULUS - 1 {
        (1 - favorite, "explore")
    } else {
        (favorite, "exploit")
    }
}
fn update(policy: &mut Policy, reward_r: f64, net: f64) {
    let r = reward_r.clamp(-MAX_SCORE_R, MAX_SCORE_R);
    policy.score = if policy.samples == 0 { ALPHA * r } else { (1. - ALPHA) * policy.score + ALPHA * r };
    policy.samples += 1;
    policy.wins += u64::from(net > 0.);
    policy.last_net_r = r;
}
fn valid_contract(t: &Telemetry) -> Option<(f64, f64, f64, f64)> {
    let (point, tick, loss, profit) = (
        t.point?, t.tick_size?, t.tick_value_loss?, t.tick_value_profit?
    );
    if [point,tick,loss,profit].iter().all(|x| x.is_finite() && *x > 0.) {
        Some((point,tick,loss,profit))
    } else { None }
}
fn observed_spread(t: &Telemetry) -> Option<f64> {
    t.bid.zip(t.ask).and_then(|(bid,ask)| {
        let v=ask-bid;
        if bid > 0. && v.is_finite() && v >= 0. { Some(v) } else { None }
    })
}
fn spread_for(bar: &FrameBar, point: f64, fallback: f64) -> (f64, &'static str) {
    if bar.spread_points > 0 {
        ((bar.spread_points as f64 * point).max(fallback.min(0.)), "bar_spread")
    } else { (fallback, "current_quote_proxy") }
}
fn virtual_outcome(
    bars: &[FrameBar], entry_idx: usize, side: i8, atr: f64, spread: f64,
    tick: f64, loss: f64, profit: f64, commission: f64, slip_ticks: f64,
) -> Option<(f64, f64, &'static str, u64)> {
    let entry = &bars[entry_idx].candle;
    let slip = tick * slip_ticks;
    let buy = side > 0;
    let price = entry.open + if buy { spread + slip } else { -slip };
    let stop_distance = (0.8 * atr).max(2. * spread + 2. * slip + 2. * tick);
    let stop = price - side as f64 * stop_distance;
    let target = price + side as f64 * stop_distance * 1.2;
    if stop <= 0. || target <= 0. || price <= 0. { return None; }
    let mut exit = bars[entry_idx + HORIZON - 1].candle.close + if buy { -slip } else { spread + slip };
    let mut reason = "time";
    let mut exit_time = bars[entry_idx + HORIZON - 1].candle.time + 60;
    for b in &bars[entry_idx..entry_idx+HORIZON] {
        let hi = b.candle.high + if buy { 0. } else { spread };
        let lo = b.candle.low + if buy { 0. } else { spread };
        let hit_stop = if buy { lo <= stop } else { hi >= stop };
        let hit_target = if buy { hi >= target } else { lo <= target };
        if hit_stop {
            // Conservative SL-first when both levels are reached within the same M1 candle.
            exit = stop - side as f64 * slip;
            reason = "stop_or_intrabar_ambiguity";
            exit_time = b.candle.time + 60;
            break;
        }
        if hit_target {
            exit = target - side as f64 * slip;
            reason = "target";
            exit_time = b.candle.time + 60;
            break;
        }
    }
    let movement = (exit-price)*side as f64;
    let per_lot = movement / tick * if movement >= 0. {profit} else {loss} - commission;
    let risk_money = stop_distance / tick * loss.max(profit) + commission + 2.*slip/tick*loss.max(profit);
    if !per_lot.is_finite() || !risk_money.is_finite() || risk_money <= 0. { return None; }
    Some((per_lot, (per_lot/risk_money).clamp(-MAX_SCORE_R, MAX_SCORE_R), reason, exit_time))
}
impl Learner {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS adaptive_brain_samples(\
             context TEXT NOT NULL, entry_time INTEGER NOT NULL, exit_time INTEGER NOT NULL,\
             regime TEXT NOT NULL, policy INTEGER NOT NULL, payload TEXT NOT NULL,\
             PRIMARY KEY(context,entry_time));\
             CREATE INDEX IF NOT EXISTS adaptive_brain_context ON adaptive_brain_samples(context,exit_time);"
        )?;
        Ok(Self::default())
    }
    fn load(db: &Connection, ctx: &str) -> Result<State> {
        let mut result=State::default();
        let mut statement=db.prepare("SELECT entry_time,exit_time,regime,policy,payload FROM adaptive_brain_samples WHERE context=?1 ORDER BY entry_time")?;
        let rows=statement.query_map([ctx], |row| Ok((
            row.get::<_,u64>(0)?, row.get::<_,u64>(1)?,
            row.get::<_,String>(2)?, row.get::<_,usize>(3)?, row.get::<_,String>(4)?
        )))?;
        for row in rows {
            let (_,ex,regime,policy,raw)=row?;
            let record: Value=serde_json::from_str(&raw)?;
            if policy >= 2 { continue; }
            let reward=record["reward_r"].as_f64().unwrap_or(0.);
            let net=record["net_per_lot"].as_f64().unwrap_or(0.);
            update(&mut result.overall[policy],reward,net);
            update(&mut result.regimes.entry(regime).or_default()[policy],reward,net);
            result.next_experiment += 1;
            result.last_exit = result.last_exit.max(ex);
        }
        Ok(result)
    }
    /// The only training path. Reconciled, completed M1 history is replayed in
    /// chronological order. Each update sees solely the newly *closed* virtual outcome.
    pub fn observe(
        &mut self, db: &Connection, t: &Telemetry, config: &Config,
        frames: &BTreeMap<String,Vec<FrameBar>>, now: u64
    ) -> Result<Option<Signal>> {
        let ctx=crate::coordinator_learning::context(t);
        let Some(bars)=frames.get("M1") else {return Ok(None)};
        if !t.connected || t.symbol != config.symbol || bars.len() < MIN_HISTORY + HORIZON + 1 {
            return Ok(None);
        }
        let Some((point,tick,loss,profit))=valid_contract(t) else{return Ok(None)};
        let Some(quote_spread)=observed_spread(t) else{return Ok(None)};
        if !self.cached.contains_key(&ctx) {
            self.cached.insert(ctx.clone(),Self::load(db,&ctx)?);
        }
        let state=self.cached.get_mut(&ctx).expect("state inserted");
        let fee=config.commission_profile(t).map(|p|p.basis.round_trip(p.amount).unwrap_or(config.risk.commission_per_lot))
            .unwrap_or(config.risk.commission_per_lot);
        for i in MIN_HISTORY..bars.len().saturating_sub(HORIZON-1) {
            let entry_time=bars[i].candle.time;
            if entry_time < state.last_exit { continue; }
            if !bars[i-1..i+HORIZON].windows(2).all(|w|w[0].candle.time+60==w[1].candle.time) {
                continue;
            }
            let (spread,source)=spread_for(&bars[i-1],point,quote_spread);
            let Some(f)=features(bars,i,spread) else { continue };
            let (policy,exploration)=choose(state,&f.regime);
            let side = if policy==0 {f.trend}else{-f.trend};
            let Some((net_per_lot,reward_r,exit_reason,exit_time))=virtual_outcome(
                bars,i,side,f.atr,spread,tick,loss,profit,fee,config.risk.slippage_ticks,
            ) else {continue};
            let previous=state.regimes.get(&f.regime).map(|v|v[policy].score).unwrap_or(0.);
            let payload=json!({
                "source":"completed_m1_simulation_not_broker",
                "account_context":ctx,"signal_bar":bars[i-1].candle.time,
                "entry_time":entry_time,"exit_time":exit_time,
                "policy":policy_name(policy),"selection":exploration,
                "side":side,"regime":f.regime,"momentum_atr":f.momentum_atr,
                "spread_atr":f.spread_atr,"spread":spread,"cost_source":source,
                "fee_per_lot_round_trip":fee,"slippage_ticks":config.risk.slippage_ticks,
                "net_per_lot":net_per_lot,"reward_r":reward_r,
                "exit_reason":exit_reason,"score_before":previous
            });
            db.execute(
                "INSERT OR IGNORE INTO adaptive_brain_samples(context,entry_time,exit_time,regime,policy,payload) VALUES(?1,?2,?3,?4,?5,?6)",
                params![&ctx,entry_time,exit_time,&f.regime,policy,payload.to_string()],
            )?;
            state.last_exit=exit_time;
            state.next_experiment+=1;
            update(&mut state.overall[policy],reward_r,net_per_lot);
            update(&mut state.regimes.entry(f.regime).or_default()[policy],reward_r,net_per_lot);
        }
        let last=bars.last().expect("nonempty");
        let (spread,source)=spread_for(last,point,quote_spread);
        let Some(f)=features(bars,bars.len(),spread) else{return Ok(None)};
        if last.candle.time + 60 > now / 1000 || now/1000 > last.candle.time + 130 {
            return Ok(None);
        }
        let (policy,selection)=choose(state,&f.regime);
        let side=if policy==0 {f.trend}else{-f.trend};
        let scoped=state.regimes.get(&f.regime).unwrap_or(&state.overall);
        let samples=scoped[policy].samples;
        let score=scoped[policy].score;
        let s=Signal {
            brain:"adaptive".into(),
            side,
            key:format!("adaptive:M1:{}:{}:{}",last.candle.time,f.regime,policy_name(policy)),
            bar_time:last.candle.time,
            stop_distance: (f.atr * 0.8).max(2.*spread + 2.*config.risk.slippage_ticks*tick + 2.*tick),
            target_distance: (f.atr * 0.8).max(2.*spread + 2.*config.risk.slippage_ticks*tick + 2.*tick)*1.2,
            hold_ms:5*60*1000,
            reason:format!(
                "Adaptive v1 / {} {} | regime {} | {} عينات منتهية في هذا السياق | score {:.3} R | momentum {:.2} ATR | spread/ATR {:.2} | مصدر التكلفة {} | سجلات التعلم تقديرية وليست صفقات وسيط",
                policy_name(policy),selection,f.regime,samples,score,f.momentum_atr,f.spread_atr,source
            ),
            ready:true,
            coordination:None,
        };
        self.last_view.insert(ctx,json!({
            "brain":"adaptive","version":"adaptive-contextual-v1",
            "source":"historical/forward simulated M1 after-cost results, NOT broker fills",
            "total_closed_training_samples":state.next_experiment,
            "latest_closed_trade_time":state.last_exit,
            "regime":f.regime,"selection":selection,"policy":policy_name(policy),
            "policy_scores":{"trend":scoped[0].score,"reversion":scoped[1].score},
            "policy_samples":{"trend":scoped[0].samples,"reversion":scoped[1].samples},
            "persisted":true,"training_account":t.login,
            "warning":"Research proxy evidence is not real executed Demo/Live PnL; actual broker volume and margin remain mandatory."
        }));
        Ok(Some(s))
    }
    pub fn status(&self,t: Option<&Telemetry>) -> Value {
        t.and_then(|v|self.last_view.get(&crate::coordinator_learning::context(v))).cloned()
            .unwrap_or_else(||json!({"brain":"adaptive","status":"waiting for valid M1 and broker contract","persisted":true}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::Candle;
    fn dataset(n: usize) -> Vec<FrameBar> {
        (0..n).map(|i| {
            let price=2000.+(i as f64/6.).sin()*3.+i as f64*.02;
            FrameBar {candle:Candle{time:1_800_000_000/60*60+i as u64*60,open:price,
            high:price+1.3,low:price-1.3,close:price+0.1,tick_volume:100},
            spread_points:20, real_volume:0}
        }).collect()
    }
    fn telemetry(now:u64) -> Telemetry {
        Telemetry{
            connected:true,server:"DemoSrv".into(),login:42,symbol:"XAUUSD".into(),
            currency:"USD".into(),point:Some(0.01),tick_size:Some(0.01),
            tick_value_loss:Some(1.),tick_value_profit:Some(1.),bid:Some(2020.),
            ask:Some(2020.2),tick_age_ms:Some(0),quote_time_ms:Some(now),
            ..Default::default()
        }
    }
    #[test]
    fn persistent_causal_samples_and_no_double_count_on_restart() {
        let db=Connection::open_in_memory().unwrap();
        let mut learner=Learner::new(&db).unwrap();
        let data=dataset(170);
        let now=(data.last().unwrap().candle.time+60)*1000;
        let frames=BTreeMap::from([("M1".into(),data)]);
        let t=telemetry(now);
        let result=learner.observe(&db,&t,&Config::default(),&frames,now).unwrap().unwrap();
        assert!(result.ready);
        let state=learner.status(Some(&t));
        let n=state["total_closed_training_samples"].as_u64().unwrap();
        assert!(n>=10);
        assert!(state["policy_samples"]["trend"].as_u64().unwrap()+state["policy_samples"]["reversion"].as_u64().unwrap()>0);
        drop(learner);
        let mut reloaded=Learner::new(&db).unwrap();
        let same=reloaded.observe(&db,&t,&Config::default(),&frames,now).unwrap().unwrap();
        assert_eq!(same.key,result.key);
        assert_eq!(reloaded.status(Some(&t))["total_closed_training_samples"],n);
        let count:u64=db.query_row("SELECT COUNT(*) FROM adaptive_brain_samples",[],|r|r.get(0)).unwrap();
        assert_eq!(count,n);
    }
    #[test]
    fn one_thousand_negative_rewards_cannot_be_erased_by_one_win() {
        let mut p=Policy::default();
        for _ in 0..1000 {update(&mut p,-1.,-1.);}
        let before=p.score;
        update(&mut p,1.,1.);
        assert!(before < -0.9);
        assert!(p.score < 0.);
        assert_eq!(p.samples,1001);
        assert_eq!(p.wins,1);
    }
    #[test]
    fn spread_and_commission_change_rewards() {
        let bars=dataset(50);
        let cheap=virtual_outcome(&bars,40,1,1.5,0.1,0.01,1.,1.,0.,0.).unwrap();
        let dear=virtual_outcome(&bars,40,1,1.5,0.5,0.01,1.,1.,7.,2.).unwrap();
        assert!(dear.0<cheap.0);
    }
}
