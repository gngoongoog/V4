//! Experiment lineage is separate from deterministic Report JSON.
use super::*;

pub(super) fn source(db: &Connection, id: &str) -> Result<Option<String>> {
    Ok(db.query_row("SELECT source_id FROM backtest_experiments WHERE run_id=?1", [id], |r| r.get(0)).optional()?)
}

fn verify(a: &Report, ah: &History, b: &Report, bh: &History) -> Result<()> {
    let mut options = b.options.clone();
    options.strategy = a.options.strategy;
    if serde_json::to_value(ah)? != serde_json::to_value(bh)?
        || serde_json::to_value(&a.options)? != serde_json::to_value(options)? {
        bail!("تعذرت المقارنة: البيانات أو الفترة أو التكاليف غير متطابقة");
    }
    Ok(())
}

pub(super) fn load(db: &Connection, id: &str, r: &Report, h: &History) -> Result<Option<Value>> {
    let Some(source_id) = source(db, id)? else { return Ok(None) };
    let (baseline, history) = Service::load(db, &source_id)?;
    verify(&baseline, &history, r, h)?;
    Ok(Some(json!({
        "source_id":source_id,
        "source_algorithm":baseline.algorithm,
        "target_algorithm":r.algorithm,
        "identical_history":true,
        "identical_options_except_strategy":true,
        "currency":r.currency,
        "from":r.options.from,"to":r.options.to,
        "initial_equity":r.options.initial_equity,
        "spread_floor":r.options.spread_floor,"risk":r.options.risk,
        "source_metrics":baseline.metrics,"target_metrics":r.metrics,
        "delta_net":r.metrics.net-baseline.metrics.net,
        "delta_drawdown_percent":r.metrics.max_drawdown_percent-baseline.metrics.max_drawdown_percent
    })))
}

fn number(v: &Value) -> String {
    v.as_f64().map(|n| format!("{n:.2}")).unwrap_or_else(|| "غير معرّف".into())
}
const FIELDS: [(&str, &str); 8] = [
    ("trades", "الصفقات"), ("net", "الصافي بعد التكاليف"),
    ("return_percent", "العائد %"), ("max_drawdown_percent", "أقصى تراجع %"),
    ("profit_factor", "معامل الربح"), ("win_rate", "الصفقات الرابحة %"),
    ("commissions", "العمولات"), ("gap_exits", "خروج بسبب فجوة")
];
pub(super) fn text(c: &Value) -> String {
    let mut out = format!("مقارنة بنفس البيانات والتكاليف\nالأصل: {}\nالتجربة: {}\n", c["source_algorithm"].as_str().unwrap_or(""), c["target_algorithm"].as_str().unwrap_or(""));
    for (field,label) in FIELDS {
        out.push_str(&format!("{label}: {} → {}\n",number(&c["source_metrics"][field]),number(&c["target_metrics"][field])));
    }
    out.push_str(&format!("فرق الصافي (التجربة − الأصل): {} {}\nهذه مقارنة داخل العينة؛ لا تحدد أفضلية مستقبلية.\n",number(&c["delta_net"]),c["currency"].as_str().unwrap_or("")));
    out
}
pub(super) fn html(c: &Value) -> String {
    let mut out = format!("<section id=\"experiment-comparison\"><h2>مقارنة بنفس البيانات والتكاليف</h2><p>الأصل: {}<br>التجربة: {}</p><div class=\"scroll\"><table><thead><tr><th>المقياس</th><th>الأصل</th><th>التجربة</th></tr></thead><tbody>",escape(c["source_algorithm"].as_str().unwrap_or("")),escape(c["target_algorithm"].as_str().unwrap_or("")));
    for (field,label) in FIELDS {
        out.push_str(&format!("<tr data-comparison-metric=\"{field}\"><td>{label}</td><td>{}</td><td>{}</td></tr>",number(&c["source_metrics"][field]),number(&c["target_metrics"][field])));
    }
    out.push_str(&format!("</tbody></table></div><p>فرق الصافي: {} {}. تطابقت الشموع والفترة ورأس المال ومواصفات العقد والسبريد والانزلاق والعمولات وإعدادات المخاطرة. المقارنة داخل العينة ولا تثبت أفضلية مستقبلية.</p><small>النتيجة الأصلية: {}</small></section>",number(&c["delta_net"]),escape(c["currency"].as_str().unwrap_or("")),escape(c["source_id"].as_str().unwrap_or(""))));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_rejects_different_costs_or_candles() {
        let history: History = serde_json::from_value(json!({
            "symbol":"TEST","server":"FIXTURE","login":1,"currency":"USD","captured_ms":1800000,
            "contract":{"point":0.01,"tick_size":0.01,"tick_loss":1.,"tick_profit":1.,"volume_min":0.01,"volume_step":0.01,"volume_max":1.},
            "bars":[{"time":600,"open":100.,"high":101.,"low":99.,"close":100.,"tick_volume":1,"spread_points":2},
                    {"time":660,"open":100.,"high":101.,"low":99.,"close":100.,"tick_volume":1,"spread_points":2}]
        })).unwrap();
        let o = Options {strategy:Strategy::MetaV1,from:660,to:720,initial_equity:10000.,spread_floor:0.02,risk:Default::default()};
        let a = backtest::run(&history,&o,&AtomicBool::new(false),&AtomicUsize::new(0)).unwrap();
        let mut b = a.clone(); b.options.strategy = Strategy::MetaV2;
        assert!(verify(&a,&history,&b,&history).is_ok());
        b.options.risk.commission_per_lot += 1.;
        assert!(verify(&a,&history,&b,&history).is_err());
        b.options = a.options.clone(); b.options.to += 60;
        assert!(verify(&a,&history,&b,&history).is_err());
        b.options = a.options.clone();
        let mut changed = history.clone(); changed.bars[0].spread_points += 1;
        assert!(verify(&a,&history,&b,&changed).is_err());
        changed = history.clone(); changed.contract.tick_loss += 0.1;
        assert!(verify(&a,&history,&b,&changed).is_err());
        changed = history.clone(); changed.bars[0].candle.close += 0.1;
        assert!(verify(&a,&history,&b,&changed).is_err());
    }
}
