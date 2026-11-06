use crate::backtest::{run, History, Metrics, Options, Report, Strategy, DAY};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const VERSION: &str = "chronological-evaluation-v1";
const MIN_WARMUP: u64 = 2 * DAY;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum EvaluationPlan {
    Holdout { holdout_percent: u8 },
    Rolling {
        development_days: u16,
        holdout_days: u16,
        step_days: u16,
    },
}
impl Default for EvaluationPlan {
    fn default() -> Self {
        Self::Holdout { holdout_percent: 30 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowRole {
    Development,
    Holdout,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationWindow {
    pub cycle: usize,
    pub role: WindowRole,
    pub from: u64,
    pub to: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyWindowResult {
    pub strategy: Strategy,
    pub algorithm: String,
    pub metrics: Metrics,
    pub first_test_bar: u64,
    pub last_test_bar: u64,
    pub gaps: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowEvaluation {
    pub window: EvaluationWindow,
    pub meta_v1: StrategyWindowResult,
    pub meta_v2: StrategyWindowResult,
    pub delta_net_v2_minus_v1: f64,
    pub delta_return_percent_v2_minus_v1: f64,
    pub delta_drawdown_percent_v2_minus_v1: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StrategyAggregate {
    pub strategy: String,
    pub windows: usize,
    pub trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub breakeven: usize,
    pub positive_windows: usize,
    pub net_sum: f64,
    pub average_return_percent: f64,
    pub worst_drawdown_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChronologicalEvaluation {
    pub version: String,
    pub plan: EvaluationPlan,
    pub symbol: String,
    pub server: String,
    pub login: u64,
    pub currency: String,
    pub captured_ms: u64,
    pub history_first: u64,
    pub history_last: u64,
    pub history_bars: usize,
    pub options_template: Options,
    pub windows: Vec<WindowEvaluation>,
    pub meta_v1: StrategyAggregate,
    pub meta_v2: StrategyAggregate,
    pub average_delta_net_v2_minus_v1: f64,
    pub average_delta_return_percent_v2_minus_v1: f64,
    pub note: String,
}

fn windows(options: &Options, plan: &EvaluationPlan) -> Result<Vec<EvaluationWindow>, String> {
    if !options.from.is_multiple_of(DAY) || !options.to.is_multiple_of(DAY) {
        return Err("التقييم الزمني يتطلب حدود أيام UTC كاملة".into());
    }
    let total_days = (options.to - options.from) / DAY;
    match plan {
        EvaluationPlan::Holdout { holdout_percent } => {
            if !(10..=50).contains(holdout_percent) || total_days < 4 {
                return Err("Holdout يحتاج 4 أيام على الأقل ونسبة بين 10% و50%".into());
            }
            let mut development_days = total_days * (100 - *holdout_percent as u64) / 100;
            development_days = development_days.clamp(2, total_days - 1);
            let split = options.from + development_days * DAY;
            Ok(vec![
                EvaluationWindow {
                    cycle: 0,
                    role: WindowRole::Development,
                    from: options.from,
                    to: split,
                },
                EvaluationWindow {
                    cycle: 0,
                    role: WindowRole::Holdout,
                    from: split,
                    to: options.to,
                },
            ])
        }
        EvaluationPlan::Rolling {
            development_days,
            holdout_days,
            step_days,
        } => {
            if *development_days < 2
                || *holdout_days < 1
                || *step_days < 1
                || *development_days as u64 + *holdout_days as u64 > total_days
            {
                return Err("Rolling يحتاج development >=2 أيام وholdout/step >=1 ويجب أن تتسع الفترة لدورة كاملة".into());
            }
            let mut out = vec![];
            let mut cursor = options.from;
            let mut cycle = 0usize;
            loop {
                let dev_to = cursor + *development_days as u64 * DAY;
                let hold_to = dev_to + *holdout_days as u64 * DAY;
                if hold_to > options.to {
                    break;
                }
                out.push(EvaluationWindow {
                    cycle,
                    role: WindowRole::Development,
                    from: cursor,
                    to: dev_to,
                });
                out.push(EvaluationWindow {
                    cycle,
                    role: WindowRole::Holdout,
                    from: dev_to,
                    to: hold_to,
                });
                cycle += 1;
                cursor = cursor.saturating_add(*step_days as u64 * DAY);
            }
            if out.is_empty() {
                return Err("لا توجد دورة rolling مكتملة ضمن الفترة".into());
            }
            Ok(out)
        }
    }
}

fn summarized(report: Report) -> StrategyWindowResult {
    StrategyWindowResult {
        strategy: report.options.strategy,
        algorithm: report.algorithm,
        metrics: report.metrics,
        first_test_bar: report.first_test_bar,
        last_test_bar: report.last_test_bar,
        gaps: report.gaps,
    }
}

fn aggregate(strategy: Strategy, rows: impl Iterator<Item = StrategyWindowResult>) -> StrategyAggregate {
    let mut out = StrategyAggregate {
        strategy: strategy.algorithm().into(),
        ..Default::default()
    };
    let mut return_sum = 0.0;
    for row in rows {
        out.windows += 1;
        out.trades += row.metrics.trades;
        out.wins += row.metrics.wins;
        out.losses += row.metrics.losses;
        out.breakeven += row.metrics.breakeven;
        out.positive_windows += usize::from(row.metrics.net > 0.0);
        out.net_sum += row.metrics.net;
        return_sum += row.metrics.return_percent;
        out.worst_drawdown_percent = out
            .worst_drawdown_percent
            .max(row.metrics.max_drawdown_percent);
    }
    if out.windows > 0 {
        out.average_return_percent = return_sum / out.windows as f64;
    }
    out
}

pub fn evaluate(
    history: &History,
    options: &Options,
    plan: &EvaluationPlan,
    cancel: &AtomicBool,
    progress: &AtomicUsize,
) -> Result<ChronologicalEvaluation, String> {
    history.validate()?;
    options.validate()?;
    if !matches!(options.strategy, Strategy::MetaV1 | Strategy::MetaV2) {
        return Err("التقييم الزمني v1 مخصص لمقارنة Meta v1 وMeta v2 فقط".into());
    }
    let first_history = history.bars.first().unwrap().candle.time;
    if options.from.saturating_sub(first_history) < MIN_WARMUP {
        return Err("نحتاج يومين كاملين على الأقل من الإحماء الحقيقي قبل أول نافذة تقييم".into());
    }
    let definitions = windows(options, plan)?;
    let total_runs = definitions.len() * 2;
    let mut results = Vec::with_capacity(definitions.len());
    for (window_index, window) in definitions.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err("تم إلغاء التقييم الزمني".into());
        }
        let mut reports = Vec::with_capacity(2);
        for (strategy_index, strategy) in [Strategy::MetaV1, Strategy::MetaV2].into_iter().enumerate() {
            let mut child = options.clone();
            child.strategy = strategy;
            child.from = window.from;
            child.to = window.to;
            let child_progress = AtomicUsize::new(0);
            let report = run(history, &child, cancel, &child_progress)?;
            reports.push(summarized(report));
            let completed = window_index * 2 + strategy_index + 1;
            progress.store(completed * 100 / total_runs, Ordering::Relaxed);
        }
        let meta_v1 = reports.remove(0);
        let meta_v2 = reports.remove(0);
        results.push(WindowEvaluation {
            delta_net_v2_minus_v1: meta_v2.metrics.net - meta_v1.metrics.net,
            delta_return_percent_v2_minus_v1: meta_v2.metrics.return_percent
                - meta_v1.metrics.return_percent,
            delta_drawdown_percent_v2_minus_v1: meta_v2.metrics.max_drawdown_percent
                - meta_v1.metrics.max_drawdown_percent,
            window,
            meta_v1,
            meta_v2,
        });
    }
    let a1 = aggregate(Strategy::MetaV1, results.iter().map(|r| r.meta_v1.clone()));
    let a2 = aggregate(Strategy::MetaV2, results.iter().map(|r| r.meta_v2.clone()));
    let count = results.len().max(1) as f64;
    let delta_net = results.iter().map(|r| r.delta_net_v2_minus_v1).sum::<f64>() / count;
    let delta_return = results
        .iter()
        .map(|r| r.delta_return_percent_v2_minus_v1)
        .sum::<f64>()
        / count;
    progress.store(100, Ordering::Relaxed);
    Ok(ChronologicalEvaluation {
        version: VERSION.into(),
        plan: plan.clone(),
        symbol: history.symbol.clone(),
        server: history.server.clone(),
        login: history.login,
        currency: history.currency.clone(),
        captured_ms: history.captured_ms,
        history_first: first_history,
        history_last: history.bars.last().unwrap().candle.time,
        history_bars: history.bars.len(),
        options_template: options.clone(),
        windows: results,
        meta_v1: a1,
        meta_v2: a2,
        average_delta_net_v2_minus_v1: delta_net,
        average_delta_return_percent_v2_minus_v1: delta_return,
        note: "كل نافذة تعيد ضبط دفتر الصفقات ورأس المال الافتراضي. الإحماء يقرأ الماضي السابق فقط، ولا يسمح بأي دخول قبل بداية النافذة. التجميع يجمع نتائج نوافذ مستقلة ولا يمثل منحنى رأس مال متصل أو إثبات ربحية.".into(),
    })
}

/// Side-by-side CES hypotheses on the SAME broker history, contract, risk
/// settings, cost assumptions and non-overlapping chronological windows.
///
/// This is historical research, NOT a frozen/prespecified independent OOS
/// certificate. The candidate never executes MT5 broker orders or auto-promotes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CesEpiWindow {
    pub window: EvaluationWindow,
    pub tick_volume_policy: StrategyWindowResult,
    pub candle_range_policy: StrategyWindowResult,
    pub net_difference_range_minus_tick: f64,
    pub return_difference_range_minus_tick: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CesEpiComparison {
    pub version: String,
    pub source: String,
    pub symbol: String,
    pub server: String,
    pub login: u64,
    pub currency: String,
    pub captured_ms: u64,
    pub plan: EvaluationPlan,
    pub options_template: Options,
    pub windows: Vec<CesEpiWindow>,
    pub tick_volume_policy: StrategyAggregate,
    pub candle_range_policy: StrategyAggregate,
    pub research_only: bool,
    pub broker_orders: bool,
    pub automatic_promotion: bool,
    pub independently_frozen_oos: bool,
    pub note: String,
}
pub fn compare_ces_epi(
    history:&History, options:&Options,plan:&EvaluationPlan,
    cancel:&AtomicBool,progress:&AtomicUsize
)->Result<CesEpiComparison,String>{
    history.validate()?;
    options.validate()?;
    let first=history.bars.first().unwrap().candle.time;
    if options.from.saturating_sub(first)<MIN_WARMUP {
        return Err("CES comparison requires two real UTC days of warmup".into());
    }
    let windows=windows(options,plan)?;
    let total=windows.len()*2;
    let mut rows=Vec::with_capacity(windows.len());
    for (i,window) in windows.into_iter().enumerate(){
        if cancel.load(Ordering::Relaxed){return Err("CES comparison cancelled".into());}
        let mut reports=Vec::with_capacity(2);
        for (j,strategy) in [Strategy::CesV1,Strategy::CesRangeResearch].into_iter().enumerate(){
            let mut child=options.clone();
            child.strategy=strategy;
            child.from=window.from;
            child.to=window.to;
            let child_progress=AtomicUsize::new(0);
            reports.push(summarized(run(history,&child,cancel,&child_progress)?));
            progress.store((i*2+j+1)*100/total,Ordering::Relaxed);
        }
        let legacy=reports.remove(0);
        let range=reports.remove(0);
        rows.push(CesEpiWindow {
            net_difference_range_minus_tick:range.metrics.net-legacy.metrics.net,
            return_difference_range_minus_tick:
                range.metrics.return_percent-legacy.metrics.return_percent,
            window,
            tick_volume_policy:legacy,
            candle_range_policy:range,
        });
    }
    let legacy=aggregate(Strategy::CesV1,rows.iter().map(|w|w.tick_volume_policy.clone()));
    let range=aggregate(Strategy::CesRangeResearch,rows.iter().map(|w|w.candle_range_policy.clone()));
    progress.store(100,Ordering::Relaxed);
    Ok(CesEpiComparison{
        version:"ces-two-epi-chronological-research-v1".into(),
        source:"same_saved_mt5_history_and_contract_costs".into(),
        symbol:history.symbol.clone(),server:history.server.clone(),
        login:history.login,currency:history.currency.clone(),
        captured_ms:history.captured_ms,
        plan:plan.clone(),options_template:options.clone(),windows:rows,
        tick_volume_policy:legacy,candle_range_policy:range,
        research_only:true,broker_orders:false,automatic_promotion:false,
        independently_frozen_oos:false,
        note:"Historical comparison of two FIXED CES definitions on the same completed M1 history and cost emulator. Holdout is chronological but already observable: no independent frozen OOS or broker execution. Each window resets capital; no automatic selection or weight promotion. Very few trades cannot establish an edge.".into(),
    })
}

/// Four predeclared deterministic advisor policies under the same M1
/// entry/exit/cost/stop model, grouped by chronological window. This
/// compares SIGNALS rather than reproducing exact per-brain live execution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrainWindow {
    pub window: EvaluationWindow,
    pub trend_baseline: StrategyWindowResult,
    pub wyckoff: StrategyWindowResult,
    pub liquidity_sweep: StrategyWindowResult,
    pub trend_pullback: StrategyWindowResult,
    /// Descriptive sample quality, not a statistical significance or OOS
    /// certificate. All four arms need 30 CLOSED virtual trades to avoid
    /// treating one lucky winner as a validated improvement.
    pub evidence_status: String,
    pub minimum_trades_per_arm: usize,
    pub evidence_reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrainAblation {
    pub version:String,
    pub symbol:String,
    pub server:String,
    pub login:u64,
    pub currency:String,
    pub captured_ms:u64,
    pub plan:EvaluationPlan,
    pub options_template:Options,
    pub windows:Vec<BrainWindow>,
    pub trend_baseline:StrategyAggregate,
    pub wyckoff:StrategyAggregate,
    pub liquidity_sweep:StrategyAggregate,
    pub trend_pullback:StrategyAggregate,
    pub research_only:bool,
    pub broker_orders:bool,
    pub automatic_promotion:bool,
    pub independently_frozen_oos:bool,
    pub note:String,
}
pub fn compare_brain_signals(
    history:&History,options:&Options,plan:&EvaluationPlan,
    cancel:&AtomicBool,progress:&AtomicUsize
)->Result<BrainAblation,String>{
    history.validate()?;options.validate()?;
    let first=history.bars.first().unwrap().candle.time;
    if options.from.saturating_sub(first)<MIN_WARMUP {
        return Err("Brain signal comparison needs at least two prior UTC warmup days".into());
    }
    let windows=windows(options,plan)?;
    let total=windows.len()*4;
    let mut rows=Vec::with_capacity(windows.len());
    for (i,window) in windows.into_iter().enumerate(){
        if cancel.load(Ordering::Relaxed){return Err("Brain study cancelled".into())}
        let mut reports=Vec::with_capacity(4);
        for (j,strategy) in [
            Strategy::BrainTrendResearch,Strategy::BrainWyckoffResearch,
            Strategy::BrainLiquidityResearch,Strategy::BrainPullbackResearch
        ].into_iter().enumerate(){
            let mut child=options.clone();child.strategy=strategy;
            child.from=window.from;child.to=window.to;
            reports.push(summarized(run(history,&child,cancel,&AtomicUsize::new(0))?));
            progress.store((i*4+j+1)*100/total,Ordering::Relaxed);
        }
        let incomplete=reports.iter().any(|r|r.gaps>0);
        let smallest=reports.iter().map(|r|r.metrics.trades).min().unwrap_or(0);
        let (quality,reason)=if incomplete {
            ("data_gaps","One or more policies were simulated across missing minutes; execution path is not a complete tick sequence")
        }else if smallest<30{
            ("insufficient_trades","At least one advisor has fewer than 30 completed hypothetical trades; relative PnL is descriptive, not evidence of improvement")
        }else{
            ("descriptive_only","At least 30 hypothetical closed trades per policy; still no independent frozen OOS or broker order proof")
        };
        rows.push(BrainWindow{window,trend_baseline:reports.remove(0),
            wyckoff:reports.remove(0),liquidity_sweep:reports.remove(0),
            trend_pullback:reports.remove(0),
            evidence_status:quality.into(),minimum_trades_per_arm:30,
            evidence_reason:reason.into()});
    }
    let summary=|strategy,fn_row:fn(&BrainWindow)->StrategyWindowResult|{
        aggregate(strategy,rows.iter().map(fn_row))
    };
    progress.store(100,Ordering::Relaxed);
    Ok(BrainAblation{
        version:"brain-signal-ablation-m1-ohlc-v1".into(),
        symbol:history.symbol.clone(),server:history.server.clone(),
        login:history.login,currency:history.currency.clone(),
        captured_ms:history.captured_ms,plan:plan.clone(),
        options_template:options.clone(),
        trend_baseline:summary(Strategy::BrainTrendResearch,|r|r.trend_baseline.clone()),
        wyckoff:summary(Strategy::BrainWyckoffResearch,|r|r.wyckoff.clone()),
        liquidity_sweep:summary(Strategy::BrainLiquidityResearch,|r|r.liquidity_sweep.clone()),
        trend_pullback:summary(Strategy::BrainPullbackResearch,|r|r.trend_pullback.clone()),
        windows:rows,research_only:true,broker_orders:false,
        automatic_promotion:false,independently_frozen_oos:false,
        note:"Signal-only comparison of actual deterministic rule votes vs Trend under identical 1.5ATR/2R/30min OHLC fills and historical cost assumptions. Last-close quote proxy is not a true broker tick; structural stops, fill latency, real volume and live execution are NOT replicated. Holdout already observable and may be subject to selection bias. Do not promote or claim edge without prospective frozen evidence.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Candle, RiskConfig};
    use crate::backtest::{Contract, HistoryBar};

    const BASE: u64 = 1_728_000_000; // exact UTC day boundary

    fn fixture(days: usize) -> History {
        let count = days * DAY as usize / 60;
        History {
            symbol: "TEST".into(),
            server: "FIXTURE".into(),
            login: 7,
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
                    let wave = ((i % 240) as f64 - 120.0) * 0.002;
                    let p = 2000.0 + i as f64 * 0.003 + wave;
                    HistoryBar {
                        candle: Candle {
                            time: BASE + i as u64 * 60,
                            open: p,
                            high: p + 0.18,
                            low: p - 0.16,
                            close: p + 0.04,
                            tick_volume: 80 + (i % 40) as u64,
                        },
                        spread_points: 2,
                    }
                })
                .collect(),
        }
    }

    fn options() -> Options {
        Options {
            strategy: Strategy::MetaV1,
            from: BASE + 3 * DAY,
            to: BASE + 10 * DAY,
            initial_equity: 10_000.0,
            spread_floor: 0.02,
            risk: RiskConfig::default(),
        }
    }

    #[test]
    fn holdout_is_non_overlapping_and_each_report_is_bounded() {
        let h = fixture(12);
        let o = options();
        let e = evaluate(
            &h,
            &o,
            &EvaluationPlan::Holdout { holdout_percent: 30 },
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
        )
        .unwrap();
        assert_eq!(e.windows.len(), 2);
        assert_eq!(e.windows[0].window.role, WindowRole::Development);
        assert_eq!(e.windows[1].window.role, WindowRole::Holdout);
        assert_eq!(e.windows[0].window.to, e.windows[1].window.from);
        for row in &e.windows {
            for result in [&row.meta_v1, &row.meta_v2] {
                assert!(result.first_test_bar >= row.window.from);
                assert!(result.last_test_bar < row.window.to);
            }
        }
        assert_eq!(e.meta_v1.windows, 2);
        assert_eq!(e.meta_v2.windows, 2);
    }

    #[test]
    fn future_mutation_cannot_change_completed_development_window() {
        let h = fixture(12);
        let o = options();
        let plan = EvaluationPlan::Holdout { holdout_percent: 30 };
        let original = evaluate(&h, &o, &plan, &AtomicBool::new(false), &AtomicUsize::new(0)).unwrap();
        let split = original.windows[0].window.to;
        let mut changed = h.clone();
        for bar in changed.bars.iter_mut().filter(|b| b.candle.time >= split) {
            bar.candle.open += 500.0;
            bar.candle.high += 500.0;
            bar.candle.low += 500.0;
            bar.candle.close += 500.0;
            bar.spread_points = 50_000;
        }
        let replay = evaluate(&changed, &o, &plan, &AtomicBool::new(false), &AtomicUsize::new(0)).unwrap();
        assert_eq!(
            serde_json::to_value(&original.windows[0].meta_v1).unwrap(),
            serde_json::to_value(&replay.windows[0].meta_v1).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&original.windows[0].meta_v2).unwrap(),
            serde_json::to_value(&replay.windows[0].meta_v2).unwrap()
        );
    }

    #[test]
    fn rolling_definitions_are_exact_and_ordered() {
        let o = options();
        let rows = windows(
            &o,
            &EvaluationPlan::Rolling {
                development_days: 2,
                holdout_days: 1,
                step_days: 2,
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].from, BASE + 3 * DAY);
        assert_eq!(rows[0].to, BASE + 5 * DAY);
        assert_eq!(rows[1].from, BASE + 5 * DAY);
        assert_eq!(rows[1].to, BASE + 6 * DAY);
        assert_eq!(rows[2].from, BASE + 5 * DAY);
        assert_eq!(rows[5].to, BASE + 10 * DAY);
        assert!(rows.windows(2).all(|w| w[0].cycle <= w[1].cycle));
    }

    #[test]
    fn ces_abatement_comparison_keeps_time_windows_costs_and_legacy_policy(){
        let h=fixture(12);
        let o=options();
        let plan=EvaluationPlan::Holdout{holdout_percent:30};
        let first=compare_ces_epi(&h,&o,&plan,&AtomicBool::new(false),
            &AtomicUsize::new(0)).unwrap();
        assert_eq!(first.windows.len(),2);
        assert_eq!(first.windows[0].window.to,first.windows[1].window.from);
        assert_eq!(first.windows[0].window.role,WindowRole::Development);
        assert_eq!(first.windows[1].window.role,WindowRole::Holdout);
        assert!(first.research_only&&!first.broker_orders&&!first.automatic_promotion);
        assert!(!first.independently_frozen_oos);
        for w in &first.windows {
            assert_eq!(w.tick_volume_policy.strategy,Strategy::CesV1);
            assert_eq!(w.candle_range_policy.strategy,Strategy::CesRangeResearch);
            for result in [&w.tick_volume_policy,&w.candle_range_policy]{
                assert!(result.first_test_bar>=w.window.from);
                assert!(result.last_test_bar<w.window.to);
            }
            let mut fixed=o.clone();fixed.from=w.window.from;fixed.to=w.window.to;
            fixed.strategy=Strategy::CesV1;
            let direct=summarized(run(&h,&fixed,&AtomicBool::new(false),
                &AtomicUsize::new(0)).unwrap());
            assert_eq!(serde_json::to_value(&w.tick_volume_policy).unwrap(),
                serde_json::to_value(&direct).unwrap(),
                "Comparison may never alter historical CES v1 trading results");
        }
        let mut future=h.clone();
        let split=first.windows[0].window.to;
        for bar in future.bars.iter_mut().filter(|b|b.candle.time>=split) {
            bar.candle.open+=300.;
            bar.candle.high+=300.;
            bar.candle.low+=300.;
            bar.candle.close+=300.;
        }
        let revised=compare_ces_epi(&future,&o,&plan,
            &AtomicBool::new(false),&AtomicUsize::new(0)).unwrap();
        for policy in 0..2{
            let a=&first.windows[0];
            let b=&revised.windows[0];
            let (p,q)=if policy==0{(&a.tick_volume_policy,&b.tick_volume_policy)}
                else{(&a.candle_range_policy,&b.candle_range_policy)};
            assert_eq!(serde_json::to_value(p).unwrap(),serde_json::to_value(q).unwrap(),
                "Future holdout prices must not change development result");
        }
    }

    #[test]
    fn four_brain_comparison_is_chronological_fixed_cost_and_no_promotion(){
        let h=fixture(7);
        let mut o=options();
        o.from=BASE+2*DAY;o.to=BASE+6*DAY;
        let result=compare_brain_signals(&h,&o,
            &EvaluationPlan::Holdout{holdout_percent:30},
            &AtomicBool::new(false),&AtomicUsize::new(0)).unwrap();
        assert!(result.research_only&&!result.broker_orders&&!result.automatic_promotion);
        assert!(!result.independently_frozen_oos);
        assert_eq!(result.windows.len(),2);
        assert_eq!(result.windows[0].window.to,result.windows[1].window.from);
        assert!(result.windows.iter().all(|w|w.minimum_trades_per_arm==30));
        assert!(result.windows.iter().all(|w|w.evidence_status=="insufficient_trades"),
            "Sparse advisor events must never be framed as a proven winner");
        for w in &result.windows {
            for p in [&w.trend_baseline,&w.wyckoff,&w.liquidity_sweep,&w.trend_pullback]{
                assert!(p.first_test_bar>=w.window.from&&p.last_test_bar<w.window.to);
                assert!(p.metrics.commissions>=0.);
            }
        }
        // A future mutation cannot alter an already finished development
        // window; this includes all 4 advisor signals, not just baseline.
        let mut modified=h.clone();
        for bar in modified.bars.iter_mut().filter(|b|b.candle.time>=result.windows[0].window.to){
            bar.candle.open+=100.;
            bar.candle.high+=100.;
            bar.candle.low+=100.;
            bar.candle.close+=100.;
        }
        let rerun=compare_brain_signals(&modified,&o,
            &EvaluationPlan::Holdout{holdout_percent:30},
            &AtomicBool::new(false),&AtomicUsize::new(0)).unwrap();
        assert_eq!(serde_json::to_value(&result.windows[0]).unwrap(),
            serde_json::to_value(&rerun.windows[0]).unwrap());
    }

    #[test]
    fn insufficient_warmup_and_non_meta_are_rejected() {
        let h = fixture(12);
        let mut o = options();
        o.from = BASE + DAY;
        assert!(evaluate(&h, &o, &EvaluationPlan::default(), &AtomicBool::new(false), &AtomicUsize::new(0)).is_err());
        o = options();
        o.strategy = Strategy::CesV1;
        assert!(evaluate(&h, &o, &EvaluationPlan::default(), &AtomicBool::new(false), &AtomicUsize::new(0)).is_err());
    }
}
