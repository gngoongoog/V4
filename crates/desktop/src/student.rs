use super::*;
use std::fmt::Write;

fn label(value: &str) -> &str {
    match value {
        "baseline" => "الإعداد الأصلي",
        "quick" => "خروج أسرع",
        "patient" => "هدف أبعد",
        "collecting" => "جمع الخبرة",
        "validating" => "اختبار على صفقات جديدة",
        "monitoring" => "مراقبة التحسّن",
        "cost_drag" => "التكاليف محت الربح",
        "adverse_entry_path" => "حركة معاكسة بعد الدخول",
        "profit_giveback" => "رجوع ربح مرصود",
        "positive_after_costs" => "ربح بعد التكاليف",
        "unclassified" => "السبب غير محسوم",
        "observation_gap" => "فجوة في وصول الرصد",
        "history_recovery" => "استرجاع تنفيذ؛ مسار الأسعار غير مكتمل",
        "missing_fill_time" => "وقت تأكيد التنفيذ غير متاح",
        "too_few_quotes" => "عدد الأسعار المرصودة غير كافٍ للمقارنة",
        "volume_changed" => "حجم الإغلاق يختلف عن التجربة",
        "expired" => "انتهت مهلة الرصد",
        "legacy_quality_unknown" => "تجربة قديمة؛ سبب الاستبعاد غير مسجّل",
        "compare_net_exits_after_repeated_cost_drag" => "التكاليف تكررت: مقارنة صافي طرق الخروج",
        "compare_exits_after_repeated_giveback" => "رجوع الربح تكرر: اختبار طريقة خروج بديلة",
        "compare_exits_entry_cause_unproven" => "مقارنة خروج؛ سبب الدخول المعاكس يحتاج بحثاً مستقلاً",
        "compare_exits_from_paired_evidence" => "اختبار بديل اختارته المقارنات السابقة",
        "challenger_frozen_for_future_entries" => "تثبيت البديل؛ بدأ الاختبار اللاحق",
        "baseline_retained_after_training" => "الإعداد الأصلي أفضل في عينة التدريب",
        "promoted_on_later_paired_quotes" => "تفعيل البديل بعد تحسّن المقارنة اللاحقة",
        "validation_did_not_confirm_improvement" => "الاختبار اللاحق لم يؤكد التحسّن",
        "returned_to_baseline_after_deterioration" => "الرجوع للأصل بعد تراجع الأداء",
        "improvement_retained_in_monitoring" => "التحسّن مستمر في نافذة المراقبة",
        _ => value,
    }
}

pub fn paint(ui: &Dashboard, v: &Value) {
    ui.set_student_total(v["total_experiences"].to_string().into());
    ui.set_student_closed(v["closed_broker_trades"].to_string().into());
    ui.set_student_pairs(v["usable_shadow_pairs"].to_string().into());
    ui.set_student_status(
        if v["demo_connected"] == true {
            "الديمو متصل • التعلّم محفوظ لهذا الحساب والرمز"
        } else {
            "بيانات الحساب المحفوظة • ينتظر اتصال ديمو حديث"
        }
        .into(),
    );
    let mut patterns = String::new();
    for (key, count) in v["recent_pattern_counts"].as_object().into_iter().flatten() {
        let _ = writeln!(patterns, "{}: {}", label(key), count);
    }
    if patterns.is_empty() {
        patterns =
            "لا توجد صفقات مغلقة في سجل الطالب بعد. تبدأ المراجعة بعد وصول النتيجة الكاملة من MT5."
                .into();
    }
    let _ = writeln!(
        patterns,
        "\nالنافذة الأخيرة: {} صفقة مغلقة • الصافي {:+.2}",
        v["recent_closed"],
        v["recent_net"].as_f64().unwrap_or(0.)
    );
    for (reason, count) in v["recent_exclusions"].as_object().into_iter().flatten() {
        let _ = writeln!(patterns, "استبعاد مقارنة: {} • {}", label(reason), count);
    }
    if let Some(blockers) = v["blocker_observations"]
        .as_array()
        .filter(|v| !v.is_empty())
    {
        let _ = writeln!(
            patterns,
            "\nأسباب الانتظار — مرات رصد خلال 7 أيام، وليست صفقات فائتة:"
        );
        for b in blockers.iter().take(6) {
            let _ = writeln!(
                patterns,
                "{}: {} ({})",
                b["brain"].as_str().unwrap_or(""),
                b["reason"].as_str().unwrap_or(""),
                b["observations"]
            );
        }
    }
    ui.set_student_patterns(patterns.into());
    let mut policies = String::new();
    for item in v["studies"].as_array().into_iter().flatten() {
        let s = &item["study"];
        let source: Value =
            serde_json::from_str(item["source"].as_str().unwrap_or("[]")).unwrap_or(Value::Null);
        let _=writeln!(policies,"{} / {} / {}\n{} • {} / 20 مقارنة\nالمعتمد بالتعلّم: {} | المرشح: {}\nآخر فرق متوسط: {:+.3} R | ترقيات {} • رجوع {}\n{}\n",
            source[3].as_str().unwrap_or("—"),source[4].as_str().unwrap_or("—"),source[2].as_str().unwrap_or("—"),
            label(s["phase"].as_str().unwrap_or("collecting")),s["samples"],label(s["active"].as_str().unwrap_or("baseline")),
            label(s["challenger"].as_str().unwrap_or("baseline")),s["last_gain_r"].as_f64().unwrap_or(0.),s["promotions"],s["rollbacks"],
            label(s["last_decision"].as_str().unwrap_or("")));
        let _ = writeln!(
            policies,
            "خسائر تمت مراجعتها: {} • {}\n",
            s["reviewed_losses"].as_u64().unwrap_or(0),
            label(s["experiment_reason"].as_str().unwrap_or(""))
        );
        if let Some(regime) = source
            .as_array()
            .and_then(|a| a.last())
            .and_then(|v| v["exit_regime"].as_str())
        {
            let _ = writeln!(policies, "حالة السوق: {regime}\n");
        }
    }
    if policies.is_empty() {
        policies="الخروج الأصلي فعّال. تُجمع مقارنات الهدف والوقف والمدة من الأسعار القادمة، ثم يُختبر البديل على دخول جديد قبل اعتماده.".into();
    }
    ui.set_student_policies(policies.into());
    let mut journal = String::new();
    for e in v["experiences"].as_array().into_iter().flatten().take(20) {
        let sel = &e["entry"]["signal"]["coordination"]["selection"];
        let intel = &e["entry"]["learning"]["intelligence"];
        let profile = label(
            e["entry"]["signal"]["coordination"]["student"]["profile"]
                .as_str()
                .unwrap_or("baseline"),
        );
        let exit_label = if e["entry"]["signal"]["coordination"]["student"]["cost_fallback"] == true
        {
            format!("{profile} — الهدف المتعلّم لا يناسب التكلفة الحالية")
        } else {
            profile.to_owned()
        };
        let net = e["actual"]["net"]
            .as_f64()
            .map(|n| format!("{n:+.2}"))
            .unwrap_or_else(|| "بانتظار إغلاق كامل".into());
        let _=writeln!(journal,"{} | صفقة {} | صافي MT5: {}\n{} / {} | {}\nاستكشاف {:.1}% • عينات السياق {} | الخروج: {}\nMFE مرصود {:.5} / MAE {:.5} • رصد الصفقة {} • جودة المسار {}\nمقارنة الخروج: {} • أكبر فجوة {} ms\n",
            e["brain"].as_str().unwrap_or("—"),e["position_id"],net,
            intel["regime"].as_str().unwrap_or("—"),intel["session"].as_str().unwrap_or("—"),
            e["entry"]["signal"]["reason"].as_str().unwrap_or(""),
            sel["exploration_rate"].as_f64().unwrap_or(0.)*100.,sel["context_samples"].as_u64().unwrap_or(0),
            exit_label,
            e["observed_mfe_price"].as_f64().unwrap_or(0.),e["observed_mae_price"].as_f64().unwrap_or(0.),e["position_samples"],
            if e["position_path_usable"]==true{"كافٍ للوصف"}else{"محدود"},
            if e["shadow"]["usable"]!=true{label(e["shadow"]["exclusion_reason"].as_str().filter(|s|!s.is_empty()).unwrap_or("legacy_quality_unknown"))}else if e["admitted"]==true{"دخلت التعلّم"}else if e["study_consumed"]==true{"لم تدخل نافذة المقارنة"}else{"تنتظر أسعاراً لاحقة"},e["shadow"]["max_observation_gap_ms"]);
    }
    if journal.is_empty() {
        journal =
            "بعد أول تعبئة فعلية يظهر هنا سبب الدخول، السياق، الملف المستخدم والنتائج.".into();
    }
    ui.set_student_journal(journal.into());
}

pub fn setup(ui: &Dashboard) -> slint::Timer {
    let (tx, rx) = mpsc::channel::<()>();
    let weak = ui.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(2),
        move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_page() == 19 && !ui.get_student_busy() {
                    ui.set_student_busy(true);
                    let _ = tx.send(());
                }
            }
        },
    );
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            let result = Client::new(&data_dir()).and_then(|c| c.get("/v1/execution/student"));
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_student_busy(false);
                match result {
                    Ok(v) => paint(&ui, &v),
                    Err(e) => ui.set_student_status(format!("تعذر تحديث سجل الطالب: {e}").into()),
                }
            });
        }
    });
    timer
}
