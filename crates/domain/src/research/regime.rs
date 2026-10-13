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
        }
    }
}
impl RegimeBrain {
    pub fn text(&self) -> String {
        format!(
            "{}: {} | trend evidence {:.2} | ER {:.2} | volatility {} ({:.2}x) | trend-eligible {}\n{}",
            self.version,
            self.label,
            self.trend_evidence,
            self.efficiency,
            self.volatility,
            self.volatility_ratio,
            self.tradable_trend,
            self.reason
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
fn volatility_ratio(m1: &[Candle]) -> f64 {
    if m1.len() < 80 {
        return 1.0;
    }
    let current = atr_last(m1, 14).unwrap_or(0.0);
    let end = m1.len() - 14;
    let start = end.saturating_sub(51);
    let baseline = mean_true_range(&m1[start..end]).unwrap_or(0.0);
    if current <= 0.0 || baseline <= 0.0 || !current.is_finite() || !baseline.is_finite() {
        1.0
    } else {
        (current / baseline).clamp(0.05, 20.0)
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

        let down = series(60, -0.30, 0.60);
        let down5 = series(300, -0.30, 0.60);
        let down15 = series(900, -0.30, 0.60);
        let (trend, regime) = assess(&down, &down5, &down15);
        assert_eq!(trend.direction, -1);
        assert!(regime.label.starts_with("TREND_DOWN"));
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
    }

    #[test]
    fn flat_market_is_range_and_expanding_range_is_labeled() {
        let flat = series(60, 0.0, 0.60);
        let flat5 = series(300, 0.0, 0.60);
        let flat15 = series(900, 0.0, 0.60);
        let (_, regime) = assess(&flat, &flat5, &flat15);
        assert!(regime.label.starts_with("RANGE"));

        let mut expanding = series(60, 0.30, 0.40);
        for candle in expanding.iter_mut().rev().take(15) {
            candle.high = candle.close + 2.0;
            candle.low = candle.close - 2.0;
        }
        let (_, regime) = assess(&expanding, &series(300, 0.30, 0.40), &series(900, 0.30, 0.40));
        assert_eq!(regime.volatility, "EXPANDING");
        assert!(regime.volatility_ratio >= 1.35);
    }
}
