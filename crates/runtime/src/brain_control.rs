//! Account/symbol/mode scoped advisor controls and explicitly sampled activity.
use anyhow::Result;
use braintrade_domain::{execution::{Signal, BRAINS}, trading::TradingMode};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control { pub advisor: bool, pub paper: bool }
impl Default for Control {fn default()->Self {Self{advisor:true,paper:true}}}
pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS brain_controls(context TEXT NOT NULL,mode TEXT NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,mode,brain));
        CREATE TABLE IF NOT EXISTS brain_activity(context TEXT NOT NULL,mode TEXT NOT NULL,brain TEXT NOT NULL,checks INTEGER NOT NULL,signals INTEGER NOT NULL,last_key TEXT NOT NULL,last_ms INTEGER NOT NULL,PRIMARY KEY(context,mode,brain));")?;
    Ok(())
}
fn mode_key(mode:TradingMode)->String {serde_json::to_string(&mode).unwrap()}
pub fn load(db:&Connection,key:&str,mode:TradingMode)->Result<BTreeMap<String,Control>> {
    let mut stmt=db.prepare("SELECT brain,payload FROM brain_controls WHERE context=?1 AND mode=?2")?;
    let rows=stmt.query_map(params![key,mode_key(mode)],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id,p)|Ok((id,serde_json::from_str(&p)?))).collect()
}
pub fn save(db:&Connection,key:&str,mode:TradingMode,id:&str,c:Control)->Result<()> {
    anyhow::ensure!(id!="system" && BRAINS.iter().any(|(b,_)|*b==id),"اختر عقلاً فرعياً معروفاً");
    anyhow::ensure!(!key.is_empty(),"اربط حساب MT5 أولاً");
    db.execute("INSERT OR REPLACE INTO brain_controls VALUES(?1,?2,?3,?4)",params![key,mode_key(mode),id,serde_json::to_string(&c)?])?;
    Ok(())
}
/// Called at most once per second, outside the per-tick accounting path. Counts
/// ready observations with a new key/direction, NOT fills or every broker tick.
pub fn sample(db:&Connection,key:&str,mode:TradingMode,id:&str,s:&Signal,now:u64)->Result<()> {
    let mark=if s.ready && !s.key.is_empty() {format!("{}:{}",s.key,s.side)}else{String::new()};
    db.execute("INSERT INTO brain_activity VALUES(?1,?2,?3,1,CASE WHEN ?4='' THEN 0 ELSE 1 END,?4,?5)
        ON CONFLICT(context,mode,brain) DO UPDATE SET checks=checks+1,
        signals=signals+CASE WHEN excluded.last_key<>'' AND excluded.last_key<>last_key THEN 1 ELSE 0 END,
        last_key=CASE WHEN excluded.last_key='' THEN last_key ELSE excluded.last_key END,last_ms=excluded.last_ms",
        params![key,mode_key(mode),id,mark,now])?;
    Ok(())
}
pub fn enrich(value:&mut Value,db:&Connection,key:&str,mode:TradingMode)->Result<()> {
    let controls=load(db,key,mode)?;
    value["trading_mode"]=json!(mode);
    value["activity_note"]=json!("الإشارات: رصد جاهزية بعينة واحدة/ثانية مع حذف تكرار مفتاح الإشارة. ليست عدداً لكل نبضة سعر ولا أوامر منفذة. الإحصاءات أدناه للحساب والرمز والوضع الحالي فقط.");
    for b in value["brains"].as_array_mut().into_iter().flatten() {
        let id=b["config"]["id"].as_str().unwrap_or("").to_owned();
        b["control"]=json!(controls.get(&id).copied().unwrap_or_default());
        b["participates_in_coordinator"]=json!(id!="system" && controls.get(&id).is_none_or(|c|c.advisor) && b["return_pending"]!=true && mode==TradingMode::Expert);
        let activity:Option<(u64,u64,u64)>=db.query_row("SELECT checks,signals,last_ms FROM brain_activity WHERE context=?1 AND mode=?2 AND brain=?3",params![key,mode_key(mode),id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (checks,signals,last)=activity.unwrap_or_default();
        b["activity"]=json!({"checks":checks,"signals":signals,"last_ms":last});
        let (closed,wins,net):(u64,u64,f64)=db.query_row("SELECT COUNT(*),COALESCE(SUM(CASE WHEN json_extract(payload,'$.net_paper')>0 THEN 1 ELSE 0 END),0),COALESCE(SUM(json_extract(payload,'$.net_paper')),0.0) FROM paper_brain_outcomes WHERE context=?1 AND brain=?2 AND json_extract(payload,'$.trading_mode_at_open')=?3",params![key,id,serde_json::to_value(mode)?.as_str().unwrap()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let open:u64=db.query_row("SELECT COUNT(*) FROM paper_brain_positions WHERE context=?1 AND brain=?2",params![key,id],|r|r.get(0))?;
        b["paper_stats"]=json!({"closed":closed,"wins":wins,"net":net,"open_all_modes":open});
        let (orders,fills):(u64,u64)=db.query_row("SELECT COUNT(*),COALESCE(SUM(CASE WHEN CAST(json_extract(result,'$.deal') AS INTEGER)>0 THEN 1 ELSE 0 END),0) FROM broker_commands WHERE context=?1 AND kind='open' AND json_extract(payload,'$.trading_mode')=?2 AND COALESCE(json_extract(payload,'$.signal.coordination.selection.brain'),brain)=?3",params![key,serde_json::to_value(mode)?.as_str().unwrap(),id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        b["attributed_orders"]=json!({"requested":orders,"fill_acknowledged":fills});
    }
    Ok(())
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn controls_are_scoped_and_samples_are_deduplicated_across_restart() {
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        save(&db,"account-a",TradingMode::Sniper,"sniper_learner",Control{advisor:false,paper:true}).unwrap();
        assert!(!load(&db,"account-a",TradingMode::Sniper).unwrap()["sniper_learner"].advisor);
        assert!(load(&db,"account-b",TradingMode::Sniper).unwrap().is_empty());
        assert!(load(&db,"account-a",TradingMode::Expert).unwrap().is_empty());
        let mut s=Signal{brain:"trend".into(),side:1,key:"bar:1".into(),bar_time:1,stop_distance:1.,target_distance:2.,hold_ms:1000,reason:String::new(),ready:true,coordination:None};
        sample(&db,"a",TradingMode::Expert,"trend",&s,1000).unwrap();
        init(&db).unwrap();sample(&db,"a",TradingMode::Expert,"trend",&s,2000).unwrap();
        s.ready=false;sample(&db,"a",TradingMode::Expert,"trend",&s,3000).unwrap();
        s.ready=true;sample(&db,"a",TradingMode::Expert,"trend",&s,4000).unwrap();
        let counts:(u64,u64)=db.query_row("SELECT checks,signals FROM brain_activity",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();assert_eq!(counts,(4,1));
        assert!(save(&db,"a",TradingMode::Expert,"system",Control::default()).is_err());
    }
}
