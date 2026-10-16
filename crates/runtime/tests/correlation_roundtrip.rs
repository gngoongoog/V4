use braintrade_domain::{
    backtest::{Contract, History, HistoryBar},
    correlation::{self, CorrelationConfig, Input, Report},
    Candle,
};

const T0: u64 = 1_800_000_000;

fn contract() -> Contract {
    Contract {
        point: 0.01,
        tick_size: 0.01,
        tick_loss: 1.0,
        tick_profit: 1.0,
        volume_min: 0.01,
        volume_step: 0.01,
        volume_max: 100.0,
    }
}

fn history(symbol: &str, returns: &[f64]) -> History {
    let mut closes = vec![100.0];
    for value in returns {
        closes.push(*closes.last().unwrap() * value.exp());
    }
    let bars = closes
        .into_iter()
        .enumerate()
        .map(|(index, close)| HistoryBar {
            candle: Candle {
                time: T0 + index as u64 * 60,
                open: close,
                high: close,
                low: close,
                close,
                tick_volume: 100,
            },
            spread_points: 10,
        })
        .collect::<Vec<_>>();
    let last = T0 + returns.len() as u64 * 60;
    History {
        symbol: symbol.into(),
        server: "Demo".into(),
        login: 7,
        currency: "USD".into(),
        captured_ms: (last + 60) * 1000,
        contract: contract(),
        bars,
    }
}

fn analyze(a: &History, b: &History) -> Report {
    correlation::analyze(
        vec![
            Input { source_id: "a", from: T0, to: T0 + 5 * 60, history: a },
            Input { source_id: "b", from: T0, to: T0 + 5 * 60, history: b },
        ],
        CorrelationConfig { min_overlap: 4 },
    )
    .unwrap()
}

#[test]
fn report_json_roundtrip_preserves_partial_eq() {
    let a = history("AAA", &[0.01, 0.02, -0.01, 0.03]);
    let b = history("BBB", &[0.02, 0.04, -0.02, 0.06]);
    let report = analyze(&a, &b);
    let raw = serde_json::to_string(&report).unwrap();
    let decoded: Report = serde_json::from_str(&raw).unwrap();
    assert_eq!(decoded, report, "Report JSON roundtrip changed a value");
}

#[test]
fn history_json_roundtrip_reanalysis_is_identical() {
    let a = history("AAA", &[0.01, 0.02, -0.01, 0.03]);
    let b = history("BBB", &[0.02, 0.04, -0.02, 0.06]);
    let original = analyze(&a, &b);
    let a2: History = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    let b2: History = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
    let replayed = analyze(&a2, &b2);
    assert_eq!(replayed, original, "History JSON roundtrip changed deterministic analysis");
}
