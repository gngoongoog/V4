use super::*;
use braintrade_domain::execution::{BrainConfig, BRAINS};
use std::fmt::Write;

const FRAMES: [&str; 21] = [
    "M1", "M2", "M3", "M4", "M5", "M6", "M10", "M12", "M15", "M20", "M30", "H1", "H2", "H3", "H4",
    "H6", "H8", "H12", "D1", "W1", "MN1",
];

fn number(s: slint::SharedString) -> anyhow::Result<f64> {
    let n = s.trim().parse::<f64>()?;
    anyhow::ensure!(n.is_finite(), "اكتب رقماً صالحاً");
    Ok(n)
}

fn paint(ui: &Dashboard, v: &Value, load: bool, index: usize) {
    let settings = &v["settings"];
    let portfolio = &v["portfolio"];
    let portfolio_risk = portfolio["estimated_stop_risk_percent_equity"]
        .as_f64()
        .map(|value| format!("{value:.2}%"))
        .unwrap_or_else(|| "غير معروف".into());

    ui.set_exec_summary(
        format!(
            "{} | حساب {} / {}\nالتنفيذ: {} | مطابق للحساب: {}\nحماية المحفظة: {} | تعرض {} / {} | لوت {:.4} / {:.4} | خطر وقف {} / {}%",
            settings["target"].as_str().unwrap_or("demo"),
            settings["login"],
            settings["server"].as_str().unwrap_or("—"),
            v["armed"],
            v["account_matches"],
            if portfolio["enabled"] == true { "مفعلة" } else { "معطلة" },
            portfolio["exposure_slots"],
            portfolio["limits"]["max_positions"],
            portfolio["lots"].as_f64().unwrap_or(0.0),
            portfolio["limits"]["max_lots"].as_f64().unwrap_or(0.0),
            portfolio_risk,
            portfolio["limits"]["max_risk_percent"].as_f64().unwrap_or(0.0),
        )
        .into(),
    );

    let Some(b) = v["brains"].as_array().and_then(|bs| bs.get(index)) else {
        return;
    };
    let config = &b["config"];
    ui.set_exec_enabled(config["enabled"] == true);
    ui.set_exec_role(b["role"].as_str().unwrap_or("advisor").into());

    if load {
        ui.set_exec_target(if settings["target"] == "live" { 1 } else { 0 });
        ui.set_exec_deviation(settings["deviation_points"].to_string().into());
        ui.set_exec_frame(
            FRAMES
                .iter()
                .position(|f| config["timeframe"] == *f)
                .unwrap_or(0) as i32,
        );
        ui.set_exec_lots(config["lots"].to_string().into());
        ui.set_exec_stop(config["stop_atr"].to_string().into());
        ui.set_exec_reward(config["reward_risk"].to_string().into());
        ui.set_exec_hold(config["max_hold_seconds"].to_string().into());
        ui.set_exec_cooldown(config["cooldown_seconds"].to_string().into());
        ui.set_exec_risk(config["use_risk_limits"] == true);
        ui.set_exec_report(config["report_id"].as_str().unwrap_or("").into());

        ui.set_portfolio_enabled(portfolio["enabled"] == true);
        ui.set_portfolio_max_positions(
            portfolio["limits"]["max_positions"].to_string().into(),
        );
        ui.set_portfolio_max_lots(portfolio["limits"]["max_lots"].to_string().into());
        ui.set_portfolio_max_risk(
            portfolio["limits"]["max_risk_percent"].to_string().into(),
        );
    }

    let s = &b["latest"];
    let a = &s["signal"];
    let gates = s["gates"]
        .as_array()
        .map(|g| {
            g.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" • ")
        })
        .unwrap_or_default();
    ui.set_exec_detail(format!("{} | Magic {}\n{} | اتجاه {} | جاهز للإرسال: {}\n{}\nآخر حساب للعقل: {} ميكروثانية (ليس زمن الوسيط)\n{}",b["name"].as_str().unwrap_or(""),b["magic"],a["reason"].as_str().unwrap_or("بانتظار تحديث MT5"),a["side"],s["ready"],gates,s["decision_us"],if config["use_risk_limits"]==true{"الحدود الاختيارية مفعّلة"}else{"الحدود الاختيارية معطلة؛ اللوت ثابت كما اخترته"}).into());

    let mut out = String::new();
    let _ = writeln!(
        out,
        "حماية المحفظة: {} | صفقات ظاهرة {} | أوامر دخول غير محسومة {} | إجمالي لوت {:.4} | خطر الوقف {}\n",
        if portfolio["enabled"] == true { "مفعلة" } else { "معطلة افتراضياً" },
        portfolio["open_positions"],
        portfolio["unresolved_open_intents"],
        portfolio["lots"].as_f64().unwrap_or(0.0),
        portfolio_risk,
    );
    for b in v["brains"].as_array().into_iter().flatten() {
        let stats = &b["stats"];
        let _ = writeln!(
            out,
            "{} | {} | مغلقة {} / رابحة {} | صافي مسجل {:.2} | عائم {:.2}",
            b["name"].as_str().unwrap_or(""),
            match b["role"].as_str().unwrap_or("") {
                "coordinator" => "الرئيسي",
                "solo" => "منفرد",
                "returning" => "بانتظار الرجوع",
                _ => "عضو في المنظومة",
            },
            stats["closed_positions"],
            stats["wins"],
            stats["net_booked"].as_f64().unwrap_or(0.),
            stats["floating"].as_f64().unwrap_or(0.)
        );
    }
    if let Some(members) = a["coordination"]["members"].as_array() {
        out.push_str("\nالأعضاء الحاليون وأوزان التعلم:\n");
        for member in members {
            let _ = writeln!(
                out,
                "{} {} | اتجاه {} | وزن {:.3} | عينات {} | متوسط صافي/لوت {:.3}",
                member["id"].as_str().unwrap_or(""),
                member["timeframe"].as_str().unwrap_or(""),
                member["side"],
                member["weight"].as_f64().unwrap_or(1.),
                member["samples"],
                member["mean_net_per_lot"].as_f64().unwrap_or(0.)
            );
        }
    }
    out.push_str("\nأحدث الأوامر — الأزمنة مقاسة على جهازك:\n");
    for cmd in v["commands"].as_array().into_iter().flatten().take(20) {
        let q = &cmd["request"];
        let r = &cmd["result"];
        let _ = writeln!(
            out,
            "{} | {} | {} | {}\n{} | انتظار {}ms / فحص {}ms / رد MT5 {}ms\n",
            q["brain"].as_str().unwrap_or(""),
            q["kind"].as_str().unwrap_or(""),
            cmd["state"].as_str().unwrap_or(""),
            q["id"].as_str().unwrap_or(""),
            r["reason"].as_str().unwrap_or("بانتظار البوابة"),
            r["queue_to_gateway_ms"],
            r["check_ms"],
            r["send_ms"]
        );
    }
    ui.set_exec_ledger(out.into());
}

pub fn setup(ui: &Dashboard) -> slint::Timer {
    ui.set_exec_names(slint::ModelRc::new(slint::VecModel::from(
        BRAINS
            .iter()
            .map(|b| slint::SharedString::from(b.1))
            .collect::<Vec<_>>(),
    )));
    ui.set_exec_frames(slint::ModelRc::new(slint::VecModel::from(
        FRAMES
            .iter()
            .map(|f| slint::SharedString::from(*f))
            .collect::<Vec<_>>(),
    )));

    let (tx, rx) = mpsc::channel::<(String, Value, usize)>();
    let sender = tx.clone();
    let weak = ui.as_weak();
    ui.on_execution_action(move |action| {
        let Some(ui) = weak.upgrade() else { return; };
        if ui.get_exec_busy() {
            return;
        }
        let index = (ui.get_exec_selected().max(0) as usize).min(BRAINS.len() - 1);
        let id = BRAINS[index].0;
        let action = action.to_string();
        let body = (|| -> anyhow::Result<Value> {
            Ok(match action.as_str() {
                "enable" | "save" => {
                    let b = BrainConfig {
                        id: id.into(),
                        enabled: action == "enable" || ui.get_exec_enabled(),
                        timeframe: FRAMES[ui.get_exec_frame().max(0) as usize].into(),
                        lots: number(ui.get_exec_lots())?,
                        stop_atr: number(ui.get_exec_stop())?,
                        reward_risk: number(ui.get_exec_reward())?,
                        max_hold_seconds: ui.get_exec_hold().trim().parse()?,
                        cooldown_seconds: ui.get_exec_cooldown().trim().parse()?,
                        use_risk_limits: ui.get_exec_risk(),
                        report_id: ui.get_exec_report().trim().to_owned(),
                    };
                    b.validate().map_err(anyhow::Error::msg)?;
                    serde_json::to_value(b)?
                }
                "bind" => json!({
                    "target":if ui.get_exec_target()==1{"live"}else{"demo"},
                    "deviation_points":ui.get_exec_deviation().trim().parse::<u32>()?
                }),
                "portfolio" => json!({
                    "enabled":ui.get_portfolio_enabled(),
                    "max_positions":ui.get_portfolio_max_positions().trim().parse::<u32>()?,
                    "max_lots":number(ui.get_portfolio_max_lots())?,
                    "max_risk_percent":number(ui.get_portfolio_max_risk())?
                }),
                _ => json!({"brain":id}),
            })
        })();
        match body {
            Ok(body) => {
                ui.set_exec_busy(true);
                let _ = sender.send((action, body, index));
            }
            Err(e) => ui.set_exec_message(format!("تحقق من الإعدادات: {e}").into()),
        }
    });

    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(2),
        move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_page() == 17 && !ui.get_exec_busy() {
                    ui.set_exec_busy(true);
                    let _ = tx.send((
                        "poll".into(),
                        json!({}),
                        ui.get_exec_selected().max(0) as usize,
                    ));
                }
            }
        },
    );

    let weak = ui.as_weak();
    std::thread::spawn(move || {
        while let Ok((action, body, index)) = rx.recv() {
            let result = (|| -> anyhow::Result<Value> {
                let c = Client::new(&data_dir())?;
                match action.as_str() {
                    "enable" | "save" => {
                        c.post("/v1/execution/brain", body)?;
                    }
                    "disable" => {
                        let v = c.get("/v1/execution")?;
                        let mut b = v["brains"][index]["config"].clone();
                        b["enabled"] = json!(false);
                        c.post("/v1/execution/brain", b)?;
                    }
                    "bind" => {
                        c.post("/v1/execution/settings", body)?;
                    }
                    "portfolio" => {
                        c.post("/v1/execution/portfolio", body)?;
                    }
                    "close" => {
                        c.post("/v1/execution/close", body)?;
                    }
                    _ => {}
                }
                c.get("/v1/execution")
            })();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_exec_busy(false);
                match result {
                    Ok(v) => {
                        if ui.get_exec_selected().max(0) as usize == index {
                            paint(&ui, &v, action != "poll", index);
                        }
                        if !matches!(action.as_str(), "poll" | "select" | "refresh") {
                            ui.set_exec_message(
                                "تم تنفيذ الطلب وتحديث الحالة. تفاصيل الرفض أو التنفيذ تظهر في السجل أدناه."
                                    .into(),
                            );
                        }
                    }
                    Err(e) => ui.set_exec_message(format!("تعذر الطلب: {e}").into()),
                }
            });
        }
    });
    timer
}
