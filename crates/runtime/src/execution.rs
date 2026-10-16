//! Execution semantics wrapper: coordinated main brain by default, per-brain SOLO override on demand.
//! Portfolio exposure v1 is a Core-side fail-closed entry gate across all BrainTrade identities.
//! The previous 0.6 engine is preserved in execution_legacy.rs and still owns durable broker intents.
#[path = "execution_legacy.rs"]
mod legacy;

use anyhow::{Context, Result};
use braintrade_domain::{
    execution::{self, BrainConfig},
    studio::FrameBar,
    Config, Status, Telemetry,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

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
    if position.volume <= 0.0 || !position.volume.is_finite() || position.sl <= 0.0 {
        return None;
    }
    let tick = t.tick_size.filter(|v| v.is_finite() && *v > 0.0)?;
    let tick_value = t
        .tick_value_loss
        .zip(t.tick_value_profit)
        .map(|(loss, profit)| loss.max(profit))
        .filter(|v| v.is_finite() && *v > 0.0)?;
    let distance = match position.side.as_str() {
        "BUY" if position.sl < position.price_open => position.price_open - position.sl,
        "SELL" if position.sl > position.price_open => position.sl - position.price_open,
        _ => return None,
    };
    if !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    Some(
        distance / tick * tick_value * position.volume
            + c.risk.commission_per_lot * position.volume,
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
    Some(stop / tick * tick_value * lots + c.risk.commission_per_lot * lots)
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
    portfolio: PortfolioLimits,
    portfolio_state: Value,
    portfolio_context: String,
    next_portfolio_ms: u64,
    learning_context: String,
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        crate::ablation::init(db)?;
        let mut inner = legacy::Service::new(db)?;
        // `system` is the coordinated main brain. It is controlled by global START/STOP,
        // not by the per-brain SOLO switch. Keep it enabled in memory even when an older
        // 0.6 checkpoint persisted it as false.
        if let Some(system) = inner.brains.iter_mut().find(|brain| brain.id == "system") {
            system.enabled = true;
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
        Ok(Self {
            inner,
            learner: crate::coordinator_learning::Learner::new(db)?,
            portfolio,
            portfolio_state,
            portfolio_context: String::new(),
            next_portfolio_ms: 0,
            learning_context: String::new(),
        })
    }

    fn save_portfolio(&self, db: &Connection) -> Result<()> {
        db.execute(
            "INSERT OR REPLACE INTO portfolio_execution_settings VALUES(1,?1)",
            [serde_json::to_string(&self.portfolio)?],
        )?;
        Ok(())
    }

    fn account_matches(&self, status: &Status) -> bool {
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

    pub fn pending(&self, db: &Connection) -> Result<bool> {
        self.inner.pending(db)
    }

    pub fn cancel_entries(&self, db: &Connection) -> Result<()> {
        self.inner.cancel_entries(db)
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
        let context=crate::coordinator_learning::context(telemetry);
        let learning_context=format!("{}:{:?}",context,config.trading.mode);
        if self.learning_context!=learning_context {self.inner.weights.clear();}
        // Legacy engine still owns durable broker intents/reconciliation. With `system`
        // forced on, it represents coordinated execution; every other `enabled` flag means SOLO.
        self.inner
            .observe(db, telemetry, config, run, frames, now)?;
        // Apply one global entry budget after all brains have proposed their queued opens and
        // before the gateway can retrieve them. Exits and reconciliation states are untouched.
        if self.portfolio.enabled || self.portfolio_context!=context || now>=self.next_portfolio_ms {
            let current = enforce_portfolio(db, telemetry, config, &self.portfolio)?;
            self.portfolio_state = current.json(&self.portfolio, telemetry.equity);
            self.portfolio_context=context;self.next_portfolio_ms=now+500;
        }
        Ok(())
    }

    pub fn validate_live_start(&self,status:&Status)->Result<()> {
        anyhow::ensure!(self.inner.settings.target=="live"&&status.gateway_fresh&&self.account_matches(status),"اربط الحساب الحقيقي من إدارة العقول أولاً");
        Ok(())
    }

    pub fn prepare_demo(&mut self,db:&Connection,status:&Status,now:u64)->Result<()> {
        self.inner.prepare_demo(db,status,now)?;
        self.portfolio.enabled=false;self.save_portfolio(db)?;
        self.portfolio_state["enabled"]=json!(false);
        self.next_portfolio_ms=0;self.inner.weights.clear();self.learning_context.clear();
        Ok(())
    }

    pub fn learn(&mut self,db:&Connection,t:&Telemetry,c:&Config,run:bool,frames:&BTreeMap<String,Vec<FrameBar>>,now:u64)->Result<()> {
        let available=crate::coordinator_learning::available_frames(t,frames,now);
        self.inner.weights=self.learner.observe(db,t,c,&self.inner.brains,&available,run,now)?;
        self.learning_context=format!("{}:{:?}",crate::coordinator_learning::context(t),c.trading.mode);
        crate::ablation::observe(db,t,c,run,self.inner.coordinator_evidence(),&available,now)?;
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
                let solo = id != "system" && brain["config"]["enabled"] == true;
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
        value["execution_model"] = json!("adaptive-member-coordinator-v1");
        value["note"] = json!("العقل الرئيسي يعمل مع START. العقول الثانوية تشارك كمستشارين افتراضياً؛ تفعيل SOLO يفصل العقل المختار فقط، وإطفاؤه يعيده بعد تسوية صفقة SOLO المفتوحة. التعلم يحدّث وزن العضو بعد 12 شمعة لاحقة؛ أول 20 عينة تستخدم وزناً محايداً. START/STOP منفصلان عن زر العضوية.");
        Ok(value)
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
        if method == "POST" && path == "/v1/execution/portfolio" {
            anyhow::ensure!(!status.requested_run, "أوقف النظام قبل تغيير حدود المحفظة");
            let candidate: PortfolioLimits = serde_json::from_slice(body)?;
            candidate.validate()?;
            self.portfolio = candidate;
            self.save_portfolio(db)?;
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
            let solo = config.enabled;
            let mut result = self.inner.route(method, path, body, db, status, now)?;
            result["start"] = json!(false);
            result["role"] = json!(if solo { "solo" } else { "advisor" });
            result["solo_enabled"] = json!(solo);
            result["note"] = json!(if solo {
                "تم تفعيل التداول المنفرد لهذا العقل. START العام يبقى منفصلاً."
            } else {
                "تم إلغاء التداول المنفرد؛ يعود العقل للمشاركة بعد تسوية أي صفقة أو أمر قائم باسمه."
            });
            return Ok(result);
        }

        let value = self.inner.route(method, path, body, db, status, now)?;
        if method == "GET" && path == "/v1/execution" {
            let mut value = Self::enrich_roles(value, db, status)?;
            value["portfolio"] = self.portfolio_state.clone();
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
}
