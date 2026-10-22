//! Separate read-only risk dashboard. Polling cannot modify settings,
//! unlock the broker gate or interfere with independent Paper/Shadow learning.
use super::*;
fn f(v:&Value)->String {
    v.as_f64().filter(|n|n.is_finite()).map(|n|format!("{n:.2}")).unwrap_or_else(||"—".into())
}
fn paint(ui:&Dashboard,v:&Value) {
    let source=if v["state"]=="unavailable" {"غير متوفرة"}
        else if v["state"]=="disabled" {"مطفأ"} else if v["state"]=="locked" {"مقفل"}
        else if v["state"]=="breached" {"الحد متجاوز"}else {"مفعّل؛ دون الحد"};
    let mode=match v["account"]["mode"].as_i64(){Some(0)=>"DEMO",Some(2)=>"REAL",_=>"غير معروف"};
    let details=if v["account"].is_null() {
        "الحساب غير متصل؛ الأرقام غير متاحة وليست صفراً.".to_string()
    }else{
        format!("حساب {} / {} ({}) | يوم بغداد {}\nالرصيد {} | Equity {} | صافي تداول اليوم {}\nبداية اليوم المقدرة {} | خسارة Equity المقدرة {} ({}%)\nحد اليوم {}% | ميزانية خسارة اليوم المقدرة {} | القفل محفوظ: {}",
            v["account"]["login"],v["account"]["server"].as_str().unwrap_or("—"),mode,
            v["day"].as_str().unwrap_or("—"),f(&v["balance"]),f(&v["equity"]),f(&v["today_net"]),
            f(&v["estimated_day_start_balance"]),f(&v["estimated_day_loss"]),f(&v["estimated_day_loss_percent"]),
            f(&v["limit_percent"]),f(&v["allowed_day_loss"]),
            if v["locked"]==true {"نعم"} else {"لا"})
    };
    ui.set_risk_monitor_text(format!(
        "حالة القاطع: {}\n{}\n{}\nDemo: {} | REAL: {}\n\n{}\n\n{}",
        source,v["reason"].as_str().unwrap_or("بانتظار بيانات MT5"),
        details,
        if v["demo_enabled"]==true {"شغّال"} else {"مطفأ"},
        if v["real_enabled"]==true {"شغّال"} else {"مطفأ"},
        "القاطع يوقف دخول MT5 الجديد فقط، ولا يغلق مراكز قائمة ولا يعطّل تدريب SHADOW/PAPER. سجلات الإشارات والصفقات الافتراضية والتعبئات الحقيقية منفصلة في «حالة العقول والتحكم».",
        "هذه أرقام تقديرية من Equity وعمليات تداول اليوم لجميع الرموز، وليست حداً مضموناً للخسارة بسبب الفجوات أو الإيداع/السحب أو صفقات مرحّلة من أمس."
    ).into());
}
pub fn setup(ui:&Dashboard)->slint::Timer{
    let (tx,rx)=mpsc::channel::<()>();
    let weak=ui.as_weak();let sender=tx.clone();
    ui.on_risk_monitor_refresh(move ||{
        if let Some(ui)=weak.upgrade(){
            if !ui.get_risk_monitor_busy(){
                ui.set_risk_monitor_busy(true);
                let _=sender.send(());
            }
        }
    });
    let timer=slint::Timer::default();
    let weak=ui.as_weak();
    timer.start(slint::TimerMode::Repeated,Duration::from_secs(3),move ||{
        if let Some(ui)=weak.upgrade(){
            if ui.get_page()==6 && !ui.get_risk_monitor_busy(){
                ui.set_risk_monitor_busy(true);
                let _=tx.send(());
            }
        }
    });
    let weak=ui.as_weak();
    std::thread::spawn(move ||while rx.recv().is_ok(){
        let r=Client::new(&data_dir()).and_then(|client|client.get("/v1/risk/monitor"));
        let _=weak.upgrade_in_event_loop(move |ui|{
            ui.set_risk_monitor_busy(false);
            match r {
                Ok(v)=>paint(&ui,&v),
                Err(e)=>ui.set_risk_monitor_text(format!("مراقبة المخاطرة غير متاحة: {e}. لا نفترض أن القاطع يعمل دون اتصال حديث.").into())
            }
        });
    });
    timer
}
