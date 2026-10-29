//! Execution semantics wrapper: one account/mode-scoped coordinator or selected solo brain owns MT5 entries.
//! Portfolio exposure v1 is a Core-side fail-closed entry gate across all BrainTrade identities.
//! The previous 0.6 engine is preserved in execution_legacy.rs and still owns durable broker intents.
#[path = "execution_legacy.rs"]
mod legacy;
pub(crate) use legacy::PENDING_BROKER_SQL;
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
            "note":"Portfolio limits block new entries only. Broker SL/TP and reconciliation remain active; a timed close can be deferred while an order is unresolved."
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

fn command_values(db: &Connection, key: &str, include_queued: bool) -> Result<Vec<(Value,String,Value)>> {
    let queued = if include_queued { "" } else { "AND state<>'queued'" };
    let sql = format!(
        "SELECT payload,state,result FROM broker_commands WHERE context=?1 AND kind='open' AND {} {queued} ORDER BY created_ms,id",
        PENDING_BROKER_SQL
    );
    let mut statement = db.prepare(&sql)?;
    let rows = statement
        .query_map([key], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(payload,state,result)| Ok((serde_json::from_str(&payload)?,state,serde_json::from_str(&result)?)))
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
    for position in &owned {
        result.open_positions = result.open_positions.saturating_add(1);
        result.lots += position.volume.max(0.0);
        match owned_position_risk(t, c, position) {
            Some(risk) => result.estimated_risk += risk,
            None => result.risk_known = false,
        }
    }
    // An exact broker-history link may account for a visible filled slice once;
    // the possible unfilled remainder still contributes lots and stop risk.
    let mut credited_by_position=BTreeMap::<u64,f64>::new();
    for (request,state,broker_result) in command_values(db, &key, include_queued)? {
        let magic = request["magic"].as_u64().unwrap_or(0);
        if state=="filled" && visible_magics.contains(&magic) {
            continue;
        }
        let lots = request["lots"].as_f64().unwrap_or(0.0);
        let linked_position=broker_result["position_id"].as_u64().filter(|id|*id>0)
            .and_then(|id|owned.iter().find(|p|p.magic==magic
                && (p.identifier==id || p.identifier==0 && p.ticket==id)).map(|p|(id,*p)));
        if matches!(state.as_str(),"partial"|"observed") && broker_result["recovered_from_history"]==true {
            if let Some((id,position))=linked_position {
                let filled=broker_result["volume"].as_f64().unwrap_or(f64::NAN);
                if lots.is_finite() && lots>0. && filled.is_finite() && filled>0.
                    && position.volume.is_finite() && position.volume>0. {
                    let already=*credited_by_position.get(&id).unwrap_or(&0.);
                    let credit=filled.min((position.volume-already).max(0.)).min(lots);
                    *credited_by_position.entry(id).or_default()+=credit;
                    let remainder=(lots-credit).max(0.);
                    result.lots+=remainder;
                    if remainder>0. {
                        match intent_risk(t,c,&request) {
                            Some(risk)=>result.estimated_risk+=risk*remainder/lots,
                            None=>result.risk_known=false,
                        }
                    }
                    continue;
                }
            }
        }
        // Without an exact position/deal link, an unresolved command may be an
        // additional exposure even when another position shares its magic.
        result.open_intents = result.open_intents.saturating_add(1);
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
    sniper_classic_forward: crate::sniper_classic_forward::Study,
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
        // Old per-brain flags do not choose an execution owner. A scoped solo
        // selection is loaded only after a matching reconciled broker snapshot.
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
            sniper_classic_forward: crate::sniper_classic_forward::Study::default(),
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
        self.sniper_classic_forward.clear_on_mode_change();
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
    pub fn risk_master_for_status(&self,status:&Status)->Option<bool>{
        status.telemetry.as_ref().filter(|_|status.gateway_fresh&&self.account_matches(status))
            .and_then(|t|self.inner.risk_master(t))
    }
    pub fn selected_owner_for_status<'a>(&'a self,status:&Status)->Option<&'a str>{
        let t=status.telemetry.as_ref().filter(|_|status.gateway_fresh&&self.account_matches(status))?;
        self.inner.active_mode.map(|mode|self.inner.solo_owner(t,mode))
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
        let risk_master=status.telemetry.as_ref().and_then(|t|self.inner.risk_master(t)).unwrap_or(true);
        anyhow::ensure!(!risk_master || self.portfolio.enabled,
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
    fn apply_portfolio(&mut self,db:&Connection,t:&Telemetry,c:&Config)->Result<()> {
        let master=self.inner.risk_master(t).unwrap_or(true);
        let current=if master {enforce_portfolio(db,t,c,&self.portfolio)?}else{exposure(db,t,c,true)?};
        self.portfolio_state=current.json(&self.portfolio,t.equity);
        self.portfolio_state["risk_master_enabled"]=json!(master);
        self.portfolio_state["effective_enabled"]=json!(master && self.portfolio.enabled);
        if !master {self.portfolio_state["note"]=json!("حدود المحفظة محفوظة لكنها موقوفة عبر مفتاح مخاطر هذا الحساب؛ إدارة الصفقات الحالية والتسوية مستمرة.");}
        Ok(())
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
        self.inner.refresh_risk_master(db,telemetry)?;
        self.inner.refresh_solo(db,telemetry,config.trading.mode)?;
        self.inner.controls = crate::brain_control::load_account(db, &context(telemetry), config.trading.mode, telemetry.account_trade_mode)?;
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
        // Legacy engine still owns durable broker intents and reconciliation.
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
        // The reconciled path may deliver quotes the Fast Pulse did not see.
        // Detect that before the adaptive Sniper consumes this snapshot,
        // and invalidate comparability until a new all-future warmup.
        self.sniper_frozen_tick.reconcile(telemetry,config);
        // A slower full broker reconciliation flushes bounded research-only
        // events; no SQLite write or failure propagation on Fast Pulse Tick.
        self.sniper_frozen_tick.flush_research_events(db,telemetry);
        self.inner
            .observe(db, telemetry, config, entries_allowed, frames, now)?;
        // Separate read-only Classic forward cohort uses full M1 snapshots.
        // It never consumes or contaminates Learner Fast Pulse comparisons.
        self.sniper_classic_forward.observe(telemetry,config,frames,now);
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
        self.apply_portfolio(db,telemetry,config)?;
        // Diagnostic sampling uses normal reconciled snapshots only, after
        // broker intents and portfolio decisions. Never add this to Fast Pulse.
        if self.inner.matches_bound_telemetry(telemetry) && self.inner.solo_owner(telemetry,config.trading.mode)=="system" {
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

    fn enrich_roles(&self,mut value: Value, db: &Connection, status: &Status) -> Result<Value> {
        let key = status
            .telemetry
            .as_ref()
            .map(crate::coordinator_learning::context)
            .unwrap_or_default();

        let owner=status.telemetry.as_ref().filter(|_|status.gateway_fresh&&self.account_matches(status))
            .and_then(|t|self.inner.active_mode.map(|mode|self.inner.solo_owner(t,mode).to_owned()));
        value["selected_owner"]=json!(owner);
        if let Some(brains) = value.get_mut("brains").and_then(Value::as_array_mut) {
            for brain in brains {
                let id = brain["config"]["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let solo = id!="system" && owner.as_deref()==Some(id.as_str());
                let sql=format!("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain=?2 AND {})",PENDING_BROKER_SQL);
                let in_flight: bool = db.query_row(
                    &sql,
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
                brain["role"] = json!(if id == "system" && owner.as_deref()==Some("system") {
                    "coordinator"
                } else if id == "system" {
                    "observer"
                } else if solo {
                    "solo"
                } else if returning {
                    "returning"
                } else {
                    "advisor"
                });
                brain["solo_enabled"] = json!(solo);
                brain["participates_in_coordinator"] = json!(owner.as_deref()==Some("system") && id != "system" && !returning);
                brain["config"]["enabled"] = json!(owner.as_deref()==Some(id.as_str()));
            }
        }
        value["learning_review"] = json!(crate::coordinator_learning::broker_review(db,&key)?);
        value["execution_model"] = json!(match owner.as_deref(){None=>"unavailable",Some("system")=>"adaptive-member-coordinator-v1",Some(_)=>"single-brain-v1"});
        value["note"] = json!("عقل واحد فقط هو المنفّذ المختار مع START؛ بقية العقول للتحليل أو Paper. تغيير المنفّذ يحتاج STOP وتسوية وإغلاق مراكز البرنامج للرمز الحالي.");
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
            let mut report=crate::frozen_coordinator::report(db,&context(t),config.trading.mode)?;
            report["sealed_candidate"]=crate::g7_sealed_candidate::status(
                db,&context(t),config.trading.mode)?;
            return Ok(report);
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
                anyhow::ensure!(self.inner.solo_owner(t,req.mode)=="system","أعد المنفّذ إلى المنسق قبل تجميد بحث المنسق");
                let controls=crate::brain_control::load_account(db,&req.context,req.mode,t.account_trade_mode)?;
                crate::frozen_coordinator::freeze(db,t,config,&self.inner.brains,
                    &self.inner.weights,&controls,&self.inner.student_policies,now)
            },
            "seal_candidate"=>{
                anyhow::ensure!(self.inner.solo_owner(t,req.mode)=="system","أعد المنفّذ إلى المنسق قبل ختم بحث المنسق");
                let controls=crate::brain_control::load_account(db,&req.context,req.mode,t.account_trade_mode)?;
                crate::g7_sealed_candidate::seal(db,t,config,&self.inner.brains,
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
        if path=="/v1/execution/frozen-classic" {
            let t=status.telemetry.as_ref().filter(|t|
                status.gateway_fresh && self.account_matches(status)
                &&self.inner.active_mode==Some(braintrade_domain::trading::TradingMode::Sniper)
                &&t.demo&&t.account_trade_mode==Some(0));
            if method=="GET" {return Ok(self.sniper_classic_forward.report(t,now));}
            anyhow::ensure!(method=="POST","Unsupported Classic forward research method");
            anyhow::ensure!(!status.requested_run && !self.pending(db)?,
                "STOP and reconcile broker intents before Classic research");
            let t=t.context("Fresh bound MT5 Demo Sniper Classic account required")?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct ClassicRequest{action:String,context:String}
            let request:ClassicRequest=serde_json::from_slice(body)?;
            anyhow::ensure!(request.context==context(t),
                "Classic research account/session drift");
            return match request.action.as_str(){
                "freeze"=>{
                    anyhow::ensure!(self.inner.solo_owner(t,braintrade_domain::trading::TradingMode::Sniper)=="system","أعد المنفّذ إلى المنسق قبل بحث القناص المجمد");
                    let fast=self.fast.as_ref()
                        .filter(|f|f.config.trading.mode==braintrade_domain::trading::TradingMode::Sniper
                            &&f.config.trading.sniper_brain==braintrade_domain::trading::SniperBrain::Classic
                            &&f.telemetry.login==t.login &&f.telemetry.server==t.server
                            &&f.telemetry.symbol==t.symbol &&f.telemetry.currency==t.currency)
                        .context("Classic research quote cache not ready")?;
                    self.sniper_classic_forward.freeze(t,&fast.config,now)
                },
                "stop"=>self.sniper_classic_forward.stop(t),
                _=>anyhow::bail!("Unknown Classic forward research action")
            };
        }
        if path=="/v1/execution/frozen-sniper" {
            let t=status.telemetry.as_ref().filter(|t|
                status.gateway_fresh&&self.account_matches(status)
                &&self.inner.active_mode==Some(braintrade_domain::trading::TradingMode::Sniper)
                &&t.demo&&t.account_trade_mode==Some(0));
            if method=="GET" {
                let mut view=self.sniper_frozen_tick.report(t,now);
                // Account-verified, read-only history survives Core restart.
                // Never fetch another Demo account's archived research.
                if let Some(t)=t {
                    let account=crate::coordinator_learning::context(t);
                    view["research_artifact_history"]=
                        crate::sniper_research_artifact::recent(db,&account)?;
                    // Also expose one archived journal when the Core restarts
                    // and the in-memory trial no longer exists. Never infer
                    // missing future ticks from an older persisted sample.
                    let latest=view["research_artifact_history"]["artifacts"]
                        .as_array().and_then(|items|items.first()).cloned();
                    if let Some(item)=latest.filter(|item|item["integrity"]=="verified") {
                        if let (Some(id),Some(hash))=(item["id"].as_str(),item["sha256"].as_str()){
                            view["last_archived_forward_journal"]=
                                match crate::sniper_forward_journal::report(db,&account,id,hash){
                                    Ok(report)=>report,
                                    Err(_)=>json!({"integrity":"warning",
                                        "historical_oos":false,"broker_pnl":null})
                                };
                            view["last_archived_holdout_diagnostic"]=
                                crate::sniper_forward_holdout::from_journal(
                                    db,&account,id,hash)
                                .unwrap_or_else(|_|json!({"status":"unverified",
                                    "independent_oos":false,"broker_pnl":null}));
                            view["last_archived_preregistered_costs"]=
                                crate::sniper_forward_costs::from_journal(
                                    db,&account,id,hash)
                                .unwrap_or_else(|_|json!({"status":"unverified",
                                    "independent_oos":false,"broker_pnl":null}));
                            view["last_archived_demo_booked_evidence"]=
                                crate::sniper_demo_booked_audit::inspect(
                                    db,&account,id,hash)
                                .unwrap_or_else(|_|json!({"status":"unverified",
                                    "independent_oos":false,
                                    "matched_with_frozen_markouts":false,
                                    "net_demo_account_currency":null}));
                            view["last_archived_demo_recovery_audit"]=
                                crate::sniper_demo_recovery_audit::inspect(
                                    db,t,id,hash)
                                .unwrap_or_else(|_|json!({"status":"not_verified",
                                    "requires_live_broker_reconciliation":true,
                                    "new_orders_authorized":false}));
                        }
                    }
                    if let Some(id)=view["research_artifact_id"].as_str().map(str::to_owned) {
                        let expected=view["research_artifact_sha256"].as_str().map(str::to_owned);
                        match crate::sniper_research_artifact::verify(db,&account,&id,expected.as_deref()){
                            Ok(verified)=>{
                                view["research_artifact"]=verified;
                                view["prospective_event_journal"]=
                                    match crate::sniper_forward_journal::report(db,&account,
                                        &id,expected.as_deref().unwrap_or("")){
                                        Ok(report)=>report,
                                        Err(_)=>json!({"integrity":"warning",
                                            "historical_oos":false,"broker_pnl":null})
                                    };
                                view["chronological_holdout_diagnostic"]=
                                    crate::sniper_forward_holdout::from_journal(
                                        db,&account,&id,expected.as_deref().unwrap_or(""))
                                    .unwrap_or_else(|_|json!({"status":"unverified",
                                        "independent_oos":false,"broker_pnl":null}));
                                view["preregistered_cost_evidence"]=
                                    crate::sniper_forward_costs::from_journal(
                                        db,&account,&id,expected.as_deref().unwrap_or(""))
                                    .unwrap_or_else(|_|json!({"status":"unverified",
                                        "independent_oos":false,"broker_pnl":null}));
                                // Actual MT5 Demo booked trades belong to the executing
                                // coordinator, NOT the zero-order frozen study.
                                // Never pair hypothetical five-second markouts.
                                view["demo_booked_execution_audit"]=
                                    crate::sniper_demo_booked_audit::inspect(
                                        db,&account,&id,expected.as_deref().unwrap_or(""))
                                    .unwrap_or_else(|_|json!({"status":"unverified",
                                        "independent_oos":false,
                                        "matched_with_frozen_markouts":false,
                                        "net_demo_account_currency":null}));
                                view["demo_recovery_intent_audit"]=
                                    crate::sniper_demo_recovery_audit::inspect(
                                        db,t,&id,expected.as_deref().unwrap_or(""))
                                    .unwrap_or_else(|_|json!({"status":"not_verified",
                                        "requires_live_broker_reconciliation":true,
                                        "new_orders_authorized":false}));
                            },
                            Err(_)=>{
                                // Never present tampered research samples as verified.
                                view["status"]=json!("integrity_failure");
                                view["training_checkpoint"]=serde_json::Value::Null;
                                view["forward_direction_audit"]=serde_json::Value::Null;
                                view["research_artifact"]=json!({"integrity":"failed",
                                    "independent_oos":false,"broker_pnl":null});
                            }
                        }
                    }
                }
                return Ok(view);
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
            return match request.action.as_str(){
                "freeze"=>{
                    anyhow::ensure!(self.inner.solo_owner(t,braintrade_domain::trading::TradingMode::Sniper)=="system","أعد المنفّذ إلى المنسق قبل بحث القناص المجمد");
                    let fast=self.fast.as_ref()
                        .filter(|f|f.config.trading.mode==braintrade_domain::trading::TradingMode::Sniper
                            &&f.telemetry.login==t.login &&f.telemetry.server==t.server
                            &&f.telemetry.symbol==t.symbol&&f.telemetry.currency==t.currency)
                        .context("Sniper reconciled quote cache not ready")?;
                    {
                        // Freeze is a stopped operator action, never a Tick path.
                        // Reconcile the actual cached Learner replay weights to
                        // the current account's completed Demo broker ledger.
                        let checkpoint=crate::sniper_learning::frozen_training_checkpoint(
                            db,t,&self.inner.sniper,now)?;
                        self.sniper_frozen_tick.freeze_durable(db,t,&fast.config,
                            &self.inner.sniper,checkpoint,now)
                    }
                },
                "stop"=>self.sniper_frozen_tick.stop_with_research_flush(db,t),
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
            crate::brain_control::save(db,&crate::brain_control::account_key(&key,status.telemetry.as_ref().unwrap().account_trade_mode),request.mode,&request.brain,request.control)?;
            self.inner.controls.insert(request.brain,request.control);
            self.fast=None;
            self.inner.set_mode(request.mode);
            return Ok(json!({"ok":true}));
        }
        if method=="POST" && path=="/v1/execution/solo" {
            anyhow::ensure!(!status.requested_run && !self.pending(db)?,"أوقف التداول وانتظر تسوية الأوامر قبل تغيير العقل المنفّذ");
            anyhow::ensure!(status.gateway_fresh && self.account_matches(status),"اربط حساب MT5 الحالي وانتظر بيانات حديثة");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request{context:String,mode:braintrade_domain::trading::TradingMode,brain:String}
            let request:Request=serde_json::from_slice(body)?;
            let t=status.telemetry.as_ref().unwrap();
            anyhow::ensure!(request.context==context(t) && self.inner.active_mode==Some(request.mode),"الحساب أو الرمز أو الوضع تغيّر؛ أعد الاختيار");
            if request.brain==braintrade_domain::sniper::ID {
                anyhow::ensure!(request.mode==braintrade_domain::trading::TradingMode::Sniper
                    && self.fast.as_ref().is_some_and(|f|context(&f.telemetry)==request.context
                        && f.telemetry.account_trade_mode==t.account_trade_mode
                        && braintrade_domain::sniper::selected(&f.config)),
                    "القناص المتعلّم يحتاج وضع Sniper واختيار Learner من إعدادات القناص أولاً");
            }
            anyhow::ensure!(!t.positions.iter().any(|p|p.symbol==t.symbol&&execution::brain_for_magic(p.magic).is_some()),
                "أغلق مراكز البرنامج للرمز الحالي قبل تغيير العقل المنفّذ");
            use braintrade_domain::trading::TradingMode;
            if request.mode==TradingMode::Sniper {
                anyhow::ensure!(self.sniper_frozen_tick.report(Some(t),now)["status"]!="active"
                    && self.sniper_classic_forward.report(Some(t),now)["status"]!="active",
                    "أوقف دراسة القناص المجمدة قبل تغيير العقل المنفّذ");
            }else{
                anyhow::ensure!(crate::frozen_coordinator::report(db,&request.context,request.mode)?["status"]!="active",
                    "أوقف دراسة المنسق المجمدة قبل تغيير العقل المنفّذ");
            }
            self.inner.set_solo(db,t,request.mode,&request.brain,now)?;
            self.fast=None;
            return Ok(json!({"ok":true,"selected_owner":request.brain,"mode":request.mode,
                "scope":"bound_account_symbol_mode","start_unchanged":true}));
        }
        if method=="POST" && path=="/v1/execution/risk-master" {
            anyhow::ensure!(!status.requested_run && !self.pending(db)?,"أوقف التداول وانتظر تسوية أوامر الوسيط قبل تغيير إدارة المخاطر");
            anyhow::ensure!(status.gateway_fresh && self.account_matches(status),"اربط الحساب الحالي وانتظر اتصال MT5 حديثاً");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request{context:String,enabled:bool,confirm_live_login:String}
            let request:Request=serde_json::from_slice(body)?;
            let t=status.telemetry.as_ref().unwrap();
            anyhow::ensure!(request.context==context(t),"الحساب أو الرمز تغيّر؛ أعد التأكيد");
            anyhow::ensure!((t.demo && t.account_trade_mode==Some(0)) || (!t.demo && t.account_trade_mode==Some(2)),"نوع حساب MT5 غير مؤكّد");
            if !request.enabled && !t.demo {
                anyhow::ensure!(request.confirm_live_login==t.login.to_string(),"لتعطيل المخاطر على REAL، أدخل رقم حساب MT5 المطابق");
            }
            self.inner.set_risk_master(db,t,request.enabled,now)?;
            self.fast=None;
            self.portfolio_state["risk_master_enabled"]=json!(request.enabled);
            self.portfolio_state["effective_enabled"]=json!(request.enabled && self.portfolio.enabled);
            self.portfolio_state["note"]=if request.enabled {
                json!("Portfolio limits block new entries only. Broker SL/TP and reconciliation remain active; a timed close can be deferred while an order is unresolved.")
            }else{json!("حدود المحفظة محفوظة لكنها موقوفة عبر مفتاح مخاطر هذا الحساب؛ إدارة الصفقات الحالية والتسوية مستمرة.")};
            return Ok(json!({"ok":true,"enabled":request.enabled,"scope":"bound_broker_account_and_mode",
                "broker_sl_tp_retained":true,"mt5_margin_and_execution_checks_retained":true}));
        }
        if method=="POST" && path=="/v1/execution/brain-paper-all" {
            anyhow::ensure!(!status.requested_run && !self.pending(db)?, "أوقف النظام وانتظر تسوية الأوامر قبل تغيير Paper");
            anyhow::ensure!(status.gateway_fresh && self.account_matches(status), "اربط الحساب الحالي وانتظر اتصالاً حديثاً");
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request {context:String, mode:braintrade_domain::trading::TradingMode, paper:bool}
            let request:Request=serde_json::from_slice(body)?;
            anyhow::ensure!(self.inner.active_mode==Some(request.mode), "الوضع تغيّر؛ حدّث الصفحة قبل الحفظ");
            let t=status.telemetry.as_ref().unwrap();
            let key=context(t);
            anyhow::ensure!(request.context==key, "الحساب أو الرمز تغيّر؛ حدّث الصفحة قبل الحفظ");
            let controls=crate::brain_control::set_all_paper(db,&key,request.mode,t.account_trade_mode,request.paper)?;
            self.inner.controls=controls;
            self.fast=None;
            self.inner.set_mode(request.mode);
            return Ok(json!({"ok":true,"paper":request.paper,"scope":"secondary_brains_current_account_and_mode",
                "existing_paper_positions_settle":true,"global_learning_frozen":false}));
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
            let sniper_selected=self.fast.as_ref().filter(|f|!key.is_empty() && context(&f.telemetry)==key
                && status.telemetry.as_ref().is_some_and(|t|t.account_trade_mode==f.telemetry.account_trade_mode))
                .map(|f|braintrade_domain::sniper::selected(&f.config));
            let account_mode=status.telemetry.as_ref().filter(|_|!key.is_empty()).and_then(|t|t.account_trade_mode);
            crate::brain_control::enrich(&mut value,db,&key,self.inner.active_mode.unwrap_or_default(),account_mode,sniper_selected)?;
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
            self.inner.sync_owner_flags(&next.telemetry,next.config.trading.mode);
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
            self.apply_portfolio(db,&next.telemetry,&next.config)?;

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
            // Historical positions are still readable offline, but a stale
            // or wrong-mode snapshot must never certify a partial close as
            // fully settled or be used as a source of floating PnL.
            let t = status.telemetry.as_ref().filter(|t|
                status.gateway_fresh && self.account_matches(status)
                && t.connected && !t.symbol.is_empty());
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
                if let (Some(t),Some(mode))=(status.telemetry.as_ref(),self.inner.active_mode){self.inner.sync_owner_flags(t,mode);}
                result["start"] = json!(false);
                result["role"] = json!("system_settings");
                result["note"] = json!("إعدادات المنسق محفوظة؛ اختيار المنفّذ يتم من مفتاح العقل المنفرد ولا يشغّل START.");
                return Ok(result);
            }
            anyhow::ensure!(!config.enabled,
                "استخدم اختيار العقل المنفّذ المخصص؛ لا يمكن تفعيل التنفيذ بتعديل الإعدادات وحده");
            let mut result = self.inner.route(method, path, body, db, status, now)?;
            if let (Some(t),Some(mode))=(status.telemetry.as_ref(),self.inner.active_mode){self.inner.sync_owner_flags(t,mode);}
            result["start"] = json!(false);
            result["role"] = json!("brain_settings");
            result["note"] = json!("الإعدادات لا تغيّر اختيار المنفّذ ولا تبدأ التداول.");
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
            let mut value = self.enrich_roles(value, db, status)?;
            value["portfolio"] = self.portfolio_state.clone();
            let risk=self.risk_master_for_status(status);
            value["risk_master"]=json!({"enabled":risk,"scope":"bound_broker_account_and_mode",
                "note":"المفتاح يؤثر على ميزانية وقف الصفقة، وحدود المحفظة والخسارة اليومية والحدود الاختيارية. فحوص MT5 والهوية والتسوية وSL/TP تبقى فعالة."});
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
    fn partial_exposure_reserves_unfilled_remainder_without_inventing_position_links() {
        let db=setup();
        let mut t=telemetry();
        t.positions.push(Position {magic:execution::magic("trend").unwrap(),
            identifier:55,ticket:55,symbol:"XAUUSD".into(),side:"BUY".into(),
            volume:0.04,price_open:2000.,price_current:2000.1,sl:1999.9,tp:2000.2,
            ..Default::default()});
        queue(&db,&t,"partial-a","trend",0.10,0.10,1);
        let linked=json!({"state":"partial","recovered_from_history":true,
            "position_id":55,"volume":0.04,"deal":801,"deals":[801]});
        db.execute("UPDATE broker_commands SET state='partial',result=?1 WHERE id='partial-a'",
            [linked.to_string()]).unwrap();
        let view=exposure(&db,&t,&Config::default(),true).unwrap();
        assert_eq!(view.open_positions,1);assert_eq!(view.open_intents,0);
        assert!((view.lots-0.10).abs()<1e-9,"Visible 0.04 plus possible remaining 0.06");
        let mut unlinked=linked;unlinked["position_id"]=json!(99);
        db.execute("UPDATE broker_commands SET result=?1 WHERE id='partial-a'",
            [unlinked.to_string()]).unwrap();
        let conservative=exposure(&db,&t,&Config::default(),true).unwrap();
        assert_eq!(conservative.open_intents,1);
        assert!((conservative.lots-0.14).abs()<1e-9,
            "Unlinked position cannot remove the command's possible exposure");
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
    fn bulk_paper_requires_fresh_bound_stopped_account_and_reconciled_orders() {
        use braintrade_domain::trading::TradingMode;
        let db=setup();
        let mut service=Service::new(&db).unwrap();
        let mut t=telemetry(); t.demo=true; t.account_trade_mode=Some(0);
        service.inner.settings.login=t.login; service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone(); service.inner.settings.target="demo".into();
        service.inner.set_mode(TradingMode::Expert);
        let key=context(&t);
        let body=json!({"context":key,"mode":"expert","paper":false}).to_string();
        let mut status=Status::default(); status.telemetry=Some(t.clone());
        assert!(service.route("POST","/v1/execution/brain-paper-all",body.as_bytes(),&db,&status,1000).is_err());
        status.gateway_fresh=true; status.requested_run=true;
        assert!(service.route("POST","/v1/execution/brain-paper-all",body.as_bytes(),&db,&status,1000).is_err());
        status.requested_run=false;
        queue(&db,&t,"in-flight","trend",0.01,0.05,1000);
        assert!(service.route("POST","/v1/execution/brain-paper-all",body.as_bytes(),&db,&status,1000).is_err());
        db.execute("UPDATE broker_commands SET state='observed' WHERE id='in-flight'",[]).unwrap();
        let wrong=json!({"context":"wrong-account","mode":"expert","paper":false}).to_string();
        assert!(service.route("POST","/v1/execution/brain-paper-all",wrong.as_bytes(),&db,&status,1000).is_err());
        let wrong=json!({"context":key,"mode":"sniper","paper":false}).to_string();
        assert!(service.route("POST","/v1/execution/brain-paper-all",wrong.as_bytes(),&db,&status,1000).is_err());
        let result=service.route("POST","/v1/execution/brain-paper-all",body.as_bytes(),&db,&status,1000).unwrap();
        assert_eq!(result["global_learning_frozen"],false);
        let persisted=crate::brain_control::load_account(&db,&key,TradingMode::Expert,Some(0)).unwrap();
        assert_eq!(persisted.len(),braintrade_domain::execution::BRAINS.len()-1);
        assert!(persisted.values().all(|c|!c.paper));
    }

    #[test]
    fn risk_master_is_account_and_broker_mode_scoped_and_requires_explicit_real_consent() {
        let db=setup();let mut service=Service::new(&db).unwrap();
        let mut t=telemetry();t.demo=true;t.account_trade_mode=Some(0);t.trade_allowed=true;t.trade_expert=true;
        service.inner.settings.login=t.login;service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone();service.inner.settings.target="demo".into();
        service.inner.refresh_risk_master(&db,&t).unwrap();
        assert_eq!(service.inner.risk_master(&t),Some(true));
        let key=context(&t);
        let mut status=Status::default();status.telemetry=Some(t.clone());status.gateway_fresh=true;
        let body=|enabled:bool,confirm:&str|json!({"context":key,"enabled":enabled,"confirm_live_login":confirm}).to_string();
        status.requested_run=true;
        assert!(service.route("POST","/v1/execution/risk-master",body(false,"").as_bytes(),&db,&status,1000).is_err());
        status.requested_run=false;
        service.route("POST","/v1/execution/risk-master",body(false,"").as_bytes(),&db,&status,1000).unwrap();
        assert_eq!(service.inner.risk_master(&t),Some(false));
        queue(&db,&t,"unresolved","system",0.01,0.05,1001);
        assert!(service.route("POST","/v1/execution/risk-master",body(true,"").as_bytes(),&db,&status,1002).is_err());
        db.execute("UPDATE broker_commands SET state='observed' WHERE id='unresolved'",[]).unwrap();
        let mut real=t.clone();real.demo=false;real.account_trade_mode=Some(2);
        service.inner.settings.target="live".into();service.inner.refresh_risk_master(&db,&real).unwrap();
        assert_eq!(service.inner.risk_master(&real),Some(true),"Demo opt-out must never cross to REAL");
        status.telemetry=Some(real.clone());
        assert!(service.route("POST","/v1/execution/risk-master",body(false,"wrong").as_bytes(),&db,&status,1003).is_err());
        service.route("POST","/v1/execution/risk-master",body(false,&real.login.to_string()).as_bytes(),&db,&status,1004).unwrap();
        assert_eq!(service.inner.risk_master(&real),Some(false));
        let start=json!({"confirm_live_login":real.login.to_string()}).to_string();
        assert!(service.validate_start(&status,start.as_bytes()).is_ok(),"Explicit risk-off does not require a suspended portfolio guard");
        let mut restored=Service::new(&db).unwrap();restored.inner.refresh_risk_master(&db,&real).unwrap();
        assert_eq!(restored.inner.risk_master(&real),Some(false),"Account setting survives restart");
        let mut other=real.clone();other.login+=1;
        restored.inner.settings.login=other.login;restored.inner.refresh_risk_master(&db,&other).unwrap();
        assert_eq!(restored.inner.risk_master(&other),Some(true),"New accounts default to risk on");
    }

    #[test]
    fn risk_master_off_suspends_portfolio_entry_gate_only_for_the_bound_account() {
        let db=setup();let mut service=Service::new(&db).unwrap();
        let mut t=telemetry();t.demo=true;t.account_trade_mode=Some(0);
        service.inner.settings.login=t.login;service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone();service.inner.settings.target="demo".into();
        service.inner.set_risk_master(&db,&t,false,100).unwrap();
        service.portfolio=PortfolioLimits{enabled:true,max_positions:1,max_lots:1.,max_risk_percent:2.};
        queue(&db,&t,"first-risk-off","system",0.01,0.05,1000);
        queue(&db,&t,"second-risk-off","trend",0.01,0.05,1001);
        service.apply_portfolio(&db,&t,&Config::default()).unwrap();
        assert_eq!(state(&db,"first-risk-off"),"queued");assert_eq!(state(&db,"second-risk-off"),"queued");
        assert_eq!(service.portfolio_state["effective_enabled"],false);
        let mut other=t.clone();other.login+=1;
        assert_eq!(service.inner.risk_master(&other),None,"Cached opt-out must not apply to another account");
    }

    #[test]
    fn solo_owner_is_scoped_exclusive_and_requires_stopped_flat_settled_account(){
        use braintrade_domain::{Position,trading::TradingMode};
        let db=setup();let mut service=Service::new(&db).unwrap();
        let mut t=telemetry();t.demo=true;t.account_trade_mode=Some(0);
        service.inner.settings.login=t.login;service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone();service.inner.settings.target="demo".into();
        service.inner.set_mode(TradingMode::Expert);
        service.inner.refresh_solo(&db,&t,TradingMode::Expert).unwrap();
        let key=context(&t);
        let body=|brain:&str|json!({"context":key,"mode":"expert","brain":brain}).to_string();
        let mut status=Status::default();status.telemetry=Some(t.clone());status.gateway_fresh=true;
        status.requested_run=true;
        assert!(service.route("POST","/v1/execution/solo",body("trend").as_bytes(),&db,&status,100).is_err());
        status.requested_run=false;
        status.telemetry.as_mut().unwrap().positions.push(Position{ticket:9,magic:execution::magic("system").unwrap(),symbol:t.symbol.clone(),..Default::default()});
        assert!(service.route("POST","/v1/execution/solo",body("trend").as_bytes(),&db,&status,101).is_err());
        status.telemetry.as_mut().unwrap().positions.clear();
        queue(&db,&t,"pending-solo-change","system",0.01,0.05,102);
        assert!(service.route("POST","/v1/execution/solo",body("trend").as_bytes(),&db,&status,103).is_err());
        db.execute("UPDATE broker_commands SET state='observed' WHERE id='pending-solo-change'",[]).unwrap();
        assert!(service.route("POST","/v1/execution/solo",body("unknown").as_bytes(),&db,&status,104).is_err());
        service.route("POST","/v1/execution/solo",body("trend").as_bytes(),&db,&status,105).unwrap();
        assert_eq!(service.inner.solo_owner(&t,TradingMode::Expert),"trend");
        assert_eq!(service.inner.brains.iter().filter(|b|b.enabled).map(|b|b.id.as_str()).collect::<Vec<_>>(),vec!["trend"]);
        assert_eq!(service.inner.solo_owner(&t,TradingMode::Sniper),"system");
        let mut other=t.clone();other.login+=1;
        assert_eq!(service.inner.solo_owner(&other,TradingMode::Expert),"system");
        let mut restarted=Service::new(&db).unwrap();
        restarted.inner.refresh_solo(&db,&t,TradingMode::Expert).unwrap();
        assert_eq!(restarted.inner.solo_owner(&t,TradingMode::Expert),"trend");
        status.requested_run=true;
        for brain in ["system","trend","gann"] {
            let id=format!("authorize-{brain}");
            let payload=json!({"id":id,"brain":brain,"trading_mode":"expert","kind":"open","target":"demo",
                "revision":service.inner.settings.revision,"context":key,"expires_ms":10000,"created_ms":200});
            db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload) VALUES(?1,?2,?3,'open',200,'dispatched',?4)",params![id,key,brain,payload.to_string()]).unwrap();
            let result=service.route("POST","/v1/execution/authorize",json!({"id":id}).to_string().as_bytes(),&db,&status,201).unwrap();
            assert_eq!(result["authorized"],brain=="trend","Only the selected owner can reach MT5");
        }
        status.requested_run=false;
        db.execute("UPDATE broker_commands SET state='observed' WHERE id LIKE 'authorize-%'",[]).unwrap();
        service.route("POST","/v1/execution/solo",body("system").as_bytes(),&db,&status,202).unwrap();
        assert_eq!(service.inner.solo_owner(&t,TradingMode::Expert),"system");
        let mut back=Service::new(&db).unwrap();back.inner.refresh_solo(&db,&t,TradingMode::Expert).unwrap();
        assert_eq!(back.inner.solo_owner(&t,TradingMode::Expert),"system");
    }

    #[test]
    fn selected_trend_brain_generates_and_authorizes_the_only_broker_intent(){
        use braintrade_domain::{Candle,trading::TradingMode};
        let db=setup();let mut service=Service::new(&db).unwrap();
        let now=1_800_000_000_000u64;
        let mut t=telemetry();t.demo=true;t.account_trade_mode=Some(0);t.margin_mode=Some(2);
        t.trade_allowed=true;t.trade_expert=true;t.quote_time_ms=Some(now);t.tick_age_ms=Some(0);
        t.bid=Some(2024.0);t.ask=Some(2024.01);t.point=Some(0.01);
        service.inner.settings.login=t.login;service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone();service.inner.settings.target="demo".into();
        service.inner.set_mode(TradingMode::Expert);
        let key=context(&t);let mut status=Status::default();status.telemetry=Some(t.clone());status.gateway_fresh=true;
        let choose=json!({"context":key,"mode":"expert","brain":"trend"}).to_string();
        service.route("POST","/v1/execution/solo",choose.as_bytes(),&db,&status,now-100).unwrap();
        let bars=(0..120).map(|i|{
            let open=2000.+i as f64*0.2;let close=open+0.2;
            FrameBar{candle:Candle{time:now/1000-(120-i)*60,open,high:close+0.1,low:open-0.1,close,tick_volume:100},spread_points:1,real_volume:0}
        }).collect::<Vec<_>>();
        let frames=BTreeMap::from([("M1".to_owned(),bars)]);
        let mut config=Config::default();config.symbol=t.symbol.clone();config.trading.mode=TradingMode::Expert;
        service.inner.observe(&db,&t,&config,true,&frames,now).unwrap();
        let entries:Vec<String>={let mut stmt=db.prepare("SELECT brain FROM broker_commands WHERE kind='open'").unwrap();
            let mapped=stmt.query_map([],|r|r.get(0)).unwrap();mapped.collect::<rusqlite::Result<Vec<_>>>().unwrap()};
        assert_eq!(entries,vec!["trend"],"Only the explicitly selected ready strategy may queue an entry");
        status.requested_run=true;
        let work=service.route("GET","/v1/execution/gateway",b"",&db,&status,now+1).unwrap();
        assert_eq!(work["command"]["brain"],"trend");
        assert_eq!(work["command"]["magic"],execution::magic("trend").unwrap());
        let auth=json!({"id":work["command"]["id"]}).to_string();
        assert_eq!(service.route("POST","/v1/execution/authorize",auth.as_bytes(),&db,&status,now+2).unwrap()["authorized"],true);
    }

    #[test]
    fn operator_close_queues_only_confirmed_owned_tickets_and_rechecks_stop() {
        use braintrade_domain::Position;
        let db=setup(); let mut service=Service::new(&db).unwrap();
        let mut t=telemetry(); t.demo=true; t.account_trade_mode=Some(0);
        service.inner.settings.login=t.login; service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone(); service.inner.settings.target="demo".into();
        t.positions=vec![
            Position{ticket:101,magic:execution::magic("system").unwrap(),symbol:t.symbol.clone(),side:"BUY".into(),volume:0.1,profit:2.,..Default::default()},
            Position{ticket:102,magic:execution::magic("trend").unwrap(),symbol:t.symbol.clone(),side:"SELL".into(),volume:0.1,profit:-1.,..Default::default()},
            Position{ticket:103,magic:99,symbol:t.symbol.clone(),side:"BUY".into(),volume:0.1,profit:5.,..Default::default()},
            Position{ticket:104,magic:execution::magic("system").unwrap(),symbol:"OTHER".into(),side:"BUY".into(),volume:0.1,profit:5.,..Default::default()},
        ];
        let key=context(&t);
        let mut status=Status::default(); status.telemetry=Some(t.clone()); status.gateway_fresh=true;
        let body=|tickets:Vec<u64>,profit_only:bool|{
            let positions=tickets.iter().map(|ticket|t.positions.iter().find(|p|p.ticket==*ticket)
                .map(|p|json!({"ticket":p.ticket,"magic":p.magic,"side":p.side,"volume":p.volume}))
                .unwrap_or(json!({"ticket":ticket,"magic":410600,"side":"BUY","volume":0.1}))).collect::<Vec<_>>();
            json!({"context":key,"positions":positions,"profit_only":profit_only}).to_string()
        };
        status.requested_run=true;
        assert!(service.route("POST","/v1/execution/close-selected",body(vec![101],false).as_bytes(),&db,&status,1000).is_err());
        status.requested_run=false;
        for (tickets,profit_only) in [(vec![101,101],false),(vec![101,999],false),(vec![101,103],false),(vec![104],false),(vec![101,102],true)] {
            assert!(service.route("POST","/v1/execution/close-selected",body(tickets,profit_only).as_bytes(),&db,&status,1000).is_err());
        }
        let mut wrong:Value=serde_json::from_str(&body(vec![101],false)).unwrap();
        wrong["context"]=json!("old-account");
        assert!(service.route("POST","/v1/execution/close-selected",wrong.to_string().as_bytes(),&db,&status,1000).is_err());
        wrong["context"]=json!(key); wrong["positions"][0]["volume"]=json!(0.2);
        assert!(service.route("POST","/v1/execution/close-selected",wrong.to_string().as_bytes(),&db,&status,1000).is_err());
        db.execute_batch("CREATE TRIGGER block_close BEFORE INSERT ON broker_commands WHEN NEW.brain='trend' BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(service.route("POST","/v1/execution/close-selected",body(vec![101,102],false).as_bytes(),&db,&status,1000).is_err());
        let count:i64=db.query_row("SELECT COUNT(*) FROM broker_commands",[],|r|r.get(0)).unwrap(); assert_eq!(count,0);
        db.execute_batch("DROP TRIGGER block_close;").unwrap();
        let result=service.route("POST","/v1/execution/close-selected",body(vec![101,102],false).as_bytes(),&db,&status,1000).unwrap();
        assert_eq!(result["queued"],2);
        assert_eq!(result["scope"],"owned_positions_current_symbol");
        assert!(service.route("POST","/v1/execution/close-selected",body(vec![101],false).as_bytes(),&db,&status,1001).is_err());
        let work=service.route("GET","/v1/execution/gateway",b"",&db,&status,1001).unwrap();
        assert_eq!(work["command"]["manual_close"],true);
        let auth=json!({"id":work["command"]["id"]}).to_string();
        status.requested_run=true;
        assert_eq!(service.route("POST","/v1/execution/authorize",auth.as_bytes(),&db,&status,1002).unwrap()["authorized"],false);
        status.requested_run=false;
        assert_eq!(service.route("POST","/v1/execution/authorize",auth.as_bytes(),&db,&status,1002).unwrap()["authorized"],true);
    }

    #[test]
    fn stale_or_wrong_mode_mt5_snapshot_does_not_override_booked_history() {
        let db=setup();
        let mut service=Service::new(&db).unwrap();
        let mut t=telemetry();
        t.demo=true;
        t.account_trade_mode=Some(0);
        service.inner.settings.login=t.login;
        service.inner.settings.server=t.server.clone();
        service.inner.settings.currency=t.currency.clone();
        service.inner.settings.target="demo".into();
        let key=context(&t);
        for (ticket,entry) in [(50,0),(51,1)] {
            let deal=json!({"ticket":ticket,"position_id":99,"symbol":"XAUUSD",
                "type":0,"entry":entry,"volume":0.10,
                "time_msc":ticket*1000,"price":2000.,
                "profit":0.,"commission":0.,"swap":0.,"fee":0.});
            db.execute("INSERT INTO broker_deals VALUES(?1,?2,99,'system',?3)",
                params![key,ticket,deal.to_string()]).unwrap();
        }
        // This terminal image claims a live position even though its last
        // observation is stale. Never let it relabel an independently
        // covered closed MT5 deal history as OPEN.
        t.positions.push(Position{
            identifier:99,ticket:99,magic:410_600,symbol:"XAUUSD".into(),
            side:"BUY".into(),volume:0.10,profit:500.,
            ..Default::default()
        });
        let stale=Status{telemetry:Some(t.clone()),gateway_fresh:false,
            ..Default::default()};
        let saved=service.route("GET","/v1/execution/trades",b"",
            &db,&stale,600).unwrap();
        assert_eq!(saved["rows"][0]["status"],"CLOSED");
        assert_eq!(saved["rows"][0]["floating_broker_pnl"],Value::Null);
        assert_eq!(saved["rows"][0]["broker_net_booked"],0.0);
        let mut wrong=t.clone();
        wrong.account_trade_mode=Some(2);
        wrong.demo=false;
        let mode_switched=Status{telemetry:Some(wrong),gateway_fresh:true,
            ..Default::default()};
        let view=service.route("GET","/v1/execution/trades",b"",
            &db,&mode_switched,610).unwrap();
        assert_eq!(view["rows"][0]["status"],"CLOSED");
        assert_eq!(view["rows"][0]["floating_broker_pnl"],Value::Null);
        // A *fresh* matching account may legitimately supersede stale
        // book history for current open-position status.
        let current=Status{telemetry:Some(t),gateway_fresh:true,
            ..Default::default()};
        let latest=service.route("GET","/v1/execution/trades",b"",
            &db,&current,620).unwrap();
        assert_eq!(latest["rows"][0]["status"],"PARTIAL / OPEN");
        assert_eq!(latest["rows"][0]["floating_broker_pnl"],500.0);
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
