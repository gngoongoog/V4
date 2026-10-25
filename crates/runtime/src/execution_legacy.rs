//! Durable broker intents, independent brain ownership and broker-observed results.
use anyhow::{bail, Context, Result};
use braintrade_domain::{
    execution::{self, BrainConfig, Signal, BRAINS},
    studio::FrameBar,
    Config, Status, Telemetry,
    trading::TradingMode,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub target: String,
    pub login: u64,
    pub server: String,
    pub currency: String,
    pub bound_ms: u64,
    pub deviation_points: u32,
    pub revision: u64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            target: "demo".into(),
            login: 0,
            server: String::new(),
            currency: String::new(),
            bound_ms: 0,
            deviation_points: 20,
            revision: 0,
        }
    }
}
pub struct Service {
    pub sniper: braintrade_domain::sniper::Engine,
    /// Raw learner signal BEFORE coordinator participation/position/risk gates.
    /// Observation-only, for the matched future-Tick frozen policy study.
    pub last_sniper_raw: Option<Signal>,
    pub settings: Settings,
    pub brains: Vec<BrainConfig>,
    pub weights:BTreeMap<String,braintrade_domain::execution::LearnedWeight>,
    pub student_policies:BTreeMap<String,braintrade_domain::student::ExitStudy>,
    pub controls: BTreeMap<String, crate::brain_control::Control>,
    signals: BTreeMap<String, Value>,
    new_entries: Vec<Value>,
    last_sync_ms: u64,
    history_until_ms: u64,
    pub active_mode: Option<TradingMode>,
    last_paper_sample_ms: u64,
}
fn ctx(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap()
}
// Account identity deliberately excludes symbol: the recovery milestone
// must stay locked across XAUUSD and every other instrument after restart.
fn micro_account_key(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.currency)).unwrap()
}
// Daily lock isolates Demo/REAL as well as the account; recovery lock is separate.
fn daily_account_key(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.currency, t.account_trade_mode)).unwrap()
}
/// Durable, account/mode/symbol-scoped high-water mark. The DB row is only
/// advanced inside the same transaction that writes verified broker Deal
/// receipts, so a crash or a rejected receipt cannot claim unrecorded history.
fn broker_sync_watermark(db: &Connection, context: &str, target: &str) -> Result<(u64,u64)> {
    Ok(db.query_row(
        "SELECT until_ms,synced_ms FROM broker_history_sync WHERE context=?1 AND target=?2",
        params![context,target],|row|Ok((row.get(0)?,row.get(1)?))
    ).optional()?.unwrap_or((0,0)))
}

fn json_rows(db: &Connection, sql: &str, key: &str) -> Result<Vec<Value>> {
    let mut st = db.prepare(sql)?;
    let rows = st
        .query_map([key], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.iter().map(|r| Ok(serde_json::from_str(r)?)).collect()
}
/// Only monotonic, broker-history-proven split fills can supersede a partial
/// acknowledgement. This is a result journal update, NEVER a new send intent.
fn valid_recovered_partial_progress(previous: &Value, next: &Value) -> bool {
    if previous["state"] != "partial"
        || next["recovered_from_history"] != true
        || !matches!(next["state"].as_str(), Some("partial" | "filled"))
        || previous["request"] != next["request"]
    {
        return false;
    }
    let Some(requested) = next["request"]["volume"].as_f64() else { return false; };
    let Some(volume) = next["volume"].as_f64() else { return false; };
    let previous_volume = if previous["recovered_from_history"] == true {
        previous["volume"].as_f64().unwrap_or(f64::NAN)
    } else { 0.0 };
    if !requested.is_finite() || !volume.is_finite() || !previous_volume.is_finite()
        || requested <= 0.0 || volume <= 0.0 || previous_volume < 0.0
        || volume + 1e-8 < previous_volume
        || volume > requested + requested.mul_add(1e-7, 1e-8)
    { return false; }
    if next["state"] == "filled" && (volume - requested).abs() > requested.mul_add(1e-7, 1e-8) {
        return false;
    }
    if next["state"] == "partial" && volume + 1e-8 >= requested {
        return false;
    }
    let Some(deals) = next["deals"].as_array() else { return false; };
    if deals.is_empty() { return false; }
    let mut unique = BTreeSet::new();
    for deal in deals {
        let Some(ticket) = deal.as_u64().filter(|ticket| *ticket > 0) else { return false; };
        if !unique.insert(ticket) { return false; }
    }
    if previous["recovered_from_history"] == true {
        let Some(prior) = previous["deals"].as_array() else { return false; };
        if !prior.iter().all(|d| d.as_u64().is_some_and(|ticket| unique.contains(&ticket))) {
            return false;
        }
    }
    true
}

/// A broker ACK never certifies observation by itself. Match the exact
/// immutable MT5 Deal ticket to a physical position before releasing this
/// intent's per-brain reconciliation gate. A flat position list is not proof
/// of complete exit receipts.
fn broker_command_observed(
    db:&Connection, context:&str, t:&Telemetry, command:&Value, result:&Value
) -> Result<bool> {
    let Some(ticket)=result["deal"].as_u64().filter(|v|*v>0)
        else{return Ok(false)};
    let Some(magic)=command["magic"].as_u64() else{return Ok(false)};
    if command["symbol"]!=t.symbol {return Ok(false)}
    let stored:Option<(u64,String)>=db.query_row(
        "SELECT position_id,payload FROM broker_deals WHERE context=?1 AND ticket=?2",
        params![context,ticket],|r|Ok((r.get(0)?,r.get(1)?))
    ).optional()?;
    let Some((pid,raw))=stored else{return Ok(false)};
    if pid==0 {return Ok(false)}
    let deal:Value=serde_json::from_str(&raw)
        .context("Corrupt booked MT5 Deal receipt; cannot observe command")?;
    if deal["ticket"].as_u64()!=Some(ticket)
        || deal["position_id"].as_u64()!=Some(pid)
        || deal["symbol"]!=t.symbol
        || !matches!(deal["type"].as_i64(),Some(0|1))
        || result["position_id"].as_u64().is_some_and(|v|v!=pid)
    {return Ok(false)}
    let position=t.positions.iter().find(|p|
        (p.identifier==pid || p.identifier==0 && p.ticket==pid)
        && p.symbol==t.symbol && p.magic==magic);
    if command["kind"]=="open" {
        if deal["entry"]!=0 || deal["magic"].as_u64()!=Some(magic) {
            return Ok(false)
        }
        let created=command["created_ms"].as_u64().unwrap_or(0);
        let filled=deal["time_msc"].as_u64().unwrap_or(0);
        if created>0 && filled>0 && filled.saturating_add(60_000)<created {
            return Ok(false)
        }
        if position.is_some() {return Ok(true)}
    } else if command["kind"]=="close" {
        if !matches!(deal["entry"].as_i64(),Some(1|3)) {
            return Ok(false)
        }
        let Some(target)=command["position"].as_u64().filter(|v|*v>0)
            else{return Ok(false)};
        // MT5 position ticket can differ from immutable position identifier.
        // Match the current ticket or a previously observed, account-scoped
        // physical-position snapshot, never a different brain's close.
        let recorded:Option<String>=db.query_row(
            "SELECT payload FROM broker_observations WHERE context=?1 AND position_id=?2",
            params![context,pid],|r|r.get(0)
        ).optional()?;
        let historical_target=recorded
            .and_then(|s|serde_json::from_str::<Value>(&s).ok())
            .is_some_and(|p|p["ticket"].as_u64()==Some(target)
                && p["magic"].as_u64()==Some(magic) && p["symbol"]==t.symbol);
        let current_target=position.is_some_and(|p|p.ticket==target);
        if pid!=target && !current_target && !historical_target {
            return Ok(false)
        }
        if result["state"]=="partial" && current_target {
            let requested=result["request"]["volume"].as_f64()
                .filter(|v|v.is_finite()&&*v>0.);
            if let Some(requested)=requested {
                if position.is_some_and(|p|p.volume.is_finite()
                    && p.volume>0. && p.volume+1e-8<requested) {
                    return Ok(true)
                }
            }
        }
    } else {
        return Ok(false)
    }
    // An older ACK can coexist with a late partial exit. Only an entirely
    // covered physical position, absent from the fresh broker snapshot,
    // proves final observation without keeping a stale pending command.
    if t.positions.iter().any(|p|p.identifier==pid || p.ticket==pid) {
        return Ok(false)
    }
    let mut stmt=db.prepare(
        "SELECT payload FROM broker_deals WHERE context=?1 AND position_id=?2 ORDER BY ticket"
    )?;
    let raw=stmt.query_map(params![context,pid],|r|r.get::<_,String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let receipts=raw.iter().map(|s|serde_json::from_str::<Value>(s))
        .collect::<serde_json::Result<Vec<_>>>()?;
    Ok(crate::trade_journal::fully_closed_by_deals(&receipts))
}

impl Service {
    pub fn take_entries(&mut self) -> Vec<Value> { std::mem::take(&mut self.new_entries) }
    /// Entry execution is reserved to the coordinator of the selected trading
    /// mode. All other strategies remain analytical/shadow observers. Existing
    /// owned positions still receive their regular close/reconciliation logic.
    fn broker_entry_allowed(&self, cmd: &Value) -> bool {
        let Some(mode) = self.active_mode else { return false; };
        cmd["kind"] == "open"
            && cmd["brain"] == "system"
            && cmd["trading_mode"] == json!(mode)
    }
    pub fn set_mode(&mut self, mode: TradingMode) {
        self.active_mode = Some(mode);
        self.sniper.reset_quotes();
        self.last_sniper_raw=None;
        self.signals.clear();
    }

    /// Read-only identity guard for detached policy/Paper observations.
    pub fn matches_bound_telemetry(&self,t:&Telemetry)->bool{self.matches(t)}

    pub fn last_system_signal(&self)->Option<Signal>{
        serde_json::from_value(self.signals.get("system")?.get("signal")?.clone()).ok()
    }
    pub fn learning_decisions(&self)->Vec<(String,Value)> {
        self.brains.iter().filter(|b|b.enabled).filter_map(|b|self.signals.get(&b.id).map(|v|(b.id.clone(),v.clone()))).collect()
    }
    pub fn new(db: &Connection) -> Result<Self> {
        crate::shadow_paper::init(db)?;
        crate::brain_control::init(db)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS execution_settings(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS execution_equity(context TEXT PRIMARY KEY,peak REAL NOT NULL);
CREATE TABLE IF NOT EXISTS micro_real_recovered(account_key TEXT PRIMARY KEY,reached_ms INTEGER NOT NULL,reached_equity REAL NOT NULL);
CREATE TABLE IF NOT EXISTS daily_loss_locks(account_key TEXT NOT NULL,report_day TEXT NOT NULL, reached_ms INTEGER NOT NULL,loss_amount REAL NOT NULL, PRIMARY KEY(account_key,report_day));
CREATE TABLE IF NOT EXISTS brain_configs(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');
CREATE INDEX IF NOT EXISTS broker_commands_context ON broker_commands(context,brain,created_ms);
CREATE TABLE IF NOT EXISTS broker_seen(context TEXT NOT NULL,brain TEXT NOT NULL,signal TEXT NOT NULL,PRIMARY KEY(context,brain,signal));
CREATE TABLE IF NOT EXISTS broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,ticket));
CREATE TABLE IF NOT EXISTS broker_history_sync(context TEXT NOT NULL,target TEXT NOT NULL CHECK(target IN ('demo','live')),until_ms INTEGER NOT NULL CHECK(until_ms>=0),synced_ms INTEGER NOT NULL CHECK(synced_ms>=0),PRIMARY KEY(context,target));
CREATE TABLE IF NOT EXISTS broker_observations(context TEXT NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,opened_ms INTEGER NOT NULL,hold_ms INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,position_id));
CREATE TABLE IF NOT EXISTS broker_signal_log(id INTEGER PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,timestamp_ms INTEGER NOT NULL,payload TEXT NOT NULL);")?;
        let raw: Option<String> = db
            .query_row(
                "SELECT payload FROM execution_settings WHERE id=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let settings = raw
            .map(|r| serde_json::from_str(&r))
            .transpose()?
            .unwrap_or_default();
        let mut brains = vec![];
        for (id, _) in BRAINS {
            let r: Option<String> = db
                .query_row("SELECT payload FROM brain_configs WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            let b: BrainConfig = r
                .map(|s| serde_json::from_str(&s))
                .transpose()?
                .unwrap_or_else(|| BrainConfig {
                    id: id.into(),
                    ..Default::default()
                });
            b.validate().map_err(anyhow::Error::msg)?;
            brains.push(b);
        }
        Ok(Self {
            sniper: Default::default(),
            last_sniper_raw:None,
            settings,
            brains,
            signals: BTreeMap::new(),
            controls: BTreeMap::new(),
            new_entries: Vec::new(),
            weights:BTreeMap::new(),
            student_policies:BTreeMap::new(),
            last_sync_ms: 0,
            history_until_ms: 0,
            active_mode: None,
            last_paper_sample_ms: 0,
        })
    }
    fn micro_real_active(&self, db: &Connection, t: &Telemetry, c: &Config) -> Result<bool> {
        if !c.risk.micro_real_temporary || self.settings.target != "live"
            || t.account_trade_mode != Some(2) || !t.equity.is_finite()
            || t.equity <= 0.
        { return Ok(false); }
        if t.equity >= c.risk.micro_real_recovery_equity {
            // One-way, committed recovery latch persists through restarts.
            db.execute("INSERT OR IGNORE INTO micro_real_recovered VALUES(?1,?2,?3)",
                params![micro_account_key(t),crate::now_ms(),t.equity])?;
            return Ok(false);
        }
        let locked: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM micro_real_recovered WHERE account_key=?1)",
            [micro_account_key(t)], |row| row.get(0)
        )?;
        Ok(!locked)
    }
    fn daily_breaker_blocked(
        &self, db: &Connection, t: &Telemetry, now: u64, enabled: bool, limit: f64
    ) -> Result<bool> {
        if !enabled { return Ok(false); }
        if !limit.is_finite() || !(0.1..=10.).contains(&limit) {
            return Ok(true);
        }
        let stamp=chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now as i64);
        let Some(stamp)=stamp else { return Ok(true); };
        let baghdad=chrono::FixedOffset::east_opt(3*3600).unwrap();
        let day=stamp.with_timezone(&baghdad).format("%Y-%m-%d").to_string();
        // Never trust yesterday's successful snapshot for today's entry.
        if t.report_day!=day { return Ok(true); }
        let locked:bool=db.query_row(
            "SELECT EXISTS(SELECT 1 FROM daily_loss_locks WHERE account_key=?1 AND report_day=?2)",
            params![daily_account_key(t),day],|r|r.get(0)
        )?;
        if locked { return Ok(true); }
        let Some(net)=t.today_net else { return Ok(true); };
        // MT5 account-wide balance/equity includes all positions; subtract
        // today's settled trading PnL to estimate a pre-day trading baseline.
        // Any unknown/invalid broker evidence blocks, never fabricates PnL.
        let baseline=t.balance-net;
        if !net.is_finite() || !baseline.is_finite() || baseline<=0.
            || !t.equity.is_finite() || t.equity<=0. { return Ok(true); }
        let loss=baseline-t.equity;
        if loss>=baseline*limit/100. {
            db.execute("INSERT OR IGNORE INTO daily_loss_locks VALUES(?1,?2,?3,?4)",
                params![daily_account_key(t),day,now,loss])?;
            return Ok(true);
        }
        Ok(false)
    }
    fn breaker_enabled(&self, c:&Config) -> bool {
        match self.settings.target.as_str() {
            "live" => c.risk.daily_breaker_real_enabled,
            "demo" => c.risk.daily_breaker_demo_enabled,
            _ => true,
        }
    }
    fn matches(&self, t: &Telemetry) -> bool {
        t.connected
            && t.login == self.settings.login
            && t.server == self.settings.server
            && t.currency == self.settings.currency
            && t.account_trade_mode == Some(if self.settings.target == "demo" { 0 } else { 2 })
    }
    pub fn enabled(&self, s: &Status) -> bool {
        s.requested_run
            && s.gateway_fresh
            && self.brains.iter().any(|b| b.enabled)
            && s.telemetry.as_ref().is_some_and(|t| self.matches(t))
    }
    fn save_candidate(db: &Connection, candidate: &Settings) -> Result<()> {
        db.execute(
            "INSERT OR REPLACE INTO execution_settings VALUES(1,?1)",
            [serde_json::to_string(candidate)?],
        )?;
        Ok(())
    }
    fn bind_with_settings(&mut self, db: &Connection, t: &Telemetry, now: u64, mut candidate: Settings) -> Result<()> {
        if !t.connected
            || t.account_trade_mode != Some(if candidate.target == "demo" { 0 } else { 2 })
        {
            bail!("نوع الحساب لا يطابق هدف التنفيذ المختار؛ Demo يحتاج حساب MT5 تجريبي");
        }
        candidate.login = t.login;
        candidate.server = t.server.clone();
        candidate.currency = t.currency.clone();
        candidate.bound_ms = now;
        // Never activate or advertise an account/target whose durable
        // execution_settings write failed. START and final authorization
        // must always agree with settings restored after restart.
        Self::save_candidate(db, &candidate)?;
        self.settings = candidate;
        self.history_until_ms = now;
        self.last_sync_ms = 0;
        Ok(())
    }
    fn bind(&mut self, db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
        self.bind_with_settings(db, t, now, self.settings.clone())
    }
    pub fn pending(&self, db: &Connection) -> Result<bool> {
        Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE state IN ('queued','dispatched','unknown','placed','filled','partial'))",[],|r|r.get(0))?)
    }
    pub fn cancel_entries(&self, db: &Connection) -> Result<()> {
        db.execute(
            "UPDATE broker_commands SET state='cancelled' WHERE kind='open' AND state='queued'",
            [],
        )?;
        Ok(())
    }
    fn queue(
        &self,
        db: &Connection,
        t: &Telemetry,
        b: &BrainConfig,
        kind: &str,
        signal: Option<&Signal>,
        position: u64,
        now: u64,
        config: Option<&Config>,
    ) -> Result<Value> {
        let key = ctx(t);
        let id = uuid::Uuid::new_v4().simple().to_string();
        let s = signal
            .map(serde_json::to_value)
            .transpose()?
            .unwrap_or(Value::Null);
        let mut p = json!({"id":id,"revision":self.settings.revision,"context":key,"brain":b.id,"kind":kind,"login":t.login,"server":t.server,"currency":t.currency,"target":self.settings.target,"symbol":t.symbol,"magic":execution::magic(&b.id),"position":position,"lots":b.lots,"deviation_points":self.settings.deviation_points,"created_ms":now,"max_quote_age_ms":signal.map(|s|if s.key.starts_with("tick:"){1000}else{3000}).unwrap_or(3000),"expires_ms":if kind=="open"{now+3000}else{now+30_000},"signal":s});
        if let Some(c) = config {
            p["trading_mode"] = json!(c.trading.mode);
            p["demo_flexible"] = json!(c.trading.demo_flexible && t.demo && t.account_trade_mode==Some(0));
            p["commission_per_lot"] = json!(c.risk.commission_per_lot);
            p["daily_breaker_enabled"] = json!(self.breaker_enabled(c));
            p["daily_breaker_limit_percent"] = json!(c.risk.daily_loss_percent);
            // Mandatory for every new entry (Demo and REAL). The broker uses
            // order_calc_profit at the EXACT final stop and account equity to
            // cap the requested volume without ever increasing it.
            let micro_real = self.micro_real_active(db, t, c)?;
            p["risk_percent"] = json!(if micro_real { c.risk.micro_real_cap_percent } else { c.risk.risk_percent });
            p["micro_real_temporary"] = json!(micro_real);
            p["micro_real_recovery_equity"] = json!(c.risk.micro_real_recovery_equity);
            p["micro_demo_risk_opt_in"] = json!(c.risk.micro_demo_risk_opt_in);
            p["slippage_ticks"] = json!(c.risk.slippage_ticks);
            p["learning"] = json!({"trading_mode":c.trading.mode,"timeframe":b.timeframe,"report_id":b.report_id,
                "tick_size":t.tick_size,"tick_value_loss":t.tick_value_loss,"tick_value_profit":t.tick_value_profit,
                "slippage_ticks":c.risk.slippage_ticks,"bid":t.bid,"ask":t.ask,"quote_time_ms":t.quote_time_ms,"quote_age_ms":t.tick_age_ms,
                "context_key":self.weights.get(&b.id).map(|w|w.context_key.clone()).unwrap_or_default(),
                "weight_at_entry":self.weights.get(&b.id).map(|w|w.weight()).unwrap_or(1.),
                "intelligence":self.weights.get(&b.id).map(|w|w.intelligence()).unwrap_or_default()});
        }
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload) VALUES(?1,?2,?3,?4,?5,'queued',?6)",params![id,key,b.id,kind,now,p.to_string()])?;
        Ok(p)
    }
    pub fn observe(
        &mut self,
        db: &Connection,
        t: &Telemetry,
        c: &Config,
        run: bool,
        frames: &BTreeMap<String, Vec<FrameBar>>,
        now: u64,
    ) -> Result<()> {
        if !self.matches(t) {
            self.sniper.reset_quotes();
            self.signals.clear();
            return Ok(());
        }
        let key = ctx(t);
        // Checking eligibility also durably records the milestone once it is reached.
        let micro_real = self.micro_real_active(db, t, c)?;
        let daily_breaker_blocked=self.daily_breaker_blocked(
            db,t,now,self.breaker_enabled(c),c.risk.daily_loss_percent)?;
        db.execute("INSERT INTO execution_equity VALUES(?1,?2) ON CONFLICT(context) DO UPDATE SET peak=MAX(peak,excluded.peak)",params![key,t.equity])?;
        let peak:f64=db.query_row("SELECT peak FROM execution_equity WHERE context=?1",[&key],|r|r.get(0))?;
        // A fill acknowledgement is not a reconciled position snapshot. Keep the
        // brain blocked until the broker exposes its position or its deal history.
        let acknowledged=json_rows(db,"SELECT json_object('request',json(payload),'result',json(result)) FROM broker_commands WHERE context=?1 AND state IN ('filled','partial')",&key)?;
        for item in acknowledged {
            let q = &item["request"];
            let r = &item["result"];
            if broker_command_observed(db,&key,t,q,r)? {
                db.execute(
                    "UPDATE broker_commands SET state='observed' WHERE id=?1",
                    [q["id"].as_str().unwrap()],
                )?;
            }
        }
        self.active_mode = Some(c.trading.mode);
        let paper_due = now.saturating_sub(self.last_paper_sample_ms) >= 1_000;
        if paper_due {self.last_paper_sample_ms=now;}
        let mut activity_samples=Vec::new();
        let sniper_signal = self.sniper.evaluate(t,c,now);
        self.last_sniper_raw=Some(sniper_signal.clone());
        let available=crate::coordinator_learning::available_frames(t,frames,now);
        for b in self.brains.clone() {
            let magic = execution::magic(&b.id).unwrap();
            let active:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain=?2 AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",params![key,b.id],|r|r.get(0))?;
            let positions: Vec<_> = t
                .positions
                .iter()
                .filter(|p| p.symbol == t.symbol && p.magic == magic)
                .collect();
            for p in &positions {
                let pid = if p.identifier > 0 {
                    p.identifier
                } else {
                    p.ticket
                };
                let raw:Option<String>=db.query_row("SELECT payload FROM broker_commands WHERE context=?1 AND brain=?2 AND kind='open' ORDER BY created_ms DESC LIMIT 1",params![key,b.id],|r|r.get(0)).optional()?;
                let hold = raw
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .and_then(|v| v["signal"]["hold_ms"].as_u64())
                    .unwrap_or(b.max_hold_seconds * 1000);
                db.execute(
                    "INSERT OR IGNORE INTO broker_observations VALUES(?1,?2,?3,?4,?5,?6)",
                    params![key, pid, b.id, p.opened_ms, hold, serde_json::to_string(p)?],
                )?;
                let frozen: u64 = db.query_row(
                    "SELECT hold_ms FROM broker_observations WHERE context=?1 AND position_id=?2",
                    params![key, pid],
                    |r| r.get(0),
                )?;
                let last_close:u64=db.query_row("SELECT COALESCE(MAX(created_ms),0) FROM broker_commands WHERE context=?1 AND brain=?2 AND kind='close'",params![key,b.id],|r|r.get(0))?;
                if !active
                    && frozen > 0
                    && p.opened_ms > 0
                    && now.saturating_sub(p.opened_ms) >= frozen
                    && now.saturating_sub(last_close) > 5000
                {
                    self.queue(db, t, &b, "close", None, p.ticket, now, None)?;
                }
            }
            // Do not skip other brains: evaluate them as observation-only advisors.
            // Their ready signals can feed paper evidence but never MT5 dispatch.
            let genome=crate::coordinator_learning::genome(db,&b,t)?;
            let started = std::time::Instant::now();
            let base=if b.id==braintrade_domain::sniper::ID || (b.id=="system" && braintrade_domain::sniper::selected(c)) {sniper_signal.clone()}
                else {execution::evaluate(&b,t,c,&available,genome.as_ref(),now)};
            let mut signal=if b.id=="system" && braintrade_domain::sniper::selected(c) {
                let member=self.brains.iter().find(|m|m.id==braintrade_domain::sniper::ID).unwrap();
                let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain='sniper_learner' AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",[&key],|r|r.get(0))?;
                let occupied=t.positions.iter().any(|p|p.magic==410_617 && p.symbol==t.symbol);
                let mut selected=sniper_signal.clone(); selected.brain="system".into();
                if !self.controls.get(braintrade_domain::sniper::ID).is_none_or(|c|c.advisor) || member.enabled || pending || occupied {
                    selected.ready=false;selected.reason="مشاركة القناص موقوفة أو توجد صفقة قديمة تنتظر التسوية".into();
                }
                selected.coordination.get_or_insert_with(||json!({}))["intelligence"]=json!(self.weights.get(braintrade_domain::sniper::ID).map(|w|w.intelligence()).unwrap_or_default());
                selected
            } else if b.id=="system" {
                let mut votes=vec![];
                for member in self.brains.iter().filter(|m|m.id!="system" && self.controls.get(&m.id).is_none_or(|c|c.advisor)) {
                    // Returning SOLO positions finish under their original owner before rejoining.
                    let in_flight:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain=?2 AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",params![key,member.id],|r|r.get(0))?;
                    if in_flight||t.positions.iter().any(|p|p.magic==execution::magic(&member.id).unwrap()&&p.symbol==t.symbol){continue;}
                    let g=crate::coordinator_learning::genome(db,member,t)?;
                    let vote=execution::evaluate(member,t,c,&available,g.as_ref(),now);
                    votes.push((member.clone(),vote,self.weights.get(&member.id).cloned().unwrap_or_default()));
                }
                execution::coordinate_for_account(base,c,t,votes)
            }else{base};
            if b.id!="system" {
                signal.coordination.get_or_insert_with(||json!({}))["intelligence"]=json!(self.weights.get(&b.id).map(|w|w.intelligence()).unwrap_or_default());
            }
            crate::student_learning::adapt(&mut signal,t,c,&b,&self.student_policies);
            // Independent price-sampled paper practice for every non-selected
            // brain. This never queues an MT5 order or enters broker PnL.
            if paper_due {
                activity_samples.push((b.id.clone(),signal.clone()));
                let mut paper_signal=signal.clone();
                if !self.controls.get(&b.id).is_none_or(|control|control.paper) {paper_signal.ready=false;}
                // Pausing new paper entries must still settle previously opened practice.
                crate::shadow_paper::observe(db,&key,&b,&paper_signal,t,c,now)?;
                // Frozen trials are isolated quote-observed paper evidence, not
                // orders and not input to the adaptive coordinator learner.
                if b.id!="system"{
                    // Shadow failures must NEVER abort the actual broker execution
                    // coordinator or prevent closing positions. Record for audit.
                    if let Err(error)=crate::frozen_shadow::observe(db,&key,c.trading.mode,&b.id,&signal,t,now,&available){
                        let _=db.execute(
                            "INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,?2,?3)",
                            params![now,"frozen_shadow_observe_error",
                                json!({"context":key,"brain":b.id,
                                    "error":error.to_string().chars().take(240).collect::<String>()}).to_string()]
                        );
                    }
                }
            }
            let decision_us = started.elapsed().as_micros() as u64;
            let mut gates = vec![];
            if t.margin_mode!=Some(2)&&db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain<>?2 AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",params![key,b.id],|r|r.get::<_,bool>(0))?{gates.push("Netting: أمر لعقل آخر لم تتم تسويته");}

            if !run {
                gates.push("النظام متوقف");
            }
            if daily_breaker_blocked {
                gates.push("قاطع الخسارة اليومي للحساب: الحد بلغ أو بيانات اليوم غير موثوقة");
            }
            // Elevated risk is deliberately Demo-only. Reject in Core before
            // queueing to avoid a stream of guaranteed broker rejections on REAL.
            // The MT5 adapter independently enforces the exact broker mode.
            if c.risk.risk_percent > 2.0
                && (self.settings.target != "demo" || t.account_trade_mode != Some(0))
            {
                gates.push("المخاطرة فوق 2% تجريبية على حساب MT5 Demo فقط؛ REAL يبقى بحد 2%");
            }
            if b.id != "system" {
                gates.push("SHADOW / PAPER فقط: الوضع المختار يمنع هذا العقل من إرسال صفقات إلى MT5");
            }
            if active {
                gates.push("أمر قيد التنفيذ أو المصالحة");
            }
            if !positions.is_empty() {
                gates.push("للعقل صفقة مفتوحة");
            }
            if !t.trade_allowed || !t.trade_expert {
                gates.push("التداول الآلي غير مسموح في MT5");
            }
            if t.margin_mode != Some(2) && t.positions.iter().any(|p| p.symbol == t.symbol) {
                gates.push("حساب Netting: الرمز مملوك لصفقة حالية");
            }
            let last:u64=db.query_row("SELECT COALESCE(MAX(created_ms),0) FROM broker_commands WHERE context=?1 AND brain=?2 AND kind='open'",params![key,b.id],|r|r.get(0))?;
            let cooldown = if matches!(b.id.as_str(),"system"|"sniper_learner")
                && c.trading.mode == braintrade_domain::trading::TradingMode::Sniper
            {
                c.trading.sniper_cooldown_seconds.max(b.cooldown_seconds)
            } else {
                b.cooldown_seconds
            };
            if now.saturating_sub(last) < cooldown * 1000 {
                gates.push("فاصل الدخول المختار");
            }
            if b.use_risk_limits {
                if peak>0. && (peak-t.equity)/peak*100. >= c.risk.max_drawdown_percent {
                    gates.push("حد التراجع الاختياري من أعلى Equity مرصود لهذا الحساب والرمز");
                }
                if b.lots > c.risk.max_lots {
                    gates.push("حد اللوت الاختياري");
                }
                let loss = signal.stop_distance / t.tick_size.unwrap_or(0.)
                    * t.tick_value_loss.unwrap_or(0.)
                    * b.lots
                    + braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot,b.lots).map_err(anyhow::Error::msg)?;
                let active_risk_percent = if micro_real {c.risk.micro_real_cap_percent} else {c.risk.risk_percent};
                if loss > t.equity * active_risk_percent / 100. {
                    gates.push("ميزانية المخاطرة الاختيارية");
                }
                // Daily loss now has one explicit account-wide ON/OFF switch
                // outside optional per-brain limits. Do not double-gate Demo.
                // The configurable ratio is spread / ATR, not spread / stop.
                // Sniper uses a fixed-point stop, so dividing that stop by the
                // brain's stop_atr fabricated ATR and rejected valid setups.
                let atr_for_spread = if c.trading.mode == braintrade_domain::trading::TradingMode::Sniper {
                    t.market.as_ref().map(|m| braintrade_domain::trading::volatility(&m.m1))
                } else {
                    Some(signal.stop_distance / b.stop_atr)
                };
                match (t.bid.zip(t.ask), atr_for_spread.filter(|v| v.is_finite() && *v > 0.)) {
                    (Some((bid, ask)), Some(atr)) if (ask-bid) > atr * c.risk.max_spread_atr => {
                        gates.push("حد السبريد الاختياري: سبريد MT5 تجاوز نسبته من ATR الحقيقي");
                    }
                    (_, None) if c.trading.mode == braintrade_domain::trading::TradingMode::Sniper && signal.ready => {
                        gates.push("حد السبريد الاختياري يحتاج شموع M1 لحساب ATR الحقيقي");
                    }
                    _ => {}
                }
            }
            let seen:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_seen WHERE context=?1 AND brain=?2 AND signal=?3)",params![key,b.id,signal.key],|r|r.get(0))?;
            if seen {
                gates.push("تمت معالجة هذه الإشارة");
            }
            let can = signal.ready && gates.is_empty();
            let result = json!({"signal":signal,"gates":gates,"ready":can,"decision_us":decision_us,"evaluated_ms":now});
            self.signals.insert(b.id.clone(), result.clone());
            if can && b.id == "system" {
                let tx = db.unchecked_transaction()?;
                tx.execute(
                    "INSERT OR IGNORE INTO broker_seen VALUES(?1,?2,?3)",
                    params![key, b.id, signal.key],
                )?;
                let command = self.queue(&tx, t, &b, "open", Some(&signal), 0, now, Some(c))?;
                tx.execute("INSERT INTO broker_signal_log(context,brain,timestamp_ms,payload) VALUES(?1,?2,?3,?4)",params![key,b.id,now,result.to_string()])?;
                tx.commit()?;
                self.new_entries.push(command);
            }
        }
        // One durable commit per sampled batch, never one fsync per brain/tick.
        if !activity_samples.is_empty() {
            let tx=db.unchecked_transaction()?;
            for (id,signal) in activity_samples {crate::brain_control::sample(&tx,&key,c.trading.mode,&id,&signal,now)?;}
            tx.commit()?;
        }
        Ok(())
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
        let t = status
            .telemetry
            .as_ref()
            .filter(|t| status.gateway_fresh && t.connected);
        let key = t.map(ctx).unwrap_or_default();
        match (method, path) {
            ("GET", "/v1/execution") => {
                let deals = json_rows(
                    db,
                    "SELECT payload FROM broker_deals WHERE context=?1 ORDER BY ticket DESC",
                    &key,
                )?;
                let positions = t.map(|t| t.positions.clone()).unwrap_or_default();
                let mut brains = vec![];
                for b in &self.brains {
                    let magic = execution::magic(&b.id).unwrap();
                    let own: Vec<_> = deals.iter().filter(|d| d["brain"] == b.id).collect();
                    // Same strict cost provenance as /v1/execution/trades.
                    // A corrupt historical broker fee must not become an
                    // invented $0.00 and falsely improve a brain's win count.
                    let net = crate::trade_journal::booked_costs(own.iter().copied())
                        .map(|totals| totals[0]);
                    let mut receipts: BTreeMap<u64, Vec<&Value>> = BTreeMap::new();
                    for d in &own {
                        receipts.entry(d["position_id"].as_u64().unwrap_or(0))
                            .or_default().push(*d);
                    }
                    let groups: BTreeMap<u64, Option<f64>> = receipts.iter().map(|(pid, deals)| {
                        (*pid, crate::trade_journal::booked_costs(deals.iter().copied())
                            .map(|totals| totals[0]))
                    }).collect();
                    let closed: Vec<_> = groups
                        .iter()
                        .filter(|(pid, _)| {
                            !positions.iter().any(|p| p.identifier == **pid || p.ticket == **pid)
                                && receipts.get(*pid).is_some_and(|group|
                                    crate::trade_journal::fully_closed_by_deals(group.iter().copied()))

                        })
                        .collect();
                    brains.push(json!({"config":b,"name":BRAINS.iter().find(|v|v.0==b.id).unwrap().1,"magic":magic,"latest":self.signals.get(&b.id).filter(|v|t.is_some_and(|t|self.matches(t))&&now.saturating_sub(v["evaluated_ms"].as_u64().unwrap_or(0))<=3000),"learning":self.weights.get(&b.id).filter(|_|t.is_some_and(|t|self.matches(t))),"learned_weight":self.weights.get(&b.id).filter(|_|t.is_some_and(|t|self.matches(t))).map(|w|w.weight()),"stats":{"deals":own.len(),"closed_positions":closed.len(),"wins":closed.iter().filter(|(_,p)|p.is_some_and(|amount| amount>0.)).count(),"net_booked":net,"booked_cost_evidence":if own.is_empty(){"NO_BROKER_DEALS"}else if net.is_some(){"BROKER_COMPONENTS_VERIFIED"}else{"MISSING_OR_INVALID_BROKER_COST"},"floating":positions.iter().filter(|p|p.magic==magic).map(|p|p.profit).sum::<f64>(),"open_positions":positions.iter().filter(|p|p.magic==magic).count()}}));
                }
                let commands=json_rows(db,"SELECT json_object('request',json(payload),'state',state,'result',json(result)) FROM broker_commands WHERE context=?1 ORDER BY created_ms DESC LIMIT 100",&key)?;
                let (history_until,last_sync)=if t.is_some_and(|t|self.matches(t)) {
                    broker_sync_watermark(db,&key,&self.settings.target)?
                }else{(0,0)};
                Ok(
                    json!({"settings":self.settings,"armed":self.enabled(status),"account_matches":t.is_some_and(|t|self.matches(t)),"context":key,"brains":brains,"commands":commands,"last_broker_sync_ms":last_sync,"history_until_ms":history_until,"note":"أرباح محققة من سجل الوسيط؛ العمولات المنفصلة التي لا تحمل position_id لا تنسب تلقائياً لعقل. STOP يمنع دخولاً جديداً، وإدارة خروج الصفقات المملوكة تستمر."}),
                )
            }
            ("POST", "/v1/execution/settings") => {
                let v: Value = serde_json::from_slice(body)?;
                let target = v["target"].as_str().context("Target required")?;
                if !matches!(target, "demo" | "live") {
                    bail!("Choose demo or live");
                }
                if status.requested_run || self.pending(db)? {
                    bail!("أوقف النظام وانتظر تسوية الأوامر قبل تغيير الحساب");
                }
                let t = t.context("Connect MT5 before binding execution")?;
                if t.positions
                    .iter()
                    .any(|p| execution::brain_for_magic(p.magic).is_some())
                {
                    bail!("أغلق صفقات العقول قبل تغيير حساب التنفيذ");
                }
                let mut candidate = self.settings.clone();
                candidate.target = target.into();
                candidate.deviation_points = v["deviation_points"]
                    .as_u64()
                    .filter(|v| *v <= 100_000)
                    .context("Deviation 0..100000 required")?
                    as u32;
                if t.account_trade_mode != Some(if target == "demo" { 0 } else { 2 }) {
                    bail!("نوع حساب MT5 لا يطابق Demo/Live المختار");
                }
                // A live selection by itself must NEVER bind a real-money
                // account. The operator must enter the exact observed MT5
                // login in an explicit non-persisted confirmation field.
                if target == "live" {
                    anyhow::ensure!(
                        status.gateway_fresh && !t.demo && t.login>0
                          && v["confirm_live_login"].as_str()==Some(t.login.to_string().as_str()),
                        "تأكيد حساب REAL مطلوب: اكتب رقم الدخول الحقيقي المعروض في MT5 بدقة ثم اربط الحساب"
                    );
                }
                self.bind_with_settings(db, t, now, candidate)?;
                Ok(json!({"ok":true,"settings":self.settings}))
            }
            ("POST", "/v1/execution/brain") => {
                let b: BrainConfig = serde_json::from_slice(body)?;
                b.validate().map_err(anyhow::Error::msg)?;
                if b.enabled {
                    let t = t.context("Connect MT5 first")?;
                    if self.settings.login == 0 {
                        self.bind(db, t, now)?;
                    }
                    if !self.matches(t) {
                        bail!("الحساب الحالي لا يطابق الحساب المرتبط؛ حدده في صفحة التنفيذ");
                    }
                    if b.id == "generated" {
                        let raw:Option<String>=db.query_row("SELECT payload FROM studio_runs WHERE id=?1 AND context=?2 AND timeframe=?3",params![b.report_id,key,b.timeframe],|r|r.get(0)).optional()?;
                        if raw.is_none() {
                            bail!("اختر تقريراً محفوظاً لنفس الحساب والرمز والفريم");
                        }
                    }
                }
                // Invalidate queued decisions on changed membership/parameters. Dispatched
                // commands remain reconcilable, but must pass this revision at final authorization.
                let tx=db.unchecked_transaction()?;
                tx.execute("INSERT OR REPLACE INTO brain_configs VALUES(?1,?2)",params![b.id,serde_json::to_string(&b)?])?;
                let mut settings=self.settings.clone();settings.revision+=1;
                tx.execute("INSERT OR REPLACE INTO execution_settings VALUES(1,?1)",[serde_json::to_string(&settings)?])?;
                tx.execute("UPDATE broker_commands SET state='cancelled' WHERE kind='open' AND state='queued'",[])?;
                tx.commit()?;
                self.settings=settings;
                self.signals.clear();
                let enabled=b.enabled;
                let index = self.brains.iter().position(|x| x.id == b.id).unwrap();
                self.brains[index] = b;
                Ok(json!({"ok":true,"start":enabled}))
            }
            ("POST", "/v1/execution/close") => {
                let v: Value = serde_json::from_slice(body)?;
                let id = v["brain"].as_str().context("Brain required")?;
                let b = self
                    .brains
                    .iter()
                    .find(|b| b.id == id)
                    .context("Unknown brain")?;
                let t = t
                    .filter(|t| self.matches(t))
                    .context("Bound account required")?;
                if db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain=?2 AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",params![key,id],|r|r.get::<_,bool>(0))?{bail!("انتظر نتيجة الأمر الحالي أو المصالحة");}
                for p in t
                    .positions
                    .iter()
                    .filter(|p| p.magic == execution::magic(id).unwrap() && p.symbol == t.symbol)
                {
                    self.queue(db, t, b, "close", None, p.ticket, now, None)?;
                }
                Ok(json!({"ok":true}))
            }
            ("GET", "/v1/execution/gateway") => {
                let Some(t) = t.filter(|t| self.matches(t)) else {
                    return Ok(json!({"command":null,"history_from_ms":0}));
                };
                db.execute("UPDATE broker_commands SET state='expired' WHERE state='queued' AND CAST(json_extract(payload,'$.expires_ms') AS INTEGER)<?1",[now])?;
                let commands=json_rows(db,"SELECT json_set(payload,'$.state',state) FROM broker_commands WHERE context=?1 AND state IN ('queued','dispatched','unknown','placed') ORDER BY CASE WHEN state IN ('queued','dispatched') THEN 0 ELSE 1 END,CASE kind WHEN 'close' THEN 0 ELSE 1 END,created_ms LIMIT 1",&key)?;
                let mut command = commands.first().cloned();
                if let Some(cmd) = &command {
                    let id = cmd["id"].as_str().unwrap();
                    if cmd["state"] == "queued" {
                        let breaker_ok=cmd["kind"] == "close"
                            || !self.daily_breaker_blocked(
                                db,t,now,
                                cmd["daily_breaker_enabled"].as_bool().unwrap_or(self.settings.target=="live"),
                                cmd["daily_breaker_limit_percent"].as_f64().unwrap_or(2.))?;
                        let allowed = cmd["kind"] == "close"
                            || (breaker_ok && self.enabled(status)
                                && self.broker_entry_allowed(cmd));
                        if allowed {
                            db.execute("UPDATE broker_commands SET state='dispatched' WHERE id=?1 AND state='queued'",[id])?;
                        } else {
                            db.execute("UPDATE broker_commands SET state='cancelled' WHERE id=?1 AND state='queued'",[id])?;
                            command = None;
                        }
                    }
                }
                // Crash-safe cursor recovery: never resume from a different
                // symbol/account/target's in-memory watermark.
                let (persisted_until,_)=broker_sync_watermark(db,&key,&self.settings.target)?;
                Ok(
                    json!({"command":command,"history_from_ms":persisted_until.max(self.settings.bound_ms).saturating_sub(120_000),"account":{"login":t.login,"server":t.server,"currency":t.currency,"trade_mode":t.account_trade_mode},"poll_ms":if self.brains.iter().any(|b|b.enabled){100}else{250}}),
                )
            }
            ("POST", "/v1/execution/authorize") => {
                let v: Value = serde_json::from_slice(body)?;
                let id = v["id"].as_str().context("ID required")?;
                let (raw, state): (String, String) = db.query_row(
                    "SELECT payload,state FROM broker_commands WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let cmd: Value = serde_json::from_str(&raw)?;
                let breaker_ok=if cmd["kind"]=="close" {true} else if let Some(t)=t {
                    !self.daily_breaker_blocked(db,t,now,
                        cmd["daily_breaker_enabled"].as_bool().unwrap_or(self.settings.target=="live"),
                        cmd["daily_breaker_limit_percent"].as_f64().unwrap_or(2.))?
                } else { false };
                let ok = state == "dispatched"
                    && breaker_ok
                    && t.is_some_and(|t| self.matches(t) && ctx(t) == cmd["context"])
                    && now <= cmd["expires_ms"].as_u64().unwrap_or(0)
                    && (cmd["kind"] == "close"
                        || (cmd["revision"].as_u64()==Some(self.settings.revision)
                            && self.enabled(status)
                            && self.broker_entry_allowed(&cmd)));
                Ok(json!({"authorized":ok}))
            }
            ("POST", "/v1/execution/result") => {
                let v: Value = serde_json::from_slice(body)?;
                let id = v["id"].as_str().context("ID required")?;
                let state = v["state"].as_str().context("State required")?;
                if !matches!(
                    state,
                    "filled"
                        | "partial"
                        | "rejected"
                        | "unknown"
                        | "placed"
                        | "closed"
                        // 'observed' is reserved for Core's exact Deal and
                        // physical-position reconciler; never accept it from
                        // a Gateway /result packet, even after restart.
                        | "cancelled"
                ) {
                    bail!("Invalid broker state");
                }
                let (old, previous_json): (String, String) = db.query_row(
                    "SELECT state,result FROM broker_commands WHERE id=?1", [id],
                    |r| Ok((r.get(0)?,r.get(1)?)),
                )?;
                if matches!(old.as_str(), "dispatched" | "unknown" | "placed") {
                    db.execute(
                        "UPDATE broker_commands SET state=?2,result=?3 WHERE id=?1",
                        params![id, state, v.to_string()],
                    )?;
                } else if matches!(old.as_str(), "partial" | "observed") {
                    let previous: Value = serde_json::from_str(&previous_json)?;
                    if valid_recovered_partial_progress(&previous, &v) {
                        if old == "observed" {
                            // Position stays observed; only broker-verified fill
                            // evidence advances for cost and learning attribution.
                            db.execute("UPDATE broker_commands SET result=?2 WHERE id=?1",
                                params![id,v.to_string()])?;
                        } else {
                            db.execute(
                                "UPDATE broker_commands SET state=?2,result=?3 WHERE id=?1",
                                params![id,state,v.to_string()],
                            )?;
                        }
                    }
                }
                Ok(json!({"ok":true}))
            }
            ("POST", "/v1/execution/sync") => {
                let v: Value = serde_json::from_slice(body)?;
                let t = t
                    .filter(|t| self.matches(t))
                    .context("Bound account unavailable")?;
                if v["login"] != t.login || v["server"] != t.server || v["currency"] != t.currency
                    || t.account_trade_mode.is_none() || v["trade_mode"] != json!(t.account_trade_mode) {
                    bail!("Broker history account or Demo/REAL mode mismatch");
                }
                let ds = v["deals"].as_array().context("Deals required")?;
                if ds.len() > 100_000 {
                    bail!("History chunk too large");
                }
                let until_ms=v["until_ms"].as_u64()
                    .filter(|boundary| *boundary <= now + 2_000)
                    .context("Invalid history boundary")?;
                // Verify every booked broker cost BEFORE committing any deals.
                // The learner must never consume a 0 invented for a missing fee.
                for d in ds {
                    // The broker history cursor is a completeness claim.
                    // Refuse selected-symbol trading receipts whose mandatory
                    // execution geometry was omitted or malformed. Otherwise
                    // the cursor would skip that deal permanently and later
                    // PnL, partial volumes or learner history become fictitious.
                    // BALANCE/CREDIT/non-trading entries and other symbols are
                    // not coerced into strategy positions.
                    if d["symbol"] == t.symbol && d["type"].as_i64().is_none() {
                        bail!("MT5 deal type missing for selected symbol; history sync refused");
                    }
                    if matches!(d["type"].as_i64(), Some(0 | 1)) {
                        if d["symbol"] == t.symbol {
                            let valid_magic=d["magic"].as_u64().is_some();
                            let valid_entry=matches!(d["entry"].as_i64(),Some(0 | 1 | 2 | 3));
                            let valid_time=d["time_msc"].as_u64()
                                .is_some_and(|ms| ms>0 && ms<=until_ms.saturating_add(2_000));
                            let valid_volume=d["volume"].as_f64()
                                .is_some_and(|v| v.is_finite() && v>0.);
                            let valid_price=d["price"].as_f64()
                                .is_some_and(|v| v.is_finite() && v>0.);
                            if !(valid_magic && valid_entry && valid_time
                                && valid_volume && valid_price) {
                                bail!("MT5 trading Deal has missing/invalid type, entry, time, volume, price or magic; sync refused");
                            }
                        }
                        for field in ["profit","commission","swap","fee"] {
                            if !d[field].as_f64().is_some_and(f64::is_finite) {
                                bail!("Trading deal has missing/invalid booked cost: {field}");
                            }
                        }
                        // No trade can be accounted against the selected
                        // symbol after advancing the cursor without a durable
                        // positive Deal ticket AND physical position ID.
                        // Balance/credit transfers and other symbols are not
                        // attributed to this account+symbol broker journal.
                        if d["symbol"] == t.symbol
                            && (d["ticket"].as_u64().is_none_or(|v|v==0)
                                || d["position_id"].as_u64().is_none_or(|v|v==0)) {
                            bail!("Trading broker deal lacks a valid ticket or position ID; sync refused");
                        }
                    }
                }
                let tx = db.unchecked_transaction()?;
                for d in ds {
                    let Some(ticket) = d["ticket"].as_u64() else {
                        continue;
                    };
                    let pid = d["position_id"].as_u64().unwrap_or(0);
                    // A ticket is an immutable broker receipt, not an
                    // updatable cost estimate. INSERT OR IGNORE previously
                    // hid conflicts in commission, fee, volume or position and
                    // still advanced history_until_ms, leaving false net PnL.
                    // Inspect existing tickets *before* the magic/owner lookup:
                    // changing position IDs or attribution cannot bypass this.
                    let stored:Option<(u64,String)>=tx.query_row(
                        "SELECT position_id,payload FROM broker_deals WHERE context=?1 AND ticket=?2",
                        params![key,ticket],|r|Ok((r.get(0)?,r.get(1)?))
                    ).optional()?;
                    if let Some((stored_pid,raw))=stored {
                        let saved:Value=serde_json::from_str(&raw)
                            .context("Corrupt saved broker deal receipt; sync blocked")?;
                        let immutable=["ticket","position_id","magic","symbol","entry","type",
                            "time_msc","volume","price","profit","commission","swap","fee"];
                        if stored_pid!=pid || immutable.iter().any(|k|saved[*k]!=d[*k]) {
                            bail!("Conflicting MT5 receipt for already-booked Deal ticket; history sync refused");
                        }
                        // Exact broker replay is idempotent. Do not rewrite
                        // the durable original or relabel its brain.
                        continue;
                    }
                    // Check existing ticket *before* rejecting the wrong
                    // symbol or zero/missing position. Otherwise a poisoned
                    // replay can disappear from the chunk while the Core
                    // advances its broker history cursor.
                    if ticket == 0 || pid == 0 || d["symbol"] != t.symbol {
                        continue;
                    }
                    let owner:Option<String>=execution::brain_for_magic(d["magic"].as_u64().unwrap_or(0)).map(str::to_string).or(tx.query_row("SELECT brain FROM broker_observations WHERE context=?1 AND position_id=?2 UNION ALL SELECT brain FROM broker_deals WHERE context=?1 AND position_id=?2 LIMIT 1",params![key,pid],|r|r.get(0)).optional()?);
                    if let Some(owner) = owner {
                        let mut d = d.clone();
                        d["brain"] = json!(owner);
                        tx.execute(
                            "INSERT INTO broker_deals VALUES(?1,?2,?3,?4,?5)",
                            params![key, ticket, pid, owner, d.to_string()],
                        )?;
                    }
                }
                // The receipt batch and its high-water mark are ONE atomic
                // SQLite transaction. A failed insertion or conflicting
                // immutable ticket rolls both back; stale overlapping reads
                // can add late fills but cannot rewind an existing cursor.
                tx.execute(
                    "INSERT INTO broker_history_sync(context,target,until_ms,synced_ms)
                     VALUES(?1,?2,?3,?4)
                     ON CONFLICT(context,target) DO UPDATE SET
                     until_ms=MAX(broker_history_sync.until_ms,excluded.until_ms),
                     synced_ms=MAX(broker_history_sync.synced_ms,excluded.synced_ms)",
                    params![key,self.settings.target,until_ms,now],
                )?;
                tx.commit()?;
                self.last_sync_ms = now;
                self.history_until_ms = self.history_until_ms.max(until_ms);
                Ok(json!({"ok":true}))
            }
            _ => bail!("Unknown execution route"),
        }
    }
}

#[cfg(test)]
mod execution_persistence_tests {
    use super::*;

    fn demo() -> Telemetry {
        Telemetry {
            connected: true, account_trade_mode: Some(0), demo: true,
            login: 17, server: "Demo-Test".into(), currency: "USD".into(),
            symbol: "XAUUSD".into(), ..Default::default()
        }
    }

    fn receipt(ticket:u64,pid:u64,magic:u64,entry:u64,volume:f64)->Value {
        json!({"ticket":ticket,"position_id":pid,"symbol":"XAUUSD",
            "magic":magic,"type":if entry==0 {0}else{1},
            "entry":entry,"time_msc":250,"volume":volume,
            "price":2000.0,"profit":0.0,"commission":0.0,"swap":0.0,"fee":0.0})
    }
    fn save_deal(db:&Connection,context:&str,d:&Value) {
        db.execute("INSERT INTO broker_deals VALUES (?1,?2,?3,'system',?4)",
            params![context,d["ticket"].as_u64().unwrap(),
                d["position_id"].as_u64().unwrap(),d.to_string()]).unwrap();
    }

    #[test]
    fn external_gateway_result_cannot_force_observed_without_mt5_proof() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,100).unwrap();
        let context=ctx(&t);
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload)
            VALUES('unverified',?1,'system','open',100,'dispatched',?2)",
            params![context,json!({"id":"unverified","kind":"open",
                "symbol":"XAUUSD","magic":410600}).to_string()]).unwrap();
        let status=Status{gateway_fresh:true,telemetry:Some(t),..Default::default()};
        let forged=json!({"id":"unverified","state":"observed",
            "deal":123,"reason":"no proof"});
        assert!(service.route("POST","/v1/execution/result",
            forged.to_string().as_bytes(),&db,&status,200).is_err());
        let (state,result):(String,String)=db.query_row(
            "SELECT state,result FROM broker_commands WHERE id='unverified'",
            [],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(state,"dispatched");
        assert_eq!(result,"{}");
        // A standard fill ACK remains eligible for eventual Core-only
        // reconciliation but does not itself become observed.
        let filled=json!({"id":"unverified","state":"filled","deal":123});
        service.route("POST","/v1/execution/result",
            filled.to_string().as_bytes(),&db,&status,201).unwrap();
        let stored:String=db.query_row(
            "SELECT state FROM broker_commands WHERE id='unverified'",
            [],|r|r.get(0)).unwrap();
        assert_eq!(stored,"filled");
    }

    #[test]
    fn open_observation_requires_exact_deal_position_and_not_just_same_magic() {
        let db=Connection::open_in_memory().unwrap();
        let _=Service::new(&db).unwrap();
        let mut t=demo();
        let context=ctx(&t);
        t.positions.push(braintrade_domain::Position{
            ticket:999,identifier:999,magic:410600,symbol:"XAUUSD".into(),
            volume:0.1,..Default::default()
        });
        let q=json!({"kind":"open","id":"intent-1","symbol":"XAUUSD",
            "magic":410600,"created_ms":200});
        let r=json!({"state":"filled","deal":700,"volume":0.1});
        assert!(!broker_command_observed(&db,&context,&t,&q,&r).unwrap());
        let own=receipt(700,55,410600,0,0.1);
        save_deal(&db,&context,&own);
        assert!(!broker_command_observed(&db,&context,&t,&q,&r).unwrap(),
            "Unrelated same-magic position must never mark command observed");
        t.positions.push(braintrade_domain::Position{
            ticket:55,identifier:55,magic:410600,symbol:"XAUUSD".into(),
            volume:0.1,..Default::default()
        });
        assert!(broker_command_observed(&db,&context,&t,&q,&r).unwrap());
        t.positions.clear();
        let close=receipt(701,55,0,1,0.04);
        save_deal(&db,&context,&close);
        assert!(!broker_command_observed(&db,&context,&t,&q,&r).unwrap(),
            "Partially closed history does not release full-open intent");
        save_deal(&db,&context,&receipt(702,55,0,1,0.06));
        assert!(broker_command_observed(&db,&context,&t,&q,&r).unwrap());
        assert!(!broker_command_observed(&db,&context,&t,&q,
            &json!({"state":"filled","deal":701})).unwrap(),
            "An EXIT receipt cannot stand in for an opening ticket");
    }

    #[test]
    fn close_observation_requires_own_exit_ticket_and_broker_covered_exit() {
        let db=Connection::open_in_memory().unwrap();
        let _=Service::new(&db).unwrap();
        let mut t=demo();
        let context=ctx(&t);
        let q=json!({"kind":"close","id":"close-1","symbol":"XAUUSD",
            "magic":410600,"position":55});
        let r=json!({"state":"filled","deal":801,
            "request":{"volume":0.1}});
        assert!(!broker_command_observed(&db,&context,&t,&q,&r).unwrap(),
            "Missing position is NOT proof of broker close execution");
        save_deal(&db,&context,&receipt(800,55,410600,0,0.1));
        save_deal(&db,&context,&receipt(801,55,0,1,0.04));
        assert!(!broker_command_observed(&db,&context,&t,&q,&r).unwrap(),
            "Exit receipt must cover entire position before final observation");
        t.positions.push(braintrade_domain::Position{
            ticket:55,identifier:55,magic:410600,symbol:"XAUUSD".into(),
            volume:0.06,..Default::default()
        });
        let partial=json!({"state":"partial","deal":801,
            "request":{"volume":0.1}});
        assert!(broker_command_observed(&db,&context,&t,&q,&partial).unwrap(),
            "Broker-proven partial close can be observed with reduced volume");
        assert!(!broker_command_observed(&db,&context,&t,&q,&r).unwrap(),
            "Filled close may not be marked complete while position remains");
        t.positions.clear();
        save_deal(&db,&context,&receipt(802,55,0,1,0.06));
        assert!(broker_command_observed(&db,&context,&t,&q,&r).unwrap());
        let wrong=json!({"kind":"close","symbol":"XAUUSD",
            "magic":410600,"position":77});
        assert!(!broker_command_observed(&db,&context,&t,&wrong,&r).unwrap(),
            "A different physical ticket cannot claim another position's close");
    }

    #[test]
    fn recovered_partial_progress_requires_exact_volume_and_new_broker_deals() {
        let request=json!({"volume":0.10,"price":2000.1});
        let first=json!({"state":"partial","request":request,"volume":0.04,
            "deals":[801],"recovered_from_history":true});
        let upgraded=json!({"state":"filled","request":request,"volume":0.10,
            "deals":[801,802],"recovered_from_history":true});
        assert!(valid_recovered_partial_progress(&first,&upgraded));
        let too_large=json!({"state":"filled","request":request,"volume":0.12,
            "deals":[801,802],"recovered_from_history":true});
        assert!(!valid_recovered_partial_progress(&first,&too_large));
        let duplicated=json!({"state":"filled","request":request,"volume":0.10,
            "deals":[801,801],"recovered_from_history":true});
        assert!(!valid_recovered_partial_progress(&first,&duplicated));
        let regressed=json!({"state":"partial","request":request,"volume":0.03,
            "deals":[801],"recovered_from_history":true});
        assert!(!valid_recovered_partial_progress(&first,&regressed));
        let switched=json!({"state":"filled","request":{"volume":0.20,"price":2000.1},
            "volume":0.20,"deals":[801,802],"recovered_from_history":true});
        assert!(!valid_recovered_partial_progress(&first,&switched));
    }

    #[test]
    fn late_split_fill_updates_core_result_without_reopening_observed_order() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        let st=Status::default();
        let req=json!({"volume":0.10,"price":2000.10});
        let first=json!({"id":"p0b-1","state":"partial","request":req,
            "volume":0.04,"deals":[801],"recovered_from_history":true});
        let payload=json!({"id":"p0b-1","kind":"open"});
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result)
            VALUES (?1,?2,'system','open',100,'partial',?3,?4)",
            params!["p0b-1","ctx-demo",payload.to_string(),first.to_string()]).unwrap();
        let second=json!({"id":"p0b-1","state":"filled","request":req,
            "volume":0.10,"deals":[801,802],"recovered_from_history":true});
        svc.route("POST","/v1/execution/result",second.to_string().as_bytes(),&db,&st,200).unwrap();
        let row: (String,String)=db.query_row("SELECT state,result FROM broker_commands WHERE id='p0b-1'",
            [],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(row.0,"filled");
        assert_eq!(serde_json::from_str::<Value>(&row.1).unwrap()["volume"],0.10);
        db.execute("UPDATE broker_commands SET state='observed',result=?1 WHERE id='p0b-1'",
            [first.to_string()]).unwrap();
        svc.route("POST","/v1/execution/result",second.to_string().as_bytes(),&db,&st,201).unwrap();
        let observed:(String,String)=db.query_row("SELECT state,result FROM broker_commands WHERE id='p0b-1'",
            [],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(observed.0,"observed");
        assert_eq!(serde_json::from_str::<Value>(&observed.1).unwrap()["volume"],0.10);
        svc.route("POST","/v1/execution/result",first.to_string().as_bytes(),&db,&st,202).unwrap();
        let last:String=db.query_row("SELECT result FROM broker_commands WHERE id='p0b-1'",[],|r|r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&last).unwrap()["state"],"filled");
    }

    #[test]
    fn independent_daily_breaker_latches_across_symbols_but_resets_next_baghdad_day() {
        let db=Connection::open_in_memory().unwrap();
        let service=Service::new(&db).unwrap();
        let mut t=demo();
        t.balance=100.;t.equity=97.;t.today_net=Some(-3.);
        let now=crate::now_ms();
        let zone=chrono::FixedOffset::east_opt(3*3600).unwrap();
        let stamp=chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now as i64).unwrap();
        t.report_day=stamp.with_timezone(&zone).format("%Y-%m-%d").to_string();
        assert!(service.daily_breaker_blocked(&db,&t,now,true,2.).unwrap());
        t.today_net=Some(0.);t.equity=100.;
        t.symbol="EURUSD".into();
        assert!(service.daily_breaker_blocked(&db,&t,now,true,2.).unwrap());
        assert!(!service.daily_breaker_blocked(&db,&t,now,false,2.).unwrap());
        // OFF never erases the lock: ON again stays blocked until tomorrow.
        assert!(service.daily_breaker_blocked(&db,&t,now,true,2.).unwrap());
        let tomorrow=now+86_400_000;
        let next=chrono::DateTime::<chrono::Utc>::from_timestamp_millis(tomorrow as i64).unwrap();
        t.report_day=next.with_timezone(&zone).format("%Y-%m-%d").to_string();
        assert!(!service.daily_breaker_blocked(&db,&t,tomorrow,true,2.).unwrap());
        t.today_net=None;
        assert!(service.daily_breaker_blocked(&db,&t,tomorrow,true,2.).unwrap());
        assert!(!service.daily_breaker_blocked(&db,&t,tomorrow,false,2.).unwrap());
        let reloaded=Service::new(&db).unwrap();
        t.report_day=stamp.with_timezone(&zone).format("%Y-%m-%d").to_string();
        assert!(reloaded.daily_breaker_blocked(&db,&t,now,true,2.).unwrap());
    }

    #[test]
    fn stop_keeps_owned_close_authorized_but_cancels_new_open() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,100).unwrap();
        service.set_mode(TradingMode::Sniper);
        let b=service.brains.iter().find(|b|b.id=="system").unwrap().clone();
        let close=service.queue(&db,&t,&b,"close",None,101,200,None).unwrap();
        let signal=Signal{brain:"system".into(),side:1,key:"before-stop".into(),bar_time:100,
            stop_distance:1.,target_distance:2.,hold_ms:60_000,
            reason:"stop test".into(),ready:true,coordination:None};
        let open=service.queue(&db,&t,&b,"open",Some(&signal),0,201,Some(&Config::default())).unwrap();
        let mut status=Status::default();
        status.telemetry=Some(t);
        status.gateway_fresh=true;
        status.requested_run=false;
        let close_work=service.route("GET","/v1/execution/gateway",b"",&db,&status,250).unwrap();
        assert_eq!(close_work["command"]["id"],close["id"]);
        let id=close["id"].as_str().unwrap();
        let auth=service.route("POST","/v1/execution/authorize",
            json!({"id":id}).to_string().as_bytes(),&db,&status,251).unwrap();
        assert_eq!(auth["authorized"],true);
        service.route("POST","/v1/execution/result",
            json!({"id":id,"state":"closed"}).to_string().as_bytes(),&db,&status,252).unwrap();
        let open_work=service.route("GET","/v1/execution/gateway",b"",&db,&status,253).unwrap();
        assert!(open_work["command"].is_null());
        let state:String=db.query_row("SELECT state FROM broker_commands WHERE id=?1",
            [open["id"].as_str().unwrap()],|r|r.get(0)).unwrap();
        assert_eq!(state,"cancelled");
    }

    #[test]
    fn brain_summary_refuses_missing_historical_fee_as_verified_profit() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        let t=demo();
        svc.bind(&db,&t,100).unwrap();
        let key=ctx(&t);
        let entry=json!({"ticket":710,"position_id":77,"symbol":"XAUUSD","brain":"system",
            "type":0,"entry":0,"magic":410600,"volume":0.1,
            "profit":0.,"commission":-0.35,"swap":0.,"fee":0.});
        let mut exit=json!({"ticket":711,"position_id":77,"symbol":"XAUUSD","brain":"system",
            "type":1,"entry":1,"magic":410600,"volume":0.1,
            "profit":3.,"swap":-0.2,"fee":0.});
        for (ticket,deal) in [(710,&entry),(711,&exit)] {
            db.execute("INSERT INTO broker_deals VALUES(?1,?2,77,'system',?3)",
                params![key,ticket,deal.to_string()]).unwrap();
        }
        let mut st=Status::default();
        st.gateway_fresh=true;
        st.telemetry=Some(t);
        let first=svc.route("GET","/v1/execution",b"",&db,&st,200).unwrap();
        let stats=&first["brains"].as_array().unwrap().iter()
            .find(|v|v["config"]["id"]=="system").unwrap()["stats"];
        assert!(stats["net_booked"].is_null());
        assert_eq!(stats["booked_cost_evidence"],"MISSING_OR_INVALID_BROKER_COST");
        assert_eq!(stats["wins"],0);
        exit["commission"]=json!(-0.35);
        db.execute("UPDATE broker_deals SET payload=?1 WHERE context=?2 AND ticket=711",
            params![exit.to_string(),key]).unwrap();
        let clean=svc.route("GET","/v1/execution",b"",&db,&st,201).unwrap();
        let again=&clean["brains"].as_array().unwrap().iter()
            .find(|v|v["config"]["id"]=="system").unwrap()["stats"];
        assert_eq!(again["booked_cost_evidence"],"BROKER_COMPONENTS_VERIFIED");
        assert!((again["net_booked"].as_f64().unwrap()-2.1).abs()<1e-9);
        assert_eq!(again["wins"],1);
    }

    #[test]
    fn incomplete_exit_is_not_a_closed_brain_win_even_with_flat_snapshot() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        let t=demo();
        svc.bind(&db,&t,100).unwrap();
        let key=ctx(&t);
        for (ticket,entry,volume,profit) in [
            (710,0,0.1,0.0),(711,1,0.04,3.0)
        ] {
            let deal=json!({"ticket":ticket,"position_id":77,"symbol":"XAUUSD",
                "brain":"system","type":0,"entry":entry,"volume":volume,
                "price":2000.,"time_msc":ticket,"profit":profit,
                "commission":0.,"swap":0.,"fee":0.});
            db.execute("INSERT INTO broker_deals VALUES(?1,?2,77,'system',?3)",
                params![key,ticket,deal.to_string()]).unwrap();
        }
        let st=Status{gateway_fresh:true,telemetry:Some(t),..Default::default()};
        let view=svc.route("GET","/v1/execution",b"",&db,&st,800).unwrap();
        let stats=&view["brains"].as_array().unwrap().iter()
            .find(|v|v["config"]["id"]=="system").unwrap()["stats"];
        assert_eq!(stats["closed_positions"],0);
        assert_eq!(stats["wins"],0);
        assert_eq!(stats["net_booked"],3.0,
            "Already booked partial PnL remains visible but not a closed win");
        let final_deal=json!({"ticket":712,"position_id":77,"symbol":"XAUUSD",
            "brain":"system","type":1,"entry":3,"volume":0.06,
            "price":2001.,"time_msc":712,"profit":1.,
            "commission":0.,"swap":0.,"fee":0.});
        db.execute("INSERT INTO broker_deals VALUES(?1,712,77,'system',?2)",
            params![key,final_deal.to_string()]).unwrap();
        let full=svc.route("GET","/v1/execution",b"",&db,&st,801).unwrap();
        let complete=&full["brains"].as_array().unwrap().iter()
            .find(|v|v["config"]["id"]=="system").unwrap()["stats"];
        assert_eq!(complete["closed_positions"],1);
        assert_eq!(complete["wins"],1);
        assert_eq!(complete["net_booked"],4.0);
    }

    #[test]
    fn core_history_sync_fails_closed_for_wrong_mode_cost_or_boundary() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,100).unwrap();
        let mut status=Status::default();
        status.telemetry=Some(t);
        status.gateway_fresh=true;
        let deal=json!({"ticket":890,"position_id":201,"magic":410601,
            "symbol":"XAUUSD","type":0,"entry":0,"time_msc":150,
            "volume":0.1,"price":2000.,"profit":1.,
            "commission":-0.07,"swap":0.,"fee":0.});
        let mut request=json!({"login":17,"server":"Demo-Test","currency":"USD",
            "trade_mode":2,"until_ms":200,"deals":[deal.clone()]});
        assert!(service.route("POST","/v1/execution/sync",request.to_string().as_bytes(),&db,&status,300).is_err());
        request.as_object_mut().unwrap().remove("trade_mode");
        assert!(service.route("POST","/v1/execution/sync",request.to_string().as_bytes(),&db,&status,300).is_err());
        request["trade_mode"]=json!(0);
        request["deals"][0].as_object_mut().unwrap().remove("fee");
        assert!(service.route("POST","/v1/execution/sync",request.to_string().as_bytes(),&db,&status,300).is_err());
        request["deals"]=json!([deal]);
        request["until_ms"]=json!(99999);
        assert!(service.route("POST","/v1/execution/sync",request.to_string().as_bytes(),&db,&status,300).is_err());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        request["until_ms"]=json!(200);
        service.route("POST","/v1/execution/sync",request.to_string().as_bytes(),&db,&status,300).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    }

    #[test]
    fn sync_cannot_skip_trading_deal_with_missing_ticket_or_position() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,100).unwrap();
        let mut status=Status::default();
        status.telemetry=Some(t);
        status.gateway_fresh=true;
        let original=json!({"ticket":809,"position_id":99,"magic":410601,
            "symbol":"XAUUSD","type":0,"entry":0,"time_msc":150,
            "volume":0.1,"price":2000.0,"profit":0.0,
            "commission":0.0,"swap":0.0,"fee":0.0});
        let mut req=json!({"login":17,"server":"Demo-Test","currency":"USD",
            "trade_mode":0,"until_ms":220,"deals":[]});
        for (field,value) in [
            ("ticket",Value::Null),("ticket",json!(0)),
            ("position_id",Value::Null),("position_id",json!(0))
        ] {
            let mut damaged=original.clone();
            damaged[field]=value;
            req["deals"]=json!([damaged]);
            assert!(service.route("POST","/v1/execution/sync",
                req.to_string().as_bytes(),&db,&status,300).is_err(),
                "Missing broker {field} advanced history");
            assert_eq!(service.history_until_ms,100);
            let count:u64=db.query_row("SELECT COUNT(*) FROM broker_deals",
                [],|r|r.get(0)).unwrap();
            assert_eq!(count,0);
        }
        // A different broker symbol is not silently assigned to XAUUSD.
        let mut foreign=original.clone();
        foreign["symbol"]=json!("EURUSD");
        foreign["position_id"]=Value::Null;
        req["deals"]=json!([foreign]);
        service.route("POST","/v1/execution/sync",
            req.to_string().as_bytes(),&db,&status,300).unwrap();
        assert_eq!(service.history_until_ms,220);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
            [],|r|r.get::<_,u64>(0)).unwrap(),0);
    }

    #[test]
    fn strict_broker_deal_geometry_blocks_cursor_and_bad_strategy_metrics() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        let t=demo();
        svc.bind(&db,&t,100).unwrap();
        let mut st=Status::default();
        st.gateway_fresh=true;
        st.telemetry=Some(t);
        let original=json!({"ticket":800,"position_id":88,"symbol":"XAUUSD",
            "magic":410601,"type":0,"entry":0,"time_msc":150,
            "volume":0.1,"price":2000.1,"profit":2.,"commission":0.,
            "fee":0.,"swap":0.});
        let mut req=json!({"login":17,"server":"Demo-Test","currency":"USD",
            "trade_mode":0,"until_ms":210,"deals":[]});
        for (field,replacement) in [
            ("type",Value::Null),("type",json!("BUY")),
            ("entry",Value::Null),("entry",json!(9)),
            ("time_msc",Value::Null),("time_msc",json!(0)),
            ("time_msc",json!(100_000)),
            ("volume",Value::Null),("volume",json!(0.)),
            ("volume",json!(-0.2)),("volume",json!("NaN")),
            ("price",Value::Null),("price",json!(0.)),
            ("price",json!(-2000.)),("price",json!("Infinity")),
            ("magic",Value::Null),("magic",json!(-3))
        ] {
            let mut damaged=original.clone();
            damaged[field]=replacement;
            req["deals"]=json!([damaged]);
            assert!(svc.route("POST","/v1/execution/sync",
                req.to_string().as_bytes(),&db,&st,300).is_err(),
                "Corrupt broker {field} must not advance history");
            assert_eq!(svc.history_until_ms,100,"History cursor moved despite invalid {field}");
            assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
                [],|r|r.get::<_,u64>(0)).unwrap(),0);
        }
        // MT5 netting/close-by classifications can exist in the broker
        // ledger. They are kept for accounting, but not reinterpreted as a
        // filled new opening order by Gateway's stricter intent reconciler.
        for entry in [0,1,2,3] {
            let mut valid=original.clone();
            valid["entry"]=json!(entry);
            valid["ticket"]=json!(800+entry);
            req["deals"]=json!([valid]);
            svc.route("POST","/v1/execution/sync",
                req.to_string().as_bytes(),&db,&st,300).unwrap();
        }
        assert_eq!(svc.history_until_ms,210);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
            [],|r|r.get::<_,u64>(0)).unwrap(),4);
    }

    #[test]
    fn durable_history_watermark_survives_core_restart_and_isolates_broker_contexts() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        let t=demo();
        svc.bind(&db,&t,100_000).unwrap();
        let status=Status{gateway_fresh:true,telemetry:Some(t.clone()),..Default::default()};
        let mut request=json!({"login":t.login,"server":t.server,"currency":t.currency,
            "trade_mode":0,"until_ms":500_000,"deals":[]});
        svc.route("POST","/v1/execution/sync",
            request.to_string().as_bytes(),&db,&status,650_000).unwrap();
        assert_eq!(broker_sync_watermark(&db,&ctx(&t),"demo").unwrap(),(500_000,650_000));
        let work=svc.route("GET","/v1/execution/gateway",b"",&db,&status,660_000).unwrap();
        assert_eq!(work["history_from_ms"],380_000);
        let view=svc.route("GET","/v1/execution",b"",&db,&status,660_000).unwrap();
        assert_eq!(view["history_until_ms"],500_000);
        assert_eq!(view["last_broker_sync_ms"],650_000);
        // The rebuilt Rust Core must read committed cursor from SQLite,
        // never replay from the original bind time.
        let mut restored=Service::new(&db).unwrap();
        assert_eq!(restored.history_until_ms,0);
        let work=restored.route("GET","/v1/execution/gateway",
            b"",&db,&status,670_000).unwrap();
        assert_eq!(work["history_from_ms"],380_000);
        // An overlapping older read is allowed to carry delayed broker
        // tickets, but cannot rewind a successful newer watermark.
        request["until_ms"]=json!(450_000);
        request["deals"]=json!([{"ticket":900,"position_id":42,
            "magic":410600,"symbol":"XAUUSD","entry":0,"type":0,
            "time_msc":400_000,"volume":0.1,"price":2000.,
            "profit":0.,"commission":0.,"swap":0.,"fee":0.}]);
        restored.route("POST","/v1/execution/sync",
            request.to_string().as_bytes(),&db,&status,680_000).unwrap();
        assert_eq!(broker_sync_watermark(&db,&ctx(&t),"demo").unwrap(),(500_000,680_000));
        let trades:u64=db.query_row("SELECT COUNT(*) FROM broker_deals",[],
            |r|r.get(0)).unwrap();
        assert_eq!(trades,1);
        // Failure in the next batch cannot claim that 700ms of history
        // was settled. SQLite cursor and booked financial evidence agree.
        request["until_ms"]=json!(700_000);
        request["deals"][0]["commission"]=json!(-100.);
        assert!(restored.route("POST","/v1/execution/sync",
            request.to_string().as_bytes(),&db,&status,710_000).is_err());
        assert_eq!(broker_sync_watermark(&db,&ctx(&t),"demo").unwrap(),(500_000,680_000));
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",[],
            |r|r.get::<_,u64>(0)).unwrap(),1);
        let other=Telemetry{login:99,..t.clone()};
        restored.bind(&db,&other,800_000).unwrap();
        let other_status=Status{gateway_fresh:true,telemetry:Some(other.clone()),..Default::default()};
        let other_work=restored.route("GET","/v1/execution/gateway",
            b"",&db,&other_status,810_000).unwrap();
        assert_eq!(other_work["history_from_ms"],680_000);
        assert_eq!(broker_sync_watermark(&db,&ctx(&other),"demo").unwrap(),(0,0));
        let mut other_symbol=other.clone();
        other_symbol.symbol="EURUSD".into();
        let other_symbol_status=Status{gateway_fresh:true,telemetry:Some(other_symbol.clone()),..Default::default()};
        let changed=restored.route("GET","/v1/execution/gateway",
            b"",&db,&other_symbol_status,820_000).unwrap();
        assert_eq!(changed["history_from_ms"],680_000);
        assert_eq!(broker_sync_watermark(&db,&ctx(&other_symbol),"demo").unwrap(),(0,0));
        // No automatic promotion between Demo and REAL historical cursors.
        assert_eq!(broker_sync_watermark(&db,&ctx(&t),"live").unwrap(),(0,0));
    }

    #[test]
    fn older_overlapping_broker_sync_keeps_cursor_and_accepts_late_deal() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,100).unwrap();
        let mut status=Status::default();
        status.telemetry=Some(t);
        status.gateway_fresh=true;
        let first=json!({"ticket":710,"position_id":55,"magic":410600,
            "symbol":"XAUUSD","type":0,"entry":0,"time_msc":150,
            "volume":0.1,"price":2000.0,"profit":0.0,
            "commission":-0.35,"swap":0.0,"fee":0.0});
        let mut req=json!({"login":17,"server":"Demo-Test","currency":"USD",
            "trade_mode":0,"until_ms":220,"deals":[first.clone()]});
        service.route("POST","/v1/execution/sync",
            req.to_string().as_bytes(),&db,&status,300).unwrap();
        assert_eq!(service.history_until_ms,220);
        // The broker disclosed a delayed fill in an older overlap window.
        let mut late=first.clone();
        late["ticket"]=json!(711);
        late["time_msc"]=json!(180);
        late["volume"]=json!(0.04);
        late["commission"]=json!(-0.14);
        req["until_ms"]=json!(190);
        req["deals"]=json!([first.clone(),late.clone()]);
        service.route("POST","/v1/execution/sync",
            req.to_string().as_bytes(),&db,&status,310).unwrap();
        assert_eq!(service.history_until_ms,220,
            "Overlapping history must not rewind the most recent cursor");
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
            [],|r|r.get::<_,u64>(0)).unwrap(),2,
            "Previously-unseen late receipt must be recorded");
        req["deals"]=json!([first,late]);
        service.route("POST","/v1/execution/sync",
            req.to_string().as_bytes(),&db,&status,320).unwrap();
        assert_eq!(service.history_until_ms,220);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
            [],|r|r.get::<_,u64>(0)).unwrap(),2);
    }

    #[test]
    fn broker_deal_receipt_replays_exactly_but_refuses_changed_booked_costs() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,100).unwrap();
        let mut status=Status::default();
        status.telemetry=Some(t);
        status.gateway_fresh=true;
        let deal=json!({"ticket":890,"position_id":201,"magic":410601,
            "symbol":"XAUUSD","type":0,"entry":0,"time_msc":150,
            "volume":0.1,"price":2000.0,"profit":1.25,
            "commission":-0.07,"swap":0.0,"fee":0.0,"reason":3});
        let mut req=json!({"login":17,"server":"Demo-Test","currency":"USD",
            "trade_mode":0,"until_ms":200,"deals":[deal.clone()]});
        service.route("POST","/v1/execution/sync",req.to_string().as_bytes(),
            &db,&status,300).unwrap();
        let original:String=db.query_row(
            "SELECT payload FROM broker_deals WHERE ticket=890",[],|r|r.get(0)).unwrap();
        req["until_ms"]=json!(220);
        req["deals"]=json!([deal.clone(),deal.clone()]);
        service.route("POST","/v1/execution/sync",req.to_string().as_bytes(),
            &db,&status,320).unwrap();
        assert_eq!(service.history_until_ms,220);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
            [],|r|r.get::<_,u64>(0)).unwrap(),1);
        for (field, value) in [
            ("profit",json!(1.30)), ("commission",json!(-0.70)),
            ("swap",json!(-0.20)), ("fee",json!(-0.01)),
            ("volume",json!(0.2)), ("price",json!(2001.0)),
            ("type",json!(1)), ("entry",json!(1)),
            ("position_id",json!(202)), ("position_id",json!(0)),
            ("symbol",json!("EURUSD")), ("magic",json!(410602)),
            ("time_msc",json!(151))
        ] {
            let mut altered=deal.clone();
            altered[field]=value;
            req["deals"]=json!([altered]);
            req["until_ms"]=json!(230);
            let rejected=service.route("POST","/v1/execution/sync",
                req.to_string().as_bytes(),&db,&status,330);
            assert!(rejected.is_err(),"Changed broker {field} was accepted");
            let saved:String=db.query_row(
                "SELECT payload FROM broker_deals WHERE ticket=890",[],|r|r.get(0)).unwrap();
            assert_eq!(saved,original,"Replayed ticket must remain immutable");
            assert_eq!(service.history_until_ms,220,
                "A broker receipt conflict must not advance history cursor");
        }
    }

    #[test]
    fn conflicted_history_batch_is_atomic_and_does_not_save_new_deals() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        let t=demo();
        svc.bind(&db,&t,100).unwrap();
        let mut st=Status::default();
        st.telemetry=Some(t);
        st.gateway_fresh=true;
        let first=json!({"ticket":900,"position_id":301,"magic":410601,
            "symbol":"XAUUSD","type":0,"entry":0,"time_msc":150,
            "volume":0.1,"price":2000.0,"profit":0.0,
            "commission":-0.07,"swap":0.0,"fee":0.0});
        let mut req=json!({"login":17,"server":"Demo-Test","currency":"USD",
            "trade_mode":0,"until_ms":200,"deals":[first.clone()]});
        svc.route("POST","/v1/execution/sync",req.to_string().as_bytes(),
            &db,&st,300).unwrap();
        let mut newer=first.clone();
        newer["ticket"]=json!(901);
        newer["position_id"]=json!(302);
        let mut conflicting=first.clone();
        conflicting["fee"]=json!(-99.0);
        req["deals"]=json!([newer,conflicting]);
        req["until_ms"]=json!(240);
        assert!(svc.route("POST","/v1/execution/sync",req.to_string().as_bytes(),
            &db,&st,320).is_err());
        assert_eq!(svc.history_until_ms,200);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
            [],|r|r.get::<_,u64>(0)).unwrap(),1,
            "Insert of ticket 901 must rollback with ticket 900 conflict");
        // Two contradictory receipts in one new sync batch are also rejected.
        let mut variant=first.clone();
        variant["ticket"]=json!(902);
        let mut altered=variant.clone();
        altered["commission"]=json!(-7.0);
        req["deals"]=json!([variant,altered]);
        assert!(svc.route("POST","/v1/execution/sync",req.to_string().as_bytes(),
            &db,&st,321).is_err());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_deals",
            [],|r|r.get::<_,u64>(0)).unwrap(),1);
    }

    #[test]
    fn real_micro_risk_is_consented_latched_by_broker_account_and_restored() {
        let db=Connection::open_in_memory().unwrap();
        let mut svc=Service::new(&db).unwrap();
        let mut t=demo();
        t.account_trade_mode=Some(2);t.demo=false;t.equity=50.;t.balance=50.;
        svc.settings.target="live".into();
        svc.settings.login=t.login;
        svc.settings.server=t.server.clone();
        svc.settings.currency=t.currency.clone();
        let mut c=Config::default();
        c.risk.micro_real_temporary=true;
        c.risk.micro_real_cap_percent=10.;
        c.risk.micro_real_recovery_equity=500.;
        assert!(svc.micro_real_active(&db,&t,&c).unwrap());
        let b=svc.brains.iter().find(|b|b.id=="system").unwrap();
        let signal=Signal{brain:"system".into(),side:1,key:"real-micro".into(),
            bar_time:12345,stop_distance:1.,target_distance:2.,hold_ms:60_000,
            reason:"micro mode".into(),ready:true,coordination:None};
        let first=svc.queue(&db,&t,b,"open",Some(&signal),0,12345,Some(&c)).unwrap();
        assert_eq!(first["risk_percent"],10.);
        assert_eq!(first["micro_real_temporary"],true);
        t.equity=500.;
        assert!(!svc.micro_real_active(&db,&t,&c).unwrap());
        t.equity=25.;
        assert!(!svc.micro_real_active(&db,&t,&c).unwrap());
        t.symbol="EURUSD".into();
        assert!(!svc.micro_real_active(&db,&t,&c).unwrap());
        let normal=svc.queue(&db,&t,b,"open",Some(&signal),0,12346,Some(&c)).unwrap();
        assert_eq!(normal["risk_percent"],0.25);
        assert_eq!(normal["micro_real_temporary"],false);
        let mut restarted=Service::new(&db).unwrap();
        restarted.settings.target="live".into();
        restarted.settings.login=t.login;
        restarted.settings.server=t.server.clone();
        restarted.settings.currency=t.currency.clone();
        assert!(!restarted.micro_real_active(&db,&t,&c).unwrap());
    }

    #[test]
    fn every_new_core_entry_records_the_exact_risk_budget_and_full_roundtrip_fee() {
        let db=Connection::open_in_memory().unwrap();
        let service=Service::new(&db).unwrap();
        let t=demo();
        let b=service.brains.iter().find(|b|b.id=="system").unwrap();
        let mut c=Config::default();
        c.risk.risk_percent=0.25;
        c.risk.commission_per_lot=7.;
        c.risk.slippage_ticks=2.;
        let signal=Signal{brain:"system".into(),side:1,key:"risk-stage2".into(),
            bar_time:12345,stop_distance:1.,target_distance:2.,hold_ms:60000,
            reason:"risk sizing test".into(),ready:true,coordination:None};
        let payload=service.queue(&db,&t,b,"open",Some(&signal),0,12345,Some(&c)).unwrap();
        assert_eq!(payload["risk_percent"],0.25);
        assert_eq!(payload["commission_per_lot"],7.);
        assert_eq!(payload["slippage_ticks"],2.);
        assert_eq!(payload["lots"],b.lots);
        assert_eq!(payload["kind"],"open");
        c.risk.commission_per_lot=0.;
        let free=service.queue(&db,&t,b,"open",Some(&signal),0,12346,Some(&c)).unwrap();
        assert_eq!(free["commission_per_lot"],0.);
    }

    #[test]
    fn selected_mode_only_can_dispatch_and_authorize_its_coordinator() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,50).unwrap();
        service.set_mode(TradingMode::Sniper);
        for brain in &mut service.brains {
            // Simulate previously saved 0.6 SOLO flags.
            brain.enabled=true;
        }
        let mut status=Status::default();
        status.telemetry=Some(t.clone());
        status.gateway_fresh=true;
        status.requested_run=true;
        let key=ctx(&t);
        for (id,brain,mode) in [
            ("old-solo","trend","sniper"),
            ("old-mode","system","expert"),
            ("valid-sniper","system","sniper"),
        ] {
            let payload=json!({"id":id,"brain":brain,"trading_mode":mode,"kind":"open","revision":service.settings.revision,
                "context":key,"expires_ms":10000,"created_ms":100});
            db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload) VALUES (?1,?2,?3,'open',100,'dispatched',?4)",
                params![id,key,brain,payload.to_string()]).unwrap();
            let result=service.route("POST","/v1/execution/authorize",
                json!({"id":id}).to_string().as_bytes(),&db,&status,200).unwrap();
            assert_eq!(result["authorized"],id=="valid-sniper", "{id}");
        }
        service.set_mode(TradingMode::Expert);
        let result=service.route("POST","/v1/execution/authorize",
            json!({"id":"valid-sniper"}).to_string().as_bytes(),&db,&status,201).unwrap();
        assert_eq!(result["authorized"],false);
    }

    #[test]
    fn real_account_requires_exact_login_confirmation_and_fresh_telemetry_to_bind() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let mut t=demo();
        t.demo=false;t.account_trade_mode=Some(2);t.login=981234;t.server="Real-Server".into();
        let mut status=Status::default();status.gateway_fresh=true;status.telemetry=Some(t.clone());
        for confirm in [json!(null),json!("98123"),json!("17")] {
            let v=json!({"target":"live","deviation_points":20,"confirm_live_login":confirm});
            assert!(service.route("POST","/v1/execution/settings",v.to_string().as_bytes(),&db,&status,100).is_err());
            assert_eq!(service.settings.target,"demo");
            assert_eq!(service.settings.login,0);
        }
        status.gateway_fresh=false;
        let v=json!({"target":"live","deviation_points":20,"confirm_live_login":"981234"});
        assert!(service.route("POST","/v1/execution/settings",v.to_string().as_bytes(),&db,&status,101).is_err());
        status.gateway_fresh=true;
        service.route("POST","/v1/execution/settings",v.to_string().as_bytes(),&db,&status,102).unwrap();
        assert_eq!(service.settings.target,"live");
        assert_eq!(service.settings.login,981234);
        assert_eq!(Service::new(&db).unwrap().settings.target,"live");
    }

    #[test]
    fn failed_bind_does_not_activate_an_uncommitted_account() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        db.execute_batch("PRAGMA query_only=ON").unwrap();
        assert!(service.bind(&db,&t,1234).is_err());
        assert_eq!(service.settings.login,0);
        assert_eq!(service.settings.bound_ms,0);
        assert_eq!(service.history_until_ms,0);
        assert_eq!(service.last_sync_ms,0);
        db.execute_batch("PRAGMA query_only=OFF").unwrap();
        service.bind(&db,&t,1235).unwrap();
        assert_eq!(service.settings.login,17);
        assert_eq!(service.history_until_ms,1235);
        let restarted=Service::new(&db).unwrap();
        assert_eq!(restarted.settings.login,17);
        assert_eq!(restarted.settings.server,"Demo-Test");
    }

    #[test]
    fn failed_settings_change_keeps_in_memory_and_durable_binding_identical() {
        let db=Connection::open_in_memory().unwrap();
        let mut service=Service::new(&db).unwrap();
        let t=demo();
        service.bind(&db,&t,100).unwrap();
        let old_revision=service.settings.revision;
        let mut status=Status::default();
        status.gateway_fresh=true;
        status.telemetry=Some(t);
        let request=json!({"target":"demo","deviation_points":35}).to_string();
        db.execute_batch("PRAGMA query_only=ON").unwrap();
        assert!(service.route("POST","/v1/execution/settings",request.as_bytes(),&db,&status,200).is_err());
        assert_eq!(service.settings.deviation_points,20);
        assert_eq!(service.settings.bound_ms,100);
        assert_eq!(service.settings.revision,old_revision);
        db.execute_batch("PRAGMA query_only=OFF").unwrap();
        let restarted=Service::new(&db).unwrap();
        assert_eq!(restarted.settings.deviation_points,20);
        assert_eq!(restarted.settings.bound_ms,100);
        service.route("POST","/v1/execution/settings",request.as_bytes(),&db,&status,201).unwrap();
        assert_eq!(service.settings.deviation_points,35);
        assert_eq!(Service::new(&db).unwrap().settings.deviation_points,35);
    }
}
