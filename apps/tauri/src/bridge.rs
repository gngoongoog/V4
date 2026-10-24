//! V4 engine remains authoritative. The browser NEVER reads ipc.key, obtains
//! a broker token or connects directly to the Core's HTTP port.
use braintrade_runtime::{ensure_core, Client};
use serde_json::{json, Value};
use std::{thread, time::{Duration, Instant}, sync::{Arc,atomic::{AtomicBool,Ordering}}};
use tauri::ipc::Channel;

struct AppLifecycle { stopping: Arc<AtomicBool> }

fn open_client() -> Result<Client, String> {
    ensure_core().map_err(|err| err.to_string())
}

#[tauri::command]
fn get_status() -> Result<Value, String> {
    open_client()?.get("/v1/status").map_err(|err| err.to_string())
}

// Paths can only be chosen from a fixed read-only set. No arbitrary URL,
// browser proxy, secret file, or POST to gateway endpoints can be requested.
fn allowed_read_route(route: &str) -> bool {
    matches!(route,
        "/v1/config" | "/v1/learning" | "/v1/execution/brains" | "/v1/decisions" |
        "/v1/opportunities/history" | "/v1/events" | "/v1/risk/monitor" |
        "/v1/trading/settings" | "/v1/studio" | "/v1/news" |
        "/v1/evaluation" | "/v1/backtest" | "/v1/backups" |
        "/v1/execution" | "/v1/execution/trades" | "/v1/execution/quality" | "/v1/execution/mode-evidence" | "/v1/execution/frozen-coordinator" |
        "/v1/research/correlation" | "/v1/enterprise/supervisor"
    )
}

#[tauri::command]
fn read_panel(route: String) -> Result<Value, String> {
    if !allowed_read_route(&route) { return Err("غير مسموح بهذا المسار".into()); }
    open_client()?.get(&route).map_err(|err| err.to_string())
}

#[tauri::command]
fn execute_control(action: String, confirm_live_login: Option<String>) -> Result<Value, String> {
    let (route, body) = match action.as_str() {
        "start" => ("/v1/start", json!({"confirm_live_login":confirm_live_login.unwrap_or_default()})),
        "stop" => ("/v1/stop", json!({})),
        "reconnect" => ("/v1/reconnect", json!({})),
        _ => return Err("الأمر غير مسموح".into()),
    };
    open_client()?.post(route, body).map_err(|err| err.to_string())
}

#[tauri::command]
fn shutdown_core(lifecycle: tauri::State<AppLifecycle>) -> Result<Value, String> {
    // Core checks STOPPED, broker freshness, empty positions and resolved intents.
    // A rejected shutdown must never exit the terminal or kill a gateway.
    let result=open_client()?.post("/v1/shutdown", json!({})).map_err(|err|err.to_string())?;
    // Prevent the live-status worker from automatically respawning Core while
    // the shutdown response is handed back to the renderer.
    lifecycle.stopping.store(true,Ordering::SeqCst);
    Ok(result)
}

#[tauri::command]
fn close_terminal(app: tauri::AppHandle) {
    // Leave Core working when the operator only closes the presentation window.
    app.exit(0);
}

#[tauri::command]
fn backup_action(action: String) -> Result<Value, String> {
    let route=match action.as_str(){
        "create"=>"/v1/backup",
        "brain_export"=>"/v1/brain-snapshot/export",
        _=>return Err("هذا الإجراء الاحتياطي غير مسموح".into()),
    };
    open_client()?.post(route,json!({})).map_err(|err|err.to_string())
}

#[tauri::command]
fn backtest_action(action: String, request: Option<Value>) -> Result<Value, String> {
    let (route,body)=match action.as_str(){
        "start"=>{
            let input=request.ok_or("تفاصيل الاختبار التاريخي غير متاحة")?;
            let allowed=["from_date","to_date","initial_equity","spread_floor"];
            if input.as_object().is_none_or(|m|m.keys().any(|k| !allowed.contains(&k.as_str()))) {
                return Err("حقول الاختبار التاريخي غير صالحة".into());
            }
            ("/v1/backtest/start",input)
        },
        "cancel"=>("/v1/backtest/cancel",json!({})),
        _=>return Err("هذا الإجراء التاريخي غير مسموح".into()),
    };
    open_client()?.post(route,body).map_err(|err|err.to_string())
}

#[tauri::command]
fn bind_execution(target: String, deviation_points: u32, confirm_live_login: String) -> Result<Value,String> {
    if !matches!(target.as_str(),"demo" | "live") || deviation_points > 100_000 {
        return Err("Invalid broker execution target or deviation".into());
    }
    // Rust Core validates account_trade_mode, broker freshness, exact REAL
    // login confirmation, stopped state, and unresolved broker commands.
    open_client()?.post("/v1/execution/settings",
        json!({"target":target,"deviation_points":deviation_points,
               "confirm_live_login":confirm_live_login})).map_err(|err|err.to_string())
}

#[tauri::command]
fn update_brain_control(context: String, brain: String, mode: String, advisor: bool, paper: bool) -> Result<Value,String> {
    let supported=["scalping","swing","expert","sniper"];
    if !supported.contains(&mode.as_str()) || brain == "system" || brain.len() > 100 || context.len() > 500 {
        return Err("هوية العقل أو وضع التشغيل غير مسموح".into());
    }
    // Rust Core verifies the exact broker context, mode, stopped state,
    // outstanding intents and advisor ID. Never enable secondary live trading.
    open_client()?.post("/v1/execution/brain-control",
        json!({"context":context,"brain":brain,"mode":mode,
               "control":{"advisor":advisor,"paper":paper}}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn frozen_coordinator_action(action: String, context: String, mode: String) -> Result<Value, String> {
    // Research-only. The Rust Core independently requires STOPPED, no
    // pending broker intents and a fresh exact-bound MT5 DEMO Expert context.
    if !matches!(action.as_str(),"freeze"|"stop")
        || !matches!(mode.as_str(),"expert"|"scalping"|"swing")
        || context.len()>500 || context.trim().is_empty() {
        return Err("Frozen coordinator action or account context invalid".into());
    }
    open_client()?.post("/v1/execution/frozen-coordinator",
        json!({"action":action,"context":context,"mode":mode}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn update_supervisor(settings: Value) -> Result<Value, String> {
    // Rust Core performs both validation and STOPPED/flat/settled enforcement.
    open_client()?.post("/v1/enterprise/supervisor", settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn update_risk(risk: Value) -> Result<Value, String> {
    // Core enforces all risk constraints, execution state, and audit log.
    open_client()?.post("/v1/risk", risk).map_err(|err| err.to_string())
}

#[tauri::command]
fn update_portfolio(settings: Value) -> Result<Value, String> {
    // This command accepts exactly the Core's portfolio policy shape.
    // It never starts the engine; Core requires STOPPED and no unresolved intents.
    let data=settings.as_object().ok_or("Portfolio settings must be an object")?;
    if data.len()!=4 || !["enabled","max_positions","max_lots","max_risk_percent"]
        .iter().all(|field|data.contains_key(*field)) {
        return Err("Portfolio settings fields are invalid".into());
    }
    if !settings["enabled"].is_boolean() ||
        !settings["max_positions"].as_u64().is_some_and(|n|(1..=100).contains(&n)) ||
        !settings["max_lots"].as_f64().is_some_and(|n|n.is_finite()&&(0.001..=10000.0).contains(&n)) ||
        !settings["max_risk_percent"].as_f64().is_some_and(|n|n.is_finite()&&(0.01..=100.0).contains(&n)) {
        return Err("Portfolio settings outside valid bounds".into());
    }
    open_client()?.post("/v1/execution/portfolio", settings).map_err(|err|err.to_string())
}

#[tauri::command]
fn update_settings(config: Value, telegram_token: String, clear_telegram_token: bool) -> Result<Value, String> {
    // Never expose the saved token. Core validates account and secret changes.
    open_client()?.post("/v1/config", json!({
        "config":config,"telegram_token":telegram_token,
        "clear_telegram_token":clear_telegram_token
    })).map_err(|err| err.to_string())
}

#[tauri::command]
fn update_trading_settings(settings: Value) -> Result<Value, String> {
    open_client()?.post("/v1/trading/settings", settings).map_err(|err| err.to_string())
}

#[tauri::command]
fn priority_handoff(sniper_active: bool) -> Result<Value, String> {
    open_client()?.post("/v1/trading/priority/handoff", json!({"sniper_active":sniper_active}))
        .map_err(|err| err.to_string())
}

/// A channel carries the most recent MT5/Core status, not an untrusted browser poll.
/// Poll interval bounds bridge refresh to ~250 ms; true end-to-end MT5 freshness
/// depends on when the gateway actually supplies a completed snapshot.
/// On disconnect, transmit unavailability instead of retaining stale metrics.
#[tauri::command]
fn subscribe_status(channel: Channel<Value>, lifecycle: tauri::State<AppLifecycle>) -> Result<(), String> {
    let stopping=Arc::clone(&lifecycle.stopping);
    thread::Builder::new().name("mt5-status-stream".into()).spawn(move || {
        let mut last_payload = String::new();
        let mut client: Option<Client> = None;
        let mut failures: u32 = 0;
        let mut last_send: Option<Instant> = None;
        loop {
            if stopping.load(Ordering::SeqCst) { break; }
            if client.is_none() { client = ensure_core().ok(); }
            if stopping.load(Ordering::SeqCst) { break; }
            let payload = client.as_ref().and_then(|c| c.get("/v1/status").ok());
            let next = match payload {
                Some(status) => { failures = 0; json!({"ok":true,"status":status}) },
                None => {
                    failures = failures.saturating_add(1);
                    client = None;
                    json!({"ok":false,"error":"MT5/Core غير متاح","status":null})
                }
            };
            // A deduplicated channel still needs a liveness heartbeat. Without
            // this, a perfectly healthy, idle Core is falsely marked offline
            // by the client after five seconds. Heartbeat is never a broker
            // quote and must not reset MarketData freshness.
            let encoded = next.to_string();
            if encoded != last_payload || last_send.is_none_or(|last| last.elapsed() >= Duration::from_secs(2)) {
                if channel.send(next).is_err() { break; }
                last_payload = encoded;
                last_send = Some(Instant::now());
            }
            // Failure loops are throttled; live feed responds within one cycle.
            thread::sleep(Duration::from_millis(if failures > 0 { 900 } else { 250 }));
        }
    }).map_err(|err|err.to_string())?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppLifecycle {stopping:Arc::new(AtomicBool::new(false))})
        .invoke_handler(tauri::generate_handler![
            get_status, read_panel, execute_control,
            update_risk, update_portfolio, update_supervisor, frozen_coordinator_action, update_settings, update_trading_settings,
            priority_handoff, subscribe_status, shutdown_core, close_terminal,
            backup_action, backtest_action, update_brain_control, bind_execution
        ])
        .run(tauri::generate_context!())
        .expect("could not start BRAINTRADE Tauri terminal");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_routes_are_strictly_scoped() {
        assert!(allowed_read_route("/v1/risk/monitor"));
        assert!(allowed_read_route("/v1/execution/brains"));
        assert!(allowed_read_route("/v1/enterprise/supervisor"));
        assert!(!allowed_read_route("/v1/gateway/snapshot"));
        assert!(!allowed_read_route("http://attacker.local"));
        assert!(!allowed_read_route("/v1/config/../../ipc.key"));
        assert!(!allowed_read_route("/v1/status?token=x"));
    }
}
