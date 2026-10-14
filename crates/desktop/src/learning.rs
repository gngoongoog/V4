//! Native controls for the existing immutable learning/audit APIs.
use super::*;
use anyhow::{bail, Context};
use std::fmt::Write;

fn summary(value: &Value) -> String {
    let mut out = format!("عينات الشموع: {} | نتائج ناضجة: {} | قيد الانتظار: {}\nنماذج مسجلة: {}\nChallenger: {}\n",
        value["snapshots"],value["matured_labels"],value["pending_labels"],value["models"],
        value["challenger"].as_str().unwrap_or("لم يُسجل نموذج بعد"));
    let review=&value["manual_review_eligibility"];
    let inference=&value["read_only_inference"];
    let scoring=&value["prediction_scoring"];
    let performance=&value["challenger_performance_review"];
    let _=writeln!(out,"\nأهلية الدليل الزمني: {}\n{}\nتوقعات للمراقبة: {} | نتائج مقاسة: {} | تنتظر نضج الهدف: {}\nجاهزية التوقع البحثي: {}",
        review["eligible_for_manual_review"],review["reason"].as_str().unwrap_or("راجع تفاصيل الدليل قبل تسجيل المراجعة"),
        inference["predictions"],scoring["scored"],scoring["pending"],inference["read_only_gate_ready"]);
    let _=writeln!(out,"\nمراجعة أداء Challenger: {} | عينات: {} | مرشح للنظر البشري فقط: {}\nنقص الدليل: {}\nأسباب الرفض: {}\nتحذيرات: {}",
        performance["status"].as_str().unwrap_or("insufficient_evidence"),performance["samples"],
        performance["eligible_for_human_consideration"],performance["insufficient_reason_codes"],
        performance["reject_reason_codes"],performance["warning_codes"]);
    for group in scoring["groups"].as_array().into_iter().flatten() {
        let m=&group["metrics"];
        let number=|key:&str|m[key].as_f64().map(|v|format!("{v:.4}")).unwrap_or_else(||"—".into());
        let _=writeln!(out,"\n{} | {}\nعينات: {} | توزيع الفئات: {}\nAccuracy {} | Balanced {} | Macro-F1 {}\nLog-loss {} | Brier {} | ECE {}\nتحذير قلة العينات: {}",
            group["model_id"].as_str().unwrap_or("—"),group["target"].as_str().unwrap_or("—"),m["samples"],m["class_support"],
            number("accuracy"),number("balanced_accuracy"),number("macro_f1"),number("mean_log_loss"),number("mean_brier"),number("ece_10_bin"),m["small_sample_warning"]);
    }
    for part in [value,inference,scoring,performance] {
        if let Some(error)=part["last_error"].as_str().filter(|s|!s.is_empty()) {let _=writeln!(out,"خطأ: {error}");}
    }
    out.push_str("\nالتدريب خارج المحرك. التوقعات ومراجعة الأداء للمراقبة فقط؛ لا تغيّر Meta أو المحاكاة أو المخاطرة، ولا يوجد نموذج Champion مفعّل.");
    out
}

fn opportunity_history(value: &Value) -> String {
    let mut out=String::from("سجل تاريخي — أول رصد لكل نموذج، وليس أسعاراً حالية أو أوامر منفذة.\n");
    let rows=value["setups"].as_array();
    if rows.is_none_or(|rows|rows.is_empty()) {out.push_str("لم تُحفظ فرص بعد. يحتاج السجل بيانات MT5 وشروط نموذج مكتمل.");}
    for row in rows.into_iter().flatten() {
        let setup=&row["setup"];
        let _=writeln!(out,"\n{} | {} | {}\nشمعة UTC epoch {} | الحالة عند أول رصد: {}\n{}\n{}",
            row["context"].as_str().unwrap_or("—"),setup["name"].as_str().unwrap_or("—"),
            if setup["side"]==1 {"BUY"}else{"SELL"},row["bar_time"],setup["state"].as_str().unwrap_or("—"),
            setup["reasons"],setup["gates"]);
    }
    out
}

fn run_action(client:&Client,action:&str,model:&str,note:&str)->anyhow::Result<String> {
    match action {
        "refresh"=>Ok("تم تحديث بيانات التعلم".into()),
        "export"=>{let v=client.post("/v1/learning/export",json!({}))?;Ok(format!("Dataset: {}\n{} rows",v["path"].as_str().unwrap_or("—"),v["rows"]))},
        "register"=>{
            let Some(path)=rfd::FileDialog::new().add_filter("Research model JSON",&["json"]).pick_file() else{return Ok("أُلغي اختيار النموذج".into());};
            if std::fs::metadata(&path)?.len()>1_900_000 {bail!("Model file exceeds the Core request limit");}
            let artifact:Value=serde_json::from_slice(&std::fs::read(path)?)?;
            let v=client.post("/v1/learning/register",artifact)?;
            Ok(format!("سُجل نموذج بحثي: {}",v["model_id"].as_str().unwrap_or("—")))
        },
        "review"=>{
            let eligibility=client.get("/v1/learning/review/status")?;
            if model.is_empty() || eligibility["model_id"].as_str()!=Some(model) {bail!("Challenger changed; refresh and review the current evidence first");}
            if eligibility["eligible_for_manual_review"]!=true {bail!("Chronological evidence does not cover every model context");}
            let v=client.post("/v1/learning/review/create",json!({"model_id":model,"evaluation_ids":eligibility["evidence_ids"],"note":note}))?;
            Ok(format!("حُفظت أهلية المراجعة: {}. التوقعات تبقى للمراقبة فقط.",v["id"].as_str().unwrap_or("—")))
        },
        "details"=>{
            let value=client.get("/v1/learning")?;
            Ok(serde_json::to_string_pretty(&value)?)
        },
        "predictions"=>Ok(serde_json::to_string_pretty(&client.get("/v1/learning/inference/recent")?)?),
        "outcomes"=>Ok(serde_json::to_string_pretty(&client.get("/v1/learning/scoring/recent")?)?),
        "records"=>{
            let eligibility=client.get("/v1/learning/review/records")?;
            let performance=client.get("/v1/learning/performance-review/records")?;
            Ok(format!("Manual eligibility records:\n{}\n\nChallenger performance records:\n{}",
                serde_json::to_string_pretty(&eligibility)?,serde_json::to_string_pretty(&performance)?))
        },
        _=>bail!("Unknown learning action"),
    }
}

pub fn setup(ui:&Dashboard)->slint::Timer {
    let (tx,rx)=mpsc::channel::<(String,String,String)>();
    let sender=tx.clone();let weak=ui.as_weak();
    ui.on_learning_action(move |action|{if let Some(ui)=weak.upgrade(){
        ui.set_learning_busy(true);
        let _=sender.send((action.to_string(),ui.get_learning_model().to_string(),ui.get_learning_note().to_string()));
    }});
    let sender=tx.clone();let weak=ui.as_weak();
    ui.on_load_opportunity_history(move ||{if let Some(ui)=weak.upgrade(){ui.set_opportunity_history("جاري تحميل السجل…".into());}let _=sender.send(("opportunities".into(),String::new(),String::new()));});
    let timer=slint::Timer::default();let weak=ui.as_weak();
    timer.start(slint::TimerMode::Repeated,Duration::from_secs(10),move ||{
        if weak.upgrade().is_some_and(|ui|ui.get_page()==11 && !ui.get_learning_busy()) {
            let _=tx.send(("poll".into(),String::new(),String::new()));
        }
    });
    let weak=ui.as_weak();
    std::thread::spawn(move ||{while let Ok((action,model,note))=rx.recv(){
        let result=(||->anyhow::Result<(Option<Value>,String)>{
            let client=Client::new(&data_dir()).context("المحرك غير متاح؛ افتح التطبيق وانتظر الاتصال")?;
            if action=="opportunities" {return Ok((None,opportunity_history(&client.get("/v1/opportunities/history")?)));}
            let message=if action=="poll" {String::new()} else {run_action(&client,&action,&model,&note)?};
            Ok((Some(client.get("/v1/learning")?),message))
        })();
        let _=weak.upgrade_in_event_loop(move |ui|{
            ui.set_learning_busy(false);
            match result {
                Ok((None,text))=>ui.set_opportunity_history(text.into()),
                Ok((Some(value),text))=>{
                    ui.set_learning_summary(summary(&value).into());
                    ui.set_learning_model(value["challenger"].as_str().unwrap_or("").into());
                    ui.set_learning_review_ready(value["manual_review_eligibility"]["eligible_for_manual_review"]==true);
                    ui.set_learning_status(format!("آخر تحديث UTC epoch {}",braintrade_runtime::now_ms()/1000).into());
                    if !text.is_empty(){ui.set_learning_detail(text.into());}
                },
                Err(error)=>{
                    if action=="opportunities" {ui.set_opportunity_history(format!("تعذر تحميل السجل: {error}").into());}
                    else {
                        ui.set_learning_status(format!("تعذر التحديث: {error}").into());
                        ui.set_learning_review_ready(false);
                        ui.set_learning_summary("البيانات الحالية غير متاحة. التفاصيل المعروضة من آخر طلب ناجح فقط.".into());
                    }
                }
            }
        });
    }});
    timer
}
