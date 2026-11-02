use super::{atr, sign, Candle, Swing};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
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
    /// Experimental 1x1 / 2x1 / 1x2 price-time projections; anchored to
    /// the pivot's *historical* ATR, not current chart pixel scaling.
    pub fan_anchor_atr: f64,
    pub fan_1x1: f64,
    pub fan_2x1: f64,
    pub fan_1x2: f64,
    pub fan_distance_atr: f64,
    pub fan_agrees_with_direction: bool,
    /// Square-of-Nine-inspired root lattice. Unit = one quote-currency price
    /// unit, NOT broker tick size; a change in scale changes all levels.
    pub square9_available: bool,
    pub square9_quote_unit: f64,
    pub square9_steps_45deg: i64,
    pub square9_nearest_level: f64,
    pub square9_distance_atr: f64,
    /// Nearest predeclared *bar-count* cycle since the confirmed pivot.
    /// This is not a market-clock prediction or automatically a trade trigger.
    pub time_nearest_cycle_bars: u64,
    pub time_distance_bars: u64,
    pub time_cycle_near: bool,
    pub note: String,
}

// Fixed, explicit research hypotheses, not a data-fitted best cycle or
// hidden broker tick-scaled Square-of-Nine magic factor.
const CYCLE_BARS: [u64;7] = [30,45,60,90,120,180,360];
const SQUARE9_QUOTE_UNIT: f64 = 1.0;
const FAN_ATR_PER_BAR_1X1: f64 = 0.10;

/// Canonical root-square step: 45° in this *defined* convention means
/// sqrt(P / quote_unit) shifts by 1/4. The chart's horizontal/vertical
/// pixel scales and tick sizes are deliberately irrelevant here.
fn square9_nearest(anchor:f64,price:f64,atr:f64)->Option<(i64,f64,f64)>{
    if !anchor.is_finite()||!price.is_finite()||!atr.is_finite()
        ||anchor<=0.||price<=0.||atr<=0. {return None}
    let root=(anchor/SQUARE9_QUOTE_UNIT).sqrt();
    let target=(price/SQUARE9_QUOTE_UNIT).sqrt();
    let n=((target-root)*4.).round();
    // Without bounded cast a saturated i64 can conceal overflow from a
    // corrupted quote or wildly different instrument denomination.
    if !n.is_finite() || n.abs()>1_000_000. {return None}
    let next=root+n*0.25;
    if next<=0. {return None}
    let level=next*next*SQUARE9_QUOTE_UNIT;
    if !level.is_finite() {return None}
    Some((n as i64,level,(price-level).abs()/atr))
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
        note: "هندسة جان بحثية: swing مؤكد + أثمان + ATR لكل شمعة + مروحة بمقياس ATR مثبت عند المحور + شبكة جذر السعر (الوحدة 1.0) + دورات بالشموع. ليست توقعاً مضموناً ولا زاوية شاشة.".into(),
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

    // Freeze the fan slope's volatility scale at the historical pivot bar:
    // using the current ATR here would silently repaint all older fan levels.
    if index>=15 {
        let anchor_atr=atr(&series[..=index]);
        if anchor_atr.is_finite()&&anchor_atr>0. {
            let projected=anchor.side as f64*anchor_atr*FAN_ATR_PER_BAR_1X1
                * bars_since as f64;
            out.fan_anchor_atr=anchor_atr;
            out.fan_1x1=anchor.price+projected;
            out.fan_2x1=anchor.price+2.*projected;
            out.fan_1x2=anchor.price+0.5*projected;
            out.fan_distance_atr=(last.close-out.fan_1x1).abs()/a;
            out.fan_agrees_with_direction=direction!=0 &&
                (last.close-out.fan_1x1)*direction as f64>=0.;
        }
    }
    if let Some((n,level,distance))=square9_nearest(anchor.price,last.close,a){
        out.square9_available=true;
        out.square9_quote_unit=SQUARE9_QUOTE_UNIT;
        out.square9_steps_45deg=n;
        out.square9_nearest_level=level;
        out.square9_distance_atr=distance;
    }
    let age=bars_since as u64;
    let cycle=CYCLE_BARS.into_iter().min_by_key(|c|age.abs_diff(*c)).unwrap_or(30);
    out.time_nearest_cycle_bars=cycle;
    out.time_distance_bars=age.abs_diff(cycle);
    out.time_cycle_near=out.time_distance_bars<=2;
    out
}
