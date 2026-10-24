mod backup;
mod execution;
mod execution_quality;
mod broker_quality_circuit;
mod enterprise_supervisor;
mod coordinator_learning;
mod sniper_learning;
mod brain_control;
mod sniper_latency_audit;
mod sniper_deal_link_audit;
mod sniper_policy_quality;
mod sniper_opportunities;
mod sniper_blocker_review;
mod sniper_demo_lineage;
mod trade_journal;
mod shadow_paper;
mod frozen_shadow;
mod frozen_coordinator;
mod coordinator_paper;
mod research_order_lifecycle;
mod research_admission;
mod research_execution_bridge;
mod research_demo_deal_audit;
mod matched_forward;
mod matched_trade;
mod student_learning;
mod studio;
mod backtest;
mod correlation;
mod evaluation;
mod learning;
mod learning_inference;
mod learning_performance_review;
mod learning_review;
mod learning_scoring;
mod learning_quality;
mod brain_integrity;
mod mode_evidence;
mod learning_forward_audit;
mod trading_settings;
pub mod news;
mod research;
mod risk_monitor;
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
struct Engine{dir:PathBuf,shared:Arc<Mutex<Shared>>,db:Connection,gateway:Option<Child>,gateway_retry_ms:u64,shutting_down:bool,research:research::Research,backtests:backtest::Service,evaluations:evaluation::Service,news:news::Service,learning:learning::Service,inference:learning_inference::Service,scoring:learning_scoring::Service,studio:studio::Service,execution:execution::Service}
#[derive(Deserialize)]
struct ConfigRequest{config:Config,#[serde(default)]telegram_token:String,#[serde(default)]clear_telegram_token:bool}
#[derive(Serialize)]
struct Event{id:i64,timestamp_ms:u64,kind:String,detail:Value}
fn ensure_execution_mutation_stopped(status:&Status,pending:bool)->Result<()> {
    if status.requested_run { bail!("Stop the system before changing execution-affecting risk or trading costs"); }
    if pending { bail!("Wait for broker command reconciliation before changing execution-affecting risk or trading costs"); }
    Ok(())
}
// Fail-closed operator shutdown gate. STOP only prevents NEW entries. Shutting
// down the Core would abandon timed management/reconciliation for open trades.
fn verify_safe_reconfiguration(status: &Status, unresolved: bool) -> Result<()> {
    anyhow::ensure!(!status.requested_run,
        "أوقف الدخول الجديد أولاً عبر STOP قبل تبديل مسار MT5 أو الحساب");
    anyhow::ensure!(!unresolved,"انتظر تسوية كل الأوامر المعلقة قبل تغيير اتصال MT5");
    // Once the Core has observed a broker session, an unknown or stale
    // portfolio NEVER proves that switching to another account is safe.
    // Initial first-time configuration without any prior broker session is
    // still supported so users can enter their MT5 terminal path.
    if status.last_received_ms.is_some() || status.telemetry.is_some() {
        anyhow::ensure!(status.gateway_fresh && status.telemetry.as_ref().is_some_and(|t|t.connected),
            "لا يمكن تغيير حساب أو مسار MT5 أثناء انقطاعه؛ استعد الاتصال وتأكد من المراكز أولاً");
        anyhow::ensure!(status.telemetry.as_ref().is_some_and(|t|
            t.positions.is_empty() && t.pending_orders == 0),
            "لا يمكن تبديل اتصال MT5 أثناء وجود مراكز مفتوحة أو أوامر معلّقة");
    }
    Ok(())
}

fn verify_safe_shutdown(status: &Status, unresolved: bool) -> Result<()> {
    anyhow::ensure!(!status.requested_run, "أوقف الدخول الجديد أولاً عبر STOP قبل الخروج الكامل");
    anyhow::ensure!(!unresolved, "تعذر الخروج الكامل: توجد أوامر وسيط غير محسومة");
    if let Some(t) = &status.telemetry {
        anyhow::ensure!(status.gateway_fresh && t.connected,
            "تعذر الخروج الكامل: حالة MT5 قديمة أو الاتصال مقطوع، تحقق من الصفقات أولاً");
        anyhow::ensure!(t.positions.is_empty() && t.pending_orders == 0,
            "تعذر الخروج الكامل: ما زالت توجد مراكز أو أوامر معلقة في MT5");
    } else {
        anyhow::ensure!(status.last_received_ms.is_none(),
            "تعذر الخروج الكامل: فقدت حالة حساب MT5 بعد الاتصال");
    }
    Ok(())
}

impl Engine{
    fn event(&self,kind:&str,detail:Value)->Result<()>{self.db.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,?2,?3)",params![now_ms(),kind,detail.to_string()])?;Ok(())}
    fn launch_gateway(&mut self)->Result<()>{
        if std::env::var("BRAINTRADE_GATEWAY_DISABLED").as_deref()==Ok("1"){return Ok(());}
        if let Some(child)=self.gateway.as_mut(){if child.try_wait()?.is_none(){return Ok(());}}
        let exe=std::env::current_exe()?;let base=exe.parent().context("Executable parent missing")?;
        // NSIS places Tauri resources in `resources/runtime`, while a Portable
        // build keeps `runtime` beside BrainTradeNext.exe. Support both with
        // no system-wide Python dependency or mutable working-directory guess.
        let direct=base.join("runtime");
        let bundled=base.join("resources").join("runtime");
        let runtime_root=if direct.join("python/python.exe").is_file() || direct.join("BrainTradeGateway.exe").is_file() {
            direct
        } else if bundled.join("python/python.exe").is_file() || bundled.join("BrainTradeGateway.exe").is_file() {
            bundled
        } else { direct };
        let packed=runtime_root.join(if cfg!(windows){"BrainTradeGateway.exe"}else{"BrainTradeGateway"});
        let embedded_python=runtime_root.join("python").join("python.exe");
        let embedded_script=runtime_root.join("gateway").join("mt5_gateway.py");
        let mut command=if packed.exists(){Command::new(packed)}else if cfg!(windows)&&embedded_python.is_file()&&embedded_script.is_file(){let mut c=Command::new(embedded_python);c.arg(embedded_script);c}else{let script=std::env::var_os("BRAINTRADE_GATEWAY_SCRIPT").map(PathBuf::from);let Some(script)=script else{bail!("Gateway executable missing. Build the Windows distribution or set BRAINTRADE_GATEWAY_SCRIPT for development.");};let mut c=Command::new(std::env::var_os("BRAINTRADE_PYTHON").unwrap_or("python".into()));c.arg(script);c};
        command.env("BRAINTRADE_DATA_DIR",&self.dir).env("BRAINTRADE_IPC_TOKEN",read_ipc_key(&self.dir)?);command.stdin(Stdio::null()).stdout(Stdio::null());let log=OpenOptions::new().append(true).create(true).open(self.dir.join("gateway.log"))?;command.stderr(log);
        #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
        self.gateway=Some(command.spawn().context("Could not launch MT5 gateway")?);self.event("gateway_started",json!({}))?;Ok(())
    }
    fn stop_gateway(&mut self){if let Some(mut child)=self.gateway.take(){let _=child.kill();let _=child.wait();}}
    fn tick(&mut self){let cfg=self.shared.lock().unwrap().config.clone();self.studio.tick(&self.db,&cfg,now_ms());self.news.tick(&self.db,now_ms());self.backtests.tick(&self.db,now_ms());self.evaluations.tick(&self.db);{let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());s.status.execution_enabled=self.execution.enabled(&s.status);}if now_ms()>=self.gateway_retry_ms{self.gateway_retry_ms=now_ms()+15_000;if let Err(error)=self.launch_gateway(){self.shared.lock().unwrap().status.message=error.to_string();}}}
    fn route(&mut self,method:&str,path:&str,body:&[u8])->Result<Value>{
        if path=="/v1/research/execution-lifecycle" && method=="POST" {
            return research_order_lifecycle::replay(body);
        }
        if path=="/v1/research/execution-admission" && method=="POST" {
            return research_admission::evaluate(body);
        }
        if path=="/v1/research/execution-bridge" && method=="POST" {
            return research_execution_bridge::compare(body);
        }
        if path=="/v1/research/demo-deal-audit" && method=="POST" {
            return research_demo_deal_audit::audit(body);
        }
        if method=="GET" && path=="/v1/risk/monitor" {
            let (cfg,status)={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());
                (s.config.clone(),s.status.clone())};
            return risk_monitor::snapshot(&self.db,&cfg,&status,now_ms());
        }
        if path=="/v1/execution/frozen-shadow" {
            let (cfg,status)={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());(s.config.clone(),s.status.clone())};
            let per_account=status.telemetry.as_ref().map(|t|cfg.for_account(t)).unwrap_or(cfg);
            let result=self.execution.frozen_route(method,body,&self.db,&status,&per_account,now_ms())?;
            if method=="POST"{self.event("frozen_demo_shadow_action",json!({"result":result}))?;}
            return Ok(result);
        }
        if path=="/v1/execution/frozen-coordinator" {
            let (cfg,status)={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());(s.config.clone(),s.status.clone())};
            let per_account=status.telemetry.as_ref().map(|t|cfg.for_account(t)).unwrap_or(cfg);
            let result=self.execution.coordinator_twin_route(method,body,&self.db,&status,&per_account,now_ms())?;
            if method=="POST"{self.event("frozen_coordinator_policy_action",json!({"result":result}))?;}
            return Ok(result);
        }
        if path.starts_with("/v1/execution") {
            let status={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());s.status.clone()};
            let result=self.execution.route(method,path,body,&self.db,&status,now_ms())?;
            if method=="POST"&&!matches!(path,"/v1/execution/sync"|"/v1/execution/authorize"|"/v1/execution/trades") {self.event("broker_execution_action",json!({"path":path,"result":result}))?;}
            let mut s=self.shared.lock().unwrap();s.status.execution_enabled=self.execution.enabled(&s.status);
            return Ok(result);
        }

        if path=="/v1/studio"||path.starts_with("/v1/studio/")||path.starts_with("/v1/gateway/research/") {let(c,s)={let mut shared=self.shared.lock().unwrap();shared.status.refresh(now_ms());(shared.config.clone(),shared.status.clone())};let c=s.telemetry.as_ref().map(|t|c.for_account(t)).unwrap_or(c);return self.studio.route(method,path,body,&self.db,&c,&s,now_ms());}
        // Explicit single-owner priority handoff. The Sniper pause button
        // activates the preselected backup without restarting an armed Demo/Live
        // session, but ONLY when the broker is flat and reconciliation is clear.
        // Never silently hand over during an open/unknown MT5 order.
        if path=="/v1/trading/priority/handoff" && method=="POST" {
            let request:Value=serde_json::from_slice(body)?;
            let desired=request["sniper_active"].as_bool()
                .context("sniper_active must be an explicit boolean")?;
            let mut shared=self.shared.lock().unwrap();
            shared.status.refresh(now_ms());
            let running=shared.status.requested_run;
            let current=shared.status.clone();
            anyhow::ensure!(!self.execution.pending(&self.db)?,
                "تعذر التبديل: توجد أوامر معلّقة لم تُسوَّ مع الوسيط");
            if running {
                anyhow::ensure!(self.execution.enabled(&current),
                    "تعذر التبديل أثناء التشغيل: حساب MT5 غير متصل أو غير مطابق");
                let telemetry=current.telemetry.as_ref().context("MT5 telemetry missing")?;
                anyhow::ensure!(telemetry.positions.is_empty(),
                    "لا يمكن التبديل قبل تسوية وإغلاق جميع مراكز الحساب المفتوحة");
                anyhow::ensure!(telemetry.pending_orders==0,
                    "لا يمكن التحويل: توجد أوامر معلّقة في MT5");
                let now=now_ms();
                anyhow::ensure!(
                    telemetry.quote_is_fresh(now, 3000),
                    "سعر MT5 قديم أو بلا وقت موثوق؛ لا نُحوّل التنفيذ بين العقول");
                if self.execution.is_live_target() {
                    self.execution.validate_start(&current,body)?;
                }
            }
            let next=trading_settings::priority_handoff(&shared.config,desired)?;
            // Serialize all observable state transitions through the Core event
            // loop; reset strategy caches and leave requested_run unchanged.
            let handoff_event=json!({
                "sniper_active":desired,
                "active_mode":next.trading.mode,
                "fallback_mode":next.trading.fallback_mode,
                "monitoring_preserved":running,
                "flat_broker_required":running
            });
            // An audit failure must abort before changing the active owner.
            self.event("trading_priority_handoff_requested",handoff_event.clone())?;
            atomic_write(&self.dir.join("config.json"),
                &serde_json::to_vec_pretty(&next)?)?;
            self.execution.set_mode(next.trading.mode);
            shared.config=next;
            shared.status.analysis=Default::default();
            // A committed mode must be acknowledged even if the optional
            // second audit insert fails after durable persistence.
            let _=self.event("trading_priority_handoff_committed",handoff_event);
            shared.status.message=if desired {
                "القناص عاد منسقاً رئيسياً؛ البديل يراقب بدون إرسال أوامر."
            } else {
                "تم إيقاف القناص وتحويل التنفيذ للبديل المختار فقط."
            }.into();
            return Ok(trading_settings::view(&shared.config,&shared.status));
        }
        if path=="/v1/enterprise/supervisor" {
            let (status,config)={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());(s.status.clone(),s.config.clone())};
            if method=="GET" {
                let frames=status.telemetry.as_ref().map(|t|self.studio.live_frames(t,now_ms())).unwrap_or_default();
                return enterprise_supervisor::report(&self.db,status.telemetry.as_ref(),&config,&frames,now_ms());
            }
            anyhow::ensure!(method=="POST","Unsupported supervisor method");
            // Never mistake missing broker telemetry for a flat portfolio.
            // Even STOPPED policy edits require fresh, bound MT5 reconciliation.
            let flat=enterprise_supervisor::broker_flat_is_authoritative(
                status.telemetry.as_ref(),status.gateway_fresh,self.execution.account_matches(&status));
            let result=enterprise_supervisor::save(&self.db,body,!status.requested_run,flat,!self.execution.pending(&self.db)?)?;
            self.execution.refresh_supervisor(&self.db);
            self.event("enterprise_supervisor_settings_changed",json!({"settings":result["settings"],"scope":"entry_only","mode_handoff":false}))?;
            return Ok(result);
        }
        if path=="/v1/trading/settings" {
            let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());
            if method=="GET" {return Ok(trading_settings::view(&s.config,&s.status));}
            if method=="POST" {
                ensure_execution_mutation_stopped(&s.status,self.execution.pending(&self.db)?)?;
                let c=trading_settings::update(&s.config,&s.status,body)?;
                atomic_write(&self.dir.join("config.json"),&serde_json::to_vec_pretty(&c)?)?;
                self.event("trading_mode_changed",json!({"trading":c.trading,"commission_profiles":c.commission_profiles.len(),"default_commission_per_lot":c.risk.commission_per_lot}))?;
                self.execution.set_mode(c.trading.mode);
                s.config=c;s.status.analysis=Default::default();
                return Ok(trading_settings::view(&s.config,&s.status));
            }
        }
        if path=="/v1/trading/preview" && method=="POST" {
            let v:Value=serde_json::from_slice(body)?;let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());let mut c=s.config.clone();let basis=serde_json::from_value::<braintrade_domain::commission::Basis>(v.get("commission_basis").cloned().unwrap_or(json!("round_trip")))?;c.risk.commission_per_lot=basis.round_trip(v["commission_per_lot"].as_f64().context("Commission required")?).map_err(anyhow::Error::msg)?;c.trading.pip_points=v["pip_points"].as_f64().context("Pip size required")?;if let Some(price)=v.get("sniper_pip_price"){c.trading.sniper_pip_price=price.as_f64().context("Pip price must be a number")?;}if let Some(ticks)=v.get("slippage_ticks"){c.risk.slippage_ticks=ticks.as_f64().context("Slippage ticks must be a number")?;}let lots=v["lots"].as_f64().context("Lots required")?;c.validate().map_err(anyhow::Error::msg)?;if !lots.is_finite()||lots<=0.||lots>100. {bail!("Lots must be positive and at most 100");}
            let commission=braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot,lots)
                .map_err(anyhow::Error::msg)?;
            let commission_examples=json!({
                "one_lot":braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot,1.).map_err(anyhow::Error::msg)?,
                "one_tenth_lot":braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot,0.10).map_err(anyhow::Error::msg)?,
                "one_hundredth_lot":braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot,0.01).map_err(anyhow::Error::msg)?,
                "is_estimate_only":true
            });
            let preview=if s.status.gateway_fresh{s.status.telemetry.as_ref().map(|t|{let mut q=t.clone();q.tick_age_ms=q.tick_age_ms.map(|a|a.saturating_add(now_ms().saturating_sub(s.status.last_received_ms.unwrap_or(0))));braintrade_domain::trading::costs(&q,&c,lots,now_ms())})}else{None};
            return Ok(match preview {
                Some(Ok(p))=>json!({"costs":p,"commission":commission,"commission_examples":commission_examples}),
                Some(Err(e))=>json!({"commission":commission,"commission_examples":commission_examples,"error":e}),
                None=>json!({"commission":commission,"commission_examples":commission_examples,
                    "error":"Connect MT5 for actual spread and contract costs"})
            });
        }
        if path=="/v1/news"||path.starts_with("/v1/news/"){return self.news.route(method,path,body,&self.db);}
        if path=="/v1/evaluation"||path.starts_with("/v1/evaluation/"){return self.evaluations.route(method,path,body,&self.db,&self.dir);}
        if path=="/v1/learning/inference"||path.starts_with("/v1/learning/inference/"){return self.inference.route(method,path,&self.db);}
        if path=="/v1/learning/scoring"||path.starts_with("/v1/learning/scoring/"){return self.scoring.route(method,path,&self.db);}
        if path.starts_with("/v1/learning/performance-review/"){return learning_performance_review::route(method,path,body,&mut self.db,now_ms());}
        if path.starts_with("/v1/learning/review/"){return learning_review::route(method,path,body,&mut self.db,now_ms());}
        if path=="/v1/research/correlation"||path.starts_with("/v1/research/correlation/"){return correlation::route(method,path,body,&self.db,now_ms());}
        if path=="/v1/backtest"||path.starts_with("/v1/backtest/")||path.starts_with("/v1/gateway/history/"){
            let(config,status)={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());(s.config.clone(),s.status.clone())};let config=status.telemetry.as_ref().map(|t|config.for_account(t)).unwrap_or(config);let open=status.telemetry.as_ref().is_some_and(|t|self.research.has_open_position(t));return self.backtests.route(method,path,body,&self.db,&self.dir,&config,&status,open);
        }
        if method=="POST"&&path=="/v1/brain-snapshot/export"{
            let config=self.shared.lock().unwrap().config.clone();
            let result=backup::export_brain(&self.db,&self.dir,&config,now_ms())?;
            self.event("brain_snapshot_exported",json!({"file_name":result["file_name"],"bytes":result["bytes"],"checksum":result["checksum"],"secrets_included":false,"gateway_send_journal_restored":false}))?;
            return Ok(result);
        }
        if method=="POST"&&path=="/v1/brain-snapshot/import"{
            let result=backup::import_brain(&self.dir,body,now_ms())?;
            self.event("brain_snapshot_imported",json!({"backup_name":result["backup_name"],"checksum":result["checksum"],"restore_required":true,"broker_effect":false}))?;
            return Ok(result);
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
            if self.shared.lock().unwrap().status.requested_run{bail!("Stop the system before staging restore");}
            if self.backtests.loading(){bail!("Cannot stage restore while broker history is loading");}if self.execution.pending(&self.db)?||self.shared.lock().unwrap().status.telemetry.as_ref().is_some_and(|t|t.positions.iter().any(|p|braintrade_domain::execution::brain_for_magic(p.magic).is_some())){bail!("Close owned broker positions and reconcile commands before restore");}
            let config=self.shared.lock().unwrap().config.clone();
            let result=backup::stage(&self.db,&self.dir,&config,body,now_ms())?;
            self.event("backup_restore_staged",json!({"source":result["source"],"rollback":result["rollback"],"secrets_restored":false,"broker_effect":false}))?;
            self.backtests.cancel();self.evaluations.cancel();self.studio.cancel();self.stop_gateway();
            {let mut s=self.shared.lock().unwrap();s.status.requested_run=false;s.status.message="Validated restore staged. Core is stopping; reopen BrainTrade to apply it before SQLite startup.".into();s.status.refresh(now_ms());}
            self.shutting_down=true;
            return Ok(result);
        }
        match(method,path){
            ("GET","/v1/status")=>{let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());Ok(serde_json::to_value(&s.status)?)},
            ("GET","/v1/config")=>{let s=self.shared.lock().unwrap();Ok(json!({"config":s.config,"token_saved":s.token.is_some()}))},
            ("GET","/v1/learning")=>{let base=self.learning.status(&self.db)?;let reviewed=learning_review::enrich_learning_status(&self.db,base)?;let inferred=self.inference.enrich_learning_status(&self.db,reviewed)?;let scored=self.scoring.enrich_learning_status(&self.db,inferred)?;let reviewed=learning_performance_review::enrich_learning_status(&self.db,scored)?;let mut result=reviewed;result["frozen_forward_audit"]=learning_forward_audit::audit(&self.db)?;Ok(result)},
            ("POST","/v1/learning/export")=>self.learning.export(&self.db,&self.dir,now_ms()),
            ("POST","/v1/learning/register")=>{let artifact:Value=serde_json::from_slice(body)?;self.learning.register(&mut self.db,artifact,now_ms())},
            ("GET","/v1/decisions")=>research::decision_history(&self.db),
            ("GET","/v1/opportunities/history")=>research::opportunity_history(&self.db),
            ("POST","/v1/risk")=>{
                let risk:braintrade_domain::RiskConfig=serde_json::from_slice(body)?;risk.validate().map_err(anyhow::Error::msg)?;
                let pending=self.execution.pending(&self.db)?;
                let mut s=self.shared.lock().unwrap();
                ensure_execution_mutation_stopped(&s.status,pending)?;
                let mut updated=s.config.clone();updated.risk=risk;
                atomic_write(&self.dir.join("config.json"),&serde_json::to_vec_pretty(&updated)?)?;
                self.event("risk_settings_changed",json!({"risk":updated.risk,"scope":"research_and_optional_broker_execution_limits"}))?;
                s.config=updated;Ok(json!({"ok":true}))
            },
            ("POST","/v1/export")=>{
                let name=format!("research-{}-{}",now_ms(),uuid::Uuid::new_v4());let staging=self.dir.join("exports").join(format!(".{name}.tmp"));fs::create_dir_all(&staging)?;
                for table in["decisions","mode_decisions","paper_events","candles","opportunity_setups"]{let sql=match table{"opportunity_setups"=>"SELECT context,first_seen_ms,payload FROM opportunity_setups ORDER BY first_seen_ms,setup_id","decisions"=>"SELECT context,timestamp_ms,payload FROM decisions ORDER BY id","mode_decisions"=>"SELECT context,timestamp_ms,payload FROM mode_decisions ORDER BY id","paper_events"=>"SELECT context,timestamp_ms,json_object('kind',kind,'detail',json(payload)) FROM paper_events ORDER BY id",_=>"SELECT context,bar_time*1000,json_object('timeframe',timeframe,'candle',json(payload)) FROM candles ORDER BY context,timeframe,bar_time"};let mut stmt=self.db.prepare(sql)?;let mut rows=stmt.query([])?;let mut output=fs::File::create(staging.join(format!("{table}.jsonl")))?;while let Some(row)=rows.next()?{let payload:Value=serde_json::from_str(&row.get::<_,String>(2)?)?;writeln!(output,"{}",json!({"context":row.get::<_,String>(0)?,"timestamp_ms":row.get::<_,u64>(1)?,"payload":payload}))?;}output.sync_all()?;}
                atomic_write(&staging.join("README.txt"),b"BrainTrade research export. Virtual fills only; UTC timestamps in milliseconds. See Source/docs/RESEARCH.md for limitations.")?;let dest=self.dir.join("exports").join(name);fs::rename(staging,&dest)?;Ok(json!({"path":dest,"note":"Research export: candles, decisions and virtual ledger (JSONL)."}))
            },
            ("POST","/v1/config")=>{
                let request:ConfigRequest=serde_json::from_slice(body)?;request.config.validate().map_err(anyhow::Error::msg)?;
                if !request.config.mt5_path.is_empty(){let path=Path::new(&request.config.mt5_path);if !path.is_absolute()||!path.is_file()||path.extension().and_then(|v|v.to_str()).map(|v|v.to_ascii_lowercase())!=Some("exe".into()){bail!("MT5 path must point to an existing terminal64.exe file");}}
                let token=request.telegram_token.trim();if !token.is_empty()&&(token.len()<20||!token.contains(':')||token.chars().any(char::is_whitespace)){bail!("Invalid Telegram bot token format");}if request.clear_telegram_token&&!token.is_empty(){bail!("Cannot save and remove a token together");}
                let existing=self.shared.lock().unwrap().token.clone();let effective=if request.clear_telegram_token{None}else if !token.is_empty(){Some(token.to_owned())}else{existing};if request.config.telegram_enabled&&effective.is_none(){bail!("Enter a bot token before enabling Telegram");}
                let pending=self.execution.pending(&self.db)?;
                let current={let mut state=self.shared.lock().unwrap();state.status.refresh(now_ms());state.status.clone()};
                verify_safe_reconfiguration(&current,pending)?;
                self.execution.cancel_entries(&self.db)?;
                if !token.is_empty(){secrets::write(&self.dir.join("telegram.sealed"),token)?;}atomic_write(&self.dir.join("config.json"),&serde_json::to_vec_pretty(&request.config)?)?;if request.clear_telegram_token&&self.dir.join("telegram.sealed").exists(){fs::remove_file(self.dir.join("telegram.sealed"))?;}
                {let mut s=self.shared.lock().unwrap();s.config=request.config;s.token=effective;s.status.gateway_fresh=false;s.status.last_received_ms=None;s.status.telemetry=None;s.status.analysis=Default::default();s.status.paper=Default::default();s.status.message="Settings saved; reconnecting MT5".into();}
                self.event("settings_changed",json!({"secrets":"redacted"}))?;if self.backtests.loading(){self.backtests.cancel();}self.stop_gateway();self.gateway_retry_ms=0;Ok(json!({"ok":true}))
            },
            ("POST","/v1/start")=>{
                if self.backtests.loading(){bail!("جلب التاريخ قيد العمل؛ انتظر أو ألغِ الاختبار قبل تشغيل المحاكاة");}
                let status={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());s.status.clone()};
                self.execution.validate_start(&status,body)?;
                self.event("monitoring_requested",json!({"broker_execution_possible":true,"requires_fresh_bound_account":true,
                    "real_login_confirmed":self.execution.is_live_target()}))?;
                let mut s=self.shared.lock().unwrap();
                s.status.requested_run=true;
                s.status.message="بدأ النظام. منسّق الوضع المختار وحده يرسل أوامر إلى الحساب المرتبط الحديث.".into();
                s.status.refresh(now_ms());
                Ok(serde_json::to_value(&s.status)?)
            },
            ("POST","/v1/stop")=>{self.execution.cancel_entries(&self.db)?;self.event("monitoring_stopped",json!({"positions_unchanged":true}))?;let mut s=self.shared.lock().unwrap();s.status.requested_run=false;s.status.analysis.action="WAIT".into();s.status.analysis.gates.push("System stopped — new virtual entries disabled".into());s.status.message="New entries stopped. Existing owned positions retain broker SL/TP and timed exits while Core runs.".into();s.status.refresh(now_ms());Ok(serde_json::to_value(&s.status)?)},
            ("POST","/v1/reconnect")=>{
                anyhow::ensure!(!self.execution.pending(&self.db)?,
                    "تعذر إعادة الاتصال: توجد أوامر تنفيذ غير محسومة؛ انتظر المصالحة أولاً");
                // Recovery may be necessary with open MT5 positions. The user
                // is warned in UI; broker reconciliation remains authoritative.
                self.event("reconnect_requested",json!({"gateway_restart":true}))?;
                self.stop_gateway();self.gateway_retry_ms=0;
                {let mut s=self.shared.lock().unwrap();
                 // Preserve the previous broker-session marker; otherwise a
                 // reconnect with open positions could erase the evidence and
                 // let /v1/config treat it as a never-connected installation.
                 s.status.gateway_fresh=false;
                 s.status.telemetry=None;
                 s.status.refresh(now_ms());}
                Ok(json!({"ok":true}))
            },
            ("POST","/v1/gateway/snapshot")=>{
                let telemetry:Telemetry=serde_json::from_slice(body)?;if !telemetry.validate(){bail!("Invalid telemetry");}let(config,run)={let s=self.shared.lock().unwrap();(s.config.for_account(&telemetry),s.status.requested_run)};let observed_ms=now_ms();let mut next=self.research.clone();let frames=self.studio.live_frames(&telemetry,observed_ms);let(analysis,paper)=next.update_with_frames(&mut self.db,&telemetry,&config,run,observed_ms,&frames)?;self.learning.observe(&mut self.db,&telemetry,&analysis,observed_ms);self.inference.observe(&self.db,&telemetry,&analysis,observed_ms);self.scoring.observe(&self.db,observed_ms);self.research=next;let execution_frames=self.studio.execution_frames(&telemetry,observed_ms);self.execution.observe(&self.db,&telemetry,&config,run,&execution_frames,observed_ms)?;let mut s=self.shared.lock().unwrap();let changed=s.status.telemetry.as_ref().map(|t|t.connected)!=Some(telemetry.connected);if changed{self.event("gateway_connection",json!({"connected":telemetry.connected}))?;}if !telemetry.error.is_empty(){s.status.message=telemetry.error.clone();}else{s.status.message="اتصال MT5 قائم. الوضع المختار وحده ينفذ، وبقية العقول للمشورة أو التدريب الافتراضي.".into();}s.status.analysis=analysis;s.status.paper=paper;s.status.telemetry=Some(telemetry);s.status.last_received_ms=Some(observed_ms);s.status.refresh(now_ms());s.status.execution_enabled=self.execution.enabled(&s.status);Ok(json!({"ok":true}))
            },
            ("GET","/v1/events")=>{let mut statement=self.db.prepare("SELECT id,timestamp_ms,kind,detail FROM events ORDER BY id DESC LIMIT 100")?;let events=statement.query_map([],|r|Ok(Event{id:r.get(0)?,timestamp_ms:r.get(1)?,kind:r.get(2)?,detail:serde_json::from_str(&r.get::<_,String>(3)?).unwrap_or(Value::Null)}))?.collect::<std::result::Result<Vec<_>,_>>()?;Ok(json!({"events":events}))},
            ("POST","/v1/shutdown")=>{
                let unresolved=self.execution.pending(&self.db)?;
                let current={let mut s=self.shared.lock().unwrap();s.status.refresh(now_ms());s.status.clone()};
                verify_safe_shutdown(&current,unresolved)?;
                self.event("core_shutdown_requested",json!({"safe":true,"positions":0,"pending_orders":0}))?;
                self.shutting_down=true;
                Ok(json!({"ok":true,"safe":true}))
            },
            _=>bail!("Unknown API route")
        }
    }
}

pub fn run_core()->Result<()>{
    let dir=data_dir();fs::create_dir_all(&dir)?;
    #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(&dir,fs::Permissions::from_mode(0o700))?;}
    let lock=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join("core.lock"))?;if lock.try_lock_exclusive().is_err(){return Ok(());}let restore_result=backup::apply_pending(&dir)?;let key=ipc_key(&dir)?;let config=read_config(&dir).context("Config invalid: preserve file and correct it; refusing silent reset")?;let db=Connection::open(dir.join("events.sqlite"))?;configure_database(&db)?;let research=research::Research::load(&db)?;let backtests=backtest::Service::new(&db)?;correlation::init(&db)?;let evaluations=evaluation::Service::new(&db)?;let news=news::Service::new(&db)?;let learning=learning::Service::new(&db)?;learning_review::init(&db)?;let inference=learning_inference::Service::new(&db)?;let scoring=learning_scoring::Service::new(&db)?;let studio=studio::Service::new(&db)?;let execution=execution::Service::new(&db)?;execution.cancel_entries(&db)?;learning_performance_review::init(&db)?;verify_foreign_key_integrity(&db)?;let token=if dir.join("telegram.sealed").exists(){Some(secrets::read(&dir.join("telegram.sealed"))?)}else{None};let shared=Arc::new(Mutex::new(Shared{status:Status::default(),config,token}));let server=tiny_http::Server::http(("127.0.0.1",PORT)).map_err(|e|anyhow::anyhow!(e.to_string()))?;let mut engine=Engine{dir,shared:shared.clone(),db,gateway:None,gateway_retry_ms:0,shutting_down:false,research,backtests,evaluations,news,learning,inference,scoring,studio,execution};engine.event("core_started",json!({"version":braintrade_domain::VERSION,"execution_enabled":false,"challenger_inference":"read_only","prediction_scoring":"audit_only","challenger_performance_review":"manual_read_only","backup_restore":"validated_staged_restart_rollback","foreign_keys":true}))?;if let Some(result)=restore_result{engine.event("backup_restore_result",result)?;}telegram::spawn(shared);
    while !engine.shutting_down{engine.tick();let Some(mut request)=server.recv_timeout(Duration::from_millis(250))? else{continue;};let authenticated=request.headers().iter().any(|h|h.field.equiv("Authorization")&&h.value.as_str()==format!("Bearer {key}"));let origin=request.headers().iter().any(|h|h.field.equiv("Origin"));if !authenticated||origin{let _=request.respond(tiny_http::Response::from_string("{\"error\":\"Unauthorized\"}").with_status_code(401));continue;}if request.body_length().is_none()&&request.method().as_str()=="POST"{let _=request.respond(tiny_http::Response::empty(411));continue;}if request.body_length().unwrap_or(0)>2_000_000{let _=request.respond(tiny_http::Response::empty(413));continue;}let mut body=Vec::new();if request.as_reader().read_to_end(&mut body).is_err(){continue;}let result=engine.route(request.method().as_str(),request.url(),&body);let(code,value)=match result{Ok(v)=>(200,v),Err(e)=>(400,json!({"error":e.to_string()}))};let response=tiny_http::Response::from_string(value.to_string()).with_status_code(code).with_header(tiny_http::Header::from_bytes("Content-Type","application/json").unwrap());let _=request.respond(response);}
    engine.stop_gateway();engine.backtests.cancel();engine.evaluations.cancel();engine.event("core_stopped",json!({}))?;drop(lock);Ok(())
}
pub fn ensure_core()->Result<Client>{
    let dir=data_dir();if let Ok(client)=Client::new(&dir){if let Ok(status)=client.get("/v1/status"){if status["version"].as_str()!=Some(braintrade_domain::VERSION){bail!("An older Core is running. Open the old application, choose Full Exit / خروج كامل, then launch this version.");}return Ok(client);}}
    let mut command=Command::new(std::env::current_exe()?);fs::create_dir_all(&dir)?;command.arg("--core").stdin(Stdio::null()).stdout(Stdio::null()).stderr(OpenOptions::new().append(true).create(true).open(dir.join("core.log"))?);
    #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000008);}
    command.spawn()?;for _ in 0..100{std::thread::sleep(Duration::from_millis(100));if let Ok(client)=Client::new(&dir){if client.get("/v1/status").is_ok(){return Ok(client);}}}bail!("Core did not become ready. See {}",dir.join("core.log").display())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn full_exit_is_blocked_if_monitoring_or_account_is_unverified(){
        let mut s=Status::default();
        assert!(verify_safe_shutdown(&s,false).is_ok());
        assert!(verify_safe_shutdown(&s,true).is_err());
        s.requested_run=true;
        assert!(verify_safe_shutdown(&s,false).is_err());
        s.requested_run=false;
        s.last_received_ms=Some(123);
        assert!(verify_safe_shutdown(&s,false).is_err());
        s.telemetry=Some(braintrade_domain::Telemetry::default());
        assert!(verify_safe_shutdown(&s,false).is_err());
        s.gateway_fresh=true;
        s.telemetry.as_mut().unwrap().connected=true;
        assert!(verify_safe_shutdown(&s,false).is_ok());
        s.telemetry.as_mut().unwrap().pending_orders=1;
        assert!(verify_safe_shutdown(&s,false).is_err());
    }
    #[test]
    fn account_reconfiguration_requires_flat_stopped_broker(){
        let mut s=Status::default();
        assert!(verify_safe_reconfiguration(&s,false).is_ok());
        assert!(verify_safe_reconfiguration(&s,true).is_err());
        s.requested_run=true;
        assert!(verify_safe_reconfiguration(&s,false).is_err());
        s.requested_run=false;
        s.telemetry=Some(braintrade_domain::Telemetry::default());
        s.telemetry.as_mut().unwrap().pending_orders=1;
        assert!(verify_safe_reconfiguration(&s,false).is_err());
        s.telemetry.as_mut().unwrap().pending_orders=0;
        assert!(verify_safe_reconfiguration(&s,false).is_err(),"Disconnected broker must not be considered flat");
        s.telemetry.as_mut().unwrap().connected=true;
        s.gateway_fresh=true;
        assert!(verify_safe_reconfiguration(&s,false).is_ok());
        s.gateway_fresh=false;
        assert!(verify_safe_reconfiguration(&s,false).is_err(),"Stale broker state must not authorize reconfiguration");
        s.telemetry=None;
        s.last_received_ms=Some(123);
        assert!(verify_safe_reconfiguration(&s,false).is_err(),"Lost portfolio after reconnect must not authorize account switch");
    }
    #[test]
    fn atomic_config_replacement(){let dir=std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());fs::create_dir_all(&dir).unwrap();let p=dir.join("config.json");atomic_write(&p,b"first").unwrap();atomic_write(&p,b"second").unwrap();assert_eq!(fs::read(&p).unwrap(),b"second");fs::remove_dir_all(dir).unwrap();}
    #[test]
    fn sqlite_foreign_keys_are_enforced(){let db=Connection::open_in_memory().unwrap();configure_database(&db).unwrap();let enabled:i64=db.query_row("PRAGMA foreign_keys",[],|row|row.get(0)).unwrap();assert_eq!(enabled,1);db.execute_batch("CREATE TABLE parent(id INTEGER PRIMARY KEY); CREATE TABLE child(id INTEGER PRIMARY KEY,parent_id INTEGER NOT NULL REFERENCES parent(id));").unwrap();assert!(db.execute("INSERT INTO child(id,parent_id) VALUES(1,99)",[]).is_err());db.execute("INSERT INTO parent(id) VALUES(99)",[]).unwrap();db.execute("INSERT INTO child(id,parent_id) VALUES(1,99)",[]).unwrap();verify_foreign_key_integrity(&db).unwrap();}
    #[test]
    fn execution_affecting_config_requires_stopped_and_reconciled(){let mut status=Status::default();assert!(ensure_execution_mutation_stopped(&status,false).is_ok());status.requested_run=true;assert!(ensure_execution_mutation_stopped(&status,false).is_err());status.requested_run=false;assert!(ensure_execution_mutation_stopped(&status,true).is_err());}
}
