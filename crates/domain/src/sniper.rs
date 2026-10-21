//! Quote-driven Sniper specialist. Learning uses closed Demo broker evidence only;
//! the tick path reads a cached model and never trains or touches storage.
use crate::{execution::Signal, trading::{self, SniperBrain, SniperStyle, SniperTargetMode, TradingMode}, Config, Telemetry};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};

pub const ID: &str = "sniper_learner";
pub const POLICIES: [&str; 2] = ["impulse", "rebound"];
pub const TARGET_ARMS: usize = 4;
/// Four stationary arms inside the user-defined target bounds. Only real,
/// fully closed Demo results for the chosen arm may change its score.
pub fn target_candidates(c: &Config) -> [f64; TARGET_ARMS] {
    let min = c.trading.sniper_min_target_pips;
    let gap = c.trading.sniper_max_target_pips - min;
    std::array::from_fn(|i| min + gap * (i as f64 / (TARGET_ARMS-1) as f64))
}

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
pub type TargetModels = BTreeMap<(String, String, String), [Policy; TARGET_ARMS]>;

pub fn selected(c: &Config) -> bool {
    c.trading.mode == TradingMode::Sniper && c.trading.sniper_brain == SniperBrain::Learner
}
pub fn definition(t: &Telemetry, c: &Config) -> String {
    // Monetary tick conversion changes must not fragment the same policy's history.
    let mut definition = json!(["sniper-ticks-v1", c.trading.sniper_target_points, c.trading.sniper_stop_points,
        c.trading.sniper_max_hold_seconds, c.risk.commission_per_lot,
        c.risk.slippage_ticks, t.point, t.tick_size, c.trading.pip_points, c.trading.sniper_pip_price,
        c.trading.sniper_target_mode, c.trading.sniper_min_target_pips,
        c.trading.sniper_max_target_pips]);
    // Preserve existing Auto evidence; never mix a new style into that history.
    if c.trading.sniper_style != SniperStyle::Auto {
        definition.as_array_mut().unwrap().push(json!(c.trading.sniper_style));
    }
    definition.to_string()
}
#[derive(Default)]
pub struct Engine {
    pub models: Models,
    pub target_models: TargetModels,
    account: String,
    definition: String,
    quotes: VecDeque<(u64, u64, f64)>, // broker clock, receipt clock, midpoint
}
impl Engine {
    pub fn set_models(&mut self, account: String, models: Models) {
        self.set_models_with_targets(account,models,TargetModels::new());
    }
    pub fn set_models_with_targets(&mut self, account: String, models: Models, targets: TargetModels) {
        if self.account != account { self.quotes.clear(); }
        self.account = account;
        self.models = models;
        self.target_models = targets;
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
        // A candidate-specific cost gate runs after the pip arm is chosen.
        // Comparing against the fixed target here would incorrectly reject
        // profitable larger user-permitted alternatives before they are seen.
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
        let sides = match c.trading.sniper_style {
            SniperStyle::Auto => [impulse, rebound],
            SniperStyle::Impulse => [impulse, 0],
            SniperStyle::Rebound => [0, rebound],
            SniperStyle::Confirmed => {
                let same_direction = n >= 5 && q[n-3..].windows(2)
                    .all(|w| (w[1]-w[0]) * impulse as f64 > 0.);
                [if same_direction && movement.abs()/path.max(tick) >= 0.75 {impulse} else {0}, 0]
            }
        };
        let regime = format!("{}_{}", if movement.abs()/path.max(tick) >= 0.6 {"directional"} else {"choppy"},
            if cost.total/(cost.total+cost.desired_net_target_money) > 0.25 {"costly"} else {"normal"});
        let policies = self.models.get(&(def.clone(), regime.clone())).cloned().unwrap_or_default();
        let candidates: Vec<usize> = (0..2).filter(|i| sides[*i] != 0).collect();
        if candidates.is_empty() { s.reason = format!("القناص — {}: ينتظر حركة تستوفي شروط هذا الأسلوب", c.trading.sniper_style.label()); return s; }
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
        let adaptive = demo && c.trading.sniper_target_mode == SniperTargetMode::Adaptive;
        let arms = target_candidates(c);
        let fixed_pips = c.trading.sniper_target_points / c.trading.pip_points;
        let target_key=(def.clone(), regime.clone(), POLICIES[chosen].to_owned());
        let target_policies = self.target_models.get(&target_key).cloned().unwrap_or_default();
        let mut eligible = Vec::new();
        if adaptive {
            for i in 0..TARGET_ARMS {
                let desired=arms[i]*c.trading.sniper_pip_price/tick*t.tick_value_profit.unwrap();
                if desired.is_finite() && desired>0. && cost.total/(cost.total+desired) <= c.trading.max_cost_fraction {
                    eligible.push(i);
                }
            }
        }
        let target_total: u64 = target_policies.iter().map(|p|p.samples).sum();
        let mut target_arm: Option<usize> = None;
        let mut target_exploring=false;
        let target_pips=if adaptive {
            if eligible.is_empty() {
                s.reason=format!(
                    "القناص: لا هدف ضمن {:.2}–{:.2} pip يجتاز حصة التكلفة {:.0}% | سبريد {:.2} pip | عمولة {:.4}/لوت | انزلاق {:.2} tick/جهة | الإجمالي {:.4}/لوت. يمكن تعديل هدف pip أو نسبة التكلفة، لكن السبريد الحقيقي لا يمكن تجاهله",
                    c.trading.sniper_min_target_pips, c.trading.sniper_max_target_pips,
                    c.trading.max_cost_fraction*100.,cost.spread_pips,c.risk.commission_per_lot,
                    c.risk.slippage_ticks,cost.total
                );
                return s;
            }
            let best_arm = *eligible.iter().max_by(|a,b|
                target_policies[**a].score().total_cmp(&target_policies[**b].score())
                    .then_with(|| b.cmp(a))
            ).unwrap();
            let target_epsilon=crate::student::exploration_rate(target_total);
            target_exploring=eligible.len()>1 && crate::student::draw(&seed,2)<target_epsilon;
            let pick=if target_exploring {
                eligible[((crate::student::draw(&seed,3)*eligible.len() as f64) as usize).min(eligible.len()-1)]
            } else {best_arm};
            target_arm=Some(pick);
            arms[pick]
        } else {
            let desired=fixed_pips*c.trading.sniper_pip_price/tick*t.tick_value_profit.unwrap();
            if !desired.is_finite() || desired<=0. || cost.total/(cost.total+desired)>c.trading.max_cost_fraction {
                s.reason=format!(
                    "القناص: الهدف {:.2} pip لا يجتاز حصة التكلفة {:.0}% | السبريد {:.2} pip (حقيقي) | العمولة {:.4}/لوت | الانزلاق {:.2} tick/جهة | الإجمالي {:.4}/لوت. راجع هدف الربح وعمولة الحساب ولا تُخفّض السبريد الحقيقي",
                    fixed_pips,c.trading.max_cost_fraction*100.,cost.spread_pips,
                    c.risk.commission_per_lot,c.risk.slippage_ticks,cost.total
                );
                return s;
            }
            fixed_pips
        };
        s.side = sides[chosen];
        s.stop_distance = (c.trading.sniper_stop_points*point).max(ask-bid+2.*tick);
        // The shared broker-entry geometry intentionally excludes the spread
        // from target distance: Ask/Bid entry already paid that spread once.
        s.target_distance = match trading::sniper_target_from_entry(
            &cost,
            target_pips * c.trading.sniper_pip_price,
        ) {
            Ok(v) => v,
            Err(error) => { s.side = 0; s.reason = error; return s; }
        };
        s.ready = true;
        s.reason = format!("القناص المتعلّم: {} | {} | هدف {:.2} pip {} | نتائج الاستراتيجية {} / عينات الهدف {}",
            if chosen==0 {"اندفاع سريع"} else {"ارتداد سريع"}, regime, target_pips,
            if adaptive {"تجريبي مرن"} else {"ثابت"}, total,target_total);
        s.coordination = Some(json!({"version":"sniper-learner-v1","trading_mode":"sniper",
            "selection":{"sniper_style":c.trading.sniper_style,"brain":ID,"timeframe":"M1","report_id":"","policy":"sniper-ticks-v1",
                "sniper_policy":POLICIES[chosen],"sniper_definition":def,"sniper_regime":regime,
                "definition":["sniper-ticks-v1",POLICIES[chosen]],"exploring":exploring,
                "exploration_rate":epsilon,"selection_probability":probability,"broker_samples":total,
                "sniper_target_mode":if adaptive {"adaptive"}else{"fixed"},
                "sniper_target_pips":target_pips,"sniper_target_points":target_pips*c.trading.pip_points,
                "sniper_target_arm":target_arm,"sniper_target_exploring":target_exploring,
                "sniper_target_samples":target_total,"sniper_target_candidates_pips":arms,
                "sniper_target_policies":target_policies,
                "sniper_target_evidence":"booked Demo net outcomes for the chosen pip arm only; not a win probability",
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
    #[test]
    fn substyles_are_isolated_and_confirmation_waits_for_evidence() {
        let (mut t,mut c)=fixture();let original=definition(&t,&c);
        for style in [SniperStyle::Impulse,SniperStyle::Rebound,SniperStyle::Confirmed] {
            c.trading.sniper_style=style;
            assert_ne!(original,definition(&t,&c));
            let mut e=Engine::default();
            feed(&mut e,&mut t,&c,10000,2000.);
            feed(&mut e,&mut t,&c,10010,2000.08);
            let signal=feed(&mut e,&mut t,&c,10020,2000.06);
            match style {
                SniperStyle::Impulse=>{assert!(signal.ready);assert_eq!(signal.side,1);},
                SniperStyle::Rebound=>{assert!(signal.ready);assert_eq!(signal.side,-1);},
                SniperStyle::Confirmed=>assert!(!signal.ready),
                _=>unreachable!(),
            }
        }
        c.trading.sniper_style=SniperStyle::Confirmed;let mut e=Engine::default();
        for i in 0..5 {
            let signal=feed(&mut e,&mut t,&c,11000+i*10,2000.+i as f64*0.04);
            assert_eq!(signal.ready,i==4);
        }
        c.trading.sniper_style=SniperStyle::Rebound;
        assert!(!feed(&mut e,&mut t,&c,11060,2000.20).ready,"style switch must warm up again");
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
    #[test]
    fn exactly_five_and_ten_gold_pips_even_when_quote_point_is_tenth_of_dollar() {
        for (pips,want) in [(5.,0.50),(10.,1.00)] {
            let (mut t,mut c)=fixture();
            t.point=Some(0.10);
            t.tick_size=Some(0.10);
            t.tick_value_profit=Some(10.);
            t.tick_value_loss=Some(10.);
            c.risk.commission_per_lot=0.;
            c.risk.slippage_ticks=0.;
            c.trading.sniper_target_points=pips*c.trading.pip_points;
            let mut engine=Engine::default();
            let mut result=None;
            for (ts,bid) in [(10000,2000.),(10010,2000.2),(10020,2000.4)] {
                t.bid=Some(bid);t.ask=Some(bid+0.10);t.quote_time_ms=Some(ts);
                result=Some(engine.evaluate(&t,&c,ts));
            }
            let signal=result.unwrap();
            assert!(signal.ready,"{}",signal.reason);
            assert!((signal.target_distance-want).abs()<1e-8);
            assert_eq!(signal.coordination.unwrap()["selection"]["sniper_target_pips"],pips);
        }
    }

    #[test]
    fn zero_commission_zero_slippage_uses_only_observed_spread_and_never_adds_it_to_tp() {
        let (mut t,mut c) = fixture();
        c.risk.commission_per_lot = 0.;
        c.risk.slippage_ticks = 0.;
        c.trading.sniper_target_points = 70.;
        let mut engine = Engine::default();
        for (ts,bid) in [(10000,2000.),(10010,2000.04),(10020,2000.08)] {
            let signal=feed(&mut engine,&mut t,&c,ts,bid);
            if ts==10020 {
                assert!(signal.ready);
                assert!((signal.target_distance-0.7).abs()<1e-8);
            }
        }
        // The real broker spread is still a cost and should remain visible
        // even when commission and hypothetical slippage are both zero.
        let p=trading::costs(&t,&c,0.01,10020).unwrap();
        assert_eq!(p.commission,0.);
        assert_eq!(p.slippage_money,0.);
        assert!(p.spread_money>0.);
        assert!((p.break_even_from_entry_points).abs()<1e-8);
    }

    #[test]
    fn user_defined_pip_targets_are_fixed_or_bounded_and_cost_checked() {
        let (mut t,mut c)=fixture();
        c.trading.pip_points=10.;
        c.trading.sniper_target_points=70.;
        let mut engine=Engine::default();
        for (ts,p) in [(10000,2000.),(10010,2000.04),(10020,2000.08)] {
            let s=feed(&mut engine,&mut t,&c,ts,p);
            if ts==10020 {
                assert!(s.ready);
                assert_eq!(s.coordination.as_ref().unwrap()["selection"]["sniper_target_pips"],7.);
                assert!((s.target_distance-0.7).abs()<1e-8);
            }
        }
        c.trading.sniper_target_mode=SniperTargetMode::Adaptive;
        c.trading.sniper_min_target_pips=3.;
        c.trading.sniper_max_target_pips=9.;
        engine.reset_quotes();
        feed(&mut engine,&mut t,&c,10030,2000.);
        feed(&mut engine,&mut t,&c,10040,2000.04);
        let s=feed(&mut engine,&mut t,&c,10050,2000.08);
        assert!(s.ready);
        let v=&s.coordination.as_ref().unwrap()["selection"];
        let pip=v["sniper_target_pips"].as_f64().unwrap();
        assert!((3. ..=9.).contains(&pip));
        let arm=v["sniper_target_arm"].as_u64().unwrap() as usize;
        assert!((pip-target_candidates(&c)[arm]).abs()<1e-8);
        let mut live=t.clone();live.demo=false;live.account_trade_mode=Some(2);
        engine.reset_quotes();
        feed(&mut engine,&mut live,&c,10060,2000.);
        feed(&mut engine,&mut live,&c,10070,2000.04);
        let s=feed(&mut engine,&mut live,&c,10080,2000.08);
        assert!(s.ready);
        assert_eq!(s.coordination.as_ref().unwrap()["selection"]["sniper_target_mode"],"fixed");
        assert_eq!(s.coordination.as_ref().unwrap()["selection"]["sniper_target_pips"],7.);
    }
    #[test]
    fn learned_target_arm_uses_completed_demo_reward_and_never_blocks_exploration() {
        let (mut t,mut c)=fixture();
        c.trading.sniper_target_mode=SniperTargetMode::Adaptive;
        c.trading.sniper_min_target_pips=2.;c.trading.sniper_max_target_pips=11.;
        let def=definition(&t,&c);
        let mut engine=Engine::default();
        let mut stats=std::array::from_fn(|_| Policy::default());
        for _ in 0..20 { stats[0].learn(-0.7,-2.);stats[3].learn(0.7,2.); }
        engine.target_models.insert((def,"directional_normal".into(),"impulse".into()),stats);
        let mut saw_best=false;
        for n in 0..90 {
            engine.reset_quotes();
            let first=10000+n*100;
            feed(&mut engine,&mut t,&c,first,2000.);
            feed(&mut engine,&mut t,&c,first+10,2000.04);
            let s=feed(&mut engine,&mut t,&c,first+20,2000.08);
            assert!(s.ready);
            let v=&s.coordination.as_ref().unwrap()["selection"];
            assert!(v["sniper_target_pips"].as_f64().unwrap()>=2.);
            if v["sniper_policy"]=="impulse" && v["sniper_target_exploring"]==false {
                saw_best |= v["sniper_target_arm"]==3;
            }
        }
        assert!(saw_best);
    }

}
