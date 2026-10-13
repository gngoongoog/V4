//! Independent deterministic Trend / Market-Regime research brains.
//! Evidence values are normalized feature strength, never win probabilities.
use super::Candle;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrendFrame {
    pub timeframe: String,
    pub direction: i8,
    pub evidence: f64,
    pub separation_atr: f64,
    pub slope_atr: f64,
    pub efficiency: f64,
}
impl Default for TrendFrame {
    fn default() -> Self {
        Self {
            timeframe: String::new(),
            direction: 0,
            evidence: 0.0,
            separation_atr: 0.0,
            slope_atr: 0.0,
            efficiency: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrendBrain {
    pub version: String,
    pub direction: i8,
    pub evidence: f64,
    pub aligned: bool,
    pub frames: Vec<TrendFrame>,
    pub reason: String,
}
impl Default for TrendBrain {
    fn default() -> Self {
        Self {
            version: "trend-v1".into(),
            direction: 0,
            evidence: 0.0,
            aligned: false,
            frames: vec![],
            reason: "Trend Brain unavailable".into(),
        }
    }
}
impl TrendBrain {
    pub fn text(&self) -> String {
        let direction = match self.direction {
            1 => "UP / صعود",
            -1 => "DOWN / هبوط",
            _ => "NEUTRAL / غير محسوم",
        };
        let frames = self
            .frames
            .iter()
            .map(|f| {
                format!(
                    "{}={} evidence {:.2} | sep {:.3} ATR | slope {:.3} ATR | ER {:.2}",
                    f.timeframe,
                    match f.direction {
                        1 => "UP",
                        -1 => "DOWN",
                        _ => "NEUTRAL",
                    },
                    f.evidence,
                    f.separation_atr,
                    f.slope_atr,
                    f.efficiency
                )
            })
            .collect::<Vec<_>>()
            .join(" ; ");
        format!(
            "{}: {} | evidence {:.2} | aligned {}\n{}\n{}",
            self.version, direction, self.evidence, self.aligned, self.reason, frames
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StructureFrame {
    pub timeframe: String,
    pub direction: i8,
    pub evidence: f64,
    pub breakout: bool,
    pub prior_high: f64,
    pub prior_low: f64,
    pub close_location: f64,
    pub range_width_atr: f64,
}
impl Default for StructureFrame {
    fn default() -> Self {
        Self {
            timeframe: String::new(),
            direction: 0,
            evidence: 0.0,
            breakout: false,
            prior_high: 0.0,
            prior_low: 0.0,
            close_location: 0.0,
            range_width_atr: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StructureBrain {
    pub version: String,
    pub direction: i8,
    pub evidence: f64,
    pub consensus: bool,
    pub frames: Vec<StructureFrame>,
    pub reason: String,
}
impl Default for StructureBrain {
    fn default() -> Self {
        Self {
            version: "structure-v1".into(),
            direction: 0,
            evidence: 0.0,
            consensus: false,
            frames: vec![],
            reason: "Structure Brain unavailable".into(),
        }
    }
}
impl StructureBrain {
    pub fn text(&self) -> String {
        let frames = self
            .frames
            .iter()
            .map(|f| {
                format!(
                    "{}={} evidence {:.2} | breakout {} | prior {:.5}..{:.5} | location {:.2} | width {:.2} ATR",
                    f.timeframe,
                    match f.direction {
                        1 => "UP",
                        -1 => "DOWN",
                        _ => "INSIDE",
                    },
                    f.evidence,
                    f.breakout,
                    f.prior_low,
                    f.prior_high,
                    f.close_location,
                    f.range_width_atr
                )
            })
            .collect::<Vec<_>>()
            .join(" ; ");
        format!(
            "{}: direction {} | evidence {:.2} | consensus {}\n{}\n{}",
            self.version, self.direction, self.evidence, self.consensus, self.reason, frames
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VolatilityBrain {
    pub version: String,
    pub state: String,
    pub atr: f64,
    pub baseline_true_range: f64,
    pub ratio: f64,
    pub shock_ratio: f64,
    pub shock: bool,
    pub evidence: f64,
    pub reason: String,
}
impl Default for VolatilityBrain {
    fn default() -> Self {
        Self {
            version: "volatility-v1".into(),
            state: "UNKNOWN".into(),
            atr: 0.0,
            baseline_true_range: 0.0,
            ratio: 1.0,
            shock_ratio: 0.0,
            shock: false,
            evidence: 0.0,
            reason: "Volatility Brain unavailable".into(),
        }
    }
}
impl VolatilityBrain {
    pub fn text(&self) -> String {
        format!(
            "{}: {} | ATR {:.5} | baseline TR {:.5} | ratio {:.2}x | last-bar shock {:.2}x ({}) | evidence {:.2}\n{}",
            self.version,
            self.state,
            self.atr,
            self.baseline_true_range,
            self.ratio,
            self.shock_ratio,
            self.shock,
            self.evidence,
            self.reason
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MetaBrainV2 {
    pub version: String,
    pub action: String,
    pub direction: i8,
    pub evidence: f64,
    pub eligible: bool,
    pub momentum_atr: f64,
    pub gates: Vec<String>,
    pub reason: String,
}
impl Default for MetaBrainV2 {
    fn default() -> Self {
        Self {
            version: "meta-v2-candidate".into(),
            action: "WAIT".into(),
            direction: 0,
            evidence: 0.0,
            eligible: false,
            momentum_atr: 0.0,
            gates: vec!["Candidate unavailable".into()],
            reason: "Meta v2 candidate is observational only".into(),
        }
    }
}
impl MetaBrainV2 {
    pub fn text(&self) -> String {
        format!(
            "{}: {} | direction {} | evidence {:.2} | eligible {} | momentum {:.2} ATR\n{}\nGates: {}",
            self.version,
            self.action,
            self.direction,
            self.evidence,
            self.eligible,
            self.momentum_atr,
            self.reason,
            if self.gates.is_empty() { "none".into() } else { self.gates.join(" | ") }
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RegimeBrain {
    pub version: String,
    pub label: String,
    pub direction: i8,
    pub trend_evidence: f64,
    pub efficiency: f64,
    pub volatility_ratio: f64,
    pub volatility: String,
    pub tradable_trend: bool,
    pub reason: String,
    #[serde(default)]
    pub structure_brain: StructureBrain,
    #[serde(default)]
    pub volatility_brain: VolatilityBrain,
    #[serde(default)]
    pub meta_v2: MetaBrainV2,
}
impl Default for RegimeBrain {
    fn default() -> Self {
        Self {
            version: "regime-v1".into(),
            label: "UNKNOWN".into(),
            direction: 0,
            trend_evidence: 0.0,
            efficiency: 0.0,
            volatility_ratio: 1.0,
            volatility: "UNKNOWN".into(),
            tradable_trend: false,
            reason: "Regime Brain unavailable".into(),
            structure_brain: StructureBrain::default(),
            volatility_brain: VolatilityBrain::default(),
            meta_v2: MetaBrainV2::default(),
        }
    }
}
impl RegimeBrain {
    pub fn text(&self) -> String {
        format!(
            "{}: {} | trend evidence {:.2} | ER {:.2} | volatility {} ({:.2}x) | trend-eligible {}\n{}\n\nSTRUCTURE BRAIN\n{}\n\nVOLATILITY BRAIN\n{}\n\nMETA BRAIN V2 CANDIDATE\n{}",
            self.version,
            self.label,
            self.trend_evidence,
            self.efficiency,
            self.volatility,
            self.volatility_ratio,
            self.tradable_trend,
            self.reason,
            self.structure_brain.text(),
            self.volatility_brain.text(),
            self.meta_v2.text()
        )
    }
}

fn ema(series: &[Candle], period: usize) -> Option<f64> {
    let first = series.first()?.close;
    let alpha = 2.0 / (period as f64 + 1.0);
    Some(
        series
            .iter()
            .skip(1)
            .fold(first, |value, candle| alpha * candle.close + (1.0 - alpha) * value),
    )
}
fn true_range(previous: &Candle, current: &Candle) -> f64 {
    (current.high - current.low)
        .max((current.high - previous.close).abs())
        .max((current.low - previous.close).abs())
}
fn mean_true_range(series: &[Candle]) -> Option<f64> {
    if series.len() < 2 {
        return None;
    }
    let total: f64 = series.windows(2).map(|w| true_range(&w[0], &w[1])).sum();
    Some(total / (series.len() - 1) as f64)
}
fn atr_last(series: &[Candle], period: usize) -> Option<f64> {
    if period == 0 || series.len() <= period {
        return None;
    }
    mean_true_range(&series[series.len() - period - 1..])
}
fn efficiency(series: &[Candle], lookback: usize) -> f64 {
    if lookback == 0 || series.len() <= lookback {
        return 0.0;
    }
    let slice = &series[series.len() - lookback - 1..];
    let displacement = (slice.last().unwrap().close - slice.first().unwrap().close).abs();
    let path: f64 = slice
        .windows(2)
        .map(|w| (w[1].close - w[0].close).abs())
        .sum();
    if path <= f64::EPSILON {
        0.0
    } else {
        (displacement / path).clamp(0.0, 1.0)
    }
}
fn sign(value: f64, threshold: f64) -> i8 {
    if value > threshold {
        1
    } else if value < -threshold {
        -1
    } else {
        0
    }
}
fn frame(series: &[Candle], timeframe: &str) -> TrendFrame {
    if series.len() < 100 {
        return TrendFrame {
            timeframe: timeframe.into(),
            ..Default::default()
        };
    }
    let atr = atr_last(series, 14).unwrap_or(0.0);
    if !atr.is_finite() || atr <= 0.0 {
        return TrendFrame {
            timeframe: timeframe.into(),
            ..Default::default()
        };
    }
    let ema20 = ema(series, 20).unwrap();
    let ema50 = ema(series, 50).unwrap();
    let previous_ema20 = ema(&series[..series.len() - 3], 20).unwrap();
    let separation = (ema20 - ema50) / atr;
    let slope = (ema20 - previous_ema20) / atr;
    let separation_direction = sign(separation, 0.15);
    let slope_direction = sign(slope, 0.03);
    let direction = if separation_direction != 0 && separation_direction == slope_direction {
        separation_direction
    } else {
        0
    };
    let er = efficiency(series, 20);
    let separation_score = (separation.abs() / 0.75).clamp(0.0, 1.0);
    let slope_score = (slope.abs() / 0.15).clamp(0.0, 1.0);
    let evidence = if direction == 0 {
        (0.30 * separation_score + 0.20 * slope_score + 0.10 * er).clamp(0.0, 0.49)
    } else {
        (0.50 * separation_score + 0.25 * slope_score + 0.25 * er).clamp(0.0, 1.0)
    };
    TrendFrame {
        timeframe: timeframe.into(),
        direction,
        evidence,
        separation_atr: separation,
        slope_atr: slope,
        efficiency: er,
    }
}
fn structure_frame(series: &[Candle], timeframe: &str) -> StructureFrame {
    if series.len() < 40 {
        return StructureFrame {
            timeframe: timeframe.into(),
            ..Default::default()
        };
    }
    let atr = atr_last(series, 14).unwrap_or(0.0);
    if !atr.is_finite() || atr <= 0.0 {
        return StructureFrame {
            timeframe: timeframe.into(),
            ..Default::default()
        };
    }
    let count = series.len();
    let prior = &series[count - 21..count - 1];
    let prior_high = prior.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
    let prior_low = prior.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let close = series[count - 1].close;
    let buffer = 0.05 * atr;
    let direction = if close > prior_high + buffer {
        1
    } else if close < prior_low - buffer {
        -1
    } else {
        0
    };
    let half = ((prior_high - prior_low) * 0.5).max(f64::EPSILON);
    let midpoint = (prior_high + prior_low) * 0.5;
    let close_location = ((close - midpoint) / half).clamp(-2.0, 2.0);
    let range_width_atr = ((prior_high - prior_low) / atr).clamp(0.0, 100.0);
    let distance_atr = if direction == 1 {
        (close - prior_high) / atr
    } else if direction == -1 {
        (prior_low - close) / atr
    } else {
        0.0
    };
    let evidence = if direction == 0 {
        (close_location.abs() * 0.20).clamp(0.0, 0.40)
    } else {
        (0.55 + distance_atr.max(0.0) * 0.45).clamp(0.55, 1.0)
    };
    StructureFrame {
        timeframe: timeframe.into(),
        direction,
        evidence,
        breakout: direction != 0,
        prior_high,
        prior_low,
        close_location,
        range_width_atr,
    }
}
fn assess_structure(m1: &[Candle], m5: &[Candle], m15: &[Candle]) -> StructureBrain {
    let frames = vec![
        structure_frame(m1, "M1"),
        structure_frame(m5, "M5"),
        structure_frame(m15, "M15"),
    ];
    let up = frames.iter().filter(|frame| frame.direction == 1).count();
    let down = frames.iter().filter(|frame| frame.direction == -1).count();
    let direction = if up >= 2 && down == 0 {
        1
    } else if down >= 2 && up == 0 {
        -1
    } else {
        0
    };
    let consensus = direction != 0;
    let weights = [0.20, 0.35, 0.45];
    let evidence = frames
        .iter()
        .zip(weights)
        .map(|(frame, weight)| frame.evidence * weight)
        .sum::<f64>()
        .clamp(0.0, 1.0);
    StructureBrain {
        version: "structure-v1".into(),
        direction,
        evidence,
        consensus,
        reason: if consensus {
            "At least two completed-bar timeframes break the prior 20-bar range in the same direction with no opposite breakout".into()
        } else if up > 0 || down > 0 {
            "Breakout evidence is isolated or conflicts across timeframes; Structure Brain abstains".into()
        } else {
            "All closes remain inside their prior 20-bar ranges; location is reported as context only".into()
        },
        frames,
    }
}
fn assess_volatility(m1: &[Candle]) -> VolatilityBrain {
    if m1.len() < 80 {
        return VolatilityBrain::default();
    }
    let current = atr_last(m1, 14).unwrap_or(0.0);
    let end = m1.len() - 14;
    let start = end.saturating_sub(51);
    let baseline = mean_true_range(&m1[start..end]).unwrap_or(0.0);
    if current <= 0.0 || baseline <= 0.0 || !current.is_finite() || !baseline.is_finite() {
        return VolatilityBrain::default();
    }
    let ratio = (current / baseline).clamp(0.05, 20.0);
    let state = if ratio >= 1.35 {
        "EXPANDING"
    } else if ratio <= 0.75 {
        "COMPRESSED"
    } else {
        "NORMAL"
    };
    let last_range = true_range(&m1[m1.len() - 2], &m1[m1.len() - 1]);
    let shock_ratio = (last_range / current).clamp(0.0, 20.0);
    let shock = shock_ratio >= 2.50;
    let evidence = if ratio >= 1.0 {
        ((ratio - 1.0) / 1.5).clamp(0.0, 1.0)
    } else {
        ((1.0 - ratio) / 0.75).clamp(0.0, 1.0)
    };
    VolatilityBrain {
        version: "volatility-v1".into(),
        state: state.into(),
        atr: current,
        baseline_true_range: baseline,
        ratio,
        shock_ratio,
        shock,
        evidence,
        reason: if shock {
            "Latest completed M1 true range is at least 2.5x current ATR; this is reported as a shock, not a trading signal".into()
        } else {
            "ATR14 is compared with an earlier completed-bar true-range baseline; state is observational only".into()
        },
    }
}
fn volatility_ratio(m1: &[Candle]) -> f64 {
    assess_volatility(m1).ratio
}
fn assess_meta_v2(
    trend: &TrendBrain,
    structure: &StructureBrain,
    volatility: &VolatilityBrain,
    tradable_trend: bool,
    m1: &[Candle],
) -> MetaBrainV2 {
    let mut gates = Vec::new();
    let direction = trend.direction;
    let atr = atr_last(m1, 14).unwrap_or(0.0);
    let momentum_atr = if m1.len() >= 6 && atr > 0.0 {
        (m1[m1.len() - 1].close - m1[m1.len() - 6].close) / atr
    } else {
        0.0
    };
    let momentum_direction = sign(momentum_atr, 0.50);
    let momentum_evidence = (momentum_atr.abs() / 2.0).clamp(0.0, 1.0);

    if !trend.aligned || direction == 0 {
        gates.push("Trend Brain is not aligned".into());
    }
    if !tradable_trend {
        gates.push("Regime is not trend-eligible".into());
    }
    if volatility.shock {
        gates.push("Volatility shock veto".into());
    }
    if structure.direction != 0 && direction != 0 && structure.direction != direction {
        gates.push("Structure breakout opposes trend".into());
    }
    if direction != 0 && momentum_direction == -direction {
        gates.push("Momentum opposes trend".into());
    }
    let structure_confirms = direction != 0 && structure.direction == direction;
    let momentum_confirms = direction != 0 && momentum_direction == direction;
    if direction != 0 && !structure_confirms && !momentum_confirms {
        gates.push("No independent structure or momentum confirmation".into());
    }
    if volatility.state == "COMPRESSED" && !structure_confirms {
        gates.push("Compressed volatility requires structure breakout confirmation".into());
    }

    // Regime efficiency is a gate, not another score component, because it overlaps Trend evidence.
    // This avoids pretending correlated features are independent confirmations.
    let evidence = (0.60 * trend.evidence
        + if structure_confirms { 0.25 * structure.evidence } else { 0.0 }
        + if momentum_confirms { 0.15 * momentum_evidence } else { 0.0 })
        .clamp(0.0, 1.0);
    if direction != 0 && evidence < 0.55 {
        gates.push("Combined independent evidence below 0.55".into());
    }
    let eligible = gates.is_empty() && direction != 0;
    let action = if eligible {
        if direction == 1 { "BUY" } else { "SELL" }
    } else {
        "WAIT"
    };
    MetaBrainV2 {
        version: "meta-v2-candidate".into(),
        action: action.into(),
        direction: if eligible { direction } else { 0 },
        evidence,
        eligible,
        momentum_atr,
        reason: if eligible {
            "Trend and regime eligibility agree with independent structure or momentum confirmation; candidate only, not execution authority".into()
        } else {
            "Candidate abstains until all deterministic gates pass; legacy Meta remains execution source for Shadow".into()
        },
        gates,
    }
}

/// Uses only the supplied completed bars. This module does not inspect quotes or future bars.
pub(super) fn assess(m1: &[Candle], m5: &[Candle], m15: &[Candle]) -> (TrendBrain, RegimeBrain) {
    let frames = vec![frame(m1, "M1"), frame(m5, "M5"), frame(m15, "M15")];
    let aligned_direction = frames.first().map(|f| f.direction).unwrap_or(0);
    let aligned = aligned_direction != 0 && frames.iter().all(|f| f.direction == aligned_direction);
    let weights = [0.20, 0.35, 0.45];
    let evidence = frames
        .iter()
        .zip(weights)
        .map(|(frame, weight)| frame.evidence * weight)
        .sum::<f64>()
        .clamp(0.0, 1.0);
    let efficiency = frames
        .iter()
        .zip(weights)
        .map(|(frame, weight)| frame.efficiency * weight)
        .sum::<f64>()
        .clamp(0.0, 1.0);
    let direction = if aligned { aligned_direction } else { 0 };
    let trend = TrendBrain {
        version: "trend-v1".into(),
        direction,
        evidence,
        aligned,
        frames,
        reason: if aligned {
            "EMA20/EMA50 direction and three-bar EMA20 slope agree on M1/M5/M15; evidence also includes 20-bar efficiency".into()
        } else {
            "Timeframes are not fully aligned; Trend Brain abstains from a directional verdict".into()
        },
    };

    let structure_brain = assess_structure(m1, m5, m15);
    let volatility_brain = assess_volatility(m1);
    let ratio = volatility_ratio(m1);
    let volatility = if ratio >= 1.35 {
        "EXPANDING"
    } else if ratio <= 0.75 {
        "COMPRESSED"
    } else {
        "NORMAL"
    };
    let strong_trend = aligned && evidence >= 0.45 && efficiency >= 0.30;
    let range_like = !aligned && efficiency <= 0.25;
    let (label, regime_direction, tradable_trend, reason) = if strong_trend {
        (
            if direction == 1 { "TREND_UP" } else { "TREND_DOWN" },
            direction,
            true,
            "All three trend frames align with sufficient normalized evidence and directional efficiency",
        )
    } else if range_like {
        (
            "RANGE",
            0,
            false,
            "Directional efficiency is low and the three trend frames do not align",
        )
    } else {
        (
            "TRANSITION",
            0,
            false,
            "Directional structure is mixed or not strong enough for a trend-regime verdict",
        )
    };
    let meta_v2 = assess_meta_v2(
        &trend,
        &structure_brain,
        &volatility_brain,
        tradable_trend,
        m1,
    );
    let regime = RegimeBrain {
        version: "regime-v1".into(),
        label: format!("{label} / {volatility}"),
        direction: regime_direction,
        trend_evidence: evidence,
        efficiency,
        volatility_ratio: ratio,
        volatility: volatility.into(),
        tradable_trend,
        reason: reason.into(),
        structure_brain,
        volatility_brain,
        meta_v2,
    };
    (trend, regime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(seconds: u64, slope: f64, range: f64) -> Vec<Candle> {
        (0..180)
            .map(|i| {
                let close = 2000.0 + slope * i as f64;
                Candle {
                    time: 1_700_000_000 + i * seconds,
                    open: close - slope * 0.25,
                    high: close + range,
                    low: close - range,
                    close,
                    tick_volume: 100,
                }
            })
            .collect()
    }

    #[test]
    fn aligned_up_and_down_trends_are_explicit() {
        let up = series(60, 0.30, 0.60);
        let up5 = series(300, 0.30, 0.60);
        let up15 = series(900, 0.30, 0.60);
        let (trend, regime) = assess(&up, &up5, &up15);
        assert_eq!(trend.direction, 1);
        assert!(trend.aligned && trend.evidence > 0.45);
        assert_eq!(regime.direction, 1);
        assert!(regime.label.starts_with("TREND_UP"));
        assert!(regime.tradable_trend);
        assert_eq!(regime.meta_v2.action, "BUY");
        assert!(regime.meta_v2.eligible);

        let down = series(60, -0.30, 0.60);
        let down5 = series(300, -0.30, 0.60);
        let down15 = series(900, -0.30, 0.60);
        let (trend, regime) = assess(&down, &down5, &down15);
        assert_eq!(trend.direction, -1);
        assert!(regime.label.starts_with("TREND_DOWN"));
        assert_eq!(regime.meta_v2.action, "SELL");
    }

    #[test]
    fn mixed_high_efficiency_frames_are_transition_not_fake_consensus() {
        let up = series(60, 0.30, 0.60);
        let up5 = series(300, 0.30, 0.60);
        let down15 = series(900, -0.30, 0.60);
        let (trend, regime) = assess(&up, &up5, &down15);
        assert_eq!(trend.direction, 0);
        assert!(!trend.aligned);
        assert!(regime.label.starts_with("TRANSITION"));
        assert!(!regime.tradable_trend);
        assert_eq!(regime.meta_v2.action, "WAIT");
    }

    #[test]
    fn flat_market_is_range_and_expanding_range_is_labeled() {
        let flat = series(60, 0.0, 0.60);
        let flat5 = series(300, 0.0, 0.60);
        let flat15 = series(900, 0.0, 0.60);
        let (_, regime) = assess(&flat, &flat5, &flat15);
        assert!(regime.label.starts_with("RANGE"));
        assert_eq!(regime.meta_v2.action, "WAIT");

        let mut expanding = series(60, 0.30, 0.40);
        for candle in expanding.iter_mut().rev().take(15) {
            candle.high = candle.close + 2.0;
            candle.low = candle.close - 2.0;
        }
        let (_, regime) = assess(
            &expanding,
            &series(300, 0.30, 0.40),
            &series(900, 0.30, 0.40),
        );
        assert_eq!(regime.volatility, "EXPANDING");
        assert!(regime.volatility_ratio >= 1.35);
        assert_eq!(regime.volatility_brain.state, "EXPANDING");
    }

    #[test]
    fn structure_requires_multi_timeframe_consensus() {
        let mut m1 = series(60, 0.0, 0.60);
        let mut m5 = series(300, 0.0, 0.60);
        let m15 = series(900, 0.0, 0.60);
        for data in [&mut m1, &mut m5] {
            let n = data.len();
            let prior_high = data[n - 21..n - 1]
                .iter()
                .map(|c| c.high)
                .fold(f64::NEG_INFINITY, f64::max);
            data[n - 1].close = prior_high + 1.0;
            data[n - 1].high = prior_high + 1.2;
            data[n - 1].low = data[n - 1].open.min(data[n - 1].close) - 0.2;
        }
        let (_, regime) = assess(&m1, &m5, &m15);
        assert_eq!(regime.structure_brain.version, "structure-v1");
        assert_eq!(regime.structure_brain.direction, 1);
        assert!(regime.structure_brain.consensus);

        let (_, isolated) = assess(&m1, &series(300, 0.0, 0.60), &m15);
        assert_eq!(isolated.structure_brain.direction, 0);
        assert!(!isolated.structure_brain.consensus);
    }

    #[test]
    fn volatility_shock_is_observational_and_vetoes_meta_candidate() {
        let mut m1 = series(60, 0.30, 0.40);
        let n = m1.len();
        let close = m1[n - 1].close;
        m1[n - 1].high = close + 5.0;
        m1[n - 1].low = close - 5.0;
        let (_, regime) = assess(
            &m1,
            &series(300, 0.30, 0.40),
            &series(900, 0.30, 0.40),
        );
        assert_eq!(regime.volatility_brain.version, "volatility-v1");
        assert!(regime.volatility_brain.shock);
        assert!(regime.volatility_brain.shock_ratio >= 2.5);
        assert_eq!(regime.meta_v2.action, "WAIT");
        assert!(regime.meta_v2.gates.iter().any(|gate| gate == "Volatility shock veto"));
        assert!(regime.text().contains("STRUCTURE BRAIN"));
        assert!(regime.text().contains("VOLATILITY BRAIN"));
        assert!(regime.text().contains("META BRAIN V2 CANDIDATE"));
    }

    #[test]
    fn meta_candidate_never_counts_regime_efficiency_as_independent_vote() {
        let m1 = series(60, 0.30, 0.60);
        let m5 = series(300, 0.30, 0.60);
        let m15 = series(900, 0.30, 0.60);
        let (trend, regime) = assess(&m1, &m5, &m15);
        assert!(regime.meta_v2.evidence <= trend.evidence * 0.60 + 0.40 + 1e-12);
        assert!(regime.meta_v2.reason.contains("candidate only"));
    }
}