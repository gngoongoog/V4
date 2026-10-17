//! Research-only online exploration. Every decision sees completed M1 bars and past closed outcomes.
//! No broker orders; fractional research lots may be below the actual MT5 minimum.
use super::*;
use serde::{Deserialize, Serialize};

const MICRO_STEP: f64 = 0.000001;
const LEARNING_RATE: f64 = 0.20;
const HOLD_SECONDS: u64 = 5 * 60;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub entry_time: u64,
    pub exit_time: u64,
    pub policy: String,
    pub selection: String,
    pub side: i8,
    pub exit_reason: String,
    pub spread: f64,
    pub lots: f64,
    pub broker_volume_min: f64,
    pub net_after_costs: f64,
    pub risk_budget_at_entry: f64,
    pub realized_r: f64,
    pub score_before: f64,
    pub score_after: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Audit {
    pub algorithm: String,
    pub scores: [f64; 2],
    pub closed_samples: [usize; 2],
    pub exploration_entries: usize,
    pub below_broker_minimum: usize,
    pub observations: Vec<Observation>,
}
struct Pending {
    policy: usize,
    selection: &'static str,
    budget: f64,
    spread: f64,
    score_before: f64,
}
impl Audit {
    fn record(&mut self, opening: Pending, trade: &Trade) {
        // Reward is net of spread, slippage and commission, scaled by pre-entry risk.
        // Cap extreme outcomes so a single lucky trade does not dominate future decisions.
        let r = (trade.net / opening.budget.max(1e-9)).clamp(-4.0, 4.0);
        let after = self.scores[opening.policy] * (1.0 - LEARNING_RATE) + LEARNING_RATE * r;
        self.scores[opening.policy] = after;
        self.closed_samples[opening.policy] += 1;
        self.observations.push(Observation {
            entry_time: trade.entry_time,
            exit_time: trade.exit_bar,
            policy: if opening.policy == 0 { "trend" } else { "reversion" }.into(),
            selection: opening.selection.into(),
            side: trade.side,
            exit_reason: trade.reason.clone(),
            spread: opening.spread,
            lots: trade.lots,
            broker_volume_min: trade.lots.max(0.0), // overwritten by caller below
            net_after_costs: trade.net,
            risk_budget_at_entry: opening.budget,
            realized_r: r,
            score_before: opening.score_before,
            score_after: after,
        });
    }
}
fn settle(ledger: &Ledger, pending: &mut Option<Pending>, audit: &mut Audit, min_volume: f64) {
    if let (Some(p), Some(trade)) = (pending.take(), ledger.trades.last()) {
        audit.record(p, trade);
        if let Some(last) = audit.observations.last_mut() {
            last.broker_volume_min = min_volume;
        }
    }
}
fn continuous(bars: &[Candle]) -> bool {
    bars.windows(2).all(|w| w[0].time + 60 == w[1].time)
}

pub(super) fn run(
    history: &History,
    options: &Options,
    cancel: &AtomicBool,
    progress: &AtomicUsize,
) -> Result<Report, String> {
    let spec = &history.contract;
    let risk = &options.risk;
    let bars: Vec<_> = history.bars.iter().map(|b| b.candle.clone()).collect();
    let indexes: Vec<_> = (1..bars.len())
        .filter(|&i| bars[i].time >= options.from && bars[i].time + 60 <= options.to)
        .collect();
    if indexes.is_empty() {
        return Err("لا توجد شموع مكتملة ضمن الفترة المطلوبة".into());
    }
    let first = bars[*indexes.first().unwrap()].time;
    let last = bars[*indexes.last().unwrap()].time;
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
    let mut audit = Audit {
        algorithm: "explore-v1/causal-two-policy-online-ema-v1".into(),
        ..Audit::default()
    };
    let mut position: Option<Position> = None;
    let mut pending: Option<Pending> = None;
    let mut equity = Vec::new();
    let mut gaps = 0usize;
    let slip = risk.slippage_ticks * spec.tick_size;
    let mut last_spread = 0.0;

    for (step, &i) in indexes.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("تم إلغاء الاختبار".into());
        }
        progress.store(step * 100 / indexes.len(), Ordering::Relaxed);
        let c = &bars[i];
        let prev = &bars[i - 1];
        let gap = c.time != prev.time + 60;
        gaps += usize::from(gap);
        let spread = options
            .spread_floor
            .max(history.bars[i - 1].spread_points as f64 * spec.point);
        last_spread = spread;
        let open_equity = ledger.balance + position.as_ref().map(|p| {
            let quote = c.open + if p.side < 0 { spread + slip } else { -slip };
            p.pnl(quote, spec)
        }).unwrap_or(0.0);
        ledger.mark(open_equity, c.time, risk, true);

        let mut exited = false;
        if let Some(p) = position.as_ref() {
            if let Some((price, why, ambiguous, phase)) = exit_at(p, c, spread, gap, false) {
                ledger.close(position.take().unwrap(), price, c.time, phase, why, ambiguous, spec, slip);
                settle(&ledger, &mut pending, &mut audit, spec.volume_min);
                exited = true;
                ledger.mark(ledger.balance, c.time, risk, false);
            }
        }
        if position.is_some() {
            ledger.skip("استكشاف: صفقة قيد المراقبة");
        } else if exited {
            ledger.skip("استكشاف: لا إعادة دخول بدقيقة الإغلاق");
        } else if gap {
            ledger.skip("استكشاف: فجوة تاريخ");
        } else if i < 30 || !continuous(&bars[i-30..i]) {
            ledger.skip("استكشاف: إحماء 30 شمعة متصلة");
        } else if ledger.balance <= 1.0 {
            ledger.skip("استكشاف: الرصيد الافتراضي غير كاف");
        } else {
            // Only the 30 fully closed bars before this entry are consulted.
            let atr = crate::trading::volatility(&bars[i-15..i]);
            let delta = prev.close - bars[i-4].close;
            let trend = if delta > atr * 0.05 { 1 } else if delta < -atr * 0.05 { -1 } else { 0 };
            if !atr.is_finite() || atr <= 0.0 || trend == 0 {
                ledger.skip("استكشاف: تذبذب أو زخم غير صالح");
            } else {
                let greedy = if audit.scores[1] > audit.scores[0] { 1 } else { 0 };
                // Deterministic 25% exploration, without RNG or future outcomes.
                let exploring = (audit.exploration_entries + audit.observations.len()) % 4 == 3;
                let chosen = if exploring { 1 - greedy } else { greedy };
                let side = if chosen == 0 { trend } else { -trend };
                let entry = c.open + if side > 0 { spread + slip } else { -slip };
                let stop_distance = (atr * 0.8).max(spread * 1.5 + 2.0 * slip + 2.0 * spec.tick_size);
                let stop = if side > 0 {
                    ((entry - stop_distance) / spec.tick_size).floor() * spec.tick_size
                } else {
                    ((entry + stop_distance) / spec.tick_size).ceil() * spec.tick_size
                };
                let distance = (entry - stop).abs();
                let target = entry + side as f64 * 1.2 * distance;
                let loss_per_lot = (distance + slip) / spec.tick_size
                    * spec.tick_loss.max(spec.tick_profit)
                    + risk.commission_per_lot;
                let budget = ledger.balance.max(0.0) * risk.risk_percent / 100.0;
                // Fractional units are explicit research hypotheses, not broker-legal volumes.
                // Preserve the money risk budget, rather than silently rounding up to 0.01.
                if entry <= 0.0 || stop <= 0.0 || target <= 0.0 {
                    ledger.skip("استكشاف: هندسة سعر غير صالحة");
                } else if let Some(lots) = size_lots(
                    budget, loss_per_lot, MICRO_STEP, MICRO_STEP,
                    risk.max_lots.min(spec.volume_max),
                ) {
                    audit.exploration_entries += 1;
                    if lots + 1e-12 < spec.volume_min {
                        audit.below_broker_minimum += 1;
                    }
                    let p = Position {
                        time_limit: HOLD_SECONDS,
                        side,
                        entry,
                        stop,
                        target,
                        lots,
                        commission: risk.commission_per_lot * lots,
                        entry_time: c.time,
                        signal_bar: prev.time,
                    };
                    pending = Some(Pending {
                        policy: chosen,
                        selection: if exploring { "explore" } else { "exploit" },
                        budget,
                        spread,
                        score_before: audit.scores[chosen],
                    });
                    ledger.mark(ledger.balance + p.pnl(
                        c.open + if side < 0 { spread + slip } else { -slip }, spec),
                        c.time, risk, false);
                    if let Some((price, why, ambiguous, phase)) = exit_at(&p, c, spread, false, false) {
                        ledger.close(p, price, c.time, phase, why, ambiguous, spec, slip);
                        settle(&ledger, &mut pending, &mut audit, spec.volume_min);
                    } else {
                        position = Some(p);
                    }
                } else {
                    ledger.skip("استكشاف: حتى حجم البحث الافتراضي يتجاوز المخاطرة");
                }
            }
        }
        let marked = ledger.balance + position.as_ref().map(|p| {
            let quote = c.close + if p.side < 0 { spread + slip } else { -slip };
            p.pnl(quote, spec)
        }).unwrap_or(0.0);
        if !marked.is_finite() {
            return Err("نتيجة حسابية غير محدودة".into());
        }
        ledger.mark(marked, c.time, risk, false);
        equity.push(EquityPoint { time: c.time + 60, equity: marked, balance: ledger.balance });
    }
    if let Some(p) = position.take() {
        let c = &bars[*indexes.last().unwrap()];
        let price = c.close + if p.side < 0 { last_spread } else { 0.0 };
        ledger.close(p, price, c.time, "final_close", "end_of_test", false, spec, slip);
        settle(&ledger, &mut pending, &mut audit, spec.volume_min);
        if let Some(point) = equity.last_mut() {
            point.equity = ledger.balance;
            point.balance = ledger.balance;
        }
    }
    let mut report = finish(history, options, ledger, equity, first, last, indexes.len(), gaps, progress)?;
    report.warnings.push(
        "Explore v1: بحث افتراضي فقط؛ أحجام ميكرو أدنى من حد الوسيط قد تُستخدم للحفاظ على مخاطرة 100$. لا يمكن افتراض تنفيذها في Demo/Live أو صحة الهامش. حدود التوقف اليومي/التراجع لا توقف الاستكشاف؛ التراجع يُقاس ويُعرض.".into()
    );
    report.warnings.push(
        "Explore v1: اختيار trend/reversion قبل الدخول من نتائج صفقات مغلقة سابقاً فقط؛ 25% استكشاف دوري وEMA reward محقق بعد السبريد والعمولة والانزلاق. هذا نموذج bandit بسيط، لا تعلم عميق ولا توقع مضمون.".into()
    );
    report.warnings.push(format!(
        "Explore v1: {} صفقة تقييم؛ trend {} بنتيجة {:.3} R، reversion {} بنتيجة {:.3} R؛ {} دخول بحجم أدنى من حجم الوسيط.",
        audit.observations.len(), audit.closed_samples[0], audit.scores[0],
        audit.closed_samples[1], audit.scores[1], audit.below_broker_minimum
    ));
    report.exploration = Some(audit);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: u64 = 1_728_000_000;
    fn sample() -> (History, Options) {
        let n = 1200u64;
        let bars = (0..n).map(|i| {
            let mid = 2400.0 + (i as f64 / 11.0).sin() * 2.0 + (i as f64 / 43.0).cos();
            HistoryBar {
                candle: Candle {
                    time: BASE + i * 60,
                    open: mid,
                    high: mid + 0.7,
                    low: mid - 0.7,
                    close: mid + 0.02,
                    tick_volume: 100,
                },
                spread_points: 20,
            }
        }).collect();
        let h = History {
            symbol: "XAUUSD".into(), server: "TEST".into(), login: 17, currency: "USD".into(),
            captured_ms: (BASE + n * 60) * 1000,
            contract: Contract {
                point: 0.01, tick_size: 0.01, tick_loss: 1.0, tick_profit: 1.0,
                volume_min: 0.01, volume_step: 0.01, volume_max: 100.0,
            },
            bars,
        };
        let o = Options {
            strategy: Strategy::ExploreV1,
            from: BASE + 60 * 40, to: BASE + n * 60,
            initial_equity: 100.0, spread_floor: 0.20,
            risk: RiskConfig::default(),
        };
        (h, o)
    }
    #[test]
    fn hundred_dollar_exploration_learns_from_closed_trades_and_labels_non_executable_lots() {
        let (history, opts) = sample();
        let r = run(&history, &opts, &AtomicBool::new(false), &AtomicUsize::new(0)).unwrap();
        let audit = r.exploration.as_ref().unwrap();
        assert!(r.metrics.trades >= 10);
        assert_eq!(audit.observations.len(), r.metrics.trades);
        assert!(audit.below_broker_minimum > 0);
        assert!(audit.closed_samples[0] > 0 && audit.closed_samples[1] > 0);
        let mut prior = [0.0_f64; 2];
        for observation in &audit.observations {
            let index = if observation.policy == "trend" {0} else {1};
            assert!((observation.score_before - prior[index]).abs() < 1e-9);
            prior[index] = observation.score_after;
            assert!(observation.risk_budget_at_entry > 0.0);
            assert!(observation.net_after_costs.is_finite());
        }
    }
    #[test]
    fn deterministic_offline_replay_has_identical_online_trace() {
        let (history, opts) = sample();
        let a = run(&history, &opts, &AtomicBool::new(false), &AtomicUsize::new(0)).unwrap();
        let b = run(&history, &opts, &AtomicBool::new(false), &AtomicUsize::new(0)).unwrap();
        assert_eq!(a.exploration.unwrap().observations, b.exploration.unwrap().observations);
    }
}
