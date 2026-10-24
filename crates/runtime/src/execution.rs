//! Execution semantics wrapper: selected-mode coordinator exclusively owns MT5 entries; advisors are paper-only.
//! Portfolio exposure v1 is a Core-side fail-closed entry gate across all BrainTrade identities.
//! The previous 0.6 engine is preserved in execution_legacy.rs and still owns durable broker intents.
#[path = "execution_legacy.rs"]
mod legacy;
#[path = "execution_pulse.rs"]
mod pulse;

use anyhow::{Context, Result};
use braintrade_domain::{
    execution::{self, BrainConfig},
    studio::FrameBar,
    Config, Status, Telemetry,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PortfolioLimits {
    /// Master switch. Defaults off so upgrading from 0.6 does not silently change execution.
    enabled: bool,
    /// Maximum combined open BrainTrade positions plus unresolved open intents.
    max_positions: u32,
    /// Maximum combined owned lots plus unresolved open-intent lots.
    max_lots: f64,
    /// Maximum estimated stop risk as a percentage of current equity.
    /// A positive value fails closed when any contributing position/intent risk is unknown.
    max_risk_percent: f64,
}
impl Default for PortfolioLimits {
    fn default() -> Self {
        Self {
            enabled: false,
            max_positions: 3,
            max_lots: 0.30,
            max_risk_percent: 2.0,
        }
    }
}
impl PortfolioLimits {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            (1..=100).contains(&self.max_positions),
            "Portfolio max positions must be 1..100"
        );
        anyhow::ensure!(
            self.max_lots.is_finite() && (0.001..=10_000.0).contains(&self.max_lots),
            "Portfolio max lots must be 0.001..10000"
        );
        anyhow::ensure!(
            self.max_risk_percent.is_finite()
                && (0.01..=100.0).contains(&self.max_risk_percent),
            "Portfolio max risk must be 0.01..100 percent"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct Exposure {
    open_positions: u32,
    open_intents: u32,
    lots: f64,
    estimated_risk: f64,
    risk_known: bool,
}
impl Exposure {
    fn total_slots(&self) -> u32 {
        self.open_positions.saturating_add(self.open_intents)
    }
    fn json(&self, limits: &PortfolioLimits, equity: f64) -> Value {
        let risk_percent = if self.risk_known && equity.is_finite() && equity > 0.0 {
            Some(self.estimated_risk / equity * 100.0)
        } else {
            None
        };
        json!({
            "enabled":limits.enabled,
            "limits":{
                "max_positions":limits.max_positions,
                "max_lots":limits.max_lots,
                "max_risk_percent":limits.max_risk_percent
            },
            "open_positions":self.open_positions,
            "unresolved_open_intents":self.open_intents,
            "exposure_slots":self.total_slots(),
            "lots":self.lots,
            "estimated_stop_risk":if self.risk_known { Value::from(self.estimated_risk) } else { Value::Null },
            "estimated_stop_risk_percent_equity":risk_percent,
            "risk_known":self.risk_known,
            "note":"Portfolio limits block new entries only. Existing positions, broker SL/TP, timed exits and reconciliation remain active."
        })
    }
}

fn context(t: &Telemetry) -> String {
    crate::coordinator_learning::context(t)
}

fn owned_position_risk(t: &Telemetry, c: &Config, position: &braintrade_domain::Position) -> Option<f64> {
    if position.volume <= 0.0 || !position.volume.is_finite()
        || !position.sl.is_finite() || position.sl <= 0.0
        || !position.price_open.is_finite() || position.price_open <= 0.0
    {
        return None;
    }
    let tick = t.tick_size.filter(|v| v.is_finite() && *v > 0.0)?;
    let tick_value = t
        .tick_value_loss
        .zip(t.tick_value_profit)
        .map(|(loss, profit)| loss.max(profit))
        .filter(|v| v.is_finite() && *v > 0.0)?;
    // A breakeven/trailing stop past entry locks in some gross profit and no
    // longer has downside price risk relative to entry. It is NOT an unknown
    // stop: count zero price loss plus the estimated round-trip commission.
    // A genuinely missing/invalid stop still fails closed.
    let distance = match position.side.as_str() {
        "BUY" => (position.price_open - position.sl).max(0.0),
        "SELL" => (position.sl - position.price_open).max(0.0),
        _ => return None,
    };
    if !distance.is_finite() {
        return None;
    }
    Some(
        distance / tick * tick_value * position.volume
            + braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot,position.volume).ok()?,
    )
}

fn intent_risk(t: &Telemetry, c: &Config, request: &Value) -> Option<f64> {
    let lots = request["lots"].as_f64().filter(|v| v.is_finite() && *v > 0.0)?;
    let stop = request["signal"]["stop_distance"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)?;
    let tick = t.tick_size.filter(|v| v.is_finite() && *v > 0.0)?;
    let tick_value = t
        .tick_value_loss
        .zip(t.tick_value_profit)
        .map(|(loss, profit)| loss.max(profit))
        .filter(|v| v.is_finite() && *v > 0.0)?;
    Some(stop / tick * tick_value * lots + braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot,lots).ok()?)
}

fn command_values(db: &Connection, key: &str, include_queued: bool) -> Result<Vec<Value>> {
    let states = if include_queued {
        "('queued','dispatched','unknown','placed','filled','partial')"
    } else {
        "('dispatched','unknown','placed','filled','partial')"
    };
    let sql = format!(
        "SELECT json(payload) FROM broker_commands WHERE context=?1 AND kind='open' AND state IN {states} ORDER BY created_ms,id"
    );
    let mut statement = db.prepare(&sql)?;
    let rows = statement
        .query_map([key], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|row| Ok(serde_json::from_str(&row)?))
        .collect()
}

fn exposure(db: &Connection, t: &Telemetry, c: &Config, include_queued: bool) -> Result<Exposure> {
    let key = context(t);
    let mut result = Exposure {
        risk_known: true,
        ..Default::default()
    };
    let owned: Vec<_> = t
        .positions
        .iter()
        .filter(|position| {
            position.symbol == t.symbol && execution::brain_for_magic(position.magic).is_some()
        })
        .collect();
    let visible_magics: BTreeSet<u64> = owned.iter().map(|position| position.magic).collect();
    for position in owned {
        result.open_positions = result.open_positions.saturating_add(1);
        result.lots += position.volume.max(0.0);
        match owned_position_risk(t, c, position) {
            Some(risk) => result.estimated_risk += risk,
            None => result.risk_known = false,
        }
    }
    for request in command_values(db, &key, include_queued)? {
        let magic = request["magic"].as_u64().unwrap_or(0);
        // A filled/partial intent and its now-visible position are one exposure, not two.
        if visible_magics.contains(&magic) {
            continue;
        }
        result.open_intents = result.open_intents.saturating_add(1);
        let lots = request["lots"].as_f64().unwrap_or(0.0);
        if !lots.is_finite() || lots <= 0.0 {
            result.risk_known = false;
        } else {
            result.lots += lots;
        }
        match intent_risk(t, c, &request) {
            Some(risk) => result.estimated_risk += risk,
            None => result.risk_known = false,
        }
    }
    if !result.lots.is_finite() || !result.estimated_risk.is_finite() {
        result.risk_known = false;
    }
    Ok(result)
}

fn portfolio_reason(limits: &PortfolioLimits, next: &Exposure, equity: f64) -> Option<String> {
    if !limits.enabled {
        return None;
    }
    if next.total_slots() > limits.max_positions {
        return Some(format!(
            "Portfolio exposure: {} positions/intents would exceed limit {}",
            next.total_slots(), limits.max_positions
        ));
    }
    if next.lots > limits.max_lots + 1e-9 {
        return Some(format!(
            "Portfolio exposure: {:.4} lots would exceed limit {:.4}",
            next.lots, limits.max_lots
        ));
    }
    if !next.risk_known || !equity.is_finite() || equity <= 0.0 {
        return Some("Portfolio exposure: stop risk is unknown; new entry blocked fail-closed".into());
    }
    let cap = equity * limits.max_risk_percent / 100.0;
    if next.estimated_risk > cap + 1e-8 {
        return Some(format!(
            "Portfolio exposure: estimated stop risk {:.2} exceeds {:.2}% equity cap {:.2}",
            next.estimated_risk, limits.max_risk_percent, cap
        ));
    }
    None
}

fn enforce_portfolio(
    db: &Connection,
    t: &Telemetry,
    c: &Config,
    limits: &PortfolioLimits,
) -> Result<Exposure> {
    if !limits.enabled {
        return exposure(db, t, c, true);
    }
    let key = context(t);
    // Existing broker exposure and already-dispatched intents form the immutable base.
    let mut current = exposure(db, t, c, false)?;
    let mut statement = db.prepare(
        "SELECT id,payload FROM broker_commands WHERE context=?1 AND kind='open' AND state='queued' ORDER BY created_ms,id",
    )?;
    let rows = statement
        .query_map([&key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (id, raw) in rows {
        let request: Value = serde_json::from_str(&raw)?;
        let mut next = current.clone();
        next.open_intents = next.open_intents.saturating_add(1);
        let lots = request["lots"].as_f64().unwrap_or(0.0);
        if lots.is_finite() && lots > 0.0 {
            next.lots += lots;
        } else {
            next.risk_known = false;
        }
        match intent_risk(t, c, &request) {
            Some(risk) => next.estimated_risk += risk,
            None => next.risk_known = false,
        }
        if let Some(reason) = portfolio_reason(limits, &next, t.equity) {
            let result = json!({
                "id":id,
                "state":"cancelled",
                "reason":reason,
                "portfolio":current.json(limits,t.equity)
            });
            db.execute(
                "UPDATE broker_commands SET state='cancelled',result=?2 WHERE id=?1 AND state='queued'",
                params![id, result.to_string()],
            )?;
        } else {
            current = next;
        }
    }
    Ok(current)
}

pub struct Service {
    inner: legacy::Service,
    learner: crate::coordinator_learning::Learner,
    student: crate::student_learning::Student,
    sniper_opportunities: crate::sniper_opportunities::Recorder,
    sniper_tick_audit: crate::sniper_tick_audit::Audit,
    sniper_frozen_tick: crate::sniper_frozen_tick::Study,
    portfolio: PortfolioLimits,
    portfolio_state: Value,
    supervisor: crate::enterprise_supervisor::Settings,
    supervisor_policy_valid: bool,
    quality_circuit: crate::broker_quality_circuit::Snapshot,
    fast: Option<pulse::FastContext>,
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        crate::frozen_shadow::init(db)?;
        crate::frozen_coordinator::init(db)?;
        crate::coordinator_paper::init(db)?;
        crate::matched_forward::init(db)?;
        crate::matched_trade::init(db)?;
        crate::sniper_opportunities::init(db)?;
        let mut inner = legacy::Service::new(db)?;
        // `system` is the coordinated main brain. It is controlled by global START/STOP,
        // not by the per-brain SOLO switch. Keep it enabled in memory even when an older
        // 0.6 checkpoint persisted it as false.
        for brain in &mut inner.brains {
            brain.enabled = brain.id == "system";
        }
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS portfolio_execution_settings(\
             id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);",
        )?;
        let raw: Option<String> = db
            .query_row(
                "SELECT payload FROM portfolio_execution_settings WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let portfolio: PortfolioLimits = raw
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default();
        portfolio.validate()?;
        let portfolio_state = Exposure {
            risk_known: false,
            ..Default::default()
        }
        .json(&portfolio, 0.0);
        let (supervisor,supervisor_policy_valid)=Self::load_supervisor_fail_closed(db);
        Ok(Self {
            inner,
            learner: crate::coordinator_learning::Learner::new(db)?,
            student: crate::student_learning::Student::new(db)?,
            sniper_opportunities: crate::sniper_opportunities::Recorder::default(),
            sniper_tick_audit: crate::sniper_tick_audit::Audit::default(),
            sniper_frozen_tick: crate::sniper_frozen_tick::Study::default(),
            portfolio,
            portfolio_state,
            // Cache policy once; never query/CREATE a SQLite table per Sniper tick.
            // A corrupt policy fails closed for entries without disabling Core.
            supervisor,
            supervisor_policy_valid,
            quality_circuit: crate::broker_quality_circuit::Snapshot::default(),
            fast: None,
        })
    }

    fn load_supervisor_fail_closed(db:&Connection)->(crate::enterprise_supervisor::Settings,bool) {
        match crate::enterprise_supervisor::load(db) {
            Ok(settings)=>(settings,true),
            Err(_)=>{
                // A malformed policy cannot be made safe by merely enabling
                // default checks: a valid market could otherwise pass those
                // defaults, silently allowing new broker orders.
                let mut settings=crate::enterprise_supervisor::Settings::default();
                settings.enforce_entry_gate=true;
                (settings,false)
            }
        }
    }
    fn supervisor_permits_entry(&self, assessed: bool)->bool {
        self.supervisor_policy_valid && (!self.supervisor.enforce_entry_gate || assessed)
    }
    pub fn refresh_supervisor(&mut self,db:&Connection) {
        // Cache coherence is updated synchronously in Core after persisted POST.
        // Invalid stored JSON always keeps a separate hard-block sentinel.
        let (settings,valid)=Self::load_supervisor_fail_closed(db);
        self.supervisor=settings;
        self.supervisor_policy_valid=valid;
    }

    pub fn set_mode(&mut self, mode: braintrade_domain::trading::TradingMode) {
        self.inner.set_mode(mode);
        self.fast = None; // Never execute a new selection using a previous cached config.
        self.sniper_tick_audit=crate::sniper_tick_audit::Audit::default();
        self.sniper_frozen_tick.clear_on_mode_change();
    }

    fn save_portfolio(db: &Connection, candidate: &PortfolioLimits) -> Result<()> {
        db.execute(
            "INSERT OR REPLACE INTO portfolio_execution_settings VALUES(1,?1)",
            [serde_json::to_string(candidate)?],
        )?;
        Ok(())
    }

    pub(crate) fn account_matches(&self, status: &Status) -> bool {
        status.telemetry.as_ref().is_some_and(|t| {
            t.connected
                && t.login == self.inner.settings.login
                && t.server == self.inner.settings.server
                && t.currency == self.inner.settings.currency
                && t.account_trade_mode
                    == Some(if self.inner.settings.target == "demo" { 0 } else { 2 })
        })
    }

    /// Global broker execution is armed by START + a fresh bound account.
    /// Secondary-brain SOLO switches are optional overrides and are not the master arm.
    pub fn enabled(&self, status: &Status) -> bool {
        status.requested_run && status.gateway_fresh && self.account_matches(status)
    }

    /// Real-money START is independently acknowledged every time. Merely
    /// selecting Live, restoring prior settings or pressing generic START
    /// must not enable execution on a real broker account.
    pub fn is_live_target(&self) -> bool {
        self.inner.settings.target == "live"
    }

    pub fn validate_start(&self, status: &Status, body: &[u8]) -> Result<()> {
        if self.inner.settings.target != "live" {
            return Ok(());
        }
        let request: Value = serde_json::from_slice(body)
            .context("Real START requires an explicit confirmation payload")?;
        let login = self.inner.settings.login;
        anyhow::ensure!(login > 0 && request["confirm_live_login"].as_str()
            == Some(login.to_string().as_str()),
            "تأكيد START الحقيقي مطلوب: اكتب رقم حساب REAL المربوط في خانة بدء التداول ثم اضغط START"
        );
        anyhow::ensure!(
            status.gateway_fresh && self.account_matches(status)
                && status.telemetry.as_ref().is_some_and(|t|
                    !t.demo && t.account_trade_mode==Some(2)
                    && t.trade_allowed && t.trade_expert),
            "تعذر تشغيل LIVE: يجب مطابقة حساب MT5 الحقيقي وتحديث الاتصال والسماح بالتداول الآلي"
        );
        anyhow::ensure!(self.portfolio.enabled,
            "تشغيل REAL يحتاج تفعيل حماية المحفظة أولاً (عدد الصفقات، اللوت الكلي، وخطر الوقف)"
        );
        Ok(())
    }

    pub fn pending(&self, db: &Connection) -> Result<bool> {
        self.inner.pending(db)
    }

    pub fn cancel_entries(&self, db: &Connection) -> Result<()> {
        self.inner.cancel_entries(db)
    }

    // Refresh only on a reconciled snapshot or persisted broker result;
    // Sniper tick hot path reads the cached, account-scoped state.
    fn refresh_quality_circuit(&mut self,db:&Connection,t:&Telemetry,now:u64) {
        let key=crate::broker_quality_circuit::context(t);
        self.quality_circuit=if self.inner.matches_bound_telemetry(t) {
            crate::broker_quality_circuit::inspect(db,t,now)
                .unwrap_or_else(|_|crate::broker_quality_circuit::Snapshot::unavailable(key,now))
        } else {
            crate::broker_quality_circuit::Snapshot::unavailable(key,now)
        };
    }

    pub fn observe(
        &mut self,
        db: &Connection,
        telemetry: &Telemetry,
        config: &Config,
        run: bool,
        frames: &BTreeMap<String, Vec<FrameBar>>,
        now: u64,
    ) -> Result<()> {
        if let Some(system) = self.inner.brains.iter_mut().find(|brain| brain.id == "system") {
            system.enabled = true;
        }
        self.inner.controls = crate::brain_control::load(db, &context(telemetry), config.trading.mode)?;
        let available = crate::coordinator_learning::available_frames(telemetry, frames, now);
        self.inner.weights = self.learner.observe(
            db,
            telemetry,
            config,
            &self.inner.brains,
            &available,
            run,
            now,
        )?;
        crate::sniper_learning::refresh(&mut self.inner.sniper, db, telemetry, now)?;
        self.student.observe(db,telemetry,now)?;
        self.inner.student_policies=self.student.policies.clone();
        // Legacy engine still owns durable broker intents/reconciliation. With `system`
        // forced on, it represents coordinated execution; every other `enabled` flag means SOLO.
        // V20 institutional admission: never prevent already-open position exits
        // or broker reconciliation. The gate is explicit opt-in, persisted in
        // SQLite, and cannot transfer execution ownership to another brain.
        // A damaged supervisor settings row must never suspend broker exit
        // management. Fail closed for fresh opens while reconciliation proceeds.
        // This check reads the durable Core journal only on reconciled snapshots.
        self.refresh_quality_circuit(db,telemetry,now);
        let quality_allows=self.quality_circuit.entry_allowed(telemetry,self.is_live_target());
        let entries_allowed=if run && quality_allows && self.supervisor_policy_valid {
            if !self.supervisor.enforce_entry_gate {true}
            else {
                let candle_frames: BTreeMap<String,Vec<braintrade_domain::Candle>>=frames.iter().map(|(tf,bars)|
                    (tf.clone(),bars.iter().map(|bar|bar.candle.clone()).collect())).collect();
                self.supervisor_permits_entry(
                    crate::enterprise_supervisor::assess(&self.supervisor,Some(telemetry),config,&candle_frames,now).entry_allowed)
            }
        } else {false};
        self.sniper_tick_audit.clear_on_identity_change(telemetry);
        self.inner
            .observe(db, telemetry, config, entries_allowed, frames, now)?;
        // Immutable Demo coordinator policy is a separate prospective study.
        // An audit failure cannot interfere with LIVE broker execution or exits.
        if let Some(active_signal)=self.inner.last_system_signal(){
            if let Err(error)=crate::frozen_coordinator::observe(
                db,telemetry,config,&active_signal,&available,now){
                let _=db.execute(
                    "INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,?2,?3)",
                    params![now,"frozen_coordinator_observation_error",
                        json!({"context":context(telemetry),
                            "error":error.to_string().chars().take(240).collect::<String>()}).to_string()]);
            }
        }
        // Paired coordinator Paper trades observe the same quote but NEVER
        // call the broker. Their errors are isolated from order management.
        if self.inner.matches_bound_telemetry(telemetry) {
          if let Err(error)=crate::coordinator_paper::observe(
            db,telemetry,config,now){
            let _=db.execute(
                "INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,?2,?3)",
                params![now,"coordinator_paper_observation_error",
                    json!({"context":context(telemetry),
                        "error":error.to_string().chars().take(240).collect::<String>()}).to_string()]);
          }
        }
        self.student.capture_entries(self.inner.take_entries(), now);
        if run {crate::student_learning::log_opportunities(db,telemetry,&self.inner.learning_decisions(),now)?;}
        // Apply one global entry budget after all brains have proposed their queued opens and
        // before the gateway can retrieve them. Exits and reconciliation states are untouched.
        let current = enforce_portfolio(db, telemetry, config, &self.portfolio)?;
        self.portfolio_state = current.json(&self.portfolio, telemetry.equity);
        // Diagnostic sampling uses normal reconciled snapshots only, after
        // broker intents and portfolio decisions. Never add this to Fast Pulse.
        if self.inner.matches_bound_telemetry(telemetry) {
            let decisions = self.inner.learning_decisions();
            let system = decisions.iter().find(|(id,_)| id == "system").map(|(_,d)| d);
            if let Err(error) = self.sniper_opportunities.observe(db,telemetry,config,run,system,now) {
                let _ = db.execute(
                    "INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,?2,?3)",
                    params![now,"sniper_opportunity_observation_error",
                        json!({"context":context(telemetry),
                            "error":error.to_string().chars().take(240).collect::<String>()}).to_string()]);
            }
        }
        // Cache only data already accepted by the regular reconciled snapshot path. Fast
        // pulses may overlay a newer quote but can never replace account/position state.
        self.fast = Some(pulse::FastContext::new(telemetry, config, frames));
        Ok(())
    }

    fn enrich_roles(mut value: Value, db: &Connection, status: &Status) -> Result<Value> {
        let key = status
            .telemetry
            .as_ref()
            .map(crate::coordinator_learning::context)
            .unwrap_or_default();

        if let Some(brains) = value.get_mut("brains").and_then(Value::as_array_mut) {
            for brain in brains {
                let id = brain["config"]["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let solo = false; // all non-system brains are paper/shadow only
                let in_flight: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain=?2 AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",
                    params![key, id],
                    |row| row.get(0),
                )?;
                let open = status.telemetry.as_ref().is_some_and(|t| {
                    t.positions.iter().any(|position| {
                        Some(position.magic) == execution::magic(&id) && position.symbol == t.symbol
                    })
                });
                let returning = id != "system" && !solo && (in_flight || open);
                brain["return_pending"] = json!(returning);
                brain["role"] = json!(if id == "system" {
                    "coordinator"
                } else if solo {
                    "solo"
                } else if returning {
                    "returning"
                } else {
                    "advisor"
                });
                brain["solo_enabled"] = json!(solo);
                brain["participates_in_coordinator"] = json!(id != "system" && !solo && !returning);
                if id == "system" {
                    brain["config"]["enabled"] = json!(true);
                }
            }
        }
        value["learning_review"] = json!(crate::coordinator_learning::broker_review(db,&key)?);
        value["execution_model"] = json!("adaptive-member-coordinator-v1");
        value["note"] = json!("العقل الرئيسي يعمل مع START. العقول الثانوية تشارك كمستشارين افتراضياً؛ تفعيل SOLO يفصل العقل المختار فقط، وإطفاؤه يعيده بعد تسوية صفقة SOLO المفتوحة. الصفقات المغلقة على الديمو تحدّث الوزن من أول نتيجة، بعد التكاليف؛ تقييم 12 شمعة يبقى دليلاً افتراضياً منفصلاً. START/STOP منفصلان عن زر العضوية.");
        Ok(value)
    }

    /// Frozen Expert Demo *decision-policy* only; NOT a broker or risk-gate twin.
    pub fn coordinator_twin_route(&mut self,method:&str,body:&[u8],
        db:&Connection,status:&Status,config:&Config,now:u64)->Result<Value>{
        let current=status.telemetry.as_ref()
            .filter(|t|status.gateway_fresh && t.connected && self.account_matches(status));
        if method=="GET"{
            let Some(t)=current else{return Ok(json!({"status":"unbound","broker_effect":false}))};
            if config.trading.mode==braintrade_domain::trading::TradingMode::Sniper{
                return Ok(json!({"status":"tick_native_research_not_implemented",
                    "mode":"sniper","read_only":true,"no_broker_orders":true,
                    "reason":"M1 frozen coordinator cannot represent Sniper Fast Pulse ticks"}));
            }
            return crate::frozen_coordinator::report(db,&context(t),config.trading.mode);
        }
        anyhow::ensure!(method=="POST","Unknown frozen-coordinator method");
        anyhow::ensure!(!status.requested_run && !self.pending(db)?,
            "أوقف التداول وسوِّ جميع الأوامر قبل تجميد سياسة المنسق");
        let t=current.context("اربط MT5 Demo بقراءة حديثة أولاً")?;
        anyhow::ensure!(t.demo&&t.account_trade_mode==Some(0)
            &&matches!(config.trading.mode,
                braintrade_domain::trading::TradingMode::Expert|
                braintrade_domain::trading::TradingMode::Scalping|
                braintrade_domain::trading::TradingMode::Swing),
            "البحث المجمّد لـ Expert/Scalping/Swing مسموح في Demo فقط؛ Sniper يحتاج بيانات Tick");
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {action:String,context:String,mode:braintrade_domain::trading::TradingMode}
        let req:Request=serde_json::from_slice(body)?;
        anyhow::ensure!(req.context==context(t) && req.mode==config.trading.mode,
            "الحساب أو الوضع تغيّر؛ أعد تحميل البيانات");
        match req.action.as_str(){
            "freeze"=>{
                let controls=crate::brain_control::load(db,&req.context,req.mode)?;
                crate::frozen_coordinator::freeze(db,t,config,&self.inner.brains,
                    &self.inner.weights,&controls,&self.inner.student_policies,now)
            },
            "stop"=>crate::frozen_coordinator::stop(db,&req.context,req.mode),
            _=>anyhow::bail!("Unknown frozen coordinator action")
        }
    }
    /// Isolated Demo-only frozen advisor trials. Never returns a broker command.
    pub fn frozen_route(
        &mut self,method:&str,body:&[u8],db:&Connection,status:&Status,
        config:&Config,now:u64
    )->Result<Value>{
        let current=status.telemetry.as_ref()
            .filter(|t|status.gateway_fresh && t.connected && self.account_matches(status));
        if method=="GET" {
            let Some(t)=current else {return Ok(json!({"status":"unbound","trials":[]}));};
            let key=context(t);
            let mut trials=Vec::new();
            for b in self.inner.brains.iter().filter(|b|b.id!="system"){
                trials.push(json!({"brain":b.id,
                    "trial":crate::frozen_shadow::report(db,&key,config.trading.mode,&b.id)?}));
            }
            return Ok(json!({"status":"read_only","trials":trials,
                "broker_effect":false,"training_effect":false}));
        }
        anyhow::ensure!(method=="POST","Unknown frozen-shadow method");
        anyhow::ensure!(!status.requested_run && !self.pending(db)?,
            "أوقف النظام وانتظر تسوية أوامر MT5 قبل حفظ نسخة مجمدة");
        let t=current.context("حساب MT5 المرتبط غير متصل أو بياناته قديمة")?;
        anyhow::ensure!(t.demo && t.account_trade_mode==Some(0),
            "تجميد Shadow متاح لحساب MT5 Demo فقط");
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request{action:String,context:String,mode:braintrade_domain::trading::TradingMode,brain:String}
        let req:Request=serde_json::from_slice(body)?;
        anyhow::ensure!(req.context==context(t) && req.mode==config.trading.mode
            && self.inner.active_mode==Some(req.mode),
            "حساب أو رمز أو وضع التداول تغير؛ حدّث الصفحة قبل الحفظ");
        let b=self.inner.brains.iter().find(|b|b.id==req.brain)
            .context("العقل المحدد غير موجود")?;
        match req.action.as_str(){
            "freeze"=>{
                let g=crate::coordinator_learning::genome(db,b,t)?;
                crate::frozen_shadow::freeze(db,&req.context,req.mode,b,config,g.as_ref(),now)
            }
            "stop"=>crate::frozen_shadow::stop(db,&req.context,req.mode,&req.brain),
            _=>anyhow::bail!("Unknown frozen-shadow action")
        }
    }
    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        db: &Connection,
        status: &Status,
        now: u64,
    ) -> Result<Value> {
        if path=="/v1/execution/frozen-sniper" {
            let t=status.telemetry.as_ref().filter(|t|
                status.gateway_fresh&&self.account_matches(status)
                &&self.inner.active_mode==Some(braintrade_domain::trading::TradingMode::Sniper)
                &&t.demo&&t.account_trade_mode==Some(0));
            if method=="GET" {
                return Ok(self.sniper_frozen_tick.report(t,now));
            }
            anyhow::ensure!(method=="POST","Unsupported frozen Sniper method");
            anyhow::ensure!(!status.requested_run && !self.pending(db)?,
                "STOP and reconcile broker intents before freezing Sniper");
            let t=t.context("Fresh bound MT5 Demo Sniper account required")?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request{action:String,context:String}
            let request:Request=serde_json::from_slice(body)?;
            anyhow::ensure!(request.context==context(t),
                "Sniper frozen research account/session drift");
            match request.action.as_str(){
                "freeze"=>{
                    let fast=self.fast.as_ref()
                        .filter(|f|f.config.trading.mode==braintrade_domain::trading::TradingMode::Sniper
                            &&f.telemetry.login==t.login &&f.telemetry.server==t.server
                            &&f.telemetry.symbol==t.symbol&&f.telemetry.currency==t.currency)
                        .context("Sniper reconciled quote cache not ready")?;
                    self.sniper_frozen_tick.freeze(t,&fast.config,&self.inner.sniper,now)
                },
                "stop"=>self.sniper_frozen_tick.stop(t),
                _=>anyhow::bail!("Unknown Sniper frozen study action")
            }
        }
        if method=="POST" && path=="/v1/execution/brain-control" {
            anyhow::ensure!(!status.requested_run && !self.pending(db)?, "أوقف النظام وانتظر تسوية الأوامر قبل تعديل مشاركة العقول");
            anyhow::ensure!(status.gateway_fresh && self.account_matches(status), "اربط الحساب الحالي وانتظر اتصالاً حديثاً");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request {context:String, brain:String, mode:braintrade_domain::trading::TradingMode, control:crate::brain_control::Control}
            let request:Request=serde_json::from_slice(body)?;
            anyhow::ensure!(self.inner.active_mode==Some(request.mode), "الوضع تغيّر؛ حدّث الصفحة قبل الحفظ");
            let key=context(status.telemetry.as_ref().unwrap());
            anyhow::ensure!(request.context==key, "الحساب أو الرمز تغيّر؛ حدّث الصفحة قبل الحفظ");
            crate::brain_control::save(db,&key,request.mode,&request.brain,request.control)?;
            self.inner.controls.insert(request.brain,request.control);
            self.fast=None;
            self.inner.set_mode(request.mode);
            return Ok(json!({"ok":true}));
        }
        // P0-B: broker-journal diagnostics, not an order or an admission gate.
        // Never return any history for a stale, unbound or mismatched account.
        if method=="GET" && path=="/v1/execution/quality" {
            let t=status.telemetry.as_ref().filter(|t|
                status.gateway_fresh && self.account_matches(status)
                && !t.symbol.is_empty() && !t.currency.is_empty());
            let mut quality=crate::execution_quality::report(db,t,now)?;
            if let Some(t)=t {
                let key=crate::broker_quality_circuit::context(t);
                let state=crate::broker_quality_circuit::inspect(db,t,now)
                    .unwrap_or_else(|_|crate::broker_quality_circuit::Snapshot::unavailable(key,now));
                quality["entry_circuit"]=state.as_json(self.is_live_target());
            }
            return Ok(quality);
        }
        if method=="GET" && path=="/v1/execution/sniper-tick-audit" {
            let scoped=status.telemetry.as_ref().filter(|t|
                status.gateway_fresh && self.account_matches(status)
                && t.connected && t.demo && t.account_trade_mode==Some(0)
                && self.inner.active_mode==Some(braintrade_domain::trading::TradingMode::Sniper)
                && self.fast.as_ref().is_some_and(|f|
                    f.config.trading.mode==braintrade_domain::trading::TradingMode::Sniper
                    && f.telemetry.login==t.login && f.telemetry.server==t.server
                    && f.telemetry.currency==t.currency && f.telemetry.symbol==t.symbol));
            return Ok(self.sniper_tick_audit.report(scoped,now));
        }
        if method=="GET" && path=="/v1/execution/mode-evidence" {
            // No REAL, stale or cross-bound data may be used as evidence
            // for any of the four coordinators' Demo results.
            let t=status.telemetry.as_ref().filter(|t|
                status.gateway_fresh && self.account_matches(status)
                && t.demo && t.account_trade_mode==Some(0));
            return crate::mode_evidence::report(db,t,now);
        }
        if method=="GET" && path=="/v1/execution/brains" {
            let mut value=self.route("GET","/v1/execution",&[],db,status,now)?;
            let key=if status.gateway_fresh && self.account_matches(status) {context(status.telemetry.as_ref().unwrap())}else{String::new()};
            crate::brain_control::enrich(&mut value,db,&key,self.inner.active_mode.unwrap_or_default())?;
            crate::brain_integrity::enrich(&mut value,db,status,self.inner.active_mode.unwrap_or_default())?;
            value["controls_available"]=json!(!key.is_empty());
            return Ok(value);
        }
        if method=="GET" && path=="/v1/execution/student" {
            let key=status.telemetry.as_ref().map(crate::coordinator_learning::context).unwrap_or_default();
            let mut view=crate::student_learning::view(db,&key)?;
            view["demo_connected"]=json!(status.gateway_fresh && status.telemetry.as_ref().is_some_and(|t|t.connected&&t.demo&&t.account_trade_mode==Some(0)));
            return Ok(view);
        }
        // PUT intentionally avoids the generic POST audit write on every market tick.
        // The pulse only changes the cached quote, then runs execution/reconciliation
        // logic and returns gateway work in the same response. Final authorization and
        // the gateway's synchronous durable send journal still happen before order_send.
        if method == "PUT" && path == "/v1/execution/pulse" {
            anyhow::ensure!(
                status.gateway_fresh,
                "A fresh reconciled MT5 snapshot is required before Sniper pulse"
            );
            let started = Instant::now();
            let next = self
                .fast
                .as_ref()
                .context("Sniper pulse cache is not ready")?
                .apply(body, now)?;
            if let Some(system) = self.inner.brains.iter_mut().find(|brain| brain.id == "system") {
                system.enabled = true;
            }
            self.student.quote(&next.telemetry, now);
            // Fast Pulse MUST obey the exact same optional V20 entry guard.
            // No re-sending an unknown command, no bypass on lower latency.
            // Cached reconciled candles are reused; fresh pulse quote is overlaid.
            let quality_allows=self.quality_circuit.entry_allowed(&next.telemetry,self.is_live_target());
            let enter=if status.requested_run && quality_allows && self.supervisor_policy_valid {
                if !self.supervisor.enforce_entry_gate {true}
                else {self.supervisor_permits_entry(crate::enterprise_supervisor::assess(&self.supervisor,Some(&next.telemetry),
                    &next.config,&BTreeMap::new(),now).entry_allowed)}
            } else {false};
            self.inner.observe(
                db,
                &next.telemetry,
                &next.config,
                enter,
                next.frames.as_ref(),
                now,
            )?;
            // Independent frozen learner observes the actual raw adaptive
            // Sniper decision on this same accepted quote before later
            // execution/portfolio gates. Research never creates a broker intent.
            self.sniper_frozen_tick.observe(&next.telemetry,&next.config,
                self.inner.last_sniper_raw.as_ref(),body,now);
            self.student.capture_entries(self.inner.take_entries(), now);
            let current = enforce_portfolio(db, &next.telemetry, &next.config, &self.portfolio)?;
            self.portfolio_state = current.json(&self.portfolio, next.telemetry.equity);

            let mut pulse_status = status.clone();
            pulse_status.telemetry = Some(next.telemetry.clone());
            pulse_status.last_received_ms = Some(now);
            pulse_status.gateway_fresh = true;
            pulse_status.execution_enabled = self.enabled(&pulse_status);
            self.fast = Some(next);

            let mut work = self.inner.route(
                "GET",
                "/v1/execution/gateway",
                &[],
                db,
                &pulse_status,
                now,
            )?;
            // No SQLite writes on Sniper hot path; this is bounded in-memory
            // accepted-pulse observation only. A failed Core pulse is not counted.
            self.sniper_tick_audit.observe(pulse_status.telemetry.as_ref().unwrap(),body,now);
            work["pulse_fast_path"] = json!(true);
            work["pulse_to_work_us"] = json!(started.elapsed().as_micros() as u64);
            return Ok(work);
        }

        if method == "POST" && path == "/v1/execution/portfolio" {
            anyhow::ensure!(
                !status.requested_run && !self.pending(db)?,
                "أوقف النظام وانتظر تسوية كل الأوامر قبل تغيير حدود المحفظة"
            );
            let candidate: PortfolioLimits = serde_json::from_slice(body)?;
            candidate.validate()?;
            // Persist the proposed policy before replacing the in-memory one;
            // a disk failure must not leave two inconsistent exposure policies.
            Self::save_portfolio(db, &candidate)?;
            self.portfolio = candidate;
            if let Some(t) = status.telemetry.as_ref().filter(|t| t.connected) {
                // Read-only refresh; queued entries are not created while STOPPED and exits remain untouched.
                let config = status
                    .analysis
                    .current
                    .then_some(())
                    .map(|_| ())
                    .unwrap_or(());
                let _ = config; // Status intentionally does not own Config; observe refreshes exact risk state.
                self.portfolio_state["enabled"] = json!(self.portfolio.enabled);
                self.portfolio_state["limits"] = json!({
                    "max_positions":self.portfolio.max_positions,
                    "max_lots":self.portfolio.max_lots,
                    "max_risk_percent":self.portfolio.max_risk_percent
                });
                self.portfolio_state["account"] = json!({"login":t.login,"server":t.server});
            } else {
                self.portfolio_state["enabled"] = json!(self.portfolio.enabled);
                self.portfolio_state["limits"] = json!({
                    "max_positions":self.portfolio.max_positions,
                    "max_lots":self.portfolio.max_lots,
                    "max_risk_percent":self.portfolio.max_risk_percent
                });
            }
            return Ok(json!({"ok":true,"portfolio":self.portfolio_state}));
        }

        // A brain role change must never arm global START implicitly. START/STOP is a
        // separate master control. The `enabled` bit on a non-system brain means SOLO.
        if path == "/v1/execution/trades" && matches!(method, "GET" | "POST") {
            let (offset,limit) = if method == "POST" {
                let input: Value = serde_json::from_slice(body)?;
                (input["offset"].as_u64().unwrap_or(0), input["limit"].as_u64().unwrap_or(50))
            } else {(0,50)};
            anyhow::ensure!(offset<=1_000_000 && (1..=100).contains(&limit), "Invalid journal page");
            let t = status.telemetry.as_ref().filter(|t|t.connected
                && t.login == self.inner.settings.login
                && t.server == self.inner.settings.server
                && t.currency == self.inner.settings.currency);
            let mut key=t.map(|t|serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)))
                .transpose()?.unwrap_or_default();
            if key.is_empty() && self.inner.settings.login>0 {
                // Offline history remains readable: only use a persisted context
                // matching the exact previously-bound broker identity, never
                // another account's private journal.
                key=db.query_row(
                    "SELECT context FROM broker_deals WHERE json_extract(context,'$[0]')=?1                      AND CAST(json_extract(context,'$[1]') AS INTEGER)=?2                      AND json_extract(context,'$[3]')=?3 ORDER BY rowid DESC LIMIT 1",
                    params![self.inner.settings.server,self.inner.settings.login,self.inner.settings.currency],
                    |row|row.get(0),
                ).optional()?.unwrap_or_default();
            }
            return crate::trade_journal::list(db, &key, t, offset as u32, limit as u32);
        }
        if method == "POST" && path == "/v1/execution/brain" {
            let mut config: BrainConfig = serde_json::from_slice(body)?;
            if config.id == "system" {
                config.enabled = true;
                let encoded = serde_json::to_vec(&config)?;
                let mut result = self
                    .inner
                    .route(method, path, &encoded, db, status, now)?;
                result["start"] = json!(false);
                result["role"] = json!("coordinator");
                result["note"] = json!("العقل الرئيسي يتبع START/STOP ولا يحتاج زر تداول منفرد.");
                return Ok(result);
            }
            anyhow::ensure!(!config.enabled,
                "التداول المنفرد ملغى: الوضع المختار وحده ينفذ، وهذه العقول تراقب وتتعلم افتراضياً");
            let mut result = self.inner.route(method, path, body, db, status, now)?;
            result["start"] = json!(false);
            result["role"] = json!("shadow");
            result["solo_enabled"] = json!(false);
            result["note"] = json!("العقل مراقب/تجريبي فقط ولا يرسل أوامر MT5؛ النظام الرئيسي للوضع المختار وحده ينفذ");
            return Ok(result);
        }

        let value = self.inner.route(method, path, body, db, status, now)?;
        // Final authorization is the last Core gate before Gateway calls MT5.
        // Read the committed journal independently here; never trust only a
        // previous tick's cached snapshot. Existing close/reconciliation intents
        // must not be denied by the quality circuit.
        if method=="POST" && path=="/v1/execution/authorize"
            && value["authorized"]==true {
            let request:Value=serde_json::from_slice(body)?;
            let id=request["id"].as_str().context("Authorization id required")?;
            let (kind,cmd_context):(String,String)=db.query_row(
                "SELECT kind,context FROM broker_commands WHERE id=?1",
                [id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if kind=="open" && self.is_live_target() {
                let authorized_quality=status.telemetry.as_ref()
                    .filter(|t|status.gateway_fresh&&self.account_matches(status)
                        && crate::broker_quality_circuit::context(t)==cmd_context)
                    .and_then(|t|crate::broker_quality_circuit::inspect(db,t,now).ok()
                        .map(|quality|quality.entry_allowed(t,true)))
                    .unwrap_or(false);
                if !authorized_quality {
                    return Ok(json!({"authorized":false,"reason":"core_broker_quality_entry_circuit"}));
                }
            }
        }
        if method == "POST" && path == "/v1/execution/result" {
            if let Some(t) = status.telemetry.as_ref().filter(|t| status.gateway_fresh && self.account_matches(status)) {
                // Result was persisted before refreshing the cache. Next Sniper
                // Tick cannot race back into a just-breached quality circuit.
                self.refresh_quality_circuit(db,t,now);
                // Register acknowledged fill before the next quote pulse.
                self.student.observe(db, t, now)?;
            } else {
                self.quality_circuit=crate::broker_quality_circuit::Snapshot::default();
            }
        }
        if method == "GET" && path == "/v1/execution" {
            let mut value = Self::enrich_roles(value, db, status)?;
            value["portfolio"] = self.portfolio_state.clone();
            let context=status.telemetry.as_ref().map(|t|
                serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap_or_default()
            ).unwrap_or_default();
            value["shadow_paper"] = crate::shadow_paper::summary(db,&context)?;
            Ok(value)
        } else {
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::Position;

    fn telemetry() -> Telemetry {
        Telemetry {
            connected: true,
            login: 17,
            server: "Demo".into(),
            currency: "USD".into(),
            symbol: "XAUUSD".into(),
            equity: 10_000.0,
            balance: 10_000.0,
            tick_size: Some(0.01),
            tick_value_loss: Some(1.0),
            tick_value_profit: Some(1.0),
            ..Default::default()
        }
    }
    fn setup() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        let _ = Service::new(&db).unwrap();
        db
    }
    fn queue(db: &Connection, t: &Telemetry, id: &str, brain: &str, lots: f64, stop: f64, created: u64) {
        let key = context(t);
        let magic = execution::magic(brain).unwrap();
        let payload = json!({
            "id":id,"context":key,"brain":brain,"kind":"open","login":t.login,
            "server":t.server,"currency":t.currency,"target":"demo","symbol":t.symbol,
            "magic":magic,"position":0,"lots":lots,"created_ms":created,
            "signal":{"side":1,"stop_distance":stop,"target_distance":stop*2.0}
        });
        db.execute(
            "INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload) VALUES(?1,?2,?3,'open',?4,'queued',?5)",
            params![id, context(t), brain, created, payload.to_string()],
        )
        .unwrap();
    }
    fn state(db: &Connection, id: &str) -> String {
        db.query_row(
            "SELECT state FROM broker_commands WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn corrupt_policy_cannot_open_trades_even_if_market_assessment_would_pass() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        assert!(service.supervisor_policy_valid);
        crate::enterprise_supervisor::load(&db).unwrap();
        db.execute("INSERT INTO enterprise_supervisor_config(id,settings) VALUES(1,?1)",
            [r#"{"enforce_entry_gate":"invalid"}"#]).unwrap();
        service.refresh_supervisor(&db);
        assert!(!service.supervisor_policy_valid);
        assert!(service.supervisor.enforce_entry_gate);
        assert!(!service.supervisor_permits_entry(true), "Corrupt policy must never permit entries");
        assert!(!service.supervisor_permits_entry(false));
        db.execute("UPDATE enterprise_supervisor_config SET settings=?1 WHERE id=1",
            [r#"{"enforce_entry_gate":false}"#]).unwrap();
        service.refresh_supervisor(&db);
        assert!(service.supervisor_policy_valid);
        assert!(service.supervisor_permits_entry(true));
    }

    #[test]
    fn disabled_portfolio_preserves_existing_behavior() {
        let db = setup();
        let t = telemetry();
        queue(&db, &t, "a", "trend", 5.0, 10.0, 1);
        let limits = PortfolioLimits::default();
        enforce_portfolio(&db, &t, &Config::default(), &limits).unwrap();
        assert_eq!(state(&db, "a"), "queued");
    }

    #[test]
    fn aggregate_lot_limit_accepts_first_and_cancels_second() {
        let db = setup();
        let t = telemetry();
        queue(&db, &t, "a", "trend", 0.10, 0.05, 1);
        queue(&db, &t, "b", "gann", 0.10, 0.05, 2);
        let limits = PortfolioLimits {
            enabled: true,
            max_positions: 10,
            max_lots: 0.15,
            max_risk_percent: 100.0,
        };
        let view = enforce_portfolio(&db, &t, &Config::default(), &limits).unwrap();
        assert_eq!(state(&db, "a"), "queued");
        assert_eq!(state(&db, "b"), "cancelled");
        assert_eq!(view.open_intents, 1);
        assert!((view.lots - 0.10).abs() < 1e-9);
    }

    #[test]
    fn visible_position_and_filled_intent_are_not_double_counted() {
        let db = setup();
        let mut t = telemetry();
        t.positions.push(Position {
            magic: execution::magic("trend").unwrap(),
            identifier: 55,
            ticket: 55,
            symbol: "XAUUSD".into(),
            side: "BUY".into(),
            volume: 0.10,
            price_open: 2000.0,
            price_current: 2000.1,
            profit: 1.0,
            sl: 1999.9,
            tp: 2000.2,
            ..Default::default()
        });
        queue(&db, &t, "a", "trend", 0.10, 0.10, 1);
        db.execute("UPDATE broker_commands SET state='filled' WHERE id='a'", [])
            .unwrap();
        let view = exposure(&db, &t, &Config::default(), true).unwrap();
        assert_eq!(view.open_positions, 1);
        assert_eq!(view.open_intents, 0);
        assert!((view.lots - 0.10).abs() < 1e-9);
    }

    #[test]
    fn real_start_requires_new_exact_login_and_enabled_portfolio_protection() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        svc.inner.settings.target="live".into();
        svc.inner.settings.login=922244;
        svc.inner.settings.server="Broker-Real".into();
        svc.inner.settings.currency="USD".into();
        let mut status=Status::default();
        status.gateway_fresh=true;
        status.telemetry=Some(Telemetry {
            connected:true, login:922244,server:"Broker-Real".into(),
            currency:"USD".into(),demo:false,account_trade_mode:Some(2),
            trade_allowed:true,trade_expert:true,..Default::default()
        });
        for payload in [json!({}),json!({"confirm_live_login":"92224"}),json!({"confirm_live_login":922244})] {
            assert!(svc.validate_start(&status,payload.to_string().as_bytes()).is_err());
        }
        let valid=json!({"confirm_live_login":"922244"}).to_string();
        assert!(svc.validate_start(&status,valid.as_bytes()).is_err(),"portfolio must be on");
        svc.portfolio.enabled=true;
        assert!(svc.validate_start(&status,valid.as_bytes()).is_ok());
        status.gateway_fresh=false;
        assert!(svc.validate_start(&status,valid.as_bytes()).is_err());
        status.gateway_fresh=true;
        status.telemetry.as_mut().unwrap().trade_expert=false;
        assert!(svc.validate_start(&status,valid.as_bytes()).is_err());
        status.telemetry.as_mut().unwrap().trade_expert=true;
        status.telemetry.as_mut().unwrap().login=999999;
        assert!(svc.validate_start(&status,valid.as_bytes()).is_err());
        status.telemetry.as_mut().unwrap().login=922244;
        status.telemetry.as_mut().unwrap().account_trade_mode=Some(0);
        assert!(svc.validate_start(&status,valid.as_bytes()).is_err());
        svc.inner.settings.target="demo".into();
        assert!(svc.validate_start(&status,b"{}").is_ok(),"demo START keeps original monitoring behavior");
    }

    #[test]
    fn unknown_existing_stop_risk_blocks_new_entry_fail_closed() {
        let db = setup();
        let mut t = telemetry();
        t.positions.push(Position {
            magic: execution::magic("trend").unwrap(),
            identifier: 55,
            ticket: 55,
            symbol: "XAUUSD".into(),
            side: "BUY".into(),
            volume: 0.10,
            price_open: 2000.0,
            price_current: 2000.1,
            profit: 1.0,
            sl: 0.0,
            tp: 2000.2,
            ..Default::default()
        });
        queue(&db, &t, "b", "gann", 0.01, 0.10, 2);
        let limits = PortfolioLimits {
            enabled: true,
            max_positions: 10,
            max_lots: 10.0,
            max_risk_percent: 2.0,
        };
        enforce_portfolio(&db, &t, &Config::default(), &limits).unwrap();
        assert_eq!(state(&db, "b"), "cancelled");
    }

    #[test]
    fn breakeven_and_profit_lock_stops_remain_known_risk_but_missing_sl_fails_closed() {
        let mut t=telemetry();
        let mut buy=Position {
            magic:execution::magic("trend").unwrap(),identifier:50,ticket:50,
            symbol:"XAUUSD".into(),side:"BUY".into(),volume:0.1,
            price_open:2000.0,price_current:2001.0,sl:2000.0,tp:2002.0,
            ..Default::default()
        };
        let c=Config::default();
        let commission=c.risk.commission_per_lot*buy.volume;
        assert!((owned_position_risk(&t,&c,&buy).unwrap()-commission).abs()<1e-8);
        buy.sl=2000.5;
        assert!((owned_position_risk(&t,&c,&buy).unwrap()-commission).abs()<1e-8);
        buy.sl=0.0;
        assert!(owned_position_risk(&t,&c,&buy).is_none());
        buy.sl=f64::NAN;
        assert!(owned_position_risk(&t,&c,&buy).is_none());

        let sell=Position{
            magic:execution::magic("gann").unwrap(),identifier:51,ticket:51,
            symbol:"XAUUSD".into(),side:"SELL".into(),volume:0.1,
            price_open:2000.0,price_current:1999.0,sl:1999.5,tp:1998.0,
            ..Default::default()
        };
        assert!((owned_position_risk(&t,&c,&sell).unwrap()-commission).abs()<1e-8);
        t.positions.push(sell);
        let db=setup();
        queue(&db,&t,"trailing-advisor","trend",0.01,0.05,1000);
        let limits=PortfolioLimits{enabled:true,max_positions:3,max_lots:1.0,max_risk_percent:2.0};
        let exposure=enforce_portfolio(&db,&t,&c,&limits).unwrap();
        assert!(exposure.risk_known);
        assert_eq!(state(&db,"trailing-advisor"),"queued");
    }

    #[test]
    fn portfolio_settings_wait_for_pending_broker_intents_and_do_not_mutate_on_rejection() {
        let db=setup();
        let mut service=Service::new(&db).unwrap();
        let t=telemetry();
        let status=Status::default();
        let updated=PortfolioLimits{enabled:true,max_positions:2,max_lots:0.2,max_risk_percent:1.0};
        let body=serde_json::to_vec(&updated).unwrap();
        queue(&db,&t,"in-flight","trend",0.01,0.05,1000);
        for state_name in ["queued","dispatched","unknown","placed","filled","partial"] {
            db.execute("UPDATE broker_commands SET state=?1 WHERE id='in-flight'",[state_name]).unwrap();
            assert!(service.route("POST","/v1/execution/portfolio",&body,&db,&status,1001).is_err(),"{state_name}");
            assert!(!service.portfolio.enabled);
            let persisted: i64=db.query_row("SELECT COUNT(*) FROM portfolio_execution_settings",[],|r|r.get(0)).unwrap();
            assert_eq!(persisted,0);
        }
        db.execute("UPDATE broker_commands SET state='observed' WHERE id='in-flight'",[]).unwrap();
        assert!(service.route("POST","/v1/execution/portfolio",&body,&db,&status,1002).is_ok());
        assert!(service.portfolio.enabled);
        let restarted=Service::new(&db).unwrap();
        assert!(restarted.portfolio.enabled);
        assert_eq!(restarted.portfolio.max_positions,2);
    }

    #[test]
    fn position_count_includes_unresolved_open_intents() {
        let db = setup();
        let t = telemetry();
        queue(&db, &t, "a", "trend", 0.01, 0.05, 1);
        db.execute("UPDATE broker_commands SET state='unknown' WHERE id='a'", [])
            .unwrap();
        queue(&db, &t, "b", "gann", 0.01, 0.05, 2);
        let limits = PortfolioLimits {
            enabled: true,
            max_positions: 1,
            max_lots: 10.0,
            max_risk_percent: 100.0,
        };
        enforce_portfolio(&db, &t, &Config::default(), &limits).unwrap();
        assert_eq!(state(&db, "a"), "unknown");
        assert_eq!(state(&db, "b"), "cancelled");
    }
    #[test]
    fn p0c_final_authorization_blocks_unresolved_and_three_rejects_but_allows_close() {
        let db=setup();
        let mut service=Service::new(&db).unwrap();
        let mut t=telemetry();
        t.demo=false;t.account_trade_mode=Some(2);
        t.trade_allowed=true;t.trade_expert=true;
        // Synthetic 1970 timestamp requires a matching, complete daily PnL
        // snapshot; otherwise the independent REAL daily breaker correctly
        // denies authorization before the new quality circuit is exercised.
        t.report_day="1970-01-01".into();
        t.today_net=Some(0.);
        service.inner.settings.target="live".into();
        service.inner.settings.login=t.login;
        service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone();
        service.inner.set_mode(braintrade_domain::trading::TradingMode::Sniper);
        let status=Status{requested_run:true,gateway_fresh:true,
            telemetry:Some(t.clone()),..Default::default()};
        let key=context(&t);
        let revision=service.inner.settings.revision;
        let insert=|id:&str,kind:&str,state:&str,created:u64,result:&str|{
            let payload=json!({"id":id,"kind":kind,"brain":"system",
                "trading_mode":"sniper","context":key,
                "revision":revision,
                "created_ms":created,"expires_ms":1_000_000_u64});
            db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result)
                VALUES(?1,?2,'system',?3,?4,?5,?6,?7)",
                params![id,key,kind,created,state,payload.to_string(),result]).unwrap();
        };
        insert("p0c-entry","open","dispatched",100,"{}");
        let authorize=|svc:&mut Service,id:&str,now:u64|->Value{
            svc.route("POST","/v1/execution/authorize",
                json!({"id":id}).to_string().as_bytes(),&db,&status,now).unwrap()
        };
        assert_eq!(authorize(&mut service,"p0c-entry",200)["authorized"],true);
        insert("p0c-unknown","open","unknown",101,r#"{"reason":"send interrupted"}"#);
        assert_eq!(authorize(&mut service,"p0c-entry",202)["authorized"],false);
        // Quality circuit must never gate owned-position close/reconciliation.
        insert("p0c-close","close","dispatched",102,"{}");
        assert_eq!(authorize(&mut service,"p0c-close",202)["authorized"],true);
        db.execute("UPDATE broker_commands SET state='observed' WHERE id='p0c-unknown'",[]).unwrap();
        assert_eq!(authorize(&mut service,"p0c-entry",203)["authorized"],true);
        for n in 0..3 {
            insert(&format!("p0c-reject-{n}"),"open","rejected",300+n,
                r#"{"retcode":10019,"reason":"broker margin"}"#);
        }
        assert_eq!(authorize(&mut service,"p0c-entry",400)["authorized"],false);
        assert_eq!(authorize(&mut service,"p0c-close",401)["authorized"],true);
        let quality=service.route("GET","/v1/execution/quality",&[],&db,&status,402).unwrap();
        assert_eq!(quality["entry_circuit"]["state"],"blocked");
        assert_eq!(quality["entry_circuit"]["recent_broker_rejections"],3);
        assert_eq!(quality["entry_circuit"]["never_blocks_close_or_reconcile"],true);
        // Failed final authorization never changes durable intent to another state.
        assert_eq!(state(&db,"p0c-entry"),"dispatched");
    }

    #[test]
    fn p0c_fast_cache_is_account_bound_and_missing_data_never_authorizes_real() {
        let db=setup();
        let mut service=Service::new(&db).unwrap();
        let mut t=telemetry();
        t.demo=false;t.account_trade_mode=Some(2);
        service.inner.settings.target="live".into();
        service.inner.settings.login=t.login;
        service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone();
        assert!(!service.quality_circuit.entry_allowed(&t,true));
        service.refresh_quality_circuit(&db,&t,100);
        assert!(service.quality_circuit.entry_allowed(&t,true));
        let mut next=t.clone();next.symbol="EURUSD".into();
        assert!(!service.quality_circuit.entry_allowed(&next,true));
        queue(&db,&t,"p0c-pending","system",0.01,1.,101);
        db.execute("UPDATE broker_commands SET state='unknown' WHERE id='p0c-pending'",[]).unwrap();
        service.refresh_quality_circuit(&db,&t,102);
        assert!(!service.quality_circuit.entry_allowed(&t,true));
        let prior=service.quality_circuit.clone();
        assert_eq!(prior.reason,"unresolved_broker_submission");
        assert!(!prior.entry_allowed(&t,true));
        // Demo is optional/advisory but does not bypass mandatory Gateway/broker rules.
        assert!(prior.entry_allowed(&t,false));
    }

}
