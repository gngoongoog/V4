//! CES default formulas from CES_v1_Critical_Entropy_Scalper.pine; MT5 tick volume.
use crate::Candle;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub pae: f64,
    pub highest_pae: f64,
    pub eg: f64,
    pub epi: f64,
    pub dbd: f64,
    pub volume_ratio: f64,
    pub atr: f64,
    pub pressure_range: f64,
    pub compressed: bool,
    pub negative_three: bool,
    pub side: i8,
}
fn class(c: &Candle, tr: f64, atr: f64) -> usize {
    let r = if atr > 0.0 { tr / atr } else { 0.0 };
    let size = if r < 0.5 {
        0
    } else if r < 1.0 {
        1
    } else if r < 1.5 {
        2
    } else {
        3
    };
    size + if c.close >= c.open { 0 } else { 4 }
}
fn entropy(classes: &[usize]) -> f64 {
    let mut count = [0usize; 8];
    for &c in classes {
        count[c] += 1;
    }
    count
        .iter()
        .filter(|n| **n > 0)
        .map(|&n| {
            let p = n as f64 / classes.len() as f64;
            -p * p.log2()
        })
        .sum::<f64>()
        / 3.0
}
fn direction(compressed: bool, negative_three: bool, epi: f64, dbd: f64) -> i8 {
    if !compressed || !negative_three || epi <= 0.70 {
        0
    } else if dbd > 0.20 {
        1
    } else if dbd < -0.20 {
        -1
    } else {
        0
    }
}
/// Causal series, SMA seeded Wilder ATR20; 100 warmup bars and 30 contiguous M1 bars.
pub fn frames(candles: &[Candle]) -> Result<Vec<Option<Frame>>, String> {
    if candles.len() > 60_000
        || candles
            .iter()
            .any(|c| !c.valid() || c.high > 1e12 || c.time % 60 != 0)
        || candles.windows(2).any(|w| w[0].time >= w[1].time)
    {
        return Err("Invalid CES candles".into());
    }
    let n = candles.len();
    let mut out = vec![None; n];
    let mut tr = vec![0.0; n];
    let mut atr = vec![0.0; n];
    let mut classes = vec![0; n];
    let mut pae: Vec<Option<f64>> = vec![None; n];
    let mut eg: Vec<Option<f64>> = vec![None; n];
    for i in 0..n {
        let c = &candles[i];
        tr[i] = c.high - c.low;
        if i > 0 {
            tr[i] = tr[i]
                .max((c.high - candles[i - 1].close).abs())
                .max((c.low - candles[i - 1].close).abs());
        }
        if i < 19 {
            continue;
        }
        atr[i] = if i == 19 {
            tr[..20].iter().sum::<f64>() / 20.0
        } else {
            (atr[i - 1] * 19.0 + tr[i]) / 20.0
        };
        classes[i] = class(c, tr[i], atr[i]);
        if i < 38 {
            continue;
        }
        let h = entropy(&classes[i - 19..=i]);
        pae[i] = Some(h);
        if let Some(p) = pae[i - 3] {
            eg[i] = Some((h - p) / 3.0);
        }
        if i < 99
            || atr[i] <= 0.0
            || candles[i - 29..=i]
                .windows(2)
                .any(|w| w[1].time != w[0].time + 60)
        {
            continue;
        }
        let highest = pae[i - 49..=i]
            .iter()
            .flatten()
            .copied()
            .fold(0.0, f64::max);
        let gradient = eg[i].unwrap();
        let negative_three = (i - 2..=i).all(|j| eg[j].is_some_and(|v| v < 0.0));
        let avg = candles[i - 19..=i]
            .iter()
            .map(|c| c.tick_volume as f64)
            .sum::<f64>()
            / 20.0;
        let volume_ratio = if avg > 0.0 {
            c.tick_volume as f64 / avg
        } else {
            0.0
        };
        let epi = gradient.abs() * volume_ratio;
        let bulls = candles[i - 9..=i]
            .iter()
            .filter(|c| c.close >= c.open)
            .count() as f64;
        let dbd = (2.0 * bulls - 10.0) / 10.0;
        let compressed = h <= highest * 0.6;
        out[i] = Some(Frame {
            pae: h,
            highest_pae: highest,
            eg: gradient,
            epi,
            dbd,
            volume_ratio,
            atr: atr[i],
            pressure_range: tr[i - 2..=i].iter().sum::<f64>() / 3.0,
            compressed,
            negative_three,
            side: direction(compressed, negative_three, epi, dbd),
        });
    }
    Ok(out)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn fixture(n: usize) -> Vec<Candle> {
        (0..n)
            .map(|i| {
                let p = 2000.0 + i as f64 * 0.1;
                Candle {
                    time: 1_728_000_000 + i as u64 * 60,
                    open: p,
                    high: p + 0.7,
                    low: p - 0.3,
                    close: p + 0.1,
                    tick_volume: 100,
                }
            })
            .collect()
    }
    pub fn pressure_fixture() -> Vec<Candle> {
        (0..150)
            .map(|i| {
                let range = if i < 120 {
                    [0.3, 0.7, 1.2, 2.2][i % 4]
                } else {
                    0.8
                };
                let up = i >= 120 || i % 8 < 4;
                let open: f64 = 2000.0;
                let close = open + if up { 0.02 } else { -0.02 };
                Candle {
                    time: 1_728_000_000 + i as u64 * 60,
                    open,
                    close,
                    high: open.max(close) + (range - 0.02) / 2.0,
                    low: open.min(close) - (range - 0.02) / 2.0,
                    tick_volume: if i == 137 { 100000 } else { 1 },
                }
            })
            .collect()
    }
    #[test]
    fn pressure_signal_matches_independent_reference() {
        let f = frames(&pressure_fixture()).unwrap()[137].clone().unwrap();
        assert!((f.pae - 0.18966519786309374).abs() < 1e-12);
        assert!((f.eg - (-0.0914182838435214)).abs() < 1e-12);
        assert!((f.epi - 1.8280183533832852).abs() < 1e-10);
        assert_eq!(f.dbd, 1.0);
        assert!(f.compressed && f.negative_three);
        assert_eq!(f.side, 1);
    }
    #[test]
    fn entropy_volume_and_strict_thresholds() {
        assert_eq!(entropy(&[0; 20]), 0.0);
        assert!((entropy(&[0, 1, 2, 3, 4, 5, 6, 7]) - 1.0).abs() < 1e-12);
        let mut c = fixture(120);
        let f = frames(&c).unwrap().pop().unwrap().unwrap();
        assert_eq!(f.pae, 0.0);
        assert_eq!(f.dbd, 1.0);
        assert_eq!(f.epi, 0.0);
        assert_eq!(f.side, 0);
        for c in &mut c {
            c.tick_volume = 0;
        }
        assert_eq!(frames(&c).unwrap()[119].as_ref().unwrap().volume_ratio, 0.0);
        assert_eq!(direction(true, true, 0.70, 1.0), 0);
        assert_eq!(direction(true, true, 0.71, 0.20), 0);
        assert_eq!(direction(true, true, 0.71, 0.4), 1);
        assert_eq!(direction(true, true, 0.71, -0.4), -1);
        assert_eq!(direction(false, true, 1.0, 1.0), 0);
    }
    #[test]
    fn causal_prefix_and_gap_warmup() {
        let mut c = fixture(200);
        let prefix = frames(&c[..150]).unwrap();
        c[180].high += 20.0;
        c[180].tick_volume = 10000;
        assert_eq!(prefix, frames(&c).unwrap()[..150]);
        c.remove(175);
        let f = frames(&c).unwrap();
        assert!(f[175].is_none());
        assert!(f[98].is_none());
        assert!(f[99].is_some());
    }
}
