//! Research-only adaptive-exit challenger.
//! It observes reconciled MT5 Demo positions and records the first causal alternative exit.
//! This module has no broker command writer and never queues/closes a position.
use anyhow::Result;
use braintrade_domain::{
    execution as domain_execution,
    studio::FrameBar,
    Config, Position, Telemetry,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const UPDATE_INTERVAL_MS: u64 = 500;

fn context(t: &Telemetry) -> String {
    crate::coordinator_learning::context(t)
}

fn table_exists(db: &Connection, name: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?)
}

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS shadow_exit_trials(\
            context TEXT NOT NULL,\
            position_id INTEGER NOT NULL,\
            opened_ms INTEGER NOT NULL,\
            updated_ms INTEGER NOT NULL,\
            state TEXT NOT NULL,\
            payload TEXT NOT NULL,\
            PRIMARY KEY(context,position_id));\
         CREATE INDEX IF NOT EXISTS shadow_exit_state ON shadow_exit_trials(context,state,updated_ms);",
    )?;
    Ok(())
}

fn position_id(p: &Position) -> u64 {
    if p.identifier > 0 { p.identifier } else { p.ticket }
}

fn executable_exit_price(t: &Telemetry, p: &Position) -> Option<f64> {
    match p.side.as_str() {
        "BUY" => t.bid,
        "SELL" => t.ask,
        _ => None,
    }
    .filter(|price| price.is_finite() && *price > 0.0)
}

fn direction(p: &Position) -> Option<f64> {
    match p.side.as_str() {
        "BUY" => Some(1.0),
        "SELL" => Some(-1.0),
        _ => None,
    }
}

fn frozen_stop_distance(db: &Connection, key: &str, p: &Position) -> Result<Option<f64>> {
    let dir = direction(p)?;
    let attached = match p.side.as_str() {
        "BUY" if p.sl > 0.0 && p.sl < p.price_open => Some(p.price_open - p.sl),
        "SELL" if p.sl > p.price_open => Some(p.sl - p.price_open),
        _ => None,
    }
    .filter(|distance| distance.is_finite() && *distance > 0.0);
    if attached.is_some() {
        return Ok(attached);
    }
    if !table_exists(db, "broker_commands")? || !table_exists(db, "broker_deals")? {
        return Ok(None);
    }
    let pid = position_id(p);
    let stop: Option<f64> = db
        .query_row(
            "SELECT CAST(json_extract(c.payload,'$.signal.stop_distance') AS REAL)\
             FROM broker_commands c\
             JOIN broker_deals d ON d.context=c.context\
              AND d.ticket=CAST(json_extract(c.result,'$.deal') AS INTEGER)\
             WHERE c.context=?1 AND c.brain='system' AND c.kind='open' AND d.position_id=?2\
             ORDER BY c.created_ms DESC LIMIT 1",
            params![key, pid],
            |row| row.get(0),
        )
        .optional()?;
    Ok(stop.filter(|distance| distance.is_finite() && *distance > 0.0 && dir.is_finite()))
}

fn frozen_hold_ms(db: &Connection, key: &str, pid: u64) -> Result<u64> {
    if !table_exists(db, "broker_observations")? {
        return Ok(0);
    }
    Ok(db
        .query_row(
            "SELECT hold_ms FROM broker_observations WHERE context=?1 AND position_id=?2",
            params![key, pid],
            |row| row.get::<_, u64>(0),
        )
        .optional()?
        .unwrap_or(0))
}

fn estimated_net(t: &Telemetry, c: &Config, p: &Position, exit_price: f64) -> Option<f64> {
    let dir = direction(p)?;
    let tick = t.tick_size.filter(|v| v.is_finite() && *v > 0.0)?;
    let profit_value = t.tick_value_profit.filter(|v| v.is_finite() && *v > 0.0)?;
    let loss_value = t.tick_value_loss.filter(|v| v.is_finite() && *v > 0.0)?;
    if !p.volume.is_finite() || p.volume <= 0.0 || !p.price_open.is_finite() || p.price_open <= 0.0 {
        return None;
    }
    let move_price = (exit_price - p.price_open) * dir;
    let value = if move_price >= 0.0 { profit_value } else { loss_value };
    let gross = move_price / tick * value * p.volume;
    // Entry is an already-booked broker fill. The shadow exit estimates one additional
    // market-exit slippage while retaining the configured round-trip commission model.
    let exit_slippage = c.risk.slippage_ticks.max(0.0) * value * p.volume;
    let commission = c.risk.commission_per_lot.max(0.0) * p.volume;
    let net = gross - exit_slippage - commission;
    net.is_finite().then_some(net)
}

fn candidate_reason(
    p: &Position,
    current_r: f64,
    peak_r: f64,
    hold_fraction: f64,
    market_direction: i8,
) -> Option<&'static str> {
    if peak_r >= 1.0 && current_r <= peak_r - 0.40 && current_r >= 0.25 {
        return Some("profit_retrace");
    }
    let position_direction = if p.side == "BUY" { 1 } else if p.side == "SELL" { -1 } else { 0 };
    if market_direction != 0 && position_direction != 0 && market_direction == -position_direction && current_r <= 0.50 {
        return Some("regime_flip");
    }
    if hold_fraction >= 0.75 && current_r < 0.25 {
        return Some("time_decay");
    }
    None
}

fn actual_broker_outcome(db: &Connection, key: &str, pid: u64) -> Result<Option<Value>> {
    if !table_exists(db, "broker_deals")? {
        return Ok(None);
    }
    let mut statement = db.prepare(
        "SELECT payload FROM broker_deals WHERE context=?1 AND position_id=?2 ORDER BY ticket",
    )?;
    let rows = statement
        .query_map(params![key, pid], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut net = 0.0;
    let mut closed_ms = 0_u64;
    let mut exit_value = 0.0;
    let mut exit_volume = 0.0;
    let mut has_exit = false;
    for raw in rows {
        let deal: Value = serde_json::from_str(&raw)?;
        net += deal["profit"].as_f64().unwrap_or(0.0)
            + deal["commission"].as_f64().unwrap_or(0.0)
            + deal["swap"].as_f64().unwrap_or(0.0)
            + deal["fee"].as_f64().unwrap_or(0.0);
        if matches!(deal["entry"].as_i64(), Some(1) | Some(3)) {
            has_exit = true;
            let volume = deal["volume"].as_f64().unwrap_or(0.0);
            let price = deal["price"].as_f64().unwrap_or(0.0);
            if volume.is_finite() && volume > 0.0 && price.is_finite() && price > 0.0 {
                exit_value += price * volume;
                exit_volume += volume;
            }
            closed_ms = closed_ms.max(deal["time_msc"].as_u64().unwrap_or(0));
        }
    }
    if !has_exit || !net.is_finite() {
        return Ok(None);
    }
    Ok(Some(json!({
        "actual_net":net,
        "actual_exit_price":if exit_volume>0.0 { Value::from(exit_value/exit_volume) } else { Value::Null },
        "closed_ms":closed_ms,
        "source":"exact_booked_mt5_deals_profit_plus_commission_plus_swap_plus_fee"
    })))
}

fn settle_closed(db: &Connection, t: &Telemetry, now: u64) -> Result<()> {
    let key = context(t);
    let visible: std::collections::BTreeSet<u64> = t
        .positions
        .iter()
        .filter(|p| p.symbol == t.symbol && p.magic == domain_execution::magic("system").unwrap_or(0))
        .map(position_id)
        .collect();
    let mut statement = db.prepare(
        "SELECT position_id,payload FROM shadow_exit_trials WHERE context=?1 AND state IN ('tracking','candidate')",
    )?;
    let rows = statement
        .query_map([&key], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (pid, raw) in rows {
        if visible.contains(&pid) {
            continue;
        }
        let Some(actual) = actual_broker_outcome(db, &key, pid)? else {
            continue;
        };
        let mut payload: Value = serde_json::from_str(&raw)?;
        let shadow_net = payload["candidate"]["estimated_net"].as_f64();
        let actual_net = actual["actual_net"].as_f64();
        payload["actual"] = actual;
        payload["comparison"] = json!({
            "shadow_minus_actual_net":shadow_net.zip(actual_net).map(|(shadow,actual)|shadow-actual),
            "interpretation":"counterfactual estimate minus exact booked broker outcome; not a fill claim"
        });
        payload["settled_ms"] = json!(now);
        let state = if shadow_net.is_some() { "settled_candidate" } else { "settled_no_candidate" };
        db.execute(
            "UPDATE shadow_exit_trials SET state=?3,updated_ms=?4,payload=?5 WHERE context=?1 AND position_id=?2",
            params![&key, pid, state, now, payload.to_string()],
        )?;
    }
    Ok(())
}

pub fn observe(
    db: &Connection,
    t: &Telemetry,
    c: &Config,
    frames: &BTreeMap<String, Vec<FrameBar>>,
    now: u64,
) -> Result<()> {
    // V1 shadow evidence is deliberately scoped to exact Demo execution context.
    if !t.connected || !t.demo || t.account_trade_mode != Some(0) {
        return Ok(());
    }
    init(db)?;
    let key = context(t);
    let intelligence = domain_execution::intelligence_context(t, frames, now);
    let system_magic = domain_execution::magic("system").unwrap_or(0);
    for p in t.positions.iter().filter(|p| p.symbol == t.symbol && p.magic == system_magic) {
        let pid = position_id(p);
        let Some(exit_price) = executable_exit_price(t, p) else { continue; };
        let Some(dir) = direction(p) else { continue; };
        let Some(stop_distance) = frozen_stop_distance(db, &key, p)? else { continue; };
        if !p.price_open.is_finite() || p.price_open <= 0.0 || stop_distance <= 0.0 {
            continue;
        }
        let current_r = ((exit_price - p.price_open) * dir / stop_distance).clamp(-20.0, 20.0);
        let hold_ms = frozen_hold_ms(db, &key, pid)?;
        let hold_fraction = if hold_ms > 0 && p.opened_ms > 0 {
            now.saturating_sub(p.opened_ms) as f64 / hold_ms as f64
        } else {
            0.0
        };
        let existing: Option<String> = db
            .query_row(
                "SELECT payload FROM shadow_exit_trials WHERE context=?1 AND position_id=?2",
                params![&key, pid],
                |row| row.get(0),
            )
            .optional()?;
        let mut payload = existing
            .as_deref()
            .map(serde_json::from_str::<Value>)
            .transpose()?
            .unwrap_or_else(|| json!({
                "mode":"research_only_shadow",
                "can_send_orders":false,
                "context":key,
                "position_id":pid,
                "opened_ms":p.opened_ms,
                "side":p.side,
                "entry_price":p.price_open,
                "volume":p.volume,
                "stop_distance":stop_distance,
                "hold_ms":hold_ms,
                "peak_r":current_r,
                "trough_r":current_r,
                "last_r":current_r,
                "candidate":Value::Null,
                "actual":Value::Null,
            }));
        let previous_update = payload["last_observed_ms"].as_u64().unwrap_or(0);
        let old_peak = payload["peak_r"].as_f64().unwrap_or(current_r);
        let old_trough = payload["trough_r"].as_f64().unwrap_or(current_r);
        let peak_r = old_peak.max(current_r);
        let trough_r = old_trough.min(current_r);
        let reason = if payload["candidate"].is_null() {
            candidate_reason(p, current_r, peak_r, hold_fraction, intelligence.direction)
        } else {
            None
        };
        let material = peak_r != old_peak
            || trough_r != old_trough
            || reason.is_some()
            || now.saturating_sub(previous_update) >= UPDATE_INTERVAL_MS;
        if !material {
            continue;
        }
        payload["peak_r"] = json!(peak_r);
        payload["trough_r"] = json!(trough_r);
        payload["last_r"] = json!(current_r);
        payload["last_exit_quote"] = json!(exit_price);
        payload["last_observed_ms"] = json!(now);
        payload["last_intelligence"] = serde_json::to_value(&intelligence)?;
        if let Some(reason) = reason {
            payload["candidate"] = json!({
                "reason":reason,
                "observed_ms":now,
                "exit_price":exit_price,
                "r":current_r,
                "peak_r_at_decision":peak_r,
                "trough_r_at_decision":trough_r,
                "hold_fraction":hold_fraction,
                "estimated_net":estimated_net(t,c,p,exit_price),
                "estimate_model":"executable Bid for BUY / Ask for SELL; actual entry fill; configured round-trip commission plus one hypothetical exit slippage"
            });
        }
        let state = if payload["candidate"].is_null() { "tracking" } else { "candidate" };
        db.execute(
            "INSERT INTO shadow_exit_trials(context,position_id,opened_ms,updated_ms,state,payload)\
             VALUES(?1,?2,?3,?4,?5,?6)\
             ON CONFLICT(context,position_id) DO UPDATE SET updated_ms=excluded.updated_ms,state=excluded.state,payload=excluded.payload",
            params![&key, pid, p.opened_ms, now, state, payload.to_string()],
        )?;
    }
    settle_closed(db, t, now)?;
    Ok(())
}

pub fn summary(db: &Connection, t: Option<&Telemetry>) -> Result<Value> {
    init(db)?;
    let Some(t) = t.filter(|t| t.connected) else {
        return Ok(json!({"mode":"research_only_shadow","can_send_orders":false,"active":0,"settled":0,"candidates":0,"latest":[]}));
    };
    let key = context(t);
    let (active, settled, candidates): (u64, u64, u64) = db.query_row(
        "SELECT\
           COALESCE(SUM(CASE WHEN state IN ('tracking','candidate') THEN 1 ELSE 0 END),0),\
           COALESCE(SUM(CASE WHEN state LIKE 'settled_%' THEN 1 ELSE 0 END),0),\
           COALESCE(SUM(CASE WHEN json_type(payload,'$.candidate')='object' THEN 1 ELSE 0 END),0)\
         FROM shadow_exit_trials WHERE context=?1",
        [&key],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mean_delta: Option<f64> = db.query_row(
        "SELECT AVG(CAST(json_extract(payload,'$.comparison.shadow_minus_actual_net') AS REAL))\
         FROM shadow_exit_trials WHERE context=?1 AND state='settled_candidate'",
        [&key],
        |row| row.get(0),
    )?;
    let mut statement = db.prepare(
        "SELECT payload FROM shadow_exit_trials WHERE context=?1 ORDER BY updated_ms DESC LIMIT 20",
    )?;
    let latest = statement
        .query_map([&key], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|raw| serde_json::from_str::<Value>(&raw).map_err(anyhow::Error::from))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "mode":"research_only_shadow",
        "can_send_orders":false,
        "active":active,
        "settled":settled,
        "candidates":candidates,
        "mean_shadow_minus_actual_net":mean_delta,
        "latest":latest,
        "note":"Shadow Adaptive Exit is observational only. Candidate PnL is an estimate; actual PnL comes only from booked MT5 deals."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE broker_observations(context TEXT NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,opened_ms INTEGER NOT NULL,hold_ms INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,position_id));\
             CREATE TABLE broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,ticket));\
             CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');",
        ).unwrap();
        db
    }

    fn telemetry(now: u64) -> Telemetry {
        Telemetry {
            connected:true,demo:true,account_trade_mode:Some(0),login:9,server:"DEMO".into(),currency:"USD".into(),symbol:"XAUUSD".into(),
            bid:Some(2001.20),ask:Some(2001.30),tick_size:Some(0.01),point:Some(0.01),tick_value_profit:Some(1.0),tick_value_loss:Some(1.0),tick_age_ms:Some(0),quote_time_ms:Some(now),
            positions:vec![Position{magic:domain_execution::magic("system").unwrap(),identifier:700,ticket:700,symbol:"XAUUSD".into(),side:"BUY".into(),volume:0.1,price_open:2000.0,price_current:2001.2,profit:12.0,sl:1999.0,tp:2003.0,opened_ms:now-30_000}],
            ..Default::default()
        }
    }

    #[test]
    fn first_retrace_candidate_uses_buy_bid_and_never_writes_broker_commands() {
        let db=setup();let now=1_800_000_000_000;let mut t=telemetry(now);let key=context(&t);
        db.execute("INSERT INTO broker_observations VALUES(?1,700,'system',?2,120000,'{}')",params![key,now-30_000]).unwrap();
        observe(&db,&t,&Config::default(),&BTreeMap::new(),now).unwrap();
        t.bid=Some(2000.70);t.ask=Some(2000.80);t.quote_time_ms=Some(now+1_000);
        observe(&db,&t,&Config::default(),&BTreeMap::new(),now+1_000).unwrap();
        let raw:String=db.query_row("SELECT payload FROM shadow_exit_trials WHERE position_id=700",[],|r|r.get(0)).unwrap();let p:Value=serde_json::from_str(&raw).unwrap();
        assert_eq!(p["candidate"]["reason"],"profit_retrace");assert_eq!(p["candidate"]["exit_price"],2000.70);
        let commands:u64=db.query_row("SELECT COUNT(*) FROM broker_commands",[],|r|r.get(0)).unwrap();assert_eq!(commands,0);
    }

    #[test]
    fn closed_candidate_is_compared_with_exact_booked_broker_net() {
        let db=setup();let now=1_800_000_000_000;let mut t=telemetry(now);let key=context(&t);
        db.execute("INSERT INTO broker_observations VALUES(?1,700,'system',?2,120000,'{}')",params![&key,now-30_000]).unwrap();
        observe(&db,&t,&Config::default(),&BTreeMap::new(),now).unwrap();
        t.bid=Some(2000.70);t.ask=Some(2000.80);observe(&db,&t,&Config::default(),&BTreeMap::new(),now+1_000).unwrap();
        t.positions.clear();
        let open=json!({"entry":0,"profit":0.0,"commission":-0.3,"swap":0.0,"fee":0.0,"volume":0.1,"price":2000.0,"time_msc":now-30_000});
        let close=json!({"entry":1,"profit":5.0,"commission":-0.3,"swap":-0.1,"fee":-0.1,"volume":0.1,"price":2000.5,"time_msc":now+2_000});
        db.execute("INSERT INTO broker_deals VALUES(?1,1,700,'system',?2)",params![&key,open.to_string()]).unwrap();
        db.execute("INSERT INTO broker_deals VALUES(?1,2,700,'system',?2)",params![&key,close.to_string()]).unwrap();
        observe(&db,&t,&Config::default(),&BTreeMap::new(),now+3_000).unwrap();
        let raw:String=db.query_row("SELECT payload FROM shadow_exit_trials WHERE position_id=700",[],|r|r.get(0)).unwrap();let p:Value=serde_json::from_str(&raw).unwrap();
        assert!((p["actual"]["actual_net"].as_f64().unwrap()-4.2).abs()<1e-9);assert!(p["comparison"]["shadow_minus_actual_net"].is_number());
    }

    #[test]
    fn live_context_is_not_recorded() {
        let db=setup();let now=1_800_000_000_000;let mut t=telemetry(now);t.demo=false;t.account_trade_mode=Some(2);
        observe(&db,&t,&Config::default(),&BTreeMap::new(),now).unwrap();
        let n:u64=db.query_row("SELECT COUNT(*) FROM shadow_exit_trials",[],|r|r.get(0)).unwrap();assert_eq!(n,0);
    }
}
