mod backup;
mod backtest;
mod evaluation;
mod learning;
mod learning_inference;
mod learning_performance_review;
mod learning_review;
mod learning_scoring;
pub mod news;
mod research;
pub mod secrets;
mod telegram;
use anyhow::{bail, Context, Result};
use braintrade_domain::{Config, Status, Telemetry};
use fs2::FileExt;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const PORT: u16 = 18741;
pub const BASE: &str = "http://127.0.0.1:18741";
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
pub fn data_dir() -> PathBuf {
    std::env::var_os("BRAINTRADE_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        directories::ProjectDirs::from("com", "BrainTrade", "BrainTrade").expect("User data directory unavailable").data_local_dir().to_path_buf()
    })
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new(); options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut f = options.open(&tmp)?; f.write_all(bytes)?; f.sync_all()?; drop(f);
    if let Err(e) = fs::rename(&tmp, path) { let _ = fs::remove_file(&tmp); return Err(e.into()); }
    Ok(())
}
fn configure_database(db: &Connection) -> Result<()> {
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY, timestamp_ms INTEGER NOT NULL, kind TEXT NOT NULL, detail TEXT NOT NULL); PRAGMA user_version=1;")?;
    let foreign_keys: i64 = db.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    if foreign_keys != 1 { bail!("SQLite foreign-key enforcement unavailable"); }
    Ok(())
}
fn verify_foreign_key_integrity(db: &Connection) -> Result<()> {
    let mut statement = db.prepare("PRAGMA foreign_key_check")?;
    let mut rows = statement.query([])?;
    if let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let rowid: Option<i64> = row.get(1)?;
        bail!("SQLite foreign-key integrity check failed in {table} at row {rowid:?}");
    }
    Ok(())
}
fn read_config(dir: &Path) -> Result<Config> {
    let path=dir.join("config.json"); if !path.exists(){return Ok(Config::default());}
    let c:Config=serde_json::from_slice(&fs::read(path)?)?; c.validate().map_err(anyhow::Error::msg)?; Ok(c)
}
fn ipc_key(dir:&Path)->Result<String>{
    let path=dir.join("ipc.key");if path.exists(){return read_ipc_key(dir);}let key=format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple());
    #[cfg(windows)] secrets::write(&path,&key)?;
    #[cfg(not(windows))] atomic_write(&path,key.as_bytes())?;
    Ok(key)
}
pub fn read_ipc_key(dir:&Path)->Result<String>{
    #[cfg(windows)] { secrets::read(&dir.join("ipc.key")) }
    #[cfg(not(windows))] { Ok(fs::read_to_string(dir.join("ipc.key"))?) }
}
#[derive(Clone)]
pub struct Client{http:reqwest::blocking::Client,key:String}
impl Client{
    pub fn new(dir:&Path)->Result<Self>{Ok(Self{http:reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(4)).build()?,key:read_ipc_key(dir)?})}
    pub fn get(&self,path:&str)->Result<Value>{Ok(self.http.get(format!("{BASE}{path}")).bearer_auth(&self.key).send()?.error_for_status()?.json()?)}
    pub fn post(&self,path:&str,body:Value)->Result<Value>{let response=self.http.post(format!("{BASE}{path}")).bearer_auth(&self.key).json(&body).send()?;let code=response.status();let value:Value=response.json()?;if !code.is_success(){bail!("{}",value["error"].as_str().unwrap_or("Request failed"));}Ok(value)}
}
pub(crate) struct Shared{pub status:Status,pub config:Config,pub token:Option<String>}
struct Engine{dir:PathBuf,shared:Arc<Mutex<Shared>>,db:Connection,gateway:Option<Child>,gateway_retry_ms:u64,shutting_down:bool,research:research::Research,backtests:backtest::Service,evaluations:evaluation::Service,news:news::Service,learning:learning::Service,inference:learning_inference::Service,scoring:learning_scoring::Service}
#[derive(Deserialize)]
struct ConfigRequest{config:Config,#[serde(default)]telegram_token:String,#[serde(default)]clear_telegram_token:bool}
#[derive(Serialize)]
struct Event{id:i64,timestamp_ms:u64,kind:String,detail:Value}
impl Engine{
    fn event(&self,kind:&str,detail:Value)->Result<()>{self.db.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,?2,?3)",params![now_ms(),kind,detail.to_string()])?;Ok(())}
    fn launch_gateway(&mut self)->Result<()>{
        if let Some(child)=self.gateway.as_mut(){if child.try_wait()?.is_none(){return Ok(());}}
        let exe=std::env::current_exe()?;let base=exe.parent().context("Executable parent missing")?;let packed=base.join("runtime").join(if cfg!(windows){"BrainTradeGateway.exe"}else{"BrainTradeGateway"});let embedded_python=base.join("runtime").join("python").join("python.exe");let embedded_script=base.join("runtime").join("gateway").join("mt5_gateway.py");
        let mut command=if packed.exists(){Command::new(packed)}else if cfg!(windows)&&embedded_python.is_file()&&embedded_script.is_file(){let mut c=Command::new(embedded_python);c.arg(embedded_script);c}else{let script=std::env::var_os("BRAINTRADE_GATEWAY_SCRIPT").map(PathBuf::from);let Some(script)=script else{bail!("Gateway executable missing. Build the Windows distribution or set BRAINTRADE_GATEWAY_SCRIPT for development.");};let mut c=Command::new(std::env::var_os("BRAINTRADE_PYTHON").unwrap_or("python".into()));c.arg(script);c};
        command.env("BRAINTRADE_DATA_DIR",&self.dir).env("BRAINTRADE_IPC_TOKEN",read_ipc_key(&self.dir)?);command.stdin(Stdio::null()).stdout(Stdio::null());let log=OpenOptions::new().append(true).create(true).open(self.dir.join("gateway.log"))?;command.stderr(log);
        #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
        self.gateway=Some(command.spawn().context("Could not launch MT5 gateway")?);self.event("gateway_started",json!({}))?;Ok(())
    }
    fn stop_gateway(&mut self){if let Some(mut child)=self.gateway.take(){let _=child.kill();let _=child.wait();}}
    fn tick(&mut self){self.news.tick(&self.db,now_ms());self.backtests.tick(&self.db,now_ms());self.evaluations.tick(&self.db);self.shared.lock().unwrap().status.refresh(now_ms());if now_ms()>=self.gateway_retry_ms{self.gateway_retry_ms=now_ms()+15_000;if let Err(error)=self.launch_gateway(){self.shared.lock().unwrap().status.message=error.to_string();}}}
    fn route(&mut self,method:&str,path:&str,body:&[u8])->Result<Value>{
        if path=="/v1/news"||path.starts_with("/v1/news/"){return self.news.route(method,path,body,&self.db);}
        if path=="/v1/evaluation"||path.starts_with("/v1/evaluation/"){return self.evaluations.route(method,path,body,&self.db,&self.dir);}
        if path=="/v1/learning/inference"||path.starts_with("/v1/learning/inference/"){return self.inference.route(method,path,&self.db);}
        if path=="/v1/learning/scoring"||path.starts_with("/v1/learning/scoring/"){return self.scoring.route(method,path,&self.db);}
        if path.starts_with("/v1/learning/performance-review/"){return learning_performance_review::route(method,path,body,&mut self.db,now_ms());}
        if path.starts_with("/v1/learning/review/"){return learning_review::route(method,path,body,&mut self.db,now_ms());}
        if path=="/v1/backtest"||path.starts_with("/v1/backtest/")||path.starts_with("/v1/gateway/history/"){
            let(config,status)={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());(s.config.clone(),s.status.clone())};let open=status.telemetry.as_ref().is_some_and(|t|self.research.has_open_position(t));return self.backtests.route(method,path,body,&self.db,&self.dir,&config,&status,open);
        }
        if method=="GET"&&path=="/v1/backups"{return backup::list(&self.dir);}
        if method=="GET"&&path=="/v1/backup/restore/status"{return backup::pending_status(&self.dir);}
        if method=="POST"&&path=="/v1/backup/restore/validate"{return backup::validate_request(&self.dir,body);}
        if method=="POST"&&path=="/v1/backup"{
            let config=self.shared.lock().unwrap().config.clone();
            let result=backup::create(&self.db,&self.dir,&config,now_ms())?;
            self.event("foundation_backup_created",json!({"path":result["path"],"schema":result["schema"],"manifest_hash":result["manifest_hash"]}))?;
            return Ok(result);
        }
        if method=="POST"&&path=="/v1/backup/restore/stage"{
            if self.backtests.loading(){bail!("Cannot stage restore while broker history is loading");}
            let config=self.shared.lock().unwrap().config.clone();
            let result=backup::stage(&self.db,&self.dir,&config,body,now_ms())?;
            self.event("backup_restore_staged",json!({"source":result["source"],"rollback":result["rollback"],"secrets_restored":false,"broker_effect":false}))?;
            self.backtests.cancel();self.evaluations.cancel();self.stop_gateway();
            {let mut s=self.shared.lock().unwrap();s.status.requested_run=false;s.status.message="Validated restore staged. Core is stopping; reopen BrainTrade to apply it before SQLite startup.".into();s.status.refresh(now_ms());}
            self.shutting_down=true;
            return Ok(result);
        }
        match(method,path){
            ("GET","/v1/status")=>{let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());Ok(serde_json::to_value(&s.status)?)},
            ("GET","/v1/config")=>{let s=self.shared.lock().unwrap();Ok(json!({"config":s.config,"token_saved":s.token.is_some()}))},
            ("GET","/v1/learning")=>{let base=self.learning.status(&self.db)?;let reviewed=learning_review::enrich_learning_status(&self.db,base)?;let inferred=self.inference.enrich_learning_status(&self.db,reviewed)?;let scored=self.scoring.enrich_learning_status(&self.db,inferred)?;learning_performance_review::enrich_learning_status(&self.db,scored)},
            ("POST","/v1/learning/export")=>self.learning.export(&self.db,&self.dir,now_ms()),
            ("POST","/v1/learning/register")=>{let artifact:Value=serde_json::from_slice(body)?;self.learning.register(&mut self.db,artifact,now_ms())},
            ("GET","/v1/decisions")=>research::decision_history(&self.db),
            ("GET","/v1/opportunities/history")=>research::opportunity_history(&self.db),
            ("POST","/v1/risk")=>{let risk:braintrade_domain::RiskConfig=serde_json::from_slice(body)?;risk.validate().map_err(anyhow::Error::msg)?;let mut s=self.shared.lock().unwrap();let mut updated=s.config.clone();updated.risk=risk;atomic_write(&self.dir.join("config.json"),&serde_json::to_vec_pretty(&updated)?)?;self.event("risk_settings_changed",json!({"risk":updated.risk,"mode":"shadow"}))?;s.config=updated;Ok(json!({"ok":true}))},
            ("POST","/v1/export")=>{
                let name=format!("research-{}-{}",now_ms(),uuid::Uuid::new_v4());let staging=self.dir.join("exports").join(format!(".{name}.tmp"));fs::create_dir_all(&staging)?;
                for table in["decisions","paper_events","candles","opportunity_setups"]{let sql=match table{"opportunity_setups"=>"SELECT context,first_seen_ms,payload FROM opportunity_setups ORDER BY first_seen_ms,setup_id","decisions"=>"SELECT context,timestamp_ms,payload FROM decisions ORDER BY id","paper_events"=>"SELECT context,timestamp_ms,json_object('kind',kind,'detail',json(payload)) FROM paper_events ORDER BY id",_=>"SELECT context,bar_time*1000,json_object('timeframe',timeframe,'candle',json(payload)) FROM candles ORDER BY context,timeframe,bar_time"};let mut stmt=self.db.prepare(sql)?;let mut rows=stmt.query([])?;let mut output=fs::File::create(staging.join(format!("{table}.jsonl")))?;while let Some(row)=rows.next()?{let payload:Value=serde_json::from_str(&row.get::<_,String>(2)?)?;writeln!(output,"{}",json!({"context":row.get::<_,String>(0)?,"timestamp_ms":row.get::<_,u64>(1)?,"payload":payload}))?;}output.sync_all()?;}
                atomic_write(&staging.join("README.txt"),b"BrainTrade research export. Virtual fills only; UTC timestamps in milliseconds. See Source/docs/RESEARCH.md for limitations.")?;let dest=self.dir.join("exports").join(name);fs::rename(staging,&dest)?;Ok(json!({"path":dest,"note":"Research export: candles, decisions and virtual ledger (JSONL)."}))
            },
            ("POST","/v1/config")=>{
                let request:ConfigRequest=serde_json::from_slice(body)?;request.config.validate().map_err(anyhow::Error::msg)?;
                if !request.config.mt5_path.is_empty(){let path=Path::new(&request.config.mt5_path);if !path.is_absolute()||!path.is_file()||path.extension().and_then(|v|v.to_str()).map(|v|v.to_ascii_lowercase())!=Some("exe".into()){bail!("MT5 path must point to an existing terminal64.exe file");}}
                let token=request.telegram_token.trim();if !token.is_empty()&&(token.len()<20||!token.contains(':')||token.chars().any(char::is_whitespace)){bail!("Invalid Telegram bot token format");}if request.clear_telegram_token&&!token.is_empty(){bail!("Cannot save and remove a token together");}
                let existing=self.shared.lock().unwrap().token.clone();let effective=if request.clear_telegram_token{None}else if !token.is_empty(){Some(token.to_owned())}else{existing};if request.config.telegram_enabled&&effective.is_none(){bail!("Enter a bot token before enabling Telegram");}
                if !token.is_empty(){secrets::write(&self.dir.join("telegram.sealed"),token)?;}atomic_write(&self.dir.join("config.json"),&serde_json::to_vec_pretty(&request.config)?)?;if request.clear_telegram_token&&self.dir.join("telegram.sealed").exists(){fs::remove_file(self.dir.join("telegram.sealed"))?;}
                {let mut s=self.shared.lock().unwrap();s.config=request.config;s.token=effective;s.status.gateway_fresh=false;s.status.last_received_ms=None;s.status.telemetry=None;s.status.analysis=Default::default();s.status.paper=Default::default();s.status.message="Settings saved; reconnecting MT5".into();}
                self.event("settings_changed",json!({"secrets":"redacted"}))?;if self.backtests.loading(){self.backtests.cancel();}self.stop_gateway();self.gateway_retry_ms=0;Ok(json!({"ok":true}))
            },
            ("POST","/v1/start")=>{if self.backtests.loading(){bail!("جلب التاريخ قيد العمل؛ انتظر أو ألغِ الاختبار قبل تشغيل المحاكاة");}self.event("monitoring_requested",json!({"execution":false}))?;let mut s=self.shared.lock().unwrap();s.status.requested_run=true;s.status.message="Analysis and SHADOW simulation started. No broker orders.".into();s.status.refresh(now_ms());Ok(serde_json::to_value(&s.status)?)},
            ("POST","/v1/stop")=>{self.event("monitoring_stopped",json!({"positions_unchanged":true}))?;let mut s=self.shared.lock().unwrap();s.status.requested_run=false;s.status.analysis.action="WAIT".into();s.status.analysis.gates.push("System stopped — new virtual entries disabled".into());s.status.message="New virtual entries stopped. Virtual exits continue while Core runs. MT5 positions untouched.".into();s.status.refresh(now_ms());Ok(serde_json::to_value(&s.status)?)},
            ("POST","/v1/reconnect")=>{self.stop_gateway();self.gateway_retry_ms=0;{let mut s=self.shared.lock().unwrap();s.status.last_received_ms=None;s.status.telemetry=None;s.status.refresh(now_ms());}self.event("reconnect_requested",json!({}))?;Ok(json!({"ok":true}))},
            ("POST","/v1/gateway/snapshot")=>{
                let telemetry:Telemetry=serde_json::from_slice(body)?;if !telemetry.validate(){bail!("Invalid telemetry");}let(config,run)={let s=self.shared.lock().unwrap();(s.config.clone(),s.status.requested_run)};let observed_ms=now_ms();let mut next=self.research.clone();let(analysis,paper)=next.update(&mut self.db,&telemetry,&config,run,observed_ms)?;self.learning.observe(&mut self.db,&telemetry,&analysis,observed_ms);self.inference.observe(&self.db,&telemetry,&analysis,observed_ms);self.scoring.observe(&self.db,observed_ms);self.research=next;let mut s=self.shared.lock().unwrap();let changed=s.status.telemetry.as_ref().map(|t|t.connected)!=Some(telemetry.connected);if changed{self.event("gateway_connection",json!({"connected":telemetry.connected}))?;}if !telemetry.error.is_empty(){s.status.message=telemetry.error.clone();}else{s.status.message="MT5 connected. Rule-based analysis + SHADOW; challenger inference/scoring/performance review are read-only audit telemetry.".into();}s.status.analysis=analysis;s.status.paper=paper;s.status.telemetry=Some(telemetry);s.status.last_received_ms=Some(observed_ms);s.status.refresh(now_ms());Ok(json!({"ok":true}))
            },
            ("GET","/v1/events")=>{let mut statement=self.db.prepare("SELECT id,timestamp_ms,kind,detail FROM events ORDER BY id DESC LIMIT 100")?;let events=statement.query_map([],|r|Ok(Event{id:r.get(0)?,timestamp_ms:r.get(1)?,kind:r.get(2)?,detail:serde_json::from_str(&r.get::<_,String>(3)?).unwrap_or(Value::Null)}))?.collect::<std::result::Result<Vec<_>,_>>()?;Ok(json!({"events":events}))},
            ("POST","/v1/shutdown")=>{self.event("core_shutdown_requested",json!({}))?;self.shutting_down=true;Ok(json!({"ok":true}))},
            _=>bail!("Unknown API route")
        }
    }
}

pub fn run_core()->Result<()>{
    let dir=data_dir();fs::create_dir_all(&dir)?;
    #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(&dir,fs::Permissions::from_mode(0o700))?;}
    let lock=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join("core.lock"))?;if lock.try_lock_exclusive().is_err(){return Ok(());}let restore_result=backup::apply_pending(&dir)?;let key=ipc_key(&dir)?;let config=read_config(&dir).context("Config invalid: preserve file and correct it; refusing silent reset")?;let db=Connection::open(dir.join("events.sqlite"))?;configure_database(&db)?;let research=research::Research::load(&db)?;let backtests=backtest::Service::new(&db)?;let evaluations=evaluation::Service::new(&db)?;let news=news::Service::new(&db)?;let learning=learning::Service::new(&db)?;learning_review::init(&db)?;let inference=learning_inference::Service::new(&db)?;let scoring=learning_scoring::Service::new(&db)?;learning_performance_review::init(&db)?;verify_foreign_key_integrity(&db)?;let token=if dir.join("telegram.sealed").exists(){Some(secrets::read(&dir.join("telegram.sealed"))?)}else{None};let shared=Arc::new(Mutex::new(Shared{status:Status::default(),config,token}));let server=tiny_http::Server::http(("127.0.0.1",PORT)).map_err(|e|anyhow::anyhow!(e.to_string()))?;let mut engine=Engine{dir,shared:shared.clone(),db,gateway:None,gateway_retry_ms:0,shutting_down:false,research,backtests,evaluations,news,learning,inference,scoring};engine.event("core_started",json!({"version":braintrade_domain::VERSION,"execution_enabled":false,"challenger_inference":"read_only","prediction_scoring":"audit_only","challenger_performance_review":"manual_read_only","backup_restore":"validated_staged_restart_rollback","foreign_keys":true}))?;if let Some(result)=restore_result{engine.event("backup_restore_result",result)?;}telegram::spawn(shared);
    while !engine.shutting_down{engine.tick();let Some(mut request)=server.recv_timeout(Duration::from_millis(250))? else{continue;};let authenticated=request.headers().iter().any(|h|h.field.equiv("Authorization")&&h.value.as_str()==format!("Bearer {key}"));let origin=request.headers().iter().any(|h|h.field.equiv("Origin"));if !authenticated||origin{let _=request.respond(tiny_http::Response::from_string("{\"error\":\"Unauthorized\"}").with_status_code(401));continue;}if request.body_length().is_none()&&request.method().as_str()=="POST"{let _=request.respond(tiny_http::Response::empty(411));continue;}if request.body_length().unwrap_or(0)>2_000_000{let _=request.respond(tiny_http::Response::empty(413));continue;}let mut body=Vec::new();if request.as_reader().read_to_end(&mut body).is_err(){continue;}let result=engine.route(request.method().as_str(),request.url(),&body);let(code,value)=match result{Ok(v)=>(200,v),Err(e)=>(400,json!({"error":e.to_string()}))};let response=tiny_http::Response::from_string(value.to_string()).with_status_code(code).with_header(tiny_http::Header::from_bytes("Content-Type","application/json").unwrap());let _=request.respond(response);}
    engine.stop_gateway();engine.backtests.cancel();engine.evaluations.cancel();engine.event("core_stopped",json!({}))?;drop(lock);Ok(())
}
pub fn ensure_core()->Result<Client>{
    let dir=data_dir();if let Ok(client)=Client::new(&dir){if let Ok(status)=client.get("/v1/status"){if status["version"].as_str()!=Some(braintrade_domain::VERSION){bail!("An older Core is running. Open the old application, choose Full Exit / خروج كامل, then launch this version.");}return Ok(client);}}
    let mut command=Command::new(std::env::current_exe()?);fs::create_dir_all(&dir)?;command.arg("--core").stdin(Stdio::null()).stdout(Stdio::null()).stderr(OpenOptions::new().append(true).create(true).open(dir.join("core.log"))?);
    #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000008);}
    command.spawn()?;for _ in 0..100{std::thread::sleep(Duration::from_millis(100));if let Ok(client)=Client::new(&dir){if client.get("/v1/status").is_ok(){return Ok(client);}}}bail!("Core did not become ready. See {}",dir.join("core.log").display())
}
