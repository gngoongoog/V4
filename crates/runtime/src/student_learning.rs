//! Durable Demo experience ledger and causal, quote-only exit experiments.
//! Broker labels remain in coordinator_broker_samples; no shadow result can enter it.
use anyhow::Result;
use braintrade_domain::{
    execution::{BrainConfig, Signal},
    student::{ExitProfile, ExitStudy, Trial},
    Config, Telemetry,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn source(context: &str, mode: &Value, brain: &str, timeframe: &str, report: &str) -> String {
    json!([context, "student-exits-v1", mode, brain, timeframe, report]).to_string()
}

/// Called before the durable entry is queued; original geometry and policy are frozen.
pub fn adapt(
    signal: &mut Signal,
    t: &Telemetry,
    c: &Config,
    b: &BrainConfig,
    policies: &BTreeMap<String, ExitStudy>,
) {
    if !signal.ready || !t.demo || t.account_trade_mode != Some(0) {
        return;
    }
    let context = crate::coordinator_learning::context(t);
    let selection = signal
        .coordination
        .as_ref()
        .map(|v| v["selection"].clone())
        .unwrap_or(Value::Null);
    let brain = selection["brain"].as_str().unwrap_or(&b.id);
    let timeframe = selection["timeframe"].as_str().unwrap_or(&b.timeframe);
    let report = selection["report_id"].as_str().unwrap_or(&b.report_id);
    let mut identity: Value = serde_json::from_str(&source(
        &context,
        &json!(c.trading.mode),
        brain,
        timeframe,
        report,
    ))
    .unwrap();
    let definition = if selection["definition"].is_array() {
        selection["definition"].clone()
    } else {
        json!([b.stop_atr, b.reward_risk, b.max_hold_seconds])
    };
    // Tick monetary values are frozen per experience, not policy identity: conversion
    // changes must not restart the same strategy study on every quote.
    identity.as_array_mut().unwrap().push(
        json!({"definition":definition,"sniper_target":c.trading.sniper_target_points,
        "sniper_stop":c.trading.sniper_stop_points,"sniper_hold":c.trading.sniper_max_hold_seconds,
        "commission":c.risk.commission_per_lot,"slippage":c.risk.slippage_ticks,"tick":t.tick_size}),
    );
    let key = identity.to_string();
    let study = policies.get(&key).cloned().unwrap_or_default();
    let mut profile = if c.trading.adaptive_exits {
        study.active
    } else {
        ExitProfile::Baseline
    };
    // Preserve the user's Sniper cost fraction after a learned target change.
    // Falling back keeps the already-valid baseline entry available.
    let mut cost_fallback = false;
    if profile != ExitProfile::Baseline && b.id == "system"
        && c.trading.mode == braintrade_domain::trading::TradingMode::Sniper {
        let (_, target, _) = profile.geometry(signal.stop_distance, signal.target_distance, signal.hold_ms);
        let net_target = t.tick_size.zip(t.tick_value_profit).filter(|(tick,value)|*tick>0.&&*value>0.)
            .map(|(tick,value)|(target/tick*value-c.risk.commission_per_lot-c.risk.slippage_ticks*value)*b.lots);
        let affordable = braintrade_domain::trading::costs(t,c,b.lots,t.quote_time_ms.unwrap_or(0)).ok()
            .zip(net_target).is_some_and(|(cost,net)| net>0. && cost.total/(cost.total+net)<=c.trading.max_cost_fraction);
        if !affordable { profile=ExitProfile::Baseline; cost_fallback=true; }
    }
    let baseline = json!({"stop":signal.stop_distance,"target":signal.target_distance,"hold_ms":signal.hold_ms});
    let coord = signal.coordination.get_or_insert_with(|| json!({}));
    coord["student"] = json!({"version":1,"source":key,"brain":brain,"timeframe":timeframe,"report_id":report,
        "baseline":baseline,"profile":profile,"adaptive_exits":c.trading.adaptive_exits,
        "phase":if study.phase.is_empty(){"collecting"}else{&study.phase},"decision":study.last_decision,
        "validation_gain_r":study.last_gain_r,"cost_fallback":cost_fallback,"basis":"prospective_paired_quote_trials"});
    (signal.stop_distance, signal.target_distance, signal.hold_ms) =
        profile.geometry(signal.stop_distance, signal.target_distance, signal.hold_ms);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Experience {
    command_id: String,
    context: String,
    source: String,
    brain: String,
    owner: String,
    open_deal: u64,
    position_id: u64,
    volume: f64,
    opened_ms: u64,
    entry: Value,
    trial: Trial,
    actual: Option<Value>,
    position_mfe: f64,
    position_mae: f64,
    position_samples: u64,
    last_position_ms: u64,
    study_consumed: bool,
    #[serde(default)]
    shadow_expired: bool,
}
impl Experience {
    fn complete(&self) -> bool {
        self.actual.is_some()
            && (self.trial.complete() || self.shadow_expired)
            && self.study_consumed
    }
    fn review(&self) -> Value {
        let net = self.actual.as_ref().and_then(|v| v["net"].as_f64());
        let closed = self
            .actual
            .as_ref()
            .and_then(|v| v["closed_ms"].as_u64())
            .unwrap_or(0);
        let coverage =
            self.position_samples > 1 && (closed == 0 || self.last_position_ms <= closed);
        let mut patterns = Vec::new();
        if let Some(actual) = &self.actual {
            if actual["attribution"] == "costs_erased_gross_profit" {
                patterns.push("cost_drag");
            }
            if coverage
                && net.is_some_and(|n| n < 0.)
                && self.position_mae >= self.trial.stop * 0.5
                && self.position_mfe < self.trial.stop * 0.2
            {
                patterns.push("adverse_entry_path");
            }
            let observed_peak =
                self.position_mfe / self.trial.tick_size * self.trial.profit_value * self.volume
                    - self.trial.commission_per_lot * self.volume;
            if coverage && observed_peak > 0. && net.is_some_and(|n| n < observed_peak * 0.25) {
                patterns.push("profit_giveback");
            }
            if patterns.is_empty() {
                patterns.push(if net.is_some_and(|n| n > 0.) {
                    "positive_after_costs"
                } else {
                    "unclassified"
                });
            }
        }
        json!({"command_id":self.command_id,"brain":self.brain,"owner":self.owner,"position_id":self.position_id,
            "opened_ms":self.opened_ms,"entry":self.entry,"actual":self.actual,"patterns":patterns,
            "observed_mfe_price":self.position_mfe,"observed_mae_price":self.position_mae,"position_samples":self.position_samples,
            "position_path_usable":coverage,"shadow":self.trial,"shadow_expired":self.shadow_expired,"study_consumed":self.study_consumed,
            "note":"Patterns describe observed paths, not proof of a cause. Shadow exits are estimates; actual PnL is booked MT5 data."})
    }
}

pub struct Student {
    context: String,
    active: BTreeMap<String, Experience>,
    pub policies: BTreeMap<String, ExitStudy>,
}
impl Student {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS student_experiences(context TEXT NOT NULL,command_id TEXT NOT NULL,source TEXT NOT NULL,opened_ms INTEGER NOT NULL,complete INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,command_id));
            CREATE INDEX IF NOT EXISTS student_pending ON student_experiences(context,complete,opened_ms);
            CREATE TABLE IF NOT EXISTS student_exit_studies(source TEXT PRIMARY KEY,context TEXT NOT NULL,payload TEXT NOT NULL);")?;
        Ok(Self {
            context: String::new(),
            active: BTreeMap::new(),
            policies: BTreeMap::new(),
        })
    }
    fn flush(&self, db: &Connection) -> Result<()> {
        let tx = db.unchecked_transaction()?;
        for e in self.active.values() {
            tx.execute("INSERT INTO student_experiences VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(context,command_id) DO UPDATE SET complete=excluded.complete,payload=excluded.payload",
                params![e.context,e.command_id,e.source,e.opened_ms,e.complete(),serde_json::to_string(e)?])?;
        }
        for (key, study) in &self.policies {
            tx.execute(
                "INSERT OR REPLACE INTO student_exit_studies VALUES(?1,?2,?3)",
                params![key, self.context, serde_json::to_string(study)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    fn load(&mut self, db: &Connection, key: &str) -> Result<()> {
        if self.context == key {
            return Ok(());
        }
        self.flush(db)?;
        self.active.clear();
        self.policies.clear();
        self.context = key.into();
        let mut st=db.prepare("SELECT payload FROM student_experiences WHERE context=?1 AND complete=0 ORDER BY opened_ms")?;
        for raw in st.query_map([key], |r| r.get::<_, String>(0))? {
            let e: Experience = serde_json::from_str(&raw?)?;
            self.active.insert(e.command_id.clone(), e);
        }
        let mut st =
            db.prepare("SELECT source,payload FROM student_exit_studies WHERE context=?1")?;
        for row in st.query_map([key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (k, raw) = row?;
            self.policies.insert(k, serde_json::from_str(&raw)?);
        }
        Ok(())
    }
    /// Fast pulses update only memory. SQLite persistence and study changes use snapshots.
    pub fn quote(&mut self, t: &Telemetry) {
        if !t.demo
            || t.account_trade_mode != Some(0)
            || !t.connected
            || crate::coordinator_learning::context(t) != self.context
            || t.tick_age_ms.is_none_or(|age| age > 1000)
        {
            return;
        }
        let (Some(bid), Some(ask), Some(ms)) = (t.bid, t.ask, t.quote_time_ms) else {
            return;
        };
        for e in self
            .active
            .values_mut()
            .filter(|e| !e.trial.complete() && !e.shadow_expired)
        {
            e.trial.observe(bid, ask, ms);
        }
    }
    pub fn observe(&mut self, db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
        if !t.connected || !t.demo || t.account_trade_mode != Some(0) {
            return Ok(());
        }
        let key = crate::coordinator_learning::context(t);
        self.load(db, &key)?;
        // Register from acknowledged fills immediately, without waiting five seconds for history.
        let mut st=db.prepare("SELECT c.payload,c.result FROM broker_commands c WHERE c.context=?1 AND c.kind='open' AND CAST(json_extract(c.result,'$.deal') AS INTEGER)>0 AND json_extract(c.payload,'$.signal.coordination.student.version')=1 AND NOT EXISTS(SELECT 1 FROM student_experiences e WHERE e.context=c.context AND e.command_id=c.id) ORDER BY c.created_ms,c.id")?;
        let rows = st
            .query_map([&key], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (raw, result) in rows {
            let cmd: Value = serde_json::from_str(&raw)?;
            let r: Value = serde_json::from_str(&result)?;
            let id = cmd["id"].as_str().unwrap_or("");
            if self.active.contains_key(id) {
                continue;
            }
            let m = &cmd["signal"]["coordination"]["student"];
            let baseline = &m["baseline"];
            let l = &cmd["learning"];
            let entry = r["price"].as_f64().unwrap_or(0.);
            let volume = r["volume"].as_f64().unwrap_or(0.);
            let tick = l["tick_size"].as_f64().unwrap_or(0.);
            let profit = l["tick_value_profit"].as_f64().unwrap_or(0.);
            let loss = l["tick_value_loss"].as_f64().unwrap_or(0.);
            let stop = baseline["stop"].as_f64().unwrap_or(0.);
            let target = baseline["target"].as_f64().unwrap_or(0.);
            if [entry, volume, tick, profit, loss, stop, target]
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.)
            {
                continue;
            }
            let opened = r["acknowledged_ms"]
                .as_u64()
                .unwrap_or(cmd["created_ms"].as_u64().unwrap_or(now));
            let e = Experience {
                command_id: id.into(),
                context: key.clone(),
                source: m["source"].as_str().unwrap_or("").into(),
                brain: m["brain"].as_str().unwrap_or("").into(),
                owner: cmd["brain"].as_str().unwrap_or("").into(),
                open_deal: r["deal"].as_u64().unwrap(),
                position_id: 0,
                volume,
                opened_ms: opened,
                entry: json!({"learning":l,"signal":cmd["signal"],"volume":volume,"fill_price":entry,"timestamp_basis":"local_acknowledgement","request":r["request"]}),
                trial: Trial {
                    opened_ms: opened,
                    side: cmd["signal"]["side"].as_i64().unwrap_or(0) as i8,
                    entry,
                    stop,
                    target,
                    hold_ms: baseline["hold_ms"].as_u64().unwrap_or(1000),
                    tick_size: tick,
                    profit_value: profit,
                    loss_value: loss,
                    commission_per_lot: cmd["commission_per_lot"].as_f64().unwrap_or(0.),
                    exit_slippage: l["slippage_ticks"].as_f64().unwrap_or(0.) * tick,
                    last_quote_ms: opened,
                    samples: 0,
                    max_gap_ms: 0,
                    usable: r["acknowledged_ms"].as_u64().is_some(),
                    mfe: 0.,
                    mae: 0.,
                    outcomes: [None, None, None],
                },
                actual: None,
                position_mfe: 0.,
                position_mae: 0.,
                position_samples: 0,
                last_position_ms: 0,
                study_consumed: false,
                shadow_expired: false,
            };
            self.active.insert(id.into(), e);
        }
        for e in self.active.values_mut() {
            if e.position_id == 0 {
                e.position_id = db
                    .query_row(
                        "SELECT position_id FROM broker_deals WHERE context=?1 AND ticket=?2",
                        params![key, e.open_deal],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or(0);
            }
            if e.actual.is_none() {
                let actual:Option<String>=db.query_row("SELECT payload FROM coordinator_broker_samples WHERE context=?1 AND json_extract(payload,'$.open_command_id')=?2 AND brain=?3 LIMIT 1",params![key,e.command_id,e.owner],|r|r.get(0)).optional()?;
                if let Some(raw) = actual {
                    let label: Value = serde_json::from_str(&raw)?;
                    if label["closed_volume"]
                        .as_f64()
                        .is_some_and(|volume| (volume - e.volume).abs() > 1e-8)
                    {
                        e.trial.usable = false;
                    }
                    e.actual = Some(label);
                }
            }
            // Actual excursions require an observed position with an exact identifier.
            if e.actual.is_none() && e.position_id > 0 {
                if let Some(p) = t
                    .positions
                    .iter()
                    .find(|p| p.identifier == e.position_id || p.ticket == e.position_id)
                {
                    let movement = (p.price_current - e.trial.entry) * e.trial.side as f64;
                    if movement.is_finite() && now > e.last_position_ms {
                        e.position_mfe = e.position_mfe.max(movement);
                        e.position_mae = e.position_mae.max(-movement);
                        e.position_samples += 1;
                        e.last_position_ms = now;
                    }
                }
            }
        }
        self.quote(t);
        for e in self.active.values_mut() {
            // End stale trials with an explicit exclusion, never a fabricated outcome.
            if now.saturating_sub(e.opened_ms) > e.trial.hold_ms.saturating_mul(2) + 60_000
                && !e.trial.complete()
            {
                e.trial.usable = false;
                e.shadow_expired = true;
            }
            if (e.trial.complete() || e.shadow_expired) && e.actual.is_some() && !e.study_consumed {
                if let Some(rewards) = e.trial.paired_rewards() {
                    self.policies.entry(e.source.clone()).or_default().observe(
                        e.opened_ms,
                        now,
                        rewards,
                    );
                }
                e.study_consumed = true;
            }
        }
        self.flush(db)?;
        self.active.retain(|_, e| !e.complete());
        Ok(())
    }
}

pub fn view(db: &Connection, key: &str) -> Result<Value> {
    let mut st=db.prepare("SELECT payload FROM student_experiences WHERE context=?1 ORDER BY opened_ms DESC LIMIT 100")?;
    let mut experiences = Vec::new();
    let mut patterns: BTreeMap<String, u64> = BTreeMap::new();
    for raw in st.query_map([key], |r| r.get::<_, String>(0))? {
        let e: Experience = serde_json::from_str(&raw?)?;
        let v = e.review();
        for p in v["patterns"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            *patterns.entry(p.into()).or_default() += 1;
        }
        experiences.push(v);
    }
    let (total,closed,usable):(u64,u64,u64)=db.query_row("SELECT COUNT(*),COALESCE(SUM(json_type(payload,'$.actual')='object'),0),COALESCE(SUM(json_extract(payload,'$.trial.usable')=1 AND json_extract(payload,'$.study_consumed')=1),0) FROM student_experiences WHERE context=?1",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let mut st = db.prepare(
        "SELECT source,payload FROM student_exit_studies WHERE context=?1 ORDER BY source",
    )?;
    let studies = st
        .query_map([key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .map(|r| {
            let (source, raw) = r?;
            Ok(json!({"source":source,"study":serde_json::from_str::<Value>(&raw)?}))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"version":1,"context":key,"total_experiences":total,"closed_broker_trades":closed,"usable_shadow_pairs":usable,
        "recent_pattern_counts":patterns,"pattern_window":experiences.len(),"studies":studies,"experiences":experiences,
        "training_pairs":20,"validation_pairs":20,"gap_exclusion_ms":2000,
        "notes":["Actual learning uses complete booked Demo closes. Shadow results never become broker PnL.",
            "Exit policies train on 20 paired paths, freeze, then validate on 20 newly opened trades. Baseline remains available while learning.",
            "Quote gaps over 2 seconds exclude comparison. Observed extrema may miss between-snapshot moves. Swap is excluded from shadow estimates.",
            "Automatic exit application is Demo-only and affects future entries. A positive validation window does not guarantee later profit."]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::Position;
    fn telemetry() -> Telemetry {
        Telemetry {
            connected: true,
            demo: true,
            account_trade_mode: Some(0),
            login: 17,
            server: "Demo".into(),
            currency: "USD".into(),
            symbol: "XAUUSD".into(),
            bid: Some(100.),
            ask: Some(100.2),
            tick_size: Some(0.1),
            tick_value_profit: Some(1.),
            tick_value_loss: Some(1.),
            tick_age_ms: Some(0),
            quote_time_ms: Some(1000),
            ..Default::default()
        }
    }
    fn signal() -> Signal {
        Signal {
            brain: "trend".into(),
            side: 1,
            key: "fixture".into(),
            bar_time: 1,
            stop_distance: 1.,
            target_distance: 2.,
            hold_ms: 4000,
            reason: "trend fixture".into(),
            ready: true,
            coordination: None,
        }
    }
    #[test]
    fn future_entry_adaptation_scoped_to_definition_and_demo() {
        let mut t = telemetry();
        let b = BrainConfig {
            id: "trend".into(),
            ..Default::default()
        };
        let c = Config::default();
        let mut s = signal();
        adapt(&mut s, &t, &c, &b, &BTreeMap::new());
        let source = s.coordination.unwrap()["student"]["source"]
            .as_str()
            .unwrap()
            .to_owned();
        let policies = BTreeMap::from([(
            source,
            ExitStudy {
                active: ExitProfile::Quick,
                ..Default::default()
            },
        )]);
        let mut s = signal();
        adapt(&mut s, &t, &c, &b, &policies);
        assert_eq!(s.target_distance, 1.5);
        assert_eq!(s.hold_ms, 2000);
        t.tick_value_profit = Some(1.01);
        t.tick_value_loss = Some(1.02);
        let mut s = signal();
        adapt(&mut s, &t, &c, &b, &policies);
        assert_eq!(s.target_distance, 1.5);
        let mut changed = c.clone();
        changed.risk.commission_per_lot += 1.;
        let mut s = signal();
        adapt(&mut s, &t, &changed, &b, &policies);
        assert_eq!(s.target_distance, 2.);
        let mut disabled = c.clone();
        disabled.trading.adaptive_exits = false;
        let mut s = signal();
        adapt(&mut s, &t, &disabled, &b, &policies);
        assert_eq!(s.target_distance, 2.);
        t.demo = false;
        t.account_trade_mode = Some(2);
        let mut s = signal();
        adapt(&mut s, &t, &c, &b, &policies);
        assert!(s.coordination.is_none());
    }
    #[test]
    fn learned_sniper_exit_preserves_cost_fraction_without_blocking_entry() {
        let mut t=telemetry();t.point=Some(0.1);
        let mut c=Config::default();c.trading.mode=braintrade_domain::trading::TradingMode::Sniper;
        c.trading.sniper_target_points=19.;c.trading.max_cost_fraction=0.15;
        c.risk.commission_per_lot=1.;c.risk.slippage_ticks=0.;
        let b=BrainConfig{id:"system".into(),..Default::default()};
        let mut s=signal();adapt(&mut s,&t,&c,&b,&BTreeMap::new());
        let key=s.coordination.unwrap()["student"]["source"].as_str().unwrap().to_owned();
        let policies=BTreeMap::from([(key,ExitStudy{active:ExitProfile::Quick,..Default::default()})]);
        let mut s=signal();adapt(&mut s,&t,&c,&b,&policies);
        assert!(s.ready);assert_eq!(s.target_distance,2.);
        assert_eq!(s.coordination.unwrap()["student"]["cost_fallback"],true);
        c.trading.max_cost_fraction=0.5;
        let mut s=signal();adapt(&mut s,&t,&c,&b,&policies);
        assert!(s.ready);assert_eq!(s.target_distance,1.5);
        assert_eq!(s.coordination.unwrap()["student"]["cost_fallback"],false);
    }
    #[test]
    fn ledger_restart_exact_labels_and_shadow_isolation() {
        let db = Connection::open_in_memory().unwrap();
        let _service = crate::execution::Service::new(&db).unwrap();
        let mut student = Student::new(&db).unwrap();
        let mut t = telemetry();
        let key = crate::coordinator_learning::context(&t);
        let mut s = signal();
        adapt(
            &mut s,
            &t,
            &Config::default(),
            &BrainConfig {
                id: "trend".into(),
                ..Default::default()
            },
            &BTreeMap::new(),
        );
        let cmd = json!({"id":"one","brain":"trend","created_ms":1000,"commission_per_lot":7.,"signal":s,
            "learning":{"tick_size":0.1,"tick_value_profit":1.,"tick_value_loss":1.,"slippage_ticks":1.}});
        let result = json!({"deal":77,"price":100.,"volume":0.1,"acknowledged_ms":1000});
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result) VALUES('one',?1,'trend','open',1000,'filled',?2,?3)",params![key,cmd.to_string(),result.to_string()]).unwrap();
        t.bid = Some(101.6);
        t.ask = Some(101.8);
        t.quote_time_ms = Some(1500);
        student.observe(&db, &t, 1500).unwrap();
        assert_eq!(view(&db, &key).unwrap()["total_experiences"], 1);
        assert_eq!(view(&db, &key).unwrap()["closed_broker_trades"], 0);
        let mut student = Student::new(&db).unwrap();
        db.execute(
            "INSERT INTO broker_deals VALUES(?1,77,55,'trend','{}')",
            [&key],
        )
        .unwrap();
        t.positions = vec![Position {
            identifier: 55,
            price_current: 101.6,
            ..Default::default()
        }];
        t.quote_time_ms = Some(1750);
        student.observe(&db, &t, 1750).unwrap();
        t.positions[0].price_current = 102.6;
        t.bid = Some(102.6);
        t.ask = Some(102.8);
        t.quote_time_ms = Some(2000);
        student.observe(&db, &t, 2000).unwrap();
        assert_eq!(view(&db, &key).unwrap()["usable_shadow_pairs"], 0);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
        let label = json!({"open_command_id":"one","closed_ms":2100,"closed_volume":0.1,"net":-0.5,"attribution":"costs_erased_gross_profit"});
        db.execute(
            "INSERT INTO coordinator_broker_samples VALUES('fixture',55,?1,'trend',2100,?2)",
            params![key, label.to_string()],
        )
        .unwrap();
        t.positions.clear();
        t.quote_time_ms = Some(2250);
        student.observe(&db, &t, 2250).unwrap();
        let v = view(&db, &key).unwrap();
        assert_eq!(v["usable_shadow_pairs"], 1);
        assert_eq!(v["closed_broker_trades"], 1);
        assert_eq!(v["recent_pattern_counts"]["cost_drag"], 1);
        assert_eq!(v["experiences"][0]["actual"]["net"], -0.5);
        let mut student = Student::new(&db).unwrap();
        student.observe(&db, &t, 2500).unwrap();
        assert_eq!(view(&db, &key).unwrap()["total_experiences"], 1);
        assert_eq!(
            view(&db, &key).unwrap()["studies"][0]["study"]["samples"],
            1
        );
        t.login = 99;
        student.observe(&db, &t, 2500).unwrap();
        assert!(student.policies.is_empty());
        assert_eq!(
            view(&db, &crate::coordinator_learning::context(&t)).unwrap()["total_experiences"],
            0
        );
    }
}
