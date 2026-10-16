//! Explain current eligibility and broker evidence without calling MT5.
use braintrade_domain::{Config,Status};
use serde_json::{json,Value};
pub fn report(v:&Value,s:&Status,c:&Config,now:u64)->Value {
    let commands:Vec<&Value>=v["commands"].as_array().into_iter().flatten().collect();
    let acknowledged=|kind:&str|commands.iter().filter(|q|q["request"]["kind"]==kind&&matches!(q["result"]["state"].as_str(),Some("filled"|"partial"))).count();
    let latest=commands.first().copied();
    let system=v["brains"].as_array().and_then(|bs|bs.iter().find(|b|b["config"]["id"]=="system"));
    let t=s.telemetry.as_ref().filter(|t|s.gateway_fresh&&t.connected);
    let phase=if t.is_none(){"بانتظار اتصال MT5 حديث"}else if !s.requested_run{"متوقف — اضغط تشغيل الديمو"}else if v["account_matches"]!=true{"الحساب الحالي لا يطابق حساب التنفيذ"}else if t.is_some_and(|t|!t.trade_allowed||!t.trade_expert){"فعّل Algo Trading والسماح بتداول Python في MT5"}else if system.is_some_and(|b|b["latest"]["ready"]==true){"إشارة جاهزة؛ راقب حالة الأمر ورد MT5"}else{"التنفيذ مفعّل؛ سبب انتظار العقل ظاهر أدناه"};
    let costs=t.and_then(|t|braintrade_domain::trading::costs(t,&c.for_account(t),system.and_then(|b|b["config"]["lots"].as_f64()).unwrap_or(0.01),now).ok());
    let cost_text=costs.as_ref().map(|p|format!("\nتكلفة {:.3} لوت: سبريد {:.4} + عمولة {:.4} + احتياط انزلاق {:.4} {}",p.lots,p.spread_money,p.commission,p.slippage_money,p.currency)).unwrap_or_default();
    let last=latest.map(|q|format!("\nآخر أمر: {} / {} — {}",q["request"]["kind"].as_str().unwrap_or(""),q["state"].as_str().unwrap_or(""),q["result"]["reason"].as_str().unwrap_or("بانتظار الإرسال"))).unwrap_or_else(||"\nلم يُسجّل أمر لهذا الحساب والرمز بعد.".into());
    json!({"phase":phase,"open_acknowledgements":acknowledged("open"),"close_acknowledgements":acknowledged("close"),"window":"latest 100 account/symbol commands","costs":costs,"text":format!("{phase}\nضمن آخر 100 أمر: رد تنفيذ فتح {} / إغلاق {}{last}{cost_text}",acknowledged("open"),acknowledged("close"))})
}
