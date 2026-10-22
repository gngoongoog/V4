//! Frozen non-system advisor Shadow trial (Demo-only): immutable policy
//! parameters and an isolated, quote-observed paper ledger. NEVER broker orders.
use anyhow::{bail,ensure,Result};
use braintrade_domain::{
    execution::{self,BrainConfig,Signal},studio::{FrameBar,Genome},
    trading::TradingMode,Config,Telemetry
};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use std::collections::BTreeMap;

const POLICY:&str="frozen-advisor-demo-shadow-v1";
fn mode_key(mode:TradingMode)->Result<String>{
    Ok(serde_json::to_value(mode)?.as_str().unwrap_or("unknown").to_owned())
}
fn paper_id(id:&str)->String {format!("frozen_{id}")}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS frozen_shadow_runs(
           id TEXT PRIMARY KEY,context TEXT NOT NULL,mode TEXT NOT NULL,
           brain TEXT NOT NULL,created_ms INTEGER NOT NULL,
           snapshot TEXT NOT NULL,active INTEGER NOT NULL CHECK(active IN (0,1)));
         CREATE INDEX IF NOT EXISTS frozen_shadow_scope_idx
           ON frozen_shadow_runs(context,mode,brain,created_ms DESC);
         CREATE TABLE IF NOT EXISTS frozen_shadow_signal_pairs(
           run_id TEXT NOT NULL,bar_time INTEGER NOT NULL,observed_ms INTEGER NOT NULL,
           frozen TEXT NOT NULL,adaptive TEXT NOT NULL,
           PRIMARY KEY(run_id,bar_time),
           FOREIGN KEY(run_id) REFERENCES frozen_shadow_runs(id));"
    )?;
    Ok(())
}
pub fn freeze(db:&Connection,context:&str,mode:TradingMode,b:&BrainConfig,
    cfg:&Config,genome:Option<&Genome>,now:u64)->Result<Value>{
    ensure!(!context.is_empty(),"يجب ربط حساب MT5 Demo");
    ensure!(mode==TradingMode::Expert && cfg.trading.mode==mode,
        "التجميد التجريبي متاح لعقل مستشار في وضع الخبير فقط");
    ensure!(b.id!="system"&&b.id!="sniper_learner"
        &&execution::magic(&b.id).is_some(),
        "اختر عقلاً فرعياً قابلاً للتقييم، وليس القناص أو النظام");
    b.validate().map_err(anyhow::Error::msg)?;
    cfg.validate().map_err(anyhow::Error::msg)?;
    ensure!(b.id!="generated" || genome.is_some(),
        "العقل المولّد يحتاج نسخة Genome مثبتة");
    let mode=mode_key(mode)?;
    let exists:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM frozen_shadow_runs
         WHERE context=?1 AND mode=?2 AND brain=?3 AND active=1)",
        params![context,mode,b.id],|r|r.get(0))?;
    ensure!(!exists,"توجد تجربة مجمّدة نشطة؛ أوقفها أولاً قبل إنشاء نسخة جديدة");
    let run_id=uuid::Uuid::new_v4().simple().to_string();
    let snapshot=json!({"brain":b,"config":cfg,"genome":genome,
        "frozen_at_ms":now,"policy":POLICY,"training_updates":false,
        "broker_permission":false});
    db.execute(
        "INSERT INTO frozen_shadow_runs
         (id,context,mode,brain,created_ms,snapshot,active)
         VALUES(?1,?2,?3,?4,?5,?6,1)",
         params![run_id,context,mode,b.id,now,snapshot.to_string()])?;
    Ok(json!({"ok":true,"run_id":run_id,"brain":b.id,
        "active":true,"policy":POLICY,"no_broker_orders":true,
        "frozen":true,"created_ms":now}))
}
pub fn stop(db:&Connection,context:&str,mode:TradingMode,brain:&str)->Result<Value>{
    let count=db.execute(
        "UPDATE frozen_shadow_runs SET active=0
         WHERE context=?1 AND mode=?2 AND brain=?3 AND active=1",
        params![context,mode_key(mode)?,brain])?;
    Ok(json!({"ok":true,"stopped":count,"no_broker_orders":true}))
}
pub fn observe(db:&Connection,context:&str,mode:TradingMode,source_brain:&str,adaptive:&Signal,
    t:&Telemetry,now:u64,frames:&BTreeMap<String,Vec<FrameBar>>)->Result<()>{
    if mode!=TradingMode::Expert || !t.connected||!t.demo||t.account_trade_mode!=Some(0)
        ||!t.tick_age_ms.is_some_and(|v|v<=3000){return Ok(())}
    let mode=mode_key(mode)?;
    // Retired runs with a virtual position still get their exit checked; no new entries.
    let mut stmt=db.prepare(
        "SELECT id,snapshot,created_ms,active
         FROM frozen_shadow_runs
         WHERE context=?1 AND mode=?2 AND brain=?3
           AND (active=1 OR EXISTS(
             SELECT 1 FROM paper_brain_positions p
             WHERE p.context=frozen_shadow_runs.context
               AND p.brain='frozen_'||frozen_shadow_runs.id)
           OR EXISTS(SELECT 1 FROM frozen_shadow_markouts m
               WHERE m.run_id=frozen_shadow_runs.id AND m.state='pending'))
         ORDER BY created_ms,id")?;
    let rows=stmt.query_map(params![context,mode,source_brain],|r|
        Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,
            r.get::<_,u64>(2)?,r.get::<_,bool>(3)?))
    )?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for(id,raw,created,active) in rows {
        let saved:Value=serde_json::from_str(&raw)?;
        let b:BrainConfig=serde_json::from_value(saved["brain"].clone())?;
        let c:Config=serde_json::from_value(saved["config"].clone())?;
        ensure!(b.id==source_brain && c.trading.mode==TradingMode::Expert && c.symbol==t.symbol,
            "المعلمات المجمدة لا تطابق سياق الرصد؛ رفض بيانات تجريبية مشوهة");
        let g:Option<Genome>=serde_json::from_value(saved["genome"].clone())?;
        // Time and bar embargo: only post-freeze completed bars count.
        let signal=if active {
            execution::evaluate(&b,t,&c,frames,g.as_ref(),now)
        } else {
            Signal{brain:b.id.clone(),side:0,key:String::new(),bar_time:0,
                stop_distance:0.,target_distance:0.,hold_ms:0,
                reason:"Frozen trial retired".into(),ready:false,coordination:None}
        };
        let post_freeze=active && signal.bar_time>0
            && signal.bar_time.checked_mul(1000).is_some_and(|ms|ms>created && ms<=now);
        let mut virtual_signal=signal.clone();
        if !post_freeze { virtual_signal.ready=false; }
        let mut virtual_brain=b.clone();
        virtual_brain.id=paper_id(&id);
        // Price-sampled independent paper trades use *frozen* round-trip
        // commission, slippage, volumes, stop and target. Never own MT5 magic.
        crate::shadow_paper::observe(db,context,&virtual_brain,&virtual_signal,t,&c,now)?;
        // Same-entry-time A/B directional markout: fixed five-minute horizon
        // and identical prices, lot and assumed execution costs for both arms.
        // Retired trials only settle already opened markouts; never add one.
        crate::matched_forward::observe(
            db,&id,created,post_freeze,&signal,adaptive,t,&c,now)?;
        if post_freeze {
            db.execute(
                "INSERT OR IGNORE INTO frozen_shadow_signal_pairs
                 VALUES(?1,?2,?3,?4,?5)",
                params![id,signal.bar_time,now,
                    serde_json::to_string(&signal)?,
                    serde_json::to_string(adaptive)?])?;
        }
    }
    Ok(())
}
pub fn report(db:&Connection,context:&str,mode:TradingMode,brain:&str)->Result<Value>{
    if context.is_empty() {return Ok(json!({"status":"unbound","policy":POLICY}))}
    let item:Option<(String,u64,bool)>=db.query_row(
        "SELECT id,created_ms,active FROM frozen_shadow_runs
         WHERE context=?1 AND mode=?2 AND brain=?3
         ORDER BY created_ms DESC,id DESC LIMIT 1",
        params![context,mode_key(mode)?,brain],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((id,created,active))=item else{
        return Ok(json!({"status":"not_started","policy":POLICY,
            "note":"Freezing requires explicit Demo user action; no backfilled data"}))
    };
    let (bars,both_ready,agreements):(u64,u64,u64)=db.query_row(
        "SELECT COUNT(*),
            COALESCE(SUM(CASE WHEN json_extract(frozen,'$.ready')=1
                    AND json_extract(adaptive,'$.ready')=1 THEN 1 ELSE 0 END),0),
            COALESCE(SUM(CASE WHEN json_extract(frozen,'$.ready')=1
                    AND json_extract(adaptive,'$.ready')=1
                    AND json_extract(frozen,'$.side')=json_extract(adaptive,'$.side')
                    THEN 1 ELSE 0 END),0)
         FROM frozen_shadow_signal_pairs WHERE run_id=?1", [&id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let (closed,wins,net):(u64,u64,f64)=db.query_row(
        "SELECT COUNT(*),
          COALESCE(SUM(CASE WHEN json_extract(payload,'$.net_paper')>0
                    THEN 1 ELSE 0 END),0),
          COALESCE(SUM(json_extract(payload,'$.net_paper')),0.)
         FROM paper_brain_outcomes WHERE context=?1 AND brain=?2",
         params![context,paper_id(&id)],
         |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let open:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM paper_brain_positions WHERE context=?1 AND brain=?2)",
        params![context,paper_id(&id)],|r|r.get(0))?;
    Ok(json!({
        "status":if active{"active"}else{"stopped"},
        "policy":POLICY,"run_id":id,"created_ms":created,"active":active,
        "frozen_snapshot_immutable":true,"freeze_at_user_request":true,
        "paired_bars":bars,"both_ready":both_ready,"side_agreements":agreements,
        "paper_trades_closed":closed,"paper_wins":wins,"paper_net_after_assumed_costs":net,
        "paper_position_open":open,
        "matched_five_minute":crate::matched_forward::summary(db,&id)?,
        "execution_permission":false,
        "adaptive_reference":"Live signal on same observed bar; descriptive comparison only",
        "provenance":"Forward quote-sampled shadow; no actual broker fills",
        "limitations":"Not an independent live PnL comparison or whole-coordinator freeze. Missing tick paths and overnight funding may differ from MT5. No statistical conclusion until enough disjoint future trades.",
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn db()->Connection{
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        crate::shadow_paper::init(&db).unwrap();
        db
    }
    fn brain()->BrainConfig{BrainConfig{id:"trend".into(),..Default::default()}}
    fn cfg()->Config{Config{symbol:"XAUUSD".into(),..Default::default()}}
    #[test]
    fn snapshot_is_immutable_scoped_and_restart_safe(){
        let db=db();let b=brain();let c=cfg();
        let started=freeze(&db,"account1",TradingMode::Expert,&b,&c,None,1000).unwrap();
        let id=started["run_id"].as_str().unwrap().to_owned();
        assert!(freeze(&db,"account1",TradingMode::Expert,&b,&c,None,1002).is_err());
        assert_eq!(report(&db,"account1",TradingMode::Expert,"trend").unwrap()["status"],"active");
        assert_eq!(report(&db,"account2",TradingMode::Expert,"trend").unwrap()["status"],"not_started");
        let frozen: String=db.query_row(
            "SELECT snapshot FROM frozen_shadow_runs WHERE id=?1",
            [&id],|r|r.get(0)).unwrap();
        let saved:Value=serde_json::from_str(&frozen).unwrap();
        assert_eq!(saved["brain"]["lots"],b.lots);
        assert_eq!(saved["config"]["risk"]["commission_per_lot"],c.risk.commission_per_lot);
        stop(&db,"account1",TradingMode::Expert,"trend").unwrap();
        assert_eq!(report(&db,"account1",TradingMode::Expert,"trend").unwrap()["status"],"stopped");
        assert!(db.query_row("SELECT snapshot FROM frozen_shadow_runs WHERE id=?1",
            [&id],|r|r.get::<_,String>(0)).unwrap()==frozen);
        let newer=freeze(&db,"account1",TradingMode::Expert,&b,&c,None,2000).unwrap();
        assert_ne!(newer["run_id"],started["run_id"]);
    }
    #[test]
    fn stopped_trial_settles_existing_paper_position_without_new_entries(){
        let db=db();let b=brain();let c=cfg();
        let started=freeze(&db,"A",TradingMode::Expert,&b,&c,None,500).unwrap();
        let id=started["run_id"].as_str().unwrap();
        let pos=json!({
            "mode":"expert","opened_ms":1000,"brain":paper_id(id),
            "lots":0.01,"side":1,"entry":100.1,
            "stop_distance":1.,"target_distance":1.,
            "hold_ms":10000,"tick_size":0.1,"profit_tick_value":1.,
            "loss_tick_value":1.,"commission_per_lot":7.,"slippage_ticks":0.,
            "signal_reason":"synthetic open virtual position"
        });
        db.execute("INSERT INTO paper_brain_positions VALUES(?1,?2,?3)",
            params!["A",paper_id(id),pos.to_string()]).unwrap();
        stop(&db,"A",TradingMode::Expert,"trend").unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            symbol:"XAUUSD".into(),bid:Some(101.2),ask:Some(101.3),
            quote_time_ms:Some(2000),tick_age_ms:Some(0),
            tick_size:Some(0.1),tick_value_profit:Some(1.),
            tick_value_loss:Some(1.),..Default::default()};
        let adaptive=Signal{brain:"trend".into(),side:0,key:String::new(),
            bar_time:0,stop_distance:0.,target_distance:0.,hold_ms:0,
            reason:"idle".into(),ready:false,coordination:None};
        observe(&db,"A",TradingMode::Scalping,"trend",&adaptive,&t,2000,&BTreeMap::new()).unwrap();
        assert_eq!(report(&db,"A",TradingMode::Expert,"trend").unwrap()["paper_position_open"],true,
            "a different mode cannot inspect or settle this trial");
        observe(&db,"A",TradingMode::Expert,"trend",&adaptive,&t,2000,&BTreeMap::new()).unwrap();
        let summary=report(&db,"A",TradingMode::Expert,"trend").unwrap();
        assert_eq!(summary["paper_trades_closed"],1);
        assert_eq!(summary["paper_position_open"],false);
        assert_eq!(summary["paired_bars"],0);
        assert!(summary["paper_net_after_assumed_costs"].as_f64().unwrap()>0.);
        observe(&db,"A",TradingMode::Expert,"trend",&adaptive,&t,3000,&BTreeMap::new()).unwrap();
        assert_eq!(report(&db,"A",TradingMode::Expert,"trend").unwrap()["paper_trades_closed"],1);
    }

    #[test]
    fn no_system_or_sniper_or_unresolved_generated_policy_can_freeze(){
        let db=db();let mut b=brain();let c=cfg();
        b.id="system".into();
        assert!(freeze(&db,"account1",TradingMode::Expert,&b,&c,None,100).is_err());
        b.id="sniper_learner".into();
        assert!(freeze(&db,"account1",TradingMode::Expert,&b,&c,None,100).is_err());
        b.id="generated".into();
        assert!(freeze(&db,"account1",TradingMode::Expert,&b,&c,None,100).is_err());
        b.id="trend".into();
        assert!(freeze(&db,"account1",TradingMode::Sniper,&b,&c,None,100).is_err());
    }
}