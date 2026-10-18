//! Durable broker intents, independent brain ownership and broker-observed results.
use anyhow::{bail, Context, Result};
use braintrade_domain::{
    execution::{self, BrainConfig, Signal, BRAINS},
    studio::FrameBar,
    Config, Status, Telemetry,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

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
    pub settings: Settings,
    pub brains: Vec<BrainConfig>,
    pub weights:BTreeMap<String,braintrade_domain::execution::LearnedWeight>,
    pub student_policies:BTreeMap<String,braintrade_domain::student::ExitStudy>,
    signals: BTreeMap<String, Value>,
    last_sync_ms: u64,
    history_until_ms: u64,
}
fn ctx(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap()
}
fn json_rows(db: &Connection, sql: &str, key: &str) -> Result<Vec<Value>> {
    let mut st = db.prepare(sql)?;
    let rows = st
        .query_map([key], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.iter().map(|r| Ok(serde_json::from_str(r)?)).collect()
}
impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS execution_settings(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS execution_equity(context TEXT PRIMARY KEY,peak REAL NOT NULL);
CREATE TABLE IF NOT EXISTS brain_configs(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');
CREATE INDEX IF NOT EXISTS broker_commands_context ON broker_commands(context,brain,created_ms);
CREATE TABLE IF NOT EXISTS broker_seen(context TEXT NOT NULL,brain TEXT NOT NULL,signal TEXT NOT NULL,PRIMARY KEY(context,brain,signal));
CREATE TABLE IF NOT EXISTS broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,ticket));
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
            settings,
            brains,
            signals: BTreeMap::new(),
            weights:BTreeMap::new(),
            student_policies:BTreeMap::new(),
            last_sync_ms: 0,
            history_until_ms: 0,
        })
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
    fn save(&self, db: &Connection) -> Result<()> {
        db.execute(
            "INSERT OR REPLACE INTO execution_settings VALUES(1,?1)",
            [serde_json::to_string(&self.settings)?],
        )?;
        Ok(())
    }
    fn bind(&mut self, db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
        if !t.connected
            || t.account_trade_mode != Some(if self.settings.target == "demo" { 0 } else { 2 })
        {
            bail!("نوع الحساب لا يطابق هدف التنفيذ المختار؛ Demo يحتاج حساب MT5 تجريبي");
        }
        self.settings.login = t.login;
        self.settings.server = t.server.clone();
        self.settings.currency = t.currency.clone();
        self.settings.bound_ms = now;
        self.history_until_ms = now;
        self.last_sync_ms = 0;
        self.save(db)
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
    ) -> Result<()> {
        let key = ctx(t);
        let id = uuid::Uuid::new_v4().simple().to_string();
        let s = signal
            .map(serde_json::to_value)
            .transpose()?
            .unwrap_or(Value::Null);
        let mut p = json!({"id":id,"revision":self.settings.revision,"context":key,"brain":b.id,"kind":kind,"login":t.login,"server":t.server,"currency":t.currency,"target":self.settings.target,"symbol":t.symbol,"magic":execution::magic(&b.id),"position":position,"lots":b.lots,"deviation_points":self.settings.deviation_points,"created_ms":now,"max_quote_age_ms":signal.map(|s|if s.key.starts_with("tick:"){1000}else{3000}).unwrap_or(3000),"expires_ms":if kind=="open"{now+3000}else{now+30_000},"signal":s});
        if let Some(c) = config {
            p["demo_flexible"] = json!(c.trading.demo_flexible && t.demo && t.account_trade_mode==Some(0));
            p["commission_per_lot"] = json!(c.risk.commission_per_lot);
            p["learning"] = json!({"trading_mode":c.trading.mode,"timeframe":b.timeframe,"report_id":b.report_id,
                "tick_size":t.tick_size,"tick_value_loss":t.tick_value_loss,"tick_value_profit":t.tick_value_profit,
                "slippage_ticks":c.risk.slippage_ticks,"bid":t.bid,"ask":t.ask,"quote_time_ms":t.quote_time_ms,
                "context_key":self.weights.get(&b.id).map(|w|w.context_key.clone()).unwrap_or_default(),
                "weight_at_entry":self.weights.get(&b.id).map(|w|w.weight()).unwrap_or(1.),
                "intelligence":self.weights.get(&b.id).map(|w|w.intelligence()).unwrap_or_default()});
        }
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload) VALUES(?1,?2,?3,?4,?5,'queued',?6)",params![id,key,b.id,kind,now,p.to_string()])?;
        Ok(())
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
            self.signals.clear();
            return Ok(());
        }
        let key = ctx(t);
        db.execute("INSERT INTO execution_equity VALUES(?1,?2) ON CONFLICT(context) DO UPDATE SET peak=MAX(peak,excluded.peak)",params![key,t.equity])?;
        let peak:f64=db.query_row("SELECT peak FROM execution_equity WHERE context=?1",[&key],|r|r.get(0))?;
        // A fill acknowledgement is not a reconciled position snapshot. Keep the
        // brain blocked until the broker exposes its position or its deal history.
        let acknowledged=json_rows(db,"SELECT json_object('request',json(payload),'result',json(result)) FROM broker_commands WHERE context=?1 AND state IN ('filled','partial')",&key)?;
        for item in acknowledged {
            let q = &item["request"];
            let r = &item["result"];
            let magic = q["magic"].as_u64().unwrap_or(0);
            let is_open = q["kind"] == "open";
            let visible = if is_open {
                t.positions.iter().any(|p| {
                    p.magic == magic
                        && p.symbol == t.symbol
                        && p.opened_ms + 5000 >= q["created_ms"].as_u64().unwrap_or(u64::MAX)
                })
            } else {
                !t.positions
                    .iter()
                    .any(|p| p.ticket == q["position"].as_u64().unwrap_or(0))
            };
            let confirmed_closed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_deals d JOIN broker_deals e ON e.context=d.context AND e.position_id=d.position_id WHERE d.context=?1 AND d.ticket=?2 AND CAST(json_extract(e.payload,'$.entry') AS INTEGER) IN (1,3))",params![key,r["deal"].as_u64().unwrap_or(0)],|r|r.get(0))?;
            let partial_close_seen=!is_open&&r["state"]=="partial"&&t.positions.iter().find(|p|p.ticket==q["position"].as_u64().unwrap_or(0)).is_some_and(|p|p.volume+1e-8<r["request"]["volume"].as_f64().unwrap_or(0.));
            if visible||(is_open&&confirmed_closed)||partial_close_seen {
                db.execute(
                    "UPDATE broker_commands SET state='observed' WHERE id=?1",
                    [q["id"].as_str().unwrap()],
                )?;
            }
        }
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
            let genome=crate::coordinator_learning::genome(db,&b,t)?;
            let available=crate::coordinator_learning::available_frames(t,frames,now);
            let started = std::time::Instant::now();
            let base=execution::evaluate(&b,t,c,&available,genome.as_ref(),now);
            let mut signal=if b.id=="system" {
                let mut votes=vec![];
                for member in self.brains.iter().filter(|m|m.id!="system"&&!m.enabled) {
                    // Returning SOLO positions finish under their original owner before rejoining.
                    let in_flight:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain=?2 AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",params![key,member.id],|r|r.get(0))?;
                    if in_flight||t.positions.iter().any(|p|p.magic==execution::magic(&member.id).unwrap()&&p.symbol==t.symbol){continue;}
                    let g=crate::coordinator_learning::genome(db,member,t)?;
                    let vote=execution::evaluate(member,t,c,&available,g.as_ref(),now);
                    votes.push((member.clone(),vote,self.weights.get(&member.id).cloned().unwrap_or_default()));
                }
                execution::coordinate_for_account(base,c,t,votes)
            }else{base};
            crate::student_learning::adapt(&mut signal,t,c,&b,&self.student_policies);
            let decision_us = started.elapsed().as_micros() as u64;
            let mut gates = vec![];
            if t.margin_mode!=Some(2)&&db.query_row("SELECT EXISTS(SELECT 1 FROM broker_commands WHERE context=?1 AND brain<>?2 AND state IN ('queued','dispatched','unknown','placed','filled','partial'))",params![key,b.id],|r|r.get::<_,bool>(0))?{gates.push("Netting: أمر لعقل آخر لم تتم تسويته");}

            if !run {
                gates.push("النظام متوقف");
            }
            if !b.enabled {
                gates.push("التداول المنفرد غير مشغّل");
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
            let cooldown = if b.id == "system"
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
                    + c.risk.commission_per_lot * b.lots;
                if loss > t.equity * c.risk.risk_percent / 100. {
                    gates.push("ميزانية المخاطرة الاختيارية");
                }
                if t.today_net
                    .is_none_or(|v| v < -t.balance * c.risk.daily_loss_percent / 100.)
                {
                    gates.push("حد الخسارة اليومي الاختياري أو نتيجته غير متاحة");
                }
                if t.bid.zip(t.ask).is_some_and(|(bid, ask)| {
                    ask - bid > signal.stop_distance * c.risk.max_spread_atr / b.stop_atr
                }) {
                    gates.push("حد السبريد الاختياري");
                }
            }
            let seen:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM broker_seen WHERE context=?1 AND brain=?2 AND signal=?3)",params![key,b.id,signal.key],|r|r.get(0))?;
            if seen {
                gates.push("تمت معالجة هذه الإشارة");
            }
            let can = signal.ready && gates.is_empty();
            let result = json!({"signal":signal,"gates":gates,"ready":can,"decision_us":decision_us,"evaluated_ms":now});
            self.signals.insert(b.id.clone(), result.clone());
            if can {
                let tx = db.unchecked_transaction()?;
                tx.execute(
                    "INSERT OR IGNORE INTO broker_seen VALUES(?1,?2,?3)",
                    params![key, b.id, signal.key],
                )?;
                self.queue(&tx, t, &b, "open", Some(&signal), 0, now, Some(c))?;
                tx.execute("INSERT INTO broker_signal_log(context,brain,timestamp_ms,payload) VALUES(?1,?2,?3,?4)",params![key,b.id,now,result.to_string()])?;
                tx.commit()?;
            }
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
                    let mut groups: BTreeMap<u64, f64> = BTreeMap::new();
                    let mut net = 0.;
                    for d in &own {
                        let pnl = d["profit"].as_f64().unwrap_or(0.)
                            + d["commission"].as_f64().unwrap_or(0.)
                            + d["swap"].as_f64().unwrap_or(0.)
                            + d["fee"].as_f64().unwrap_or(0.);
                        net += pnl;
                        *groups
                            .entry(d["position_id"].as_u64().unwrap_or(0))
                            .or_default() += pnl;
                    }
                    let closed: Vec<_> = groups
                        .iter()
                        .filter(|(pid, _)| {
                            !positions.iter().any(|p| p.identifier == **pid)
                                && own.iter().any(|d| {
                                    d["position_id"].as_u64() == Some(**pid)
                                        && matches!(d["entry"].as_i64(), Some(1 | 3))
                                })
                        })
                        .collect();
                    brains.push(json!({"config":b,"name":BRAINS.iter().find(|v|v.0==b.id).unwrap().1,"magic":magic,"latest":self.signals.get(&b.id).filter(|v|t.is_some_and(|t|self.matches(t))&&now.saturating_sub(v["evaluated_ms"].as_u64().unwrap_or(0))<=3000),"learning":self.weights.get(&b.id).filter(|_|t.is_some_and(|t|self.matches(t))),"learned_weight":self.weights.get(&b.id).filter(|_|t.is_some_and(|t|self.matches(t))).map(|w|w.weight()),"stats":{"deals":own.len(),"closed_positions":closed.len(),"wins":closed.iter().filter(|(_,p)|**p>0.).count(),"net_booked":net,"floating":positions.iter().filter(|p|p.magic==magic).map(|p|p.profit).sum::<f64>(),"open_positions":positions.iter().filter(|p|p.magic==magic).count()}}));
                }
                let commands=json_rows(db,"SELECT json_object('request',json(payload),'state',state,'result',json(result)) FROM broker_commands WHERE context=?1 ORDER BY created_ms DESC LIMIT 100",&key)?;
                Ok(
                    json!({"settings":self.settings,"armed":self.enabled(status),"account_matches":t.is_some_and(|t|self.matches(t)),"context":key,"brains":brains,"commands":commands,"last_broker_sync_ms":self.last_sync_ms,"history_until_ms":self.history_until_ms,"note":"أرباح محققة من سجل الوسيط؛ العمولات المنفصلة التي لا تحمل position_id لا تنسب تلقائياً لعقل. STOP يمنع دخولاً جديداً، وإدارة خروج الصفقات المملوكة تستمر."}),
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
                self.settings = candidate;
                self.bind(db, t, now)?;
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
                        let allowed = cmd["kind"] == "close"
                            || (self.enabled(status)
                                && self
                                    .brains
                                    .iter()
                                    .any(|b| b.id == cmd["brain"] && b.enabled));
                        if allowed {
                            db.execute("UPDATE broker_commands SET state='dispatched' WHERE id=?1 AND state='queued'",[id])?;
                        } else {
                            db.execute("UPDATE broker_commands SET state='cancelled' WHERE id=?1 AND state='queued'",[id])?;
                            command = None;
                        }
                    }
                }
                Ok(
                    json!({"command":command,"history_from_ms":self.history_until_ms.max(self.settings.bound_ms).saturating_sub(120_000),"account":{"login":t.login,"server":t.server,"currency":t.currency},"poll_ms":if self.brains.iter().any(|b|b.enabled){100}else{250}}),
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
                let ok = state == "dispatched"
                    && t.is_some_and(|t| self.matches(t) && ctx(t) == cmd["context"])
                    && now <= cmd["expires_ms"].as_u64().unwrap_or(0)
                    && (cmd["kind"] == "close"
                        || (cmd["revision"].as_u64()==Some(self.settings.revision)
                            && self.enabled(status)
                            && self
                                .brains
                                .iter()
                                .any(|b| b.id == cmd["brain"] && b.enabled)));
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
                        | "observed"
                        | "cancelled"
                ) {
                    bail!("Invalid broker state");
                }
                let old: String =
                    db.query_row("SELECT state FROM broker_commands WHERE id=?1", [id], |r| {
                        r.get(0)
                    })?;
                if matches!(old.as_str(), "dispatched" | "unknown" | "placed") {
                    db.execute(
                        "UPDATE broker_commands SET state=?2,result=?3 WHERE id=?1",
                        params![id, state, v.to_string()],
                    )?;
                }
                Ok(json!({"ok":true}))
            }
            ("POST", "/v1/execution/sync") => {
                let v: Value = serde_json::from_slice(body)?;
                let t = t
                    .filter(|t| self.matches(t))
                    .context("Bound account unavailable")?;
                if v["login"] != t.login || v["server"] != t.server || v["currency"] != t.currency {
                    bail!("Broker history account mismatch");
                }
                let ds = v["deals"].as_array().context("Deals required")?;
                if ds.len() > 100_000 {
                    bail!("History chunk too large");
                }
                let tx = db.unchecked_transaction()?;
                for d in ds {
                    let Some(ticket) = d["ticket"].as_u64() else {
                        continue;
                    };
                    let pid = d["position_id"].as_u64().unwrap_or(0);
                    if pid == 0 || d["symbol"] != t.symbol {
                        continue;
                    }
                    let owner:Option<String>=execution::brain_for_magic(d["magic"].as_u64().unwrap_or(0)).map(str::to_string).or(tx.query_row("SELECT brain FROM broker_observations WHERE context=?1 AND position_id=?2 UNION ALL SELECT brain FROM broker_deals WHERE context=?1 AND position_id=?2 LIMIT 1",params![key,pid],|r|r.get(0)).optional()?);
                    if let Some(owner) = owner {
                        let mut d = d.clone();
                        d["brain"] = json!(owner);
                        tx.execute(
                            "INSERT OR IGNORE INTO broker_deals VALUES(?1,?2,?3,?4,?5)",
                            params![key, ticket, pid, owner, d.to_string()],
                        )?;
                    }
                }
                tx.commit()?;
                self.last_sync_ms = now;
                self.history_until_ms = v["until_ms"]
                    .as_u64()
                    .filter(|v| *v <= now + 2000)
                    .context("Invalid history boundary")?;
                Ok(json!({"ok":true}))
            }
            _ => bail!("Unknown execution route"),
        }
    }
}
