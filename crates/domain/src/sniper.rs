//! Quote-driven Sniper specialist. Learning uses closed Demo broker evidence only;
//! the tick path reads a cached model and never trains or touches storage.
use crate::{execution::Signal, trading::{self, SniperBrain, TradingMode}, Config, Telemetry};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};

pub const ID: &str = "sniper_learner";
pub const POLICIES: [&str; 2] = ["impulse", "rebound"];

#[derive(Clone, Debug, Default, Serialize)]
pub struct Policy {
    pub samples: u64,
    pub wins: u64,
    pub reward: f64,
    pub loss_streak: u64,
}
impl Policy {
    pub fn learn(&mut self, reward: f64, net: f64) {
        if !reward.is_finite() || !net.is_finite() { return; }
        self.reward = 0.9 * self.reward + 0.1 * reward.clamp(-1., 1.);
        self.samples += 1;
        self.wins += u64::from(net > 0.);
        self.loss_streak = if net < 0. { self.loss_streak + 1 } else { 0 };
    }
    fn score(&self) -> f64 {
        self.reward * self.samples as f64 / (self.samples as f64 + 8.)
            - if self.loss_streak >= 3 { (self.loss_streak as f64 * 0.02).min(0.15) } else { 0. }
    }
}
pub type Models = BTreeMap<(String, String), [Policy; 2]>;

pub fn selected(c: &Config) -> bool {
    c.trading.mode == TradingMode::Sniper && c.trading.sniper_brain == SniperBrain::Learner
}
pub fn definition(t: &Telemetry, c: &Config) -> String {
    // Monetary tick conversion changes must not fragment the same policy's history.
    json!(["sniper-ticks-v1", c.trading.sniper_target_points, c.trading.sniper_stop_points,
        c.trading.sniper_max_hold_seconds, c.risk.commission_per_lot,
        c.risk.slippage_ticks, t.point, t.tick_size]).to_string()
}
#[derive(Default)]
pub struct Engine {
    pub models: Models,
    account: String,
    definition: String,
    quotes: VecDeque<(u64, u64, f64)>, // broker clock, receipt clock, midpoint
}
impl Engine {
    pub fn set_models(&mut self, account: String, models: Models) {
        if self.account != account { self.quotes.clear(); }
        self.account = account;
        self.models = models;
    }
    pub fn reset_quotes(&mut self) { self.quotes.clear(); }
    pub fn evaluate(&mut self, t: &Telemetry, c: &Config, now: u64) -> Signal {
        let mut s = Signal { brain: ID.into(), side: 0, key: String::new(), bar_time: now / 1000,
            stop_distance: 0., target_distance: 0., hold_ms: c.trading.hold_ms(),
            reason: "عقل القناص المتعلّم: اختر Sniper ثم القناص المتعلّم من مركز التداول".into(),
            ready: false, coordination: None };
        if !selected(c) { self.reset_quotes(); return s; }
        let def = definition(t, c);
        if self.definition != def { self.reset_quotes(); self.definition = def.clone(); }
        let Some(ts) = t.quote_time_ms.filter(|ts| *ts <= now + 2000 && now.saturating_sub(*ts) <= 1000) else {
            self.reset_quotes(); s.reason = "القناص: ينتظر سعراً حديثاً".into(); return s;
        };
        let cost = match trading::costs(t, c, 1., now) {
            Ok(cost) if t.symbol == c.symbol && t.tick_age_ms.is_some_and(|age| age <= 1000) => cost,
            _ => { self.reset_quotes(); s.reason = "القناص: بيانات السعر أو العقد غير مكتملة".into(); return s; }
        };
        let (bid, ask, tick, point) = (t.bid.unwrap(), t.ask.unwrap(), t.tick_size.unwrap(), t.point.unwrap());
        let mid = (bid + ask) / 2.;
        if self.quotes.back().is_some_and(|q| now.saturating_sub(q.1) > 1000 || ts < q.0) { self.reset_quotes(); }
        if self.quotes.back().is_none_or(|q| ts > q.0) { self.quotes.push_back((ts, now, mid)); }
        while self.quotes.len() > 64 || self.quotes.front().is_some_and(|q| now.saturating_sub(q.1) > 2000) { self.quotes.pop_front(); }
        s.key = format!("tick:{ts}");
        if self.quotes.len() < 3 { s.reason = "القناص: يجمع ثلاثة أسعار متتابعة قبل القرار".into(); return s; }
        if cost.total / (cost.total + cost.desired_net_target_money) > c.trading.max_cost_fraction {
            s.reason = "القناص: حصة السبريد والعمولة والانزلاق تتجاوز اختيارك".into(); return s;
        }
        let q: Vec<f64> = self.quotes.iter().map(|q| q.2).collect();
        let n = q.len();
        let movement = q[n-1] - q[0];
        let step = q[n-1] - q[n-2];
        let path: f64 = q.windows(2).map(|w| (w[1]-w[0]).abs()).sum();
        let threshold = tick.max((ask-bid)*0.15);
        let direction = |v: f64| if v >= threshold * 0.999 { 1 } else if v <= -threshold * 0.999 { -1 } else { 0 };
        let impulse = if movement.abs()/path.max(tick) >= 0.5 { direction(movement) } else { 0 };
        // A measured reversal after a prior excursion; never an arbitrary opposite trade.
        let prior = q[n-2] - q[0];
        let rebound = if prior.abs() >= threshold*2. && step*prior < 0. { direction(step) } else { 0 };
        let sides = [impulse, rebound];
        let regime = format!("{}_{}", if movement.abs()/path.max(tick) >= 0.6 {"directional"} else {"choppy"},
            if cost.total/(cost.total+cost.desired_net_target_money) > 0.25 {"costly"} else {"normal"});
        let policies = self.models.get(&(def.clone(), regime.clone())).cloned().unwrap_or_default();
        let candidates: Vec<usize> = (0..2).filter(|i| sides[*i] != 0).collect();
        if candidates.is_empty() { s.reason = "القناص: لا يوجد اندفاع أو ارتداد مكتمل في الأسعار الحالية".into(); return s; }
        let demo = t.demo && t.account_trade_mode == Some(0);
        let total: u64 = policies.iter().map(|p| p.samples).sum();
        let epsilon = if demo { crate::student::exploration_rate(total) } else { 0. };
        let best = *candidates.iter().max_by(|a,b| {
            let score = |i: usize| if demo { policies[i].score() } else { 0. };
            score(**a).total_cmp(&score(**b)).then_with(|| b.cmp(a))
        }).unwrap();
        let seed = json!([self.account, def, regime, ts]).to_string();
        let exploring = candidates.len()>1 && crate::student::draw(&seed, 0) < epsilon;
        let chosen = if exploring { candidates[(crate::student::draw(&seed, 1)*candidates.len() as f64) as usize] } else { best };
        let probability = epsilon / candidates.len() as f64 + if chosen==best { 1.-epsilon } else { 0. };
        s.side = sides[chosen];
        s.stop_distance = (c.trading.sniper_stop_points*point).max(ask-bid+2.*tick);
        s.target_distance = c.trading.sniper_target_points*point
            + (c.risk.commission_per_lot + 2.*c.risk.slippage_ticks*t.tick_value_loss.unwrap().max(t.tick_value_profit.unwrap())) / t.tick_value_profit.unwrap()*tick;
        s.ready = true;
        s.reason = format!("القناص المتعلّم: {} | {} | صفقات مكتملة {} | استكشاف {}",
            if chosen==0 {"اندفاع سريع"} else {"ارتداد سريع"}, regime, total, exploring);
        s.coordination = Some(json!({"version":"sniper-learner-v1","trading_mode":"sniper",
            "selection":{"brain":ID,"timeframe":"M1","report_id":"","policy":"sniper-ticks-v1",
                "sniper_policy":POLICIES[chosen],"sniper_definition":def,"sniper_regime":regime,
                "definition":["sniper-ticks-v1",POLICIES[chosen]],"exploring":exploring,
                "exploration_rate":epsilon,"selection_probability":probability,"broker_samples":total,
                "policies":policies,"applied_demo_learning":demo},
            "members":[{"id":ID,"timeframe":"M1","report_id":"","bar_time":s.bar_time,"side":s.side,"ready":true}]}));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Telemetry, Config) {
        let mut c=Config::default(); c.trading.mode=TradingMode::Sniper; c.trading.sniper_brain=SniperBrain::Learner;
        let t=Telemetry{connected:true,symbol:c.symbol.clone(),demo:true,account_trade_mode:Some(0),
            bid:Some(2000.),ask:Some(2000.01),tick_size:Some(0.01),point:Some(0.01),tick_value_loss:Some(1.),
            tick_value_profit:Some(1.),tick_age_ms:Some(0),..Default::default()}; (t,c)
    }
    fn feed(e:&mut Engine,t:&mut Telemetry,c:&Config,ts:u64,bid:f64)->Signal {
        t.bid=Some(bid);t.ask=Some(bid+0.01);t.quote_time_ms=Some(ts);e.evaluate(t,c,ts)
    }
    #[test] fn ticks_costs_mode_freshness_and_reversal() {
        let (mut t,mut c)=fixture(); let mut e=Engine::default();
        assert!(!feed(&mut e,&mut t,&c,10000,2000.).ready);
        assert!(!feed(&mut e,&mut t,&c,10010,2000.04).ready);
        let s=feed(&mut e,&mut t,&c,10020,2000.08); assert!(s.ready);assert_eq!(s.side,1);assert!(s.key.starts_with("tick:"));
        let s=feed(&mut e,&mut t,&c,10030,2000.05);assert!(s.ready);assert_eq!(s.side,-1);
        assert_eq!(s.coordination.unwrap()["selection"]["sniper_policy"],"rebound");
        c.trading.max_cost_fraction=0.01;assert!(!feed(&mut e,&mut t,&c,10040,2000.10).ready);
        c.trading.mode=TradingMode::Expert;assert!(!feed(&mut e,&mut t,&c,10050,2000.20).ready);
        c.trading.mode=TradingMode::Sniper;assert!(!feed(&mut e,&mut t,&c,10060,2000.30).ready);
        assert!(!feed(&mut e,&mut t,&c,12000,2000.40).ready);
    }
    #[test] fn duplicate_ticks_do_not_warm_up_and_account_change_resets() {
        let (mut t,c)=fixture();let mut e=Engine::default();
        for _ in 0..10 {assert!(!feed(&mut e,&mut t,&c,10000,2000.).ready);}
        feed(&mut e,&mut t,&c,10010,2000.1);assert!(feed(&mut e,&mut t,&c,10020,2000.2).ready);
        e.set_models("new account".into(),Models::new());assert!(!feed(&mut e,&mut t,&c,10030,2000.3).ready);
    }
    #[test] fn losses_reduce_preference_and_nonloss_clears_streak() {
        let mut p=Policy::default();for _ in 0..10 {p.learn(-0.5,-2.);}
        assert!(p.score() < -0.2);p.learn(0.5,2.);assert_eq!(p.loss_streak,0);assert!(p.score()<0.);
        p.learn(f64::NAN,2.);assert_eq!(p.samples,11);
    }
    #[test] fn closed_evidence_changes_choice_without_eliminating_exploration() {
        let (mut t,c)=fixture();let mut changed=false;
        for start in (10000..10200).step_by(10) {
            let mut baseline=Engine::default();let mut learned=Engine::default();
            let mut policies=[Policy::default(),Policy::default()];
            for _ in 0..20 { policies[0].learn(-0.5,-2.);policies[1].learn(0.5,2.); }
            learned.models.insert((definition(&t,&c),"directional_normal".into()),policies);
            for (ts,bid) in [(start,2000.),(start+1,2000.08)] {
                feed(&mut baseline,&mut t,&c,ts,bid);feed(&mut learned,&mut t,&c,ts,bid);
            }
            let a=feed(&mut baseline,&mut t,&c,start+2,2000.07);
            let b=feed(&mut learned,&mut t,&c,start+2,2000.07);
            assert!(a.ready && b.ready);
            let choice=b.coordination.unwrap();
            assert!(choice["selection"]["exploration_rate"].as_f64().unwrap()>0.);
            if !choice["selection"]["exploring"].as_bool().unwrap() {
                assert_eq!(b.side,-1);changed |= a.side!=b.side;
            }
        }
        assert!(changed);
    }
}
