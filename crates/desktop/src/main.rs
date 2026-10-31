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
mod risk_monitor;

enum Task {
    Action(String, Value),
    Save(Value),
    Risk(Value),
    Browse,
}

// Plot only MT5-sourced, valid, completed OHLC bars. Values are normalized
// against the plotted series; these are not order markers or simulated fills.
fn native_chart_candles(market: &braintrade_domain::MarketData, frame: i32) -> Vec<ChartCandle> {
    if !market.valid() { return Vec::new(); }
    let candles = match frame {
        0 => &market.m1,
        1 => &market.m5,
        2 => &market.m15,
        _ => return Vec::new(),
    };
    let subset = &candles[candles.len().saturating_sub(70)..];
    if subset.len() < 3 { return Vec::new(); }
    let low = subset.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let high = subset.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
    let extent = high - low;
    if !extent.is_finite() || extent <= 0.0 || !low.is_finite() || !high.is_finite() {
        return Vec::new();
    }
    let padding = extent * 0.05;
    let lower = low - padding;
    let upper = high + padding;
    let denom = upper - lower;
    let at = |price: f64| ((upper - price) / denom).clamp(0.0, 1.0) as f32;
    let max_tick_volume = subset.iter().map(|c| c.tick_volume).max().unwrap_or(0);
    subset.iter().enumerate().map(|(i, candle)| ChartCandle {
        x: (i as f32 + 0.5) / subset.len() as f32,
        high_y: at(candle.high),
        low_y: at(candle.low),
        body_top: at(candle.open.max(candle.close)),
        body_bottom: at(candle.open.min(candle.close)),
        up: candle.close >= candle.open,
        volume: if max_tick_volume == 0 { 0.0 } else {
            (candle.tick_volume as f32 / max_tick_volume as f32).clamp(0.0, 1.0)
        },
    }).collect()
}

fn paint(ui: &Dashboard, status: Status, bound: &brain_monitor::BoundAccountScope) {
    let current=brain_monitor::scope_for_status(&status);
    let changed={
        let mut previous=bound.lock().unwrap();
        let changed=*previous!=current;
        *previous=current;
        changed
    };
    if changed {
        // The independent Brain Monitor worker may still hold a response
        // from the former account. Remove it as soon as primary status changes.
        brain_monitor::invalidate_account_view(ui);
    }
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
        ui.set_account_mode_label(if t.demo { "DEMO".into() }
            else if t.account_trade_mode == Some(2) { "REAL".into() }
            else { "OTHER".into() });
        ui.set_free_margin(t.margin_free.map(|v| format!("{v:.2} {}", t.currency))
            .unwrap_or_else(|| "—".into()).into());
        ui.set_active_positions(t.positions.len().to_string().into());
        ui.set_bid_display(t.bid.filter(|v| v.is_finite() && *v > 0.)
            .map(|v| format!("{v:.5}")).unwrap_or_else(|| "—".into()).into());
        ui.set_ask_display(t.ask.filter(|v| v.is_finite() && *v > 0.)
            .map(|v| format!("{v:.5}")).unwrap_or_else(|| "—".into()).into());
        ui.set_spread_display(t.bid.zip(t.ask)
            .filter(|(bid,ask)| bid.is_finite() && ask.is_finite() && *bid > 0. && *ask >= *bid)
            .map(|(bid,ask)| format!("{:.5} price", ask-bid))
            .unwrap_or_else(|| "—".into()).into());
        ui.set_quote_age_display(t.tick_age_ms.map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| "—".into()).into());
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map(|v| v.as_millis() as u64).unwrap_or(0);
        let bars = t.market.as_ref()
            .filter(|market| market.captured_ms > 0
                && now_ms >= market.captured_ms
                && now_ms.saturating_sub(market.captured_ms) <= 120_000)
            .map(|market| native_chart_candles(market, ui.get_chart_frame()))
            .unwrap_or_default();
        ui.set_chart_bars(slint::ModelRc::new(slint::VecModel::from(bars)));
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
        ui.set_account_mode_label("NOT CONNECTED".into());
        ui.set_free_margin("—".into());
        ui.set_active_positions("—".into());
        ui.set_bid_display("—".into());
        ui.set_ask_display("—".into());
        ui.set_spread_display("—".into());
        ui.set_quote_age_display("—".into());
        ui.set_chart_bars(slint::ModelRc::new(slint::VecModel::<ChartCandle>::default()));
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
    let startup=braintrade_runtime::prepare_user_start().and_then(|_|run_ui());
    if let Err(e) = startup {
        rfd::MessageDialog::new()
            .set_title("BrainTrade startup error")
            .set_description(format!("{e:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}
/// Appearance preferences are local UI-only state. No broker/risk/core config is changed.
fn parse_appearance(raw:&str)->(i32,bool,bool){
    let Ok(v)=serde_json::from_str::<Value>(raw) else{return (2,false,false)};
    let theme=v["theme"].as_i64().filter(|v|(0..=2).contains(v))
        .map(|v|v as i32).unwrap_or(2);
    (theme,
        v["sidebar_collapsed"].as_bool().unwrap_or(false),
        v["expanded_workspaces"].as_bool().unwrap_or(false))
}

fn appearance_path()->std::path::PathBuf{
    data_dir().join("ui-appearance.json")
}
fn load_appearance()->(i32,bool,bool){
    std::fs::read_to_string(appearance_path())
        .ok().map(|s|parse_appearance(&s)).unwrap_or((2,false,false))
}
fn save_appearance(theme:i32,collapsed:bool,expanded:bool)->anyhow::Result<()>{
    anyhow::ensure!((0..=2).contains(&theme),"Unsupported desktop theme");
    let file=appearance_path();
    if let Some(parent)=file.parent(){std::fs::create_dir_all(parent)?;}
    std::fs::write(file,serde_json::to_vec(&json!({
        "schema":1,"theme":theme,"sidebar_collapsed":collapsed,
        "expanded_workspaces":expanded,
    }))?)?;
    Ok(())
}
#[cfg(test)]
mod appearance_tests{
    use super::*;
    #[test]
    fn no_invalid_theme_may_be_restored(){
        assert_eq!(parse_appearance("{\"theme\":1,\"sidebar_collapsed\":true,\"expanded_workspaces\":true}"),(1,true,true));
        assert_eq!(parse_appearance("{\"theme\":2}"),(2,false,false));
        assert_eq!(parse_appearance("{\"theme\":-99,\"sidebar_collapsed\":true}"),(2,true,false));
        assert_eq!(parse_appearance("{\"theme\":100,\"expanded_workspaces\":true}"),(2,false,true));
        assert_eq!(parse_appearance("{\"theme\":1,\"expanded_workspaces\":\"yes\"}"),(1,false,false));
        assert_eq!(parse_appearance("not json"),(2,false,false));
    }
}

fn run_ui() -> anyhow::Result<()> {
    let ui = Dashboard::new()?;
    let (initial_theme,initial_compact,initial_expanded)=load_appearance();
    ui.set_theme_choice(initial_theme);
    ui.set_compact_sidebar(initial_compact);
    ui.set_expanded_workspaces(initial_expanded);
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
    let bound_account:brain_monitor::BoundAccountScope=std::sync::Arc::new(
        std::sync::Mutex::new(None));
    let _brain_monitor_timer=brain_monitor::setup(&ui,bound_account.clone());
    let _risk_monitor_timer=risk_monitor::setup(&ui);
    let weak = ui.as_weak();
    ui.on_save_appearance(move |theme, collapsed, expanded| {
        if let Err(err)=save_appearance(theme,collapsed,expanded){
            if let Some(ui)=weak.upgrade(){
                ui.set_feedback(format!("تعذر حفظ ثيم الواجهة محلياً: {err}").into());
            }
        }
    });
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
                    ui.set_daily_breaker_demo_enabled(c.risk.daily_breaker_demo_enabled);
                    ui.set_daily_breaker_real_enabled(c.risk.daily_breaker_real_enabled);
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
                    let current_scope=bound_account.clone();
                    let _ = weak.upgrade_in_event_loop(move |ui| paint(&ui, s, &current_scope));
                }
                Err(_) => {
                    let current_scope=bound_account.clone();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        *current_scope.lock().unwrap()=None;
                        brain_monitor::invalidate_account_view(&ui);
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

#[cfg(test)]
mod institutional_dashboard_tests {
    use super::*;
    #[test]
    fn real_ohlc_chart_requires_at_least_three_complete_candles() {
        let mut m = braintrade_domain::MarketData::default();
        assert!(native_chart_candles(&m, 0).is_empty());
        for i in 0..4 {
            m.m1.push(braintrade_domain::Candle {
                time: 1_800_000_000 + i * 60,
                open: 5000.0 + i as f64, high: 5002.0 + i as f64,
                low: 4999.0 + i as f64, close: 5001.0 + i as f64,
                tick_volume: 30,
            });
        }
        let view = native_chart_candles(&m, 0);
        assert_eq!(view.len(), 4);
        assert!(view.iter().all(|c| c.high_y <= c.body_top
            && c.body_top <= c.body_bottom && c.body_bottom <= c.low_y));
        assert!(view.iter().all(|c| (c.volume - 1.0).abs() < f32::EPSILON));
        m.m1[0].tick_volume = 0;
        m.m1[1].tick_volume = 10;
        m.m1[2].tick_volume = 30;
        m.m1[3].tick_volume = 15;
        let updated = native_chart_candles(&m, 0);
        assert_eq!(updated[0].volume, 0.0);
        assert!((updated[1].volume - (1.0 / 3.0)).abs() < 0.0001);
        assert_eq!(updated[2].volume, 1.0);
        assert_eq!(updated[3].volume, 0.5);
        for candle in &mut m.m1 { candle.tick_volume = 0; }
        assert!(native_chart_candles(&m, 0).iter().all(|c| c.volume == 0.0));
        assert!(native_chart_candles(&m, 2).is_empty());
    }
}
