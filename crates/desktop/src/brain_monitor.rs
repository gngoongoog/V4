//! Brain control room: broker ownership, attributed orders and paper evidence stay distinct.
use super::*;
use braintrade_domain::execution::BRAINS;
use std::{thread, sync::{Arc,Mutex}};
fn amount(v:&Value)->f64 {v.as_f64().unwrap_or(0.)}
fn yes(v:&Value)->&'static str {if *v==true {"نعم"}else{"لا"}}
fn mode(v:&Value)->&'static str {match v.as_str().unwrap_or("") {"sniper"=>"القناص","scalping"=>"سكالبينغ","swing"=>"سوينغ",_=>"الخبير"}}
fn paint(ui:&Dashboard,v:&Value,index:usize,load:bool) {
    let rows=v["brains"].as_array();
    let mut overview=String::new();
    for b in rows.into_iter().flatten() {
        overview.push_str(&format!("{} | إشارات {} | دخول منسوب {} / تعبئة مؤكدة {} | تدريب مغلق {} / رابح {}\n",
            b["name"].as_str().unwrap_or("—"),b["activity"]["signals"],
            b["attributed_orders"]["requested"],b["attributed_orders"]["fill_acknowledged"],
            b["paper_stats"]["closed"],b["paper_stats"]["wins"]));
    }
    ui.set_brain_monitor_summary(format!("الوضع: {} | الحساب {} / {} | التنفيذ: {}\n{}",
        mode(&v["trading_mode"]),v["settings"]["login"],v["settings"]["server"].as_str().unwrap_or("—"),yes(&v["armed"]),
        v["activity_note"].as_str().unwrap_or("")).into());
    ui.set_brain_monitor_overview(overview.into());
    ui.set_brain_monitor_available(v["controls_available"]==true);
    let Some(b)=rows.and_then(|r|r.get(index)) else {return};
    if load {
        ui.set_brain_advisor(b["control"]["advisor"]==true);
        ui.set_brain_paper(b["control"]["paper"]==true);
    }
    let s=&b["latest"];let paper=&b["paper_stats"];let stats=&b["stats"];
    let role=if index==0 {"المنفّذ الوحيد — يتبع بدء/إيقاف النظام"}
        else if b["return_pending"]==true {"يتابع صفقة قديمة؛ لا يستقبل دخولاً منفرداً جديداً"}
        else if b["participates_in_coordinator"]==true {"مستشار مؤهل لقرار الخبير"}
        else {"مراقبة/تدريب؛ لا يرسل أوامر مستقلة"};
    let gates=s["gates"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(" • ");
    let mut detail=format!("{}\n{}\nآخر إشارة: {} | جاهزية الإرسال: {}\n{}\nسبب الانتظار: {}\nزمن آخر حساب: {} ميكروثانية؛ ليس زمن تنفيذ الوسيط\n\nالوضع الحالي — رصد ورقي محاكى: مغلقة {} / رابحة {} / صافي افتراضي {:.2}\nصفقات ورقية مفتوحة من كل الأوضاع: {}؛ هذه الإحصاءات لا تغيّر أوزان التنفيذ الحقيقي\n\nسجل ملكية MT5 عبر كل الأوضاع — مغلقة {} / رابحة {} / مفتوحة {}\nصافي مسجل {:.2} | عائم {:.2}\nالدخول المنسوب للاستراتيجية في الأعلى مختلف عن ملكية الصفقة: المنفّذ الحالي هو النظام.",
        b["name"].as_str().unwrap_or("—"),role,
        match s["signal"]["side"].as_i64(){Some(1)=>"شراء",Some(-1)=>"بيع",_=>"انتظار"},yes(&s["ready"]),
        s["signal"]["reason"].as_str().unwrap_or("لا توجد إشارة حديثة؛ انتظر الاتصال"),gates,
        s["decision_us"],paper["closed"],paper["wins"],amount(&paper["net"]),paper["open_all_modes"],
        stats["closed_positions"],stats["wins"],stats["open_positions"],amount(&stats["net_booked"]),amount(&stats["floating"]));
    let learned=&b["learning"];
    detail.push_str(&format!(
        "\n\nالدليل الذي يستخدمه العقل فعلياً: عينات MT5 Demo مغلقة {} | نتائج رصد سببي لاحق {} | مصدر الترجيح {} | الوزن المعروض {}.\nنتائج Paper لا تُضاف تلقائياً لأوزان MT5.",
        learned["broker_samples"],learned["forward_samples"],
        learned["evidence_source"].as_str().unwrap_or("لا توجد أدلة مؤكدة"),
        b["learned_weight"]
    ));
    ui.set_brain_monitor_detail(detail.into());
}
pub fn setup(ui:&Dashboard)->slint::Timer {
    ui.set_brain_monitor_names(slint::ModelRc::new(slint::VecModel::from(BRAINS.iter().map(|(_,n)|slint::SharedString::from(*n)).collect::<Vec<_>>())));
    let cache=Arc::new(Mutex::new(json!({})));
    let (tx,rx)=mpsc::channel::<(String,Value,usize)>();
    let sender=tx.clone();let weak=ui.as_weak();let current=cache.clone();
    ui.on_brain_monitor_action(move |a| {
        let Some(ui)=weak.upgrade() else{return};if ui.get_brain_monitor_busy(){return}
        let index=ui.get_brain_monitor_selected().max(0) as usize;
        let v=current.lock().unwrap();
        if a=="select" {paint(&ui,&v,index,true);return}
        let body=json!({"context":v["context"],"brain":BRAINS.get(index).map(|b|b.0).unwrap_or("system"),"mode":v["trading_mode"],
            "control":{"advisor":ui.get_brain_advisor(),"paper":ui.get_brain_paper()}});
        ui.set_brain_monitor_busy(true);
        let _=sender.send((a.to_string(),body,index));
    });
    let timer=slint::Timer::default();let weak=ui.as_weak();
    timer.start(slint::TimerMode::Repeated,Duration::from_secs(3),move ||{
        if let Some(ui)=weak.upgrade(){if ui.get_page()==21 && !ui.get_brain_monitor_busy(){
            ui.set_brain_monitor_busy(true);let _=tx.send(("poll".into(),json!({}),ui.get_brain_monitor_selected().max(0) as usize));
        }}
    });
    let weak=ui.as_weak();
    thread::spawn(move ||while let Ok((action,body,index))=rx.recv(){
        let result=(||->anyhow::Result<Value>{let c=Client::new(&data_dir())?;
            if action=="save"{c.post("/v1/execution/brain-control",body)?;}
            c.get("/v1/execution/brains")})();
        let cache=cache.clone();
        let _=weak.upgrade_in_event_loop(move |ui|{
            ui.set_brain_monitor_busy(false);
            match result {Ok(v)=>{
                let changed={let previous=cache.lock().unwrap();previous["trading_mode"]!=v["trading_mode"] || previous["context"]!=v["context"]};
                paint(&ui,&v,index,action!="poll"||changed);*cache.lock().unwrap()=v;
                if action=="save"{ui.set_brain_monitor_message("حُفظت المشاركة والتدريب لهذا الحساب والرمز والوضع فقط".into());}
            },Err(e)=>ui.set_brain_monitor_message(format!("تعذر التحديث: {e}").into())}
        });
    });timer
}
