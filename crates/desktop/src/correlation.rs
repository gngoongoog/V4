use anyhow::Context;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct CorrelationView {
    pub title: String,
    pub matrix: String,
    pub portfolio: String,
    pub provenance: String,
    pub boundary: String,
}

pub fn source_rows(value: &Value) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let rows = value["sources"]
        .as_array()
        .context("Invalid correlation source list")?;
    let mut ids = Vec::with_capacity(rows.len());
    let mut labels = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row["id"].as_str().context("Correlation source ID missing")?;
        let symbol = row["symbol"].as_str().unwrap_or("?");
        let server = row["server"].as_str().unwrap_or("?");
        let login = row["login"].as_u64().unwrap_or(0);
        let from = row["from"].as_u64().unwrap_or(0);
        let to = row["to"].as_u64().unwrap_or(0);
        ids.push(id.to_owned());
        labels.push(format!("{symbol} | {login}@{server} | {from} → {to} | {}", &id[..id.len().min(8)]));
    }
    Ok((ids, labels))
}

pub fn report_rows(value: &Value) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let rows = value["reports"]
        .as_array()
        .context("Invalid correlation report list")?;
    let mut ids = Vec::with_capacity(rows.len());
    let mut labels = Vec::with_capacity(rows.len());
    for row in rows {
        let summary = &row["summary"];
        let id = summary["id"].as_str().context("Correlation report ID missing")?;
        let symbols = summary["symbols"]
            .as_array()
            .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" × "))
            .unwrap_or_else(|| "?".into());
        labels.push(format!(
            "{symbols} | pairs {}/{} | common {} | {}",
            summary["available_pairs"].as_u64().unwrap_or(0),
            summary["pair_count"].as_u64().unwrap_or(0),
            summary["portfolio"]["overlap"].as_u64().unwrap_or(0),
            &id[..id.len().min(8)]
        ));
        ids.push(id.to_owned());
    }
    Ok((ids, labels))
}

fn fmt_optional(value: &Value, digits: usize) -> String {
    value.as_f64().map(|v| format!("{v:.digits$}")).unwrap_or_else(|| "—".into())
}

pub fn render_report(value: &Value) -> anyhow::Result<CorrelationView> {
    let report = &value["report"];
    let sources = report["sources"]
        .as_array()
        .context("Correlation report sources missing")?;
    let symbols = sources
        .iter()
        .map(|source| source["symbol"].as_str().unwrap_or("?").to_owned())
        .collect::<Vec<_>>();
    if symbols.len() < 2 {
        anyhow::bail!("Correlation report must contain at least two symbols");
    }

    let mut matrix = String::from("PAIRWISE CORRELATION / ارتباط العوائد المتزامنة\n");
    for pair in report["pairs"].as_array().context("Correlation pair list missing")? {
        let left = pair["left"].as_str().unwrap_or("?");
        let right = pair["right"].as_str().unwrap_or("?");
        let overlap = pair["overlap"].as_u64().unwrap_or(0);
        if pair["available"] == true {
            matrix.push_str(&format!(
                "{left} × {right} | r {} | cov {} | n {overlap}\n",
                fmt_optional(&pair["correlation"], 4),
                fmt_optional(&pair["covariance"], 8),
            ));
        } else {
            matrix.push_str(&format!(
                "{left} × {right} | unavailable | n {overlap} | {}\n",
                pair["reason"].as_str().unwrap_or("insufficient evidence")
            ));
        }
    }

    let portfolio = &report["portfolio"];
    let portfolio_text = format!(
        "COMMON-INTERSECTION PORTFOLIO / تقاطع كل الرموز\nSamples: {}\nVolatility: {}\nDiversification ratio: {}\nAvailable: {}\n{}",
        portfolio["overlap"].as_u64().unwrap_or(0),
        fmt_optional(&portfolio["volatility"], 8),
        fmt_optional(&portfolio["diversification_ratio"], 4),
        portfolio["available"].as_bool().unwrap_or(false),
        portfolio["reason"].as_str().unwrap_or("")
    );

    let provenance = value["provenance"]
        .as_array()
        .context("Correlation provenance missing")?
        .iter()
        .map(|row| format!(
            "{}: {}",
            row["ordinal"].as_u64().unwrap_or(0) + 1,
            row["backtest_id"].as_str().unwrap_or("?")
        ))
        .collect::<Vec<_>>()
        .join("\n");

    let title = format!(
        "{} | {} → {} | min overlap {} | ID {}",
        symbols.join(" × "),
        report["from"].as_u64().unwrap_or(0),
        report["to"].as_u64().unwrap_or(0),
        report["min_overlap"].as_u64().unwrap_or(0),
        value["id"].as_str().unwrap_or("?")
    );

    let safe = value["research_only"] == true
        && value["broker_effect"] == false
        && report["execution_effect"] == false
        && report["forward_fill"] == false;
    let boundary = if safe {
        "RESEARCH ONLY — saved backtests only; no live feed, no forward-fill, no broker effect.".to_string()
    } else {
        "BOUNDARY WARNING — report flags are inconsistent; do not use this result.".to_string()
    };

    Ok(CorrelationView {
        title,
        matrix: matrix.trim_end().into(),
        portfolio: portfolio_text.trim_end().into(),
        provenance,
        boundary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_and_report_lists_are_compact_and_stable() {
        let sources = json!({"sources":[{"id":"abcdef123456","symbol":"XAUUSD","server":"Demo","login":7,"from":100,"to":200}]});
        let (ids, labels) = source_rows(&sources).unwrap();
        assert_eq!(ids, vec!["abcdef123456"]);
        assert!(labels[0].contains("XAUUSD"));
        assert!(labels[0].contains("abcdef12"));

        let reports = json!({"reports":[{"summary":{"id":"112233445566","symbols":["XAUUSD","EURUSD"],"available_pairs":1,"pair_count":1,"portfolio":{"overlap":80}}}]});
        let (ids, labels) = report_rows(&reports).unwrap();
        assert_eq!(ids, vec!["112233445566"]);
        assert!(labels[0].contains("XAUUSD × EURUSD"));
        assert!(labels[0].contains("common 80"));
    }

    #[test]
    fn report_formatter_preserves_unavailable_pairs_and_research_boundary() {
        let value = json!({
            "id":"deadbeef",
            "research_only":true,
            "broker_effect":false,
            "provenance":[{"ordinal":0,"backtest_id":"a"},{"ordinal":1,"backtest_id":"b"}],
            "report":{
                "from":100,"to":200,"min_overlap":30,"execution_effect":false,"forward_fill":false,
                "sources":[{"symbol":"XAUUSD"},{"symbol":"EURUSD"}],
                "pairs":[{"left":"EURUSD","right":"XAUUSD","overlap":12,"available":false,"correlation":null,"covariance":null,"reason":"Need at least 30 pairwise-valid returns"}],
                "portfolio":{"overlap":12,"available":false,"volatility":null,"diversification_ratio":null,"reason":"Need at least 30 all-symbol-valid returns"}
            }
        });
        let view = render_report(&value).unwrap();
        assert!(view.matrix.contains("unavailable"));
        assert!(view.matrix.contains("n 12"));
        assert!(view.boundary.contains("RESEARCH ONLY"));
        assert!(view.provenance.contains("1: a"));
    }

    #[test]
    fn unsafe_flags_surface_a_boundary_warning() {
        let value = json!({
            "id":"x","research_only":true,"broker_effect":false,"provenance":[],
            "report":{"from":1,"to":2,"min_overlap":2,"execution_effect":true,"forward_fill":false,"sources":[{"symbol":"A"},{"symbol":"B"}],"pairs":[],"portfolio":{"overlap":0,"available":false,"volatility":null,"diversification_ratio":null,"reason":""}}
        });
        assert!(render_report(&value).unwrap().boundary.contains("WARNING"));
    }
}
