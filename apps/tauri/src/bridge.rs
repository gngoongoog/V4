//! V4 engine remains authoritative. The browser NEVER reads ipc.key, obtains
//! a broker token or connects directly to the Core's HTTP port.
use braintrade_runtime::{connect_existing_core, ensure_core, Client, ShutdownError};
use serde_json::{json, Value};
use std::{thread, time::{Duration, Instant}, sync::{Arc,Mutex,OnceLock,atomic::{AtomicBool,Ordering}}};
use tauri::ipc::Channel;

#[derive(Clone,Default)]
struct AppLifecycle { stopping: Arc<AtomicBool>, start_gate: Arc<Mutex<()>> }
static LIFECYCLE: OnceLock<AppLifecycle> = OnceLock::new();

fn lifecycle() -> &'static AppLifecycle { LIFECYCLE.get_or_init(AppLifecycle::default) }

fn with_start_permit<T>(state:&AppLifecycle, operation:impl FnOnce()->Result<T,String>)->Result<T,String>{
    let _guard=state.start_gate.lock().map_err(|_|"تعذر قفل حالة بدء Core".to_string())?;
    if state.stopping.load(Ordering::SeqCst) { return Err("طلب الخروج قيد التحقق؛ لن يُعاد تشغيل Core تلقائياً".into()); }
    operation()
}

fn open_client() -> Result<Client, String> {
    with_start_permit(lifecycle(),||ensure_core().map_err(|err| err.to_string()))
}

// Keep a Windows process HANDLE to the exact Core instance. A PID alone can
// be recycled, and a closed HTTP port or released lock does not prove exit.
#[cfg(windows)]
mod tracked_core {
    use std::{ffi::c_void, io, ptr};
    #[link(name="kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access:u32,inherit:i32,pid:u32)->*mut c_void;
        fn WaitForSingleObject(handle:*mut c_void,millis:u32)->u32;
        fn CloseHandle(handle:*mut c_void)->i32;
    }
    pub struct Handle(*mut c_void);
    impl Handle {
        pub fn open(pid:u32)->Result<Self,String>{
            let handle=unsafe{OpenProcess(0x0010_0000,0,pid)}; // SYNCHRONIZE
            if handle==ptr::null_mut(){return Err(format!("تعذر فتح مقبض عملية Core: {}",io::Error::last_os_error()));}
            Ok(Self(handle))
        }
        fn wait_for(&self,millis:u32)->Result<(),String>{
            match unsafe{WaitForSingleObject(self.0,millis)} {
                0=>Ok(()),
                258=>Err("انتهت مهلة انتظار عملية Core؛ لم يُؤكد الخروج".into()),
                _=>Err(format!("تعذر التحقق من عملية Core: {}",io::Error::last_os_error())),
            }
        }
        pub fn wait(&self)->Result<(),String>{self.wait_for(15_000)}
        #[cfg(test)] pub fn is_running(&self)->bool{(unsafe{WaitForSingleObject(self.0,0)})==258}
    }
    impl Drop for Handle { fn drop(&mut self){unsafe{CloseHandle(self.0);}} }
}

#[cfg(not(windows))]
mod tracked_core {
    pub struct Handle;
    impl Handle {
        pub fn open(_:u32)->Result<Self,String>{Err("التحقق من عملية الخروج الكامل متاح على Windows فقط".into())}
        pub fn wait(&self)->Result<(),String>{Err("التحقق من عملية الخروج الكامل متاح على Windows فقط".into())}
    }
}

fn process_identity(status:&Value)->Result<(u32,String),String>{
    let process=&status["core_process"];
    let pid=process["pid"].as_u64().and_then(|v|u32::try_from(v).ok()).filter(|v|*v>0)
        .ok_or("تعذر تحديد عملية Core الحالية")?;
    let instance=process["instance_id"].as_str().filter(|v|!v.is_empty())
        .ok_or("تعذر تحديد جلسة Core الحالية")?.to_string();
    Ok((pid,instance))
}

fn confirm_full_exit(state:&AppLifecycle)->Result<Value,String>{
    let _guard=state.start_gate.lock().map_err(|_|"تعذر قفل حالة بدء Core".to_string())?;
    if state.stopping.load(Ordering::SeqCst) {return Err("طلب خروج سابق غير محسوم؛ تحقق من العمليات قبل إعادة فتح التطبيق".into());}
    let client=connect_existing_core().map_err(|e|format!("تعذر الوصول إلى Core الحالي: {e}"))?;
    let first=client.get("/v1/status").map_err(|e|e.to_string())?;
    let (pid,instance)=process_identity(&first)?;
    let process=tracked_core::Handle::open(pid)?;
    let second=client.get("/v1/status").map_err(|e|e.to_string())?;
    if process_identity(&second)?!=(pid,instance.clone()) {return Err("تغيرت جلسة Core؛ أعد طلب الخروج".into());}
    // All auto-start paths acquire this same gate. Set the flag before the
    // request, not after the acknowledgement, to close the respawn race.
    state.stopping.store(true,Ordering::SeqCst);
    let result=match client.shutdown(&instance){
        Ok(value)=>value,
        Err(ShutdownError::Rejected(reason))=>{
            state.stopping.store(false,Ordering::SeqCst);
            return Err(reason);
        }
        Err(ShutdownError::Uncertain(reason))=>return Err(format!("تعذر تأكيد نتيجة طلب الإغلاق؛ لن نعيد تشغيل Core تلقائياً: {reason}")),
    };
    if result["ok"]!=true || result["gateway_stopped"]!=true || process_identity(&result)?!=(pid,instance) {
        return Err("رد الإغلاق لا يثبت إيقاف Gateway أو هوية Core؛ لم يُؤكد الخروج".into());
    }
    process.wait()?;
    Ok(result)
}

#[tauri::command]
fn get_status() -> Result<Value, String> {
    open_client()?.get("/v1/status").map_err(|err| err.to_string())
}

// Paths can only be chosen from a fixed read-only set. No arbitrary URL,
// browser proxy, secret file, or POST to gateway endpoints can be requested.
fn allowed_read_route(route: &str) -> bool {
    matches!(route,
        "/v1/config" | "/v1/learning" | "/v1/learning/account-evidence" | "/v1/execution/brains" | "/v1/decisions" |
        "/v1/opportunities/history" | "/v1/events" | "/v1/risk/monitor" |
        "/v1/trading/settings" | "/v1/studio" | "/v1/studio/chart-frames" | "/v1/news" |
        "/v1/evaluation" | "/v1/backtest" | "/v1/backups" |
        "/v1/execution" | "/v1/execution/trades" | "/v1/execution/quality" | "/v1/execution/reconciliation" | "/v1/execution/sniper-diagnosis" | "/v1/execution/sniper-tick-audit" | "/v1/execution/frozen-sniper" | "/v1/execution/frozen-classic" | "/v1/execution/mode-evidence" | "/v1/execution/frozen-coordinator" |
        "/v1/research/correlation" | "/v1/enterprise/supervisor"
    )
}

#[tauri::command]
fn read_panel(route: String) -> Result<Value, String> {
    if !allowed_read_route(&route) { return Err("غير مسموح بهذا المسار".into()); }
    open_client()?.get(&route).map_err(|err| err.to_string())
}

/// Pagination is read-only, even though Core exposes this specific query as
/// POST. The renderer cannot choose a destination, method or arbitrary body.
fn valid_trade_page(offset:u32,limit:u32)->bool{
    offset<=1_000_000 && (1..=100).contains(&limit)
}

#[tauri::command]
fn read_trade_history(offset: u32, limit: u32) -> Result<Value, String> {
    if !valid_trade_page(offset,limit) {
        return Err("حدود تصفح سجل التداول غير صالحة".into());
    }
    open_client()?.post("/v1/execution/trades",json!({"offset":offset,"limit":limit}))
        .map_err(|err|err.to_string())
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
async fn shutdown_core(lifecycle: tauri::State<'_,AppLifecycle>) -> Result<Value, String> {
    let state=lifecycle.inner().clone();
    tauri::async_runtime::spawn_blocking(move || confirm_full_exit(&state)).await
        .map_err(|e|format!("تعذر إكمال تحقق الخروج: {e}"))?
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
fn frozen_sniper_action(action:String,context:String)->Result<Value,String>{
    // Research-only, Sniper Learner. Rust Core must independently verify
    // STOP, reconciled broker intents, fresh Demo and exact account identity.
    if !matches!(action.as_str(),"freeze"|"stop")
        ||context.trim().is_empty()||context.len()>500 {
        return Err("Invalid frozen Sniper research request".into());
    }
    open_client()?.post("/v1/execution/frozen-sniper",
        json!({"action":action,"context":context}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn frozen_classic_action(action:String,context:String)->Result<Value,String>{
    // Research-only; all final STOP/Demo/account checks live in Rust Core.
    if !matches!(action.as_str(),"freeze"|"stop")
        ||context.trim().is_empty()||context.len()>500 {
        return Err("Invalid frozen Classic research request".into());
    }
    open_client()?.post("/v1/execution/frozen-classic",
        json!({"action":action,"context":context}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn frozen_coordinator_action(action: String, context: String, mode: String) -> Result<Value, String> {
    // Research-only. The Rust Core independently requires STOPPED, no
    // pending broker intents and a fresh exact-bound MT5 DEMO Expert context.
    if !matches!(action.as_str(),"freeze"|"seal_candidate"|"stop")
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
            if stopping.load(Ordering::SeqCst) { thread::sleep(Duration::from_millis(250)); continue; }
            if client.is_none() { client = open_client().ok(); }
            if stopping.load(Ordering::SeqCst) { continue; }
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
    // Only a freshly launched UI clears a completed exit marker. Existing
    // windows and background status workers must never clear it.
    if let Err(error)=braintrade_runtime::prepare_user_start(){eprintln!("Could not prepare Core startup: {error}");}
    tauri::Builder::default()
        .manage(lifecycle().clone())
        .invoke_handler(tauri::generate_handler![
            get_status, read_panel, read_trade_history, execute_control,
            update_risk, update_portfolio, update_supervisor, frozen_coordinator_action, frozen_sniper_action, frozen_classic_action, update_settings, update_trading_settings,
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
    fn full_exit_requires_a_valid_core_process_identity() {
        assert_eq!(process_identity(&json!({"core_process":{"pid":42,"instance_id":"session-a"}})).unwrap(),(42,"session-a".to_string()));
        assert!(process_identity(&json!({"core_process":{"pid":0,"instance_id":"session-a"}})).is_err());
        assert!(process_identity(&json!({"core_process":{"pid":42,"instance_id":""}})).is_err());
        assert!(process_identity(&json!({})).is_err());
    }
    #[test]
    fn shutdown_gate_blocks_a_concurrent_automatic_start() {
        use std::sync::mpsc;
        let state=AppLifecycle::default();
        let guard=state.start_gate.lock().unwrap();
        state.stopping.store(true,Ordering::SeqCst);
        let child=state.clone();
        let (tx,rx)=mpsc::channel();
        let worker=std::thread::spawn(move ||{
            let result=with_start_permit(&child,||{tx.send(()).unwrap();Ok(())});
            assert!(result.is_err());
        });
        drop(guard);
        worker.join().unwrap();
        assert!(rx.try_recv().is_err(),"start was called during shutdown");
        state.stopping.store(false,Ordering::SeqCst);
        assert_eq!(with_start_permit(&state,||Ok(7)).unwrap(),7);
    }
    #[cfg(windows)]
    #[test]
    fn tracked_core_handle_waits_for_a_real_process_exit() {
        use std::process::{Command,Stdio};
        let mut child=Command::new("cmd.exe").args(["/C","pause >NUL"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        let handle=tracked_core::Handle::open(child.id()).unwrap();
        assert!(handle.is_running(),"Core-like process exited before its pipe was closed");
        drop(child.stdin.take());
        handle.wait().unwrap();
        assert!(child.wait().unwrap().success());
    }
    #[cfg(windows)]
    #[test]
    fn tauri_full_exit_waits_for_a_real_isolated_core() {
        use std::{fs,process::{Child,Command,Stdio},time::{SystemTime,UNIX_EPOCH}};
        struct Fixture { dir:std::path::PathBuf, child:Option<Child>, old:Vec<(&'static str,Option<std::ffi::OsString>)> }
        impl Drop for Fixture {
            fn drop(&mut self){
                if let Some(child)=self.child.as_mut(){if child.try_wait().ok().flatten().is_none(){let _=child.kill();let _=child.wait();}}
                for (key,value) in &self.old {unsafe{match value {Some(v)=>std::env::set_var(key,v),None=>std::env::remove_var(key)}}}
                let _=fs::remove_dir_all(&self.dir);
            }
        }
        let suffix=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir=std::env::temp_dir().join(format!("braintrade-tauri-exit-{}-{suffix}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let keys=["BRAINTRADE_DATA_DIR","BRAINTRADE_GATEWAY_DISABLED","BRAINTRADE_NEWS_OFFLINE"];
        let mut fixture=Fixture{dir:dir.clone(),child:None,old:keys.iter().map(|k|(*k,std::env::var_os(k))).collect()};
        unsafe{std::env::set_var(keys[0],&dir);std::env::set_var(keys[1],"1");std::env::set_var(keys[2],"1");}
        let binary=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/braintrade-core.exe");
        fixture.child=Some(Command::new(binary).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
        let pid=fixture.child.as_ref().unwrap().id();
        let deadline=Instant::now()+Duration::from_secs(20);
        loop {
            if let Ok(client)=connect_existing_core(){
                if let Ok(status)=client.get("/v1/status"){
                    if process_identity(&status).is_ok_and(|(observed,_)|observed==pid){break;}
                }
            }
            assert!(Instant::now()<deadline,"isolated Core did not become ready");
            thread::sleep(Duration::from_millis(100));
        }
        let state=AppLifecycle::default();
        let response=confirm_full_exit(&state).unwrap();
        assert_eq!(response["gateway_stopped"],true);
        assert!(state.stopping.load(Ordering::SeqCst));
        assert!(fixture.child.as_mut().unwrap().wait().unwrap().success());
        assert!(dir.join("full-exit.requested").exists());
    }
    #[test]
    fn read_routes_are_strictly_scoped() {
        assert!(allowed_read_route("/v1/risk/monitor"));
        assert!(allowed_read_route("/v1/execution/brains"));
        assert!(allowed_read_route("/v1/execution/reconciliation"));
        assert!(allowed_read_route("/v1/enterprise/supervisor"));
        assert!(!allowed_read_route("/v1/gateway/snapshot"));
        assert!(!allowed_read_route("http://attacker.local"));
        assert!(!allowed_read_route("/v1/config/../../ipc.key"));
        assert!(!allowed_read_route("/v1/status?token=x"));
    }
    #[test]
    fn journal_pagination_is_bounded_and_not_an_arbitrary_post_proxy() {
        // Core must retain the only exact destination and numeric bounds.
        assert!(valid_trade_page(0,1));
        assert!(valid_trade_page(1_000_000,100));
        assert!(!valid_trade_page(1_000_001,25));
        assert!(!valid_trade_page(0,0));
        assert!(!valid_trade_page(0,101));
        assert!(!allowed_read_route("/v1/execution/trades?limit=999999"));
    }
}
