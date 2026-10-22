//! Brain control room: broker ownership, attributed orders and paper evidence stay distinct.
use super::*;
use braintrade_domain::execution::BRAINS;
use std::{thread, sync::{Arc,Mutex}};
fn amount(v:&Value)->f64 {v.as_f64().unwrap_or(0.)}
fn maybe_number(v:&Value)->String {v.as_f64().filter(|n|n.is_finite()).map(|n|format!("{n:.2}")).unwrap_or_else(||"—".into())}
fn yes(v:&Value)->&'static str {if *v==true {"نعم"}else{"لا"}}
fn mode(v:&Value)->&'static str {match v.as_str().unwrap_or("") {"sniper"=>"القناص","scalping"=>"سكالبينغ","swing"=>"سوينغ",_=>"الخبير"}}
fn paint(ui:&Dashboard,v:&Value,index:usize,load:bool) {
    let rows=v["brains"].as_array();
    let mut overview=String::new();
    for b in rows.into_iter().flatten() {
        overview.push_str(&format!("{} | إشارات {} | دخول منسوب {} / تعبئة مؤكدة {} | تدريب مغلق {} / رابح {} | تقييم Demo مغلق {} ({}؛ لا يرقي تلقائياً)\n",
            b["name"].as_str().unwrap_or("—"),b["activity"]["signals"],
            b["attributed_orders"]["requested"],b["attributed_orders"]["fill_acknowledged"],
            b["paper_stats"]["closed"],b["paper_stats"]["wins"],
            b["quality"]["broker_samples"],b["quality"]["status"].as_str().unwrap_or("unknown")));
    }
    ui.set_brain_monitor_summary(format!("الوضع: {} | الحساب {} / {} | التنفيذ: {}\n{}",
        mode(&v["trading_mode"]),v["settings"]["login"],v["settings"]["server"].as_str().unwrap_or("—"),yes(&v["armed"]),
        v["activity_note"].as_str().unwrap_or("")).into());
    ui.set_brain_monitor_overview(overview.into());
    ui.set_brain_monitor_available(v["controls_available"]==true);
    ui.set_frozen_trial_available(v["controls_available"]==true
        && v["settings"]["target"]=="demo" && v["trading_mode"]=="expert");
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
    let q=&b["quality"];
    let description=match q["status"].as_str().unwrap_or("") {
        "insufficient_evidence"=>"أدلة Broker غير كافية للتقييم الزمني",
        "mixed_policy_monitor_only"=>"المصدر/الفريم تغيّر: لا تخلط النسخ لتقرر التحسن",
        "recent_deterioration"=>"تحذير: متوسط صافي الصفقات الأحدث غير موجب",
        "monitor_only"=>"بيانات كافية للمراقبة فقط؛ ليست دليلاً على نجاح خارج العينة",
        _=>"تقييم غير متاح",
    };
    detail.push_str(&format!(
        "\n\nتدقيق جودة التعلّم — صفقات MT5 Demo مغلقة فقط، وليس Paper/Shadow:\n{}\nعينات صالحة {} | مستبعدة {} | تكرارات متقاطعة {} | مصادر استراتيجية {}\nالجزء الأقدم: {} صفقة | متوسط صافي/لوت {} | معدل ربح بعد التكلفة {}\nآخر {} صفقات: {} صفقة | متوسط صافي/لوت {} | معدل ربح بعد التكلفة {}\nالإجمالي: صافي محقق {} | عمولات ورسوم وسواب {} | صفقات محا الربحَ فيها أثرُ التكلفة {}\nالجزء الأخير مراقبة زمنية وليس اختبار OOS مستقلاً؛ قد يكون العقل تعلّم من هذه الصفقات بالفعل. لا يوجد تعديل آلي للأوزان أو الموافقة على تنفيذ جديد بهذه الصفحة.",
        description,
        q["broker_samples"],q["invalid_samples"],q["duplicate_position_samples"],q["distinct_strategy_sources"],
        q["earlier"]["samples"],maybe_number(&q["earlier"]["mean_net_per_lot"]),
        q["earlier"]["win_rate"].as_f64().map(|v|format!("{:.1}%",v*100.)).unwrap_or_else(||"—".into()),
        q["recent_window"],q["recent"]["samples"],maybe_number(&q["recent"]["mean_net_per_lot"]),
        q["recent"]["win_rate"].as_f64().map(|v|format!("{:.1}%",v*100.)).unwrap_or_else(||"—".into()),
        maybe_number(&q["all"]["net_booked"]),maybe_number(&q["all"]["cost_drag"]),q["all"]["gross_positive_but_net_nonpositive"]
    ));
    if b["config"]["id"]=="system" {
        let twin=&b["frozen_coordinator"];
        detail.push_str(&format!(
            "\n\nمنسّق Expert المجمّد — رصد سياسة فقط:\nالحالة {} | شموع مستقبلية مقارنة {} | جاهزية مشتركة {}\nاتجاه متفق {} | اتجاه مختلف {} | مجمّد فقط {} | متعلّم فقط {}\nهذه نسخة ثابتة لإعدادات المنسق وأصوات وأوزان المستشارين وملفات الخروج. لا تعيد تشغيل بوابات الأوامر والهامش والتنفيذ ولا تقيس الربحية.",
            twin["status"].as_str().unwrap_or("غير متاح"),
            twin["bars_compared"],twin["both_actionable"],
            twin["same_direction"],twin["different_direction"],
            twin["frozen_only_ready"],twin["adaptive_only_ready"]
        ));
        let matched=&twin["paired_quote_paper"];
        detail.push_str(&format!(
            "\n\nتجربة Paper مزدوجة للمنسّق — نفس Bid/Ask ولوت 0.01:\nأزواج مكتملة {} | مفتوحة {} | فجوات أسعار {}\nمتوسط صافي المجمّد لكل لوت {} | المتعلّم {} | الفرق بينهما {}\nصفقات رابحة بعد العمولة المفترضة: المجمّد {} | المتعلّم {}\nالعينة تشمل فقط لحظات جاهزية السياستين. الستوب والهدف والوقت يتحققون عند أول سعر مرصود، وليس محاكاة تنفيذ وسيط؛ لا تشمل بوابات المخاطرة أو الهامش أو الانزلاق الحقيقي.",
            matched["completed_pairs"],matched["open_pairs"],
            matched["pairs_with_observation_gaps"],
            maybe_number(&matched["mean_frozen_net_per_lot"]),
            maybe_number(&matched["mean_adaptive_net_per_lot"]),
            maybe_number(&matched["mean_difference_frozen_minus_adaptive_per_lot"]),
            matched["frozen_wins_after_costs"],matched["adaptive_wins_after_costs"]
        ));
        detail.push_str(&format!(
            "\n\nفلترة صلاحية Paper بشكل استرشادي — ليست موافقة MT5:\nأزواج مكتملة توافق شروط اللوت والهامش والميزانية تقريبياً {} | مخالفة {} | بيانات الوسيط ناقصة {}\nالمتوسط الافتراضي لكل لوت ضمن الأزواج التي اجتازت الفحص فقط: المجمّد {} | المتعلم {}\nغياب البيانات يُصنّف مجهولاً، لا يُعتبر موافقة. حساب الهامش والخسارة تقديري وقد تختلف موافقة وسيط MT5 الحقيقية؛ لا تشمل هذه الفلترة جميع بوابات التنفيذ.",
            matched["indicative_contract_pass_pairs"],
            matched["indicative_contract_fail_pairs"],
            matched["indicative_contract_unknown_pairs"],
            maybe_number(&matched["indicative_pass_frozen_mean_net_per_lot"]),
            maybe_number(&matched["indicative_pass_adaptive_mean_net_per_lot"])
        ));
    }
    let frozen=&b["frozen_trial"];
    let stage=match frozen["status"].as_str().unwrap_or("") {
        "active"=>"نشطة — نسخة الإعدادات لا تتعلم وتتداول Paper فقط",
        "stopped"=>"منتهية — السجل محفوظ",
        "not_started"=>"لا توجد تجربة بعد",
        _=>"التجربة غير متاحة دون حساب Demo مربوط"
    };
    detail.push_str(&format!(
        "\n\nنسخة Shadow مجمّدة: {}\nالشموع المستقبلية المقارنة {} | إشارتان جاهزتان {} | اتجاه متوافق {}\nصفقات ورقية منتهية {} | رابحة {} | صافي افتراضي بعد تكاليف مفترضة {}\n{}",
        stage,frozen["paired_bars"],frozen["both_ready"],frozen["side_agreements"],
        frozen["paper_trades_closed"],frozen["paper_wins"],
        maybe_number(&frozen["paper_net_after_assumed_costs"]),
        "هذه نسخة سياسة مستشار ثابتة على Demo فقط، لا تتلقى تحديثاً بعد التجميد ولا تدخل أوامر MT5؛ نتائجها ليست أرباحاً حقيقية ولا اختباراً للقناص أو النظام الرئيسي."
    ));
    let pair=&frozen["matched_five_minute"];
    detail.push_str(&format!(
        "\n\nمقارنة مستقبلية على نفس سعر الدخول والأفق (5 دقائق؛ بدون TP أو SL):\nنتائج مكتملة {} | بانتظار انتهاء الأفق {} | استُبعدت لغياب سعر قريب من الاستحقاق {}\nمتوسط صافي العقل المجمد لكل لوت {} | المتعلّم لكل لوت {} | فرق المجمد ناقص المتعلّم لكل لوت {}\nالنتائج ذات التفوق الافتراضي للمجمد {} | التعادل {} | الحالة {}\nالجهتان تستعملان نفس Bid/Ask واللوت 0.01 وعمولة وانزلاق مفترضين. هذا قياس اتجاه افتراضي، وليس مقارنة أرباح التنفيذ أو إثبات تحسن إحصائي.",
        pair["matched_five_minute_samples"],pair["pending"],pair["expired_no_quote"],
        maybe_number(&pair["frozen_mean_net_per_lot"]),
        maybe_number(&pair["adaptive_mean_net_per_lot"]),
        maybe_number(&pair["mean_delta_frozen_minus_adaptive_per_lot"]),
        pair["frozen_outperforms_pairs"],pair["exact_ties"],
        pair["status"].as_str().unwrap_or("غير متاح")
    ));
    let full=&frozen["matched_full_exits"];
    detail.push_str(&format!(
        "\n\nدراسة مستقبلية للصفقة الكاملة — نفس لحظة الدخول، ستوب/هدف/وقت خاص بكل عقل:\nأزواج صفقات منتهية {} | قيد الرصد {} | نتائج بها فجوات أسعار {}\nصافي مجمّد افتراضي لكل لوت {} | المتعلم {} | الفرق (المجمّد - المتعلّم) {}\nالمجمّد: رابحة {} / ستوبات {} / أهداف {} | المتعلّم: رابحة {} / ستوبات {} / أهداف {}\nتفوق المجمّد في أزواج {} | تعادل {} | جودة الدليل {}\nيُحسب الخروج على أول Bid/Ask مرصود يلامس الستوب أو الهدف أو ينتهي الوقت، مع احتساب عمولة كاملة وانزلاق مفترضين. ليس تنفيذ MT5، ولا يشمل الحركة غير المرصودة، وممنوع استعماله وحده لاعتماد REAL.",
        full["completed_pairs"],full["open_pairs"],full["pairs_with_observation_gaps"],
        maybe_number(&full["mean_frozen_net_per_lot"]),
        maybe_number(&full["mean_adaptive_net_per_lot"]),
        maybe_number(&full["mean_difference_frozen_minus_adaptive_per_lot"]),
        full["frozen_wins_after_costs"],full["frozen_stops"],full["frozen_targets"],
        full["adaptive_wins_after_costs"],full["adaptive_stops"],full["adaptive_targets"],
        full["frozen_outperforms_pairs"],full["exact_ties"],
        full["status"].as_str().unwrap_or("غير متاح")
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
        let body=if a=="freeze_coordinator"||a=="stop_coordinator" {
            json!({"action":if a=="freeze_coordinator"{"freeze"}else{"stop"},
                "context":v["context"],"mode":v["trading_mode"]})
        }else if a=="freeze"||a=="stop_frozen" {
            json!({"action":if a=="freeze"{"freeze"}else{"stop"},
                "context":v["context"],"brain":BRAINS.get(index).map(|b|b.0).unwrap_or("system"),
                "mode":v["trading_mode"]})
        }else{
            json!({"context":v["context"],"brain":BRAINS.get(index).map(|b|b.0).unwrap_or("system"),"mode":v["trading_mode"],
                "control":{"advisor":ui.get_brain_advisor(),"paper":ui.get_brain_paper()}})
        };
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
            else if action=="freeze"||action=="stop_frozen"{c.post("/v1/execution/frozen-shadow",body)?;}
            else if action=="freeze_coordinator"||action=="stop_coordinator"{
                c.post("/v1/execution/frozen-coordinator",body)?;
            }
            c.get("/v1/execution/brains")})();
        let cache=cache.clone();
        let _=weak.upgrade_in_event_loop(move |ui|{
            ui.set_brain_monitor_busy(false);
            match result {Ok(v)=>{
                let changed={let previous=cache.lock().unwrap();previous["trading_mode"]!=v["trading_mode"] || previous["context"]!=v["context"]};
                paint(&ui,&v,index,action!="poll"||changed);*cache.lock().unwrap()=v;
                if action=="save"{ui.set_brain_monitor_message("حُفظت المشاركة والتدريب لهذا الحساب والرمز والوضع فقط".into());}
                if action=="freeze"{ui.set_brain_monitor_message("أنشئت نسخة مستشار Shadow ثابتة؛ ستُقاس على شموع جديدة دون إرسال أوامر MT5".into());}
                if action=="stop_frozen"{ui.set_brain_monitor_message("أُنهِي استقبال دخول Shadow جديد؛ الصفقة الافتراضية المفتوحة ستُسوى عند وصول سعر لاحق".into());}
                if action=="freeze_coordinator"{ui.set_brain_monitor_message("تم تجميد سياسة منسّق Expert على Demo؛ ستُقارن قرارات مستقبلية دون أي أوامر جديدة".into());}
                if action=="stop_coordinator"{ui.set_brain_monitor_message("أُوقِف رصد سياسة منسّق Expert؛ السجل السابق باقٍ للمراجعة".into());}
            },Err(e)=>ui.set_brain_monitor_message(format!("تعذر التحديث: {e}").into())}
        });
    });timer
}
