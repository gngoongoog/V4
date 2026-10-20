//! Deterministic multi-symbol correlation research over saved M1 histories.
//!
//! Research-only: this module has no broker/execution API. Missing minutes stay
//! missing; returns are emitted only when both t and t-60 exist for a symbol.
use crate::backtest::{History, MAX_BARS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ALGORITHM: &str = "multisymbol-correlation-v1 / exact-M1-no-fill-v1";
pub const IDENTITY_ALGORITHM: &str = "fnv1a64-v1";
const MIN_SOURCES: usize = 2;
const MAX_SOURCES: usize = 8;
const MINUTE: u64 = 60;
const EPS: f64 = 1e-15;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CorrelationConfig {
    pub min_overlap: usize,
}
impl Default for CorrelationConfig {
    fn default() -> Self { Self { min_overlap: 30 } }
}
impl CorrelationConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=MAX_BARS).contains(&self.min_overlap) {
            return Err("Correlation minimum overlap must be between 2 and MAX_BARS".into());
        }
        Ok(())
    }
}

/// One immutable saved research source and the exact analysis interval from its run.
pub struct Input<'a> {
    pub source_id: &'a str,
    pub from: u64,
    pub to: u64, // exclusive UTC seconds
    pub history: &'a History,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SourceInfo {
    pub source_id: String,
    pub symbol: String,
    pub server: String,
    pub login: u64,
    pub currency: String,
    pub captured_ms: u64,
    pub from: u64,
    pub to: u64,
    pub available_minutes: usize,
    pub return_samples: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlignedMinute {
    pub time: u64,
    /// Same source order as `Report.sources`.
    pub available: Vec<bool>,
    /// Log close-to-close M1 return. None means unavailable; never forward-filled.
    pub returns: Vec<Option<f64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PairStats {
    pub left: String,
    pub right: String,
    pub overlap: usize,
    pub available: bool,
    pub correlation: Option<f64>,
    pub covariance: Option<f64>,
    pub left_stddev: Option<f64>,
    pub right_stddev: Option<f64>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PortfolioStats {
    /// Rows where every source has an actual return at the same M1 timestamp.
    pub overlap: usize,
    pub available: bool,
    pub covariance: Option<Vec<Vec<f64>>>,
    pub volatility: Option<f64>,
    pub diversification_ratio: Option<f64>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub algorithm: String,
    pub identity_algorithm: String,
    pub identity: String,
    pub from: u64,
    pub to: u64,
    pub min_overlap: usize,
    pub sources: Vec<SourceInfo>,
    pub minutes: Vec<AlignedMinute>,
    pub pairs: Vec<PairStats>,
    pub portfolio: PortfolioStats,
    pub execution_effect: bool,
    pub forward_fill: bool,
}

struct Prepared<'a> {
    input: Input<'a>,
    closes: BTreeMap<u64, f64>,
}

pub fn analyze(mut inputs: Vec<Input<'_>>, config: CorrelationConfig) -> Result<Report, String> {
    config.validate()?;
    if !(MIN_SOURCES..=MAX_SOURCES).contains(&inputs.len()) {
        return Err(format!("Correlation research requires {MIN_SOURCES}..={MAX_SOURCES} sources"));
    }
    for input in &inputs {
        input.history.validate()?;
        if input.source_id.trim().is_empty() || input.source_id.len() > 160 {
            return Err("Correlation source ID is empty or too long".into());
        }
        if input.from == 0 || input.from % MINUTE != 0 || input.to % MINUTE != 0 || input.to <= input.from {
            return Err("Correlation source interval must be a valid exclusive UTC M1 range".into());
        }
    }
    inputs.sort_by(|a, b| (a.history.symbol.as_str(), a.source_id).cmp(&(b.history.symbol.as_str(), b.source_id)));
    let mut source_ids = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    for input in &inputs {
        if !source_ids.insert(input.source_id.to_owned()) { return Err("Duplicate correlation source ID".into()); }
        if !symbols.insert(input.history.symbol.clone()) { return Err("Correlation v1 requires distinct symbols".into()); }
    }
    let first = inputs[0].history;
    if inputs.iter().any(|v| {
        v.history.login != first.login || v.history.server != first.server || v.history.currency != first.currency
    }) {
        return Err("Correlation v1 requires the same account, server and currency context".into());
    }
    let from = inputs.iter().map(|v| v.from).max().unwrap();
    let to = inputs.iter().map(|v| v.to).min().unwrap();
    if to <= from || (to - from) / MINUTE > MAX_BARS as u64 {
        return Err("Correlation sources have no valid common M1 interval".into());
    }

    let prepared: Vec<_> = inputs.into_iter().map(|input| {
        let closes = input.history.bars.iter()
            .filter(|bar| bar.candle.time >= from && bar.candle.time < to)
            .map(|bar| (bar.candle.time, bar.candle.close))
            .collect();
        Prepared { input, closes }
    }).collect();

    let mut minutes = Vec::with_capacity(((to - from) / MINUTE) as usize);
    let mut time = from;
    while time < to {
        let mut available = Vec::with_capacity(prepared.len());
        let mut returns = Vec::with_capacity(prepared.len());
        for source in &prepared {
            let current = source.closes.get(&time).copied();
            available.push(current.is_some());
            let value = if time > from {
                current.zip(source.closes.get(&(time - MINUTE)).copied()).and_then(|(now, previous)| {
                    let value = (now / previous).ln();
                    value.is_finite().then_some(value)
                })
            } else { None };
            returns.push(value);
        }
        minutes.push(AlignedMinute { time, available, returns });
        time += MINUTE;
    }

    let sources = prepared.iter().enumerate().map(|(index, source)| SourceInfo {
        source_id: source.input.source_id.to_owned(),
        symbol: source.input.history.symbol.clone(),
        server: source.input.history.server.clone(),
        login: source.input.history.login,
        currency: source.input.history.currency.clone(),
        captured_ms: source.input.history.captured_ms,
        from: source.input.from,
        to: source.input.to,
        available_minutes: minutes.iter().filter(|m| m.available[index]).count(),
        return_samples: minutes.iter().filter(|m| m.returns[index].is_some()).count(),
    }).collect::<Vec<_>>();

    let mut pairs = Vec::new();
    for left in 0..sources.len() {
        for right in left + 1..sources.len() {
            let rows = minutes.iter().filter_map(|minute| minute.returns[left].zip(minute.returns[right])).collect::<Vec<_>>();
            pairs.push(pair_stats(&sources[left].symbol, &sources[right].symbol, &rows, config.min_overlap));
        }
    }
    let portfolio = portfolio_stats(&minutes, sources.len(), config.min_overlap);
    let identity = identity(&prepared, &minutes, from, to, config.min_overlap);
    Ok(Report {
        algorithm: ALGORITHM.into(),
        identity_algorithm: IDENTITY_ALGORITHM.into(),
        identity,
        from,
        to,
        min_overlap: config.min_overlap,
        sources,
        minutes,
        pairs,
        portfolio,
        execution_effect: false,
        forward_fill: false,
    })
}

fn pair_stats(left: &str, right: &str, rows: &[(f64, f64)], minimum: usize) -> PairStats {
    if rows.len() < minimum {
        return unavailable_pair(left, right, rows.len(), format!("Need at least {minimum} pairwise-valid returns"));
    }
    let xs = rows.iter().map(|v| v.0).collect::<Vec<_>>();
    let ys = rows.iter().map(|v| v.1).collect::<Vec<_>>();
    let Some((covariance, left_var, right_var)) = covariance2(&xs, &ys) else {
        return unavailable_pair(left, right, rows.len(), "Pair statistics are not finite".into());
    };
    if left_var <= EPS || right_var <= EPS {
        return PairStats {
            left: left.into(), right: right.into(), overlap: rows.len(), available: false,
            correlation: None, covariance: Some(covariance),
            left_stddev: Some(left_var.max(0.0).sqrt()), right_stddev: Some(right_var.max(0.0).sqrt()),
            reason: "Correlation unavailable because one return series has zero variance".into(),
        };
    }
    let left_stddev = left_var.sqrt();
    let right_stddev = right_var.sqrt();
    let correlation = (covariance / (left_stddev * right_stddev)).clamp(-1.0, 1.0);
    PairStats {
        left: left.into(), right: right.into(), overlap: rows.len(), available: true,
        correlation: Some(correlation), covariance: Some(covariance),
        left_stddev: Some(left_stddev), right_stddev: Some(right_stddev), reason: String::new(),
    }
}

fn unavailable_pair(left: &str, right: &str, overlap: usize, reason: String) -> PairStats {
    PairStats { left: left.into(), right: right.into(), overlap, available: false,
        correlation: None, covariance: None, left_stddev: None, right_stddev: None, reason }
}

fn covariance2(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    if xs.len() != ys.len() || xs.len() < 2 { return None; }
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let denominator = n - 1.0;
    let covariance = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / denominator;
    let vx = xs.iter().map(|x| (x - mx).powi(2)).sum::<f64>() / denominator;
    let vy = ys.iter().map(|y| (y - my).powi(2)).sum::<f64>() / denominator;
    [covariance, vx, vy].iter().all(|v| v.is_finite()).then_some((covariance, vx, vy))
}

fn portfolio_stats(minutes: &[AlignedMinute], count: usize, minimum: usize) -> PortfolioStats {
    let rows = minutes.iter().filter_map(|minute| {
        let values = minute.returns.iter().copied().collect::<Option<Vec<_>>>()?;
        (values.len() == count).then_some(values)
    }).collect::<Vec<_>>();
    if rows.len() < minimum {
        return PortfolioStats { overlap: rows.len(), available: false, covariance: None, volatility: None,
            diversification_ratio: None, reason: format!("Need at least {minimum} all-symbol-valid returns") };
    }
    let means = (0..count).map(|column| rows.iter().map(|row| row[column]).sum::<f64>() / rows.len() as f64).collect::<Vec<_>>();
    let denominator = (rows.len() - 1) as f64;
    let mut covariance = vec![vec![0.0; count]; count];
    for i in 0..count {
        for j in i..count {
            let value = rows.iter().map(|row| (row[i] - means[i]) * (row[j] - means[j])).sum::<f64>() / denominator;
            if !value.is_finite() {
                return PortfolioStats { overlap: rows.len(), available: false, covariance: None, volatility: None,
                    diversification_ratio: None, reason: "Portfolio covariance is not finite".into() };
            }
            covariance[i][j] = value;
            covariance[j][i] = value;
        }
    }
    let weight = 1.0 / count as f64;
    let variance = (0..count).flat_map(|i| (0..count).map(move |j| (i, j)))
        .map(|(i, j)| weight * weight * covariance[i][j]).sum::<f64>();
    if !variance.is_finite() || variance < -EPS {
        return PortfolioStats { overlap: rows.len(), available: false, covariance: None, volatility: None,
            diversification_ratio: None, reason: "Portfolio variance is invalid".into() };
    }
    let volatility = variance.max(0.0).sqrt();
    let standalone = (0..count).map(|i| covariance[i][i].max(0.0).sqrt()).sum::<f64>() * weight;
    let diversification_ratio = (volatility > EPS && standalone.is_finite()).then_some(standalone / volatility);
    PortfolioStats { overlap: rows.len(), available: true, covariance: Some(covariance), volatility: Some(volatility),
        diversification_ratio, reason: if diversification_ratio.is_none() { "Diversification ratio unavailable at zero portfolio volatility".into() } else { String::new() } }
}

fn identity(prepared: &[Prepared<'_>], minutes: &[AlignedMinute], from: u64, to: u64, minimum: usize) -> String {
    let mut hash = Fnv1a64::new();
    hash.bytes(ALGORITHM.as_bytes());
    hash.u64(from); hash.u64(to); hash.u64(minimum as u64);
    for source in prepared {
        hash.bytes(source.input.source_id.as_bytes());
        hash.bytes(source.input.history.symbol.as_bytes());
        hash.bytes(source.input.history.server.as_bytes());
        hash.u64(source.input.history.login);
        hash.bytes(source.input.history.currency.as_bytes());
        hash.u64(source.input.history.captured_ms);
        hash.u64(source.input.from); hash.u64(source.input.to);
    }
    for minute in minutes {
        hash.u64(minute.time);
        for (available, value) in minute.available.iter().zip(&minute.returns) {
            hash.u8(u8::from(*available));
            match value { Some(v) => { hash.u8(1); hash.u64(v.to_bits()); }, None => hash.u8(0) }
        }
    }
    format!("{:016x}", hash.0)
}

struct Fnv1a64(u64);
impl Fnv1a64 {
    fn new() -> Self { Self(0xcbf29ce484222325) }
    fn u8(&mut self, value: u8) { self.0 ^= u64::from(value); self.0 = self.0.wrapping_mul(0x100000001b3); }
    fn bytes(&mut self, values: &[u8]) { self.u64(values.len() as u64); for value in values { self.u8(*value); } }
    fn u64(&mut self, value: u64) { for byte in value.to_le_bytes() { self.u8(byte); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backtest::{Contract, HistoryBar}, Candle};

    const T0: u64 = 1_800_000_000;
    fn contract() -> Contract { Contract { point:0.01, tick_size:0.01, tick_loss:1.0, tick_profit:1.0, volume_min:0.01, volume_step:0.01, volume_max:100.0 } }
    fn history(symbol: &str, returns: &[f64], missing: &[usize]) -> History {
        let mut closes = vec![100.0];
        for r in returns { closes.push(*closes.last().unwrap() * r.exp()); }
        let bars = closes.into_iter().enumerate().filter(|(i, _)| !missing.contains(i)).map(|(i, close)| HistoryBar {
            candle: Candle { time:T0+i as u64*60, open:close, high:close, low:close, close, tick_volume:100 }, spread_points:10
        }).collect::<Vec<_>>();
        let last = T0 + returns.len() as u64 * 60;
        History { symbol:symbol.into(), server:"Demo".into(), login:7, currency:"USD".into(), captured_ms:(last+60)*1000, contract:contract(), bars }
    }
    fn input<'a>(id: &'a str, history: &'a History, n: usize) -> Input<'a> { Input { source_id:id, from:T0, to:T0+(n as u64+1)*60, history } }
    fn cfg(min_overlap: usize) -> CorrelationConfig { CorrelationConfig { min_overlap } }
    fn pair(report: &Report) -> &PairStats { &report.pairs[0] }

    #[test]
    fn perfect_positive_and_negative_correlation() {
        let a_returns=[0.01,0.02,-0.01,0.03,-0.02,0.015];
        let b_returns=a_returns.map(|v|v*2.0);
        let c_returns=a_returns.map(|v|-v);
        let a=history("A",&a_returns,&[]); let b=history("B",&b_returns,&[]); let c=history("C",&c_returns,&[]);
        let positive=analyze(vec![input("a",&a,a_returns.len()),input("b",&b,b_returns.len())],cfg(4)).unwrap();
        assert!((pair(&positive).correlation.unwrap()-1.0).abs()<1e-12);
        let negative=analyze(vec![input("a",&a,a_returns.len()),input("c",&c,c_returns.len())],cfg(4)).unwrap();
        assert!((pair(&negative).correlation.unwrap()+1.0).abs()<1e-12);
    }

    #[test]
    fn orthogonal_fixture_is_near_zero() {
        let a=history("A",&[0.01,-0.01,0.01,-0.01],&[]);
        let b=history("B",&[0.01,0.01,-0.01,-0.01],&[]);
        let report=analyze(vec![input("a",&a,4),input("b",&b,4)],cfg(4)).unwrap();
        assert!(pair(&report).correlation.unwrap().abs()<1e-12);
    }

    #[test]
    fn missing_minute_is_not_filled_and_invalidates_following_return() {
        let returns=[0.01,0.02,0.03,0.04,0.05];
        let a=history("A",&returns,&[2]); let b=history("B",&returns,&[]);
        let report=analyze(vec![input("a",&a,5),input("b",&b,5)],cfg(2)).unwrap();
        assert!(!report.minutes[2].available[0]);
        assert_eq!(report.minutes[2].returns[0],None);
        assert_eq!(report.minutes[3].returns[0],None);
        assert!(report.minutes[4].returns[0].is_some());
        assert!(!report.forward_fill);
    }

    #[test]
    fn availability_mask_tracks_asynchronous_sources() {
        let returns=[0.01,0.02,0.03,0.04];
        let a=history("A",&returns,&[1]); let b=history("B",&returns,&[3]);
        let report=analyze(vec![input("a",&a,4),input("b",&b,4)],cfg(2)).unwrap();
        assert_eq!(report.minutes[1].available,vec![false,true]);
        assert_eq!(report.minutes[3].available,vec![true,false]);
    }

    #[test]
    fn insufficient_overlap_is_unavailable_not_zero() {
        let a=history("A",&[0.01,0.02,0.03],&[]); let b=history("B",&[0.01,0.02,0.03],&[]);
        let report=analyze(vec![input("a",&a,3),input("b",&b,3)],cfg(4)).unwrap();
        assert!(!pair(&report).available); assert_eq!(pair(&report).correlation,None); assert!(pair(&report).reason.contains("at least 4"));
    }

    #[test]
    fn zero_variance_does_not_fabricate_correlation() {
        let a=history("A",&[0.0,0.0,0.0,0.0],&[]); let b=history("B",&[0.01,0.02,-0.01,0.03],&[]);
        let report=analyze(vec![input("a",&a,4),input("b",&b,4)],cfg(4)).unwrap();
        assert!(!pair(&report).available); assert_eq!(pair(&report).correlation,None); assert!(pair(&report).reason.contains("zero variance"));
    }

    #[test]
    fn input_order_does_not_change_identity_or_source_order() {
        let a=history("AAA",&[0.01,0.02,0.03],&[]); let b=history("BBB",&[0.03,0.02,0.01],&[]);
        let x=analyze(vec![input("source-b",&b,3),input("source-a",&a,3)],cfg(2)).unwrap();
        let y=analyze(vec![input("source-a",&a,3),input("source-b",&b,3)],cfg(2)).unwrap();
        assert_eq!(x.identity,y.identity); assert_eq!(x.sources,y.sources); assert_eq!(x.minutes,y.minutes); assert_eq!(x.pairs,y.pairs);
        assert_eq!(x.sources.iter().map(|s|s.symbol.as_str()).collect::<Vec<_>>(),vec!["AAA","BBB"]);
    }

    #[test]
    fn mixed_account_context_and_duplicate_symbols_are_rejected() {
        let a=history("A",&[0.01,0.02],&[]); let mut b=history("B",&[0.01,0.02],&[]); b.login=8;
        assert!(analyze(vec![input("a",&a,2),input("b",&b,2)],cfg(2)).unwrap_err().contains("same account"));
        let b=history("A",&[0.01,0.02],&[]);
        assert!(analyze(vec![input("a",&a,2),input("b",&b,2)],cfg(2)).unwrap_err().contains("distinct symbols"));
    }

    #[test]
    fn later_price_change_cannot_change_earlier_aligned_returns() {
        let r=[0.01,0.02,0.03,0.04];
        let a=history("A",&r,&[]); let b=history("B",&r,&[]);
        let before=analyze(vec![input("a",&a,4),input("b",&b,4)],cfg(2)).unwrap();
        let mut altered=history("B",&r,&[]); let last=altered.bars.last_mut().unwrap(); last.candle.open*=2.0; last.candle.high*=2.0; last.candle.low*=2.0; last.candle.close*=2.0;
        let after=analyze(vec![input("a",&a,4),input("b",&altered,4)],cfg(2)).unwrap();
        assert_eq!(before.minutes[..4],after.minutes[..4]);
        assert_ne!(before.identity,after.identity);
    }

    #[test]
    fn portfolio_uses_only_all_symbol_intersection() {
        let r=[0.01,0.02,0.03,0.04,0.05];
        let a=history("A",&r,&[]); let b=history("B",&r,&[]); let c=history("C",&r,&[2]);
        let report=analyze(vec![input("a",&a,5),input("b",&b,5),input("c",&c,5)],cfg(2)).unwrap();
        // Returns at t1, t4 and t5 are all-symbol valid; t2 is missing and t3 has no predecessor for C.
        assert_eq!(report.portfolio.overlap,3);
        assert!(report.portfolio.available);
        assert_eq!(report.portfolio.covariance.as_ref().unwrap().len(),3);
    }

    #[test]
    fn common_interval_is_intersection_and_empty_intersection_fails() {
        let a=history("A",&[0.01,0.02,0.03,0.04],&[]); let b=history("B",&[0.01,0.02,0.03,0.04],&[]);
        let report=analyze(vec![Input{source_id:"a",from:T0,to:T0+5*60,history:&a},Input{source_id:"b",from:T0+60,to:T0+5*60,history:&b}],cfg(2)).unwrap();
        assert_eq!(report.from,T0+60); assert_eq!(report.minutes.first().unwrap().time,T0+60);
        let error=analyze(vec![Input{source_id:"a",from:T0,to:T0+60,history:&a},Input{source_id:"b",from:T0+60,to:T0+120,history:&b}],cfg(2)).unwrap_err();
        assert!(error.contains("no valid common"));
    }

    #[test]
    fn report_is_observational_only() {
        let a=history("A",&[0.01,0.02,0.03],&[]); let b=history("B",&[0.03,0.02,0.01],&[]);
        let report=analyze(vec![input("a",&a,3),input("b",&b,3)],cfg(2)).unwrap();
        assert!(!report.execution_effect); assert_eq!(report.algorithm,ALGORITHM); assert_eq!(report.identity_algorithm,IDENTITY_ALGORITHM);
    }
}
