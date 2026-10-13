use super::*;
use braintrade_domain::{
    backtest::{History, Report, Strategy},
    meta_v2_compare,
};
use std::{fs, path::Path};

pub(super) fn run(client: &Client, id: &str) -> anyhow::Result<String> {
    let export = client.post("/v1/backtest/export", json!({"id": id}))?;
    let folder = export["path"]
        .as_str()
        .context("Backtest export did not return a folder")?;
    let folder = Path::new(folder);
    let history_path = folder.join("history.json");
    let report_path = folder.join("report.json");
    let history: History = serde_json::from_slice(
        &fs::read(&history_path)
            .with_context(|| format!("تعذر قراءة {}", history_path.display()))?,
    )
    .with_context(|| format!("ملف التاريخ غير صالح: {}", history_path.display()))?;
    let report: Report = serde_json::from_slice(
        &fs::read(&report_path)
            .with_context(|| format!("تعذر قراءة {}", report_path.display()))?,
    )
    .with_context(|| format!("ملف التقرير غير صالح: {}", report_path.display()))?;

    if history.symbol != report.symbol
        || history.server != report.server
        || history.login != report.login
        || history.currency != report.currency
        || history.captured_ms != report.captured_ms
    {
        anyhow::bail!("history.json و report.json لا يعودان لنفس الاختبار المحفوظ");
    }

    let mut options = report.options.clone();
    options.strategy = Strategy::MetaV1;
    let comparison = meta_v2_compare::compare(&history, &options)
        .map_err(anyhow::Error::msg)
        .context("فشلت مقارنة Meta v1 / Meta v2")?;
    let summary = comparison.summary();
    let json_path = folder.join("meta_v2_comparison.json");
    let text_path = folder.join("meta_v2_comparison.txt");
    fs::write(&json_path, serde_json::to_vec_pretty(&comparison)?)
        .with_context(|| format!("تعذر حفظ {}", json_path.display()))?;
    fs::write(&text_path, format!("{summary}\n"))
        .with_context(|| format!("تعذر حفظ {}", text_path.display()))?;
    Ok(format!(
        "{summary}\n\nتم حفظ المقارنة:\n{}\n{}",
        json_path.display(),
        text_path.display()
    ))
}
