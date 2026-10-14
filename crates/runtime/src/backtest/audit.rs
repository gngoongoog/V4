use super::*;
use braintrade_domain::backtest::CandidateDecision;
use std::{collections::BTreeMap, fmt::Write};

pub(super) fn summary(decisions: &[CandidateDecision]) -> String {
    if decisions.is_empty() { return String::new() }
    let mut outcomes = BTreeMap::new();
    let mut gates = BTreeMap::new();
    for d in decisions {
        *outcomes.entry(d.outcome.as_str()).or_insert(0) += 1;
        for gate in &d.candidate.gates { *gates.entry(gate.as_str()).or_insert(0) += 1; }
    }
    let mut out = format!("سجل Meta v2: {} قراراً عند افتتاح الدقائق؛ evidence قوة خصائص وليست احتمال ربح.\nنتائج قبول الدخول:\n",decisions.len());
    for (reason,count) in outcomes { let _ = writeln!(out,"{reason}: {count}"); }
    out.push_str("بوابات المرشح (قد توجد أكثر من بوابة بالدقيقة):\n");
    for (reason,count) in gates { let _ = writeln!(out,"{reason}: {count}"); }
    out
}
pub(super) fn csv(decisions: &[CandidateDecision]) -> String {
    let mut out = "time_utc,signal_bar_utc,current,atr,legacy_action,candidate_action,eligible,direction,evidence,momentum_atr,gates,data_reason,outcome\n".to_owned();
    for d in decisions {
        let c = &d.candidate;
        let _ = writeln!(out,"{},{},{},{},{},{},{},{},{:.10},{:.10},{},{},{}", d.time,d.signal_bar,d.current,d.atr.map(|v|format!("{v:.10}")).unwrap_or_default(),csv_cell(&d.legacy_action),csv_cell(&c.action),c.eligible,c.direction,c.evidence,c.momentum_atr,csv_cell(&c.gates.join(" | ")),csv_cell(&d.data_reason),csv_cell(&d.outcome));
    }
    out
}
pub(super) fn html(decisions: &[CandidateDecision]) -> String {
    if decisions.is_empty() { return String::new() }
    let mut out = format!("<section id=\"candidate-audit\"><h2>قرارات Meta v2 وبواباتها</h2><pre>{}</pre><p>آخر 50 قراراً؛ كل الدقائق محفوظة في decisions.csv وreport.json. القرار يستخدم الشموع المكتملة قبل الافتتاح؛ النتيجة تبين هل سمحت المحاكاة بالدخول.</p><div class=\"scroll\"><table><thead><tr><th>الوقت UTC</th><th>Meta v1</th><th>Meta v2</th><th>قوة الخصائص</th><th>البوابات / البيانات</th><th>نتيجة الدخول</th></tr></thead><tbody>",escape(&summary(decisions)));
    for d in decisions.iter().rev().take(50) {
        let _ = write!(out,"<tr data-candidate-time=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td><td>{}</td><td>{}</td></tr>",d.time,utc(d.time),escape(&d.legacy_action),escape(&d.candidate.action),d.candidate.evidence,escape(&format!("{} {}",d.candidate.gates.join(" | "),d.data_reason)),escape(&d.outcome));
    }
    out.push_str("</tbody></table></div></section>");
    out
}
