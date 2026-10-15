//! Causal Wyckoff-inspired events. Phase labels remain tentative.
use crate::studio::FrameBar;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Wyckoff {
    pub phase: String,
    pub event: String,
    pub side: i8,
    pub signal_time: u64,
    pub support: Option<f64>,
    pub resistance: Option<f64>,
    pub volume_source: String,
    pub evidence: Vec<String>,
    pub confirmed: bool,
}
pub fn analyze(bars: &[FrameBar]) -> Wyckoff {
    let mut r = Wyckoff {
        phase: "غير محددة".into(),
        event: "انتظار".into(),
        ..Default::default()
    };
    if bars.len() < 100 {
        r.evidence.push("نحتاج 100 شمعة مكتملة".into());
        return r;
    }
    let n = bars.len();
    let range = &bars[n - 75..n - 7];
    let low = range
        .iter()
        .map(|b| b.candle.low)
        .fold(f64::INFINITY, f64::min);
    let high = range
        .iter()
        .map(|b| b.candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let width = high - low;
    if width <= 0. {
        return r;
    }
    let real = bars[n - 75..].iter().all(|b| b.real_volume > 0);
    let vol = |b: &FrameBar| {
        if real {
            b.real_volume as f64
        } else {
            b.candle.tick_volume as f64
        }
    };
    r.volume_source = if real {
        "broker_real_volume"
    } else {
        "tick_activity_proxy"
    }
    .into();
    r.support = Some(low);
    r.resistance = Some(high);
    let touches_low = range
        .iter()
        .filter(|b| b.candle.low <= low + width * 0.12)
        .count();
    let touches_high = range
        .iter()
        .filter(|b| b.candle.high >= high - width * 0.12)
        .count();
    let baseline = range.iter().map(vol).sum::<f64>() / range.len() as f64;
    let avg_range = range
        .iter()
        .map(|b| b.candle.high - b.candle.low)
        .sum::<f64>()
        / range.len() as f64;
    let last = &bars[n - 1];
    let c = &last.candle;
    r.signal_time = c.time;
    r.evidence.push(format!(
        "ملامسات الدعم {touches_low} / المقاومة {touches_high}؛ نشاط نسبي {:.2}",
        vol(last) / baseline.max(1.)
    ));
    if touches_low < 2 || touches_high < 2 || width < avg_range * 2. {
        r.evidence.push("نطاق غير كافٍ لتأكيد وايكوف".into());
        return r;
    }
    r.phase = "B — نطاق محتمل".into();
    for event in bars[n - 6..n - 1].iter().rev() {
        let e = &event.candle;
        let quiet = vol(last) < vol(event) * 0.85 && vol(last) > 0.;
        if e.low < low - width * 0.015
            && e.close > low
            && e.close < high
            && c.low >= e.low
            && c.low <= low + width * 0.2
            && c.close > low
            && c.close > c.open
            && quiet
        {
            r.phase = "C → D محتملة".into();
            r.event = "Spring + Test".into();
            r.side = 1;
            r.confirmed = true;
            break;
        }
        if e.high > high + width * 0.015
            && e.close < high
            && e.close > low
            && c.high <= e.high
            && c.high >= high - width * 0.2
            && c.close < high
            && c.close < c.open
            && quiet
        {
            r.phase = "C → D محتملة".into();
            r.event = "Upthrust + Test".into();
            r.side = -1;
            r.confirmed = true;
            break;
        }
        let effort = e.high - e.low > avg_range * 1.3 && vol(event) > baseline * 1.3;
        if e.close > high
            && e.close > e.open
            && effort
            && c.low <= high + width * 0.12
            && c.close > high
            && c.close > c.open
            && quiet
        {
            r.phase = "D محتملة".into();
            r.event = "SOS → LPS".into();
            r.side = 1;
            r.confirmed = true;
            break;
        }
        if e.close < low
            && e.close < e.open
            && effort
            && c.high >= low - width * 0.12
            && c.close < low
            && c.close < c.open
            && quiet
        {
            r.phase = "D محتملة".into();
            r.event = "SOW → LPSY".into();
            r.side = -1;
            r.confirmed = true;
            break;
        }
    }
    if !r.confirmed {
        if c.low < low && c.close > low {
            r.event = "Spring محتمل؛ ينتظر الاختبار".into();
            r.phase = "C محتملة".into();
        } else if c.high > high && c.close < high {
            r.event = "Upthrust محتمل؛ ينتظر الاختبار".into();
            r.phase = "C محتملة".into();
        } else if c.close > high || c.close < low {
            r.event = "خروج نطاق؛ ينتظر إعادة اختبار".into();
        }
    }
    if vol(last) > baseline * 1.5 && c.high - c.low < avg_range * 0.7 {
        r.evidence.push("جهد مرتفع وحركة ضيقة: امتصاص محتمل".into());
    }
    r.evidence
        .push("تقدير قواعدي؛ التعلم يقاس بعد الإشارة ولا يضمن الصفقة التالية".into());
    r
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Candle;
    fn bars() -> Vec<FrameBar> {
        (0..110)
            .map(|i| {
                let p = 100. + (i as f64 * 0.5).sin() * 1.7;
                FrameBar {
                    candle: Candle {
                        time: 60 * (i + 1),
                        open: p,
                        high: p + 0.3,
                        low: p - 0.3,
                        close: p + 0.1,
                        tick_volume: 100,
                    },
                    spread_points: 2,
                    real_volume: 0,
                }
            })
            .collect()
    }
    #[test]
    fn spring_waits_for_subsequent_test() {
        let mut b = bars();
        b[109].candle = Candle {
            time: 6600,
            open: 100.,
            high: 101.,
            low: 97.,
            close: 99.,
            tick_volume: 200,
        };
        assert!(!analyze(&b).confirmed);
        b.push(FrameBar {
            candle: Candle {
                time: 6660,
                open: 98.2,
                high: 100.,
                low: 98.1,
                close: 99.8,
                tick_volume: 80,
            },
            spread_points: 2,
            real_volume: 0,
        });
        let r = analyze(&b);
        assert!(r.confirmed);
        assert_eq!(r.side, 1);
        assert_eq!(r.volume_source, "tick_activity_proxy");
    }
    #[test]
    fn mirrored_upthrust() {
        let mut b = bars();
        b[108].candle.high = 103.;
        b[108].candle.close = 101.;
        b[108].candle.tick_volume = 200;
        b[109].candle = Candle {
            time: 6600,
            open: 101.8,
            high: 101.9,
            low: 100.,
            close: 100.2,
            tick_volume: 80,
        };
        assert_eq!(analyze(&b).side, -1);
    }
}
