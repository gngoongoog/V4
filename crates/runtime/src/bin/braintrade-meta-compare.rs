use anyhow::{bail, Context, Result};
use braintrade_domain::{
    backtest::{History, Report, Strategy},
    meta_v2_compare,
};
use std::{env, fs, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("Meta comparison failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let folder = args
        .next()
        .map(PathBuf::from)
        .context("Usage: braintrade-meta-compare <export-folder> [--stdout]")?;
    let stdout_only = match args.next().as_deref() {
        None => false,
        Some("--stdout") => true,
        Some(other) => bail!("Unknown argument: {other}"),
    };
    if args.next().is_some() {
        bail!("Too many arguments");
    }

    let history_path = folder.join("history.json");
    let report_path = folder.join("report.json");
    let history: History = serde_json::from_slice(
        &fs::read(&history_path)
            .with_context(|| format!("Cannot read {}", history_path.display()))?,
    )
    .with_context(|| format!("Invalid {}", history_path.display()))?;
    let report: Report = serde_json::from_slice(
        &fs::read(&report_path)
            .with_context(|| format!("Cannot read {}", report_path.display()))?,
    )
    .with_context(|| format!("Invalid {}", report_path.display()))?;

    if history.symbol != report.symbol
        || history.server != report.server
        || history.login != report.login
        || history.currency != report.currency
    {
        bail!("history.json and report.json do not describe the same account/symbol context");
    }
    if history.captured_ms != report.captured_ms {
        bail!("history.json and report.json were not captured from the same saved run");
    }

    let mut options = report.options.clone();
    options.strategy = Strategy::MetaV1;
    let comparison = meta_v2_compare::compare(&history, &options)
        .map_err(anyhow::Error::msg)
        .context("Meta v1 vs Meta v2 comparison failed")?;
    let text = comparison.summary();
    let json = serde_json::to_vec_pretty(&comparison)?;

    if stdout_only {
        println!("{text}\n\n{}", String::from_utf8(json)?);
        return Ok(());
    }

    let json_path = folder.join("meta_v2_comparison.json");
    let text_path = folder.join("meta_v2_comparison.txt");
    fs::write(&json_path, &json)
        .with_context(|| format!("Cannot write {}", json_path.display()))?;
    fs::write(&text_path, format!("{text}\n"))
        .with_context(|| format!("Cannot write {}", text_path.display()))?;
    println!(
        "Meta comparison complete.\n{}\n{}",
        json_path.display(),
        text_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{
        backtest::{Contract, HistoryBar, Metrics, Options},
        Candle, RiskConfig,
    };

    #[test]
    fn exported_context_mismatch_is_detectable_before_comparison() {
        let history = History {
            symbol: "XAUUSD".into(),
            server: "A".into(),
            login: 1,
            currency: "USD".into(),
            captured_ms: 10,
            contract: Contract {
                point: 0.01,
                tick_size: 0.01,
                tick_loss: 1.0,
                tick_profit: 1.0,
                volume_min: 0.01,
                volume_step: 0.01,
                volume_max: 1.0,
            },
            bars: vec![HistoryBar {
                candle: Candle {
                    time: 60,
                    open: 1.0,
                    high: 1.1,
                    low: 0.9,
                    close: 1.0,
                    tick_volume: 1,
                },
                spread_points: 1,
            }],
        };
        let report = Report {
            algorithm: "x".into(),
            symbol: "XAUUSD".into(),
            server: "B".into(),
            login: 1,
            currency: "USD".into(),
            captured_ms: 10,
            options: Options {
                strategy: Strategy::MetaV1,
                from: 60,
                to: 120,
                initial_equity: 1000.0,
                spread_floor: 0.01,
                risk: RiskConfig::default(),
            },
            contract: history.contract.clone(),
            history_first: 60,
            history_last: 60,
            first_test_bar: 60,
            last_test_bar: 60,
            gaps: 0,
            metrics: Metrics::default(),
            trades: vec![],
            equity: vec![],
            warnings: vec![],
        };
        assert_ne!(history.server, report.server);
    }
}