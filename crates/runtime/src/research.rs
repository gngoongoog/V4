//! Quote-sampled research. No broker order API is used here.
use anyhow::Result;
use braintrade_domain::{
    opportunities::{self, MarketBrains},
    research::{analyze, size_lots},
    trading::{self,TradingMode}, Candle,
    Analysis, Config, MarketData, PaperSummary, Telemetry,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn default_hold()->u64 {1_800_000}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PaperPosition {
    side: i8,
    entry: f64,
    stop: f64,
    target: f64,
    lots: f64,
    tick_size: f64,
    tick_loss: f64,
    tick_profit: f64,
    commission: f64,
    slippage: f64,
    opened_ms: u64,
    interrupted: bool,
    #[serde(default="default_hold")]
    max_hold_ms:u64,
    #[serde(default)]
    mode:TradingMode,
}
impl PaperPosition {
    fn pnl(&self, exit: f64) -> f64 {
        let change = (exit - self.entry) * self.side as f64;
        change / self.tick_size
            * self.lots
            * if change >= 0.0 {
                self.tick_profit
            } else {
                self.tick_loss
            }
            - self.commission
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Book {
    starting_equity: f64,
    realized: f64,
    peak: f64,
    day: u64,
    day_start: f64,
    daily_locked: bool,
    drawdown_locked: bool,
    last_bar: u64,
    last_quote_ms: u64,
    #[serde(default)]
    mode:TradingMode,
    #[serde(default)]
    last_entry_ms:u64,
    #[serde(default)]
    decision_tick_ms:u64,
    recorded: [u64; 3],
    trades: u64,
    wins: u64,
    interrupted_trades: u64,
    position: Option<PaperPosition>,
    recent: Vec<String>,
}
fn day_id(now: u64) -> u64 {
    (now / 1000 + 3 * 3600) / 86400
}
fn context(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap()
}
fn valid_quote(t: &Telemetry) -> Option<(f64, f64)> {
    if !t.connected || t.tick_age_ms.is_none_or(|age| age > 3000) {
        return None;
    }
    match (t.bid, t.ask) {
        (Some(b), Some(a)) if b > 0.0 && a >= b && b.is_finite() && a.is_finite() => Some((b, a)),
        _ => None,
    }
}
impl Book {
    fn new(equity: f64, now: u64) -> Self {
        Self {
            starting_equity: equity,
            realized: 0.0,
            peak: equity,
            day: day_id(now),
            day_start: equity,
            daily_locked: false,
            drawdown_locked: false,
            last_bar: 0,
            last_quote_ms: 0,
            mode:TradingMode::Expert,last_entry_ms:0,decision_tick_ms:0,
            recorded: [0; 3],
            trades: 0,
            wins: 0,
            interrupted_trades: 0,
            position: None,
            recent: vec![],
        }
    }
    fn summary(&self, context: &str, quote: Option<(f64, f64)>) -> PaperSummary {
        let unrealized = quote.map(|(bid, ask)| {
            self.position
                .as_ref()
                .map(|p| {
                    p.pnl(if p.side == 1 {
                        bid - p.slippage
                    } else {
                        ask + p.slippage
                    })
                })
                .unwrap_or(0.0)
        });
        PaperSummary {context:context.into(),quote_current:quote.is_some(),equity:unrealized.map(|p|self.starting_equity+self.realized+p),realized:self.realized,unrealized,
            trades:self.trades,wins:self.wins,interrupted_trades:self.interrupted_trades,
            open:self.position.as_ref().map(|p|format!("VIRTUAL {} | {:.4} lots\nEntry {:.5} | SL {:.5} | TP {:.5}\n{}",if p.side==1 {"BUY"} else {"SELL"},p.lots,p.entry,p.stop,p.target,
                format!("{:?} | max hold {}s | {}",p.mode,p.max_hold_ms/1000,if p.interrupted {"Data gap detected; missed intragap exits unknown."} else {"Open virtual position"}))).unwrap_or("No open virtual position".into()),
            recent:self.recent.clone(),note:format!("Daily lock: {} | Drawdown lock: {}. STOP blocks new entries; virtual exits continue while Core runs.",self.daily_locked,self.drawdown_locked)}
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Research {
    books: BTreeMap<String, Book>,
    #[serde(skip)]
    opportunity_cache: BTreeMap<String, MarketBrains>,
    #[serde(skip)]
    opportunity_inputs: BTreeMap<String, MarketData>,
}
impl Research {
    pub fn has_open_position(&self, telemetry: &Telemetry) -> bool {
        self.books
            .get(&context(telemetry))
            .is_some_and(|book| book.position.is_some())
    }
    pub fn load(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS research_checkpoint(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS candles(context TEXT NOT NULL,timeframe TEXT NOT NULL,bar_time INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,timeframe,bar_time));
            CREATE TABLE IF NOT EXISTS decisions(id INTEGER PRIMARY KEY,context TEXT NOT NULL,bar_time INTEGER NOT NULL,timestamp_ms INTEGER NOT NULL,payload TEXT NOT NULL,UNIQUE(context,bar_time));
            CREATE TABLE IF NOT EXISTS paper_events(id INTEGER PRIMARY KEY,context TEXT NOT NULL,timestamp_ms INTEGER NOT NULL,kind TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS mode_decisions(id INTEGER PRIMARY KEY,context TEXT NOT NULL,timestamp_ms INTEGER NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS opportunity_setups(context TEXT NOT NULL,setup_id TEXT NOT NULL,bar_time INTEGER NOT NULL,first_seen_ms INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,setup_id));")?;
        let mut stmt = db.prepare("SELECT payload FROM research_checkpoint WHERE id=1")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        match rows.first() {
            Some(v) => Ok(serde_json::from_str(v)?),
            None => Ok(Self::default()),
        }
    }
    fn refresh_opportunity_cache(&mut self, key: &str, t: &Telemetry, a: &Analysis, now: u64) {
        if !a.current {
            if let Some(cached) = self.opportunity_cache.get_mut(key) {
                cached.withdraw(&a.reason);
            }
            return;
        }
        let Some(market) = t.market.as_ref() else {
            return;
        };
        let needs_scan = self
            .opportunity_cache
            .get(key)
            .is_none_or(|cached| cached.bar_time != a.bar_time || !cached.current)
            || self.opportunity_inputs.get(key).is_none_or(|previous|
                previous.m1 != market.m1 || previous.m5 != market.m5 || previous.m15 != market.m15);
        if needs_scan {
            let scan = opportunities::scan(
                market,
                &t.symbol,
                &a.trend_brain,
                &a.regime_brain,
                now,
            );
            self.opportunity_cache.insert(key.to_owned(), scan);
            self.opportunity_inputs.insert(key.to_owned(), market.clone());
        }
    }
    /// Call on a clone. Publish only after checkpoint, decision and ledger transaction commits.
    pub fn update(
        &mut self,
        db: &mut Connection,
        t: &Telemetry,
        c: &Config,
        run: bool,
        now: u64,
    ) -> Result<(Analysis, PaperSummary)> {
        self.update_with_frames(db,t,c,run,now,&BTreeMap::new())
    }
    pub fn update_with_frames(&mut self,db:&mut Connection,t:&Telemetry,c:&Config,run:bool,now:u64,frames:&BTreeMap<String,Vec<Candle>>)->Result<(Analysis,PaperSummary)> {
        let mut a=analyze(t.market.as_ref(),&t.symbol,now);
        let key = context(t);
        let display = format!("{} / {} / {} / {}", t.login, t.server, t.symbol, t.currency);
        let quote = valid_quote(t);
        if !t.connected || t.login == 0 || t.server.is_empty() || t.symbol != c.symbol {
            a.current = false;
            a.action = "WAIT".into();
            a.reason = "Account/symbol reconciliation unavailable".into();
            return Ok((a, PaperSummary::default()));
        }
        if t.equity <= 0.0 && !self.books.contains_key(&key) {
            a.action = "WAIT".into();
            a.gates.push("Account equity must be positive".into());
            return Ok((a, PaperSummary::default()));
        }
        self.refresh_opportunity_cache(&key,t,&a,now);
        a=trading::select(a,t,c,frames,now);
        let b = self
            .books
            .entry(key.clone())
            .or_insert_with(|| Book::new(t.equity, now));
        if b.mode!=c.trading.mode {b.mode=c.trading.mode;b.last_bar=0;}
        let Some((bid, ask)) = quote else {
            a.action = "WAIT".into();
            a.gates
                .push("Quote unavailable or older than 3 seconds".into());
            if let Some(cached) = self.opportunity_cache.get(&key) {
                let mut current = cached.clone();
                current.expire(now, false);
                a.opportunities = Some(current);
            }
            return Ok((a, b.summary(&display, None)));
        };
        let mut ledger: Vec<(&str, Value)> = vec![];
        if let Some(p) = b.position.as_mut() {
            if b.last_quote_ms == 0 || now.saturating_sub(b.last_quote_ms) > 8000 {
                if !p.interrupted {
                    ledger.push(("quote_gap", json!({"from_ms":b.last_quote_ms,"to_ms":now})));
                }
                p.interrupted = true;
            }
        }
        b.last_quote_ms = now;
        let mark = b
            .position
            .as_ref()
            .map(|p| {
                p.pnl(if p.side == 1 {
                    bid - p.slippage
                } else {
                    ask + p.slippage
                })
            })
            .unwrap_or(0.0);
        let equity = b.starting_equity + b.realized + mark;
        if b.day < day_id(now) {
            b.day = day_id(now);
            b.day_start = equity;
            b.daily_locked = false;
        }
        b.peak = b.peak.max(equity);
        if equity <= b.day_start * (1.0 - c.risk.daily_loss_percent / 100.0) {
            b.daily_locked = true;
        }
        if equity <= b.peak * (1.0 - c.risk.max_drawdown_percent / 100.0) {
            b.drawdown_locked = true;
        }
        let mut closed = false;
        if let Some(p) = b.position.as_ref() {
            let exit = if p.side == 1 {
                bid - p.slippage
            } else {
                ask + p.slippage
            };
            let stop = if p.side == 1 {
                bid <= p.stop
            } else {
                ask >= p.stop
            };
            let target = if p.side == 1 {
                bid >= p.target
            } else {
                ask <= p.target
            };
            let timeout = now.saturating_sub(p.opened_ms) >= p.max_hold_ms;
            if stop || target || timeout {
                let why = if stop {
                    "SL"
                } else if target {
                    "TP"
                } else {
                    "TIMEOUT"
                };
                let net = p.pnl(exit);
                ledger.push((
                    "close",
                    json!({"reason":why,"position":p,"exit":exit,"net":net,"quote_sampled":true}),
                ));
                b.realized += net;
                b.trades += 1;
                b.wins += u64::from(net > 0.0);
                b.interrupted_trades += u64::from(p.interrupted);
                b.recent.insert(
                    0,
                    format!(
                        "{} | {} {:.4} lots | {why} @ {exit:.5} | net {net:.2} {}{}",
                        now,
                        if p.side == 1 { "BUY" } else { "SELL" },
                        p.lots,
                        t.currency,
                        if p.interrupted {
                            " | GAP / incomplete price path"
                        } else {
                            ""
                        }
                    ),
                );
                b.recent.truncate(12);
                b.position = None;
                closed = true;
            }
        }
        let sniper=c.trading.mode==TradingMode::Sniper;let new_tick=t.quote_time_ms.is_some_and(|q|q>b.decision_tick_ms);let cooldown=now.saturating_sub(b.last_entry_ms)>=c.trading.sniper_cooldown_seconds*1000;
        let new_bar=a.current&&run&&if sniper{new_tick&&cooldown}else{a.bar_time>b.last_bar};
        if sniper&&!cooldown {a.gates.push("Sniper cooldown active".into());}if sniper&&!new_tick {a.gates.push("Waiting for new broker tick".into());}
        if !run {
            a.gates
                .push("System stopped — new virtual entries disabled".into());
        }
        if t.equity <= 0.0 {
            a.gates.push("Account equity must be positive".into());
        }
        if b.position.is_some() {
            a.gates.push("One virtual position already open".into());
        }
        if !t.positions.is_empty() || t.pending_orders > 0 {
            a.gates
                .push("MT5 already has open positions or pending orders".into());
        }
        if b.daily_locked {
            a.gates
                .push("Virtual daily loss limit reached (Baghdad day)".into());
        }
        if b.drawdown_locked {
            a.gates.push("Virtual peak drawdown limit reached".into());
        }
        if closed {
            a.gates
                .push("An exit occurred; no re-entry on the same quote".into());
        }
        if !sniper && a.current && a.bar_time <= b.last_bar {
            a.gates
                .push("This mode bar has already been evaluated".into());
        }

        // Market opportunities are observational and quote-aware. Their closed-bar map is cached
        // per context; only quote projection changes between M1 closes.
        if a.current {
            let opportunity_gates = a
                .gates
                .iter()
                .filter(|gate| gate.as_str() != "This mode bar has already been evaluated")
                .cloned()
                .collect::<Vec<_>>();
            let available = (equity
                - b.day_start * (1.0 - c.risk.daily_loss_percent / 100.0))
                .min(equity - b.peak * (1.0 - c.risk.max_drawdown_percent / 100.0));
            let budget = (((b.starting_equity + b.realized).min(t.equity)
                * c.risk.risk_percent
                / 100.0)
                .min(available))
                .max(0.0);
            if let Some(cached) = self.opportunity_cache.get(&key) {
                let mut current = cached.clone();
                current.price(t, &c.risk, budget, &opportunity_gates, now);
                a.opportunities = Some(current);
            }
        }

        let raw = a.action.clone();
        if matches!(raw.as_str(), "BUY" | "SELL") {
            let side = if raw == "BUY" { 1 } else { -1 };
            let atr = a.atr.unwrap();
            if ask - bid > atr * c.risk.max_spread_atr {
                a.gates
                    .push("Spread exceeds configured ATR fraction".into());
            }
            if (if c.trading.mode==TradingMode::Swing {frames.get("H1").and_then(|b|b.last())}else{t.market.as_ref().and_then(|m|m.m1.last())})
                .is_some_and(|last| ((ask + bid) / 2.0 - last.close).abs() > 2.0 * atr)
            {
                a.gates
                    .push("Quote moved more than 2 ATR from the analyzed close".into());
            }
            let specs = match (
                t.tick_size,
                t.tick_value_loss,
                t.tick_value_profit,
                t.volume_min,
                t.volume_step,
                t.volume_max,
            ) {
                (Some(tick), Some(loss), Some(profit), Some(min), Some(step), Some(max))
                    if [tick, loss, profit, min, step, max]
                        .iter()
                        .all(|v| v.is_finite() && *v > 0.0) =>
                {
                    Some((tick, loss, profit, min, step, max))
                }
                _ => None,
            };
            if let Some((tick, loss, profit, min, step, max)) = specs {
                let slip = tick * c.risk.slippage_ticks;
                let entry = if side == 1 { ask + slip } else { bid - slip };
                let (stop_distance,mut target_distance)=c.trading.geometry(atr,t.point.unwrap_or(0.));
                if sniper {match trading::costs(t,c,1.,now) {Ok(cost)=>{if cost.total/(cost.total+cost.desired_net_target_money)>c.trading.max_cost_fraction {a.gates.push("Sniper entry costs exceed configured share of movement".into());}},Err(e)=>a.gates.push(e)}target_distance+=c.risk.commission_per_lot/profit*tick+slip;}
                let stop = if side == 1 {
                    ((entry - stop_distance) / tick).floor() * tick
                } else {
                    ((entry + stop_distance) / tick).ceil() * tick
                };
                let target = if side == 1 {
                    ((entry + if c.trading.mode==TradingMode::Expert {2.0*(entry-stop).abs()}else{target_distance}) / tick).ceil() * tick
                } else {
                    ((entry - if c.trading.mode==TradingMode::Expert {2.0*(entry-stop).abs()}else{target_distance}) / tick).floor() * tick
                };
                if (side==1&&stop>=bid)||(side==-1&&stop<=ask) {a.gates.push("Stop must exceed entry spread and slippage".into());}
                let loss_per_lot = ((entry - stop).abs() + slip) / tick * loss.max(profit)
                    + c.risk.commission_per_lot;
                let available = (equity - b.day_start * (1.0 - c.risk.daily_loss_percent / 100.0))
                    .min(equity - b.peak * (1.0 - c.risk.max_drawdown_percent / 100.0));
                let budget = ((b.starting_equity + b.realized).min(t.equity) * c.risk.risk_percent
                    / 100.0)
                    .min(available);
                let lots = size_lots(budget, loss_per_lot, min, step, max.min(c.risk.max_lots));
                if entry <= 0.0
                    || stop <= 0.0
                    || target <= 0.0
                    || [entry, stop, target, loss_per_lot]
                        .iter()
                        .any(|v| !v.is_finite())
                {
                    a.gates.push("Invalid price/risk geometry".into());
                } else if let Some(lots) = lots {
                    a.entry = Some(entry);
                    a.stop = Some(stop);
                    a.target = Some(target);
                    a.lots = Some(lots);
                    a.estimated_risk = Some(lots * loss_per_lot);
                    if a.gates.is_empty() && new_bar {
                        let p = PaperPosition {
                            side,
                            entry,
                            stop,
                            target,
                            lots,
                            tick_size: tick,
                            tick_loss: loss,
                            tick_profit: profit,
                            commission: lots * c.risk.commission_per_lot,
                            slippage: slip,
                            opened_ms: now,
                            interrupted: false,
                            max_hold_ms:c.trading.hold_ms(),mode:c.trading.mode,
                        };
                        ledger.push((
                            "open",
                            json!({"position":p,"risk":c.risk,"bar_time":a.bar_time}),
                        ));
                        b.position = Some(p);b.last_entry_ms=now;
                        a.action = format!("SHADOW_{raw}");
                        a.reason =
                            "Virtual position opened from current quote; no broker order sent"
                                .into();
                    }
                } else {
                    a.gates
                        .push("Minimum broker lot exceeds risk budget or lot cap".into());
                }
            } else {
                a.gates
                    .push("Broker tick values or volume limits unavailable".into());
            }
        }
        if !a.gates.is_empty() {
            a.action = "WAIT".into();
        }
        if new_bar {
            b.last_bar = a.bar_time;
            if sniper {b.decision_tick_ms=t.quote_time_ms.unwrap_or(0);}
        }
        // A Shadow entry on this quote must not leave a concurrent READY plan visible.
        if b.position.is_some() {
            if let Some(opportunities)=a.opportunities.as_mut() {
                opportunities.pause("توجد صفقة افتراضية مفتوحة؛ أرقام الفرص الجديدة مسحوبة");
            }
        }
        let summary = b.summary(&display, quote);
        let tx = db.transaction()?;
        if let Some(m) = t.market.as_ref().filter(|m| m.valid()) {
            for (index, (tf, bars)) in [("M1", &m.m1), ("M5", &m.m5), ("M15", &m.m15)]
                .iter()
                .enumerate()
            {
                let interval = [60, 300, 900][index];
                for bar in bars.iter().filter(|bar| {
                    bar.time > b.recorded[index] && bar.time.saturating_add(interval) <= now / 1000
                }) {
                    tx.execute("INSERT OR IGNORE INTO candles(context,timeframe,bar_time,payload) VALUES(?1,?2,?3,?4)",params![key,tf,bar.time,serde_json::to_string(bar)?])?;
                }
                if let Some(last) = bars
                    .iter()
                    .rfind(|bar| bar.time.saturating_add(interval) <= now / 1000)
                {
                    b.recorded[index] = b.recorded[index].max(last.time);
                }
            }
        }
        if let Some(opps) = a.opportunities.as_ref() {
            for setup in &opps.setups {
                tx.execute(
                    "INSERT OR IGNORE INTO opportunity_setups(context,setup_id,bar_time,first_seen_ms,payload) VALUES(?1,?2,?3,?4,?5)",
                    params![key, setup.id, setup.signal_bar, now, serde_json::to_string(setup)?],
                )?;
            }
        }
        if new_bar {
            tx.execute("INSERT OR IGNORE INTO decisions(context,bar_time,timestamp_ms,payload) VALUES(?1,?2,?3,?4)",params![key,a.bar_time,now,json!({"analysis":a,"risk":c.risk,"bid":bid,"ask":ask,"mode":"shadow"}).to_string()])?;
        }
        if new_bar&&matches!(raw.as_str(),"BUY"|"SELL") {tx.execute("INSERT INTO mode_decisions(context,timestamp_ms,payload) VALUES(?1,?2,?3)",params![key,now,json!({"analysis":a,"trading":c.trading,"risk":c.risk,"quote_time_ms":t.quote_time_ms,"bid":bid,"ask":ask}).to_string()])?;}
        for (kind, payload) in ledger {
            tx.execute(
                "INSERT INTO paper_events(context,timestamp_ms,kind,payload) VALUES(?1,?2,?3,?4)",
                params![key, now, kind, payload.to_string()],
            )?;
        }
        tx.execute("INSERT INTO research_checkpoint(id,payload) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[serde_json::to_string(self)?])?;
        tx.commit()?;
        Ok((a, summary))
    }
}
pub fn opportunity_history(db: &Connection) -> Result<Value> {
    let mut statement = db.prepare("SELECT context,setup_id,bar_time,first_seen_ms,payload FROM opportunity_setups ORDER BY first_seen_ms DESC,setup_id DESC LIMIT 100")?;
    let rows = statement.query_map([], |row| {
        let raw: String = row.get(4)?;
        Ok(json!({"context":row.get::<_,String>(0)?,"setup_id":row.get::<_,String>(1)?,
            "bar_time":row.get::<_,u64>(2)?,"first_seen_ms":row.get::<_,u64>(3)?,
            "setup":serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null)}))
    })?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(json!({"setups":rows,"scope":"First observation only; historical plans are never current quotes or broker orders."}))
}
pub fn decision_history(db: &Connection) -> Result<Value> {
    let mut q = db.prepare(
        "SELECT id,context,bar_time,timestamp_ms,payload FROM decisions ORDER BY id DESC LIMIT 100",
    )?;
    let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"context":r.get::<_,String>(1)?,"bar_time":r.get::<_,u64>(2)?,"timestamp_ms":r.get::<_,u64>(3)?,"decision":serde_json::from_str::<Value>(&r.get::<_,String>(4)?).unwrap_or(Value::Null)})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(json!({"decisions":rows}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{Candle, MarketData};
    fn snapshot(now: u64) -> Telemetry {
        let bars = |seconds: u64| {
            (0..180)
                .map(|i| {
                    let p = 2000.0 + i as f64 * 0.3;
                    Candle {
                        time: now / 1000 / seconds * seconds - (180 - i) * seconds,
                        open: p,
                        high: p + 0.6,
                        low: p - 0.6,
                        close: p + 0.25,
                        tick_volume: 100,
                    }
                })
                .collect()
        };
        Telemetry {
            connected: true,
            login: 123,
            server: "TEST".into(),
            currency: "USD".into(),
            symbol: "XAUUSD".into(),
            equity: 10000.0,
            balance: 10000.0,
            bid: Some(2054.0),
            ask: Some(2054.1),
            tick_age_ms: Some(0),
            tick_size: Some(0.01),
            tick_value_loss: Some(1.0),
            tick_value_profit: Some(1.0),
            volume_min: Some(0.01),
            volume_step: Some(0.01),
            volume_max: Some(100.0),
            market: Some(MarketData {
                captured_ms: now,
                m1: bars(60),
                m5: bars(300),
                m15: bars(900),
                error: String::new(),
            }),
            ..Default::default()
        }
    }
    #[test]
    fn opportunity_cache_refreshes_corrected_history_not_capture_timestamp() {
        let now=1_700_000_000_000;
        let mut db=Connection::open_in_memory().unwrap();
        let mut research=Research::load(&db).unwrap();let mut t=snapshot(now);
        let c=Config::default();let key=context(&t);
        research.update(&mut db,&t,&c,false,now).unwrap();
        let first=research.opportunity_cache[&key].computed_ms;
        t.market.as_mut().unwrap().captured_ms=now+1;
        research.update(&mut db,&t,&c,false,now+1).unwrap();
        assert_eq!(research.opportunity_cache[&key].computed_ms,first);
        t.market.as_mut().unwrap().m5[160].high+=0.2;
        research.update(&mut db,&t,&c,false,now+2).unwrap();
        assert_eq!(research.opportunity_cache[&key].computed_ms,now+2);
        assert!(serde_json::to_value(&research).unwrap().get("opportunity_inputs").is_none());
    }
    #[test]
    fn restart_dedup_and_exit_while_stopped() {
        let now = 1800000020000;
        let c = Config::default();
        let mut t = snapshot(now);
        let mut db = Connection::open_in_memory().unwrap();
        let mut r = Research::load(&db).unwrap();
        let (a, p) = r.update(&mut db, &t, &c, true, now).unwrap();
        assert_eq!(a.action, "SHADOW_BUY");
        assert!(p.open.contains("BUY"));
        let mut r = Research::load(&db).unwrap();
        assert_eq!(
            r.update(&mut db, &t, &c, true, now + 1000)
                .unwrap()
                .0
                .action,
            "WAIT"
        );
        t.bid = Some(a.target.unwrap() + 0.5);
        t.ask = Some(a.target.unwrap() + 0.6);
        let (_, p) = r.update(&mut db, &t, &c, false, now + 2000).unwrap();
        assert_eq!(p.trades, 1);
        assert!(p.realized > 0.0);
        r.update(&mut db, &t, &c, true, now + 3000).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM paper_events WHERE kind='open'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            Research::load(&db)
                .unwrap()
                .books
                .values()
                .next()
                .unwrap()
                .trades,
            1
        );
    }
    #[test]
    fn risk_vetoes() {
        let now = 1800000020000;
        let c = Config::default();
        for n in 0..4 {
            let mut t = snapshot(now);
            let mut db = Connection::open_in_memory().unwrap();
            let mut r = Research::load(&db).unwrap();
            match n {
                0 => t.ask = Some(2060.0),
                1 => t.tick_value_loss = None,
                2 => t.tick_age_ms = Some(4000),
                _ => t.pending_orders = 1,
            };
            let (a, _) = r.update(&mut db, &t, &c, true, now).unwrap();
            assert_eq!(a.action, "WAIT");
            assert!(!a.gates.is_empty());
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM paper_events", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
    #[test]
    fn gaps_and_account_isolation() {
        let now = 1800000020000;
        let c = Config::default();
        let mut t = snapshot(now);
        let mut db = Connection::open_in_memory().unwrap();
        let mut r = Research::load(&db).unwrap();
        let (a, _) = r.update(&mut db, &t, &c, true, now).unwrap();
        t.bid = Some(a.stop.unwrap() - 1.0);
        t.ask = Some(a.stop.unwrap() - 0.9);
        let (_, p) = r.update(&mut db, &t, &c, false, now + 9000).unwrap();
        assert_eq!(p.interrupted_trades, 1);
        assert!(p.realized < -a.estimated_risk.unwrap());
        t.login = 456;
        assert_eq!(
            r.update(&mut db, &t, &c, false, now + 10000)
                .unwrap()
                .1
                .trades,
            0
        );
        assert_eq!(r.books.len(), 2);
    }
    #[test]
    fn limits_latch_through_restart() {
        let now = 1800000020000;
        let mut c = Config::default();
        let mut t = snapshot(now);
        let mut db = Connection::open_in_memory().unwrap();
        let mut r = Research::load(&db).unwrap();
        r.update(&mut db, &t, &c, true, now).unwrap();
        t.bid = Some(1900.0);
        t.ask = Some(1900.1);
        r.update(&mut db, &t, &c, false, now + 1000).unwrap();
        let mut r = Research::load(&db).unwrap();
        assert!(r.books.values().next().unwrap().daily_locked);
        assert!(r.books.values().next().unwrap().drawdown_locked);
        c.risk.daily_loss_percent = 10.0;
        c.risk.max_drawdown_percent = 30.0;
        let (a, _) = r
            .update(&mut db, &snapshot(now + 60000), &c, true, now + 60000)
            .unwrap();
        assert_eq!(a.action, "WAIT");
        assert!(a.gates.iter().any(|g| g.contains("drawdown")));
    }
    #[test]
    fn transaction_rollback() {
        let now = 1800000020000;
        let mut db = Connection::open_in_memory().unwrap();
        let live = Research::load(&db).unwrap();
        db.execute_batch("CREATE TRIGGER reject_checkpoint BEFORE INSERT ON research_checkpoint BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        let mut attempted = live.clone();
        assert!(attempted
            .update(&mut db, &snapshot(now), &Config::default(), true, now)
            .is_err());
        assert!(live.books.is_empty());
        for table in ["paper_events", "decisions", "candles"] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
    #[test]
    fn sell_costs_once_and_exits_at_zero_account_equity() {
        let now = 1800000020000;
        let c = Config::default();
        let mut t = snapshot(now);
        let m = t.market.as_mut().unwrap();
        for series in [&mut m.m1, &mut m.m5, &mut m.m15] {
            for b in series {
                let h = b.high;
                b.open = 4000.0 - b.open;
                b.close = 4000.0 - b.close;
                b.high = 4000.0 - b.low;
                b.low = 4000.0 - h;
            }
        }
        t.bid = Some(1946.0);
        t.ask = Some(1946.1);
        let mut db = Connection::open_in_memory().unwrap();
        let mut r = Research::load(&db).unwrap();
        let (a, p) = r.update(&mut db, &t, &c, true, now).unwrap();
        assert_eq!(a.action, "SHADOW_SELL");
        assert!((a.entry.unwrap() - 1945.98).abs() < 1e-8);
        assert!((p.unrealized.unwrap() + 2.1).abs() < 1e-8);
        t.equity = 0.0;
        t.ask = Some(a.stop.unwrap() + 0.1);
        t.bid = Some(a.stop.unwrap());
        let (_, p) = r.update(&mut db, &t, &c, false, now + 1000).unwrap();
        let expected = (1945.98 - (a.stop.unwrap() + 0.12)) / 0.01 * 0.1 - 0.7;
        assert_eq!(p.trades, 1);
        assert!((p.realized - expected).abs() < 1e-8);
    }
}
