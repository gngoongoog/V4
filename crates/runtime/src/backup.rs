use anyhow::{bail, Context, Result};
use braintrade_domain::Config;
use rusqlite::{Connection, DatabaseName, OpenFlags};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fs::{self, File}, io::Read, path::{Path, PathBuf}};

const SCHEMA:u64=10;
const HASH:&str="fnv1a64-v1";
const PENDING:&str="restore.pending.json";

#[derive(Deserialize)] struct RestoreRequest{name:String}
struct Validated{name:String,dir:PathBuf,manifest:Value,manifest_hash:String}

fn safe_name(name:&str)->Result<()>{
    if name.is_empty()||name.len()>160||name=="."||name==".."||
       !name.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_'|b'.')){bail!("Invalid backup name");}
    Ok(())
}
fn hash_file(path:&Path)->Result<(u64,String)>{
    let mut f=File::open(path)?;let mut h=0xcbf29ce484222325u64;let mut bytes=0u64;let mut buf=[0u8;65536];
    loop{let n=f.read(&mut buf)?;if n==0{break;}bytes+=n as u64;for b in &buf[..n]{h^=u64::from(*b);h=h.wrapping_mul(0x100000001b3);}}
    Ok((bytes,format!("{h:016x}")))
}
fn record(path:&Path)->Result<Value>{let(bytes,hash)=hash_file(path)?;Ok(json!({"bytes":bytes,"hash":hash}))}
fn cleanup_sqlite_sidecars(path:&Path)->Result<()>{
    let base=path.to_string_lossy();
    for suffix in ["-wal","-shm","-journal"]{let sidecar=PathBuf::from(format!("{base}{suffix}"));if sidecar.exists(){fs::remove_file(sidecar)?;}}
    Ok(())
}
fn valid_config(path:&Path)->Result<()>{
    let c:Config=serde_json::from_slice(&fs::read(path)?).context("Invalid backup config.json")?;
    c.validate().map_err(anyhow::Error::msg)
}
fn valid_db(path:&Path)->Result<()>{
    let db=Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .context("Backup SQLite cannot be opened read-only")?;
    let ok:String=db.query_row("PRAGMA integrity_check",[],|r|r.get(0))?;if ok!="ok"{bail!("SQLite integrity_check failed: {ok}");}
    let mut st=db.prepare("PRAGMA foreign_key_check")?;let mut rows=st.query([])?;
    if let Some(r)=rows.next()?{let table:String=r.get(0)?;let rowid:Option<i64>=r.get(1)?;bail!("Foreign-key check failed in {table} at {rowid:?}");}
    let events:i64=db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",[],|r|r.get(0))?;
    if events!=1{bail!("Required events table missing");}Ok(())
}
fn folder(root:&Path,name:&str)->Result<PathBuf>{safe_name(name)?;Ok(root.join("backups").join(name))}
fn validate(root:&Path,name:&str)->Result<Validated>{
    let dir=folder(root,name)?;if !dir.is_dir(){bail!("Backup does not exist: {name}");}
    let mut members=Vec::new();for e in fs::read_dir(&dir)?{let e=e?;if !e.file_type()?.is_file(){bail!("Backup contains unsupported directory/member");}let n=e.file_name().to_string_lossy().to_ascii_lowercase();if !matches!(n.as_str(),"events.sqlite"|"config.json"|"manifest.json"){bail!("Unexpected backup member: {n}");}members.push(n);}members.sort();members.dedup();if members.len()!=3{bail!("Backup member coverage mismatch");}
    let mp=dir.join("manifest.json");let m:Value=serde_json::from_slice(&fs::read(&mp)?).context("Invalid backup manifest")?;
    if m["schema"].as_u64()!=Some(SCHEMA){bail!("Unsupported backup schema; expected {SCHEMA}");}
    if m["version"].as_str()!=Some(braintrade_domain::VERSION){bail!("Backup application version does not match this build");}
    if m["hash_algorithm"].as_str()!=Some(HASH)||m["secrets_included"].as_bool()!=Some(false)||m["restore_implemented"].as_bool()!=Some(true){bail!("Backup manifest contract mismatch");}
    let files=m["files"].as_object().context("Backup files map missing")?;
    for n in ["events.sqlite","config.json"]{
        let expected=files.get(n).with_context(||format!("Manifest missing {n}"))?;let p=dir.join(n);if !p.is_file(){bail!("Backup file missing: {n}");}
        let(bytes,hash)=hash_file(&p)?;if expected["bytes"].as_u64()!=Some(bytes)||expected["hash"].as_str()!=Some(hash.as_str()){bail!("Backup hash/size mismatch: {n}");}
    }
    valid_config(&dir.join("config.json"))?;valid_db(&dir.join("events.sqlite"))?;let(_,manifest_hash)=hash_file(&mp)?;
    Ok(Validated{name:name.to_owned(),dir,manifest:m,manifest_hash})
}
fn create_named(db:&Connection,root:&Path,config:&Config,now:u64,prefix:&str)->Result<Value>{
    fs::create_dir_all(root.join("backups"))?;let name=format!("{prefix}-{now}-{}",uuid::Uuid::new_v4().simple());
    let staging=root.join("backups").join(format!(".{name}.tmp"));fs::create_dir_all(&staging)?;
    let dp=staging.join("events.sqlite");db.backup(DatabaseName::Main,&dp,None)?;valid_db(&dp)?;cleanup_sqlite_sidecars(&dp)?;
    let cp=staging.join("config.json");super::atomic_write(&cp,&serde_json::to_vec_pretty(config)?)?;
    let m=json!({"schema":SCHEMA,"version":braintrade_domain::VERSION,"created_ms":now,
        "kind":"settings-audit-candles-decisions-shadow-backtests-chronological-evaluations-news-learning-dataset-registry-review-eligibility-read-only-predictions-scored-outcomes-challenger-performance-review",
        "hash_algorithm":HASH,"files":{"events.sqlite":record(&dp)?,"config.json":record(&cp)?},
        "secrets_included":false,"learning_included":true,"chronological_evaluations_included":true,"trained_model_included":true,
        "learning_review_records_included":true,"learning_predictions_included":true,"learning_prediction_outcomes_included":true,
        "learning_performance_reviews_included":true,"read_only_inference_only":true,"shadow_model_effect":false,"promotion_enabled":false,
        "restore_implemented":true,"restore_strategy":"validate-stage-restart-rollback",
        "hash_security_note":"FNV-1a is a corruption checksum, not a cryptographic authenticity signature."});
    super::atomic_write(&staging.join("manifest.json"),&serde_json::to_vec_pretty(&m)?)?;
    let dest=root.join("backups").join(&name);fs::rename(&staging,&dest)?;let v=validate(root,&name)?;
    Ok(json!({"name":name,"path":dest,"schema":SCHEMA,"manifest_hash":v.manifest_hash,
        "note":"Validated backup: config + SQLite audit/research state; secrets excluded; staged rollback restore supported."}))
}
pub fn create(db:&Connection,root:&Path,config:&Config,now:u64)->Result<Value>{create_named(db,root,config,now,"foundation")}
pub fn list(root:&Path)->Result<Value>{
    let b=root.join("backups");fs::create_dir_all(&b)?;let mut items=Vec::new();
    for e in fs::read_dir(&b)?{let e=e?;if !e.file_type()?.is_dir(){continue;}let n=e.file_name().to_string_lossy().into_owned();if n.starts_with('.')||safe_name(&n).is_err(){continue;}
        match validate(root,&n){Ok(v)=>items.push(json!({"name":v.name,"created_ms":v.manifest["created_ms"],"schema":v.manifest["schema"],"valid":true,"restore_supported":true})),
            Err(err)=>items.push(json!({"name":n,"valid":false,"restore_supported":false,"error":err.to_string()}))}
    }
    items.sort_by(|a,b|b["created_ms"].as_u64().unwrap_or(0).cmp(&a["created_ms"].as_u64().unwrap_or(0)));
    Ok(json!({"schema":SCHEMA,"restore_implemented":true,"restore_requires_restart":true,"backups":items}))
}
pub fn validate_request(root:&Path,body:&[u8])->Result<Value>{
    let r:RestoreRequest=serde_json::from_slice(body)?;let v=validate(root,r.name.trim())?;
    Ok(json!({"valid":true,"name":v.name,"schema":v.manifest["schema"],"created_ms":v.manifest["created_ms"],
        "manifest_hash":v.manifest_hash,"restore_requires_restart":true,"secrets_restored":false}))
}
pub fn stage(db:&Connection,root:&Path,config:&Config,body:&[u8],now:u64)->Result<Value>{
    let pp=root.join(PENDING);if pp.exists(){bail!("A restore is already staged");}
    let r:RestoreRequest=serde_json::from_slice(body)?;let source=validate(root,r.name.trim())?;
    let rollback=create_named(db,root,config,now,"pre-restore")?;let rb=rollback["name"].as_str().context("Rollback name missing")?.to_owned();
    let p=json!({"schema":1,"staged_ms":now,"source":source.name,"source_manifest_hash":source.manifest_hash,"rollback":rb,
        "secrets_restored":false,"strategy":"validate-stage-restart-rollback"});
    super::atomic_write(&pp,&serde_json::to_vec_pretty(&p)?)?;
    Ok(json!({"staged":true,"source":p["source"],"rollback":p["rollback"],"restart_required":true,"core_will_shutdown":true,
        "secrets_restored":false,"note":"Restore validated and staged; next Core start applies it before SQLite opens and rolls back on failure."}))
}
pub fn pending_status(root:&Path)->Result<Value>{
    let p=root.join(PENDING);if !p.exists(){return Ok(json!({"pending":false}));}
    Ok(json!({"pending":true,"restore":serde_json::from_slice::<Value>(&fs::read(p)?)?}))
}
fn replace(v:&Validated,root:&Path)->Result<()>{
    let id=uuid::Uuid::new_v4().simple().to_string();let td=root.join(format!(".events.sqlite.restore-{id}.tmp"));let tc=root.join(format!(".config.json.restore-{id}.tmp"));
    fs::copy(v.dir.join("events.sqlite"),&td)?;fs::copy(v.dir.join("config.json"),&tc)?;valid_db(&td)?;valid_config(&tc)?;
    for s in ["-wal","-shm"]{let p=root.join(format!("events.sqlite{s}"));if p.exists(){fs::remove_file(p)?;}}
    let ld=root.join("events.sqlite");let lc=root.join("config.json");if ld.exists(){fs::remove_file(&ld)?;}fs::rename(&td,&ld)?;
    if lc.exists(){fs::remove_file(&lc)?;}fs::rename(&tc,&lc)?;valid_db(&ld)?;valid_config(&lc)?;Ok(())
}
fn archive(root:&Path,result:&Value)->Result<()>{
    let f=root.join(format!("restore.failed-{}-{}.json",super::now_ms(),uuid::Uuid::new_v4().simple()));
    super::atomic_write(&f,&serde_json::to_vec_pretty(result)?)?;let p=root.join(PENDING);if p.exists(){fs::remove_file(p)?;}Ok(())
}
pub fn apply_pending(root:&Path)->Result<Option<Value>>{
    let pp=root.join(PENDING);if !pp.exists(){return Ok(None);}let p:Value=serde_json::from_slice(&fs::read(&pp)?)?;
    let sn=p["source"].as_str().context("Staged source missing")?;let rn=p["rollback"].as_str().context("Staged rollback missing")?;
    let expected=p["source_manifest_hash"].as_str().context("Staged source hash missing")?;
    let source=match validate(root,sn){Ok(v) if v.manifest_hash==expected=>v,Ok(_)=>{
        let r=json!({"status":"rejected_before_apply","source":sn,"reason":"Source manifest changed after staging","rollback_used":false});archive(root,&r)?;return Ok(Some(r));},
        Err(e)=>{let r=json!({"status":"rejected_before_apply","source":sn,"reason":e.to_string(),"rollback_used":false});archive(root,&r)?;return Ok(Some(r));}};
    let rollback=validate(root,rn).context("Rollback backup invalid; refusing restore")?;
    if let Err(e)=replace(&source,root){let reason=e.to_string();replace(&rollback,root).context("Restore and automatic rollback both failed")?;
        let r=json!({"status":"rolled_back","source":source.name,"rollback":rollback.name,"reason":reason,"rollback_used":true});archive(root,&r)?;return Ok(Some(r));}
    let r=json!({"status":"applied","source":source.name,"rollback":rollback.name,"rollback_used":false,"secrets_restored":false,
        "restore_strategy":"validate-stage-restart-rollback"});let last=root.join("restore.last.json");if last.exists(){fs::remove_file(&last)?;}super::atomic_write(&last,&serde_json::to_vec_pretty(&r)?)?;fs::remove_file(pp)?;Ok(Some(r))
}

#[cfg(test)] mod tests{
    use super::*;
    fn root()->PathBuf{let p=std::env::temp_dir().join(format!("braintrade-backup-test-{}",uuid::Uuid::new_v4().simple()));fs::create_dir_all(&p).unwrap();p}
    fn db(root:&Path)->Connection{let d=Connection::open(root.join("events.sqlite")).unwrap();d.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE events(id INTEGER PRIMARY KEY,timestamp_ms INTEGER NOT NULL,kind TEXT NOT NULL,detail TEXT NOT NULL);").unwrap();d}
    #[test] fn corruption_is_rejected(){
        let r=root();let d=db(&r);d.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(1,'x','{}')",[]).unwrap();let c=Config::default();
        crate::atomic_write(&r.join("config.json"),&serde_json::to_vec_pretty(&c).unwrap()).unwrap();let b=create(&d,&r,&c,1).unwrap();let n=b["name"].as_str().unwrap();
        fs::write(r.join("backups").join(n).join("config.json"),b"{}").unwrap();assert!(validate(&r,n).is_err());drop(d);let _=fs::remove_dir_all(r);
    }
    #[test] fn staged_restore_recovers_source(){
        let r=root();let d=db(&r);d.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(1,'source','{}')",[]).unwrap();let c=Config::default();
        crate::atomic_write(&r.join("config.json"),&serde_json::to_vec_pretty(&c).unwrap()).unwrap();let b=create(&d,&r,&c,2).unwrap();
        d.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(2,'newer','{}')",[]).unwrap();
        let body=serde_json::to_vec(&json!({"name":b["name"]})).unwrap();stage(&d,&r,&c,&body,3).unwrap();drop(d);assert_eq!(apply_pending(&r).unwrap().unwrap()["status"],"applied");
        let d=Connection::open(r.join("events.sqlite")).unwrap();let count:i64=d.query_row("SELECT COUNT(*) FROM events",[],|x|x.get(0)).unwrap();assert_eq!(count,1);drop(d);let _=fs::remove_dir_all(r);
    }
}
