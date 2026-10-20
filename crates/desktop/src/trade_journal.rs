//! Read-only native UI for durable MT5 broker fills (never shadow estimates).
use braintrade_runtime::{data_dir, Client};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::{sync::{mpsc,Arc,Mutex,atomic::{AtomicU32,Ordering}}, time::Duration};
use super::Dashboard;

fn value(v:&Value,key:&str)->f64 {v[key].as_f64().unwrap_or(0.)}
fn name(v:&Value,key:&str)->&str {v[key].as_str().unwrap_or("—")}
fn amount(v:&Value,key:&str)->String {
    v[key].as_f64().map(|x|format!("{x:.2}")).unwrap_or_else(||"غير مؤكّد".into())
}
fn row_label(v:&Value)->String {
    format!("#{} | {} | {} | {} | {} | صافي محقق {}",
        v["position_id"],name(v,"status"),name(&v["origin"],"trading_mode"),
        name(&v["origin"],"opening_brain"),name(v,"symbol"),amount(v,"broker_net_booked"))
}
fn detail(v:&Value)->String {
    let o=&v["origin"];
    format!(
        "رقم مركز MT5: {}    |    الحالة: {}\n\
        الرمز: {}    |    الاتجاه: {}    |    التاريخ: {}\n\
        العقل المنفّذ: {}    |    صاحب الاستراتيجية: {}\n\
        وضع الدخول الأصلي: {}    |    استراتيجية القناص: {}\n\
        سبب الإشارة المثبّت: {}\n\
        سعر الدخول: {}    |    لوت مفتوح: {:.3}    |    لوت مغلق: {:.3}    |    المتبقي: {}\n\
        مسافة TP المطلوبة من سعر الدخول: {}    |    مسافة SL: {}\n\
        الربح السعري المحقق: {}    |    العمولة: {}    |    السواب: {}    |    الرسوم: {}\n\
        الصافي المحقق من صفقات الوسيط: {}    |    الربح العائم الحالي: {}\n\
        عدد سجلات الـ Deals: {}    |    آخر سجل: {}\n\
        معرّف أمر فتح الصفقة: {}\n\
        النتائج المحققة مأخوذة من تاريخ الوسيط، وليست حساباً افتراضياً. الربح العائم منفصل، ولا تُخَمَّن عمولات لا تخص position_id.",
        v["position_id"],name(v,"status"),name(v,"symbol"),name(v,"side"),name(v,"opened_utc"),
        name(o,"owner"),name(o,"opening_brain"),name(o,"trading_mode"),name(o,"policy"),name(o,"reason"),
        if v["entry_price"].is_number(){format!("{:.5}",value(v,"entry_price"))}else{"غير متاح".into()},
        value(v,"lots_opened"),value(v,"lots_closed"),amount(v,"remaining_lots"),
        amount(o,"target_distance"),amount(o,"stop_distance"),
        amount(v,"broker_gross"),amount(v,"commission"),amount(v,"swap"),amount(v,"fee"),
        amount(v,"broker_net_booked"),amount(v,"floating_broker_pnl"),
        v["deals"],name(v,"latest_deal_utc"),name(o,"command_id")
    )
}
pub fn setup(ui:&Dashboard) -> slint::Timer {
    let cache=Arc::new(Mutex::new(Vec::<Value>::new()));
    let offset=Arc::new(AtomicU32::new(0u32));
    let (tx,rx)=mpsc::channel::<u32>();
    let weak=ui.as_weak();
    let cache_ui=cache.clone();let offset_ui=offset.clone();
    let sender=tx.clone();
    ui.on_trade_journal_action(move |action| {
        let Some(ui)=weak.upgrade() else {return};
        if ui.get_trade_journal_busy() {return}
        let action=action.as_str();
        if action=="select" {
            let index=ui.get_trade_journal_selected();
            let rows=cache_ui.lock().unwrap();
            ui.set_trade_journal_detail(rows.get(index.max(0) as usize).map(detail)
                .unwrap_or_else(||"اختر صفقة من القائمة".into()).into());
            return;
        }
        match action {
            "next"=>{offset_ui.fetch_add(50,Ordering::Relaxed);},
            "prev"=>{offset_ui.fetch_update(Ordering::Relaxed,Ordering::Relaxed,|v|Some(v.saturating_sub(50))).ok();},
            "refresh"=>{offset_ui.store(0,Ordering::Relaxed);},
            _=>return,
        }
        ui.set_trade_journal_busy(true);
        let _=sender.send(offset_ui.load(Ordering::Relaxed));
    });
    let weak=ui.as_weak();
    let auto=tx.clone();let auto_offset=offset.clone();
    let timer=slint::Timer::default();
    timer.start(slint::TimerMode::Repeated,Duration::from_secs(5),move ||{
        if let Some(ui)=weak.upgrade(){
            if ui.get_page()==20 && !ui.get_trade_journal_busy(){
                ui.set_trade_journal_busy(true);
                let _=auto.send(auto_offset.load(Ordering::Relaxed));
            }
        }
    });
    let weak=ui.as_weak();
    std::thread::spawn(move || {
        while let Ok(page_offset)=rx.recv(){
            let result=(||->anyhow::Result<Value>{
                Client::new(&data_dir())?.post("/v1/execution/trades",
                    json!({"offset":page_offset,"limit":50}))
            })();
            let rows_cache=cache.clone();let tracked=offset.clone();
            let _=weak.upgrade_in_event_loop(move |ui|{
                ui.set_trade_journal_busy(false);
                match result {
                    Ok(v)=>{
                        let rows=v["rows"].as_array().cloned().unwrap_or_default();
                        let total=v["total"].as_u64().unwrap_or(0);
                        let labels=rows.iter().map(|v|slint::SharedString::from(row_label(v))).collect::<Vec<_>>();
                        *rows_cache.lock().unwrap()=rows;
                        ui.set_trade_journal_rows(slint::ModelRc::new(slint::VecModel::from(labels)));
                        let selected=if rows_cache.lock().unwrap().is_empty(){-1}else{0};
                        ui.set_trade_journal_selected(selected);
                        ui.set_trade_journal_detail(rows_cache.lock().unwrap().first().map(detail)
                            .unwrap_or_else(||"لا توجد صفقات مسجلة لهذا الحساب والرمز بعد".into()).into());
                        ui.set_trade_journal_has_prev(page_offset>0);
                        ui.set_trade_journal_has_next((page_offset as u64 + 50)<total);
                        ui.set_trade_journal_summary(format!(
                            "إجمالي مراكز MT5 المحفوظة: {} | الصفحة: {} | المعروض: {} صفقة",
                            total,page_offset/50+1,rows_cache.lock().unwrap().len()
                        ).into());
                        ui.set_trade_journal_notice(name(&v,"notice").into());
                        tracked.store(page_offset,Ordering::Relaxed);
                    },
                    Err(error)=>{
                        ui.set_trade_journal_detail(format!("تعذر تحميل السجل: {error}").into());
                    }
                }
            });
        }
    });
    timer
}
