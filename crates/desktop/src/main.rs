#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use braintrade_domain::{Config, RiskConfig, Status};
use braintrade_runtime::{data_dir, Client};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::{sync::mpsc, time::Duration};
slint::include_modules!();
mod backtest;
mod correlation;
mod evaluation;
mod learning;
mod backup;
mod news;
mod render;
mod studio;
mod execution;
mod student;
mod trade_journal;
mod brain_monitor;

enum Task {
    Action(String, Value),
    Save(Value),
    Risk(Value),
    Browse,
}

fn paint(ui: &Dashboard, status: Status) {
    // A fresh response from the gateway does not imply an authenticated MT5
    // account is connected. Both conditions must hold for an ONLINE display.
    let broker_online = status.gateway_fresh
        && status.telemetry.as_ref().is_some_and(|t| t.connected);
    ui.set_execution_active(status.execution_enabled);
    ui.set_brains(status.analysis.text().into());
    let opportunities = status.analysis.opportunities.as_ref();
    ui.set_opportunities(opportunities.map(|value| value.signals_text()).unwrap_or_else(||
        "بانتظار شموع MT5 المكتملة؛ لا توجد أرقام دخول صالحة حالياً.".into()).into());
    ui.set_market_map(opportunities.map(|value| value.map_text()).unwrap_or_else(||
        "خريطة المناطق والزخم وGann وDOM غير متاحة حالياً.".into()).into());
    ui.set_regime(
        if status.analysis.current {
            status.analysis.regime.clone()
        } else {
            "UNAVAILABLE".into()
        }
        .into(),
    );
    ui.set_decision(status.analysis.action.clone().into());
    ui.set_paper(status.paper.text().into());
    ui.set_state(format!("{:?}", status.state).to_uppercase().into());
    ui.set_online(broker_online);
    ui.set_telegram_status(status.telegram_status.into());
    ui.set_connection(
        if broker_online {
            "CONNECTED / متصل"
        } else {
            "DISCONNECTED OR STALE / غير متصل أو بيانات قديمة"
        }
        .into(),
    );
    ui.set_diagnostics(status.message.into());
    if let Some(t) = status.telemetry.filter(|_| broker_online) {
        ui.set_account(
            format!(
                "{} | {} | {} | {}",
                t.login,
                t.server,
                if t.demo { "DEMO" } else { "REAL" },
                t.symbol
            )
            .into(),
        );
        ui.set_balance(format!("{:.2} {}", t.balance, t.currency).into());
        ui.set_equity(format!("{:.2} {}", t.equity, t.currency).into());
        ui.set_broker_leverage(t.leverage.map(|v| format!("1:{v}")).unwrap_or_else(|| "غير متاحة".into()).into());
        let scenario = ui.get_scenario_leverage().trim().parse::<u32>();
        let note = match (scenario, t.leverage.filter(|n| *n > 0), t.minimum_lot_margin,
                          t.margin_free, t.volume_min) {
            (Ok(s), Some(actual), Some(min_margin), Some(free), Some(min_lot))
                if (1..=3000).contains(&s) && min_margin.is_finite() && min_margin >= 0.
                    && free.is_finite() && free >= 0. && min_lot.is_finite() && min_lot > 0. => {
                let approximation = min_margin * f64::from(actual) / f64::from(s);
                format!("أقل لوت لدى الوسيط: {:.3} | الهامش الحالي المقدّر: {:.2} {} | الهامش الحر: {:.2} {}. عند 1:{} الهامش الافتراضي التقريبي: {:.2} {}. مقارنة تعليمية بتناسب عكسي فقط؛ قد لا تنطبق على الذهب أو شروط الهامش الخاصة. لا تغيّر رافعة الحساب أو أوامر MT5 أو خسارة الستوب.",
                    min_lot, min_margin, t.currency, free, t.currency, s, approximation, t.currency)
            }
            (Err(_), _, _, _, _) => "أدخل رافعة صحيحة مثل 100 أو 200 أو 500.".into(),
            (Ok(s), _, _, _, _) if !(1..=3000).contains(&s) => "مدى سيناريو الرافعة: من 1:1 إلى 1:3000، للتخطيط فقط.".into(),
            _ => "بيانات الهامش أو رافعة MT5 غير متوفرة؛ لن نخترع تقديراً. تحقق من اتصال الوسيط وعقد الرمز.".into(),
        };
        ui.set_leverage_note(note.into());
        ui.set_today(
            t.today_net
                .map(|v| format!("{v:.2} {}", t.currency))
                .unwrap_or("Unavailable".into())
                .into(),
        );
        let mut rows = vec![format!(
            "{} positions / {} pending orders",
            t.positions.len(),
            t.pending_orders
        )];
        rows.extend(t.positions.iter().map(|p| {
            format!(
                "#{} | {} {} | {:.2} lots | PnL {:.2} {} | SL {} | TP {}",
                p.ticket, p.symbol, p.side, p.volume, p.profit, t.currency, p.sl, p.tp
            )
        }));
        ui.set_positions(rows.join("\n\n").into());
    } else {
        ui.set_account("Waiting for a complete current MT5 account snapshot".into());
        ui.set_balance("—".into());
        ui.set_equity("—".into());
        ui.set_broker_leverage("—".into());
        ui.set_leverage_note("بانتظار حساب MT5 متصل وبيانات هامش حديثة؛ إدخال الرافعة لا يغيّر الحساب الحقيقي.".into());
        ui.set_today("—".into());
        ui.set_positions("Current positions unavailable — not assumed to be empty.".into());
    }
}
fn main() {
    if std::env::args().any(|a| a == "--render-check") {
        if let Err(e) = render::render() {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().any(|a| a == "--core") {
        if let Err(e) = braintrade_runtime::run_core() {
            eprintln!("Core error: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().any(|a| a == "--status") {
        match Client::new(&data_dir()).and_then(|c| c.get("/v1/status")) {
            Ok(v) => println!("{v}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Err(e) = run_ui() {
        rfd::MessageDialog::new()
            .set_title("BrainTrade startup error")
            .set_description(format!("{e:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}
fn run_ui() -> anyhow::Result<()> {
    let ui = Dashboard::new()?;
    backtest::setup(&ui);
    correlation::setup(&ui);
    backup::setup(&ui);
    news::setup(&ui);
    evaluation::setup(&ui);
    let _learning_timer = learning::setup(&ui);
    let _studio_timer=studio::setup(&ui);
    let _execution_timer=execution::setup(&ui);
    let _student_timer=student::setup(&ui);
    let _trade_journal_timer=trade_journal::setup(&ui);
    let _brain_monitor_timer=brain_monitor::setup(&ui);
    let (tx, rx) = mpsc::channel::<Task>();
    let sender = tx.clone();
    let weak = ui.as_weak();
    ui.on_action(move |a| {
        let Some(ui) = weak.upgrade() else { return; };
        let action=a.to_string();
        let body=if action=="start" {
            let login=ui.get_live_start_login().trim().to_owned();
            ui.set_live_start_login("".into());
            json!({"confirm_live_login":login})
        }else{json!({})};
        ui.set_busy(true);
        let _ = sender.send(Task::Action(action,body));
    });
    let sender = tx.clone();
    let weak = ui.as_weak();
    ui.on_save_settings(move ||{if let Some(ui)=weak.upgrade(){
        ui.set_busy(true);
        let body=json!({"config":Config{schema:1,mt5_path:ui.get_mt5_path().to_string(),symbol:ui.get_symbol().to_string(),telegram_user_id:ui.get_telegram_user().to_string(),telegram_chat_id:ui.get_telegram_chat().to_string(),telegram_enabled:ui.get_telegram_enabled(),risk:RiskConfig::default(),trading:Default::default(),commission_profiles:Vec::new()},"telegram_token":ui.get_telegram_token().to_string(),"clear_telegram_token":ui.get_clear_token()});
        let _=sender.send(Task::Save(body));
    }});
    let sender = tx.clone();
    let weak = ui.as_weak();
    ui.on_save_risk(move || {
        if let Some(ui) = weak.upgrade() {
            let fields = [
                ui.get_risk_percent(),
                ui.get_daily_limit(),
                ui.get_drawdown_limit(),
                ui.get_spread_limit(),
                ui.get_max_lots(),
                ui.get_risk_commission_lot(),
                ui.get_slippage_ticks(),
                ui.get_micro_real_recovery_equity(),
                ui.get_micro_real_cap_percent(),
            ];
            match fields
                .iter()
                .map(|v| v.trim().parse::<f64>())
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(v) => {
                    let risk = RiskConfig {
                        risk_percent: v[0],
                        micro_demo_risk_opt_in: ui.get_micro_demo_risk_opt_in(),
                        micro_real_temporary: ui.get_micro_real_temporary(),
                        micro_real_recovery_equity: v[7],
                        micro_real_cap_percent: v[8],
                        daily_loss_percent: v[1],
                        daily_breaker_demo_enabled: ui.get_daily_breaker_demo_enabled(),
                        daily_breaker_real_enabled: ui.get_daily_breaker_real_enabled(),
                        max_drawdown_percent: v[2],
                        max_spread_atr: v[3],
                        max_lots: v[4],
                        commission_per_lot: v[5],
                        slippage_ticks: v[6],
                    };
                    if let Err(e) = risk.validate() {
                        ui.set_feedback(e.into());
                        return;
                    }
                    ui.set_busy(true);
                    let _ = sender.send(Task::Risk(json!(risk)));
                }
                Err(_) => ui.set_feedback("Enter numbers using a decimal point, e.g. 0.25".into()),
            }
        }
    });
    let sender = tx;
    ui.on_browse_mt5(move || {
        let _ = sender.send(Task::Browse);
    });
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let client = match braintrade_runtime::ensure_core() {
            Ok(c) => c,
            Err(e) => {
                let msg = e.to_string();
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_feedback(msg.into());
                    ui.set_state("CORE FAILED".into());
                });
                return;
            }
        };
        if let Ok(v) = client.get("/v1/config") {
            if let Ok(c) = serde_json::from_value::<Config>(v["config"].clone()) {
                let saved = v["token_saved"] == true;
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_mt5_path(c.mt5_path.into());
                    ui.set_symbol(c.symbol.into());
                    ui.set_telegram_user(c.telegram_user_id.into());
                    ui.set_telegram_chat(c.telegram_chat_id.into());
                    ui.set_telegram_enabled(c.telegram_enabled);
                    ui.set_token_saved(saved);
                    ui.set_risk_percent(c.risk.risk_percent.to_string().into());
                    ui.set_micro_demo_risk_opt_in(c.risk.micro_demo_risk_opt_in);
                    ui.set_micro_real_temporary(c.risk.micro_real_temporary);
                    ui.set_micro_real_recovery_equity(c.risk.micro_real_recovery_equity.to_string().into());
                    ui.set_micro_real_cap_percent(c.risk.micro_real_cap_percent.to_string().into());
                    ui.set_daily_limit(c.risk.daily_loss_percent.to_string().into());
                    ui.set_drawdown_limit(c.risk.max_drawdown_percent.to_string().into());
                    ui.set_spread_limit(c.risk.max_spread_atr.to_string().into());
                    ui.set_max_lots(c.risk.max_lots.to_string().into());
                    ui.set_risk_commission_lot(c.risk.commission_per_lot.to_string().into());
                    ui.set_slippage_ticks(c.risk.slippage_ticks.to_string().into());
                });
            }
        }
        loop {
            match rx.recv_timeout(Duration::from_millis(750)) {
                Ok(Task::Browse) => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("MT5 terminal", &["exe"])
                        .pick_file()
                    {
                        let p = path.to_string_lossy().into_owned();
                        let _ = weak.upgrade_in_event_loop(move |ui| ui.set_mt5_path(p.into()));
                    }
                }
                Ok(task) => {
                    let (result, saved, shutdown) = match task {
                        Task::Save(mut v) => (
                            client.get("/v1/config").and_then(|current| {
                                v["config"]["risk"] = current["config"]["risk"].clone();
                                v["config"]["trading"] = current["config"]["trading"].clone();
                                v["config"]["commission_profiles"] = current["config"]["commission_profiles"].clone();
                                client.post("/v1/config", v)
                            }),
                            true,
                            false,
                        ),
                        Task::Risk(v) => (client.post("/v1/risk", v), false, false),
                        Task::Action(a, _) if a == "decisions" => {
                            (client.get("/v1/decisions"), false, false)
                        }
                        Task::Action(a, _) if a == "events" => {
                            (client.get("/v1/events"), false, false)
                        }
                        Task::Action(a, body) => (
                            client.post(&format!("/v1/{a}"), body),
                            false,
                            a == "shutdown",
                        ),
                        Task::Browse => unreachable!(),
                    };
                    let ok = result.is_ok();
                    let token_saved = if saved && ok {
                        client
                            .get("/v1/config")
                            .ok()
                            .map(|v| v["token_saved"] == true)
                    } else {
                        None
                    };
                    let (message, events) = match result {
                        Ok(v) if v.get("decisions").is_some() => {
                            let lines = v["decisions"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|d| {
                                    let a = &d["decision"]["analysis"];
                                    format!(
                                        "#{} | {} | {} | bar {}\n{}\nGates: {}",
                                        d["id"],
                                        d["context"],
                                        a["action"],
                                        d["bar_time"],
                                        a["reason"],
                                        a["gates"]
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n\n");
                            ("Decision history refreshed".into(), Some(lines))
                        }
                        Ok(v) if v.get("events").is_some() => {
                            let lines = v["events"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|e| {
                                    format!(
                                        "#{} | {} | {}\n{}",
                                        e["id"],
                                        e["timestamp_ms"],
                                        e["kind"].as_str().unwrap_or(""),
                                        e["detail"]
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n\n");
                            ("Audit log refreshed".to_string(), Some(lines))
                        }
                        Ok(v) if v.get("path").is_some() => (
                            format!(
                                "{}\n{}",
                                v["path"].as_str().unwrap_or(""),
                                v["note"].as_str().unwrap_or("Saved")
                            ),
                            None,
                        ),
                        Ok(_) => (
                            if saved {
                                "Settings saved; reconnecting"
                            } else {
                                "Command completed"
                            }
                            .to_string(),
                            None,
                        ),
                        Err(e) => (e.to_string(), None),
                    };
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_busy(false);
                        ui.set_feedback(message.into());
                        if let Some(rows) = events {
                            ui.set_events(rows.into());
                        }
                        if saved && ok {
                            ui.set_telegram_token("".into());
                            ui.set_clear_token(false);
                            if let Some(v) = token_saved {
                                ui.set_token_saved(v);
                            }
                        }
                        if shutdown && ok {
                            let _ = slint::quit_event_loop();
                        }
                    });
                    if shutdown && ok {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            match client
                .get("/v1/status")
                .and_then(|v| Ok(serde_json::from_value::<Status>(v)?))
            {
                Ok(s) => {
                    let _ = weak.upgrade_in_event_loop(move |ui| paint(&ui, s));
                }
                Err(_) => {
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_online(false);
                        ui.set_state("CORE UNREACHABLE".into());
                        ui.set_connection(
                            "Lost core connection. Reopen the application to reconnect.".into(),
                        );
                        ui.set_balance("—".into());
                        ui.set_equity("—".into());
                        ui.set_today("—".into());
                        ui.set_positions("Live data unavailable".into());
                        ui.set_brains("Core unavailable — analysis and opportunity plans withdrawn".into());
                        ui.set_opportunities("المحرك غير متصل؛ أرقام الفرص مسحوبة".into());
                        ui.set_market_map("المحرك غير متصل؛ الخريطة مسحوبة".into());
                        ui.set_regime("UNAVAILABLE".into());
                        ui.set_decision("WAIT".into());
                        ui.set_paper("Core unavailable — current virtual equity unknown".into());
                    });
                }
            }
        }
    });
    ui.run()?;
    Ok(())
}
