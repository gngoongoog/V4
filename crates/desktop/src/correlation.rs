use super::*;
use anyhow::Context;
use serde_json::Value;
use slint::{Model, ModelRc, SharedString, VecModel};

#[derive(Clone, Debug, PartialEq)]
pub struct CorrelationView {
    pub title: String,
    pub matrix: String,
    pub portfolio: String,
    pub provenance: String,
    pub boundary: String,
}

pub fn source_rows(value: &Value) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let rows = value["sources"].as_array().context("Invalid correlation source list")?;
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
    let rows = value["reports"].as_array().context("Invalid correlation report list")?;
    let mut ids = Vec::with_capacity(rows.len());
    let mut labels = Vec::with_capacity(rows.len());
    for row in rows {
        let summary = &row["summary"];
        let id = summary["id"].as_str().context("Correlation report ID missing")?;
        let symbols = summary["symbols"].as_array()
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
    let sources = report["sources"].as_array().context("Correlation report sources missing")?;
    let symbols = sources.iter().map(|source| source["symbol"].as_str().unwrap_or("?").to_owned()).collect::<Vec<_>>();
    if symbols.len() < 2 { anyhow::bail!("Correlation report must contain at least two symbols"); }

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

    let provenance = value["provenance"].as_array().context("Correlation provenance missing")?
        .iter().map(|row| format!("{}: {}", row["ordinal"].as_u64().unwrap_or(0) + 1, row["backtest_id"].as_str().unwrap_or("?")))
        .collect::<Vec<_>>().join("\n");

    let title = format!(
        "{} | {} → {} | min overlap {} | ID {}",
        symbols.join(" × "), report["from"].as_u64().unwrap_or(0), report["to"].as_u64().unwrap_or(0),
        report["min_overlap"].as_u64().unwrap_or(0), value["id"].as_str().unwrap_or("?")
    );
    let safe = value["research_only"] == true && value["broker_effect"] == false
        && report["execution_effect"] == false && report["forward_fill"] == false;
    let boundary = if safe {
        "RESEARCH ONLY — saved backtests only; no live feed, no forward-fill, no broker effect.".to_string()
    } else {
        "BOUNDARY WARNING — report flags are inconsistent; do not use this result.".to_string()
    };
    Ok(CorrelationView { title, matrix: matrix.trim_end().into(), portfolio: portfolio_text.trim_end().into(), provenance, boundary })
}

fn model(values: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(values.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

fn refresh_lists(client: &Client, weak: &slint::Weak<Dashboard>) -> anyhow::Result<()> {
    let sources = client.get("/v1/research/correlation/sources")?;
    let reports = client.get("/v1/research/correlation/reports")?;
    let (source_ids, source_labels) = source_rows(&sources)?;
    let (report_ids, report_labels) = report_rows(&reports)?;
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_corr_source_ids(model(source_ids));
        ui.set_corr_sources(model(source_labels));
        ui.set_corr_report_ids(model(report_ids));
        ui.set_corr_reports(model(report_labels));
        if ui.get_corr_source_a() < 0 && ui.get_corr_sources().row_count() > 0 { ui.set_corr_source_a(0); }
        if ui.get_corr_source_b() < 0 && ui.get_corr_sources().row_count() > 1 { ui.set_corr_source_b(1); }
        if ui.get_corr_report_selected() < 0 && ui.get_corr_reports().row_count() > 0 { ui.set_corr_report_selected(0); }
    });
    Ok(())
}

fn show_report(client: &Client, weak: &slint::Weak<Dashboard>, id: &str) -> anyhow::Result<()> {
    let value = client.post("/v1/research/correlation/report", json!({"id":id}))?;
    let view = render_report(&value)?;
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_corr_title(view.title.into());
        ui.set_corr_matrix(view.matrix.into());
        ui.set_corr_portfolio(view.portfolio.into());
        ui.set_corr_provenance(view.provenance.into());
        ui.set_corr_boundary(view.boundary.into());
    });
    Ok(())
}

fn selected_sources(ui: &Dashboard) -> anyhow::Result<Vec<String>> {
    let ids = ui.get_corr_source_ids();
    let indexes = [ui.get_corr_source_a(), ui.get_corr_source_b(), ui.get_corr_source_c(), ui.get_corr_source_d(),
        ui.get_corr_source_e(), ui.get_corr_source_f(), ui.get_corr_source_g(), ui.get_corr_source_h()];
    let mut selected = Vec::new();
    for index in indexes.into_iter().filter(|v| *v >= 0) {
        let Some(id) = ids.row_data(index as usize) else { anyhow::bail!("اختيار مصدر correlation غير صالح"); };
        let id = id.to_string();
        if selected.contains(&id) { anyhow::bail!("اختر كل نتيجة backtest مرة واحدة فقط"); }
        selected.push(id);
    }
    if selected.len() < 2 { anyhow::bail!("اختر نتيجتين محفوظتين على الأقل"); }
    Ok(selected)
}

struct Request { action: String, body: Value }

pub fn setup(ui: &Dashboard) {
    let (tx, rx) = mpsc::channel::<Request>();
    let weak = ui.as_weak();
    ui.on_correlation_action(move |action| {
        let Some(ui) = weak.upgrade() else { return; };
        let action = action.to_string();
        let body = if action == "analyze" {
            let source_ids = match selected_sources(&ui) { Ok(v) => v, Err(e) => { ui.set_feedback(e.to_string().into()); return; } };
            let min_overlap = match ui.get_corr_min_overlap().trim().parse::<usize>() {
                Ok(v) if v >= 2 => v,
                _ => { ui.set_feedback("أدخل حد overlap عددي 2 أو أكثر".into()); return; }
            };
            json!({"source_ids":source_ids,"min_overlap":min_overlap})
        } else if action == "load" {
            let index = ui.get_corr_report_selected();
            let Some(id) = (index >= 0).then(|| ui.get_corr_report_ids().row_data(index as usize)).flatten() else {
                ui.set_feedback("اختر تقرير correlation محفوظ أولاً".into()); return;
            };
            json!({"id":id.to_string()})
        } else { json!({}) };
        ui.set_corr_busy(true);
        let _ = tx.send(Request { action, body });
    });

    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut client = None;
        loop {
            let request = match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(v) => Some(v), Err(mpsc::RecvTimeoutError::Timeout) => None, Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if client.is_none() { client = Client::new(&data_dir()).ok(); }
            let Some(c) = client.as_ref() else {
                if request.is_some() { let _ = weak.upgrade_in_event_loop(|ui| { ui.set_corr_busy(false); ui.set_corr_status("Core unavailable".into()); }); }
                continue;
            };
            let Some(request) = request else { continue; };
            let result = (|| -> anyhow::Result<String> {
                match request.action.as_str() {
                    "refresh" => { refresh_lists(c, &weak)?; Ok("تم تحديث مصادر وتقارير correlation".into()) }
                    "analyze" => {
                        let value = c.post("/v1/research/correlation/analyze", request.body)?;
                        let id = value["summary"]["id"].as_str().context("Correlation result ID missing")?.to_owned();
                        show_report(c, &weak, &id)?; refresh_lists(c, &weak)?;
                        Ok("اكتمل تحليل correlation وحُفظ التقرير".into())
                    }
                    "load" => { show_report(c, &weak, request.body["id"].as_str().context("Correlation selection missing")?)?; Ok("فُتح تقرير correlation المحفوظ".into()) }
                    _ => anyhow::bail!("Unknown correlation desktop action"),
                }
            })();
            let (message, ok) = match result { Ok(v) => (v, true), Err(e) => (e.to_string(), false) };
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_corr_busy(false); ui.set_corr_status(message.clone().into()); ui.set_feedback(message.into());
                if !ok { ui.set_corr_boundary("RESEARCH ONLY — request failed; no broker effect.".into()); }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test] fn source_and_report_lists_are_compact_and_stable() {
        let sources=json!({"sources":[{"id":"abcdef123456","symbol":"XAUUSD","server":"Demo","login":7,"from":100,"to":200}]});
        let (ids,labels)=source_rows(&sources).unwrap(); assert_eq!(ids,vec!["abcdef123456"]); assert!(labels[0].contains("XAUUSD")); assert!(labels[0].contains("abcdef12"));
        let reports=json!({"reports":[{"summary":{"id":"112233445566","symbols":["XAUUSD","EURUSD"],"available_pairs":1,"pair_count":1,"portfolio":{"overlap":80}}}]});
        let (ids,labels)=report_rows(&reports).unwrap(); assert_eq!(ids,vec!["112233445566"]); assert!(labels[0].contains("XAUUSD × EURUSD")); assert!(labels[0].contains("common 80"));
    }
    #[test] fn report_formatter_preserves_unavailable_pairs_and_research_boundary() {
        let value=json!({"id":"deadbeef","research_only":true,"broker_effect":false,"provenance":[{"ordinal":0,"backtest_id":"a"},{"ordinal":1,"backtest_id":"b"}],"report":{"from":100,"to":200,"min_overlap":30,"execution_effect":false,"forward_fill":false,"sources":[{"symbol":"XAUUSD"},{"symbol":"EURUSD"}],"pairs":[{"left":"EURUSD","right":"XAUUSD","overlap":12,"available":false,"correlation":null,"covariance":null,"reason":"Need at least 30 pairwise-valid returns"}],"portfolio":{"overlap":12,"available":false,"volatility":null,"diversification_ratio":null,"reason":"Need at least 30 all-symbol-valid returns"}}});
        let view=render_report(&value).unwrap(); assert!(view.matrix.contains("unavailable")); assert!(view.matrix.contains("n 12")); assert!(view.boundary.contains("RESEARCH ONLY")); assert!(view.provenance.contains("1: a"));
    }
    #[test] fn unsafe_flags_surface_a_boundary_warning() {
        let value=json!({"id":"x","research_only":true,"broker_effect":false,"provenance":[],"report":{"from":1,"to":2,"min_overlap":2,"execution_effect":true,"forward_fill":false,"sources":[{"symbol":"A"},{"symbol":"B"}],"pairs":[],"portfolio":{"overlap":0,"available":false,"volatility":null,"diversification_ratio":null,"reason":""}}});
        assert!(render_report(&value).unwrap().boundary.contains("WARNING"));
    }
}
