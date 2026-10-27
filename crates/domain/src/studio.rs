//! Deterministic strategy synthesis; validation selects and the purged test never selects.
use crate::{backtest::Contract, trading, wyckoff, Candle, RiskConfig};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
pub const TIMEFRAMES: [&str; 21] = [
    "H1", "H4", "D1", "M1", "M5", "M15", "M2", "M3", "M4", "M6", "M10", "M12", "M20", "M30", "H2",
    "H3", "H6", "H8", "H12", "W1", "MN1",
];
pub fn seconds(tf: &str) -> Option<u64> {
    Some(match tf {
        "M1" => 60,
        "M2" => 120,
        "M3" => 180,
        "M4" => 240,
        "M5" => 300,
        "M6" => 360,
        "M10" => 600,
        "M12" => 720,
        "M15" => 900,
        "M20" => 1200,
        "M30" => 1800,
        "H1" => 3600,
        "H2" => 7200,
        "H3" => 10800,
        "H4" => 14400,
        "H6" => 21600,
        "H8" => 28800,
        "H12" => 43200,
        "D1" => 86400,
        "W1" => 604800,
        "MN1" => 2_678_400,
        _ => return None,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameBar {
    #[serde(flatten)]
    pub candle: Candle,
    pub spread_points: u32,
    #[serde(default)]
    pub real_volume: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub login: u64,
    pub server: String,
    pub symbol: String,
    pub currency: String,
    pub timeframe: String,
    pub captured_ms: u64,
    pub current_open: u64,
    pub contract: Contract,
    pub bars: Vec<FrameBar>,
    #[serde(default)]
    pub error: String,
}
impl Frame {
    pub fn context(&self) -> String {
        serde_json::to_string(&(&self.server, self.login, &self.symbol, &self.currency)).unwrap()
    }
    pub fn validate(&self, now: u64) -> Result<(), String> {
        self.contract.validate()?;
        if !TIMEFRAMES.contains(&self.timeframe.as_str())
            || self.login == 0
            || self.server.is_empty()
            || self.symbol.is_empty()
            || self.currency.is_empty()
        {
            return Err("Invalid frame identity".into());
        }
        if self.captured_ms > now + 2000
            || now.saturating_sub(self.captured_ms) > 120_000
            || self.error.len() > 1000
        {
            return Err("Stale/future frame capture".into());
        }
        if !self.error.is_empty() {
            if !self.bars.is_empty() {
                return Err("Error frame cannot carry bars".into());
            }
            return Ok(());
        }
        if self.bars.is_empty()
            || self.bars.len() > 600
            || self.current_open == 0
            || self.current_open > now / 1000 + 2
        {
            return Err("Invalid closure boundary/count".into());
        }
        let duration = seconds(&self.timeframe).unwrap();
        if self.bars.iter().any(|b| {
            !b.candle.valid()
                || b.candle.high > 1e12
                || b.spread_points > 100_000_000
                || b.candle.time >= self.current_open
                || (self.timeframe != "MN1"
                    && b.candle.time.saturating_add(duration) > self.current_open)
        }) || self
            .bars
            .windows(2)
            .any(|w| w[0].candle.time >= w[1].candle.time)
        {
            return Err("Invalid/incomplete/unsorted frame bars".into());
        }
        Ok(())
    }
    pub fn live(&self, now: u64) -> bool {
        self.error.is_empty()
            && now.saturating_sub(self.captured_ms) < 120_000
            && self.current_open <= now / 1000
            && now / 1000 - self.current_open < seconds(&self.timeframe).unwrap_or(0) + 20
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Genome {
    pub family: String,
    pub lookback: usize,
    pub stop_atr: f64,
    pub reward: f64,
}
impl Genome {
    pub fn valid(&self)->bool {
        matches!(self.family.as_str(),"breakout"|"rejection"|"trend_pullback"|"wyckoff_test")
            && (8..=64).contains(&self.lookback)
            && self.stop_atr.is_finite() && (0.5..=3.0).contains(&self.stop_atr)
            && self.reward.is_finite() && (0.5..=4.0).contains(&self.reward)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub trades: usize,
    pub wins: usize,
    pub net_r: f64,
    pub max_drawdown_r: f64,
    pub costs_per_lot: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trial {
    pub id:usize,
    pub generation:usize,
    pub parent:Option<usize>,
    pub origin:String,
    pub genome:Genome,
    pub training:Metrics,
    pub training_score:Option<f64>,
    pub validation:Option<Metrics>,
    pub validation_score:Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evolution {
    pub schema:String,
    pub generations:usize,
    pub selected_trial:usize,
    pub best_training_trial:usize,
    pub worst_training_trial:usize,
    pub trials:Vec<Trial>,
    pub parent_run_id:Option<String>,
    pub rejection_reasons:Vec<String>,
    pub automatic_promotion:bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabReport {
    pub algorithm: String,
    pub context: String,
    pub timeframe: String,
    pub captured_ms: u64,
    pub first_bar: u64,
    pub last_bar: u64,
    pub train_end: u64,
    pub validation_start: u64,
    pub validation_end: u64,
    pub test_start: u64,
    pub test_end: u64,
    pub purge_bars: usize,
    pub trials: usize,
    pub selected: Genome,
    pub training: Metrics,
    pub validation: Metrics,
    pub test: Metrics,
    pub eligible_for_review: bool,
    pub note: String,
    pub risk: RiskConfig,
    pub contract: Contract,
    #[serde(default)]
    pub evolution:Option<Evolution>,
}
impl LabReport {
    /// A review candidate, never broker approval or proof of profitability.
    pub fn reviewable(&self)->bool {
        self.selected.valid() && self.eligible_for_review
            && self.train_end < self.validation_start && self.validation_end < self.test_start
            && self.test_start <= self.test_end && self.test_end == self.last_bar
            && self.purge_bars >= 24 && self.risk.validate().is_ok() && self.contract.validate().is_ok()
            && rejection_reasons(&self.training,&self.validation,&self.test).is_empty()
    }
}
pub fn signal(g: &Genome, b: &[FrameBar]) -> i8 {
    if b.len() < 100 || !g.valid() {
        return 0;
    }
    if g.family == "wyckoff_test" {
        return wyckoff::analyze(b).side;
    }
    let n = b.len();
    let c = &b[n - 1].candle;
    let prior = &b[n - 1 - g.lookback..n - 1];
    let high = prior
        .iter()
        .map(|v| v.candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = prior
        .iter()
        .map(|v| v.candle.low)
        .fold(f64::INFINITY, f64::min);
    match g.family.as_str() {
        "breakout" => {
            if c.close > high {
                1
            } else if c.close < low {
                -1
            } else {
                0
            }
        }
        "rejection" => {
            if c.low < low && c.close > low && c.close > c.open {
                1
            } else if c.high > high && c.close < high && c.close < c.open {
                -1
            } else {
                0
            }
        }
        _ => {
            let candles: Vec<_> = b[n - 100..].iter().map(|v| v.candle.clone()).collect();
            let side = trading::direction(&candles);
            let avg = prior.iter().map(|v| v.candle.close).sum::<f64>() / prior.len() as f64;
            if side == 1 && c.low <= avg && c.close > avg {
                1
            } else if side == -1 && c.high >= avg && c.close < avg {
                -1
            } else {
                0
            }
        }
    }
}
/// Next-bar executable fills; conservative stop-first ties, no swaps, normalized 1-lot risk.
fn simulate(g: &Genome, f: &Frame, r: &RiskConfig, start: usize, end: usize) -> Metrics {
    let mut m = Metrics::default();
    let mut i = start.max(100);
    let mut peak: f64 = 0.;
    let tick = f.contract.tick_size;
    let slip = tick * r.slippage_ticks;
    while i + 1 < end {
        let side = signal(g, &f.bars[..i]);
        if side == 0 {
            i += 1;
            continue;
        }
        let candles: Vec<_> = f.bars[i - 15..i].iter().map(|b| b.candle.clone()).collect();
        let distance = trading::volatility(&candles) * g.stop_atr;
        if distance <= tick {
            i += 1;
            continue;
        }
        let b = &f.bars[i];
        let spread = b.spread_points as f64 * f.contract.point;
        let entry = b.candle.open + if side == 1 { spread + slip } else { -slip };
        let stop = entry - side as f64 * distance;
        let target = entry + side as f64 * distance * g.reward;
        let initial_risk = (distance + slip) / tick
            * f.contract.tick_loss.max(f.contract.tick_profit)
            + r.commission_per_lot;
        let mut exit = entry;
        let mut last = i;
        let mut spread_exit = spread;
        for j in i..(i + 24).min(end) {
            let b = &f.bars[j];
            let add = if side == -1 {
                b.spread_points as f64 * f.contract.point
            } else {
                0.
            };
            let o = b.candle.open + add;
            let h = b.candle.high + add;
            let l = b.candle.low + add;
            last = j;
            spread_exit = b.spread_points as f64 * f.contract.point;
            let sl = if side == 1 { l <= stop } else { h >= stop };
            let tp = if side == 1 { h >= target } else { l <= target };
            exit = if sl {
                if side == 1 {
                    o.min(stop)
                } else {
                    o.max(stop)
                }
            } else if tp {
                target
            } else {
                b.candle.close + add
            };
            if sl || tp {
                break;
            }
        }
        exit -= side as f64 * slip;
        let change = (exit - entry) * side as f64;
        let net = change / tick
            * if change >= 0. {
                f.contract.tick_profit
            } else {
                f.contract.tick_loss
            }
            - r.commission_per_lot;
        m.trades += 1;
        m.wins += usize::from(net > 0.);
        m.net_r += net / initial_risk;
        peak = peak.max(m.net_r);
        m.max_drawdown_r = m.max_drawdown_r.max(peak - m.net_r);
        m.costs_per_lot += (if side == 1 { spread } else { spread_exit }) / tick
            * f.contract.tick_loss.max(f.contract.tick_profit)
            + 2. * r.slippage_ticks * f.contract.tick_loss.max(f.contract.tick_profit)
            + r.commission_per_lot;
        i = last + 1;
    }
    m
}
fn score(m: &Metrics) -> f64 {
    if m.trades < 4 {
        f64::NEG_INFINITY
    } else {
        m.net_r - 0.75 * m.max_drawdown_r
    }
}
fn rejection_reasons(train:&Metrics,validation:&Metrics,test:&Metrics)->Vec<String>{
    let mut reasons=Vec::new();
    for (name,m,min) in [("training",train,4),("validation",validation,4),("final_test",test,10)] {
        if m.trades<min {reasons.push(format!("{name}_insufficient_trades"));}
        if !m.net_r.is_finite() || !m.max_drawdown_r.is_finite()
            || m.max_drawdown_r<0. || m.wins>m.trades || !m.costs_per_lot.is_finite() || m.costs_per_lot<0. {
            reasons.push(format!("{name}_invalid_metrics"));
        }else if m.net_r<=0. {reasons.push(format!("{name}_nonpositive_after_costs"));}
    }
    reasons
}
fn mutations(g:&Genome)->Vec<Genome>{
    let mut children=Vec::new();
    // Bounded, explainable local changes; no search on final-test results.
    for direction in [-1.0,1.0] {
        if g.family!="wyckoff_test" {
            let mut v=g.clone();v.lookback=(g.lookback as i32+direction as i32*6).clamp(8,64) as usize;
            children.push(v);
        }
        let mut v=g.clone();v.stop_atr=(g.stop_atr+direction*0.25).clamp(0.5,3.0);children.push(v);
        let mut v=g.clone();v.reward=(g.reward+direction*0.25).clamp(0.5,4.0);children.push(v);
    }
    children.into_iter().filter(|v|v.valid() && v!=g).collect()
}
fn trial(g:Genome,f:&Frame,r:&RiskConfig,end:usize,id:usize,generation:usize,parent:Option<usize>,origin:&str)->Trial{
    let training=simulate(&g,f,r,100,end);
    let fitness=score(&training);
    Trial{id,generation,parent,origin:origin.into(),genome:g,training,
        training_score:fitness.is_finite().then_some(fitness),validation:None,validation_score:None}
}
pub fn learn(f:&Frame,r:&RiskConfig,cancel:&AtomicBool)->Result<LabReport,String>{
    learn_seeded(f,r,cancel,None)
}
pub fn learn_seeded(f:&Frame,r:&RiskConfig,cancel:&AtomicBool,seed:Option<&Genome>)->Result<LabReport,String>{
    f.validate(f.captured_ms)?;r.validate()?;
    if !f.error.is_empty() || f.bars.len()<400 {
        return Err("Need 400 actual closed bars for purged train/validation/test".into());
    }
    if seed.is_some_and(|g|!g.valid()){return Err("Invalid previous strategy genome".into())}
    let n=f.bars.len();let a=n*6/10;let b=n*8/10;let purge=24;
    let mut trials=Vec::new();
    for family in ["breakout","rejection","trend_pullback","wyckoff_test"] {
        for lookback in [12,24,48] { for stop_atr in [1.,1.5] { for reward in [1.5,2.] {
            if family=="wyckoff_test" && lookback!=12 {continue}
            if cancel.load(Ordering::Relaxed){return Err("Cancelled".into())}
            trials.push(trial(Genome{family:family.into(),lookback,stop_atr,reward},f,r,a,trials.len(),0,None,"base_grid"));
        }}}
    }
    if let Some(g)=seed.filter(|g|!trials.iter().any(|t|t.genome==**g)){
        trials.push(trial(g.clone(),f,r,a,trials.len(),0,None,"previous_run"));
    }
    // At most 41 initial + 2*4*6 children = 89 evaluations per frame/run.
    // Only training scores determine mutations; validation is consulted once.
    for generation in 1..=2 {
        let mut parents:Vec<usize>=(0..trials.len()).filter(|i|trials[*i].training_score.is_some()).collect();
        parents.sort_by(|a,b|score(&trials[*b].training).total_cmp(&score(&trials[*a].training)));
        parents.truncate(4);
        let before=trials.len();
        for parent in parents {
            for g in mutations(&trials[parent].genome) {
                if cancel.load(Ordering::Relaxed){return Err("Cancelled".into())}
                if !trials.iter().any(|t|t.genome==g){
                    trials.push(trial(g,f,r,a,trials.len(),generation,Some(parent),"training_mutation"));
                }
            }
        }
        if trials.len()==before {break}
    }
    let mut ranked:Vec<usize>=(0..trials.len()).collect();
    ranked.sort_by(|a,b|score(&trials[*b].training).total_cmp(&score(&trials[*a].training)));
    let best=ranked[0];let worst=*ranked.last().unwrap();
    ranked.truncate(8);
    for id in &ranked {
        if cancel.load(Ordering::Relaxed){return Err("Cancelled".into())}
        let m=simulate(&trials[*id].genome,f,r,a+purge,b);
        let fitness=score(&m);
        trials[*id].validation=Some(m);
        trials[*id].validation_score=fitness.is_finite().then_some(fitness);
    }
    ranked.sort_by(|a,b|score(trials[*b].validation.as_ref().unwrap()).total_cmp(&score(trials[*a].validation.as_ref().unwrap())));
    let selected_trial=ranked[0];
    let chosen=&trials[selected_trial];let selected=chosen.genome.clone();
    let training=chosen.training.clone();let validation=chosen.validation.clone().unwrap();
    if cancel.load(Ordering::Relaxed){return Err("Cancelled".into())}
    // Exactly ONE selected strategy touches the purged final test.
    let test=simulate(&selected,f,r,b+purge,n);
    let reasons=rejection_reasons(&training,&validation,&test);
    let eligible=reasons.is_empty();
    let count=trials.len();let generations=trials.iter().map(|t|t.generation).max().unwrap()+1;
    Ok(LabReport{algorithm:"rule-evolution-purged-v2".into(),context:f.context(),timeframe:f.timeframe.clone(),captured_ms:f.captured_ms,first_bar:f.bars[0].candle.time,last_bar:f.bars[n-1].candle.time,train_end:f.bars[a-1].candle.time,validation_start:f.bars[a+purge].candle.time,validation_end:f.bars[b-1].candle.time,test_start:f.bars[b+purge].candle.time,test_end:f.bars[n-1].candle.time,purge_bars:purge,trials:count,selected,training,validation,test,eligible_for_review:eligible,
        note:"Bounded strategy evolution: training-only mutation; validation selects once; final test evaluates once and never selects. Research OHLC simulation, NOT broker results. Fixed contract; spread, slippage, commission; no swaps. No automatic REAL promotion or guarantee of profitability.".into(),risk:r.clone(),contract:f.contract.clone(),
        evolution:Some(Evolution{schema:"strategy-evolution-v1".into(),generations,selected_trial,best_training_trial:best,worst_training_trial:worst,trials,parent_run_id:None,rejection_reasons:reasons,automatic_promotion:false})})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> Frame {
        Frame {
            login: 1,
            server: "S".into(),
            symbol: "X".into(),
            currency: "USD".into(),
            timeframe: "M1".into(),
            captured_ms: 60_000_000,
            current_open: 60000,
            contract: Contract {
                point: 0.01,
                tick_size: 0.01,
                tick_loss: 1.,
                tick_profit: 1.,
                volume_min: 0.01,
                volume_step: 0.01,
                volume_max: 100.,
            },
            bars: (0..600)
                .map(|i| {
                    let p = 100. + (i as f64 * 0.2).sin() * 2. + i as f64 * 0.01;
                    FrameBar {
                        candle: Candle {
                            time: 24000 + i * 60,
                            open: p,
                            high: p + 0.4,
                            low: p - 0.4,
                            close: p + 0.1,
                            tick_volume: 100,
                        },
                        spread_points: 2,
                        real_volume: 0,
                    }
                })
                .collect(),
            error: String::new(),
        }
    }
    #[test]
    fn final_test_never_selects() {
        let mut f = frame();
        let c = AtomicBool::new(false);
        let r = learn(&f, &RiskConfig::default(), &c).unwrap();
        for b in &mut f.bars[504..] {
            b.candle.high += 50.;
        }
        let changed = learn(&f, &RiskConfig::default(), &c).unwrap();
        assert_eq!(r.selected, changed.selected);
        assert_eq!(r.training.net_r, changed.training.net_r);
        assert!(r.test_start > r.validation_end + 24 * 60);
        assert!((40..=89).contains(&r.trials));
        assert_eq!(serde_json::to_value(&r.evolution.as_ref().unwrap().trials).unwrap(),serde_json::to_value(&changed.evolution.as_ref().unwrap().trials).unwrap());
    }
    #[test]
    fn reject_current_and_cancel() {
        let mut f = frame();
        f.bars.last_mut().unwrap().candle.time = f.current_open;
        assert!(f.validate(f.captured_ms).is_err());
        assert!(learn(&frame(), &RiskConfig::default(), &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn evolution_is_bounded_reproducible_and_retains_elites() {
        let f=frame(); let r=learn(&f,&RiskConfig::default(),&AtomicBool::new(false)).unwrap();
        let e=r.evolution.as_ref().unwrap();
        assert_eq!(e.trials.len(),r.trials);
        assert!(e.generations<=3); assert!(!e.automatic_promotion);
        assert_eq!(e.trials.iter().filter(|t|t.validation.is_some()).count(),8);
        for (i,t) in e.trials.iter().enumerate() {
            assert_eq!(t.id,i); assert!(t.genome.valid());
            assert!(!e.trials[..i].iter().any(|p|p.genome==t.genome));
            if let Some(parent)=t.parent {
                assert!(parent<i); assert!(e.trials[parent].generation<t.generation);
                assert!(mutations(&e.trials[parent].genome).contains(&t.genome));
            }
        }
        let best=&e.trials[e.best_training_trial];
        assert!(e.trials.iter().take(40).all(|t|score(&best.training)>=score(&t.training)));
        assert_eq!(e.trials[e.selected_trial].genome,r.selected);
        let raw=serde_json::to_string(&r).unwrap();
        let restored:LabReport=serde_json::from_str(&raw).unwrap();
        assert_eq!(raw,serde_json::to_string(&restored).unwrap());
        assert_eq!(raw,serde_json::to_string(&learn(&f,&RiskConfig::default(),&AtomicBool::new(false)).unwrap()).unwrap());
    }
    #[test]
    fn seed_is_retested_and_final_test_cannot_drive_mutations() {
        let mut f=frame(); let c=AtomicBool::new(false);
        let seed=Genome{family:"rejection".into(),lookback:18,stop_atr:1.25,reward:1.75};
        let r=learn_seeded(&f,&RiskConfig::default(),&c,Some(&seed)).unwrap();
        let trials=&r.evolution.as_ref().unwrap().trials;
        assert!(trials.iter().any(|t|t.genome==seed && t.origin=="previous_run"));
        for b in &mut f.bars[504..] {b.candle.low-=10.;}
        let changed=learn_seeded(&f,&RiskConfig::default(),&c,Some(&seed)).unwrap();
        assert_eq!(serde_json::to_value(trials).unwrap(),serde_json::to_value(&changed.evolution.unwrap().trials).unwrap());
    }
    #[test]
    fn malformed_genomes_never_panic_or_fall_back_to_another_family() {
        let f=frame();
        let base=Genome{family:"rejection".into(),lookback:12,stop_atr:1.,reward:2.};
        let bad=[Genome{lookback:usize::MAX,..base.clone()},Genome{lookback:0,..base.clone()},
            Genome{family:"unknown".into(),..base.clone()},Genome{stop_atr:f64::NAN,..base.clone()},
            Genome{reward:f64::INFINITY,..base}];
        for g in bad {assert!(!g.valid());assert_eq!(signal(&g,&f.bars),0);
            assert!(learn_seeded(&f,&RiskConfig::default(),&AtomicBool::new(false),Some(&g)).is_err());}
    }
    #[test]
    fn sparse_or_losing_results_are_explained_and_old_reports_remain_readable() {
        let empty=Metrics::default();
        let reasons=rejection_reasons(&empty,&empty,&empty);
        assert!(reasons.contains(&"final_test_insufficient_trades".into()));
        assert!(reasons.contains(&"validation_nonpositive_after_costs".into()));
        let mut r=learn(&frame(),&RiskConfig::default(),&AtomicBool::new(false)).unwrap();
        r.eligible_for_review=true; r.test=empty;
        assert!(!r.reviewable(),"Stored eligibility alone cannot approve a candidate");
        let mut old=serde_json::to_value(&r).unwrap();old.as_object_mut().unwrap().remove("evolution");
        assert!(serde_json::from_value::<LabReport>(old).unwrap().evolution.is_none());
    }

}
