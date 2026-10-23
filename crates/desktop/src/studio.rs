use super::*;
use braintrade_domain::{
    studio::TIMEFRAMES,
    trading::{SniperBrain, SniperTargetMode, TradingConfig, TradingMode},
};
use std::fmt::Write;
fn number(s: slint::SharedString) -> anyhow::Result<f64> {
    let v = s.trim().parse::<f64>()?;
    anyhow::ensure!(v.is_finite(), "Use a finite decimal number");
    Ok(v)
}
fn selected_tf(ui: &Dashboard) -> String {
    if ui.get_studio_tf() <= 0 {
        "ALL".into()
    } else {
        TIMEFRAMES
            .get((ui.get_studio_tf() - 1) as usize)
            .unwrap_or(&"ALL")
            .to_string()
    }
}
fn load(ui: &Dashboard, c: &Config) {
    ui.set_priority_enabled(c.trading.priority_enabled);
    ui.set_sniper_active(c.trading.sniper_active);
    ui.set_fallback_mode(match c.trading.fallback_mode {
        TradingMode::Scalping=>0,TradingMode::Swing=>1,_=>2,
    });
    ui.set_trade_mode(match c.trading.mode {
        TradingMode::Scalping => 0,
        TradingMode::Swing => 1,
        TradingMode::Expert => 2,
        TradingMode::Sniper => 3,
    });
    ui.set_sniper_style(match c.trading.sniper_style { braintrade_domain::trading::SniperStyle::Auto=>0, braintrade_domain::trading::SniperStyle::Impulse=>1, braintrade_domain::trading::SniperStyle::Rebound=>2, braintrade_domain::trading::SniperStyle::Confirmed=>3 });
    ui.set_sniper_brain(if c.trading.sniper_brain == SniperBrain::Learner { 1 } else { 0 });
    ui.set_demo_flexible(c.trading.demo_flexible);
    ui.set_adaptive_exits(c.trading.adaptive_exits);
    ui.set_sniper_target((c.trading.sniper_target_points / c.trading.pip_points).to_string().into());
    ui.set_sniper_target_mode(if c.trading.sniper_target_mode == SniperTargetMode::Adaptive {1}else{0});
    ui.set_sniper_min_target_pips(c.trading.sniper_min_target_pips.to_string().into());
    ui.set_sniper_max_target_pips(c.trading.sniper_max_target_pips.to_string().into());
    ui.set_sniper_stop(c.trading.sniper_stop_points.to_string().into());
    ui.set_sniper_cooldown(c.trading.sniper_cooldown_seconds.to_string().into());
    ui.set_sniper_hold(c.trading.sniper_max_hold_seconds.to_string().into());
    ui.set_pip_points(c.trading.pip_points.to_string().into());
    ui.set_sniper_pip_price(c.trading.sniper_pip_price.to_string().into());
    ui.set_cost_fraction(c.trading.max_cost_fraction.to_string().into());
    ui.set_commission_lot(c.risk.commission_per_lot.to_string().into());
    ui.set_slippage_ticks(c.risk.slippage_ticks.to_string().into());
}
fn report(v: &Value) -> String {
    let mut out = format!(
        "{} | {}\n{}\nقاعدة {} | نافذة {} | وقف {} ATR | هدف {}R\nتركيبات {} | فاصل {} شمعة\n",
        v["timeframe"],
        v["algorithm"],
        v["context"],
        v["selected"]["family"],
        v["selected"]["lookback"],
        v["selected"]["stop_atr"],
        v["selected"]["reward"],
        v["trials"],
        v["purge_bars"]
    );
    for (key, name) in [
        ("training", "التدريب"),
        ("validation", "التحقق للاختيار"),
        ("test", "الاختبار النهائي"),
    ] {
        let m = &v[key];
        let _ = writeln!(
            out,
            "{name}: {} صفقات / {} رابحة | صافي {:.3}R | تراجع {:.3}R",
            m["trades"],
            m["wins"],
            m["net_r"].as_f64().unwrap_or(0.),
            m["max_drawdown_r"].as_f64().unwrap_or(0.)
        );
    }
    let _ = writeln!(
        out,
        "اختبار UTC epoch: {} → {}\nأهلية مراجعة بحثية: {}\n{}",
        v["test_start"],
        v["test_end"],
        v["eligible_for_review"],
        v["note"].as_str().unwrap_or("")
    );
    out
}
fn paint(ui: &Dashboard, v: &Value, tf: &str) {
    let mut out = format!(
        "الحساب: {}\nمتصل: {} | تعلم تلقائي: {} | يعمل على: {} | طابور: {}\n{}\n",
        v["context"],
        v["connected"],
        v["auto"],
        v["working"],
        v["queued"],
        v["last_error"].as_str().unwrap_or("")
    );
    let mut w=String::from("وايكوف — النطاق والاختبار على الشموع المكتملة\nالمرحلة تقديرية؛ tick_activity_proxy يعني نشاط ticks وليس حجماً حقيقياً.\n\n");
    for f in v["frames"].as_array().into_iter().flatten() {
        let name = f["timeframe"].as_str().unwrap_or("");
        let _ = writeln!(
            out,
            "{name}: {} شمعة | حديث: {} | {}",
            f["bars"],
            f["live"],
            f["error"].as_str().unwrap_or("")
        );
        if tf != "ALL" && name != tf {
            continue;
        }
        let a = &f["wyckoff"];
        let evidence = a["evidence"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let _ = writeln!(
            w,
            "{name} | حديث: {} | {}\n{} | {} | اتجاه {}\nدعم {} | مقاومة {} | حجم {}\n{evidence}",
            f["live"],
            f["error"].as_str().unwrap_or(""),
            a["phase"].as_str().unwrap_or("غير متاح"),
            a["event"].as_str().unwrap_or("—"),
            a["side"],
            a["support"],
            a["resistance"],
            a["volume_source"].as_str().unwrap_or("—")
        );
        if let Some(m) = v["wyckoff_learning"]
            .as_array()
            .and_then(|r| r.iter().find(|m| m["timeframe"] == name))
        {
            let _=writeln!(w,"تعلم لاحق: {} مقاسة / {} تنتظر | رابحة {}\nمتوسط صافي/لوت {} | تكرار نجاح مملّس {:.1}%\nدليل كافٍ موجب: {}\n",m["scored"],m["pending"],m["wins"],m["mean_net_per_lot"],100.*m["smoothed_win_rate"].as_f64().unwrap_or(0.),m["research_evidence_positive"]);
        } else {
            w.push_str("لم تنضج نتائج مستقبلية بعد؛ القياس بعد 12 شمعة لاحقة.\n\n");
        }
    }
    ui.set_studio_summary(out.into());
    ui.set_wyckoff_detail(w.into());
    let reports = v["reports"].as_array().cloned().unwrap_or_default();
    let ids: Vec<slint::SharedString> = reports
        .iter()
        .map(|r| r["id"].as_str().unwrap_or("").into())
        .collect();
    let names: Vec<slint::SharedString> = reports
        .iter()
        .map(|r| {
            format!(
                "{} | {} | {}",
                r["report"]["timeframe"].as_str().unwrap_or(""),
                r["created_ms"],
                r["id"].as_str().unwrap_or("")
            )
            .into()
        })
        .collect();
    ui.set_studio_ids(std::rc::Rc::new(slint::VecModel::from(ids)).into());
    ui.set_studio_reports(std::rc::Rc::new(slint::VecModel::from(names)).into());
}
pub fn setup(ui: &Dashboard) -> slint::Timer {
    let names: Vec<slint::SharedString> = std::iter::once("ALL")
        .chain(TIMEFRAMES)
        .map(Into::into)
        .collect();
    ui.set_studio_frames(std::rc::Rc::new(slint::VecModel::from(names)).into());
    let (tx, rx) = mpsc::channel::<(String, Value, String)>();
    let sender = tx.clone();
    let weak = ui.as_weak();
    ui.on_studio_action(move|action|{
        if let Some(ui)=weak.upgrade(){
            let action=action.to_string();
            // Cancellation must remain reachable while a long studio request is busy.
            // Use a separate worker rather than queueing behind the running request.
            if action=="cancel" {
                if ui.get_studio_cancel_busy(){return;}
                ui.set_studio_cancel_busy(true);
                let ui_weak=ui.as_weak();
                std::thread::spawn(move || {
                    let result=(||->anyhow::Result<Value>{
                        Client::new(&data_dir())?.post("/v1/studio/cancel",json!({}))
                    })();
                    let _=ui_weak.upgrade_in_event_loop(move |ui|{
                        ui.set_studio_cancel_busy(false);
                        match result {
                            Ok(v) if v["cancelled"]==true => ui.set_studio_message(
                                "تم إرسال طلب إيقاف المختبر وتعطيل التعلم التلقائي.".into()),
                            Ok(_) => ui.set_studio_message(
                                "ردّ المختبر لا يؤكد الإلغاء؛ راجع حالته قبل إعادة التشغيل.".into()),
                            Err(e) => ui.set_studio_message(
                                format!("تعذر إلغاء المختبر: {e}").into()),
                        }
                    });
                });
                return;
            }
            if ui.get_studio_busy() || ui.get_studio_cancel_busy(){return;}
            let tf=selected_tf(&ui);let basis=if ui.get_commission_basis()==1{"per_side"}else{"round_trip"};let body=(||->anyhow::Result<Value>{Ok(match action.as_str(){"save"=>{let fallback_mode=match ui.get_fallback_mode(){0=>TradingMode::Scalping,1=>TradingMode::Swing,_=>TradingMode::Expert};
                let priority_enabled=ui.get_priority_enabled();
                let sniper_active=ui.get_sniper_active();
                let selected_mode=if priority_enabled {
                    if sniper_active {TradingMode::Sniper}else{fallback_mode}
                }else{match ui.get_trade_mode(){
                    0=>TradingMode::Scalping,1=>TradingMode::Swing,
                    3=>TradingMode::Sniper,_=>TradingMode::Expert
                }};
                let trading=TradingConfig{priority_enabled,sniper_active,fallback_mode,sniper_style:match ui.get_sniper_style(){1=>braintrade_domain::trading::SniperStyle::Impulse,2=>braintrade_domain::trading::SniperStyle::Rebound,3=>braintrade_domain::trading::SniperStyle::Confirmed,_=>braintrade_domain::trading::SniperStyle::Auto},sniper_brain:if ui.get_sniper_brain()==1{SniperBrain::Learner}else{SniperBrain::Classic},
sniper_target_mode:if ui.get_sniper_target_mode()==1{SniperTargetMode::Adaptive}else{SniperTargetMode::Fixed},
sniper_min_target_pips:number(ui.get_sniper_min_target_pips())?,
sniper_max_target_pips:number(ui.get_sniper_max_target_pips())?,demo_flexible:ui.get_demo_flexible(),adaptive_exits:ui.get_adaptive_exits(),mode:selected_mode,sniper_target_points:number(ui.get_sniper_target())?*number(ui.get_pip_points())?,sniper_stop_points:number(ui.get_sniper_stop())?,sniper_cooldown_seconds:ui.get_sniper_cooldown().trim().parse()?,sniper_max_hold_seconds:ui.get_sniper_hold().trim().parse()?,pip_points:number(ui.get_pip_points())?,sniper_pip_price:number(ui.get_sniper_pip_price())?,max_cost_fraction:number(ui.get_cost_fraction())?};trading.validate().map_err(anyhow::Error::msg)?;json!({"trading":trading,"commission_per_lot":number(ui.get_commission_lot())?,"commission_basis":basis,"slippage_ticks":number(ui.get_slippage_ticks())?})},
            "handoff_pause"|"handoff_resume"=>json!({
                "sniper_active":action=="handoff_resume",
                "confirm_live_login":ui.get_live_start_login().to_string()
            }),
            "cost"=>json!({"lots":number(ui.get_preview_lots())?,"commission_per_lot":number(ui.get_commission_lot())?,"commission_basis":basis,"pip_points":number(ui.get_pip_points())?,"sniper_pip_price":number(ui.get_sniper_pip_price())?,"slippage_ticks":number(ui.get_slippage_ticks())?}),"report"=>{use slint::Model;json!({"id":ui.get_studio_ids().row_data(ui.get_studio_report_index().max(0)as usize).unwrap_or_default().to_string()})},"auto_on"=>json!({"enabled":true}),_=>json!({"timeframe":tf})})})();match body{Ok(body)=>{ui.set_studio_busy(true);let _=sender.send((action,body,tf));},Err(e)=>ui.set_studio_message(format!("تحقق من الأرقام: {e}").into())}}});
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(4),
        move || {
            if let Some(ui) = weak.upgrade() {
                if (15..=16).contains(&ui.get_page()) && !ui.get_studio_busy() {
                    ui.set_studio_busy(true);
                    let _ = tx.send(("poll".into(), json!({}), selected_tf(&ui)));
                }
            }
        },
    );
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        while let Ok((action, body, tf)) = rx.recv() {
            let result = (|| -> anyhow::Result<Value> {
                let c = Client::new(&data_dir())?;
                match action.as_str() {
                    "config" => c.get("/v1/trading/settings"),
                    "save" => c.post("/v1/trading/settings", body),
                    "handoff_pause"|"handoff_resume" =>
                        c.post("/v1/trading/priority/handoff", body),
                    "cost" => c.post("/v1/trading/preview", body),
                    "report" => c.post("/v1/studio/report", body),
                    "run" => c.post("/v1/studio/run", body),
                    "cancel" => c.post("/v1/studio/cancel", body),
                    "auto_on" => c.post("/v1/studio/auto", body),
                    _ => c.get("/v1/studio"),
                }
            })();
            let _=weak.upgrade_in_event_loop(move|ui|{
                ui.set_studio_busy(false);
                match result {
                    Err(e)=>ui.set_studio_message(format!("تعذر الطلب: {e}").into()),
                    Ok(v)=>match action.as_str(){
                        "handoff_pause"|"handoff_resume"=>{
                            if let Ok(c)=serde_json::from_value::<Config>(v["config"].clone()){
                                load(&ui,&c);
                                ui.set_studio_message(format!(
                                    "تبديل منسّق التنفيذ مؤكّد: {:?}. القناص {}؛ وضع واحد يملك إرسال أوامر MT5.",
                                    c.trading.mode,if c.trading.sniper_active{"فعّال"}else{"متوقف"}
                                ).into());
                            }else{
                                ui.set_studio_message("ردّ التحويل لا يحتوي إعداداً مؤكداً؛ حدّث الصفحة.".into());
                            }
                        },
                        "config"|"save"=>{
                            if let Ok(c)=serde_json::from_value::<Config>(v["config"].clone()){load(&ui,&c);}
                            if let Some(amount)=v["commission_editor"]["amount"].as_f64(){ui.set_commission_lot(amount.to_string().into());}
                            ui.set_commission_basis(if v["commission_editor"]["basis"]=="per_side"{1}else{0});
                            let scoped=v["commission_editor"]["saved"]==true;
                            let roundtrip=v["commission_editor"]["round_trip_per_lot"].as_f64().unwrap_or(0.);
                            // Saving the trading commission may change the fallback risk
                            // commission. Keep its separate editor in round-trip units.
                            if action=="save" {ui.set_risk_commission_lot(roundtrip.to_string().into());}
                            let message=if action=="save"{
                                if scoped{format!("حُفظ الوضع وعمولة الحساب/الرمز. التكلفة الكاملة لكل لوت = {roundtrip:.4}")}
                                else{format!("حُفظ الوضع والعمولة الافتراضية. التكلفة الكاملة لكل لوت = {roundtrip:.4}")}
                            }else if scoped{
                                format!("تم تحميل عمولة الحساب والرمز الحاليين؛ ذهاب وإياب = {roundtrip:.4} لكل لوت")
                            }else{
                                format!("تم تحميل الإعدادات الافتراضية؛ ذهاب وإياب = {roundtrip:.4} لكل لوت")
                            };
                            ui.set_studio_message(message.into());
                        },
                        "cost"=>{
                            let c=&v["costs"];
                            let text=if c.is_null() {
                                format!("العمولة المقدّرة للوت المختار: {:.4} من عملة الحساب\n1.00 لوت: {:.4} | 0.10 لوت: {:.4} | 0.01 لوت: {:.4}\n{}\nالسبريد الحقيقي ونقطة التعادل يتطلبان اتصال MT5 وسعراً حديثاً.\nتنبيه: وضع العمولة = 0 يزيلها من تقديرات البرنامج فقط، ولا يغيّر الرسوم الفعلية التي يفرضها وسيط MT5.",
                                    v["commission"].as_f64().unwrap_or(0.),
                                    v["commission_examples"]["one_lot"].as_f64().unwrap_or(0.),
                                    v["commission_examples"]["one_tenth_lot"].as_f64().unwrap_or(0.),
                                    v["commission_examples"]["one_hundredth_lot"].as_f64().unwrap_or(0.),
                                    v["error"].as_str().unwrap_or("لا تتوفر بيانات العرض"))
                            } else {
                                let n=|key:&str|c[key].as_f64().unwrap_or(0.);
                                format!(
                                    "عملة الحساب: {}  |  حجم الصفقة: {:.2} لوت\n\
                                     السعر الحالي: Bid {:.5}   /   Ask {:.5}\n\
                                     حجم النقطة: {:.5}  |  حجم 1 pip وفق إعدادك: {:.5}\n\
                                     السبريد الحقيقي (Ask − Bid): {:.5} = {:.2} نقطة = {:.2} pip\n\
                                     تكلفة السبريد على اللوت المختار: {:.4}\n\
                                     عمولة ذهاب وإياب / 1.00 لوت: {:.4}\n\
                                     أمثلة العمولة: 1.00 لوت = {:.4} | 0.10 لوت = {:.4} | 0.01 لوت = {:.4}\n\
                                     عمولة هذا اللوت: {:.4}\n\
                                     الانزلاق المفترض: {:.2} tick لكل جهة = {:.4} (تقدير وليس سبريد)\n\
                                     مجموع التكلفة التقديرية: {:.4}\n\
                                     التعادل من سعر الدخول (Ask شراء / Bid بيع): {:.2} نقطة / {:.2} pip — دون إضافة السبريد مجدداً\n\
                                     التعادل من السعر قبل الدخول (Bid شراء / Ask بيع): {:.2} نقطة / {:.2} pip — يشمل السبريد مرة واحدة\n\
                                     سعر التعادل النظري للشراء عند Bid: {:.5}  |  للبيع عند Ask: {:.5}\n\
                                     معاينة لصفقة جديدة وليست التعادل الفعلي لصفقة مفتوحة؛ القيم وفق سعر MT5 وقت الضغط على احسب. وضع العمولة = 0 لا يعفيك من عمولة الوسيط الحقيقية.",
                                    c["currency"].as_str().unwrap_or(""),n("lots"),
                                    n("bid"),n("ask"),
                                    n("point_price"),n("pip_price"),
                                    n("spread_price"),n("spread_points"),n("spread_pips"),
                                    n("spread_money"),
                                    n("commission_round_trip_per_lot"),
                                    n("commission_round_trip_per_lot"),
                                    n("commission_round_trip_per_lot")*0.1,
                                    n("commission_round_trip_per_lot")*0.01,
                                    n("commission"),
                                    n("slippage_ticks_per_side"),n("slippage_money"),
                                    n("total"),
                                    n("break_even_from_entry_points"),n("break_even_from_entry_pips"),
                                    n("break_even_points"),n("break_even_pips"),
                                    n("buy_break_even_bid"),n("sell_break_even_ask")
                                )
                            };
                            ui.set_cost_detail(text.into());
                        },
                        "report"=>ui.set_studio_report(report(&v).into()),
                        "poll"|"refresh"=>paint(&ui,&v,&tf),
                        _=>ui.set_studio_message(serde_json::to_string_pretty(&v).unwrap_or_default().into())
                    }
                }
            });
        }
    });
    timer
}