//! Causal trend-pullback execution assessment.
//!
//! This brain waits for an established EMA20/EMA50 trend, a completed-bar pullback
//! toward the fast mean that does not materially break the slow mean, and a completed
//! confirmation candle back with the trend. The current executable quote is used only
//! for anti-chasing and structural-stop distance; no future candle is consulted.
use crate::{Candle, Telemetry};

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Assessment {
    pub side: i8,
    pub stop_distance: f64,
    pub trend_strength: f64,
    pub pullback_depth: f64,
    pub confirmation_body_ratio: f64,
    pub reason: String,
}

fn ema(values: &[f64], period: usize) -> Vec<f64> {
    if values.is_empty() || period == 0 {
        return vec![];
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut out = Vec::with_capacity(values.len());
    let mut current = values[0];
    out.push(current);
    for value in &values[1..] {
        current += alpha * (*value - current);
        out.push(current);
    }
    out
}

pub(super) fn assess(series: &[Candle], telemetry: &Telemetry, atr: f64) -> Assessment {
    let mut out = Assessment {
        reason: "بانتظار ترند واضح + رجوع منظم + شمعة تأكيد".into(),
        ..Default::default()
    };
    if series.len() < 60 || !atr.is_finite() || atr <= 0.0 {
        return out;
    }

    let closes: Vec<f64> = series.iter().map(|bar| bar.close).collect();
    if closes.iter().any(|value| !value.is_finite()) {
        return out;
    }
    let fast = ema(&closes, 20);
    let slow = ema(&closes, 50);
    let n = series.len();
    let last = &series[n - 1];
    let prior = &series[n - 4..n - 1];
    let fast_now = fast[n - 1];
    let slow_now = slow[n - 1];
    let fast_slope = (fast[n - 1] - fast[n - 4]) / atr;
    let slow_slope = (slow[n - 1] - slow[n - 4]) / atr;
    let separation = (fast_now - slow_now) / atr;

    let bullish_trend = separation >= 0.15 && fast_slope >= 0.05 && slow_slope >= 0.0;
    let bearish_trend = separation <= -0.15 && fast_slope <= -0.05 && slow_slope <= 0.0;
    if !bullish_trend && !bearish_trend {
        out.reason = format!(
            "لا ترند كافٍ: فصل EMA20/50 = {:.2} ATR، ميل السريع = {:.2} ATR",
            separation, fast_slope
        );
        return out;
    }
    let side = if bullish_trend { 1 } else { -1 };
    out.trend_strength = separation.abs();

    let mut touched_fast = false;
    let mut deepest = 0.0_f64;
    let mut structure_ok = true;
    for (offset, bar) in prior.iter().enumerate() {
        let i = n - 4 + offset;
        if side == 1 {
            touched_fast |= bar.low <= fast[i] + 0.15 * atr;
            structure_ok &= bar.close >= slow[i] - 0.15 * atr;
            deepest = deepest.max(((fast[i] - bar.low) / atr).max(0.0));
        } else {
            touched_fast |= bar.high >= fast[i] - 0.15 * atr;
            structure_ok &= bar.close <= slow[i] + 0.15 * atr;
            deepest = deepest.max(((bar.high - fast[i]) / atr).max(0.0));
        }
    }
    out.pullback_depth = deepest;
    if !touched_fast || !structure_ok || deepest > 1.35 {
        out.reason = format!(
            "الرجوع غير صالح: لمس EMA20={}، البنية={}، العمق {:.2} ATR",
            touched_fast, structure_ok, deepest
        );
        return out;
    }

    let range = (last.high - last.low).max(f64::EPSILON);
    let body = (last.close - last.open).abs();
    let body_ratio = (body / range).clamp(0.0, 1.0);
    out.confirmation_body_ratio = body_ratio;
    let close_location = if side == 1 {
        (last.close - last.low) / range
    } else {
        (last.high - last.close) / range
    };
    let confirmation = (last.close - last.open) * side as f64 > 0.0
        && (last.close - fast_now) * side as f64 > 0.0
        && body_ratio >= 0.45
        && close_location >= 0.65;
    if !confirmation {
        out.reason = format!(
            "الرجوع موجود لكن شمعة التأكيد ضعيفة: body {:.0}%، close-location {:.0}%",
            body_ratio * 100.0,
            close_location * 100.0
        );
        return out;
    }

    let distance_from_fast = ((last.close - fast_now) * side as f64 / atr).max(0.0);
    if distance_from_fast > 0.60 {
        out.reason = format!(
            "تأكيد صالح لكن الإغلاق ابتعد {:.2} ATR عن EMA20؛ لا مطاردة",
            distance_from_fast
        );
        return out;
    }

    let entry = if side == 1 { telemetry.ask } else { telemetry.bid }.unwrap_or(0.0);
    if !entry.is_finite() || entry <= 0.0 {
        out.reason = "إعداد pullback صالح لكن سعر التنفيذ الحالي غير متاح".into();
        return out;
    }
    if (entry - last.close).abs() > 0.30 * atr {
        out.reason = format!(
            "إعداد pullback صالح لكن السعر الحالي ابتعد {:.2} ATR عن التأكيد؛ لا مطاردة",
            (entry - last.close).abs() / atr
        );
        return out;
    }

    let structural = if side == 1 {
        prior.iter().chain(std::iter::once(last)).map(|bar| bar.low).fold(f64::INFINITY, f64::min)
            - 0.10 * atr
    } else {
        prior.iter().chain(std::iter::once(last)).map(|bar| bar.high).fold(f64::NEG_INFINITY, f64::max)
            + 0.10 * atr
    };
    let stop_distance = (entry - structural) * side as f64;
    if !stop_distance.is_finite() || stop_distance < 0.35 * atr || stop_distance > 2.75 * atr {
        out.reason = format!(
            "إعداد pullback صالح لكن الوقف البنيوي {:.2} ATR خارج النطاق 0.35–2.75",
            stop_distance / atr
        );
        return out;
    }

    out.side = side;
    out.stop_distance = stop_distance;
    out.reason = format!(
        "Trend Pullback: EMA20/50 فصل {:.2} ATR، رجوع {:.2} ATR، تأكيد body {:.0}%، وقف بنيوي {:.2} ATR؛ دخول بعد الرجوع لا بعد مطاردة الحركة",
        out.trend_strength,
        out.pullback_depth,
        out.confirmation_body_ratio * 100.0,
        out.stop_distance / atr
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trend(up: bool) -> Vec<Candle> {
        let mut rows = Vec::new();
        for i in 0..96 {
            let base = if up { 100.0 + i as f64 * 0.18 } else { 130.0 - i as f64 * 0.18 };
            rows.push(Candle {
                time: 1_700_000_000 + i * 60,
                open: base - if up { 0.04 } else { -0.04 },
                high: base + 0.20,
                low: base - 0.20,
                close: base,
                tick_volume: 100,
            });
        }
        rows
    }

    #[test]
    fn bullish_pullback_waits_for_completed_confirmation_and_structural_stop() {
        let mut rows = trend(true);
        let fast_zone = rows.last().unwrap().close - 1.20;
        for j in 0..3 {
            let base = fast_zone - j as f64 * 0.10;
            rows.push(Candle { time: 1_700_000_000 + (96 + j) * 60, open: base + 0.12, high: base + 0.20, low: base - 0.28, close: base, tick_volume: 100 });
        }
        let base = rows.last().unwrap().close + 0.24;
        rows.push(Candle { time: 1_700_000_000 + 99 * 60, open: base - 0.22, high: base + 0.08, low: base - 0.26, close: base, tick_volume: 120 });
        let t = Telemetry { bid: Some(base + 0.01), ask: Some(base + 0.03), ..Default::default() };
        let q = assess(&rows, &t, 1.0);
        assert_eq!(q.side, 1, "{}", q.reason);
        assert!(q.stop_distance > 0.0);
        assert!(q.reason.contains("دخول بعد الرجوع"));
    }

    #[test]
    fn bearish_pullback_is_symmetric_and_blocks_chasing() {
        let mut rows = trend(false);
        let fast_zone = rows.last().unwrap().close + 1.20;
        for j in 0..3 {
            let base = fast_zone + j as f64 * 0.10;
            rows.push(Candle { time: 1_700_000_000 + (96 + j) * 60, open: base - 0.12, high: base + 0.28, low: base - 0.20, close: base, tick_volume: 100 });
        }
        let base = rows.last().unwrap().close - 0.24;
        rows.push(Candle { time: 1_700_000_000 + 99 * 60, open: base + 0.22, high: base + 0.26, low: base - 0.08, close: base, tick_volume: 120 });
        let t = Telemetry { bid: Some(base - 0.03), ask: Some(base - 0.01), ..Default::default() };
        let q = assess(&rows, &t, 1.0);
        assert_eq!(q.side, -1, "{}", q.reason);
        let chased = Telemetry { bid: Some(base - 0.50), ask: Some(base - 0.48), ..Default::default() };
        let blocked = assess(&rows, &chased, 1.0);
        assert_eq!(blocked.side, 0);
        assert!(blocked.reason.contains("لا مطاردة"));
    }

    #[test]
    fn flat_market_abstains() {
        let rows = (0..100).map(|i| Candle { time: 1_700_000_000 + i * 60, open: 100.0, high: 100.2, low: 99.8, close: 100.0, tick_volume: 100 }).collect::<Vec<_>>();
        let t = Telemetry { bid: Some(100.0), ask: Some(100.02), ..Default::default() };
        assert_eq!(assess(&rows, &t, 1.0).side, 0);
    }
}
