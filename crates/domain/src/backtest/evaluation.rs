use super::*;
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
    if options.from % DAY != 0 || options.to % DAY != 0 {
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
