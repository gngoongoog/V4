//! Deterministic M1 OHLC research. No orders, future features, or synthetic bars.
use crate::{
    research::{analyze, size_lots},
    Candle, MarketData, RiskConfig,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

pub const MAX_BARS: usize = 60_000;
pub const DAY: u64 = 86_400;
mod kte;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    #[default]
    MetaV1,
    CesV1,
    KteV1,
    MetaV2,
}
impl Strategy {
    pub fn algorithm(self) -> &'static str {
        match self {
            Self::MetaV1 => "meta-v1 / M1-OHLC-conservative-v1",
            Self::CesV1 => "ces-v1 / M1-OHLC-conservative-v1",
            Self::MetaV2 => "meta-v2-candidate-v1 / M1-OHLC-conservative-v1",
            Self::KteV1 => "kte-v1 / M1-partials-conservative-v1",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contract {
    pub point: f64,
    pub tick_size: f64,
    pub tick_loss: f64,
    pub tick_profit: f64,
    pub volume_min: f64,
    pub volume_step: f64,
    pub volume_max: f64,
}
impl Contract {
    pub fn validate(&self) -> Result<(), String> {
        if [
            self.point,
            self.tick_size,
            self.tick_loss,
            self.tick_profit,
            self.volume_min,
            self.volume_step,
            self.volume_max,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.0 || *v > 1e12)
            || self.volume_min > self.volume_max
        {
            return Err("مواصفات الرمز غير مكتملة؛ نحتاج قيمة وحجم tick وحدود اللوت من MT5".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryBar {
    #[serde(flatten)]
    pub candle: Candle,
    pub spread_points: u32,
}
impl HistoryBar {
    pub fn valid(&self, captured_ms: u64) -> bool {
        self.candle.valid()
            && self.candle.time % 60 == 0
            && self.candle.time.saturating_add(60).saturating_mul(1000) <= captured_ms
            && self.spread_points <= 100_000_000
            && self.candle.high <= 1e12
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub symbol: String,
    pub server: String,
    pub login: u64,
    pub currency: String,
    pub captured_ms: u64,
    pub contract: Contract,
    pub bars: Vec<HistoryBar>,
}
impl History {
    pub fn validate(&self) -> Result<(), String> {
        self.contract.validate()?;
        if self.symbol.is_empty()
            || self.server.is_empty()
            || self.login == 0
            || self.currency.is_empty()
        {
            return Err("هوية بيانات الاختبار غير مكتملة".into());
        }
        if self.bars.len() < 2
            || self.bars.len() > MAX_BARS
            || self.bars.iter().any(|b| {
                !b.candle.valid()
                    || b.candle.time % 60 != 0
                    || b.candle.time.saturating_add(60).saturating_mul(1000) > self.captured_ms
                    || b.spread_points > 100_000_000
                    || b.candle.high > 1e12
            })
            || self
                .bars
                .windows(2)
                .any(|w| w[0].candle.time >= w[1].candle.time)
        {
            return Err("تاريخ M1 غير صالح: شموع مكتملة ومرتبة دون تكرار مطلوبة".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    #[serde(default)]
    pub strategy: Strategy,
    pub from: u64,
    pub to: u64, // exclusive, UTC
    pub initial_equity: f64,
    pub spread_floor: f64, // price units, not points
    pub risk: RiskConfig,
}
impl Options {
    pub fn validate(&self) -> Result<(), String> {
        self.risk.validate()?;
        if self.from == 0
            || self.from % 60 != 0
            || self.to % 60 != 0
            || self.to <= self.from
            || self.to - self.from > 30 * DAY
        {
            return Err("الفترة يجب أن تكون من يوم إلى 30 يوماً وبوقت UTC صحيح".into());
        }
        if !self.initial_equity.is_finite()
            || !(1.0..=1e9).contains(&self.initial_equity)
            || !self.spread_floor.is_finite()
            || !(0.0..=100_000.0).contains(&self.spread_floor)
        {
            return Err("رأس المال الافتراضي أو افتراض السبريد غير صالح".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trade {
    pub signal_bar: u64,
    pub entry_time: u64,
    pub exit_bar: u64,
    pub exit_phase: String,
    pub side: i8,
    pub entry: f64,
    pub exit: f64,
    pub stop: f64,
    pub target: f64,
    pub lots: f64,
    pub commission: f64,
    pub net: f64,
    pub reason: String,
    pub ambiguous: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Fill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_updates: Vec<StopUpdate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fill {
    pub time: u64,
    pub phase: String,
    pub price: f64,
    pub lots: f64,
    pub commission: f64,
    pub net: f64,
    pub reason: String,
    pub ambiguous: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StopUpdate {
    /// Earliest opening time at which this stop is active.
    pub effective_time: u64,
    pub price: f64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EquityPoint {
    pub time: u64,
    pub equity: f64,
    pub balance: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub breakeven: usize,
    pub net: f64,
    pub return_percent: f64,
    pub profit_factor: Option<f64>,
    pub win_rate: Option<f64>,
    pub max_drawdown: f64,
    pub max_drawdown_percent: f64,
    pub commissions: f64,
    pub ambiguous_trades: usize,
    pub gap_exits: usize,
    pub tested_bars: usize,
    pub skipped: BTreeMap<String, usize>,
}
/// One candidate snapshot per tested opening, including minutes blocked by the ledger.
/// Features use completed bars only; outcome describes simulated entry admission.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateDecision {
    pub time: u64,
    pub signal_bar: u64,
    pub current: bool,
    pub data_reason: String,
    pub atr: Option<f64>,
    pub legacy_action: String,
    pub candidate: crate::research::MetaBrainV2,
    pub outcome: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub algorithm: String,
    pub symbol: String,
    pub server: String,
    pub login: u64,
    pub currency: String,
    pub captured_ms: u64,
    pub options: Options,
    pub contract: Contract,
    pub history_first: u64,
    pub history_last: u64,
    pub first_test_bar: u64,
    pub last_test_bar: u64,
    pub gaps: usize,
    pub metrics: Metrics,
    pub trades: Vec<Trade>,
    pub equity: Vec<EquityPoint>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<CandidateDecision>,
}
impl Report {
    pub fn summary(&self) -> String {
        let m = &self.metrics;
        format!("اختبار تاريخي / BACKTEST • {}\n{}\n{} | حساب {} | {}\nUTC epoch: {} .. {} (النهاية غير مشمولة)\nرأس المال {:.2} → {:.2} {}\nصافي النتيجة {:.2} | العائد {:.2}%\nالصفقات {} | رابحة {} | خاسرة {} | متعادلة {}\nنسبة الربح {} | معامل الربح {}\nأقصى تراجع مرصود {:.2} / {:.2}%\nعمولات {:.2} | شموع اختبار {}\nوقف وهدف بنفس الشمعة {} | خروج بسبب فجوة {}\nالمواصفات الحالية للرمز ثابتة طوال الاختبار؛ نتائج تقديرية وليست تنفيذ وسيط.",
            self.symbol,self.algorithm,self.server,self.login,self.currency,self.options.from,self.options.to,
            self.options.initial_equity,self.options.initial_equity+m.net,self.currency,m.net,m.return_percent,
            m.trades,m.wins,m.losses,m.breakeven,
            m.win_rate.map(|v|format!("{v:.2}%")).unwrap_or("غير متاحة".into()),
            m.profit_factor.map(|v|format!("{v:.2}")).unwrap_or("غير معرّف دون خسائر".into()),
            m.max_drawdown,m.max_drawdown_percent,m.commissions,m.tested_bars,m.ambiguous_trades,m.gap_exits)
    }
    pub fn text(&self) -> String {
        let rows = self
            .trades
            .iter()
            .rev()
            .take(20)
            .map(|t| {
                format!(
                    "{} | {} → {} | {:.4} lot | {:.5} → {:.5} | net {:.2} | {}{}",
                    if t.side == 1 { "BUY" } else { "SELL" },
                    t.entry_time,
                    t.exit_bar,
                    t.lots,
                    t.entry,
                    t.exit,
                    t.net,
                    t.reason,
                    if t.ambiguous { " / SL-first" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        format!("{}\n\nملاحظات البيانات:\n{}\n\nأسباب عدم الدخول (عدد الدقائق):\n{}\n\nآخر 20 صفقة — أوقات UTC epoch:\n{}\n\nالتصدير يحتوي كل الصفقات ومنحنى الرصيد والشموع لإعادة المراجعة.",self.summary(),self.warnings.join("\n"),self.metrics.skipped.iter().map(|(k,v)|format!("{k}: {v}")).collect::<Vec<_>>().join("\n"),rows)
    }
}

/// Emit only complete, aligned groups. Never fill a missing minute.
fn aggregate(bars: &[HistoryBar], seconds: u64) -> Vec<Candle> {
    let mut out = vec![];
    let mut i = 0;
    while i < bars.len() {
        let start = i;
        let bucket = bars[i].candle.time / seconds * seconds;
        while i < bars.len() && bars[i].candle.time / seconds * seconds == bucket {
            i += 1;
        }
        let group = &bars[start..i];
        if group.len() != (seconds / 60) as usize
            || group
                .iter()
                .enumerate()
                .any(|(j, b)| b.candle.time != bucket + j as u64 * 60)
        {
            continue;
        }
        out.push(Candle {
            time: bucket,
            open: group[0].candle.open,
            close: group.last().unwrap().candle.close,
            high: group
                .iter()
                .map(|b| b.candle.high)
                .fold(f64::NEG_INFINITY, f64::max),
            low: group
                .iter()
                .map(|b| b.candle.low)
                .fold(f64::INFINITY, f64::min),
            tick_volume: group
                .iter()
                .fold(0u64, |n, b| n.saturating_add(b.candle.tick_volume)),
        });
    }
    out
}
fn tail(series: &[Candle], end: usize) -> Vec<Candle> {
    series[end.saturating_sub(180)..end].to_vec()
}
fn market_at(m1: &[Candle], m5: &[Candle], m15: &[Candle], i: usize) -> MarketData {
    let time = m1[i].time;
    let end5 = m5.partition_point(|c| c.time + 300 <= time);
    let end15 = m15.partition_point(|c| c.time + 900 <= time);
    MarketData {
        captured_ms: time * 1000,
        m1: tail(m1, i),
        m5: tail(m5, end5),
        m15: tail(m15, end15),
        error: String::new(),
    }
}
#[derive(Clone, Debug)]
struct Position {
    time_limit: u64,
    side: i8,
    entry: f64,
    stop: f64,
    target: f64,
    lots: f64,
    commission: f64,
    entry_time: u64,
    signal_bar: u64,
}
impl Position {
    fn pnl(&self, exit: f64, spec: &Contract) -> f64 {
        let change = (exit - self.entry) * self.side as f64;
        change / spec.tick_size
            * self.lots
            * if change >= 0.0 {
                spec.tick_profit
            } else {
                spec.tick_loss
            }
            - self.commission
    }
}
// Returns price *before* assumed exit slippage, reason, ambiguous flag, phase.
fn exit_at(
    p: &Position,
    c: &Candle,
    spread: f64,
    gap: bool,
    smart_exit: bool,
) -> Option<(f64, &'static str, bool, &'static str)> {
    let adjust = if p.side == -1 { spread } else { 0.0 };
    let open = c.open + adjust;
    let high = c.high + adjust;
    let low = c.low + adjust;
    if gap {
        return Some((open, "data_gap", false, "open"));
    }
    if (p.side == 1 && open <= p.stop) || (p.side == -1 && open >= p.stop) {
        return Some((open, "stop_gap", false, "open"));
    }
    if (p.side == 1 && open >= p.target) || (p.side == -1 && open <= p.target) {
        return Some((p.target, "target", false, "open"));
    }
    if c.time.saturating_sub(p.entry_time) >= p.time_limit {
        return Some((open, "time_limit", false, "open"));
    }
    if smart_exit {
        return Some((open, "ces_eg_positive", false, "open"));
    }
    let stop = if p.side == 1 {
        low <= p.stop
    } else {
        high >= p.stop
    };
    let target = if p.side == 1 {
        high >= p.target
    } else {
        low <= p.target
    };
    if stop {
        Some((p.stop, "stop", target, "range"))
    } else if target {
        Some((p.target, "target", false, "range"))
    } else {
        None
    }
}
struct Ledger {
    balance: f64,
    peak: f64,
    day: u64,
    day_start: f64,
    daily_locked: bool,
    dd_locked: bool,
    metrics: Metrics,
    trades: Vec<Trade>,
}
impl Ledger {
    fn mark(&mut self, equity: f64, time: u64, risk: &RiskConfig, new_day: bool) {
        let day = (time + 10800) / DAY;
        if new_day && day > self.day {
            self.day = day;
            self.day_start = equity;
            self.daily_locked = false;
        }
        self.peak = self.peak.max(equity);
        let dd = (self.peak - equity).max(0.0);
        let dd_pct = dd / self.peak * 100.0;
        self.metrics.max_drawdown = self.metrics.max_drawdown.max(dd);
        self.metrics.max_drawdown_percent = self.metrics.max_drawdown_percent.max(dd_pct);
        if equity <= self.day_start * (1.0 - risk.daily_loss_percent / 100.0) {
            self.daily_locked = true;
        }
        if dd_pct >= risk.max_drawdown_percent {
            self.dd_locked = true;
        }
    }
    fn close(
        &mut self,
        p: Position,
        price: f64,
        time: u64,
        phase: &str,
        reason: &str,
        ambiguous: bool,
        spec: &Contract,
        slip: f64,
    ) {
        let exit = price - p.side as f64 * slip;
        let net = p.pnl(exit, spec);
        self.balance += net;
        self.trades.push(Trade {
            signal_bar: p.signal_bar,
            entry_time: p.entry_time,
            exit_bar: time,
            exit_phase: phase.into(),
            side: p.side,
            entry: p.entry,
            exit,
            stop: p.stop,
            target: p.target,
            lots: p.lots,
            commission: p.commission,
            net,
            reason: reason.into(),
            ambiguous,
            fills: vec![],
            stop_updates: vec![],
            targets: vec![],
        });
    }
    fn skip<'a>(&mut self, reason: &'a str) -> &'a str {
        *self.metrics.skipped.entry(reason.into()).or_default() += 1;
        reason
    }
}
pub fn run(
    history: &History,
    options: &Options,
    cancel: &AtomicBool,
    progress: &AtomicUsize,
) -> Result<Report, String> {
    history.validate()?;
    options.validate()?;
    if options.to.saturating_mul(1000) > history.captured_ms {
        return Err("نهاية الاختبار تتجاوز وقت جمع البيانات".into());
    }
    if options.strategy == Strategy::KteV1 {
        return kte::run(history, options, cancel, progress);
    }
    let spec = &history.contract;
    let risk = &options.risk;
    let slip = risk.slippage_ticks * spec.tick_size;
    let m1: Vec<_> = history.bars.iter().map(|b| b.candle.clone()).collect();
    let ces = if options.strategy == Strategy::CesV1 {
        crate::ces::frames(&m1)?
    } else {
        vec![]
    };
    let m5 = aggregate(&history.bars, 300);
    let m15 = aggregate(&history.bars, 900);
    let indexes: Vec<_> = (1..m1.len())
        .filter(|i| m1[*i].time >= options.from && m1[*i].time + 60 <= options.to)
        .collect();
    if indexes.is_empty() {
        return Err("لا توجد شموع مكتملة ضمن الفترة المطلوبة".into());
    }
    let first = m1[*indexes.first().unwrap()].time;
    let last = m1[*indexes.last().unwrap()].time;
    let mut ledger = Ledger {
        balance: options.initial_equity,
        peak: options.initial_equity,
        day: 0,
        day_start: options.initial_equity,
        daily_locked: false,
        dd_locked: false,
        metrics: Metrics::default(),
        trades: vec![],
    };
    let mut position: Option<Position> = None;
    let mut equity = vec![];
    let mut gaps = 0;
    let mut last_spread = 0.0;
    let mut decisions = Vec::new();
    for (step, &i) in indexes.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("تم إلغاء الاختبار".into());
        }
        progress.store(step * 100 / indexes.len(), Ordering::Relaxed);
        let c = &m1[i];
        let previous = &m1[i - 1];
        let candidate_analysis = (options.strategy == Strategy::MetaV2).then(|| {
            analyze(Some(&market_at(&m1, &m5, &m15, i)), &history.symbol, c.time * 1000)
        });
        let mut outcome = "entered";
        let gap = c.time != previous.time + 60;
        if gap {
            gaps += 1;
        }
        // Only the PREVIOUS completed candle supplies historical spread.
        let spread = options
            .spread_floor
            .max(history.bars[i - 1].spread_points as f64 * spec.point);
        last_spread = spread;
        let marked = ledger.balance
            + position
                .as_ref()
                .map(|p| {
                    p.pnl(
                        if p.side == 1 {
                            c.open - slip
                        } else {
                            c.open + spread + slip
                        },
                        spec,
                    )
                })
                .unwrap_or(0.0);
        ledger.mark(marked, c.time, risk, true);
        let mut exited = false;
        if let Some(p) = position.as_ref() {
            let smart = ces
                .get(i - 1)
                .and_then(Option::as_ref)
                .is_some_and(|f| f.eg > 0.0);
            if let Some((price, reason, ambiguous, phase)) = exit_at(p, c, spread, gap, smart) {
                ledger.close(
                    position.take().unwrap(),
                    price,
                    c.time,
                    phase,
                    reason,
                    ambiguous,
                    spec,
                    slip,
                );
                exited = true;
                ledger.mark(ledger.balance, c.time, risk, false);
            }
        }
        if position.is_some() {
            outcome = ledger.skip("صفقة مفتوحة");
        } else if exited {
            outcome = ledger.skip("خروج في نفس الدقيقة؛ لا إعادة دخول");
        } else if gap {
            outcome = ledger.skip("فجوة قبل الشمعة");
        } else if ledger.daily_locked || ledger.dd_locked || ledger.balance <= 0.0 {
            outcome = ledger.skip("حد خسارة أو تراجع");
        } else {
            let (current, side, atr, stop_distance, ces_target) =
                if options.strategy == Strategy::CesV1 {
                    match ces[i - 1].as_ref() {
                        Some(f) => (
                            true,
                            f.side,
                            f.atr,
                            0.5 * f.pressure_range,
                            Some(1.5 * f.pressure_range),
                        ),
                        None => (false, 0, 0.0, 0.0, None),
                    }
                } else if let Some(a) = candidate_analysis.as_ref() {
                    let atr = a.atr.unwrap_or(0.0);
                    let candidate = &a.regime_brain.meta_v2;
                    (a.current, if candidate.eligible { candidate.direction } else { 0 }, atr, 1.5 * atr, None)
                } else {
                    let a = analyze(
                        Some(&market_at(&m1, &m5, &m15, i)),
                        &history.symbol,
                        c.time * 1000,
                    );
                    let atr = a.atr.unwrap_or(0.0);
                    (
                        a.current,
                        if a.action == "BUY" {
                            1
                        } else if a.action == "SELL" {
                            -1
                        } else {
                            0
                        },
                        atr,
                        1.5 * atr,
                        None,
                    )
                };
            if !current {
                outcome = ledger.skip("إحماء أو بيانات ناقصة");
            } else if side == 0 {
                outcome = ledger.skip(if options.strategy == Strategy::MetaV1 {
                    "عدم اتفاق العقول"
                } else if options.strategy == Strategy::MetaV2 {
                    "لم تجتز بوابات Meta v2"
                } else {
                    "لم تتحقق شروط CES"
                });
            } else {
                if spread > atr * risk.max_spread_atr {
                    outcome = ledger.skip("سبريد مرتفع");
                } else if (c.open - previous.close).abs() > 2.0 * atr {
                    outcome = ledger.skip("قفزة سعرية");
                } else {
                    let entry = if side == 1 {
                        c.open + spread + slip
                    } else {
                        c.open - slip
                    };
                    let stop = if side == 1 {
                        ((entry - stop_distance) / spec.tick_size).floor() * spec.tick_size
                    } else {
                        ((entry + stop_distance) / spec.tick_size).ceil() * spec.tick_size
                    };
                    let distance = (entry - stop).abs();
                    let target = entry + side as f64 * ces_target.unwrap_or(2.0 * distance);
                    let budget = (ledger.balance * risk.risk_percent / 100.0)
                        .min(
                            (ledger.balance
                                - ledger.day_start * (1.0 - risk.daily_loss_percent / 100.0))
                                .max(0.0),
                        )
                        .min(
                            (ledger.balance
                                - ledger.peak * (1.0 - risk.max_drawdown_percent / 100.0))
                                .max(0.0),
                        );
                    let loss_per_lot = (distance + slip) / spec.tick_size
                        * spec.tick_loss.max(spec.tick_profit)
                        + risk.commission_per_lot;
                    if entry <= 0.0 || stop <= 0.0 || target <= 0.0 {
                        outcome = ledger.skip("أسعار الوقف أو الهدف غير صالحة");
                    } else if let Some(lots) = size_lots(
                        budget,
                        loss_per_lot,
                        spec.volume_min,
                        spec.volume_step,
                        risk.max_lots.min(spec.volume_max),
                    ) {
                        let p = Position {
                            time_limit: if options.strategy == Strategy::CesV1 {
                                8 * 60
                            } else {
                                30 * 60
                            },
                            side,
                            entry,
                            stop,
                            target,
                            lots,
                            commission: risk.commission_per_lot * lots,
                            entry_time: c.time,
                            signal_bar: previous.time,
                        };
                        ledger.mark(
                            ledger.balance
                                + p.pnl(
                                    if side == 1 {
                                        c.open - slip
                                    } else {
                                        c.open + spread + slip
                                    },
                                    spec,
                                ),
                            c.time,
                            risk,
                            false,
                        );
                        if let Some((price, reason, ambiguous, phase)) =
                            exit_at(&p, c, spread, false, false)
                        {
                            ledger.close(p, price, c.time, phase, reason, ambiguous, spec, slip);
                        } else {
                            position = Some(p);
                        }
                    } else {
                        outcome = ledger.skip("اللوت الأدنى يتجاوز الميزانية");
                    }
                }
            }
        }
        if let Some(a) = candidate_analysis {
            decisions.push(CandidateDecision {
                time: c.time,
                signal_bar: previous.time,
                current: a.current,
                data_reason: if a.current { String::new() } else { a.reason },
                atr: a.atr,
                legacy_action: a.action,
                candidate: a.regime_brain.meta_v2,
                outcome: outcome.into(),
            });
        }
        let marked = ledger.balance
            + position
                .as_ref()
                .map(|p| {
                    p.pnl(
                        if p.side == 1 {
                            c.close - slip
                        } else {
                            c.close + spread + slip
                        },
                        spec,
                    )
                })
                .unwrap_or(0.0);
        if !marked.is_finite() {
            return Err("نتيجة حسابية غير محدودة؛ راجع مواصفات الرمز".into());
        }
        ledger.mark(marked, c.time, risk, false);
        equity.push(EquityPoint {
            time: c.time + 60,
            equity: marked,
            balance: ledger.balance,
        });
    }
    if let Some(p) = position.take() {
        let c = &m1[*indexes.last().unwrap()];
        let price = if p.side == 1 {
            c.close
        } else {
            c.close + last_spread
        };
        ledger.close(
            p,
            price,
            c.time,
            "final_close",
            "end_of_test",
            false,
            spec,
            slip,
        );
        ledger.mark(ledger.balance, c.time, risk, false);
        if let Some(point) = equity.last_mut() {
            point.equity = ledger.balance;
            point.balance = ledger.balance;
        }
    }
    let mut report = finish(history, options, ledger, equity, first, last, indexes.len(), gaps, progress)?;
    report.decisions = decisions;
    Ok(report)
}
fn finish(history: &History, options: &Options, ledger: Ledger, equity: Vec<EquityPoint>, first: u64, last: u64, tested_bars: usize, gaps: usize, progress: &AtomicUsize) -> Result<Report, String> {
    let mut m = ledger.metrics;
    m.tested_bars = tested_bars;
    m.trades = ledger.trades.len();
    m.wins = ledger.trades.iter().filter(|t| t.net > 1e-9).count();
    m.losses = ledger.trades.iter().filter(|t| t.net < -1e-9).count();
    m.breakeven = m.trades - m.wins - m.losses;
    m.net = ledger.balance - options.initial_equity;
    m.return_percent = m.net / options.initial_equity * 100.0;
    m.win_rate = if m.trades > 0 {
        Some(m.wins as f64 / m.trades as f64 * 100.0)
    } else {
        None
    };
    let gross_profit = ledger.trades.iter().map(|t| t.net.max(0.0)).sum::<f64>();
    let gross_loss = ledger.trades.iter().map(|t| (-t.net).max(0.0)).sum::<f64>();
    m.profit_factor = if gross_loss > 1e-9 {
        Some(gross_profit / gross_loss)
    } else {
        None
    };
    m.commissions = ledger.trades.iter().map(|t| t.commission).sum();
    m.ambiguous_trades = ledger.trades.iter().filter(|t| t.ambiguous).count();
    m.gap_exits = ledger
        .trades
        .iter()
        .filter(|t| t.reason == "data_gap")
        .count();
    let mut warnings=vec![
        "شموع M1 بنموذج bid؛ ASK مقدّر بسبريد ثابت لكل دقيقة، وليس شريط ticks.".into(),
        "السبريد = الأكبر بين الافتراض وسبريد الدقيقة المكتملة السابقة. العمولة والانزلاق من إعدادات المخاطرة المجمدة.".into(),
        "تُحسب الإشارة قبل افتتاح دقيقة الدخول. عند لمس الوقف والهدف بنفس الدقيقة يُحسب الوقف أولاً.".into(),
        "قيمة tick ومواصفات العقد الحالية ثابتة؛ تحويل العملة التاريخي والهامش والسواب والتنفيذ الحقيقي غير ممثلة.".into(),
        "التراجع محسوب عند الافتتاح والإغلاق والتنفيذ، ولا يقيس كل حركة داخل الدقيقة.".into(),
        "لا تحسين تلقائي للمعلمات ولا اختبار خارج العينة؛ لا تُفسر النتيجة كاحتمال ربح مستقبلي.".into()];
    if first > options.from || last + 60 < options.to {
        warnings.push("البيانات لا تغطي كل طرفي الفترة؛ قد يكون السبب إغلاق السوق أو محدودية تاريخ الوسيط. راجع أول وآخر شمعة.".into());
    }
    if gaps > 0 {
        warnings.push(format!("فجوات زمنية: {gaps}. لا تُملأ الشموع الناقصة؛ الصفقة المفتوحة تُغلق عند أول افتتاح عائد وتُعلّم data_gap."));
    }
    if m.trades == 0 {
        warnings.push("لم تتكون صفقات. راجع عدد شموع الإحماء وأسباب عدم الدخول؛ هذه ليست نتيجة نجاح أو فشل للاستراتيجية.".into());
    }
    if options.strategy == Strategy::CesV1 {
        warnings.push("CES يستخدم حجم ticks وATR20 من الإحماء المتاح؛ وقف 0.5 وهدف 1.5 من TR3 وخروج EG موجب أو 8 دقائق. الدخول بالافتتاح التالي يختلف عن Pine process_orders_on_close؛ لا ادعاء تطابق النتائج.".into());
    }
    if options.strategy == Strategy::MetaV2 {
        warnings.push("Meta v2 candidate v1: بوابات الاتجاه والنظام والبنية والزخم والتذبذب؛ سجل decisions يحفظ مرشح كل دقيقة وسبب منع الدخول. الوقف 1.5 ATR والهدف 2R والخروج 30 دقيقة مثل Meta v1؛ المحاكاة المباشرة تبقى Meta v1.".into());
    }
    if options.strategy == Strategy::KteV1 {
        warnings.push("KTE: أهداف 1R/2R/3R بنسبة 40/30/الباقي بعد تقريب اللوت للأسفل؛ تُرفض الصفقة إن تعذر تقسيمها إلى ثلاثة أحجام صالحة. الوقف عند الدخول بعد TP1 والتتبع بعد TP2 يُفعّلان بالدقيقة التالية ولا يتراجع الوقف. خروج 45/90 دقيقة وتهدئة 3 شموع بعد الإغلاق الكامل.".into());
        warnings.push("KTE يستخدم الافتراضات الأصلية دون فلتر جلسة أو ADX. إحماء 100 شمعة؛ الاندفاع يُلغى بعد فجوة. الدخول بالافتتاح التالي وحفظ الحالة حتى الإغلاق الكامل يختلفان عن Pine؛ لا ادعاء تطابق TradingView.".into());
    }
    progress.store(100, Ordering::Relaxed);
    Ok(Report {
        algorithm: options.strategy.algorithm().into(),
        symbol: history.symbol.clone(),
        server: history.server.clone(),
        login: history.login,
        currency: history.currency.clone(),
        captured_ms: history.captured_ms,
        options: options.clone(),
        contract: history.contract.clone(),
        history_first: history.bars[0].candle.time,
        history_last: history.bars.last().unwrap().candle.time,
        first_test_bar: first,
        last_test_bar: last,
        gaps,
        metrics: m,
        trades: ledger.trades,
        equity,
        warnings,
        decisions: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: u64 = 1_728_000_000;
    fn fixture(count: usize) -> History {
        History {
            symbol: "TEST".into(),
            server: "FIXTURE".into(),
            login: 1,
            currency: "USD".into(),
            captured_ms: (BASE + count as u64 * 60) * 1000,
            contract: Contract {
                point: 0.01,
                tick_size: 0.01,
                tick_loss: 1.0,
                tick_profit: 1.0,
                volume_min: 0.01,
                volume_step: 0.01,
                volume_max: 10.0,
            },
            bars: (0..count)
                .map(|i| {
                    let price = 2000.0 + i as f64 * 0.1;
                    HistoryBar {
                        candle: Candle {
                            time: BASE + i as u64 * 60,
                            open: price,
                            high: price + 0.3,
                            low: price - 0.1,
                            close: price + 0.08,
                            tick_volume: 100,
                        },
                        spread_points: 2,
                    }
                })
                .collect(),
        }
    }
    fn options(count: usize) -> Options {
        Options {
            strategy: Strategy::MetaV1,
            from: BASE + 1600 * 60,
            to: BASE + count as u64 * 60,
            initial_equity: 10000.0,
            spread_floor: 0.02,
            risk: RiskConfig::default(),
        }
    }
    fn execute(h: &History, o: &Options) -> Report {
        run(h, o, &AtomicBool::new(false), &AtomicUsize::new(0)).unwrap()
    }
    #[test]
    fn meta_v2_entries_reconcile_and_legacy_report_shape_stays_unchanged() {
        let h = fixture(1800);
        let mut o = options(1800);
        let legacy = execute(&h, &o);
        assert!(!serde_json::to_value(&legacy).unwrap().as_object().unwrap().contains_key("decisions"));
        o.strategy = Strategy::MetaV2;
        let r = execute(&h, &o);
        assert_eq!(r.algorithm, "meta-v2-candidate-v1 / M1-OHLC-conservative-v1");
        assert!(r.metrics.trades > 0);
        assert_eq!(r.decisions.len(), r.metrics.tested_bars);
        assert_eq!(r.decisions.iter().filter(|d| d.outcome == "entered").count(), r.trades.len());
        assert_eq!(serde_json::to_value(&r.trades).unwrap(), serde_json::to_value(&legacy.trades).unwrap());
        assert_eq!(serde_json::to_value(&r).unwrap(), serde_json::to_value(execute(&h, &o)).unwrap());
        for t in &r.trades {
            let d = r.decisions.iter().find(|d| d.time == t.entry_time).unwrap();
            assert!(d.current && d.candidate.eligible && d.candidate.gates.is_empty());
            assert_eq!(d.candidate.direction, t.side);
            assert_eq!(d.signal_bar + 60, d.time);
            assert_eq!(d.outcome, "entered");
            let budget = ((t.entry-t.stop).abs() + o.risk.slippage_ticks*h.contract.tick_size)
                / h.contract.tick_size*h.contract.tick_loss*t.lots+t.commission;
            assert!(budget <= o.initial_equity*o.risk.risk_percent/100.0+1e-8);
        }
        assert!((r.metrics.net-r.trades.iter().map(|t|t.net).sum::<f64>()).abs()<1e-7);
    }
    #[test]
    fn meta_v2_decision_prefix_is_stable_when_future_prices_and_spreads_change() {
        let h = fixture(1800);
        let mut o = options(1800);
        o.strategy = Strategy::MetaV2;
        let original = execute(&h, &o);
        let mut future = h.clone();
        for b in &mut future.bars[1700..] {
            b.candle.open += 500.0; b.candle.high += 500.0;
            b.candle.low += 500.0; b.candle.close += 500.0;
            b.spread_points = 99999;
        }
        let changed = execute(&future, &o);
        assert_eq!(serde_json::to_value(&original.decisions[..100]).unwrap(),
                   serde_json::to_value(&changed.decisions[..100]).unwrap());
        let mut prefix = h.clone();
        prefix.bars.truncate(1700);
        o.to = BASE + 1700*60;
        let short = execute(&prefix, &o);
        assert_eq!(serde_json::to_value(&original.decisions[..100]).unwrap(),
                   serde_json::to_value(&short.decisions).unwrap());
    }
    #[test]
    fn meta_v2_missing_stale_and_gap_data_never_admit_entries() {
        let mut h = fixture(1800);
        let mut o = options(1800);
        o.strategy = Strategy::MetaV2;
        o.from = BASE + 60;
        let r = execute(&h, &o);
        assert!(r.decisions.iter().take(1400).all(|d| !d.current && d.outcome != "entered"));
        o.from = BASE+1600*60;
        h.bars.remove(1601); // the first admitted position must close at next observed open
        let r = execute(&h, &o);
        assert_eq!(r.trades[0].reason, "data_gap");
        assert_eq!(r.trades[0].exit_bar, BASE+1602*60);
        assert_ne!(r.decisions[1].outcome, "entered");
        assert!(r.decisions.iter().skip(2).all(|d| !d.current && d.outcome != "entered"));
        let m1: Vec<_> = h.bars.iter().map(|b| b.candle.clone()).collect();
        let mut market = market_at(&m1, &aggregate(&h.bars,300), &aggregate(&h.bars,900),1600);
        market.m15.clear();
        assert!(!analyze(Some(&market),"TEST",market.captured_ms).regime_brain.meta_v2.eligible);
        market = market_at(&m1, &aggregate(&h.bars,300), &aggregate(&h.bars,900),1600);
        assert!(!analyze(Some(&market),"TEST",market.captured_ms+20_000).current);
    }
    #[test]
    fn meta_v2_shock_and_execution_vetoes_are_saved_separately() {
        let mut h = fixture(1800);
        let mut o = options(1800); o.strategy = Strategy::MetaV2;
        h.bars[1599].candle.high += 10.0;
        h.bars[1599].candle.low -= 10.0;
        let r = execute(&h,&o);
        assert!(r.decisions[0].candidate.gates.iter().any(|s|s=="Volatility shock veto"));
        assert_ne!(r.decisions[0].outcome,"entered");
        h = fixture(1800);
        o.spread_floor = 10.0;
        let r = execute(&h,&o);
        assert_eq!(r.metrics.trades,0);
        assert!(r.decisions.iter().all(|d|d.candidate.eligible && d.outcome=="سبريد مرتفع"));
        o.spread_floor = 0.02;
        o.initial_equity = 1.0;
        let r = execute(&h,&o);
        assert_eq!(r.metrics.trades,0);
        assert!(r.decisions.iter().all(|d|d.outcome=="اللوت الأدنى يتجاوز الميزانية"));
    }
    #[test]
    fn meta_v2_gap_loss_latches_risk_and_stops_new_entries() {
        let mut h = fixture(1800);
        let mut o = options(1800); o.strategy = Strategy::MetaV2;
        for b in &mut h.bars[1601..] {
            b.candle.open -= 100.0; b.candle.high -= 100.0;
            b.candle.low -= 100.0; b.candle.close -= 100.0;
        }
        let r = execute(&h,&o);
        assert_eq!(r.trades.len(),1);
        assert_eq!(r.trades[0].reason,"stop_gap");
        assert!(r.metrics.max_drawdown_percent > o.risk.max_drawdown_percent);
        assert!(r.decisions.iter().skip(2).all(|d|d.outcome=="حد خسارة أو تراجع"));
    }
    #[test]
    fn ces_next_open_entry_costs_smart_exit_and_eight_minute_limit() {
        let mut h = fixture(150);
        h.bars = crate::ces::tests::pressure_fixture()
            .into_iter()
            .map(|candle| HistoryBar {
                candle,
                spread_points: 2,
            })
            .collect();
        for b in &mut h.bars[138..] {
            b.candle.high = 2000.03;
            b.candle.low = 1999.99;
            b.candle.close = 2000.01;
        }
        let mut o = options(150);
        o.from = BASE + 100 * 60;
        o.strategy = Strategy::CesV1;
        let r = execute(&h, &o);
        assert_eq!(r.trades.len(), 1);
        let t = &r.trades[0];
        assert_eq!(t.signal_bar, BASE + 137 * 60);
        assert_eq!(t.entry_time, BASE + 138 * 60);
        assert_eq!(t.reason, "ces_eg_positive");
        assert_eq!(t.exit_phase, "open");
        assert!((t.entry - 2000.04).abs() < 1e-8);
        assert!((t.commission - t.lots * 7.0).abs() < 1e-8);
        assert!((t.net - r.metrics.net).abs() < 1e-8);
        let p = Position {
            time_limit: 480,
            side: 1,
            entry: 100.0,
            stop: 99.0,
            target: 102.0,
            lots: 0.1,
            commission: 0.7,
            entry_time: BASE,
            signal_bar: BASE - 60,
        };
        let mut c = Candle {
            time: BASE + 479,
            open: 100.0,
            high: 100.1,
            low: 99.9,
            close: 100.0,
            tick_volume: 1,
        };
        assert!(exit_at(&p, &c, 0.2, false, false).is_none());
        c.time = BASE + 480;
        assert_eq!(
            exit_at(&p, &c, 0.2, false, false),
            Some((100.0, "time_limit", false, "open"))
        );
    }
    #[test]
    fn complete_aggregation_never_fills_holes() {
        let mut h = fixture(30);
        assert_eq!(aggregate(&h.bars, 900).len(), 2);
        h.bars.remove(7);
        let a = aggregate(&h.bars, 900);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].time, BASE + 900);
    }
    #[test]
    fn future_candles_cannot_change_a_signal() {
        let h = fixture(1800);
        let m1 = h.bars.iter().map(|b| b.candle.clone()).collect::<Vec<_>>();
        let m5 = aggregate(&h.bars, 300);
        let m15 = aggregate(&h.bars, 900);
        let before = analyze(
            Some(&market_at(&m1, &m5, &m15, 1650)),
            "TEST",
            m1[1650].time * 1000,
        );
        let mut future = h.clone();
        for b in &mut future.bars[1650..] {
            b.candle.open = 1e6;
            b.candle.high = 1e6 + 100.0;
            b.candle.low = 1e6 - 100.0;
            b.candle.close = 1e6;
            b.spread_points = 99999;
        }
        let changed = future
            .bars
            .iter()
            .map(|b| b.candle.clone())
            .collect::<Vec<_>>();
        let after = analyze(
            Some(&market_at(
                &changed,
                &aggregate(&future.bars, 300),
                &aggregate(&future.bars, 900),
                1650,
            )),
            "TEST",
            changed[1650].time * 1000,
        );
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
    }
    #[test]
    fn conservative_exit_order_and_sell_ask() {
        let p = Position {
            time_limit: 1800,
            side: 1,
            entry: 100.0,
            stop: 99.0,
            target: 102.0,
            lots: 1.0,
            commission: 7.0,
            entry_time: BASE,
            signal_bar: BASE - 60,
        };
        let mut c = Candle {
            time: BASE,
            open: 100.0,
            high: 103.0,
            low: 98.0,
            close: 100.0,
            tick_volume: 1,
        };
        assert_eq!(
            exit_at(&p, &c, 0.2, false, false),
            Some((99.0, "stop", true, "range"))
        );
        c.open = 97.0;
        assert_eq!(
            exit_at(&p, &c, 0.2, false, false),
            Some((97.0, "stop_gap", false, "open"))
        );
        let sell = Position {
            side: -1,
            stop: 101.0,
            target: 98.0,
            ..p
        };
        c.open = 100.0;
        c.high = 100.9;
        c.low = 99.0;
        assert_eq!(
            exit_at(&sell, &c, 0.2, false, false),
            Some((101.0, "stop", false, "range"))
        );
    }
    #[test]
    fn simulation_reconciles_costs_equity_and_no_overlaps() {
        let h = fixture(2200);
        let o = options(2200);
        let r = execute(&h, &o);
        assert!(r.metrics.trades > 0);
        assert!((r.metrics.net - r.trades.iter().map(|t| t.net).sum::<f64>()).abs() < 1e-7);
        assert!((r.equity.last().unwrap().equity - o.initial_equity - r.metrics.net).abs() < 1e-7);
        assert!(r.trades.windows(2).all(|w| w[0].exit_bar < w[1].entry_time));
        for t in &r.trades {
            assert_eq!(t.signal_bar + 60, t.entry_time);
            assert!((t.commission - t.lots * o.risk.commission_per_lot).abs() < 1e-9);
            let change = (t.exit - t.entry) * t.side as f64;
            assert!((t.net - (change / h.contract.tick_size * t.lots - t.commission)).abs() < 1e-8);
        }
    }
    #[test]
    fn warmup_and_cancellation_are_explicit() {
        let h = fixture(150);
        let mut o = options(2200);
        o.from = BASE + 60;
        o.to = BASE + 150 * 60;
        let r = execute(&h, &o);
        assert_eq!(r.metrics.trades, 0);
        assert_eq!(r.metrics.win_rate, None);
        assert!(run(&h, &o, &AtomicBool::new(true), &AtomicUsize::new(0)).is_err());
    }
    #[test]
    fn duplicate_future_and_invalid_specs_are_rejected() {
        let mut h = fixture(1800);
        h.bars[1].candle.time = BASE;
        assert!(h.validate().is_err());
        h = fixture(1800);
        h.captured_ms -= 60_000;
        assert!(h.validate().is_err());
        h = fixture(1800);
        h.contract.tick_loss = f64::NAN;
        assert!(h.validate().is_err());
    }
    #[test]
    fn drawdown_latches_and_daily_lock_resets_only_next_day() {
        let risk = RiskConfig::default();
        let mut l = Ledger {
            balance: 1000.0,
            peak: 1000.0,
            day: 0,
            day_start: 1000.0,
            daily_locked: false,
            dd_locked: false,
            metrics: Metrics::default(),
            trades: vec![],
        };
        l.mark(1000.0, BASE, &risk, true);
        l.mark(940.0, BASE + 60, &risk, false);
        assert!(l.daily_locked && l.dd_locked);
        l.mark(1000.0, BASE + 120, &risk, true);
        assert!(l.daily_locked && l.dd_locked);
        l.mark(1000.0, BASE + DAY, &risk, true);
        assert!(!l.daily_locked && l.dd_locked);
    }
}
