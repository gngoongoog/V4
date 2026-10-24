//! P1-K3: forward-only, in-memory frozen Sniper Learner comparison.
//! Observes the *same accepted Core pulses*; NEVER submits broker orders,
//! learns from future closed deals, builds synthetic PnL, or backfills ticks.
//! Classic Sniper is explicitly unsupported by this Learner-only snapshot.
use anyhow::{ensure,Context,Result};
use braintrade_domain::{
    execution::Signal,sniper::Engine,trading::{SniperBrain,TradingMode},
    Config,Telemetry,
};
use serde_json::{json,Value};

const GAP_MS:u64=1500;
const WARMUP_MS:u64=2100;

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap_or_default()
}
fn eligible(t:&Telemetry,c:&Config,now:u64)->bool{
    t.connected&&t.demo&&t.account_trade_mode==Some(0)&&t.login>0
    &&!t.server.is_empty()&&!t.currency.is_empty()
    &&t.symbol==c.symbol&&c.trading.mode==TradingMode::Sniper
    &&c.trading.sniper_brain==SniperBrain::Learner
    &&t.source_quote_time_ms.is_some()
    &&t.quote_is_fresh(now,1500)
}
struct Trial{
    context:String,started_ms:u64,config:Config,
    frozen:Engine,active:bool,reason:String,
    forward:crate::sniper_forward_direction::ForwardDirectionAudit,
    training_checkpoint:Value,
    artifact_id:Option<String>,artifact_sha256:Option<String>,
    journal_write_failures:u64,
    first_future_ms:Option<u64>,last_event:Option<u64>,
    last_broker:Option<u64>,last_quote:Option<(u64,u64,u64)>,last_origin:String,
    last_dropped:Option<u64>,
    accepted: u64,comparable:u64,
    exact:u64,side_disagreements:u64,readiness_disagreements:u64,
    target_disagreements:u64,both_waiting:u64,
    excluded_legacy:u64,excluded_switch:u64,excluded_gap:u64,
    excluded_overflow:u64,excluded_warmup:u64,
    unpaired_reconciled_quote_changes:u64,
    unpaired_same_clock_price_changes:u64,
}
#[derive(Default)]
pub struct Study{trial:Option<Trial>}
impl Study{
    pub fn clear_on_mode_change(&mut self){self.trial=None;}
    #[cfg(test)]
    pub fn freeze(&mut self,t:&Telemetry,c:&Config,adaptive:&Engine,now:u64)->Result<Value>{
        self.freeze_impl(t,c,adaptive,json!({"status":"synthetic_fixture","independent_oos":false}),now)
    }
    pub fn freeze_verified(&mut self,t:&Telemetry,c:&Config,adaptive:&Engine,
        checkpoint:Value,now:u64)->Result<Value>{
        ensure!(checkpoint["status"]=="captured"
            &&checkpoint["cutoff_ms"].as_u64()==Some(now)
            &&checkpoint["broker_training_samples"]==checkpoint["cached_policy_samples"]
            &&checkpoint["broker_target_samples"]==checkpoint["cached_target_samples"]
            &&checkpoint["independent_oos"]==false
            &&checkpoint["broker_pnl"].is_null(),
            "Frozen Learner training ledger checkpoint missing, stale or inconsistent");
        self.freeze_impl(t,c,adaptive,checkpoint,now)
    }
    pub fn freeze_durable(&mut self,db:&rusqlite::Connection,t:&Telemetry,
        c:&Config,adaptive:&Engine,checkpoint:Value,now:u64)->Result<Value>{
        let mut result=self.freeze_verified(t,c,adaptive,checkpoint.clone(),now)?;
        let seal=match crate::sniper_research_artifact::persist(db,t,c,adaptive,&checkpoint,now){
            Ok(seal)=>seal,
            Err(error)=>{self.trial=None;return Err(error)}
        };
        let trial=self.trial.as_mut().expect("new frozen trial");
        trial.artifact_id=Some(seal.id.clone());
        trial.artifact_sha256=Some(seal.sha256.clone());
        result["research_artifact_id"]=json!(seal.id);
        result["research_artifact_sha256"]=json!(seal.sha256);
        result["persistence"]=json!("sqlite_freeze_checkpoint_only");
        Ok(result)
    }
    fn freeze_impl(&mut self,t:&Telemetry,c:&Config,adaptive:&Engine,
        checkpoint:Value,now:u64)->Result<Value>{
        ensure!(eligible(t,c,now),
            "Sniper Learner study needs a fresh bound MT5 Demo quote with original broker timestamp");
        ensure!(self.trial.as_ref().is_none_or(|x|!x.active),
            "Frozen Sniper Tick study is already active; stop it first");
        c.validate().map_err(anyhow::Error::msg)?;
        let frozen=adaptive.frozen_study_copy();
        self.trial=Some(Trial{
            context:context(t),started_ms:now,config:c.clone(),frozen,
            active:true,reason:String::new(),forward:Default::default(),
            training_checkpoint:checkpoint,
            artifact_id:None,artifact_sha256:None,
            journal_write_failures:0,
            first_future_ms:None,last_event:None,last_broker:None,last_quote:None,
            last_origin:String::new(),last_dropped:None,
            accepted:0,comparable:0,exact:0,side_disagreements:0,
            readiness_disagreements:0,target_disagreements:0,
            both_waiting:0,excluded_legacy:0,excluded_switch:0,
            excluded_gap:0,excluded_overflow:0,excluded_warmup:0,
            unpaired_reconciled_quote_changes:0,
            unpaired_same_clock_price_changes:0,
        });
        Ok(json!({"ok":true,"status":"active","context":context(t),
            "frozen_ms":now,"policy":"p1k3-sniper-learner-forward-tick-v1",
            "model_scores_frozen":true,"prior_tick_window_discarded":true,
            "no_broker_orders":true,"no_learning_effect":true,
            "persistence":"memory_only","training_checkpoint_status":self.trial.as_ref().unwrap().training_checkpoint["status"]}))
    }
    /// Slow reconciled snapshots and operator STOP flush prospective evidence.
    /// A database failure must never interrupt MT5 supervision; uncommitted
    /// events remain buffered and can be retried. A Core crash can still lose
    /// events since the last successful flush, clearly documented.
    pub fn flush_research_events(&mut self,db:&rusqlite::Connection,t:&Telemetry){
        let Some(trial)=self.trial.as_mut()else{return};
        if trial.context!=context(t)||!t.demo||t.account_trade_mode!=Some(0) {return}
        if trial.forward.uncommitted_events().is_empty(){return}
        let (Some(id),Some(hash))=(trial.artifact_id.as_ref(),trial.artifact_sha256.as_ref())
            else{return};
        let count=trial.forward.uncommitted_events().len();
        match crate::sniper_forward_journal::append(
            db,&trial.context,id,hash,trial.forward.uncommitted_events()){
            Ok(n) if n==count=>trial.forward.acknowledge_committed(n),
            _=>trial.journal_write_failures+=1,
        }
    }
    pub fn stop_with_research_flush(&mut self,db:&rusqlite::Connection,
        t:&Telemetry)->Result<Value>{
        let mut result=self.stop(t)?;
        self.flush_research_events(db,t);
        let trial=self.trial.as_ref().context("Study has no active research session")?;
        result["uncommitted_research_events"]=json!(trial.forward.uncommitted_events().len());
        result["journal_write_failures"]=json!(trial.journal_write_failures);
        Ok(result)
    }
    pub fn stop(&mut self,t:&Telemetry)->Result<Value>{
        let trial=self.trial.as_mut().filter(|v|v.context==context(t))
            .ok_or_else(||anyhow::anyhow!("No matching Sniper study"))?;
        trial.active=false;
        trial.forward.invalidate_segment();
        trial.reason="operator_stopped".into();
        Ok(json!({"ok":true,"status":"stopped","no_broker_orders":true}))
    }
    /// Regular MT5 reconciliation can supply a NEW broker quote independently
    /// of the Fast Pulse. The executing model consumes those quotes; if the
    /// frozen study did not, its tick path is no longer matched. Exclude the
    /// affected span and warm up afresh rather than fabricate parity.
    pub fn reconcile(&mut self,t:&Telemetry,c:&Config) {
        let Some(trial)=self.trial.as_mut().filter(|x|x.active)else{return};
        if context(t)!=trial.context||c.trading.mode!=TradingMode::Sniper
           ||c.trading.sniper_brain!=SniperBrain::Learner{
            trial.forward.invalidate_segment();
            trial.active=false;trial.reason="reconciled_broker_context_changed".into();
            return
        }
        if let Some(last)=trial.last_broker {
            let incoming=t.source_quote_time_ms;
            // Broker timestamps may have only millisecond resolution: a
            // different Bid/Ask at the SAME broker time is still a new
            // unpaired observation in the executing engine. Excluding it
            // prevents a falsely "matched" frozen-versus-live tick pair.
            let same_clock_price_change=incoming==Some(last)
                &&trial.last_quote.is_some_and(|(ts,bid,ask)|{
                    ts==last&&(t.bid.is_some_and(|v|v.to_bits()!=bid)
                        ||t.ask.is_some_and(|v|v.to_bits()!=ask))
                });
            // Unknown quote provenance on a full snapshot also breaks the
            // trace after the study has started.
            if incoming.is_none()||incoming.is_some_and(|x|x>last)
                ||same_clock_price_change{
                trial.unpaired_reconciled_quote_changes+=1;
                if same_clock_price_change{trial.unpaired_same_clock_price_changes+=1;}
                trial.first_future_ms=None;
                trial.forward.invalidate_segment();
                trial.frozen.reset_quotes();
                trial.last_origin.clear();
                trial.last_dropped=None;
                if let Some(tick)=incoming{trial.last_broker=Some(tick)}
                trial.last_quote=incoming.zip(t.bid).zip(t.ask)
                    .map(|((ms,bid),ask)|(ms,bid.to_bits(),ask.to_bits()));
            }
        }
    }
    pub fn observe(&mut self,t:&Telemetry,c:&Config,adaptive:Option<&Signal>,
        payload:&[u8],now:u64){
        let Some(trial)=self.trial.as_mut().filter(|v|v.active)else{return};
        if context(t)!=trial.context||!eligible(t,c,now) {
            // Never resume an outdated/foreign account trial.
            trial.forward.invalidate_segment();
            trial.active=false;trial.reason="broker_identity_or_mode_changed".into();return
        }
        if serde_json::to_value(c).ok()!=serde_json::to_value(&trial.config).ok(){
            trial.forward.invalidate_segment();
            trial.active=false;trial.reason="policy_config_changed_after_freeze".into();return
        }
        let meta:Value=serde_json::from_slice(payload).unwrap_or(Value::Null);
        let src=meta["quote_source"].as_str().unwrap_or("");
        let drop=meta["native_dropped_total"].as_u64();
        let (Some(event),Some(broker))=(t.quote_time_ms,t.source_quote_time_ms)
        else {trial.excluded_legacy+=1;return};
        if broker<=trial.started_ms || now<=trial.started_ms
           ||trial.last_event.is_some_and(|p|event<=p)
           ||trial.last_broker.is_some_and(|p|broker<p) {
            trial.excluded_legacy+=1;return
        }
        let gap=trial.last_broker.map(|p|broker-p).unwrap_or(0);
        let changed=!trial.last_origin.is_empty()&&src!=trial.last_origin;
        let overflow=trial.last_dropped.zip(drop)
            .is_some_and(|(p,n)|n!=p);
        // Any source jump/gap/mailbox loss makes the signal trajectory
        // ambiguous: restart ONLY the frozen study's tick window, and wait
        // for a fresh warmup. The executing engine is never modified.
        let bad_source=!matches!(src,"native"|"poll")||drop.is_none();
        if bad_source||changed||gap>GAP_MS||overflow {
            trial.forward.invalidate_segment();
            trial.frozen.reset_quotes();
            trial.first_future_ms=None;
            if bad_source{trial.excluded_legacy+=1}
            if changed{trial.excluded_switch+=1}
            if gap>GAP_MS{trial.excluded_gap+=1}
            if overflow{trial.excluded_overflow+=1}
        }
        trial.last_event=Some(event);trial.last_broker=Some(broker);
        trial.last_quote=t.bid.zip(t.ask).map(|(bid,ask)|(broker,bid.to_bits(),ask.to_bits()));
        trial.last_origin=src.to_owned();trial.last_dropped=drop;
        trial.accepted+=1;
        if bad_source||changed||gap>GAP_MS||overflow {return}
        let frozen=trial.frozen.evaluate(t,&trial.config,now);
        let start=*trial.first_future_ms.get_or_insert(broker);
        // The executing learner can retain pre-freeze quotes for <=2 seconds.
        // The first 2.1 seconds of prospective quotes are therefore warmup
        // and explicitly excluded from comparison (no historical backfill).
        if broker.saturating_sub(start)<WARMUP_MS {
            trial.excluded_warmup+=1;return
        }
        // A matured label consumes ONLY subsequent paired, accepted forward
        // broker quotes; never same-tick data or existing trade outcomes.
        // Unmatched current raw signals may settle a prior pending markout,
        // but must not originate a new unpaired candidate.
        let paired=adaptive.is_some_and(|x|x.key==frozen.key);
        trial.forward.observe_with_cost(broker,t.bid,t.ask,t.tick_size,
            t.tick_value_loss,t.tick_value_profit,
            trial.config.risk.commission_per_lot,
            trial.config.risk.slippage_ticks,
            if paired {Some(&frozen)} else {None});
        let Some(adaptive)=adaptive.filter(|x|x.key==frozen.key)
        else {trial.excluded_legacy+=1;return};
        trial.comparable+=1;
        if !adaptive.ready&&!frozen.ready{trial.both_waiting+=1}
        else if adaptive.ready!=frozen.ready{trial.readiness_disagreements+=1}
        else if adaptive.side!=frozen.side{trial.side_disagreements+=1}
        else if (adaptive.target_distance-frozen.target_distance).abs()>1e-8
            ||(adaptive.stop_distance-frozen.stop_distance).abs()>1e-8 {
            trial.target_disagreements+=1
        }else{trial.exact+=1}
    }
    pub fn report(&self,t:Option<&Telemetry>,now:u64)->Value {
        let Some(trial)=self.trial.as_ref()else{return json!({
            "schema":"p1k3-sniper-frozen-tick-v1","status":"not_started",
            "no_broker_orders":true,"independent_oos":false})};
        let Some(t)=t.filter(|t|t.connected&&t.demo
            &&t.account_trade_mode==Some(0)&&context(t)==trial.context)
        else{return json!({"schema":"p1k3-sniper-frozen-tick-v1",
            "status":"unavailable","no_broker_orders":true})};
        json!({
            "schema":"p1k3-sniper-frozen-tick-v1",
            "status":if trial.active{"active"}else{"stopped"},
            "reason":trial.reason,
            "context":trial.context,
            "mode":"sniper","sniper_brain":"learner",
            "account":{"login":t.login,"server":t.server,"symbol":t.symbol,"currency":t.currency},
            "frozen_ms":trial.started_ms,"observed_ms":now,
            "accepted_prospective_pulses":trial.accepted,
            "comparable_future_tick_decisions":trial.comparable,
            "exact_decisions":trial.exact,
            "same_direction_both_waiting":trial.both_waiting,
            "readiness_disagreements":trial.readiness_disagreements,
            "side_disagreements":trial.side_disagreements,
            "target_or_stop_disagreements":trial.target_disagreements,
            "excluded_unknown_or_legacy":trial.excluded_legacy,
            "excluded_source_switch":trial.excluded_switch,
            "excluded_broker_clock_gap":trial.excluded_gap,
            "excluded_native_queue_counter_change":trial.excluded_overflow,
            "excluded_early_warmup":trial.excluded_warmup,
            "unpaired_reconciled_quote_changes":trial.unpaired_reconciled_quote_changes,
            "unpaired_same_clock_price_changes":trial.unpaired_same_clock_price_changes,
            "minimum_warmup_ms":WARMUP_MS,
            "forward_direction_audit":trial.forward.json(),
            "research_event_write_failures":trial.journal_write_failures,
            "training_checkpoint":trial.training_checkpoint,
            "research_artifact_id":trial.artifact_id,
            "research_artifact_sha256":trial.artifact_sha256,
            "forward_only":true,
            "model_scores_frozen":true,"prior_tick_window_discarded":true,
            "persistence":"memory_only","read_only":true,
            "no_broker_orders":true,"no_learning_effect":true,
            "independent_oos":false,"paper_pnl":Value::Null,"broker_pnl":Value::Null,
            "strategy_win_rate":Value::Null,
            "note":"Sniper Learner raw signal vs immutable frozen policy on accepted future Native/Poll quotes; not Classic Sniper, no market-tick completeness, no broker gates/fills or OOS profitability certification."
        })
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn fixture(now:u64)->(Telemetry,Config,Engine){
        let mut c=Config::default();
        c.trading.mode=TradingMode::Sniper;
        c.trading.sniper_brain=SniperBrain::Learner;
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:17,server:"Demo".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),bid:Some(2500.),ask:Some(2500.2),
            quote_time_ms:Some(now),source_quote_time_ms:Some(now),
            tick_age_ms:Some(0),point:Some(0.01),tick_size:Some(0.01),
            tick_value_profit:Some(1.),tick_value_loss:Some(1.),
            ..Default::default()};
        (t,c,Engine::default())
    }
    fn meta(src:&str,drops:u64)->Vec<u8>{
        json!({"quote_source":src,"native_dropped_total":drops}).to_string().into_bytes()
    }
    #[test]
    fn account_real_classic_and_policy_drift_never_create_forward_profit(){
        let (mut t,mut c,e)=fixture(100_000);
        let mut s=Study::default();
        t.demo=false;assert!(s.freeze(&t,&c,&e,100_000).is_err());
        t.demo=true;c.trading.sniper_brain=SniperBrain::Classic;
        assert!(s.freeze(&t,&c,&e,100_000).is_err());
        c.trading.sniper_brain=SniperBrain::Learner;
        s.freeze(&t,&c,&e,100_000).unwrap();
        c.trading.sniper_min_target_pips+=1.;
        t.source_quote_time_ms=Some(101_000);t.quote_time_ms=Some(101_000);
        s.observe(&t,&c,None,&meta("native",0),101_001);
        assert_eq!(s.report(Some(&t),101_001)["status"],"stopped");
        assert_eq!(s.report(Some(&t),101_001)["reason"],"policy_config_changed_after_freeze");
        assert!(s.report(Some(&t),101_001)["broker_pnl"].is_null());
    }
    #[test]
    fn future_tick_warmup_and_source_gap_do_not_fabricate_advantage(){
        let (mut t,c,e)=fixture(100_000);
        let mut s=Study::default();
        s.freeze(&t,&c,&e,100_000).unwrap();
        for (source,broker,event,drops) in [
            ("native",100_100,100_100,0),
            ("native",101_000,101_000,0),
            ("poll",101_500,101_500,0),
            ("poll",103_900,103_900,0),
            ("poll",104_000,104_000,1),
            ("poll",104_500,104_500,1),
        ]{
            t.source_quote_time_ms=Some(broker);
            t.quote_time_ms=Some(event);
            s.observe(&t,&c,None,&meta(source,drops),broker+1);
        }
        let r=s.report(Some(&t),105_000);
        assert_eq!(r["accepted_prospective_pulses"],6);
        assert_eq!(r["excluded_source_switch"],1);
        assert_eq!(r["excluded_broker_clock_gap"],1);
        assert_eq!(r["excluded_native_queue_counter_change"],1);
        assert_eq!(r["comparable_future_tick_decisions"],0);
        assert!(r["paper_pnl"].is_null());
        assert_eq!(r["independent_oos"],false);
        assert_eq!(r["no_broker_orders"],true);
    }
    #[test]
    fn two_same_tick_engines_produce_actual_compared_future_decisions_after_warmup(){
        let (mut t,c,e)=fixture(100_000);
        let mut trial=Study::default();
        let mut adaptive=e.frozen_study_copy();
        trial.freeze(&t,&c,&e,100_000).unwrap();
        for step in 1..=12u64{
            let ts=100_000+step*250;
            t.source_quote_time_ms=Some(ts);
            t.quote_time_ms=Some(ts);
            t.bid=Some(2500.0+(step as f64)*0.03);
            t.ask=Some(2500.2+(step as f64)*0.03);
            let raw=adaptive.evaluate(&t,&c,ts+1);
            trial.observe(&t,&c,Some(&raw),&meta("native",0),ts+1);
        }
        let report=trial.report(Some(&t),103_100);
        assert_eq!(report["accepted_prospective_pulses"],12);
        assert!(report["comparable_future_tick_decisions"].as_u64().unwrap()>0);
        assert_eq!(report["side_disagreements"],0);
        assert_eq!(report["readiness_disagreements"],0);
        assert_eq!(report["target_or_stop_disagreements"],0);
        assert_eq!(report["paper_pnl"],Value::Null);
        assert_eq!(report["model_scores_frozen"],true);
        assert_eq!(report["independent_oos"],false);
    }
    #[test]
    fn new_reconciled_snapshot_quote_invalidates_pairing_until_rewarmed(){
        let (mut t,c,e)=fixture(100_000);
        let mut study=Study::default();
        study.freeze(&t,&c,&e,100_000).unwrap();
        t.source_quote_time_ms=Some(100_500);
        t.quote_time_ms=Some(100_500);
        study.observe(&t,&c,None,&meta("native",0),100_501);
        // A newer full snapshot reaches the live learner but not the
        // frozen Fast Pulse study. Treat it as an unpaired event.
        let mut full=t.clone();
        full.source_quote_time_ms=Some(100_900);
        full.quote_time_ms=Some(100_900);
        study.reconcile(&full,&c);
        let r=study.report(Some(&full),100_901);
        assert_eq!(r["unpaired_reconciled_quote_changes"],1);
        assert_eq!(r["comparable_future_tick_decisions"],0);
        assert_eq!(r["independent_oos"],false);
    }
    #[test]
    fn same_broker_millisecond_with_different_reconciled_price_is_not_paired(){
        let (mut t,c,e)=fixture(100_000);
        let mut study=Study::default();
        let mut adaptive=e.frozen_study_copy();
        study.freeze(&t,&c,&e,100_000).unwrap();
        for step in 1..=12u64 {
            let ts=100_000+step*250;
            t.source_quote_time_ms=Some(ts);
            t.quote_time_ms=Some(ts);
            t.bid=Some(2500.0+step as f64*0.03);
            t.ask=Some(2500.2+step as f64*0.03);
            let raw=adaptive.evaluate(&t,&c,ts+1);
            study.observe(&t,&c,Some(&raw),&meta("native",0),ts+1);
        }
        let before=study.report(Some(&t),103_100);
        assert!(before["comparable_future_tick_decisions"].as_u64().unwrap()>0);
        // Re-reading the IDENTICAL broker snapshot cannot exclude a tick.
        study.reconcile(&t,&c);
        assert_eq!(study.report(Some(&t),103_101)["unpaired_reconciled_quote_changes"],0);
        // MT5 may report distinct prices with the exact same broker millisecond.
        // The adaptive engine sees the snapshot; frozen Fast Pulse did not.
        let mut unpaired=t.clone();
        unpaired.ask=Some(t.ask.unwrap()+0.02);
        study.reconcile(&unpaired,&c);
        let excluded=study.report(Some(&unpaired),103_101);
        assert_eq!(excluded["unpaired_reconciled_quote_changes"],1);
        assert_eq!(excluded["unpaired_same_clock_price_changes"],1);
        let next=103_250;
        unpaired.source_quote_time_ms=Some(next);
        unpaired.quote_time_ms=Some(next);
        let raw=adaptive.evaluate(&unpaired,&c,next+1);
        study.observe(&unpaired,&c,Some(&raw),&meta("native",0),next+1);
        let later=study.report(Some(&unpaired),next+1);
        assert_eq!(later["comparable_future_tick_decisions"],before["comparable_future_tick_decisions"],
            "unpaired same-ms quote must force forward warmup, not claim agreement");
        assert!(later["paper_pnl"].is_null());
        assert_eq!(later["independent_oos"],false);
    }
    #[test]
    fn nonoverlapping_frozen_future_labels_require_postfreeze_continuity_and_time(){
        let (mut t,c,e)=fixture(100_000);
        let mut study=Study::default();
        let mut adaptive=e.frozen_study_copy();
        study.freeze(&t,&c,&e,100_000).unwrap();
        for step in 1..=50u64{
            let ts=100_000+step*250;
            t.source_quote_time_ms=Some(ts);
            t.quote_time_ms=Some(ts);
            t.bid=Some(2500.0+step as f64*0.03);
            t.ask=Some(2500.2+step as f64*0.03);
            let raw=adaptive.evaluate(&t,&c,ts+1);
            study.observe(&t,&c,Some(&raw),&meta("native",0),ts+1);
        }
        let v=study.report(Some(&t),113_000);
        let forward=&v["forward_direction_audit"];
        assert_eq!(forward["schema"],"p1k5b-forward-direction-v1");
        assert_eq!(forward["fixed_horizon_ms"],5000);
        assert!(forward["resolved_direction_labels"].as_u64().unwrap()>=1);
        assert_eq!(forward["net_pnl"],Value::Null);
        assert_eq!(forward["independent_oos"],false);
        assert_eq!(v["broker_pnl"],Value::Null);
        assert_eq!(v["no_broker_orders"],true);
    }
    #[test]
    fn frozen_forward_pending_is_censored_on_native_source_switch(){
        let (mut t,c,e)=fixture(100_000);
        let mut study=Study::default();
        let mut adaptive=e.frozen_study_copy();
        study.freeze(&t,&c,&e,100_000).unwrap();
        for step in 1..=14u64{
            let ts=100_000+step*250;
            t.source_quote_time_ms=Some(ts);
            t.quote_time_ms=Some(ts);
            t.bid=Some(2500.+step as f64*0.03);
            t.ask=Some(2500.2+step as f64*0.03);
            let raw=adaptive.evaluate(&t,&c,ts+1);
            study.observe(&t,&c,Some(&raw),&meta("native",0),ts+1);
        }
        let before=study.report(Some(&t),104_000);
        assert_eq!(before["forward_direction_audit"]["pending_unmatured_label"],1);
        t.source_quote_time_ms=Some(103_750);
        t.quote_time_ms=Some(103_750);
        study.observe(&t,&c,None,&meta("poll",0),103_751);
        let after=study.report(Some(&t),103_800);
        assert_eq!(after["forward_direction_audit"]["pending_unmatured_label"],0);
        assert_eq!(after["forward_direction_audit"]["censored_disconnected_segments"],1);
        assert_eq!(after["forward_direction_audit"]["resolved_direction_labels"],0);
        assert_eq!(after["excluded_source_switch"],1);
    }
    #[test]
    fn frozen_study_does_not_inherit_old_account_or_live_quotes(){
        let (mut t,c,e)=fixture(100_000);
        let mut s=Study::default();s.freeze(&t,&c,&e,100_000).unwrap();
        let mut other=t.clone();other.login=99;
        assert_eq!(s.report(Some(&other),100_500)["status"],"unavailable");
        assert!(s.stop(&other).is_err());
        assert_eq!(s.report(Some(&t),100_500)["status"],"active");
        t.source_quote_time_ms=Some(100_900);t.quote_time_ms=Some(100_900);
        s.observe(&t,&c,None,&meta("native",0),100_901);
        assert_eq!(s.report(Some(&t),100_901)["excluded_early_warmup"],1);
        s.stop(&t).unwrap();
        assert_eq!(s.report(Some(&t),100_901)["status"],"stopped");
    }
}
