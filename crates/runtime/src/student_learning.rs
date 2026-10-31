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
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn source(context: &str, mode: &Value, brain: &str, timeframe: &str, report: &str) -> String {
    json!([context, "student-exits-v2", mode, brain, timeframe, report]).to_string()
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
        "sniper_stop":c.trading.sniper_stop_points,"sniper_hold":c.trading.sniper_max_hold_seconds, "sniper_pip_price":c.trading.sniper_pip_price,
        "commission":c.risk.commission_per_lot,"slippage":c.risk.slippage_ticks,"tick":t.tick_size}),
    );
    // Keep prior studies intact; new, known market regimes get their own comparison.
    if let Some(regime) = signal
        .coordination
        .as_ref()
        .and_then(|v| v["intelligence"]["regime"].as_str())
        .filter(|r| !r.is_empty() && *r != "unknown")
    {
        identity
            .as_array_mut()
            .unwrap()
            .push(json!({"exit_regime":regime}));
    }
    let key = identity.to_string();
    let study = policies.get(&key).cloned().unwrap_or_else(ExitStudy::expanded);
    let mut profile = if c.trading.adaptive_exits {
        study.active
    } else {
        ExitProfile::Baseline
    };
    // Preserve the user's Sniper cost fraction after a learned target change.
    // Falling back keeps the already-valid baseline entry available.
    let mut cost_fallback = false;
    let mut fallback_reason = "";
    // A user's chosen Sniper pips are strict even with Demo adaptive exits ON.
    // The exit learner may record counterfactual outcomes but must NEVER
    // extend or shorten the fixed/adaptive pip TP before the broker order.
    if c.trading.mode == braintrade_domain::trading::TradingMode::Sniper
        && matches!(b.id.as_str(), "system" | "sniper_learner")
        && profile != ExitProfile::Baseline
    {
        profile = ExitProfile::Baseline;
        fallback_reason = "sniper_pip_target_owned";
    }
    if profile != ExitProfile::Baseline {
        let (stop, _, _) = profile.geometry(signal.stop_distance, signal.target_distance, signal.hold_ms);
        if t.bid.zip(t.ask).zip(t.tick_size).is_some_and(|((bid,ask),tick)| stop <= ask-bid+tick) {
            profile = ExitProfile::Baseline;
            fallback_reason = "stop_inside_spread";
        }
    }
    if profile != ExitProfile::Baseline
        && matches!(b.id.as_str(), "system" | "sniper_learner")
        && c.trading.mode == braintrade_domain::trading::TradingMode::Sniper
    {
        let (_, target, _) =
            profile.geometry(signal.stop_distance, signal.target_distance, signal.hold_ms);
        let net_target = t
            .tick_size
            .zip(t.tick_value_profit)
            .filter(|(tick, value)| *tick > 0. && *value > 0.)
            .map(|(tick, value)| {
                (target / tick * value - c.risk.commission_per_lot - c.risk.slippage_ticks * value)
                    * b.lots
            });
        let affordable =
            braintrade_domain::trading::costs(t, c, b.lots, t.quote_time_ms.unwrap_or(0))
                .ok()
                .zip(net_target)
                .is_some_and(|(cost, net)| {
                    net > 0. && cost.total / (cost.total + net) <= c.trading.max_cost_fraction
                });
        if !affordable {
            profile = ExitProfile::Baseline;
            cost_fallback = true;
            fallback_reason = "sniper_cost_fraction";
        }
    }
    let baseline = json!({"stop":signal.stop_distance,"target":signal.target_distance,"hold_ms":signal.hold_ms});
    let coord = signal.coordination.get_or_insert_with(|| json!({}));
    coord["student"] = json!({"version":1,"exit_candidates":2,"entry_timing":1,"source":key,"brain":brain,"timeframe":timeframe,"report_id":report,
        "baseline":baseline,"profile":profile,"adaptive_exits":c.trading.adaptive_exits,
        "phase":if study.phase.is_empty(){"collecting"}else{&study.phase},"decision":study.last_decision,
        "validation_gain_r":study.last_gain_r,"cost_fallback":cost_fallback,"fallback_reason":fallback_reason,"basis":"prospective_paired_quote_trials"});
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
    #[serde(default)]
    admitted: Option<bool>,
    #[serde(default)]
    path: Vec<(u64, f64)>,
    #[serde(default)]
    path_overflow: bool,
    #[serde(default)]
    path_coverage: bool,
    #[serde(default)]
    loss_reviewed: bool,
}
impl Experience {
    fn observe_path(&mut self, bid: f64, ask: f64, now: u64) {
        if self.actual.is_some()
            || now < self.opened_ms
            || !bid.is_finite()
            || !ask.is_finite()
            || bid <= 0.
            || ask < bid
        {
            return;
        }
        let movement = (if self.trial.side > 0 { bid } else { ask } - self.trial.entry)
            * self.trial.side as f64;
        let last = self.path.last().map(|p| p.0).unwrap_or(self.opened_ms);
        if now <= last {
            return;
        }
        // Preserve every newly observed extreme, with 100 ms coverage checkpoints.
        if now - last < 100 && movement <= self.position_mfe && -movement <= self.position_mae {
            return;
        }
        if self.path.len() >= 8192 {
            self.path_overflow = true;
            return;
        }
        self.path.push((now, movement));
        self.position_samples = self.path.len() as u64;
        self.last_position_ms = now;
        self.position_mfe = self.position_mfe.max(movement);
        self.position_mae = self.position_mae.max(-movement);
    }
    fn finalize_path(&mut self) {
        let closed = self
            .actual
            .as_ref()
            .and_then(|v| v["closed_ms"].as_u64())
            .unwrap_or(0);
        if closed == 0 {
            return;
        }
        // The broker may report closure later. Never attribute post-close prices to the trade.
        self.path.retain(|p| p.0 <= closed);
        self.position_mfe = 0.;
        self.position_mae = 0.;
        let mut last = self.opened_ms;
        let mut gap = 0;
        for &(ms, movement) in &self.path {
            gap = gap.max(ms.saturating_sub(last));
            last = ms;
            self.position_mfe = self.position_mfe.max(movement);
            self.position_mae = self.position_mae.max(-movement);
        }
        self.position_samples = self.path.len() as u64;
        self.last_position_ms = last;
        self.path_coverage = !self.path_overflow
            && self.position_samples >= 2
            && gap <= 2000
            && closed.saturating_sub(last) <= 2000;
        self.path.clear();
    }
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
        let coverage = self.path_coverage && closed > 0;
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
            "position_path_usable":coverage,"shadow":self.trial,"shadow_expired":self.shadow_expired,"study_consumed":self.study_consumed,"admitted":self.admitted,
            "note":"Patterns describe observed paths, not proof of a cause. Shadow exits are estimates; actual PnL is booked MT5 data."})
    }
}

pub struct Student {
    context: String,
    active: BTreeMap<String, Experience>,
    pub policies: BTreeMap<String, ExitStudy>,
    // No prior aggregate may masquerade as a new provenance-verified policy.
    attested_studies:BTreeSet<String>,
    // Every contributing event is inserted atomically with the experience
    // and the policy. An aggregate digest without a replayable event trace is
    // not sufficient to prove that Student ever learned the displayed counts.
    pending_study_events:Vec<(String,String,String,Value)>,
    timing: timing::Learner,
}
#[path = "student_timing.rs"]
mod timing;

impl Student {
    pub fn capture_entries(&mut self, commands: Vec<Value>, now: u64) {
        for command in commands { self.timing.capture(&command, now); }
    }
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS student_experiences(context TEXT NOT NULL,command_id TEXT NOT NULL,source TEXT NOT NULL,opened_ms INTEGER NOT NULL,complete INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,command_id));
            CREATE INDEX IF NOT EXISTS student_pending ON student_experiences(context,complete,opened_ms);
            CREATE TABLE IF NOT EXISTS student_exit_studies(source TEXT PRIMARY KEY,context TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS student_study_admissions(
                source TEXT PRIMARY KEY,context TEXT NOT NULL,payload_sha256 TEXT NOT NULL,
                first_stamped_ms INTEGER NOT NULL,protocol TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS student_study_events(
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                context TEXT NOT NULL, source TEXT NOT NULL,
                command_id TEXT NOT NULL, kind TEXT NOT NULL,
                event_ms INTEGER NOT NULL, payload TEXT NOT NULL,
                UNIQUE(context,command_id,kind));
            CREATE TABLE IF NOT EXISTS student_opportunities(context TEXT NOT NULL,brain TEXT NOT NULL,minute INTEGER NOT NULL,reason TEXT NOT NULL,observations INTEGER NOT NULL,PRIMARY KEY(context,brain,minute,reason));
            CREATE INDEX IF NOT EXISTS student_opportunity_expiry ON student_opportunities(minute);")?;
        Ok(Self {
            context: String::new(),
            active: BTreeMap::new(),
            policies: BTreeMap::new(),
            attested_studies:BTreeSet::new(),
            pending_study_events:Vec::new(),
            timing: timing::Learner::new(db)?,
        })
    }
    fn flush(&self, db: &Connection) -> Result<()> {
        let tx = db.unchecked_transaction()?;
        for e in self.active.values() {
            tx.execute("INSERT INTO student_experiences VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(context,command_id) DO UPDATE SET complete=excluded.complete,payload=excluded.payload",
                params![e.context,e.command_id,e.source,e.opened_ms,e.complete(),serde_json::to_string(e)?])?;
        }
        for (source,cmd,kind,event) in &self.pending_study_events {
            tx.execute("INSERT OR IGNORE INTO student_study_events
                (context,source,command_id,kind,event_ms,payload)
                VALUES(?1,?2,?3,?4,?5,?6)",
                params![self.context,source,cmd,kind,
                    event["event_ms"].as_u64().unwrap_or(0),
                    serde_json::to_string(event)?])?;
        }
        for (key, study) in &self.policies {
            let raw=serde_json::to_string(study)?;
            tx.execute("INSERT OR REPLACE INTO student_exit_studies VALUES(?1,?2,?3)",
                params![key, self.context,&raw])?;
            // Only Student policies that were learned from G6-J admitted Demo
            // outcomes, or loaded with a valid pre-existing study digest, can
            // receive/update this local provenance stamp. Never sign legacy
            // aggregates merely because they were parsed from SQLite.
            if self.attested_studies.contains(key) &&
                student_study_replay_verified(&tx,&self.context,key,&raw,
                    u64::MAX)? {
                let sha=format!("{:x}",Sha256::digest(raw.as_bytes()));
                tx.execute("INSERT INTO student_study_admissions
                    (source,context,payload_sha256,first_stamped_ms,protocol)
                    VALUES(?1,?2,?3,CAST(strftime('%s','now') AS INTEGER)*1000,
                        'g6k-student-admitted-study-v1')
                    ON CONFLICT(source) DO UPDATE SET
                        payload_sha256=excluded.payload_sha256",
                    params![key,self.context,sha])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    fn load(&mut self, db: &Connection, key: &str, now:u64) -> Result<()> {
        if self.context == key {
            return Ok(());
        }
        self.flush(db)?;
        self.active.clear();
        self.policies.clear();
        self.attested_studies.clear();
        self.pending_study_events.clear();
        self.context = key.into();
        self.timing.load(db, key)?;
        let mut st=db.prepare("SELECT payload FROM student_experiences WHERE context=?1 AND complete=0 ORDER BY opened_ms")?;
        for raw in st.query_map([key], |r| r.get::<_, String>(0))? {
            let mut e: Experience = serde_json::from_str(&raw?)?;
            // Old cached student experiences may contain accepted broker labels
            // from before G5-E. Recheck them on EVERY load, not only fresh
            // coordinator_broker_samples, before any further exit learning.
            if e.actual.is_some() && !cached_student_label_verified(db,key,&e,now)? {
                e.actual=None;
                e.trial.usable=false;
                e.trial.exclusion_reason="broker_label_unverified".into();
                e.path_overflow=true;
                e.loss_reviewed=false;
                e.study_consumed=false;
            }
            self.active.insert(e.command_id.clone(), e);
        }
        let mut st =
            db.prepare("SELECT source,payload FROM student_exit_studies WHERE context=?1")?;
        for row in st.query_map([key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (k, raw) = row?;
            if !study_aggregate_attested(db,key,&k,&raw,now)? {
                // Preserve old aggregates in DB for historical audit but do
                // not apply or continue training an unverifiable exit policy.
                continue;
            }
            let study=serde_json::from_str::<ExitStudy>(&raw)?;
            self.attested_studies.insert(k.clone());
            self.policies.insert(k,study);
        }
        Ok(())
    }
    /// Fast pulses update only memory. SQLite persistence and study changes use snapshots.
    pub fn quote(&mut self, t: &Telemetry, now: u64) {
        if !t.demo
            || t.account_trade_mode != Some(0)
            || !t.connected
            || crate::coordinator_learning::context(t) != self.context
        {
            return;
        }
        let (Some(bid), Some(ask), Some(ms)) = (t.bid, t.ask, t.quote_time_ms) else {
            return;
        };
        self.timing.quote(t, now);
        for e in self.active.values_mut() {
            if ms >= e.opened_ms && t.tick_age_ms.is_some_and(|age| age <= 1000) {
                e.observe_path(bid, ask, now);
            }
            if !e.trial.complete() && !e.shadow_expired {
                e.trial.observe_at(
                    bid,
                    ask,
                    ms,
                    now,
                    t.tick_age_ms.is_some_and(|age| age <= 1000),
                );
            }
        }
    }
    pub fn observe(&mut self, db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
        self.observe_gated(db,t,now,false)
    }
    pub fn observe_gated(&mut self, db: &Connection, t: &Telemetry, now: u64, frozen:bool) -> Result<()> {
        if !t.connected || !t.demo || t.account_trade_mode != Some(0) {
            return Ok(());
        }
        let key = crate::coordinator_learning::context(t);
        self.load(db, &key, now)?;
        // A policy attested on a previous pulse must not keep controlling
        // exits if any of its original Broker/paired events were edited while
        // Core stayed connected to this same account. Recheck the full replay
        // before it can be used on the next decision; baseline remains.
        let mut revoked=Vec::new();
        for (source,study) in &self.policies {
            let raw=serde_json::to_string(study)?;
            if !study_aggregate_attested(db,&key,source,&raw,now)? {
                revoked.push(source.clone());
            }
        }
        for source in revoked {
            self.policies.remove(&source);
            self.attested_studies.remove(&source);
        }
        if frozen {
            // Keep already-observed paths and revoke damaged policies, but do
            // not consume outcomes, train studies or register new trials.
            // Broker exit management and reconciliation are separate.
            self.quote(t,now);
            self.timing.persist(db)?;
            self.flush(db)?;
            return Ok(());
        }
        self.timing.restore(db, &key, now)?;
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
            // Repair old recovered acknowledgements as well as newly normalized ones.
            let history: Option<String> = db
                .query_row(
                    "SELECT payload FROM broker_deals WHERE context=?1 AND ticket=?2",
                    params![key, r["deal"].as_u64().unwrap_or(0)],
                    |row| row.get(0),
                )
                .optional()?;
            let history: Value = history
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or(Value::Null);
            let recovered = r["recovered_from_history"] == true
                || r["price"].as_f64().is_none_or(|v| v <= 0.)
                || r["volume"].as_f64().is_none_or(|v| v <= 0.);
            let mut entry = r["price"].as_f64().unwrap_or(0.);
            let mut volume = r["volume"].as_f64().unwrap_or(0.);
            if (entry <= 0. || volume <= 0.)
                && history["position_id"].as_u64().is_some_and(|p| p > 0)
            {
                let mut fills = db.prepare(
                    "SELECT payload FROM broker_deals WHERE context=?1 AND position_id=?2",
                )?;
                let mut total = 0.;
                let mut value = 0.;
                for row in fills.query_map(
                    params![key, history["position_id"].as_u64().unwrap()],
                    |row| row.get::<_, String>(0),
                )? {
                    let d: Value = serde_json::from_str(&row?)?;
                    if d["entry"] == 0 {
                        let v = d["volume"].as_f64().unwrap_or(0.);
                        let p = d["price"].as_f64().unwrap_or(0.);
                        if v > 0. && p > 0. {
                            total += v;
                            value += v * p;
                        }
                    }
                }
                if total > 0. {
                    entry = value / total;
                    volume = total;
                }
            }
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
            let opened = if recovered {
                r["fill_time_ms"].as_u64().or(history["time_msc"].as_u64())
            } else {
                r["acknowledged_ms"].as_u64()
            }
            .unwrap_or(cmd["created_ms"].as_u64().unwrap_or(now));
            let e = Experience {
                command_id: id.into(),
                context: key.clone(),
                source: m["source"].as_str().unwrap_or("").into(),
                brain: m["brain"].as_str().unwrap_or("").into(),
                owner: cmd["brain"].as_str().unwrap_or("").into(),
                open_deal: r["deal"].as_u64().unwrap(),
                position_id: r["position_id"]
                    .as_u64()
                    .or(history["position_id"].as_u64())
                    .unwrap_or(0),
                volume,
                opened_ms: opened,
                entry: json!({"learning":l,"signal":cmd["signal"],"volume":volume,"fill_price":entry,"timestamp_basis":if recovered{"broker_history"}else{"local_acknowledgement"},"recovered":recovered,"request":r["request"]}),
                trial: Trial {
                    opened_ms: opened,
                    side: cmd["signal"]["side"].as_i64().unwrap_or(0) as i8,
                    entry,
                    stop,
                    target,
                    hold_ms: baseline["hold_ms"].as_u64().unwrap_or(1000),
                    deadline_ms: None,
                    tick_size: tick,
                    profit_value: profit,
                    loss_value: loss,
                    commission_per_lot: cmd["commission_per_lot"].as_f64().unwrap_or(0.),
                    exit_slippage: l["slippage_ticks"].as_f64().unwrap_or(0.) * tick,
                    last_quote_ms: opened,
                    samples: 0,
                    max_gap_ms: 0,
                    last_observed_ms: opened,
                    max_observation_gap_ms: 0,
                    exclusion_reason: if recovered {
                        "history_recovery".into()
                    } else if r["acknowledged_ms"].as_u64().is_none() {
                        "missing_fill_time".into()
                    } else {
                        String::new()
                    },
                    usable: !recovered && r["acknowledged_ms"].as_u64().is_some(),
                    mfe: 0.,
                    mae: 0.,
                    outcomes: vec![None; if m["exit_candidates"] == 2 { 5 } else { 3 }],
                },
                actual: None,
                position_mfe: 0.,
                position_mae: 0.,
                position_samples: 0,
                last_position_ms: 0,
                study_consumed: false,
                shadow_expired: false,
                admitted: Some(false),
                path: Vec::new(),
                path_overflow: recovered,
                path_coverage: false,
                loss_reviewed: false,
            };
            self.active.insert(id.into(), e);
        }
        // Even while remaining in the same account/context, an on-disk
        // broker receipt or its admission may become damaged between observe
        // cycles. Cached actual losses must be revalidated before ANY further
        // Student loss-memory/study learning, not only during restart/load.
        for e in self.active.values_mut() {
            if e.actual.is_some() && !cached_student_label_verified(db,&key,e,now)? {
                e.actual=None;
                e.trial.usable=false;
                e.trial.exclusion_reason="broker_admission_unverified".into();
                e.path_overflow=true;
                e.loss_reviewed=false;
                e.study_consumed=false;
            }
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
            if e.actual.is_none() && e.position_id>0 {
                if let Some(label)=verified_student_accepted_label(
                    db,&key,&e.owner,&e.command_id,e.position_id,now)? {
                    if label["closed_volume"]
                        .as_f64()
                        .is_some_and(|volume| (volume - e.volume).abs() > 1e-8)
                    {
                        e.trial.usable = false;
                        e.trial.exclusion_reason = "volume_changed".into();
                        e.path_overflow = true;
                    }
                    e.actual = Some(label);
                    e.finalize_path();
                }
            }
        }
        self.quote(t, now);
        for e in self.active.values_mut() {
            if !e.loss_reviewed && e.actual.is_some() {
                if e.actual
                    .as_ref()
                    .and_then(|v| v["net"].as_f64())
                    .is_some_and(|n| n < 0.)
                {
                    let patterns: Vec<String> = e.review()["patterns"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect();
                    self.policies
                        .entry(e.source.clone())
                        .or_insert_with(|| if e.trial.outcomes.len() == 5 { ExitStudy::expanded() } else { ExitStudy::default() })
                        .record_loss(&patterns);
                    self.attested_studies.insert(e.source.clone());
                    self.pending_study_events.push((
                        e.source.clone(),e.command_id.clone(),"loss".into(),
                        json!({"event_ms":now,"patterns":patterns})
                    ));
                }
                e.loss_reviewed = true;
            }
            // End stale trials with an explicit exclusion, never a fabricated outcome.
            if now.saturating_sub(e.opened_ms) > e.trial.hold_ms.saturating_mul(2) + 60_000
                && !e.trial.complete()
            {
                e.trial.usable = false;
                if e.trial.exclusion_reason.is_empty() {
                    e.trial.exclusion_reason = "expired".into();
                }
                e.shadow_expired = true;
            }
            if (e.trial.complete() || e.shadow_expired) && e.actual.is_some() && !e.study_consumed {
                let study = self.policies.entry(e.source.clone()).or_insert_with(|| if e.trial.outcomes.len() == 5 { ExitStudy::expanded() } else { ExitStudy::default() });
                if let Some(rewards) = e.trial.paired_rewards() {
                    let accepted=study.observe(e.opened_ms, now, &rewards);
                    e.admitted=Some(accepted);
                    if accepted {
                        self.attested_studies.insert(e.source.clone());
                        self.pending_study_events.push((
                            e.source.clone(),e.command_id.clone(),"paired".into(),
                            json!({"event_ms":now,"opened_ms":e.opened_ms,
                                   "rewards":rewards})
                        ));
                    }
                } else if e.trial.usable && e.trial.samples < 2 {
                    e.trial.usable = false;
                    e.trial.exclusion_reason = "too_few_quotes".into();
                }
                e.study_consumed = true;
            }
        }
        self.timing.finish(db, now)?;
        self.flush(db)?;
        self.pending_study_events.clear();
        self.active.retain(|_, e| !e.complete());
        Ok(())
    }
}

/// Sampled only by regular snapshots, never by the fast quote path. Counts are
/// observations of blockers, not a fabricated count of missed profitable trades.
pub fn log_opportunities(
    db: &Connection,
    t: &Telemetry,
    decisions: &[(String, Value)],
    now: u64,
) -> Result<()> {
    if !t.connected || !t.demo || t.account_trade_mode != Some(0) {
        return Ok(());
    }
    let context = crate::coordinator_learning::context(t);
    let tx = db.unchecked_transaction()?;
    for (brain, v) in decisions {
        let reasons: Vec<String> = if v["signal"]["ready"] != true {
            vec!["الإشارة غير جاهزة".into()]
        } else {
            v["gates"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r.as_str().map(str::to_owned))
                .collect()
        };
        for reason in reasons {
            tx.execute("INSERT INTO student_opportunities VALUES(?1,?2,?3,?4,1) ON CONFLICT(context,brain,minute,reason) DO UPDATE SET observations=observations+1",params![context,brain,now/60_000,reason])?;
        }
    }
    tx.execute(
        "DELETE FROM student_opportunities WHERE minute<?1",
        [now.saturating_sub(7 * 86_400_000) / 60_000],
    )?;
    tx.commit()?;
    Ok(())
}

fn verified_student_broker_label(p:&Value,position_id:u64)->bool{
    position_id>0
        &&p["position_id"].as_u64()==Some(position_id)
        &&crate::coordinator_learning::verified_broker_feedback(p).is_some()
}

// G6-J: Student is a separate learning engine. The original broker close
// timestamp only says when MT5 booked the outcome; it is NOT proof that the
// Student had seen it then. A legitimate current label needs the exact
// G6-G first-Core-admission receipt stamped in the same SQLite transaction.
// Legacy unadmitted rows remain in the ledger but are not training evidence.
fn verified_student_accepted_label(db:&Connection,ctx:&str,owner:&str,
    cmd:&str,position_id:u64,now:u64)->Result<Option<Value>>{
    if position_id==0 || cmd.is_empty() || owner.is_empty(){return Ok(None);}
    let mut st=db.prepare(
        "SELECT s.source,s.position_id,s.context,s.brain,s.closed_ms,s.payload,
                a.context,a.brain,a.closed_ms,a.admitted_ms,a.payload_sha256,a.protocol
         FROM coordinator_broker_samples s
         LEFT JOIN coordinator_broker_admissions a
             ON a.source=s.source AND a.position_id=s.position_id
         WHERE s.context=?1 AND s.brain=?2 AND s.position_id=?3
         ORDER BY s.closed_ms DESC LIMIT 30")?;
    let rows=st.query_map(params![ctx,owner,position_id],|r|Ok((
        r.get::<_,String>(0)?,r.get::<_,i64>(1).unwrap_or(-1),
        r.get::<_,String>(2)?,r.get::<_,String>(3)?,
        r.get::<_,i64>(4).unwrap_or(-1),r.get::<_,String>(5)?,
        r.get::<_,Option<String>>(6).unwrap_or(None),
        r.get::<_,Option<String>>(7).unwrap_or(None),
        r.get::<_,Option<i64>>(8).unwrap_or(None),
        r.get::<_,Option<i64>>(9).unwrap_or(None),
        r.get::<_,Option<String>>(10).unwrap_or(None),
        r.get::<_,Option<String>>(11).unwrap_or(None)
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut accepted:Option<Value>=None;
    for(source,pos,stored_ctx,stored_brain,closed,raw,actx,abrain,
        aclosed,admitted,hash,protocol)in rows{
        let (Ok(pos),Ok(closed),Some(admitted))=(
            u64::try_from(pos),u64::try_from(closed),
            admitted.and_then(|v|u64::try_from(v).ok()))else{continue;};
        if pos!=position_id||closed==0||admitted<closed||admitted>now
            ||actx.as_deref()!=Some(ctx)||abrain.as_deref()!=Some(owner)
            ||aclosed!=i64::try_from(closed).ok()
            ||protocol.as_deref()!=Some("g6g-runtime-same-transaction-v1")
            ||hash.as_deref()!=Some(format!("{:x}",Sha256::digest(raw.as_bytes())).as_str())
            ||stored_ctx!=ctx||stored_brain!=owner
        {continue;}
        let Ok(p)=serde_json::from_str::<Value>(&raw)else{continue;};
        if p["context"]!=ctx||p["brain"]!=owner
            ||p["open_command_id"]!=cmd
            ||p["closed_ms"].as_u64()!=Some(closed)
            ||!verified_student_broker_label(&p,pos){continue;}
        let Ok(source_value)=serde_json::from_str::<Value>(&source)else{continue;};
        let Some(parts)=source_value.as_array().filter(|p|p.len()==6)else{continue;};
        if parts[0]!=ctx||parts[1]!="adaptive-member-v1"
            ||parts[3]!=owner||p["trading_mode"]!=parts[2]
            ||p["timeframe"]!=parts[4]||p["report_id"]!=parts[5]
        {continue;}
        // Multiple apparently valid same-position labels are ambiguous.
        // Never choose the first by SQL order and invent Student credit.
        if accepted.is_some(){return Ok(None);}
        accepted=Some(p);
    }
    Ok(accepted)
}

fn cached_student_label_verified(db:&Connection,ctx:&str,e:&Experience,now:u64)->Result<bool>{
    let Some(saved)=e.actual.as_ref()else{return Ok(false);};
    Ok(verified_student_accepted_label(db,ctx,&e.owner,&e.command_id,e.position_id,now)?
        .as_ref().is_some_and(|current|current==saved))
}

// A G6-K aggregate SHA is only self-consistent; it cannot establish its
// original per-trade inputs. G6-L verifies the actual immutable broker label
// and exact paired quote rewards for EVERY contributing event, then rebuilds
// ExitStudy deterministically in its original observation order. No event
// evidence => old G6-K counters are not considered trainable.
fn student_study_replay_verified(db:&Connection,ctx:&str,source:&str,
    raw:&str,now:u64)->Result<bool>{
    let mut st=db.prepare(
        "SELECT command_id,kind,event_ms,payload FROM student_study_events
         WHERE context=?1 AND source=?2 ORDER BY seq")?;
    let rows=st.query_map(params![ctx,source],|r|Ok((
        r.get::<_,String>(0)?,r.get::<_,String>(1)?,
        r.get::<_,i64>(2).unwrap_or(-1),r.get::<_,String>(3)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty(){return Ok(false);}
    let mut replay:Option<ExitStudy>=None;
    let mut seen=BTreeSet::new();
    for(cmd,kind,event_time,content)in rows {
        let Ok(at)=u64::try_from(event_time)else{return Ok(false);};
        if at==0 || at>now || !seen.insert((cmd.clone(),kind.clone())){
            return Ok(false);
        }
        let Some(payload):Option<String>=db.query_row(
            "SELECT payload FROM student_experiences
             WHERE context=?1 AND command_id=?2",
            params![ctx,&cmd],|r|r.get(0)).optional()? else{return Ok(false);};
        let Ok(e)=serde_json::from_str::<Experience>(&payload)else{return Ok(false);};
        if e.source!=source || e.context!=ctx || e.command_id!=cmd
            || !cached_student_label_verified(db,ctx,&e,at)? {
            return Ok(false);
        }
        let Ok(ev)=serde_json::from_str::<Value>(&content)else{return Ok(false);};
        if ev["event_ms"].as_u64()!=Some(at) {return Ok(false);}
        let study=replay.get_or_insert_with(||if e.trial.outcomes.len()==5 {
            ExitStudy::expanded()
        }else{ExitStudy::default()});
        match kind.as_str(){
            "loss"=>{
                if !e.loss_reviewed ||
                    !e.actual.as_ref().and_then(|a|a["net"].as_f64()).is_some_and(|n|n<0.)
                {return Ok(false);}
                let patterns=e.review()["patterns"].as_array().cloned().unwrap_or_default();
                if ev["patterns"]!=json!(patterns){return Ok(false);}
                let Some(patterns)=patterns.iter().map(|v|v.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()else{return Ok(false);};
                study.record_loss(&patterns);
            },
            "paired"=>{
                if !e.study_consumed || e.admitted!=Some(true)
                    || at<e.opened_ms {return Ok(false);}
                let Some(rewards)=e.trial.paired_rewards()else{return Ok(false);};
                if ev["opened_ms"].as_u64()!=Some(e.opened_ms)
                    || ev["rewards"]!=json!(rewards){return Ok(false);}
                if !study.observe(e.opened_ms,at,rewards){return Ok(false);}
            },
            _=>return Ok(false)
        }
    }
    let Some(replay)=replay else{return Ok(false);};
    let Ok(expected)=serde_json::from_str::<Value>(raw)else{return Ok(false);};
    Ok(serde_json::to_value(replay)?==expected)
}

fn study_aggregate_attested(db:&Connection,ctx:&str,source:&str,raw:&str,now:u64)->Result<bool>{
    let r:Option<(String,String,String)>=db.query_row(
        "SELECT context,payload_sha256,protocol FROM student_study_admissions
         WHERE source=?1",[source],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    Ok(r.is_some_and(|(stored_ctx,hash,protocol)|
        stored_ctx==ctx && protocol=="g6k-student-admitted-study-v1"
        && hash==format!("{:x}",Sha256::digest(raw.as_bytes())))
        && student_study_replay_verified(db,ctx,source,raw,now)?)
}

pub fn view(db: &Connection, key: &str) -> Result<Value> {
    // Dashboard read time is NOT the MT5 close time. Future local admissions
    // must not make cached losses/learning appear settled before they arrive.
    let observed_now=SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d|u64::try_from(d.as_millis()).unwrap_or(u64::MAX)).unwrap_or(0);
    let mut st=db.prepare("SELECT payload FROM student_experiences WHERE context=?1 ORDER BY opened_ms DESC LIMIT 100")?;
    let mut experiences = Vec::new();
    let mut patterns: BTreeMap<String, u64> = BTreeMap::new();
    let mut exclusions: BTreeMap<String, u64> = BTreeMap::new();
    let mut recent_net = 0.;
    let mut recent_closed = 0;
    for raw in st.query_map([key], |r| r.get::<_, String>(0))? {
        let mut e: Experience = serde_json::from_str(&raw?)?;
        if e.actual.is_some()&&!cached_student_label_verified(db,key,&e,observed_now)?{
            // Read-only revalidation for old persisted experiences: never
            // paint invalid broker PnL, win/loss patterns or training progress.
            e.actual=None;
            e.trial.usable=false;
            e.trial.exclusion_reason="broker_label_unverified".into();
        }
        let v = e.review();
        if let Some(net) = e.actual.as_ref().and_then(|v| v["net"].as_f64()) {
            recent_net += net;
            recent_closed += 1;
        }
        if !e.trial.usable {
            *exclusions
                .entry(if e.trial.exclusion_reason.is_empty() {
                    "legacy_quality_unknown".into()
                } else {
                    e.trial.exclusion_reason.clone()
                })
                .or_default() += 1;
        }
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
    let total:u64=db.query_row("SELECT COUNT(*) FROM student_experiences WHERE context=?1",
        [key],|r|r.get(0))?;
    // Do not count arbitrary legacy JSON actual blobs as MT5 closes.
    // This is read-only; underlying SQLite records remain available for audit.
    let mut counted=db.prepare("SELECT payload FROM student_experiences
        WHERE context=?1 AND json_type(payload,'$.actual')='object'")?;
    let candidates=counted.query_map([key],|r|r.get::<_,String>(0))?;
    let mut closed=0u64;
    for raw in candidates {
        let Ok(e)=serde_json::from_str::<Experience>(&raw?) else{continue;};
        if cached_student_label_verified(db,key,&e,observed_now)? {
            closed+=1;
        }
    }
    let mut st = db.prepare(
        "SELECT source,payload FROM student_exit_studies WHERE context=?1 ORDER BY source",
    )?;
    let studies = st
        .query_map([key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .map(|r| {
            let (source, raw) = r?;
            let attested=study_aggregate_attested(db,key,&source,&raw,observed_now)?;
            Ok(if attested {
                json!({"source":source,"provenance":"locally_replayed_g6l",
                       "study":serde_json::from_str::<Value>(&raw)?})
            } else {
                // Never display a forged old promotion, learned profile or
                // aggregate shadow wins as verified training performance.
                json!({"source":source,"provenance":"legacy_or_tampered_unverified",
                       "study":Value::Null})
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let admitted: u64 = studies
        .iter()
        .filter_map(|s| s["study"]["total_pairs"].as_u64())
        .sum();
    let mut st=db.prepare("SELECT brain,reason,SUM(observations) FROM student_opportunities WHERE context=?1 AND minute >= CAST(strftime('%s','now') AS INTEGER)/60-10080 GROUP BY brain,reason ORDER BY SUM(observations) DESC LIMIT 12")?;
    let blockers=st.query_map([key],|r|Ok(json!({"brain":r.get::<_,String>(0)?,"reason":r.get::<_,String>(1)?,"observations":r.get::<_,u64>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(
        json!({"version":3,"context":key,"entry_timing":timing::view(db,key)?,"total_experiences":total,"closed_broker_trades":closed,"usable_shadow_pairs":admitted,
        "recent_exclusions":exclusions,"recent_net":recent_net,"recent_closed":recent_closed,"blocker_observations":blockers,
        "recent_pattern_counts":patterns,"pattern_window":experiences.len(),"studies":studies,"experiences":experiences,
        "training_pairs":20,"validation_pairs":20,"gap_exclusion_ms":2000,
        "notes":["Actual learning uses complete booked Demo closes. Shadow results never become broker PnL.",
            "Exit policies train on 20 paired paths, freeze, then validate on 20 newly opened trades. Baseline remains available while learning.",
            "Receipt gaps over 2 seconds exclude comparison; quiet connected heartbeats do not. Observed extrema may miss between-sample moves. Swap is excluded from shadow estimates.",
            "Automatic exit application is Demo-only and affects future entries. A positive validation window does not guarantee later profit."]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test-only: convert old synthetic source='fixture' to a receipt that
    // matches Rust's actual canonical member key, then stamp an original
    // first-Core-admission SHA. Production cannot backdate legacy rows.
    fn stamp_student_fixture(db:&Connection,ctx:&str,admitted:u64){
        let mut st=db.prepare("SELECT source,position_id,closed_ms,payload
          FROM coordinator_broker_samples WHERE context=?1 AND brain='trend'").unwrap();
        let rows=st.query_map([ctx],|r|Ok((
            r.get::<_,String>(0)?,r.get::<_,i64>(1)?,
            r.get::<_,i64>(2)?,r.get::<_,String>(3)?
        ))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        drop(st);
        for(source,pos,closed,raw)in rows{
            if pos<=0||closed<=0||u64::try_from(closed).unwrap()>admitted {continue;}
            let Ok(mut label)=serde_json::from_str::<Value>(&raw)else{continue;};
            label["context"]=json!(ctx);label["brain"]=json!("trend");
            label["trading_mode"]=json!("expert");
            label["timeframe"]=json!("M1");label["report_id"]=json!("");
            let new_source=json!([ctx,"adaptive-member-v1","expert","trend","M1",""]).to_string();
            let normalized=label.to_string();
            db.execute("UPDATE coordinator_broker_samples SET source=?1,payload=?2
                WHERE source=?3 AND position_id=?4",
                params![&new_source,&normalized,&source,pos]).unwrap();
            let hash=format!("{:x}",Sha256::digest(normalized.as_bytes()));
            db.execute("INSERT OR IGNORE INTO coordinator_broker_admissions
                (source,position_id,context,brain,closed_ms,admitted_ms,payload_sha256,protocol)
                VALUES(?1,?2,?3,'trend',?4,?5,?6,'g6g-runtime-same-transaction-v1')",
                params![&new_source,pos,ctx,closed,admitted,hash]).unwrap();
        }
    }
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
        let mut t = telemetry();
        t.point = Some(0.1);
        let mut c = Config::default();
        c.trading.mode = braintrade_domain::trading::TradingMode::Sniper;
        c.trading.sniper_target_points = 19.;
        c.trading.max_cost_fraction = 0.15;
        c.risk.commission_per_lot = 1.;
        c.risk.slippage_ticks = 0.;
        let b = BrainConfig {
            id: "system".into(),
            ..Default::default()
        };
        let mut s = signal();
        adapt(&mut s, &t, &c, &b, &BTreeMap::new());
        let key = s.coordination.unwrap()["student"]["source"]
            .as_str()
            .unwrap()
            .to_owned();
        let policies = BTreeMap::from([(
            key,
            ExitStudy {
                active: ExitProfile::Quick,
                ..Default::default()
            },
        )]);
        let mut s = signal();
        adapt(&mut s, &t, &c, &b, &policies);
        assert!(s.ready);
        assert_eq!(s.target_distance, 2.);
        assert_eq!(s.coordination.unwrap()["student"]["fallback_reason"], "sniper_pip_target_owned");
        c.trading.max_cost_fraction = 0.5;
        let mut s = signal();
        adapt(&mut s, &t, &c, &b, &policies);
        assert!(s.ready);
        assert_eq!(s.target_distance, 2.);
        assert_eq!(s.coordination.unwrap()["student"]["cost_fallback"], false);
    }
    #[test]
    fn adaptive_sniper_pip_arm_cannot_be_relabelled_by_exit_profile() {
        let mut t=telemetry();
        t.point=Some(0.01);
        let mut c=Config::default();
        c.trading.mode=braintrade_domain::trading::TradingMode::Sniper;
        c.trading.sniper_target_mode=braintrade_domain::trading::SniperTargetMode::Adaptive;
        let b=BrainConfig{id:"system".into(),..Default::default()};
        let mut entry=signal();
        entry.coordination=Some(json!({"selection":{
            "brain":"sniper_learner","policy":"sniper-ticks-v1",
            "sniper_target_mode":"adaptive","sniper_target_arm":2,"sniper_target_pips":6.0,
            "sniper_definition":"fixture","sniper_regime":"directional_normal",
            "definition":["sniper-ticks-v1","impulse"],"timeframe":"M1"
        }}));
        let before_target=entry.target_distance;
        let mut baseline=entry.clone();
        adapt(&mut baseline,&t,&c,&b,&BTreeMap::new());
        let key=baseline.coordination.as_ref().unwrap()["student"]["source"].as_str().unwrap().to_string();
        let policies=BTreeMap::from([(key,ExitStudy{active:ExitProfile::Quick,..Default::default()})]);
        adapt(&mut entry,&t,&c,&b,&policies);
        assert_eq!(entry.target_distance,before_target);
        assert_eq!(entry.hold_ms,4000);
        let coord=entry.coordination.unwrap();
        assert_eq!(coord["student"]["profile"],"baseline");
        assert_eq!(coord["student"]["fallback_reason"],"sniper_pip_target_owned");
        assert_eq!(coord["selection"]["sniper_target_arm"],2);
        // Fixed targets are also owned by the user and cannot be enlarged by learning.
        c.trading.sniper_target_mode=braintrade_domain::trading::SniperTargetMode::Fixed;
        let mut fixed=signal();
        adapt(&mut fixed,&t,&c,&b,&policies);
        assert!(fixed.ready);
        assert_eq!(fixed.target_distance,before_target);
        assert_eq!(fixed.coordination.unwrap()["student"]["profile"],"baseline");
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
        t.bid = Some(104.);
        t.ask = Some(104.2);
        t.quote_time_ms = Some(2000);
        student.observe(&db, &t, 2000).unwrap();
        assert_eq!(view(&db, &key).unwrap()["usable_shadow_pairs"], 0);
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
        let label = json!({"open_command_id":"one","position_id":55,"closed_ms":2100,
            "evidence_source":"mt5_demo_broker","gross":-0.5,"commission":0.0,
            "swap":0.0,"fee":0.0,"net":-0.5,"net_per_lot":-5.0,
            "closed_volume":0.1,"risk_money":0.0,"r_multiple":0.0,"reward":-0.25,
            "attribution":"costs_erased_gross_profit"});
        db.execute(
            "INSERT INTO coordinator_broker_samples VALUES('fixture',55,?1,'trend',2100,?2)",
            params![key, label.to_string()],
        )
        .unwrap();
        stamp_student_fixture(&db,&key,2100);
        t.positions.clear();
        t.quote_time_ms = Some(2250);
        student.observe_gated(&db,&t,2250,true).unwrap();
        let pending_events:u64=db.query_row("SELECT COUNT(*) FROM student_study_events",[],|r|r.get(0)).unwrap();
        assert_eq!(pending_events,0,"Paused Student must not train or consume a verified outcome");
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
        db.execute("UPDATE student_experiences SET payload=json_remove(payload,'$.admitted') WHERE context=?1",[&key]).unwrap();
        let legacy = view(&db, &key).unwrap();
        assert!(legacy["experiences"][0]["admitted"].is_null());
        // Legacy missing admission cannot preserve a supposedly replayable
        // quote-pair counter. The raw record remains for forensic review.
        assert_eq!(legacy["usable_shadow_pairs"], 0);
        assert_eq!(legacy["studies"][0]["provenance"],
            "legacy_or_tampered_unverified");
        t.login = 99;
        student.observe(&db, &t, 2500).unwrap();
        assert!(student.policies.is_empty());
        assert_eq!(
            view(&db, &crate::coordinator_learning::context(&t)).unwrap()["total_experiences"],
            0
        );
    }
    fn seed_entry(db: &Connection, t: &Telemetry, result: Value) -> String {
        let key = crate::coordinator_learning::context(t);
        let mut s = signal();
        adapt(
            &mut s,
            t,
            &Config::default(),
            &BrainConfig {
                id: "trend".into(),
                ..Default::default()
            },
            &BTreeMap::new(),
        );
        let cmd = json!({"id":"repair","brain":"trend","created_ms":1000,"commission_per_lot":7.,"signal":s,
            "learning":{"tick_size":0.1,"tick_value_profit":1.,"tick_value_loss":1.,"slippage_ticks":0.}});
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result) VALUES('repair',?1,'trend','open',1000,'filled',?2,?3)",params![key,cmd.to_string(),result.to_string()]).unwrap();
        key
    }
    fn label(db: &Connection, key: &str, closed: u64) {
        db.execute("INSERT INTO coordinator_broker_samples VALUES('fixture',55,?1,'trend',?2,?3)",params![key,closed,json!({"open_command_id":"repair","position_id":55,"closed_ms":closed,
            "evidence_source":"mt5_demo_broker","gross":-0.5,"commission":0.0,
            "swap":0.0,"fee":0.0,"net":-0.5,"net_per_lot":-5.0,
            "closed_volume":0.1,"risk_money":0.0,"r_multiple":0.0,"reward":-0.25,
            "attribution":"costs_erased_gross_profit"}).to_string()]).unwrap();
        stamp_student_fixture(db,key,closed);
    }
    #[test]
    fn g5f_student_never_converts_fake_mt5_reward_into_real_loss_memory() {
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let t=telemetry();
        let key=seed_entry(&db,&t,json!({"deal":77}));
        db.execute("INSERT INTO broker_deals VALUES(?1,77,55,'trend',?2)",
            params![key,json!({"entry":0,"position_id":55,"price":100.,
                "volume":0.1,"time_msc":1000}).to_string()]).unwrap();
        label(&db,&key,1200);
        db.execute("UPDATE coordinator_broker_samples SET
            payload=json_set(payload,'$.reward',0.99) WHERE position_id=55",[]).unwrap();
        let mut student=Student::new(&db).unwrap();
        student.observe(&db,&t,5000).unwrap();
        let v=view(&db,&key).unwrap();
        assert_eq!(v["closed_broker_trades"],0,
            "Even a correct booked net cannot be credited with a forged reward");
        assert_eq!(v["usable_shadow_pairs"],0);
        assert_eq!(v["experiences"][0]["actual"],Value::Null);
        db.execute("UPDATE coordinator_broker_samples SET
            payload=json_set(payload,'$.reward',-0.25) WHERE position_id=55",[]).unwrap();
        let mut restarted=Student::new(&db).unwrap();
        restarted.observe(&db,&t,5500).unwrap();
        let v=view(&db,&key).unwrap();
        assert_eq!(v["closed_broker_trades"],1,
            "After exact MT5 feedback is restored, matching broker evidence can be read");
        assert_eq!(v["usable_shadow_pairs"],0,
            "Unverified earlier sampled path cannot be promoted after restoration");
        // Emulate an old persisted Experience.actual corrupted after the
        // actual broker ledger was accepted. Reports must not count it.
        db.execute("UPDATE student_experiences SET
            payload=json_set(payload,'$.actual.reward',0.99) WHERE context=?1",
            [&key]).unwrap();
        let archived=view(&db,&key).unwrap();
        assert_eq!(archived["closed_broker_trades"],0);
        assert_eq!(archived["recent_closed"],0);
        assert_eq!(archived["experiences"][0]["actual"],Value::Null);
    }

    #[test]
    fn recovered_old_fill_is_journaled_but_never_invents_a_shadow_path() {
        let db = Connection::open_in_memory().unwrap();
        let _ = crate::execution::Service::new(&db).unwrap();
        let t = telemetry();
        let key = seed_entry(&db, &t, json!({"deal":77}));
        let history = json!({"entry":0,"position_id":55,"price":100.,"volume":0.1,"time_msc":1000});
        db.execute(
            "INSERT INTO broker_deals VALUES(?1,77,55,'trend',?2)",
            params![key, history.to_string()],
        )
        .unwrap();
        label(&db, &key, 1200);
        let mut student = Student::new(&db).unwrap();
        student.observe(&db, &t, 5000).unwrap();
        let v = view(&db, &key).unwrap();
        assert_eq!(v["total_experiences"], 1);
        assert_eq!(v["closed_broker_trades"], 1);
        assert_eq!(v["usable_shadow_pairs"], 0);
        assert_eq!(v["experiences"][0]["entry"]["fill_price"], 100.);
        assert_eq!(
            v["experiences"][0]["shadow"]["exclusion_reason"],
            "history_recovery"
        );
        assert_eq!(v["studies"][0]["study"]["reviewed_losses"], 1);
        let mut restarted = Student::new(&db).unwrap();
        restarted.observe(&db, &t, 6000).unwrap();
        assert_eq!(
            view(&db, &key).unwrap()["studies"][0]["study"]["reviewed_losses"],
            1
        );
    }
    #[test]
    fn fast_quotes_before_position_history_exclude_post_close_extrema() {
        let db = Connection::open_in_memory().unwrap();
        let _ = crate::execution::Service::new(&db).unwrap();
        let mut t = telemetry();
        let key = seed_entry(
            &db,
            &t,
            json!({"deal":77,"price":100.,"volume":0.1,"acknowledged_ms":1000}),
        );
        let mut student = Student::new(&db).unwrap();
        student.observe(&db, &t, 1000).unwrap();
        for (ms, bid) in [(1100, 100.2), (1300, 100.6), (1600, 120.)] {
            t.bid = Some(bid);
            t.ask = Some(bid + 0.2);
            t.quote_time_ms = Some(ms);
            student.quote(&t, ms);
        }
        // Capturing quote extrema did not need a position snapshot. Final
        // broker attribution DOES require the immutable Deal->Position link;
        // it may arrive after the quotes and even after the close.
        label(&db, &key, 1400);
        db.execute("INSERT INTO broker_deals VALUES(?1,77,55,'trend',?2)",
            params![key,json!({"entry":0,"position_id":55,"price":100.0,
                "volume":0.1,"time_msc":1000}).to_string()]).unwrap();
        student.observe(&db, &t, 1800).unwrap();
        let v = view(&db, &key).unwrap();
        let e = &v["experiences"][0];
        assert!((e["observed_mfe_price"].as_f64().unwrap() - 0.6).abs() < 1e-9);
        assert_eq!(e["position_path_usable"], true);
        assert_eq!(e["position_samples"], 2);
    }
    #[test]
    fn one_quote_outcome_is_not_counted_as_an_admitted_comparison() {
        let db = Connection::open_in_memory().unwrap();
        let _ = crate::execution::Service::new(&db).unwrap();
        let mut t = telemetry();
        let key = seed_entry(
            &db,
            &t,
            json!({"deal":77,"price":100.,"volume":0.1,"acknowledged_ms":1000}),
        );
        let mut student = Student::new(&db).unwrap();
        student.observe(&db, &t, 1000).unwrap();
        t.bid = Some(104.);
        t.ask = Some(104.2);
        t.quote_time_ms = Some(1500);
        student.quote(&t, 1500);
        label(&db, &key, 1600);
        student.observe(&db, &t, 1700).unwrap();
        assert_eq!(view(&db, &key).unwrap()["usable_shadow_pairs"], 0);
        assert_eq!(
            view(&db, &key).unwrap()["experiences"][0]["admitted"],
            false
        );
    }
    #[test]
    fn learned_protective_stop_falls_back_when_inside_executable_spread() {
        let mut t=telemetry(); t.ask=Some(100.6);
        let b=BrainConfig { id:"trend".into(),..Default::default() }; let c=Config::default();
        let mut s=signal(); adapt(&mut s,&t,&c,&b,&BTreeMap::new());
        let key=s.coordination.unwrap()["student"]["source"].as_str().unwrap().to_owned();
        let policies=BTreeMap::from([(key,ExitStudy { active:ExitProfile::Protective,..ExitStudy::expanded() })]);
        let mut s=signal(); adapt(&mut s,&t,&c,&b,&policies);
        assert_eq!(s.stop_distance,1.); assert!(s.ready);
        assert_eq!(s.coordination.unwrap()["student"]["fallback_reason"],"stop_inside_spread");
    }

    #[test]
    fn g6j_student_never_credits_future_or_changed_broker_admissions(){
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let t=telemetry();
        let key=seed_entry(&db,&t,json!({"deal":77}));
        db.execute("INSERT INTO broker_deals VALUES(?1,77,55,'trend',?2)",
            params![&key,json!({"entry":0,"position_id":55,"price":100.,
                "volume":0.1,"time_msc":1000}).to_string()]).unwrap();
        label(&db,&key,1200);
        db.execute("UPDATE coordinator_broker_admissions
            SET admitted_ms=6000 WHERE position_id=55",[]).unwrap();
        let mut student=Student::new(&db).unwrap();
        student.observe(&db,&t,5000).unwrap();
        let v=view(&db,&key).unwrap();
        assert_eq!(v["closed_broker_trades"],0,
            "An MT5 close older than 5000 is not sufficient when Core first admitted at 6000");
        assert!(v["experiences"][0]["actual"].is_null());
        student.observe(&db,&t,6000).unwrap();
        let v=view(&db,&key).unwrap();
        assert_eq!(v["closed_broker_trades"],1);
        assert_eq!(v["studies"][0]["study"]["reviewed_losses"],1);
        // Changing the booked Broker payoff breaks the original SHA.
        db.execute("UPDATE coordinator_broker_samples
            SET payload=json_set(payload,'$.net',99)
            WHERE position_id=55",[]).unwrap();
        assert_eq!(view(&db,&key).unwrap()["closed_broker_trades"],0);
        student.observe(&db,&t,6500).unwrap();
        assert_eq!(view(&db,&key).unwrap()["closed_broker_trades"],0);
        let mut restarted=Student::new(&db).unwrap();
        restarted.observe(&db,&t,6600).unwrap();
        assert_eq!(view(&db,&key).unwrap()["closed_broker_trades"],0);
    }

    #[test]
    fn g6l_signed_aggregate_without_original_trade_events_stays_quarantined(){
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let t=telemetry();
        let key=crate::coordinator_learning::context(&t);
        let mut legacy=ExitStudy::expanded();
        legacy.active=ExitProfile::Protective;
        legacy.promotions=500;
        legacy.total_pairs=600;
        let raw=serde_json::to_string(&legacy).unwrap();
        db.execute("INSERT INTO student_exit_studies VALUES('unsigned-history',?1,?2)",
            params![&key,&raw]).unwrap();
        db.execute("INSERT INTO student_study_admissions VALUES(
            'unsigned-history',?1,?2,10,'g6k-student-admitted-study-v1')",
            params![&key,format!("{:x}",Sha256::digest(raw.as_bytes()))]).unwrap();
        assert_eq!(view(&db,&key).unwrap()["usable_shadow_pairs"],0,
            "A valid aggregate SHA alone is not proof of individual MT5 trades");
        let mut student=Student::new(&db).unwrap();
        student.observe(&db,&t,5000).unwrap();
        assert!(student.policies.is_empty());
        assert_eq!(view(&db,&key).unwrap()["studies"][0]["provenance"],
            "legacy_or_tampered_unverified");
    }

    #[test]
    fn g6l_loss_event_journal_matches_broker_and_rejects_corrupt_input(){
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let t=telemetry();
        let ctx=seed_entry(&db,&t,json!({"deal":77}));
        db.execute("INSERT INTO broker_deals VALUES(?1,77,55,'trend',?2)",
            params![&ctx,json!({"entry":0,"position_id":55,"price":100.,
                "volume":0.1,"time_msc":1000}).to_string()]).unwrap();
        label(&db,&ctx,1200);
        let mut student=Student::new(&db).unwrap();
        student.observe(&db,&t,5000).unwrap();
        let v=view(&db,&ctx).unwrap();
        assert_eq!(v["studies"][0]["study"]["reviewed_losses"],1);
        assert_eq!(v["studies"][0]["provenance"],"locally_replayed_g6l");
        let events:u64=db.query_row("SELECT COUNT(*) FROM student_study_events
            WHERE context=?1 AND kind='loss'",[&ctx],|r|r.get(0)).unwrap();
        assert_eq!(events,1);
        // Corrupt even one input while leaving aggregate and SHA untouched:
        // replay MUST detect the forgery and remove policy from future trading.
        db.execute("UPDATE student_study_events
            SET payload=json_set(payload,'$.patterns',json('[]'))
            WHERE context=?1",[&ctx]).unwrap();
        let v=view(&db,&ctx).unwrap();
        assert_eq!(v["usable_shadow_pairs"],0);
        assert!(v["studies"][0]["study"].is_null());
        student.observe(&db,&t,5500).unwrap();
        assert!(student.policies.is_empty(),
            "An already-running Student must immediately revoke learned exit geometry");
        let mut restarted=Student::new(&db).unwrap();
        restarted.observe(&db,&t,6000).unwrap();
        assert!(restarted.policies.is_empty());
    }

    #[test]
    fn g6k_legacy_student_exit_aggregate_does_not_activate_trading_exit_policy(){
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let t=telemetry();
        let ctx=crate::coordinator_learning::context(&t);
        let mut prior=ExitStudy::expanded();
        prior.active=ExitProfile::Protective;
        prior.promotions=99;
        prior.total_pairs=5000;
        db.execute("INSERT INTO student_exit_studies(source,context,payload)
            VALUES('unverifiable-legacy',?1,?2)",
            params![ctx,serde_json::to_string(&prior).unwrap()]).unwrap();
        let mut s=Student::new(&db).unwrap();
        s.observe(&db,&t,5000).unwrap();
        assert!(!s.policies.contains_key("unverifiable-legacy"),
            "Prior unsigned aggregates cannot re-enable learned exit geometries");
        let v=view(&db,&ctx).unwrap();
        assert_eq!(v["usable_shadow_pairs"],0);
        assert_eq!(v["studies"][0]["provenance"],"legacy_or_tampered_unverified");
        assert!(v["studies"][0]["study"].is_null(),
            "Old promotion counters must not look like proven training.");
        assert_eq!(db.query_row("SELECT COUNT(*) FROM student_exit_studies",[],|r|r.get::<_,u64>(0)).unwrap(),1,
            "Raw study is preserved, not destroyed");
    }

    #[test]
    fn g6k_student_study_hash_tracks_valid_broker_learning_and_rejects_changes(){
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let t=telemetry();
        let ctx=seed_entry(&db,&t,json!({"deal":77}));
        db.execute("INSERT INTO broker_deals VALUES(?1,77,55,'trend',?2)",
            params![&ctx,json!({"entry":0,"position_id":55,"price":100.,
                "volume":0.1,"time_msc":1000}).to_string()]).unwrap();
        label(&db,&ctx,1200);
        let mut student=Student::new(&db).unwrap();
        student.observe(&db,&t,5000).unwrap();
        let v=view(&db,&ctx).unwrap();
        assert_eq!(v["studies"][0]["provenance"],"locally_replayed_g6l");
        assert_eq!(v["studies"][0]["study"]["reviewed_losses"],1);
        // This study has no paired quote experiment: do not forge one.
        assert_eq!(v["usable_shadow_pairs"],0);
        db.execute("UPDATE student_exit_studies
            SET payload=json_set(payload,'$.promotions',999)",[]).unwrap();
        let v=view(&db,&ctx).unwrap();
        assert_eq!(v["usable_shadow_pairs"],0);
        assert_eq!(v["studies"][0]["provenance"],"legacy_or_tampered_unverified");
        assert!(v["studies"][0]["study"].is_null());
        let mut restarted=Student::new(&db).unwrap();
        restarted.observe(&db,&t,6000).unwrap();
        assert!(restarted.policies.is_empty(),
            "Restart must not apply mutated preloaded student policy");
    }

}
