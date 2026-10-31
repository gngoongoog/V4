//! V4 engine remains authoritative. The browser NEVER reads ipc.key, obtains
//! a broker token or connects directly to the Core's HTTP port.
use braintrade_runtime::{connect_existing_core, ensure_core, Client, ShutdownError,CheckedPostError};
use serde_json::{json, Value};
use std::{thread, time::{Duration, Instant}, sync::{Arc,Mutex,OnceLock,atomic::{AtomicBool,AtomicU64,Ordering}}};
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

static SNAPSHOT_SEQUENCE:AtomicU64=AtomicU64::new(0);
const MAX_SNAPSHOT_BYTES:u64=16*1024*1024*1024+11*1024*1024;

// The file picker, not the renderer, chooses the path. Never overwrite an
// existing user file or leave a partially copied file on an ordinary error.
fn copy_snapshot_new(source:&std::path::Path,destination:&std::path::Path)->Result<(),String>{
    use std::{fs::{self,OpenOptions},io};
    let mut input=fs::File::open(source).map_err(|e|format!("تعذر فتح ملف النسخة: {e}"))?;
    let meta=input.metadata().map_err(|e|e.to_string())?;
    if !meta.is_file()||meta.len()==0||meta.len()>MAX_SNAPSHOT_BYTES{return Err("حجم ملف نسخة العقول غير صالح".into());}
    let mut output=OpenOptions::new().write(true).create_new(true).open(destination)
        .map_err(|e|format!("تعذر إنشاء الملف؛ اختر اسماً غير مستخدم: {e}"))?;
    let copied=io::copy(&mut input,&mut output).and_then(|n|{output.sync_all()?;Ok(n)});
    match copied {
        Ok(n) if n==meta.len()=>Ok(()),
        Ok(_)=>{drop(output);let _=fs::remove_file(destination);Err("تغير حجم الملف أثناء النسخ؛ أعد اختياره".into())},
        Err(e)=>{drop(output);let _=fs::remove_file(destination);Err(format!("تعذر إكمال النسخ: {e}"))},
    }
}
fn snapshot_name_valid(name:&str)->bool{
    !name.is_empty()&&name.len()<=160&&name!="."&&name!=".."&&
        name.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_'|b'.'))
}
fn export_brain_snapshot()->Result<Value,String>{
    let Some(mut destination)=rfd::FileDialog::new().add_filter("BrainTrade Brain Snapshot",&["btbrain"])
        .set_file_name("BrainTrade-Brain.btbrain").save_file() else{return Ok(json!({"cancelled":true}));};
    if destination.extension().is_none(){destination.set_extension("btbrain");}
    if !destination.extension().and_then(|x|x.to_str()).is_some_and(|x|x.eq_ignore_ascii_case("btbrain")){
        return Err("اختر ملفاً بامتداد btbrain".into());
    }
    let client=open_client()?;
    let result=match client.checked_post("/v1/brain-snapshot/export",json!({}),Duration::from_secs(300)){
        Ok(v)=>v,
        Err(CheckedPostError::Rejected(e))=>return Err(e),
        Err(CheckedPostError::Uncertain(e))=>return Err(format!("تعذر تأكيد نتيجة إنشاء النسخة؛ راجع صفحة النسخ قبل الإعادة: {e}")),
    };
    let name=result["file_name"].as_str().filter(|n|snapshot_name_valid(n)&&n.ends_with(".btbrain"))
        .ok_or("لم يرجع Core اسم نسخة صالحاً")?;
    let source=braintrade_runtime::data_dir().join("brain_exports").join(name);
    copy_snapshot_new(&source,&destination)?;
    Ok(json!({"saved":true,"file_name":destination.file_name().and_then(|x|x.to_str()).unwrap_or("BrainTrade-Brain.btbrain"),
        "checksum":result["checksum"],"bytes":result["bytes"],"secrets_included":false}))
}
fn import_brain_snapshot()->Result<Value,String>{
    let Some(source)=rfd::FileDialog::new().add_filter("BrainTrade Brain Snapshot",&["btbrain"]).pick_file()
        else{return Ok(json!({"cancelled":true}));};
    if !source.extension().and_then(|x|x.to_str()).is_some_and(|x|x.eq_ignore_ascii_case("btbrain")){
        return Err("اختر ملفاً بامتداد btbrain".into());
    }
    let client=open_client()?;
    let inbox=braintrade_runtime::data_dir().join("brain_imports");
    std::fs::create_dir_all(&inbox).map_err(|e|e.to_string())?;
    let name=format!("tauri-{}-{}.btbrain",braintrade_runtime::now_ms(),SNAPSHOT_SEQUENCE.fetch_add(1,Ordering::Relaxed));
    let target=inbox.join(&name);copy_snapshot_new(&source,&target)?;
    match client.checked_post("/v1/brain-snapshot/import",json!({"name":name}),Duration::from_secs(300)){
        Ok(v)=>Ok(v),
        Err(CheckedPostError::Rejected(e))=>{let _=std::fs::remove_file(&target);Err(e)},
        Err(CheckedPostError::Uncertain(e))=>Err(format!("تعذر تأكيد نتيجة فحص النسخة؛ راجع قائمة النسخ قبل إعادة الاستيراد: {e}")),
    }
}
fn with_restore_barrier(state:&AppLifecycle,operation:impl FnOnce()->Result<Value,CheckedPostError>)->Result<Value,String>{
    let _guard=state.start_gate.lock().map_err(|_|"تعذر قفل حالة Core".to_string())?;
    if state.stopping.load(Ordering::SeqCst){return Err("إيقاف Core أو الاستعادة قيد التحقق".into());}
    // Prevent background status workers from restarting Core after staging.
    // Keep the barrier set when the response is lost, since Core may have
    // accepted the request and stopped even though the UI did not see it.
    state.stopping.store(true,Ordering::SeqCst);
    match operation(){
        Ok(v) if v["staged"]==true=>Ok(v),
        Ok(_)=>Err("رد الاستعادة غير واضح؛ أغلق البرنامج وتحقق من حالة النسخة قبل إعادة المحاولة".into()),
        Err(CheckedPostError::Rejected(e))=>{state.stopping.store(false,Ordering::SeqCst);Err(e)},
        Err(CheckedPostError::Uncertain(e))=>Err(format!("تعذر تأكيد نتيجة تجهيز الاستعادة؛ أغلق البرنامج وافحص حالة النسخ قبل الإعادة: {e}")),
    }
}
fn stage_snapshot_restore(state:&AppLifecycle,name:&str)->Result<Value,String>{
    if !snapshot_name_valid(name){return Err("اسم النسخة غير صالح".into());}
    with_restore_barrier(state,||{
        let client=connect_existing_core().map_err(|e|CheckedPostError::Rejected(format!("تعذر الوصول إلى Core الحالي: {e}")))?;
        let status=client.get("/v1/status").map_err(|e|CheckedPostError::Rejected(e.to_string()))?;
        let (pid,instance)=process_identity(&status).map_err(CheckedPostError::Rejected)?;
        let process=tracked_core::Handle::open(pid).map_err(CheckedPostError::Rejected)?;
        let current=client.get("/v1/status").map_err(|e|CheckedPostError::Rejected(e.to_string()))?;
        if process_identity(&current).map_err(CheckedPostError::Rejected)?!=(pid,instance.clone()){
            return Err(CheckedPostError::Rejected("تغيّرت جلسة Core؛ أعد طلب الاستعادة".into()));
        }
        let result=client.checked_post("/v1/backup/restore/stage",json!({"name":name,"expected_core_instance":instance}),Duration::from_secs(300))?;
        if result["staged"]==true {process.wait().map_err(CheckedPostError::Uncertain)?;}
        Ok(result)
    })
}
#[tauri::command]
async fn brain_snapshot_export()->Result<Value,String>{
    tauri::async_runtime::spawn_blocking(export_brain_snapshot).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn brain_snapshot_import()->Result<Value,String>{
    tauri::async_runtime::spawn_blocking(import_brain_snapshot).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn brain_snapshot_restore(name:String,lifecycle:tauri::State<'_,AppLifecycle>)->Result<Value,String>{
    let state=lifecycle.inner().clone();
    tauri::async_runtime::spawn_blocking(move||stage_snapshot_restore(&state,&name)).await.map_err(|e|e.to_string())?
}

fn write_excel_new(destination:&std::path::Path,bytes:&[u8])->Result<(),String>{
    use std::{fs::{self,OpenOptions},io::Write};
    let mut file=OpenOptions::new().write(true).create_new(true).open(destination)
        .map_err(|e|format!("تعذر إنشاء الملف؛ اختر اسماً غير مستخدم: {e}"))?;
    if let Err(error)=file.write_all(bytes).and_then(|_|file.sync_all()) {
        drop(file);let _=fs::remove_file(destination);
        return Err(format!("تعذر حفظ ملف Excel: {error}"));
    }
    Ok(())
}
fn save_excel_file(bytes:Vec<u8>,kind:String)->Result<Value,String>{
    if !matches!(kind.as_str(),"journal"|"report") || bytes.len()>2*1024*1024
        || bytes.len()<22 || !bytes.starts_with(b"PK\x03\x04") {
        return Err("بيانات ملف Excel غير صالحة".into());
    }
    let name=if kind=="journal"{"BrainTrade-Journal.xlsx"}else{"BrainTrade-Report.xlsx"};
    let Some(mut destination)=rfd::FileDialog::new().add_filter("Excel workbook",&["xlsx"])
        .set_file_name(name).save_file() else{return Ok(json!({"cancelled":true}));};
    if destination.extension().is_none(){destination.set_extension("xlsx");}
    if !destination.extension().and_then(|s|s.to_str()).is_some_and(|s|s.eq_ignore_ascii_case("xlsx")) {
        return Err("اختر اسماً بامتداد xlsx".into());
    }
    write_excel_new(&destination,&bytes)?;
    Ok(json!({"saved":true,"bytes":bytes.len()}))
}
#[tauri::command]
async fn save_excel_report(bytes:Vec<u8>,kind:String)->Result<Value,String>{
    tauri::async_runtime::spawn_blocking(move||save_excel_file(bytes,kind)).await.map_err(|e|e.to_string())?
}

#[tauri::command]
fn backtest_action(action: String, request: Option<Value>) -> Result<Value, String> {
    let (route,body)=match action.as_str(){
        "start"=>{
            let input=request.ok_or("تفاصيل الاختبار التاريخي غير متاحة")?;
            let allowed=["from_date","to_date","initial_equity","spread_floor","strategy"];
            if input.as_object().is_none_or(|m|m.keys().any(|k| !allowed.contains(&k.as_str()))) {
                return Err("حقول الاختبار التاريخي غير صالحة".into());
            }
            if input.get("strategy").is_some_and(|v|!matches!(v.as_str(),Some("meta_v1"|"ces_v1"|"kte_v1"|"meta_v2"|"explore_v1"))) {
                return Err("الاستراتيجية التاريخية المطلوبة غير مدعومة".into());
            }
            ("/v1/backtest/start",input)
        },
        "cancel"=>("/v1/backtest/cancel",json!({})),
        _=>return Err("هذا الإجراء التاريخي غير مسموح".into()),
    };
    open_client()?.post(route,body).map_err(|err|err.to_string())
}

// A strictly typed, allowlisted bridge to immutable research history.
// Read-only reports and local exports do not submit broker orders.
fn study_request(kind:&str,action:&str,id:Option<&str>,source_id:Option<&str>,plan:Option<&Value>)->Result<(&'static str,bool,Value),String>{
    let valid_id=|s:&str|s.len()>=8&&s.len()<=100&&s.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-');
    match (kind,action){
        ("backtest","runs")=>Ok(("/v1/backtest/runs",false,json!({}))),
        ("evaluation","runs")=>Ok(("/v1/evaluation/runs",false,json!({}))),
        ("backtest","report")|("backtest","export")|("evaluation","report")|("evaluation","export")=>{
            let id=id.filter(|v|valid_id(v)).ok_or("اختر نتيجة محفوظة صحيحة")?;
            let route=match (kind,action){
                ("backtest","report")=>"/v1/backtest/report",
                ("backtest","export")=>"/v1/backtest/export",
                ("evaluation","report")=>"/v1/evaluation/report",
                _=>"/v1/evaluation/export",
            };
            Ok((route,true,json!({"id":id})))
        },
        ("evaluation","start")=>{
            let source=source_id.filter(|v|valid_id(v)).ok_or("اختر اختباراً تاريخياً محفوظاً")?;
            let plan=plan.ok_or("خطة التقييم مفقودة")?;
            let valid=match plan.get("mode").and_then(Value::as_str){
                Some("holdout")=>plan.as_object().is_some_and(|m|m.len()==2)&&
                    plan.get("holdout_percent").and_then(Value::as_u64).is_some_and(|v|(10..=50).contains(&v)),
                Some("rolling")=>plan.as_object().is_some_and(|m|m.len()==4)&&
                    plan.get("development_days").and_then(Value::as_u64).is_some_and(|v|(2..=365).contains(&v))&&
                    plan.get("holdout_days").and_then(Value::as_u64).is_some_and(|v|(1..=365).contains(&v))&&
                    plan.get("step_days").and_then(Value::as_u64).is_some_and(|v|(1..=365).contains(&v)),
                _=>false,
            };
            if !valid{return Err("قيم التقييم خارج الحدود أو الحقول غير معروفة".into());}
            Ok(("/v1/evaluation/start",true,json!({"source_id":source,"plan":plan})))
        },
        ("evaluation","cancel")=>Ok(("/v1/evaluation/cancel",true,json!({}))),
        _=>Err("عملية دراسة غير مسموحة".into()),
    }
}
#[tauri::command]
fn study_action(kind:String,action:String,id:Option<String>,source_id:Option<String>,plan:Option<Value>)->Result<Value,String>{
    let (route,post,body)=study_request(&kind,&action,id.as_deref(),source_id.as_deref(),plan.as_ref())?;
    let client=open_client()?;
    if post{client.post(route,body)}else{client.get(route)}.map_err(|e|e.to_string())
}

// Fixed News RSS actions: no dynamic route, user URLs, or trade execution.
fn news_request(action:&str)->Result<(&'static str,Value),String>{
 match action{
  "refresh"=>Ok(("/v1/news/refresh",json!({}))),
  "enable"=>Ok(("/v1/news/settings",json!({"enabled":true}))),
  "disable"=>Ok(("/v1/news/settings",json!({"enabled":false}))),
  _=>Err("إجراء أخبار غير مدعوم".into())
 }
}
#[tauri::command]
fn news_action(action:String)->Result<Value,String>{
 let (route,body)=news_request(&action)?;
 let client=open_client()?;
 let response=client.post(route,body).map_err(|e|format!("تأكيد أمر الأخبار غير متاح؛ أعد قراءة الحالة قبل إعادة الطلب: {e}"))?;
 if response["ok"]!=true{return Err("Core لم يؤكد إجراء الأخبار".into());}
 // Settings must be persisted and reflected in a fresh Core readback.
 if action=="enable"||action=="disable"{
  let actual=client.get("/v1/news").map_err(|e|format!("تم قبول الأمر لكن تعذر قراءة الحالة: {e}"))?;
  if actual["enabled"].as_bool()!=Some(action=="enable"){
   return Err("لم تتطابق حالة مصادر الأخبار مع الطلب؛ حدّث الصفحة".into());
  }
 }
 Ok(response)
}

// Research-only Studio operations: no arbitrary path, order routing, or model promotion.
fn studio_request(action:&str,timeframe:Option<&str>)->Result<(&'static str,Value),String>{
 match action {
  "run"=>{
   let tf=timeframe.unwrap_or("ALL");
   if tf!="ALL"&&!braintrade_domain::studio::TIMEFRAMES.contains(&tf){
    return Err("الفريم المطلوب غير مدعوم".into());
   }
   Ok(("/v1/studio/run",json!({"timeframe":tf})))
  },
  "auto_enable"=>Ok(("/v1/studio/auto",json!({"enabled":true}))),
  "auto_disable"=>Ok(("/v1/studio/auto",json!({"enabled":false}))),
  "cancel"=>Ok(("/v1/studio/cancel",json!({}))),
  _=>Err("إجراء استوديو غير مدعوم".into())
 }
}
#[tauri::command]
fn studio_action(action:String,timeframe:Option<String>)->Result<Value,String>{
 let (route,body)=studio_request(&action,timeframe.as_deref())?;
 let client=open_client()?;
 let response=client.post(route,body).map_err(|e|format!("نتيجة الطلب البحثي غير مؤكدة؛ راجع حالة Core قبل التكرار: {e}"))?;
 match action.as_str(){
  "run" if !response["queued"].is_number()=>return Err("لم يرجع Core عدد التجارب المجدولة".into()),
  "cancel" if response["cancelled"]!=true=>return Err("لم يؤكد Core إلغاء الدراسة".into()),
  "auto_enable"|"auto_disable"=>{
   let enabled=action=="auto_enable";
   let current=client.get("/v1/studio").map_err(|e|format!("حالة الاستوديو بعد التغيير غير مؤكدة: {e}"))?;
   if response["auto"].as_bool()!=Some(enabled)||current["auto"].as_bool()!=Some(enabled){
    return Err("حالة التلقائي في Core لا تطابق التغيير المطلوب".into());
   }
  },
  _=>{}
 }
 Ok(response)
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
fn update_brain_paper_all(context: String, mode: String, paper: bool) -> Result<Value,String> {
    if !["scalping","swing","expert","sniper"].contains(&mode.as_str()) || context.len()>500 || context.is_empty() {
        return Err("حساب MT5 أو وضع التشغيل غير مسموح".into());
    }
    // Core performs the account/mode/STOP/reconciliation checks and commits
    // all secondary-brain settings atomically. This is not a trading command.
    open_client()?.post("/v1/execution/brain-paper-all",
        json!({"context":context,"mode":mode,"paper":paper}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn update_risk_master(context:String,enabled:bool,confirm_live_login:String,recovery:Option<Value>)->Result<Value,String>{
    if context.is_empty() || context.len()>500 || confirm_live_login.len()>32 {
        return Err("حساب MT5 أو التأكيد غير صالح".into());
    }
    // Core requires STOP, no unresolved broker intents, the exact current
    // account, and a separate REAL login confirmation when disabling.
    open_client()?.post("/v1/execution/risk-master",
        json!({"context":context,"enabled":enabled,"confirm_live_login":confirm_live_login,"recovery":recovery}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn update_position_protection(context:String,policy:Value,expected_policy_id:String,confirm_live_login:String)->Result<Value,String>{
    if context.is_empty() || context.len()>500 || expected_policy_id.len()>64 || confirm_live_login.len()>32 {
        return Err("حساب MT5 أو التأكيد غير صالح".into());
    }
    open_client()?.post("/v1/execution/position-protection",
        json!({"context":context,"policy":policy,"expected_policy_id":expected_policy_id,"confirm_live_login":confirm_live_login}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn set_learning_frozen(frozen:bool)->Result<Value,String>{
    open_client()?.post("/v1/learning/control",json!({"frozen":frozen}))
        .map_err(|err|err.to_string())
}

#[tauri::command]
fn manual_market(request:Value)->Result<Value,String>{
    match open_client()?.checked_post("/v1/execution/manual-market",request,Duration::from_secs(8)) {
        Ok(value)=>Ok(value),
        Err(CheckedPostError::Rejected(error))=>Err(error),
        Err(CheckedPostError::Uncertain(_))=>Err("نتيجة الطلب اليدوي غير مؤكدة. لا تكرر الطلب؛ راجع أوامر Core والمصالحة وMT5 أولاً.".into()),
    }
}

#[tauri::command]
fn cancel_pending_order(context:String,order:Value)->Result<Value,String>{
    if context.is_empty() || context.len()>500 {return Err("حساب MT5 غير صالح".into());}
    match open_client()?.checked_post("/v1/execution/cancel-pending",json!({"context":context,"order":order}),Duration::from_secs(8)) {
        Ok(value)=>Ok(value),
        Err(CheckedPostError::Rejected(error))=>Err(error),
        Err(CheckedPostError::Uncertain(_))=>Err("نتيجة طلب الإلغاء غير مؤكدة. لا تكرره؛ راجع حالة الأمر وسجل MT5 أولاً.".into()),
    }
}

#[tauri::command]
fn select_solo_brain(context:String,mode:String,brain:String)->Result<Value,String>{
    if context.is_empty() || context.len()>500 || !["scalping","swing","expert","sniper"].contains(&mode.as_str())
        || brain.is_empty() || brain.len()>100 || !brain.bytes().all(|b|b.is_ascii_lowercase()||b==b'_') {
        return Err("حساب MT5 أو الوضع أو هوية العقل غير صالحة".into());
    }
    // Core checks STOP, exact account/mode, flat owned positions and settled
    // broker intents. Selecting an owner never starts trading.
    open_client()?.post("/v1/execution/solo",json!({"context":context,"mode":mode,"brain":brain}))
        .map_err(|err|err.to_string())
}

#[derive(serde::Deserialize,serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SelectedClosePosition {ticket:u64,magic:u64,side:String,volume:f64}

#[tauri::command]
fn close_owned_positions(context:String,positions:Vec<SelectedClosePosition>,profit_only:bool)->Result<Value,String>{
    let unique:std::collections::BTreeSet<_>=positions.iter().map(|p|p.ticket).collect();
    if context.is_empty() || context.len()>500 || positions.is_empty() || positions.len()>20
        || unique.len()!=positions.len() || unique.contains(&0)
        || positions.iter().any(|p|p.ticket>9_007_199_254_740_991 || !p.volume.is_finite() || p.volume<=0.
            || !matches!(p.side.as_str(),"BUY"|"SELL")) {
        return Err("قائمة المراكز أو حساب MT5 غير صحيح".into());
    }
    // Only approved, exact owned tickets on the bound account and symbol may
    // be queued. Core and Gateway independently recheck their current state.
    match open_client()?.checked_post("/v1/execution/close-selected",
        json!({"context":context,"positions":positions,"profit_only":profit_only}),Duration::from_secs(8)) {
        Ok(value)=>Ok(value),
        Err(CheckedPostError::Rejected(e))=>Err(e),
        Err(CheckedPostError::Uncertain(_))=>Err("نتيجة طلب الإغلاق غير مؤكدة. لا تكرّر الطلب؛ راجع الأوامر والمراكز في MT5 وانتظر المصالحة.".into()),
    }
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
            backup_action, brain_snapshot_export, brain_snapshot_import, brain_snapshot_restore, save_excel_report,
            backtest_action, study_action, news_action, studio_action, update_brain_control, update_brain_paper_all, update_risk_master, update_position_protection, set_learning_frozen, manual_market, cancel_pending_order, select_solo_brain, close_owned_positions, bind_execution
        ])
        .run(tauri::generate_context!())
        .expect("could not start BRAINTRADE Tauri terminal");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn research_study_bridge_is_allowlisted_and_never_broker_execution(){
        let (path,post,_)=study_request("evaluation","start",None,Some("source-run-2026"),Some(&json!({"mode":"holdout","holdout_percent":30}))).unwrap();
        assert_eq!(path,"/v1/evaluation/start");assert!(post);
        assert!(study_request("evaluation","start",None,Some("source-run-2026"),Some(&json!({"mode":"rolling","development_days":0,"holdout_days":2,"step_days":1}))).is_err());
        assert!(study_request("evaluation","start",None,Some("source-run-2026"),Some(&json!({"mode":"holdout","holdout_percent":30,"trade_live":true}))).is_err());
        assert!(study_request("backtest","start",None,None,None).is_err());
        assert!(study_request("evaluation","../../v1/start",None,None,None).is_err());
        let (path,post,_)=study_request("backtest","report",Some("saved-run-2026"),None,None).unwrap();
        assert_eq!(path,"/v1/backtest/report");assert!(post);
        assert!(study_request("backtest","report",Some("../etc/token"),None,None).is_err());
    }
    #[test]
    fn news_actions_are_strictly_allowlisted(){
        assert_eq!(news_request("enable").unwrap().0,"/v1/news/settings");
        assert_eq!(news_request("enable").unwrap().1["enabled"],true);
        assert_eq!(news_request("disable").unwrap().1["enabled"],false);
        assert_eq!(news_request("refresh").unwrap().0,"/v1/news/refresh");
        for action in ["trade","../../v1/manual","refresh?token=1","settings"]{assert!(news_request(action).is_err());}
    }
    #[test]
    fn studio_research_actions_are_scoped(){
        let (route,data)=studio_request("run",Some("M5")).unwrap();
        assert_eq!(route,"/v1/studio/run");
        assert_eq!(data["timeframe"],"M5");
        assert_eq!(studio_request("run",None).unwrap().1["timeframe"],"ALL");
        assert_eq!(studio_request("auto_enable",None).unwrap().1["enabled"],true);
        assert_eq!(studio_request("auto_disable",None).unwrap().1["enabled"],false);
        assert!(studio_request("run",Some("XAUUSD")).is_err());
        assert!(studio_request("trade",None).is_err());
    }
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
        assert!(binary.is_file(),"Portable build must first compile the real debug Core: {}",binary.display());
        // Do NOT hide the Core's startup error. On a loaded Windows runner the
        // database initialization can take longer than a single HTTP attempt.
        // A dead child must fail immediately with its stderr, not be reported
        // as a mysterious readiness timeout 20 seconds later.
        let startup_log=dir.join("isolated-core-startup.log");
        let logfile=fs::File::create(&startup_log).unwrap();
        fixture.child=Some(Command::new(binary).stdin(Stdio::null())
            .stdout(Stdio::null()).stderr(Stdio::from(logfile)).spawn().unwrap());
        let pid=fixture.child.as_ref().unwrap().id();
        let deadline=Instant::now()+Duration::from_secs(60);
        let mut last_probe=String::from("No Core response yet");
        loop {
            if let Some(exit)=fixture.child.as_mut().unwrap().try_wait().unwrap(){
                panic!("Isolated Core PID {pid} exited before readiness ({exit}); last probe: {last_probe}; startup: {}",
                    fs::read_to_string(&startup_log).unwrap_or_else(|_|"Startup log unreadable".into()));
            }
            match connect_existing_core(){
                Ok(client)=>match client.get("/v1/status"){
                    Ok(status)=>{
                        if process_identity(&status).is_ok_and(|(observed,_)|observed==pid){break;}
                        last_probe=format!("Core identity was not the expected isolated PID {pid}");
                    }
                    Err(error)=>last_probe=format!("Core status request: {error}"),
                },
                Err(error)=>last_probe=format!("Core connection: {error}"),
            }
            if Instant::now()>=deadline {
                panic!("Isolated Core PID {pid} was not ready within 60 seconds; last probe: {last_probe}; startup: {}",
                    fs::read_to_string(&startup_log).unwrap_or_else(|_|"Startup log unreadable".into()));
            }
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
        assert!(allowed_read_route("/v1/studio/chart-frames"));
        assert!(!allowed_read_route("/v1/gateway/snapshot"));
        assert!(!allowed_read_route("http://attacker.local"));
        assert!(!allowed_read_route("/v1/config/../../ipc.key"));
        assert!(!allowed_read_route("/v1/status?token=x"));
    }
    #[test]
    fn snapshot_copy_never_overwrites_and_rejects_unsafe_names(){
        use std::{fs,time::{SystemTime,UNIX_EPOCH}};
        let unique=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir=std::env::temp_dir().join(format!("bt-snapshot-copy-{}-{unique}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source=dir.join("source.btbrain");let dest=dir.join("dest.btbrain");
        fs::write(&source,b"validated-snapshot").unwrap();
        copy_snapshot_new(&source,&dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(),b"validated-snapshot");
        fs::write(&source,b"replacement").unwrap();
        assert!(copy_snapshot_new(&source,&dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(),b"validated-snapshot");
        assert!(snapshot_name_valid("imported-brain-42-abc"));
        for bad in ["","..","../outside","a/b","a\\b"] {assert!(!snapshot_name_valid(bad));}
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn excel_save_rejects_invalid_payload_and_never_overwrites(){
        use std::{fs,time::{SystemTime,UNIX_EPOCH}};
        assert!(save_excel_file(vec![],"journal".into()).is_err());
        assert!(save_excel_file(vec![0;23],"report".into()).is_err());
        assert!(save_excel_file(vec![0;23],"unknown".into()).is_err());
        let unique=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir=std::env::temp_dir().join(format!("bt-xlsx-save-{}-{unique}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();let dest=dir.join("report.xlsx");
        write_excel_new(&dest,b"first").unwrap();
        assert!(write_excel_new(&dest,b"second").is_err());
        assert_eq!(fs::read(&dest).unwrap(),b"first");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn restore_barrier_only_reopens_after_an_explicit_rejection(){
        let state=AppLifecycle::default();
        assert!(with_restore_barrier(&state,||Err(CheckedPostError::Rejected("not flat".into()))).is_err());
        assert!(!state.stopping.load(Ordering::SeqCst));
        assert!(with_restore_barrier(&state,||Err(CheckedPostError::Uncertain("timeout".into()))).is_err());
        assert!(state.stopping.load(Ordering::SeqCst));
        assert!(with_start_permit(&state,||Ok(())).is_err());
        state.stopping.store(false,Ordering::SeqCst);
        assert!(with_restore_barrier(&state,||Ok(json!({"staged":true}))).is_ok());
        assert!(state.stopping.load(Ordering::SeqCst));
        assert!(with_start_permit(&state,||Ok(())).is_err());
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
