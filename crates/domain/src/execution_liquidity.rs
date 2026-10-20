//! Causal liquidity-sweep execution assessment.
//!
//! This brain observes only completed candles plus the current executable quote.
//! It does not claim to see hidden stops or institutional orders. A signal means
//! price swept a known 20-bar extreme, reclaimed it with a rejection wick and
//! above-normal tick activity, then remained close enough to the confirmation bar.
use crate::{Candle, Telemetry};

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Assessment {
    pub side: i8,
    pub stop_distance: f64,
    pub evidence: f64,
    pub activity_ratio: f64,
    pub reference_level: f64,
    pub reason: String,
}

pub(super) fn assess(series: &[Candle], telemetry: &Telemetry, atr: f64) -> Assessment {
    let mut out = Assessment {
        reason: "بانتظار sweep واستعادة مؤكدة لقمة/قاع سابق".into(),
        ..Default::default()
    };
    if series.len() < 21 || !atr.is_finite() || atr <= 0.0 {
        return out;
    }
    let n = series.len();
    let prior = &series[n - 21..n - 1];
    let last = &series[n - 1];
    let prior_high = prior
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let prior_low = prior.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    if !prior_high.is_finite() || !prior_low.is_finite() || prior_high <= prior_low {
        return out;
    }

    let average_activity = prior.iter().map(|c| c.tick_volume as f64).sum::<f64>()
        / prior.len() as f64;
    out.activity_ratio = if average_activity > 0.0 {
        last.tick_volume as f64 / average_activity
    } else {
        0.0
    };

    let range = (last.high - last.low).max(f64::EPSILON);
    let body = (last.close - last.open).abs();
    let lower_wick = last.open.min(last.close) - last.low;
    let upper_wick = last.high - last.open.max(last.close);
    let reclaim_buffer = 0.05 * atr;

    let buy = last.low < prior_low - reclaim_buffer
        && last.close > prior_low + reclaim_buffer
        && last.close > last.open
        && lower_wick >= 0.35 * range
        && lower_wick >= 0.8 * body
        && out.activity_ratio >= 1.10;
    let sell = last.high > prior_high + reclaim_buffer
        && last.close < prior_high - reclaim_buffer
        && last.close < last.open
        && upper_wick >= 0.35 * range
        && upper_wick >= 0.8 * body
        && out.activity_ratio >= 1.10;

    let side = match (buy, sell) {
        (true, false) => 1,
        (false, true) => -1,
        _ => {
            out.reason = format!(
                "لا sweep صالح: قاع {:.5} / قمة {:.5} / نشاط ticks {:.2}×",
                prior_low, prior_high, out.activity_ratio
            );
            return out;
        }
    };
    let reference = if side == 1 { prior_low } else { prior_high };
    let wick = if side == 1 { lower_wick } else { upper_wick };
    let reclaim = ((last.close - reference) * side as f64 / atr).max(0.0);
    let wick_score = (wick / range).clamp(0.0, 1.0);
    let activity_score = ((out.activity_ratio - 0.8) / 0.8).clamp(0.0, 1.0);
    let reclaim_score = reclaim.clamp(0.0, 1.0);
    out.evidence = (0.45 * wick_score + 0.30 * activity_score + 0.25 * reclaim_score)
        .clamp(0.0, 1.0);
    if out.evidence < 0.62 {
        out.reason = format!(
            "sweep مرصود لكن الدليل {:.2} أقل من 0.62؛ نشاط {:.2}×",
            out.evidence, out.activity_ratio
        );
        return out;
    }

    let entry = if side == 1 { telemetry.ask } else { telemetry.bid }.unwrap_or(0.0);
    if !entry.is_finite() || entry <= 0.0 {
        out.reason = "sweep مرصود لكن سعر التنفيذ الحالي غير متاح".into();
        return out;
    }
    if (entry - last.close).abs() > 0.35 * atr {
        out.reason = format!(
            "sweep مرصود لكن السعر ابتعد {:.2} ATR عن شمعة التأكيد؛ لا مطاردة",
            (entry - last.close).abs() / atr
        );
        return out;
    }

    let structural_stop = if side == 1 {
        last.low - 0.15 * atr
    } else {
        last.high + 0.15 * atr
    };
    let stop_distance = (entry - structural_stop) * side as f64;
    if !stop_distance.is_finite() || stop_distance < 0.25 * atr || stop_distance > 3.0 * atr {
        out.reason = "sweep مرصود لكن الوقف البنيوي خارج نطاق 0.25–3 ATR".into();
        return out;
    }

    out.side = side;
    out.stop_distance = stop_distance;
    out.reference_level = reference;
    out.reason = format!(
        "Liquidity sweep + reclaim عند {:.5}؛ wick {:.0}% من المدى، نشاط ticks {:.2}×، دليل {:.2}. لا ادعاء رؤية أوامر وقف مخفية",
        reference,
        100.0 * wick_score,
        out.activity_ratio,
        out.evidence
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(last: Candle) -> Vec<Candle> {
        let mut rows = (0..99)
            .map(|i| Candle {
                time: 1_700_000_000 + i * 60,
                open: if i % 2 == 0 { 99.9 } else { 100.1 },
                high: 101.0,
                low: 99.0,
                close: if i % 2 == 0 { 100.1 } else { 99.9 },
                tick_volume: 100,
            })
            .collect::<Vec<_>>();
        rows.push(last);
        rows
    }

    #[test]
    fn bullish_sweep_requires_reclaim_wick_activity_and_current_quote() {
        let last = Candle {
            time: 1_700_000_000 + 99 * 60,
            open: 99.8,
            high: 100.6,
            low: 97.8,
            close: 100.2,
            tick_volume: 150,
        };
        let t = Telemetry {
            bid: Some(100.23),
            ask: Some(100.25),
            ..Default::default()
        };
        let signal = assess(&bars(last.clone()), &t, 2.0);
        assert_eq!(signal.side, 1);
        assert!(signal.stop_distance > 0.0);
        assert!(signal.evidence >= 0.62);
        assert!(signal.reason.contains("لا ادعاء"));

        let mut quiet = last.clone();
        quiet.tick_volume = 100;
        assert_eq!(assess(&bars(quiet), &t, 2.0).side, 0);

        let mut no_reclaim = last;
        no_reclaim.open = 98.2;
        no_reclaim.close = 98.7;
        assert_eq!(assess(&bars(no_reclaim), &t, 2.0).side, 0);
    }

    #[test]
    fn bearish_sweep_is_symmetric_and_rejects_price_chasing() {
        let last = Candle {
            time: 1_700_000_000 + 99 * 60,
            open: 100.2,
            high: 102.2,
            low: 99.4,
            close: 99.8,
            tick_volume: 160,
        };
        let t = Telemetry {
            bid: Some(99.75),
            ask: Some(99.77),
            ..Default::default()
        };
        let signal = assess(&bars(last.clone()), &t, 2.0);
        assert_eq!(signal.side, -1);
        assert!(signal.stop_distance > 0.0);

        let chased = Telemetry {
            bid: Some(98.9),
            ask: Some(98.92),
            ..Default::default()
        };
        let blocked = assess(&bars(last), &chased, 2.0);
        assert_eq!(blocked.side, 0);
        assert!(blocked.reason.contains("لا مطاردة"));
    }
}