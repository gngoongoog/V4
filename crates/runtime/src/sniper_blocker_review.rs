//! Secondary, read-only analysis of Stage 3F-U's *already sampled* Demo
//! sniper_opportunity_samples. No second sampler, quote simulation or order
//! mutation. Multiple execution gates can cause the SAME blocked decision.
use anyhow::Result;
use rusqlite::{params,Connection};
use serde_json::{json,Value};
use std::collections::{BTreeMap,BTreeSet};

const REPORT_LIMIT:i64=800;
const WINDOW_MS:u64=24*60*60*1000;
const MIN_SAMPLES:u64=30;

fn strategy_reason(s:&str)->&'static str {
    if s.contains("ينتظر سعراً حديثاً"){"stale_quote"}
    else if s.contains("بيانات السعر أو العقد غير مكتملة"){"missing_contract_data"}
    else if s.contains("يجمع ثلاثة أسعار"){"initial_tick_warmup"}
    else if s.contains("الشروط غير مكتملة"){"setup_not_formed"}
    else if s.contains("لا هدف ضمن")||s.contains("لا يجتاز حصة التكلفة"){"cost_vs_target"}
    else if s.contains("مشاركة القناص موقوفة"){"specialist_busy_or_disabled"}
    else{"other_unready_reason"}
}
fn gate_code(s:&str)->&'static str {
    match s {
      "النظام متوقف"=>"engine_stopped",
      "قاطع الخسارة اليومي للحساب: الحد بلغ أو بيانات اليوم غير موثوقة"=>"daily_breaker",
      "المخاطرة فوق 2% تجريبية على حساب MT5 Demo فقط؛ REAL يبقى بحد 2%"=>"demo_only_risk",
      "أمر قيد التنفيذ أو المصالحة"=>"unsettled_intent",
      "للعقل صفقة مفتوحة"=>"open_position",
      "التداول الآلي غير مسموح في MT5"=>"mt5_trading_disabled",
      "حساب Netting: الرمز مملوك لصفقة حالية"=>"netting_symbol_occupied",
      "فاصل الدخول المختار"=>"cooldown",
      "حد التراجع الاختياري من أعلى Equity مرصود لهذا الحساب والرمز"=>"drawdown_limit",
      "حد اللوت الاختياري"=>"lot_limit",
      "ميزانية المخاطرة الاختيارية"=>"stop_risk_budget",
      "حد السبريد الاختياري: سبريد MT5 تجاوز نسبته من ATR الحقيقي"=>"spread_atr_limit",
      "حد السبريد الاختياري يحتاج شموع M1 لحساب ATR الحقيقي"=>"atr_unavailable",
      "تمت معالجة هذه الإشارة"=>"already_processed",
      "Netting: أمر لعقل آخر لم تتم تسويته"=>"netting_unsettled",
      "SHADOW / PAPER فقط: الوضع المختار يمنع هذا العقل من إرسال صفقات إلى MT5"=>"advisor_paper_only",
      _=>"other_execution_gate",
    }
}
fn label(k:&str)->&'static str{
    match k {
      "other_unready_reason"=>"سبب عدم جاهزية غير مصنف",
      "stale_quote"=>"سعر القناص قديم",
      "missing_contract_data"=>"بيانات السعر أو العقد غير مكتملة",
      "initial_tick_warmup"=>"يحتاج ثلاث أسعار متتابعة",
      "setup_not_formed"=>"شروط حركة القناص لم تكتمل",
      "cost_vs_target"=>"تكلفة التنفيذ مرتفعة بالنسبة للهدف",
      "specialist_busy_or_disabled"=>"القناص مشغول أو مشاركته موقوفة",
      "other_unready_reason"=>"سبب الاستراتيجية غير معروف",
      "engine_stopped"=>"النظام متوقف",
      "daily_breaker"=>"قاطع الخسارة اليومي",
      "demo_only_risk"=>"مخاطرة محصورة بحساب تجريبي",
      "unsettled_intent"=>"أمر بانتظار التسوية",
      "open_position"=>"صفقة موجودة",
      "mt5_trading_disabled"=>"MT5 يمنع التداول الآلي",
      "netting_symbol_occupied"=>"رمز محجوز في حساب Netting",
      "cooldown"=>"فاصل الدخول",
      "drawdown_limit"=>"حد التراجع",
      "lot_limit"=>"حد اللوت",
      "stop_risk_budget"=>"ميزانية المخاطرة",
      "spread_atr_limit"=>"السبريد تجاوز ATR",
      "atr_unavailable"=>"ATR غير متوفر",
      "already_processed"=>"إشارة سبق التعامل معها",
      "netting_unsettled"=>"صفقة Netting غير محسومة",
      "advisor_paper_only"=>"عقل مراقبة ورقي فقط",
      _=>"بوابة غير مصنفة",
    }
}
pub fn report(db:&Connection,context:&str,now:u64)->Result<Value>{
    if context.is_empty()||now==0{
        return Ok(json!({"status":"unbound","sampled_decisions":0,"execution_effect":false}));
    }
    let cutoff=now.saturating_sub(WINDOW_MS);
    let mut stmt=db.prepare("SELECT observed_ms,payload FROM sniper_opportunity_samples
        WHERE context=?1 AND observed_ms>=?2 AND observed_ms<=?3
        ORDER BY observed_ms DESC LIMIT ?4")?;
    let rows=stmt.query_map(params![context,cutoff,now,REPORT_LIMIT],
        |r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let scanned=rows.len();
    let mut valid=0u64;
    let mut unready=0u64;
    let mut blocked=0u64;
    let mut eligible=0u64;
    let mut malformed=0u64;
    let mut cause_counts=BTreeMap::<(&'static str,&'static str),u64>::new();
    for (observed,raw) in rows {
        let Some(p)=serde_json::from_str::<Value>(&raw).ok().filter(|v|v.is_object()) else {
            malformed+=1;continue;
        };
        let (Some(signal_ready),Some(entry_ready))=
            (p["signal_ready"].as_bool(),p["entry_ready"].as_bool()) else{
            malformed+=1;continue;
        };
        if p["observed_ms"].as_u64()!=Some(observed)
            || (entry_ready&&!signal_ready) {
            malformed+=1;continue;
        }
        let Some(gates)=p["gates"].as_array() else{
            malformed+=1;continue;
        };
        if entry_ready && !gates.is_empty() {malformed+=1;continue;}
        if gates.len()>20 || gates.iter().any(|g|g.as_str().is_none()) {
            malformed+=1;continue;
        }
        valid+=1;
        if !signal_ready {
            unready+=1;
            let code=strategy_reason(p["signal_reason"].as_str().unwrap_or(""));
            *cause_counts.entry(("strategy",code)).or_default()+=1;
        }else if !entry_ready {
            blocked+=1;
            let mut found=BTreeSet::new();
            for g in gates {
                found.insert(gate_code(g.as_str().unwrap()));
            }
            if found.is_empty(){found.insert("other_execution_gate");}
            for cause in found {
                *cause_counts.entry(("execution",cause)).or_default()+=1;
            }
        }else{
            eligible+=1;
        }
    }
    let mut causes=cause_counts.into_iter().map(|((dimension,code),count)|json!({
        "dimension":dimension,"code":code,"label_ar":label(code),"sampled_decisions_with_cause":count
    })).collect::<Vec<_>>();
    causes.sort_by(|a,b|
        b["sampled_decisions_with_cause"].as_u64().cmp(&a["sampled_decisions_with_cause"].as_u64())
        .then_with(||a["code"].as_str().cmp(&b["code"].as_str())));
    Ok(json!({
        "policy":"sniper_opportunity_multigate_breakdown_v1",
        "status":if valid>=MIN_SAMPLES{"descriptive_sample_only"}else{"insufficient_sample"},
        "source":"existing 3F-U sniper_opportunity_samples only",
        "account_symbol_scoped":true,"demo_only_sample_origin":true,
        "window_start_ms":cutoff,"window_end_ms":now,
        "report_row_limit":REPORT_LIMIT,"retention_in_source":4000,
        "scanned_rows":scanned,"sampled_decisions":valid,
        "malformed_or_inconsistent_rows":malformed,
        "strategy_not_ready":unready,"strategy_ready_but_blocked":blocked,
        "entry_eligible_at_snapshot":eligible,
        "top_causes":causes.into_iter().take(16).collect::<Vec<_>>(),
        "overlapping_execution_gates":true,
        "not_all_ticks_or_distinct_opportunities":true,
        "sent_orders_verified":false,"broker_fill_verified":false,
        "hypothetical_winning_signal_verified":false,
        "missed_profit_estimate":Value::Null,
        "execution_effect":false,"learning_effect":false,
        "warning":"One record is one sampled evaluation, NOT one independent setup. Readiness and outcome categories are exclusive, but multiple gates can block a single ready decision. This report never estimates profit lost to a rejection. Does not create samples and cannot reconstruct time prior to instrumented 3F-U."
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db()->Connection{
        let d=Connection::open_in_memory().unwrap();
        crate::sniper_opportunities::init(&d).unwrap();
        d
    }
    fn add(d:&Connection,ctx:&str,stamp:u64,ready:bool,entry:bool,gates:Vec<&str>,reason:&str){
        d.execute("INSERT INTO sniper_opportunity_samples(context,observed_ms,pending,payload)
            VALUES(?1,?2,0,?3)",params![ctx,stamp,json!({
            "observed_ms":stamp,"signal_ready":ready,"entry_ready":entry,
            "gates":gates,"signal_reason":reason,"reason":reason
        }).to_string()]).unwrap();
    }
    #[test]
    fn distinguishes_unready_blocked_and_eligible_without_new_recorder(){
        let d=db();
        let now=90_000_000;
        add(&d,"a",now-1000,false,false,vec![],"القناص: ينتظر سعراً حديثاً");
        add(&d,"a",now-2000,true,false,vec!["فاصل الدخول المختار","ميزانية المخاطرة الاختيارية","فاصل الدخول المختار"],"ready");
        add(&d,"a",now-3000,true,true,vec![],"ready");
        let r=report(&d,"a",now).unwrap();
        assert_eq!(r["scanned_rows"],3);
        assert_eq!(r["sampled_decisions"],3);
        assert_eq!(r["strategy_not_ready"],1);
        assert_eq!(r["strategy_ready_but_blocked"],1);
        assert_eq!(r["entry_eligible_at_snapshot"],1);
        let causes=r["top_causes"].as_array().unwrap();
        assert_eq!(causes.iter().find(|x|x["code"]=="cooldown").unwrap()["sampled_decisions_with_cause"],1);
        assert_eq!(causes.iter().find(|x|x["code"]=="stop_risk_budget").unwrap()["sampled_decisions_with_cause"],1);
        assert_eq!(r["missed_profit_estimate"],Value::Null);
        assert_eq!(r["broker_fill_verified"],false);
    }
    #[test]
    fn scoped_and_corrupt_or_contradictory_samples_excluded(){
        let d=db();let now=190_000_000;
        add(&d,"a",now-1000,true,true,vec![],"");
        add(&d,"b",now-2000,true,true,vec![],"");
        add(&d,"a",now-30*60*60*1000,true,true,vec![],"");
        add(&d,"a",now-3000,false,true,vec![],"");
        add(&d,"a",now-5000,true,true,vec!["فاصل الدخول المختار"],"");
        d.execute("INSERT INTO sniper_opportunity_samples VALUES('a',?1,0,'broken-json')",[now-4000]).unwrap();
        let r=report(&d,"a",now).unwrap();
        assert_eq!(r["sampled_decisions"],1);
        assert_eq!(r["malformed_or_inconsistent_rows"],3);
        assert_eq!(r["entry_eligible_at_snapshot"],1);
        assert_eq!(report(&d,"b",now).unwrap()["sampled_decisions"],1);
        assert_eq!(report(&d,"",now).unwrap()["status"],"unbound");
    }
    #[test]
    fn unknown_reason_and_gate_are_explicitly_unknown(){
        let d=db();let now=9_000;
        add(&d,"a",now-1000,false,false,vec![],"new reason");
        add(&d,"a",now-2000,true,false,vec!["new rule"],"");
        let r=report(&d,"a",now).unwrap();
        let c=r["top_causes"].as_array().unwrap();
        assert!(c.iter().any(|x|x["code"]=="other_unready_reason"));
        assert!(c.iter().any(|x|x["code"]=="other_execution_gate"));
        assert_eq!(r["status"],"insufficient_sample");
    }
}
