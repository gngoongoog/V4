//! Cost-aware identical-history comparison for legacy Meta v1 vs observational Meta v2.
//! Research only: this module never submits broker orders and does not promote Meta v2.
use crate::{
    backtest::{self, EquityPoint, History, Metrics, Options, Strategy, Trade, DAY},
    research::{analyze, size_lots},
    Candle, MarketData,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateReport {
    pub algorithm: String,
    pub metrics: Metrics,
    pub trades: Vec<Trade>,
    pub equity: Vec<EquityPoint>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delta {
    pub net: f64,
    pub return_percent: f64,
    pub max_drawdown: f64,
    pub max_drawdown_percent: f64,
    pub trades: i64,
    pub wins: i64,
    pub losses: i64,
    pub commissions: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comparison {
    pub baseline_algorithm: String,
    pub candidate_algorithm: String,
    pub symbol: String,
    pub server: String,
    pub login: u64,
    pub from: u64,
    pub to: u64,
    pub baseline: Metrics,
    pub candidate: Metrics,
    pub delta: Delta,
    pub warnings: Vec<String>,
}
impl Comparison {
    pub fn summary(&self) -> String {
        format!(
            "META A/B — نفس البيانات والتكاليف\n{} vs {}\n{} / {} / {} | UTC {}..{}\nBaseline: net {:.2}, return {:.2}%, DD {:.2}%, trades {}\nCandidate: net {:.2}, return {:.2}%, DD {:.2}%, trades {}\nDelta candidate-baseline: net {:+.2}, return {:+.2}%, DD {:+.2}%, trades {:+}\nهذه مقارنة بحثية داخل نفس نموذج الشموع والتكاليف، وليست دليلاً على ربحية مستقبلية.",
            self.baseline_algorithm,
            self.candidate_algorithm,
            self.symbol,
            self.server,
            self.login,
            self.from,
            self.to,
            self.baseline.net,
            self.baseline.return_percent,
            self.baseline.max_drawdown_percent,
            self.baseline.trades,
            self.candidate.net,
            self.candidate.return_percent,
            self.candidate.max_drawdown_percent,
            self.candidate.trades,
            self.delta.net,
            self.delta.return_percent,
            self.delta.max_drawdown_percent,
            self.delta.trades,
        )
    }
}

#[derive(Clone, Debug)]
struct Position {
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
    fn pnl(&self, exit: f64, history: &History) -> f64 {
        let spec = &history.contract;
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

struct Ledger {
    balance: f64,
    peak: f64,
    day: u64,
    day_start: f64,
    daily_locked: bool,
    drawdown_locked: bool,
    metrics: Metrics,
    trades: Vec<Trade>,
}
impl Ledger {
    fn new(initial: f64) -> Self {
        Self {
            balance: initial,
            peak: initial,
            day: 0,
            day_start: initial,
            daily_locked: false,
            drawdown_locked: false,
            metrics: Metrics::default(),
            trades: vec![],
        }
    }
    fn mark(&mut self, equity: f64, time: u64, options: &Options, new_day: bool) {
        let day = (time + 10_800) / DAY;
        if new_day && day > self.day {
            self.day = day;
            self.day_start = equity;
            self.daily_locked = false;
        }
        self.peak = self.peak.max(equity);
        let drawdown = (self.peak - equity).max(0.0);
        let drawdown_percent = if self.peak > 0.0 {
            drawdown / self.peak * 100.0
        } else {
            0.0
        };
        self.metrics.max_drawdown = self.metrics.max_drawdown.max(drawdown);
        self.metrics.max_drawdown_percent =
            self.metrics.max_drawdown_percent.max(drawdown_percent);
        if equity
            <= self.day_start * (1.0 - options.risk.daily_loss_percent / 100.0)
        {
            self.daily_locked = true;
        }
        if drawdown_percent >= options.risk.max_drawdown_percent {
            self.drawdown_locked = true;
        }
    }
    fn skip(&mut self, reason: &str) {
        *self.metrics.skipped.entry(reason.into()).or_default() += 1;
    }
    fn close(
        &mut self,
        position: Position,
        raw_price: f64,
        time: u64,
        phase: &str,
        reason: &str,
        ambiguous: bool,
        history: &History,
        slip: f64,
    ) {
        let exit = raw_price - position.side as f64 * slip;
        let net = position.pnl(exit, history);
        self.balance += net;
        self.trades.push(Trade {
            signal_bar: position.signal_bar,
            entry_time: position.entry_time,
            exit_bar: time,
            exit_phase: phase.into(),
            side: position.side,
            entry: position.entry,
            exit,
            stop: position.stop,
            target: position.target,
            lots: position.lots,
            commission: position.commission,
            net,
            reason: reason.into(),
            ambiguous,
            fills: vec![],
            stop_updates: vec![],
            targets: vec![],
        });
    }
}

fn aggregate(history: &History, seconds: u64) -> Vec<Candle> {
    let bars = &history.bars;
    let mut out = Vec::new();
    let mut index = 0;
    while index < bars.len() {
        let start = index;
        let bucket = bars[index].candle.time / seconds * seconds;
        while index < bars.len() && bars[index].candle.time / seconds * seconds == bucket {
            index += 1;
        }
        let group = &bars[start..index];
        if group.len() != (seconds / 60) as usize
            || group
                .iter()
                .enumerate()
                .any(|(offset, bar)| bar.candle.time != bucket + offset as u64 * 60)
        {
            continue;
        }
        out.push(Candle {
            time: bucket,
            open: group[0].candle.open,
            high: group
                .iter()
                .map(|bar| bar.candle.high)
                .fold(f64::NEG_INFINITY, f64::max),
            low: group
                .iter()
                .map(|bar| bar.candle.low)
                .fold(f64::INFINITY, f64::min),
            close: group.last().unwrap().candle.close,
            tick_volume: group
                .iter()
                .fold(0u64, |value, bar| value.saturating_add(bar.candle.tick_volume)),
        });
    }
    out
}
fn tail(series: &[Candle], end: usize) -> Vec<Candle> {
    series[end.saturating_sub(180)..end].to_vec()
}
fn market_at(m1: &[Candle], m5: &[Candle], m15: &[Candle], index: usize) -> MarketData {
    let time = m1[index].time;
    let end5 = m5.partition_point(|bar| bar.time + 300 <= time);
    let end15 = m15.partition_point(|bar| bar.time + 900 <= time);
    MarketData {
        captured_ms: time * 1000,
        m1: tail(m1, index),
        m5: tail(m5, end5),
        m15: tail(m15, end15),
        error: String::new(),
    }
}
fn exit_at(
    position: &Position,
    candle: &Candle,
    spread: f64,
    gap: bool,
) -> Option<(f64, &'static str, bool, &'static str)> {
    let ask_adjust = if position.side == -1 { spread } else { 0.0 };
    let open = candle.open + ask_adjust;
    let high = candle.high + ask_adjust;
    let low = candle.low + ask_adjust;
    if gap {
        return Some((open, "data_gap", false, "open"));
    }
    if (position.side == 1 && open <= position.stop)
        || (position.side == -1 && open >= position.stop)
    {
        return Some((open, "stop_gap", false, "open"));
    }
    if (position.side == 1 && open >= position.target)
        || (position.side == -1 && open <= position.target)
    {
        return Some((position.target, "target", false, "open"));
    }
    if candle.time.saturating_sub(position.entry_time) >= 30 * 60 {
        return Some((open, "time_limit", false, "open"));
    }
    let stop = if position.side == 1 {
        low <= position.stop
    } else {
        high >= position.stop
    };
    let target = if position.side == 1 {
        high >= position.target
    } else {
        low <= position.target
    };
    if stop {
        Some((position.stop, "stop", target, "range"))
    } else if target {
        Some((position.target, "target", false, "range"))
    } else {
        None
    }
}

pub fn run_candidate(
    history: &History,
    options: &Options,
    cancel: &AtomicBool,
    progress: &AtomicUsize,
) -> Result<CandidateReport, String> {
    history.validate()?;
    options.validate()?;
    if options.strategy != Strategy::MetaV1 {
        return Err("Meta v2 comparison requires MetaV1 options as the baseline contract".into());
    }
    if options.to.saturating_mul(1000) > history.captured_ms {
        return Err("نهاية الاختبار تتجاوز وقت جمع البيانات".into());
    }
    let spec = &history.contract;
    let risk = &options.risk;
    let slip = risk.slippage_ticks * spec.tick_size;
    let m1: Vec<Candle> = history.bars.iter().map(|bar| bar.candle.clone()).collect();
    let m5 = aggregate(history, 300);
    let m15 = aggregate(history, 900);
    let indexes: Vec<usize> = (1..m1.len())
        .filter(|index| {
            m1[*index].time >= options.from && m1[*index].time + 60 <= options.to
        })
        .collect();
    if indexes.is_empty() {
        return Err("لا توجد شموع مكتملة ضمن الفترة المطلوبة".into());
    }

    let mut ledger = Ledger::new(options.initial_equity);
    let mut position: Option<Position> = None;
    let mut equity_curve = Vec::new();
    let mut gaps = 0usize;
    let mut last_spread = options.spread_floor;

    for (step, &index) in indexes.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("تم إلغاء مقارنة Meta v2".into());
        }
        progress.store(step * 100 / indexes.len(), Ordering::Relaxed);
        let candle = &m1[index];
        let previous = &m1[index - 1];
        let gap = candle.time != previous.time + 60;
        if gap {
            gaps += 1;
        }
        let spread = options
            .spread_floor
            .max(history.bars[index - 1].spread_points as f64 * spec.point);
        last_spread = spread;

        let marked_open = ledger.balance
            + position
                .as_ref()
                .map(|p| {
                    p.pnl(
                        if p.side == 1 {
                            candle.open - slip
                        } else {
                            candle.open + spread + slip
                        },
                        history,
                    )
                })
                .unwrap_or(0.0);
        ledger.mark(marked_open, candle.time, options, true);

        let mut exited = false;
        if let Some(open_position) = position.as_ref() {
            if let Some((price, reason, ambiguous, phase)) =
                exit_at(open_position, candle, spread, gap)
            {
                ledger.close(
                    position.take().unwrap(),
                    price,
                    candle.time,
                    phase,
                    reason,
                    ambiguous,
                    history,
                    slip,
                );
                ledger.mark(ledger.balance, candle.time, options, false);
                exited = true;
            }
        }

        if position.is_some() {
            ledger.skip("صفقة مفتوحة");
        } else if exited {
            ledger.skip("خروج في نفس الدقيقة؛ لا إعادة دخول");
        } else if gap {
            ledger.skip("فجوة قبل الشمعة");
        } else if ledger.daily_locked || ledger.drawdown_locked || ledger.balance <= 0.0 {
            ledger.skip("حد خسارة أو تراجع");
        } else {
            let analysis = analyze(
                Some(&market_at(&m1, &m5, &m15, index)),
                &history.symbol,
                candle.time * 1000,
            );
            let atr = analysis.atr.unwrap_or(0.0);
            let side = match analysis.regime_brain.meta_v2.action.as_str() {
                "BUY" => 1,
                "SELL" => -1,
                _ => 0,
            };
            if !analysis.current {
                ledger.skip("إحماء أو بيانات ناقصة");
            } else if side == 0 {
                ledger.skip("Meta v2 abstain");
            } else if spread > atr * risk.max_spread_atr {
                ledger.skip("سبريد مرتفع");
            } else if (candle.open - previous.close).abs() > 2.0 * atr {
                ledger.skip("قفزة سعرية");
            } else {
                let entry = if side == 1 {
                    candle.open + spread + slip
                } else {
                    candle.open - slip
                };
                let stop_distance = 1.5 * atr;
                let stop = if side == 1 {
                    ((entry - stop_distance) / spec.tick_size).floor() * spec.tick_size
                } else {
                    ((entry + stop_distance) / spec.tick_size).ceil() * spec.tick_size
                };
                let distance = (entry - stop).abs();
                let target = entry + side as f64 * 2.0 * distance;
                let available = (ledger.balance
                    - ledger.day_start * (1.0 - risk.daily_loss_percent / 100.0))
                    .max(0.0)
                    .min(
                        (ledger.balance
                            - ledger.peak * (1.0 - risk.max_drawdown_percent / 100.0))
                            .max(0.0),
                    );
                let budget = (ledger.balance * risk.risk_percent / 100.0).min(available);
                let loss_per_lot = (distance + slip) / spec.tick_size
                    * spec.tick_loss.max(spec.tick_profit)
                    + risk.commission_per_lot;
                if entry <= 0.0
                    || stop <= 0.0
                    || target <= 0.0
                    || !loss_per_lot.is_finite()
                    || loss_per_lot <= 0.0
                {
                    ledger.skip("أسعار الوقف أو الهدف غير صالحة");
                } else if let Some(lots) = size_lots(
                    budget,
                    loss_per_lot,
                    spec.volume_min,
                    spec.volume_step,
                    risk.max_lots.min(spec.volume_max),
                ) {
                    let new_position = Position {
                        side,
                        entry,
                        stop,
                        target,
                        lots,
                        commission: risk.commission_per_lot * lots,
                        entry_time: candle.time,
                        signal_bar: previous.time,
                    };
                    ledger.mark(
                        ledger.balance
                            + new_position.pnl(
                                if side == 1 {
                                    candle.open - slip
                                } else {
                                    candle.open + spread + slip
                                },
                                history,
                            ),
                        candle.time,
                        options,
                        false,
                    );
                    if let Some((price, reason, ambiguous, phase)) =
                        exit_at(&new_position, candle, spread, false)
                    {
                        ledger.close(
                            new_position,
                            price,
                            candle.time,
                            phase,
                            reason,
                            ambiguous,
                            history,
                            slip,
                        );
                    } else {
                        position = Some(new_position);
                    }
                } else {
                    ledger.skip("اللوت الأدنى يتجاوز الميزانية");
                }
            }
        }

        let marked_close = ledger.balance
            + position
                .as_ref()
                .map(|p| {
                    p.pnl(
                        if p.side == 1 {
                            candle.close - slip
                        } else {
                            candle.close + spread + slip
                        },
                        history,
                    )
                })
                .unwrap_or(0.0);
        if !marked_close.is_finite() {
            return Err("نتيجة حسابية غير محدودة؛ راجع مواصفات الرمز".into());
        }
        ledger.mark(marked_close, candle.time, options, false);
        equity_curve.push(EquityPoint {
            time: candle.time + 60,
            equity: marked_close,
            balance: ledger.balance,
        });
    }

    if let Some(open_position) = position.take() {
        let candle = &m1[*indexes.last().unwrap()];
        let price = if open_position.side == 1 {
            candle.close
        } else {
            candle.close + last_spread
        };
        ledger.close(
            open_position,
            price,
            candle.time,
            "final_close",
            "end_of_test",
            false,
            history,
            slip,
        );
        ledger.mark(ledger.balance, candle.time, options, false);
        if let Some(point) = equity_curve.last_mut() {
            point.equity = ledger.balance;
            point.balance = ledger.balance;
        }
    }

    let metrics = finalize_metrics(
        ledger.metrics,
        &ledger.trades,
        ledger.balance,
        options,
        indexes.len(),
    );
    progress.store(100, Ordering::Relaxed);
    let mut warnings = vec![
        "Meta v2 candidate uses the same M1 bid-bar approximation, previous completed-minute spread, configured commission and slippage assumptions as the conservative baseline model.".into(),
        "Meta v2 is not promoted to live Shadow by this result; identical-history comparison is a research gate only.".into(),
        "A/B results are in-sample for the selected history unless the caller explicitly separates development and validation periods.".into(),
    ];
    if gaps > 0 {
        warnings.push(format!(
            "Historical gaps observed: {gaps}; missing bars are not fabricated and an open candidate trade exits at the first returning open."
        ));
    }
    Ok(CandidateReport {
        algorithm: "meta-v2-candidate / M1-OHLC-conservative-v1".into(),
        metrics,
        trades: ledger.trades,
        equity: equity_curve,
        warnings,
    })
}

fn finalize_metrics(
    mut metrics: Metrics,
    trades: &[Trade],
    final_balance: f64,
    options: &Options,
    tested_bars: usize,
) -> Metrics {
    metrics.tested_bars = tested_bars;
    metrics.trades = trades.len();
    metrics.wins = trades.iter().filter(|trade| trade.net > 1e-9).count();
    metrics.losses = trades.iter().filter(|trade| trade.net < -1e-9).count();
    metrics.breakeven = metrics.trades - metrics.wins - metrics.losses;
    metrics.net = final_balance - options.initial_equity;
    metrics.return_percent = metrics.net / options.initial_equity * 100.0;
    metrics.win_rate = if metrics.trades > 0 {
        Some(metrics.wins as f64 / metrics.trades as f64 * 100.0)
    } else {
        None
    };
    let gross_profit = trades.iter().map(|trade| trade.net.max(0.0)).sum::<f64>();
    let gross_loss = trades.iter().map(|trade| (-trade.net).max(0.0)).sum::<f64>();
    metrics.profit_factor = if gross_loss > 1e-9 {
        Some(gross_profit / gross_loss)
    } else {
        None
    };
    metrics.commissions = trades.iter().map(|trade| trade.commission).sum();
    metrics.ambiguous_trades = trades.iter().filter(|trade| trade.ambiguous).count();
    metrics.gap_exits = trades
        .iter()
        .filter(|trade| trade.reason == "data_gap")
        .count();
    metrics
}

pub fn compare(history: &History, options: &Options) -> Result<Comparison, String> {
    if options.strategy != Strategy::MetaV1 {
        return Err("A/B baseline must be Strategy::MetaV1".into());
    }
    let baseline_progress = AtomicUsize::new(0);
    let candidate_progress = AtomicUsize::new(0);
    let cancel = AtomicBool::new(false);
    let baseline = backtest::run(history, options, &cancel, &baseline_progress)?;
    let candidate = run_candidate(history, options, &cancel, &candidate_progress)?;
    let delta = Delta {
        net: candidate.metrics.net - baseline.metrics.net,
        return_percent: candidate.metrics.return_percent - baseline.metrics.return_percent,
        max_drawdown: candidate.metrics.max_drawdown - baseline.metrics.max_drawdown,
        max_drawdown_percent: candidate.metrics.max_drawdown_percent
            - baseline.metrics.max_drawdown_percent,
        trades: candidate.metrics.trades as i64 - baseline.metrics.trades as i64,
        wins: candidate.metrics.wins as i64 - baseline.metrics.wins as i64,
        losses: candidate.metrics.losses as i64 - baseline.metrics.losses as i64,
        commissions: candidate.metrics.commissions - baseline.metrics.commissions,
    };
    Ok(Comparison {
        baseline_algorithm: baseline.algorithm,
        candidate_algorithm: candidate.algorithm,
        symbol: history.symbol.clone(),
        server: history.server.clone(),
        login: history.login,
        from: options.from,
        to: options.to,
        baseline: baseline.metrics,
        candidate: candidate.metrics,
        delta,
        warnings: vec![
            "Both arms consume the same validated History object and the same frozen risk, spread floor, contract and cost assumptions.".into(),
            "The candidate changes only signal eligibility. Position geometry remains 1.5 ATR stop, 2R target and 30-minute time limit for this comparison checkpoint.".into(),
            "Do not promote Meta v2 from one period. Use multiple untouched validation windows and later walk-forward testing.".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::{Contract, HistoryBar};
    use crate::RiskConfig;

    const BASE: u64 = 1_800_000_000;

    fn history(count: usize) -> History {
        History {
            symbol: "XAUUSD.TEST".into(),
            server: "FIXTURE".into(),
            login: 7,
            currency: "USD".into(),
            captured_ms: (BASE + count as u64 * 60 + 60) * 1000,
            contract: Contract {
                point: 0.01,
                tick_size: 0.01,
                tick_loss: 1.0,
                tick_profit: 1.0,
                volume_min: 0.01,
                volume_step: 0.01,
                volume_max: 2.0,
            },
            bars: (0..count)
                .map(|index| {
                    let trend = index as f64 * 0.08;
                    let wave = ((index % 17) as f64 - 8.0) * 0.005;
                    let close = 1900.0 + trend + wave;
                    HistoryBar {
                        candle: Candle {
                            time: BASE + index as u64 * 60,
                            open: close - 0.03,
                            high: close + 0.20,
                            low: close - 0.20,
                            close,
                            tick_volume: 100 + (index % 30) as u64,
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
            from: BASE + 1_650 * 60,
            to: BASE + count as u64 * 60,
            initial_equity: 10_000.0,
            spread_floor: 0.02,
            risk: RiskConfig::default(),
        }
    }

    #[test]
    fn comparison_uses_same_history_and_cost_contract() {
        let h = history(2_000);
        let o = options(2_000);
        let result = compare(&h, &o).unwrap();
        assert_eq!(result.symbol, h.symbol);
        assert_eq!(result.server, h.server);
        assert_eq!(result.login, h.login);
        assert_eq!(result.from, o.from);
        assert_eq!(result.to, o.to);
        assert!(result.baseline_algorithm.starts_with("meta-v1"));
        assert!(result.candidate_algorithm.starts_with("meta-v2-candidate"));
        assert!(result.summary().contains("نفس البيانات والتكاليف"));
        assert!(result.baseline.net.is_finite());
        assert!(result.candidate.net.is_finite());
    }

    #[test]
    fn candidate_accounting_reconciles_trade_net_and_commission() {
        let h = history(2_000);
        let o = options(2_000);
        let report = run_candidate(
            &h,
            &o,
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
        )
        .unwrap();
        let trade_net: f64 = report.trades.iter().map(|trade| trade.net).sum();
        let commission: f64 = report.trades.iter().map(|trade| trade.commission).sum();
        assert!((trade_net - report.metrics.net).abs() < 1e-8);
        assert!((commission - report.metrics.commissions).abs() < 1e-8);
        assert_eq!(
            report.metrics.trades,
            report.metrics.wins + report.metrics.losses + report.metrics.breakeven
        );
    }

    #[test]
    fn candidate_rejects_non_meta_baseline_and_honors_cancel() {
        let h = history(2_000);
        let mut o = options(2_000);
        o.strategy = Strategy::CesV1;
        assert!(compare(&h, &o).is_err());
        o.strategy = Strategy::MetaV1;
        let cancel = AtomicBool::new(true);
        assert!(run_candidate(&h, &o, &cancel, &AtomicUsize::new(0)).is_err());
    }

    #[test]
    fn future_suffix_cannot_change_an_earlier_candidate_signal() {
        let h = history(2_000);
        let index = 1_800usize;
        let m1: Vec<Candle> = h.bars.iter().map(|bar| bar.candle.clone()).collect();
        let m5 = aggregate(&h, 300);
        let m15 = aggregate(&h, 900);
        let before = analyze(
            Some(&market_at(&m1, &m5, &m15, index)),
            &h.symbol,
            m1[index].time * 1000,
        )
        .regime_brain
        .meta_v2;

        let mut changed = h.clone();
        for bar in changed.bars.iter_mut().skip(index + 1) {
            bar.candle.open *= 1.4;
            bar.candle.high *= 1.4;
            bar.candle.low *= 1.4;
            bar.candle.close *= 1.4;
        }
        let changed_m1: Vec<Candle> = changed
            .bars
            .iter()
            .map(|bar| bar.candle.clone())
            .collect();
        let changed_m5 = aggregate(&changed, 300);
        let changed_m15 = aggregate(&changed, 900);
        let after = analyze(
            Some(&market_at(&changed_m1, &changed_m5, &changed_m15, index)),
            &changed.symbol,
            changed_m1[index].time * 1000,
        )
        .regime_brain
        .meta_v2;
        assert_eq!(before.action, after.action);
        assert_eq!(before.direction, after.direction);
        assert!((before.evidence - after.evidence).abs() < 1e-12);
    }
}
