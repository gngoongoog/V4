//! Demo learning primitives. Shadow exits are quote observations, never broker fills.
use crate::execution::IntelligenceContext;
use serde::{Deserialize, Serialize};

pub fn context_key(c: &IntelligenceContext) -> String {
    let spread = if c.spread_to_atr < 0.05 {
        "tight"
    } else if c.spread_to_atr < 0.15 {
        "normal"
    } else {
        "wide"
    };
    format!("{}|{}|{}", c.regime, c.session, spread)
}

pub fn exploration_rate(samples: u64) -> f64 {
    (0.30 / (1.0 + samples as f64 / 20.0).sqrt()).clamp(0.05, 0.30)
}

/// Stable across restarts and pulses. Salt separates exploration and action draws.
pub fn draw(seed: &str, salt: u64) -> f64 {
    let hash = seed.bytes().fold(0xcbf29ce484222325u64 ^ salt, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    let mut x = hash.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((x ^ (x >> 31)) >> 11) as f64 / ((1u64 << 53) as f64)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitProfile {
    #[default]
    Baseline,
    Quick,
    Patient,
    Protective,
    Extended,
}
impl ExitProfile {
    pub const ALL: [Self; 5] = [Self::Baseline, Self::Quick, Self::Patient, Self::Protective, Self::Extended];
    pub fn index(self) -> usize {
        match self {
            Self::Baseline => 0,
            Self::Quick => 1,
            Self::Patient => 2,
            Self::Protective => 3,
            Self::Extended => 4,
        }
    }
    pub fn geometry(self, stop: f64, target: f64, hold: u64) -> (f64, f64, u64) {
        let (s, t, h) = match self {
            Self::Baseline => (1., 1., 1.),
            Self::Quick => (0.9, 0.75, 0.5),
            Self::Patient => (1., 1.25, 1.25),
            Self::Protective => (0.65, 1., 0.75),
            Self::Extended => (1., 1.75, 1.5),
        };
        (stop * s, target * t, ((hold as f64 * h) as u64).max(1000))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShadowOutcome {
    pub exit_ms: u64,
    pub net_per_lot: f64,
    pub reward_r: f64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trial {
    pub opened_ms: u64,
    pub side: i8,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub hold_ms: u64,
    #[serde(default)]
    pub deadline_ms: Option<u64>,
    pub tick_size: f64,
    pub profit_value: f64,
    pub loss_value: f64,
    pub commission_per_lot: f64,
    pub exit_slippage: f64,
    pub last_quote_ms: u64,
    pub samples: u64,
    pub max_gap_ms: u64,
    #[serde(default)]
    pub last_observed_ms: u64,
    #[serde(default)]
    pub max_observation_gap_ms: u64,
    #[serde(default)]
    pub exclusion_reason: String,
    pub usable: bool,
    pub mfe: f64,
    pub mae: f64,
    pub outcomes: Vec<Option<ShadowOutcome>>,
}
impl Trial {
    pub fn complete(&self) -> bool {
        !self.outcomes.is_empty() && self.outcomes.iter().all(Option::is_some)
    }
    /// Bid is the executable long exit; ask is the executable short exit.
    /// A missing interval invalidates policy comparison instead of inventing a path.
    pub fn observe(&mut self, bid: f64, ask: f64, quote_ms: u64) {
        self.observe_at(bid, ask, quote_ms, quote_ms, true);
    }
    /// Receipt heartbeats distinguish a quiet quote from an unobserved interval.
    /// Stale prices acknowledge connectivity but cannot resolve a hypothetical exit.
    pub fn observe_at(&mut self, bid: f64, ask: f64, quote_ms: u64, now: u64, fresh: bool) {
        if !bid.is_finite()
            || !ask.is_finite()
            || bid <= 0.
            || ask < bid
            || quote_ms < self.last_quote_ms
            || quote_ms < self.opened_ms
        {
            return;
        }
        if now <= self.last_observed_ms {
            return;
        }
        let observation_gap = now.saturating_sub(self.last_observed_ms.max(self.opened_ms));
        self.max_observation_gap_ms = self.max_observation_gap_ms.max(observation_gap);
        if observation_gap > 2000 {
            self.usable = false;
            if self.exclusion_reason.is_empty() {
                self.exclusion_reason = "observation_gap".into();
            }
        }
        self.last_observed_ms = now;
        if !fresh || quote_ms <= self.last_quote_ms {
            return;
        }
        let gap = quote_ms.saturating_sub(self.last_quote_ms.max(self.opened_ms));
        self.max_gap_ms = self.max_gap_ms.max(gap);
        self.last_quote_ms = quote_ms;
        self.samples += 1;
        let price = if self.side > 0 { bid } else { ask };
        let movement = (price - self.entry) * self.side as f64;
        self.mfe = self.mfe.max(movement);
        self.mae = self.mae.max(-movement);
        for profile in ExitProfile::ALL.into_iter().take(self.outcomes.len()) {
            if self.outcomes[profile.index()].is_some() {
                continue;
            }
            let (stop, target, hold) = profile.geometry(self.stop, self.target, self.hold_ms);
            let reason = if movement <= -stop {
                "stop_quote"
            } else if movement >= target {
                "target_quote"
            } else if quote_ms >= self.deadline_ms.unwrap_or(self.opened_ms.saturating_add(hold)) {
                "time_quote"
            } else {
                continue;
            };
            // Use observed executable price, including overshoot, not an ideal level fill.
            let change = movement - self.exit_slippage;
            let value = if change >= 0. {
                self.profit_value
            } else {
                self.loss_value
            };
            let net = change / self.tick_size * value - self.commission_per_lot;
            let risk = self.stop / self.tick_size * self.loss_value;
            self.outcomes[profile.index()] = Some(ShadowOutcome {
                exit_ms: quote_ms,
                net_per_lot: net,
                reward_r: (net / risk).clamp(-5., 5.),
                reason: reason.into(),
            });
        }
    }
    pub fn paired_rewards(&self) -> Option<Vec<f64>> {
        if !self.usable || !self.complete() || self.samples < 2 {
            return None;
        }
        Some(self.outcomes.iter().map(|o| o.as_ref().unwrap().reward_r).collect())
    }
}

/// Prospective quote experiment at a queued opportunity, never a delayed broker order.
/// All three entries share a fixed deadline and original risk. Receipt time schedules
/// delays; broker quote timestamps only deduplicate quotes. No interpolation is used.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryTiming {
    pub anchor_ms: u64,
    pub deadline_ms: u64,
    pub last_received_ms: u64,
    pub last_quote_ms: u64,
    pub template: Trial,
    pub legs: Vec<Option<Trial>>,
    pub usable: bool,
    pub exclusion_reason: String,
}
impl EntryTiming {
    pub const DELAYS: [u64; 3] = [0, 250, 1000];
    pub fn new(mut template: Trial, bid: f64, ask: f64, quote_ms: u64) -> Self {
        let anchor = template.opened_ms;
        let deadline = anchor.saturating_add(template.hold_ms);
        template.outcomes = vec![None];
        template.deadline_ms = Some(deadline);
        let mut result = Self { anchor_ms: anchor, deadline_ms: deadline,
            last_received_ms: anchor, last_quote_ms: quote_ms, template,
            legs: vec![None; 3], usable: true, exclusion_reason: String::new() };
        if !bid.is_finite() || !ask.is_finite() || bid <= 0. || ask < bid {
            result.exclude("invalid_quote");
        } else if deadline.saturating_sub(anchor) <= 1100 {
            result.exclude("timing_horizon");
        } else { result.enter(0, bid, ask, anchor); }
        result
    }
    pub fn exclude(&mut self, reason: &str) {
        self.usable = false;
        if self.exclusion_reason.is_empty() { self.exclusion_reason = reason.into(); }
    }
    fn enter(&mut self, index: usize, bid: f64, ask: f64, now: u64) {
        let mut t = self.template.clone();
        t.opened_ms = now;
        t.last_observed_ms = now;
        t.last_quote_ms = now;
        t.entry = if t.side > 0 { ask + t.exit_slippage } else { bid - t.exit_slippage };
        self.legs[index] = Some(t);
    }
    pub fn complete(&self) -> bool {
        !self.usable || self.legs.iter().all(|t| t.as_ref().is_some_and(Trial::complete))
    }
    pub fn observe(&mut self, bid: f64, ask: f64, quote_ms: u64, now: u64, fresh: bool) {
        if self.complete() || now <= self.last_received_ms || !bid.is_finite()
            || !ask.is_finite() || bid <= 0. || ask < bid || quote_ms < self.last_quote_ms { return; }
        if now.saturating_sub(self.last_received_ms) > 2000 { self.exclude("observation_gap"); return; }
        self.last_received_ms = now;
        if !fresh || quote_ms <= self.last_quote_ms {
            for t in self.legs.iter_mut().flatten().filter(|t| !t.complete()) {
                t.observe_at(bid, ask, t.last_quote_ms, now, false);
            }
            return;
        }
        self.last_quote_ms = quote_ms;
        for i in 0..3 {
            if self.legs[i].is_none() {
                let due = self.anchor_ms.saturating_add(Self::DELAYS[i]);
                if now < due { continue; }
                if now > due.saturating_add(100) { self.exclude("timing_resolution"); return; }
                self.enter(i, bid, ask, now);
            } else if let Some(t) = &mut self.legs[i] {
                if t.complete() { continue; }
                // Receipt time is the common clock for this hypothetical experiment.
                t.observe_at(bid, ask, now, now, true);
            }
        }
    }
    pub fn rewards(&self) -> Option<[f64; 3]> {
        if !self.usable || !self.complete() { return None; }
        let rewards: Vec<_> = self.legs.iter().map(|t| t.as_ref()?.paired_rewards().map(|v| v[0])).collect::<Option<_>>()?;
        Some([rewards[0], rewards[1], rewards[2]])
    }
}

/// A challenger is frozen after training, then judged only on newly opened trades.
/// Promotion is a conservative evidence heuristic, not a statistical profit guarantee.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExitStudy {
    pub active: ExitProfile,
    pub challenger: ExitProfile,
    pub phase: String,
    pub cutoff_ms: u64,
    pub samples: u64,
    #[serde(default = "legacy_sums")]
    pub sums: Vec<f64>,
    pub gain_sum: f64,
    pub gain_squares: f64,
    pub baseline_sum: f64,
    pub candidate_sum: f64,
    pub total_pairs: u64,
    pub promotions: u64,
    pub rollbacks: u64,
    pub last_gain_r: f64,
    pub last_decision: String,
    #[serde(default)]
    pub reviewed_losses: u64,
    #[serde(default)]
    pub loss_patterns: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub experiment_reason: String,
}
fn legacy_sums() -> Vec<f64> { vec![0.; 3] }
impl Default for ExitStudy {
    fn default() -> Self {
        Self { active: ExitProfile::Baseline, challenger: ExitProfile::Baseline,
            phase: String::new(), cutoff_ms: 0, samples: 0, sums: legacy_sums(),
            gain_sum: 0., gain_squares: 0., baseline_sum: 0., candidate_sum: 0.,
            total_pairs: 0, promotions: 0, rollbacks: 0, last_gain_r: 0.,
            last_decision: String::new(), reviewed_losses: 0,
            loss_patterns: Default::default(), experiment_reason: String::new() }
    }
}
impl ExitStudy {
    pub fn expanded() -> Self { Self { sums: vec![0.; 5], ..Self::default() } }
    pub fn record_loss(&mut self, patterns: &[String]) {
        self.reviewed_losses += 1;
        for pattern in patterns {
            *self.loss_patterns.entry(pattern.clone()).or_default() += 1;
        }
    }
    fn reset_window(&mut self, now: u64) {
        self.cutoff_ms = now;
        self.samples = 0;
        self.sums.fill(0.);
        self.gain_sum = 0.;
        self.gain_squares = 0.;
        self.baseline_sum = 0.;
        self.candidate_sum = 0.;
    }
    pub fn observe(&mut self, opened_ms: u64, now: u64, rewards: impl AsRef<[f64]>) -> bool {
        let rewards = rewards.as_ref();
        if rewards.len() != self.sums.len() || !(3..=5).contains(&rewards.len())
            || self.active.index() >= rewards.len() || self.challenger.index() >= rewards.len()
            || rewards.iter().any(|v| !v.is_finite()) || opened_ms <= self.cutoff_ms {
            return false;
        }
        self.total_pairs += 1;
        self.samples += 1;
        for (sum, reward) in self.sums.iter_mut().zip(rewards) {
            *sum += reward;
        }
        if self.phase.is_empty() {
            self.phase = "collecting".into();
        }
        if self.phase == "collecting" {
            if self.samples < 20 {
                return true;
            }
            let index = (1..self.sums.len())
                .max_by(|a, b| self.sums[*a].total_cmp(&self.sums[*b]))
                .unwrap();
            if self.sums[index] > self.sums[0] {
                self.challenger = ExitProfile::ALL[index];
                self.phase = "validating".into();
                self.last_decision = "challenger_frozen_for_future_entries".into();
                self.experiment_reason = match self
                    .loss_patterns
                    .iter()
                    .filter(|(_, n)| **n >= 3)
                    .max_by_key(|(_, n)| **n)
                    .map(|(p, _)| p.as_str())
                {
                    Some("cost_drag") => "compare_net_exits_after_repeated_cost_drag",
                    Some("profit_giveback") => "compare_exits_after_repeated_giveback",
                    Some("adverse_entry_path") => "compare_exits_entry_cause_unproven",
                    _ => "compare_exits_from_paired_evidence",
                }
                .into();
            } else {
                self.last_decision = "baseline_retained_after_training".into();
            }
            self.reset_window(now);
            return true;
        }
        let candidate = if self.phase == "monitoring" {
            self.active
        } else {
            self.challenger
        };
        let gain = rewards[candidate.index()] - rewards[0];
        self.gain_sum += gain;
        self.gain_squares += gain * gain;
        self.baseline_sum += rewards[0];
        self.candidate_sum += rewards[candidate.index()];
        if self.samples < 20 {
            return true;
        }
        let n = self.samples as f64;
        let mean = self.gain_sum / n;
        let variance = ((self.gain_squares - n * mean * mean) / (n - 1.)).max(0.);
        self.last_gain_r = mean;
        if self.phase == "validating" {
            if mean > 0. && mean - 2. * (variance / n).sqrt() > 0. && self.candidate_sum > 0. {
                self.active = self.challenger;
                self.promotions += 1;
                self.phase = "monitoring".into();
                self.last_decision = "promoted_on_later_paired_quotes".into();
            } else {
                self.phase = "collecting".into();
                self.last_decision = "validation_did_not_confirm_improvement".into();
            }
        } else if mean <= 0. || self.candidate_sum <= 0. {
            self.active = ExitProfile::Baseline;
            self.rollbacks += 1;
            self.phase = "collecting".into();
            self.last_decision = "returned_to_baseline_after_deterioration".into();
        } else {
            self.last_decision = "improvement_retained_in_monitoring".into();
        }
        self.reset_window(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn trial(side: i8) -> Trial {
        Trial {
            opened_ms: 1000,
            side,
            entry: 100.,
            stop: 1.,
            target: 2.,
            hold_ms: 4000,
            deadline_ms: None,
            tick_size: 0.1,
            profit_value: 1.,
            loss_value: 1.,
            commission_per_lot: 7.,
            exit_slippage: 0.1,
            last_quote_ms: 1000,
            samples: 0,
            max_gap_ms: 0,
            last_observed_ms: 1000,
            max_observation_gap_ms: 0,
            exclusion_reason: String::new(),
            usable: true,
            mfe: 0.,
            mae: 0.,
            outcomes: vec![None; 3],
        }
    }
    #[test]
    fn executable_sides_costs_and_gap_quality() {
        let mut long = trial(1);
        long.observe(101.6, 101.8, 2000);
        assert!(long.outcomes[1].is_some());
        assert!((long.outcomes[1].as_ref().unwrap().net_per_lot - 8.).abs() < 1e-8);
        assert!(long.outcomes[0].is_none());
        long.observe(102.6, 102.8, 3000);
        assert!(long.paired_rewards().is_some());
        assert_eq!(long.mfe, 2.5999999999999943);
        let mut short = trial(-1);
        short.observe(98., 98.8, 2000);
        assert!(short.outcomes[1].is_none());
        short.observe(96.8, 97., 5001);
        assert!(short.complete());
        assert!(short.paired_rewards().is_none());
    }
    #[test]
    fn future_only_validation_promotion_and_rollback() {
        let mut study = ExitStudy::default();
        for n in 1..=20 {
            study.observe(n, n, [0., 1., -1.]);
        }
        assert_eq!(study.phase, "validating");
        assert_eq!(study.active, ExitProfile::Baseline);
        study.observe(10, 21, [0., 100., 0.]);
        assert_eq!(study.samples, 0);
        for n in 21..=40 {
            study.observe(n, n, [0., 0.5, -1.]);
        }
        assert_eq!(study.active, ExitProfile::Quick);
        assert_eq!(study.promotions, 1);
        for n in 41..=60 {
            study.observe(n, n, [0., -0.5, 1.]);
        }
        assert_eq!(study.active, ExitProfile::Baseline);
        assert_eq!(study.rollbacks, 1);
    }
    #[test]
    fn exploration_is_stable_bounded_and_covers_actions() {
        assert_eq!(draw("same", 0), draw("same", 0));
        assert_eq!(exploration_rate(0), 0.30);
        assert_eq!(exploration_rate(1_000_000), 0.05);
        let count = (0..1000).filter(|i| draw(&i.to_string(), 0) < 0.3).count();
        assert!((250..350).contains(&count));
    }
    #[test]
    fn quiet_heartbeats_preserve_comparison_but_transport_gaps_do_not() {
        let mut t = trial(1);
        t.observe_at(100., 100.2, 1500, 1500, true);
        for now in [2500, 3500, 4500] {
            t.observe_at(100., 100.2, 1500, now, false);
        }
        t.observe_at(103., 103.2, 5000, 5000, true);
        assert!(t.paired_rewards().is_some());
        assert_eq!(t.max_gap_ms, 3500);
        let mut gap = trial(1);
        gap.observe_at(100., 100.2, 1500, 1500, true);
        gap.observe_at(103., 103.2, 5000, 5000, true);
        assert!(!gap.usable);
        assert_eq!(gap.exclusion_reason, "observation_gap");
    }
    #[test]
    fn expanded_exit_candidates_and_legacy_records_stay_separate() {
        let old = trial(1);
        let json = serde_json::to_string(&old).unwrap();
        let loaded: Trial = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.outcomes.len(), 3);
        let mut expanded = trial(1);
        expanded.outcomes = vec![None; 5];
        expanded.observe(99.3,99.5,1200);
        assert_eq!(expanded.outcomes[3].as_ref().unwrap().reason,"stop_quote");
        assert!(expanded.outcomes[0].is_none());
        expanded.observe(104.,104.2,1300);
        assert_eq!(expanded.paired_rewards().unwrap().len(),5);
        let mut study = ExitStudy::expanded();
        assert!(!study.observe(1,1,[0.,1.,2.]));
        for i in 1..=40 { study.observe(i,i,[0.,-1.,-1.,-1.,1.]); }
        assert_eq!(study.active,ExitProfile::Extended);
        let legacy: ExitStudy = serde_json::from_str(r#"{"sums":[0,0,0]}"#).unwrap();
        assert_eq!(legacy.sums.len(),3);
    }
    #[test]
    fn timing_uses_executable_sides_costs_and_a_common_deadline() {
        for side in [1,-1] {
            let mut template = trial(side);
            template.stop=10.; template.target=20.; template.hold_ms=2500;
            let mut timing = EntryTiming::new(template,100.,100.2,1000);
            assert!((timing.legs[0].as_ref().unwrap().entry - if side>0 {100.3} else {99.9}).abs()<1e-8);
            for now in [1100,1250,1350,2000,2100,2500,3500] {
                timing.observe(100.,100.2,now,now,true);
            }
            let rewards=timing.rewards().unwrap();
            assert!((rewards[0]+0.11).abs()<1e-8); // 0.2 spread + two 0.1 slippages + 7 commission / 100 risk.
            assert_eq!(rewards[0],rewards[1]); assert_eq!(rewards[1],rewards[2]);
            for leg in timing.legs.iter().flatten() { assert_eq!(leg.outcomes[0].as_ref().unwrap().exit_ms,3500); }
        }
    }
    #[test]
    fn timing_does_not_invent_delayed_quotes_or_reuse_stale_prices() {
        let mut timing=EntryTiming::new(trial(1),100.,100.2,1000);
        timing.observe(100.,100.2,1000,1250,false);
        assert!(timing.legs[1].is_none());
        timing.observe(101.,101.2,1400,1400,true);
        assert_eq!(timing.exclusion_reason,"timing_resolution");
        assert!(timing.rewards().is_none());
        let mut gap=EntryTiming::new(trial(1),100.,100.2,1000);
        gap.observe(101.,101.2,3101,3101,true);
        assert_eq!(gap.exclusion_reason,"observation_gap");
    }
    #[test]
    fn timing_quiet_heartbeats_keep_already_entered_legs_observed() {
        let mut template=trial(1); template.stop=10.; template.target=20.; template.hold_ms=5000;
        let mut timing=EntryTiming::new(template,100.,100.2,1000);
        for now in [1100,1250,1350,2000,2100] { timing.observe(100.,100.2,now,now,true); }
        for now in [3000,4000,5000] { timing.observe(100.,100.2,2100,now,false); }
        timing.observe(100.,100.2,6000,6000,true);
        assert!(timing.rewards().is_some());
    }

}
