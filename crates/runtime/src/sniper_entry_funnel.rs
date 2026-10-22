//! Sparse, account-scoped Sniper entry-funnel *observation*, sampled by the
//! existing 1 Hz advisor activity cadence. Never a trade opportunity label:
//! no counterfactual PnL, no new entries, no mutation of risk/learning state.
use anyhow::Result;
use rusqlite::{params,Connection};
use serde_json::{json,Value};

const HOUR_MS:u64=3_600_000;
const WINDOW_MS:u64=24*HOUR_MS;
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch("CREATE TABLE IF NOT EXISTS sniper_entry_funnel_hourly (
        context TEXT NOT NULL,
        hour_ms INTEGER NOT NULL,
        dimension TEXT NOT NULL,
        code TEXT NOT NULL,
        samples INTEGER NOT NULL,
        last_ms INTEGER NOT NULL,
        PRIMARY KEY(context,hour_ms,dimension,code)
    );")?;
    Ok(())
}
fn reason_code(reason:&str)->&'static str{
    if reason.contains("ينتظر سعراً حديثاً") {"stale_or_missing_quote"}
    else if reason.contains("بيانات السعر أو العقد غير مكتملة"){"invalid_quote_or_contract"}
    else if reason.contains("يجمع ثلاثة أسعار"){"quote_warmup"}
    else if reason.contains("الشروط غير مكتملة"){"setup_conditions_not_met"}
    else if reason.contains("لا هدف ضمن")||reason.contains("لا يجتاز حصة التكلفة"){
        "target_cost_gate"
    }else if reason.contains("مشاركة القناص موقوفة"){"specialist_disabled_or_occupied"}
    else{"other_not_ready"}
}
fn gate_code(gate:&str)->&'static str{
    match gate {
        "النظام متوقف"=>"engine_stopped",
        "قاطع الخسارة اليومي للحساب: الحد بلغ أو بيانات اليوم غير موثوقة"=>"daily_breaker",
        "المخاطرة فوق 2% تجريبية على حساب MT5 Demo فقط؛ REAL يبقى بحد 2%"=>"demo_only_risk",
        "أمر قيد التنفيذ أو المصالحة"=>"existing_unsettled_order",
        "للعقل صفقة مفتوحة"=>"existing_position",
        "التداول الآلي غير مسموح في MT5"=>"terminal_trade_disabled",
        "حساب Netting: الرمز مملوك لصفقة حالية"=>"netting_position",
        "فاصل الدخول المختار"=>"cooldown",
        "حد التراجع الاختياري من أعلى Equity مرصود لهذا الحساب والرمز"=>"drawdown_limit",
        "حد اللوت الاختياري"=>"lot_limit",
        "ميزانية المخاطرة الاختيارية"=>"risk_budget",
        "حد السبريد الاختياري: سبريد MT5 تجاوز نسبته من ATR الحقيقي"=>"spread_atr_limit",
        "حد السبريد الاختياري يحتاج شموع M1 لحساب ATR الحقيقي"=>"atr_unavailable",
        "تمت معالجة هذه الإشارة"=>"duplicate_signal",
        "SHADOW / PAPER فقط: الوضع المختار يمنع هذا العقل من إرسال صفقات إلى MT5"=>"paper_only",
        "Netting: أمر لعقل آخر لم تتم تسويته"=>"netting_unsettled",
        _=>"unclassified_execution_gate",
    }
}
fn label(code:&str)->&'static str{
    match code {
        "strategy_not_ready"=>"لم تكتمل إشارة القناص",
        "ready_but_blocked"=>"إشارة جاهزة لكن منعتها بوابات التنفيذ",
        "eligible_for_queue"=>"إشارة اجتازت بوابات الدخول (ليس تأكيد تنفيذ)",
        "stale_or_missing_quote"=>"السعر قديم أو مفقود",
        "invalid_quote_or_contract"=>"بيانات العقد أو السعر غير كافية",
        "quote_warmup"=>"بانتظار تجميع أسعار كافية",
        "setup_conditions_not_met"=>"شروط حركة القناص غير مكتملة",
        "target_cost_gate"=>"الهدف لا يغطي نسبة التكلفة المطلوبة",
        "specialist_disabled_or_occupied"=>"المشاركة معطلة أو توجد صفقة معلقة",
        "other_not_ready"=>"سبب عدم جاهزية غير مصنف",
        "engine_stopped"=>"النظام متوقف",
        "daily_breaker"=>"قاطع الخسارة اليومي",
        "demo_only_risk"=>"حد مخاطرة مسموح للتجريبي فقط",
        "existing_unsettled_order"=>"أمر آخر لم تتم تسويته",
        "existing_position"=>"صفقة مفتوحة",
        "terminal_trade_disabled"=>"التداول غير مسموح في MT5",
        "netting_position"=>"رمز مشغول بحساب Netting",
        "cooldown"=>"فاصل الدخول",
        "drawdown_limit"=>"حد التراجع",
        "lot_limit"=>"حد اللوت",
        "risk_budget"=>"ميزانية المخاطرة",
        "spread_atr_limit"=>"السبريد أعلى من حد ATR",
        "atr_unavailable"=>"بيانات ATR غير متوفرة",
        "duplicate_signal"=>"الإشارة سبق التعامل معها",
        "paper_only"=>"عقل استشاري ورقي",
        "netting_unsettled"=>"أمر Netting لم تتم تسويته",
        "unclassified_execution_gate"=>"سبب منع غير مصنف",
        _=>"تصنيف غير معروف",
    }
}
fn increment(db:&Connection,context:&str,hour:u64,dimension:&str,code:&str,now:u64)->Result<()>{
    db.execute(
        "INSERT INTO sniper_entry_funnel_hourly(context,hour_ms,dimension,code,samples,last_ms)
         VALUES(?1,?2,?3,?4,1,?5)
         ON CONFLICT(context,hour_ms,dimension,code)
         DO UPDATE SET samples=samples+1,last_ms=excluded.last_ms",
         params![context,hour,dimension,code,now],
    )?;
    Ok(())
}
/// Record only the selected-mode SYSTEM's full decision, with evaluated gates.
/// The caller invokes once per paper/activity sample (at most once per second).
/// Every decision adds exactly one outcome; blocked decisions may add many
/// distinct gate causes, never miscounted as separate trading opportunities.
pub fn record(db:&Connection,context:&str,decision:&Value,now:u64)->Result<()>{
    if context.is_empty()||now==0{return Ok(());}
    let Some(signal)=decision.get("signal").filter(|s|s.is_object()) else{return Ok(());};
    if signal["brain"]!="system" || decision["evaluated_ms"].as_u64()!=Some(now) {
        return Ok(());
    }
    let gates=decision["gates"].as_array();
    let ready=signal["ready"]==true;
    let outcome=if !ready{"strategy_not_ready"}
        else if gates.is_some_and(|items|!items.is_empty()){"ready_but_blocked"}
        else if decision["ready"]==true{"eligible_for_queue"}
        else{"ready_but_blocked"};
    let hour=now/HOUR_MS*HOUR_MS;
    increment(db,context,hour,"outcome",outcome,now)?;
    if !ready{
        let reason=signal["reason"].as_str().unwrap_or("");
        increment(db,context,hour,"strategy_reason",reason_code(reason),now)?;
    }else if outcome=="ready_but_blocked" {
        // A decision can have multiple gates; count each named gate once
        // for the same decision. Gate totals need not equal blocked samples.
        let mut unique=std::collections::BTreeSet::new();
        for value in gates.into_iter().flatten(){
            unique.insert(gate_code(value.as_str().unwrap_or("")));
        }
        if unique.is_empty(){unique.insert("unclassified_execution_gate");}
        for g in unique{increment(db,context,hour,"execution_gate",g,now)?;}
    }
    Ok(())
}
pub fn report(db:&Connection,context:&str,now:u64)->Result<Value>{
    if context.is_empty()||now==0{
        return Ok(json!({"status":"unbound","sampled_decisions":0,"no_execution_effect":true}));
    }
    let oldest=now.saturating_sub(WINDOW_MS)/HOUR_MS*HOUR_MS;
    let current=now/HOUR_MS*HOUR_MS;
    let mut stmt=db.prepare(
        "SELECT dimension,code,SUM(samples)
         FROM sniper_entry_funnel_hourly
         WHERE context=?1 AND hour_ms>=?2 AND hour_ms<=?3
         GROUP BY dimension,code ORDER BY dimension,code")?;
    let raw=stmt.query_map(params![context,oldest,current],
        |row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,u64>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut outcomes=serde_json::Map::new();
    let mut causes=Vec::<Value>::new();
    let mut total=0u64;
    for (dimension,code,n) in raw {
        if dimension=="outcome"{
            total=total.saturating_add(n);
            outcomes.insert(code,json!(n));
        }else if dimension=="strategy_reason" || dimension=="execution_gate" {
            causes.push(json!({"dimension":dimension,"code":code,"label_ar":label(&code),"samples":n}));
        }
    }
    causes.sort_by(|a,b|b["samples"].as_u64().cmp(&a["samples"].as_u64())
        .then_with(||a["code"].as_str().cmp(&b["code"].as_str())));
    Ok(json!({
        "policy":"sniper_passive_entry_funnel_v1",
        "status":if total>=30{"sampled_monitor_only"}else{"insufficient_sampled_decisions"},
        "context_scoped":true,
        "window_start_hour_ms":oldest,
        "window_end_hour_ms":current,
        "approximate_window_hours":25,
        "sampling":"SYSTEM Sniper decisions at existing 1 Hz activity cadence; NOT each Tick",
        "sampled_decisions":total,
        "outcomes":outcomes,
        "top_causes":causes.into_iter().take(16).collect::<Vec<_>>(),
        "multiple_gates_per_blocked_signal":true,
        "sent_orders_verified":false,
        "counterfactual_winner_verified":false,
        "missed_profit_estimate":Value::Null,
        "no_execution_effect":true,
        "no_learning_effect":true,
        "caveat":"Non-ready does not imply a missed profitable trade. Ready-unblocked is only eligible for queue, NOT confirmed broker execution. Multi-cause gate counts overlap; never divide gate totals by outcome count to infer unique opportunities. Approximate 24h by whole hourly bins."
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn d()->Connection{let db=Connection::open_in_memory().unwrap();init(&db).unwrap();db}
    fn decision(ready:bool,gates:&[&str],reason:&str,now:u64)->Value {
        json!({"signal":{"brain":"system","ready":ready,"reason":reason},
            "ready":ready&&gates.is_empty(),"gates":gates,"evaluated_ms":now})
    }
    #[test]
    fn ready_blocked_and_nonready_are_separate_and_gates_are_multicausal(){
        let db=d();let now=17*HOUR_MS;
        record(&db,"a",&decision(false,&[],"القناص: ينتظر سعراً حديثاً",now),now).unwrap();
        record(&db,"a",&decision(true,&["فاصل الدخول المختار","ميزانية المخاطرة الاختيارية",
            "فاصل الدخول المختار"],"",now),now).unwrap();
        record(&db,"a",&decision(true,&[],"",now),now).unwrap();
        let r=report(&db,"a",now).unwrap();
        assert_eq!(r["sampled_decisions"],3);
        assert_eq!(r["outcomes"]["strategy_not_ready"],1);
        assert_eq!(r["outcomes"]["ready_but_blocked"],1);
        assert_eq!(r["outcomes"]["eligible_for_queue"],1);
        let causes=r["top_causes"].as_array().unwrap();
        assert_eq!(causes.iter().find(|x|x["code"]=="cooldown").unwrap()["samples"],1);
        assert_eq!(causes.iter().find(|x|x["code"]=="risk_budget").unwrap()["samples"],1);
        assert_eq!(causes.iter().find(|x|x["code"]=="stale_or_missing_quote").unwrap()["samples"],1);
        assert_eq!(r["missed_profit_estimate"],Value::Null);
        assert_eq!(r["sent_orders_verified"],false);
    }
    #[test]
    fn account_and_hour_isolation_and_non_system_inputs(){
        let db=d();let now=36*HOUR_MS;
        record(&db,"a",&decision(true,&[],"",now-26*HOUR_MS),now-26*HOUR_MS).unwrap();
        record(&db,"a",&decision(true,&[],"",now),now).unwrap();
        record(&db,"b",&decision(true,&[],"",now),now).unwrap();
        let mut non_system=decision(true,&[],"",now);
        non_system["signal"]["brain"]=json!("sniper_learner");
        record(&db,"a",&non_system,now).unwrap();
        assert_eq!(report(&db,"a",now).unwrap()["sampled_decisions"],1);
        assert_eq!(report(&db,"b",now).unwrap()["sampled_decisions"],1);
        assert_eq!(report(&db,"",now).unwrap()["status"],"unbound");
    }
    #[test]
    fn unknown_gates_and_reasons_are_not_silently_mislabeled(){
        let db=d();let now=HOUR_MS;
        record(&db,"a",&decision(false,&[],"new unknown reason",now),now).unwrap();
        record(&db,"a",&decision(true,&["new unknown execution rule"],"",now),now).unwrap();
        let r=report(&db,"a",now).unwrap();
        let causes=r["top_causes"].as_array().unwrap();
        assert!(causes.iter().any(|x|x["code"]=="other_not_ready"));
        assert!(causes.iter().any(|x|x["code"]=="unclassified_execution_gate"));
        assert_eq!(r["status"],"insufficient_sampled_decisions");
    }
}
