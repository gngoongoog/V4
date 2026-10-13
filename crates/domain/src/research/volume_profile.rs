//! Completed-bar volume-profile research using broker tick volume only.
//! This is not exchange traded volume and not order-flow/DOM data.
use super::Candle;
use serde::{Deserialize, Serialize};

const BINS: usize = 24;
const LOOKBACK: usize = 120;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VolumeProfileBrain {
    pub version: String,
    pub source: String,
    pub available: bool,
    pub bars: usize,
    pub bins: usize,
    pub total_tick_volume: u64,
    pub poc: Option<f64>,
    pub value_low: Option<f64>,
    pub value_high: Option<f64>,
    pub relation: String,
    pub directional_context: i8,
    pub concentration: f64,
    pub entropy: f64,
    pub reason: String,
}
impl Default for VolumeProfileBrain {
    fn default() -> Self {
        Self {
            version: "volume-profile-v1".into(),
            source: "broker_tick_volume_proxy".into(),
            available: false,
            bars: 0,
            bins: BINS,
            total_tick_volume: 0,
            poc: None,
            value_low: None,
            value_high: None,
            relation: "UNKNOWN".into(),
            directional_context: 0,
            concentration: 0.0,
            entropy: 0.0,
            reason: "Volume Profile unavailable".into(),
        }
    }
}
impl VolumeProfileBrain {
    pub fn text(&self) -> String {
        let price = |value: Option<f64>| {
            value
                .map(|v| format!("{v:.5}"))
                .unwrap_or_else(|| "—".into())
        };
        format!(
            "{}: {} | available {} | {} bars / {} bins\nPOC {} | Value Area {}..{} | relation {} | context {}\nTick volume {} | POC concentration {:.2}% | normalized entropy {:.2}\n{}\nالمصدر tick volume من وسيط MT5؛ ليس حجم بورصة حقيقياً ولا DOM/Order Flow عالمياً.",
            self.version,
            self.source,
            self.available,
            self.bars,
            self.bins,
            price(self.poc),
            price(self.value_low),
            price(self.value_high),
            self.relation,
            self.directional_context,
            self.total_tick_volume,
            self.concentration * 100.0,
            self.entropy,
            self.reason,
        )
    }
}

fn normalized_entropy(weights: &[u64], total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let populated = weights.iter().filter(|&&value| value > 0).count();
    if populated <= 1 {
        return 0.0;
    }
    let entropy = weights
        .iter()
        .filter(|&&value| value > 0)
        .map(|&value| {
            let p = value as f64 / total as f64;
            -p * p.ln()
        })
        .sum::<f64>();
    (entropy / (populated as f64).ln()).clamp(0.0, 1.0)
}

/// Approximate volume-at-price by assigning each completed candle's broker tick count
/// to the bin containing its HLC3 typical price. No intrabar price/volume distribution
/// is fabricated.
pub(super) fn assess(series: &[Candle]) -> VolumeProfileBrain {
    if series.len() < 60 {
        return VolumeProfileBrain {
            bars: series.len(),
            reason: "Need at least 60 completed M1 bars for Volume Profile".into(),
            ..Default::default()
        };
    }
    let slice = &series[series.len().saturating_sub(LOOKBACK)..];
    let low = slice
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let high = slice
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    if !low.is_finite() || !high.is_finite() || high <= low {
        return VolumeProfileBrain {
            bars: slice.len(),
            reason: "Price range is unavailable or zero".into(),
            ..Default::default()
        };
    }
    let total = slice
        .iter()
        .fold(0u64, |value, candle| value.saturating_add(candle.tick_volume));
    if total == 0 {
        return VolumeProfileBrain {
            bars: slice.len(),
            reason: "Broker tick volume is zero; Volume Profile abstains".into(),
            ..Default::default()
        };
    }

    let width = (high - low) / BINS as f64;
    if !width.is_finite() || width <= 0.0 {
        return VolumeProfileBrain {
            bars: slice.len(),
            reason: "Volume Profile bin width is invalid".into(),
            ..Default::default()
        };
    }
    let mut bins = [0u64; BINS];
    for candle in slice {
        let typical = (candle.high + candle.low + candle.close) / 3.0;
        let raw = ((typical - low) / width).floor();
        let index = if !raw.is_finite() || raw <= 0.0 {
            0
        } else {
            (raw as usize).min(BINS - 1)
        };
        bins[index] = bins[index].saturating_add(candle.tick_volume);
    }

    let poc_index = bins
        .iter()
        .enumerate()
        .max_by_key(|(_, volume)| *volume)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let poc = low + (poc_index as f64 + 0.5) * width;

    let target = (total as f64 * 0.70).ceil() as u64;
    let mut accumulated = bins[poc_index];
    let mut left = poc_index;
    let mut right = poc_index;
    while accumulated < target && (left > 0 || right + 1 < BINS) {
        let left_volume = if left > 0 { bins[left - 1] } else { 0 };
        let right_volume = if right + 1 < BINS { bins[right + 1] } else { 0 };
        if right_volume > left_volume && right + 1 < BINS {
            right += 1;
            accumulated = accumulated.saturating_add(bins[right]);
        } else if left > 0 {
            left -= 1;
            accumulated = accumulated.saturating_add(bins[left]);
        } else if right + 1 < BINS {
            right += 1;
            accumulated = accumulated.saturating_add(bins[right]);
        } else {
            break;
        }
    }
    let value_low = low + left as f64 * width;
    let value_high = low + (right as f64 + 1.0) * width;
    let close = slice.last().unwrap().close;
    let (relation, directional_context) = if close > value_high {
        ("ABOVE_VALUE", 1)
    } else if close < value_low {
        ("BELOW_VALUE", -1)
    } else {
        ("INSIDE_VALUE", 0)
    };
    let concentration = bins[poc_index] as f64 / total as f64;
    let entropy = normalized_entropy(&bins, total);

    VolumeProfileBrain {
        version: "volume-profile-v1".into(),
        source: "broker_tick_volume_proxy".into(),
        available: true,
        bars: slice.len(),
        bins: BINS,
        total_tick_volume: total,
        poc: Some(poc),
        value_low: Some(value_low),
        value_high: Some(value_high),
        relation: relation.into(),
        directional_context,
        concentration,
        entropy,
        reason: "POC and 70% value area use HLC3 binning of completed M1 candles; directional context is descriptive and is not an entry signal".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(index: u64, price: f64, volume: u64) -> Candle {
        Candle {
            time: 1_700_000_000 + index * 60,
            open: price - 0.10,
            high: price + 0.30,
            low: price - 0.30,
            close: price,
            tick_volume: volume,
        }
    }

    #[test]
    fn concentrated_tick_volume_pulls_poc_to_its_price_cluster() {
        let mut series = Vec::new();
        for index in 0..120 {
            let price = 1900.0 + index as f64 * 0.02;
            let volume = if (50..70).contains(&index) { 2_000 } else { 50 };
            series.push(candle(index, price, volume));
        }
        let brain = assess(&series);
        assert!(brain.available);
        let poc = brain.poc.unwrap();
        assert!((1900.7..1901.8).contains(&poc));
        assert!(brain.value_low.unwrap() <= poc && poc <= brain.value_high.unwrap());
        assert!(brain.concentration > 0.0 && brain.concentration <= 1.0);
        assert!((0.0..=1.0).contains(&brain.entropy));
    }

    #[test]
    fn zero_tick_volume_abstains_instead_of_inventing_volume() {
        let series = (0..120)
            .map(|index| candle(index, 1900.0 + index as f64 * 0.01, 0))
            .collect::<Vec<_>>();
        let brain = assess(&series);
        assert!(!brain.available);
        assert_eq!(brain.relation, "UNKNOWN");
        assert!(brain.poc.is_none());
        assert!(brain.reason.contains("zero"));
    }

    #[test]
    fn value_area_relation_is_context_not_trade_action() {
        let mut series = (0..120)
            .map(|index| candle(index, 1900.0 + (index % 20) as f64 * 0.02, 100))
            .collect::<Vec<_>>();
        let last = series.last_mut().unwrap();
        last.close = 1904.0;
        last.high = 1904.2;
        last.low = 1903.8;
        let brain = assess(&series);
        assert!(brain.available);
        assert_eq!(brain.directional_context, 1);
        assert_eq!(brain.relation, "ABOVE_VALUE");
        assert!(brain.reason.contains("not an entry signal"));
    }

    #[test]
    fn profile_uses_only_requested_lookback_suffix() {
        let mut a = (0..140)
            .map(|index| candle(index, 1900.0 + index as f64 * 0.01, 100))
            .collect::<Vec<_>>();
        let mut b = a.clone();
        for item in b.iter_mut().take(20) {
            item.tick_volume = 50_000;
            item.close += 100.0;
            item.high += 100.0;
            item.low += 100.0;
            item.open += 100.0;
        }
        let left = assess(&a);
        let right = assess(&b);
        assert_eq!(left.total_tick_volume, right.total_tick_volume);
        assert_eq!(left.poc, right.poc);
        assert_eq!(left.value_low, right.value_low);
        assert_eq!(left.value_high, right.value_high);
        a.clear();
    }
}
