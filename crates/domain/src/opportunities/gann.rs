use super::{atr, sign, Candle, Swing};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GannFrame {
    pub timeframe: String,
    pub available: bool,
    pub anchor_time: u64,
    pub anchor_price: f64,
    pub anchor_side: i8,
    pub bars_since_anchor: usize,
    pub range_low: f64,
    pub range_high: f64,
    pub nearest_eighth: u8,
    pub nearest_level: f64,
    pub distance_atr: f64,
    /// Price displacement from the latest confirmed swing, normalized as ATR per bar.
    /// This avoids pretending that a chart pixel angle is an invariant market quantity.
    pub atr_per_bar: f64,
    pub direction: i8,
    pub strength: f64,
    pub note: String,
}

/// Gann-inspired geometry using only completed bars and confirmed pivots.
///
/// Classical Gann angles depend on an arbitrary chart price/time scale. BrainTrade therefore
/// does not use screen angles. It uses a transparent analogue: the latest confirmed swing is
/// the time/price anchor, speed is measured in ATR per bar, and the most recent confirmed
/// swing range is divided into eighths. The result is observational evidence only.
pub fn assess(series: &[Candle], timeframe: &str, swings: &[Swing]) -> GannFrame {
    let mut out = GannFrame {
        timeframe: timeframe.into(),
        note: "هندسة مستوحاة من Gann: نطاق swing مؤكد مقسّم إلى 1/8 وميل سعري بوحدة ATR لكل شمعة؛ ليست زاوية شاشة ولا نسبة نجاح.".into(),
        ..Default::default()
    };
    if series.len() < 40 || swings.len() < 2 {
        return out;
    }
    let Some(last) = series.last() else { return out; };
    let a = atr(series);
    if !a.is_finite() || a <= 0.0 {
        return out;
    }

    let mut confirmed = swings
        .iter()
        .filter(|s| s.confirmed_at <= last.time)
        .collect::<Vec<_>>();
    confirmed.sort_by_key(|s| s.time);
    let Some(anchor) = confirmed.last().copied() else { return out; };
    let Some(high) = confirmed.iter().rev().find(|s| s.side == -1).copied() else { return out; };
    let Some(low) = confirmed.iter().rev().find(|s| s.side == 1).copied() else { return out; };
    let range_low = low.price.min(high.price);
    let range_high = low.price.max(high.price);
    let range = range_high - range_low;
    if !range.is_finite() || range < 0.5 * a {
        return out;
    }

    let index = series
        .iter()
        .position(|bar| bar.time == anchor.time)
        .unwrap_or(series.len().saturating_sub(1));
    let bars_since = series.len().saturating_sub(1 + index).max(1);
    let atr_per_bar = (last.close - anchor.price) / (a * bars_since as f64);

    let step = range / 8.0;
    let raw_index = ((last.close - range_low) / step).round().clamp(0.0, 8.0) as u8;
    let level = range_low + raw_index as f64 * step;
    let distance_atr = (last.close - level).abs() / a;

    // Direction requires both movement away from the latest confirmed pivot and non-trivial
    // normalized speed. A latest swing-low supports upward geometry; a swing-high supports down.
    let displacement_direction = sign(last.close - anchor.price, 0.15 * a);
    let speed_direction = sign(atr_per_bar, 0.035);
    let pivot_direction = anchor.side;
    let direction = if displacement_direction == speed_direction
        && speed_direction == pivot_direction
    {
        speed_direction
    } else {
        0
    };

    let speed_strength = (atr_per_bar.abs() / 0.20).min(1.0);
    let level_relevance = (1.0 - distance_atr / 0.60).clamp(0.0, 1.0);
    let range_maturity = (range / (6.0 * a)).clamp(0.0, 1.0);
    let strength = (0.50 * speed_strength + 0.30 * level_relevance + 0.20 * range_maturity)
        .clamp(0.0, 1.0);

    out.available = true;
    out.anchor_time = anchor.time;
    out.anchor_price = anchor.price;
    out.anchor_side = anchor.side;
    out.bars_since_anchor = bars_since;
    out.range_low = range_low;
    out.range_high = range_high;
    out.nearest_eighth = raw_index;
    out.nearest_level = level;
    out.distance_atr = distance_atr;
    out.atr_per_bar = atr_per_bar;
    out.direction = direction;
    out.strength = strength;
    out
}
