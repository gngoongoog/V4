//! Deterministic holdout-only evidence derived from persisted chronological evaluation.
//!
//! Research-only: development windows are deliberately excluded from all OOS aggregates.
//! This module does not promote models, alter live analysis, or affect broker execution.
use crate::{
    backtest::Metrics,
    evaluation::{ChronologicalEvaluation, EvaluationPlan, StrategyWindowResult, WindowRole},
};
use serde::{Deserialize, Serialize};

pub const VERSION: &str = "oos-evidence-v1 / holdout-only-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OosInterval {
    pub cycle: usize,
    pub from: u64,
    pub to: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct StrategyOosStats {
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
    pub win_rate_percent: Option<f64>,
    pub positive_window_rate_percent: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct OosDeltaStats {
    pub windows: usize,
    pub positive_net_delta_windows: usize,
    pub negative_net_delta_windows: usize,
    pub flat_net_delta_windows: usize,
    pub average_net_v2_minus_v1: f64,
    pub average_return_percent_v2_minus_v1: f64,
    pub average_drawdown_percent_v2_minus_v1: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OosEvidence {
    pub version: String,
    pub source_evaluation_version: String,
    pub source_plan: EvaluationPlan,
    pub symbol: String,
    pub server: String,
    pub login: u64,
    pub currency: String,
    pub captured_ms: u64,
    pub holdout_windows: usize,
    pub development_windows_ignored: usize,
    pub intervals: Vec<OosInterval>,
    pub meta_v1: StrategyOosStats,
    pub meta_v2: StrategyOosStats,
    pub delta: OosDeltaStats,
    pub research_only: bool,
    pub automatic_promotion: bool,
    pub broker_effect: bool,
    pub note: String,
}

fn finite_metrics(metrics: &Metrics) -> bool {
    [
        metrics.net,
        metrics.return_percent,
        metrics.max_drawdown,
        metrics.max_drawdown_percent,
        metrics.commissions,
    ]
    .iter()
    .all(|value| value.is_finite())
        && metrics.profit_factor.is_none_or(f64::is_finite)
        && metrics.win_rate.is_none_or(f64::is_finite)
}

fn aggregate<'a>(
    name: &str,
    rows: impl Iterator<Item = &'a StrategyWindowResult>,
) -> Result<StrategyOosStats, String> {
    let mut out = StrategyOosStats {
        strategy: name.into(),
        ..Default::default()
    };
    let mut return_sum = 0.0;
    for row in rows {
        if !finite_metrics(&row.metrics) {
            return Err("OOS source contains non-finite strategy metrics".into());
        }
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
    if out.windows == 0 {
        return Err("OOS evidence requires at least one holdout window".into());
    }
    out.average_return_percent = return_sum / out.windows as f64;
    out.win_rate_percent = (out.wins + out.losses > 0)
        .then_some(out.wins as f64 * 100.0 / (out.wins + out.losses) as f64);
    out.positive_window_rate_percent =
        Some(out.positive_windows as f64 * 100.0 / out.windows as f64);
    Ok(out)
}

pub fn summarize(source: &ChronologicalEvaluation) -> Result<OosEvidence, String> {
    if source.symbol.trim().is_empty()
        || source.server.trim().is_empty()
        || source.login == 0
        || source.currency.trim().is_empty()
    {
        return Err("OOS evaluation identity is incomplete".into());
    }
    let holdout = source
        .windows
        .iter()
        .filter(|row| row.window.role == WindowRole::Holdout)
        .collect::<Vec<_>>();
    if holdout.is_empty() {
        return Err("OOS evidence requires at least one holdout window".into());
    }
    for row in &holdout {
        if row.window.from == 0 || row.window.to <= row.window.from {
            return Err("OOS holdout interval is invalid".into());
        }
        if ![
            row.delta_net_v2_minus_v1,
            row.delta_return_percent_v2_minus_v1,
            row.delta_drawdown_percent_v2_minus_v1,
        ]
        .iter()
        .all(|value| value.is_finite())
        {
            return Err("OOS source contains non-finite delta metrics".into());
        }
    }
    if holdout
        .windows(2)
        .any(|rows| rows[0].window.from >= rows[1].window.from)
    {
        return Err("OOS holdout windows must be strictly chronological".into());
    }

    let meta_v1 = aggregate("meta_v1", holdout.iter().map(|row| &row.meta_v1))?;
    let meta_v2 = aggregate("meta_v2", holdout.iter().map(|row| &row.meta_v2))?;
    let count = holdout.len() as f64;
    let mut delta = OosDeltaStats {
        windows: holdout.len(),
        ..Default::default()
    };
    for row in &holdout {
        if row.delta_net_v2_minus_v1 > 0.0 {
            delta.positive_net_delta_windows += 1;
        } else if row.delta_net_v2_minus_v1 < 0.0 {
            delta.negative_net_delta_windows += 1;
        } else {
            delta.flat_net_delta_windows += 1;
        }
        delta.average_net_v2_minus_v1 += row.delta_net_v2_minus_v1 / count;
        delta.average_return_percent_v2_minus_v1 +=
            row.delta_return_percent_v2_minus_v1 / count;
        delta.average_drawdown_percent_v2_minus_v1 +=
            row.delta_drawdown_percent_v2_minus_v1 / count;
    }

    Ok(OosEvidence {
        version: VERSION.into(),
        source_evaluation_version: source.version.clone(),
        source_plan: source.plan.clone(),
        symbol: source.symbol.clone(),
        server: source.server.clone(),
        login: source.login,
        currency: source.currency.clone(),
        captured_ms: source.captured_ms,
        holdout_windows: holdout.len(),
        development_windows_ignored: source.windows.len().saturating_sub(holdout.len()),
        intervals: holdout
            .iter()
            .map(|row| OosInterval {
                cycle: row.window.cycle,
                from: row.window.from,
                to: row.window.to,
            })
            .collect(),
        meta_v1,
        meta_v2,
        delta,
        research_only: true,
        automatic_promotion: false,
        broker_effect: false,
        note: "This summary uses holdout windows only. Development windows are excluded. It is research evidence, not a profitability claim or automatic promotion decision.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backtest::{Metrics, Options, Strategy},
        evaluation::{
            EvaluationWindow, StrategyAggregate, StrategyWindowResult, WindowEvaluation,
        },
        RiskConfig,
    };

    fn metrics(net: f64, ret: f64, dd: f64, wins: usize, losses: usize) -> Metrics {
        Metrics {
            trades: wins + losses,
            wins,
            losses,
            breakeven: 0,
            net,
            return_percent: ret,
            profit_factor: Some(1.2),
            win_rate: Some(if wins + losses == 0 {
                0.0
            } else {
                wins as f64 * 100.0 / (wins + losses) as f64
            }),
            max_drawdown: dd * 100.0,
            max_drawdown_percent: dd,
            commissions: 1.0,
            ambiguous_trades: 0,
            gap_exits: 0,
            tested_bars: 100,
            skipped: Default::default(),
        }
    }

    fn result(strategy: Strategy, net: f64, ret: f64, dd: f64) -> StrategyWindowResult {
        StrategyWindowResult {
            strategy,
            algorithm: strategy.algorithm().into(),
            metrics: metrics(net, ret, dd, 3, 2),
            first_test_bar: 60,
            last_test_bar: 120,
            gaps: 0,
        }
    }

    fn window(cycle: usize, role: WindowRole, from: u64, v1: f64, v2: f64) -> WindowEvaluation {
        WindowEvaluation {
            window: EvaluationWindow {
                cycle,
                role,
                from,
                to: from + 60,
            },
            meta_v1: result(Strategy::MetaV1, v1, v1 / 100.0, 2.0),
            meta_v2: result(Strategy::MetaV2, v2, v2 / 100.0, 3.0),
            delta_net_v2_minus_v1: v2 - v1,
            delta_return_percent_v2_minus_v1: (v2 - v1) / 100.0,
            delta_drawdown_percent_v2_minus_v1: 1.0,
        }
    }

    fn source() -> ChronologicalEvaluation {
        ChronologicalEvaluation {
            version: "chronological-evaluation-v1".into(),
            plan: EvaluationPlan::Rolling {
                development_days: 2,
                holdout_days: 1,
                step_days: 1,
            },
            symbol: "XAUUSD".into(),
            server: "Demo".into(),
            login: 7,
            currency: "USD".into(),
            captured_ms: 1000,
            history_first: 1,
            history_last: 1000,
            history_bars: 100,
            options_template: Options {
                strategy: Strategy::MetaV1,
                from: 60,
                to: 600,
                initial_equity: 10_000.0,
                spread_floor: 0.02,
                risk: RiskConfig::default(),
            },
            windows: vec![
                window(0, WindowRole::Development, 60, 999.0, -999.0),
                window(0, WindowRole::Holdout, 120, 10.0, 12.0),
                window(1, WindowRole::Development, 180, -555.0, 777.0),
                window(1, WindowRole::Holdout, 240, -5.0, -2.0),
            ],
            meta_v1: StrategyAggregate::default(),
            meta_v2: StrategyAggregate::default(),
            average_delta_net_v2_minus_v1: 0.0,
            average_delta_return_percent_v2_minus_v1: 0.0,
            note: String::new(),
        }
    }

    #[test]
    fn only_holdout_windows_contribute_to_oos_evidence() {
        let report = summarize(&source()).unwrap();
        assert_eq!(report.holdout_windows, 2);
        assert_eq!(report.development_windows_ignored, 2);
        assert_eq!(report.meta_v1.net_sum, 5.0);
        assert_eq!(report.meta_v2.net_sum, 10.0);
        assert_eq!(report.delta.positive_net_delta_windows, 2);
        assert_eq!(report.intervals.len(), 2);
        assert!(report.research_only);
        assert!(!report.automatic_promotion);
        assert!(!report.broker_effect);
    }

    #[test]
    fn changing_development_results_cannot_change_oos_summary() {
        let original = source();
        let baseline = summarize(&original).unwrap();
        let mut changed = original;
        for row in &mut changed.windows {
            if row.window.role == WindowRole::Development {
                row.meta_v1.metrics.net = -1_000_000.0;
                row.meta_v2.metrics.net = 1_000_000.0;
                row.delta_net_v2_minus_v1 = 2_000_000.0;
            }
        }
        assert_eq!(baseline, summarize(&changed).unwrap());
    }

    #[test]
    fn changing_holdout_results_changes_oos_summary() {
        let original = source();
        let baseline = summarize(&original).unwrap();
        let mut changed = original;
        let row = changed
            .windows
            .iter_mut()
            .find(|row| row.window.role == WindowRole::Holdout)
            .unwrap();
        row.meta_v2.metrics.net += 50.0;
        assert_ne!(baseline, summarize(&changed).unwrap());
    }

    #[test]
    fn no_holdout_window_is_rejected() {
        let mut value = source();
        value.windows.retain(|row| row.window.role == WindowRole::Development);
        assert!(summarize(&value).is_err());
    }

    #[test]
    fn non_finite_holdout_metrics_are_rejected() {
        let mut value = source();
        let row = value
            .windows
            .iter_mut()
            .find(|row| row.window.role == WindowRole::Holdout)
            .unwrap();
        row.meta_v2.metrics.net = f64::NAN;
        assert!(summarize(&value).is_err());
    }
}
