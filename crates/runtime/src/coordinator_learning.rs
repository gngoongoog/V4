//! Causal per-member learning for the coordinator.
//! Forward-bar evidence is retained as a fallback, while exact closed MT5 Demo broker
//! outcomes are preferred when available. Broker evidence is never synthesized.
use anyhow::Result;
use braintrade_domain::{
    execution::{self, BrainConfig, LearnedWeight},
    studio::{FrameBar, LabReport},
    trading::TradingMode,
    Config, Telemetry,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn context(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap()
}

fn source_key(
    context: &str,
    mode: TradingMode,
    id: &str,
    timeframe: &str,
    report_id: &str,
) -> Result<String> {
    Ok(serde_json::to_string(&(
        context,
        "adaptive-member-v1",
        mode,
        id,
        timeframe,
        report_id,
    ))?)
}

fn table_exists(db: &Connection, name: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |r| r.get(0),
    )?)
}

pub fn available_frames(
    t: &Telemetry,
    frames: &BTreeMap<String, Vec<FrameBar>>,
    now: u64,
) -> BTreeMap<String, Vec<FrameBar>> {
    let mut available = frames.clone();
    if let Some(m) = &t.market {
        if now >= m.captured_ms && now - m.captured_ms <= 15_000 {
            for (tf, seconds, bs) in [
                ("M1", 60, &m.m1),
                ("M5", 300, &m.m5),
                ("M15", 900, &m.m15),
            ] {
                if bs.len() >= 100
                    && bs.last().is_some_and(|b| {
                        b.time + seconds <= now / 1000
                            && now / 1000 - (b.time + seconds) <= seconds + 20
                    })
                    && bs[bs.len() - 30..]
                        .windows(2)
                        .all(|w| w[1].time == w[0].time + seconds)
                    && available
                        .get(tf)
                        .and_then(|b| b.last())
                        .is_none_or(|b| b.candle.time < bs.last().unwrap().time)
                {
                    available.insert(
                        tf.into(),
                        bs.iter()
                            .map(|b| FrameBar {
                                candle: b.clone(),
                                spread_points: 0,
                                real_volume: 0,
                            })
                            .collect(),
                    );
                }
            }
        }
    }
    available
}

pub fn genome(
    db: &Connection,
    b: &BrainConfig,
    t: &Telemetry,
) -> Result<Option<braintrade_domain::studio::Genome>> {
    if b.id != "generated" || b.report_id.is_empty() {
        return Ok(None);
    }
    let raw: Option<String> = db
        .query_row(
            "SELECT payload FROM studio_runs WHERE id=?1 AND context=?2 AND timeframe=?3",
            params![b.report_id, context(t), b.timeframe],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| Ok(serde_json::from_str::<LabReport>(&s)?.selected))
        .transpose()
}

#[derive(Clone, Debug, Default)]
struct Stats {
    samples: u64,
    wins: u64,
    mean_net_per_lot: f64,
}

fn forward_stats(db: &Connection, source: &str) -> Result<Stats> {
    Ok(db.query_row(
        "SELECT COUNT(*),COALESCE(SUM(CASE WHEN json_extract(outcome,'$.net')>0 THEN 1 ELSE 0 END),0),COALESCE(AVG(json_extract(outcome,'$.net')),0) FROM coordinator_samples WHERE source=?1 AND json_extract(outcome,'$.state')='scored'",
        [source],
        |r| {
            Ok(Stats {
                samples: r.get(0)?,
                wins: r.get(1)?,
                mean_net_per_lot: r.get(2)?,
            })
        },
    )?)
}

fn broker_stats(db: &Connection, source: &str) -> Result<Stats> {
    Ok(db.query_row(
        "SELECT COUNT(*),COALESCE(SUM(CASE WHEN CAST(json_extract(payload,'$.net') AS REAL)>0 THEN 1 ELSE 0 END),0),COALESCE(AVG(CAST(json_extract(payload,'$.net_per_lot') AS REAL)),0) FROM coordinator_broker_samples WHERE source=?1",
        [source],
        |r| {
            Ok(Stats {
                samples: r.get(0)?,
                wins: r.get(1)?,
                mean_net_per_lot: r.get(2)?,
            })
        },
    )?)
}

fn selected_weight(forward: Stats, broker: Stats) -> LearnedWeight {
    if broker.samples > 0 {
        LearnedWeight {
            samples: broker.samples,
            wins: broker.wins,
            mean_net_per_lot: broker.mean_net_per_lot,
            broker_samples: broker.samples,
            forward_samples: forward.samples,
            evidence_source: "mt5_demo_broker".into(),
        }
    } else {
        LearnedWeight {
            samples: forward.samples,
            wins: forward.wins,
            mean_net_per_lot: forward.mean_net_per_lot,
            broker_samples: 0,
            forward_samples: forward.samples,
            evidence_source: "forward_bars".into(),
        }
    }
}

fn open_coordinator_commands(db: &Connection, key: &str) -> Result<Vec<(String, String)>> {
    let mut st = db.prepare(
        "SELECT payload,result FROM broker_commands WHERE context=?1 AND brain='system' AND kind='open' AND CAST(json_extract(result,'$.deal') AS INTEGER)>0 ORDER BY created_ms,id",
    )?;
    let mapped = st.query_map([key], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let rows = mapped.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn position_deals(db: &Connection, key: &str, position_id: u64) -> Result<Vec<String>> {
    let mut st = db.prepare(
        "SELECT payload FROM broker_deals WHERE context=?1 AND position_id=?2 ORDER BY ticket",
    )?;
    let mapped = st.query_map(params![key, position_id], |r| r.get::<_, String>(0))?;
    let rows = mapped.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Convert only exact closed coordinator trades from a bound MT5 Demo account into
/// member evidence. Attribution is conservative: a member is credited only when it was
/// ready and supported the actual executed coordinator side. Opposing/neutral members are
/// not assigned a synthetic loss. Exact deal PnL includes commission, swap and fee.
fn sync_demo_broker_evidence(db: &Connection, t: &Telemetry) -> Result<()> {
    if !t.demo || t.account_trade_mode != Some(0) {
        return Ok(());
    }
    if !table_exists(db, "broker_commands")? || !table_exists(db, "broker_deals")? {
        return Ok(());
    }
    let key = context(t);
    for (raw_cmd, raw_result) in open_coordinator_commands(db, &key)? {
        let cmd: Value = serde_json::from_str(&raw_cmd)?;
        let result: Value = serde_json::from_str(&raw_result)?;
        let Some(open_deal) = result["deal"].as_u64().filter(|v| *v > 0) else {
            continue;
        };
        let position_id: Option<u64> = db
            .query_row(
                "SELECT position_id FROM broker_deals WHERE context=?1 AND ticket=?2",
                params![key, open_deal],
                |r| r.get(0),
            )
            .optional()?;
        let Some(position_id) = position_id.filter(|v| *v > 0) else {
            continue;
        };
        if t.positions
            .iter()
            .any(|p| p.identifier == position_id || p.ticket == position_id)
        {
            continue;
        }

        let mut net = 0.0;
        let mut exit_volume = 0.0;
        let mut closed_ms = 0_u64;
        let mut has_exit = false;
        for raw in position_deals(db, &key, position_id)? {
            let d: Value = serde_json::from_str(&raw)?;
            net += d["profit"].as_f64().unwrap_or(0.0)
                + d["commission"].as_f64().unwrap_or(0.0)
                + d["swap"].as_f64().unwrap_or(0.0)
                + d["fee"].as_f64().unwrap_or(0.0);
            if matches!(d["entry"].as_i64(), Some(1) | Some(3)) {
                has_exit = true;
                let volume = d["volume"].as_f64().unwrap_or(0.0);
                if volume.is_finite() && volume > 0.0 {
                    exit_volume += volume;
                }
                closed_ms = closed_ms.max(d["time_msc"].as_u64().unwrap_or(0));
            }
        }
        if !has_exit || !net.is_finite() || !exit_volume.is_finite() || exit_volume <= 0.0 {
            continue;
        }

        let coordination = &cmd["signal"]["coordination"];
        let Ok(mode) = serde_json::from_value::<TradingMode>(coordination["trading_mode"].clone())
        else {
            continue;
        };
        let Some(members) = coordination["members"].as_array() else {
            continue;
        };
        let Some(executed_side) = cmd["signal"]["side"].as_i64() else {
            continue;
        };
        let net_per_lot = net / exit_volume;

        for member in members {
            if member["ready"] != true || member["side"].as_i64() != Some(executed_side) {
                continue;
            }
            let Some(id) = member["id"].as_str() else {
                continue;
            };
            let Some(timeframe) = member["timeframe"].as_str() else {
                continue;
            };
            let report_id = member["report_id"].as_str().unwrap_or("");
            let source = source_key(&key, mode, id, timeframe, report_id)?;
            let payload = json!({
                "evidence_source":"mt5_demo_broker",
                "context":key,
                "position_id":position_id,
                "brain":id,
                "timeframe":timeframe,
                "report_id":report_id,
                "trade_owner":"system",
                "executed_side":executed_side,
                "open_command_id":cmd["id"],
                "open_deal":open_deal,
                "closed_ms":closed_ms,
                "closed_volume":exit_volume,
                "net":net,
                "net_per_lot":net_per_lot,
                "costs":"exact booked MT5 deal profit + commission + swap + fee"
            });
            db.execute(
                "INSERT OR IGNORE INTO coordinator_broker_samples(source,position_id,context,brain,closed_ms,payload) VALUES(?1,?2,?3,?4,?5,?6)",
                params![source, position_id, key, id, closed_ms, payload.to_string()],
            )?;
        }
    }
    Ok(())
}

pub struct Learner {
    last: BTreeMap<String, u64>,
    weights: BTreeMap<String, LearnedWeight>,
}

impl Learner {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS coordinator_samples(source TEXT NOT NULL,bar INTEGER NOT NULL,payload TEXT NOT NULL,outcome TEXT,PRIMARY KEY(source,bar));
             CREATE INDEX IF NOT EXISTS coordinator_pending ON coordinator_samples(source,outcome);
             CREATE TABLE IF NOT EXISTS coordinator_broker_samples(source TEXT NOT NULL,position_id INTEGER NOT NULL,context TEXT NOT NULL,brain TEXT NOT NULL,closed_ms INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source,position_id));
             CREATE INDEX IF NOT EXISTS coordinator_broker_context ON coordinator_broker_samples(context,brain,closed_ms);",
        )?;
        Ok(Self {
            last: BTreeMap::new(),
            weights: BTreeMap::new(),
        })
    }

    pub fn observe(
        &mut self,
        db: &Connection,
        t: &Telemetry,
        c: &Config,
        brains: &[BrainConfig],
        frames: &BTreeMap<String, Vec<FrameBar>>,
        run: bool,
        now: u64,
    ) -> Result<BTreeMap<String, LearnedWeight>> {
        sync_demo_broker_evidence(db, t)?;
        let mut out = BTreeMap::new();
        let key = context(t);

        for b in brains.iter().filter(|b| b.id != "system") {
            let source = source_key(&key, c.trading.mode, &b.id, &b.timeframe, &b.report_id)?;
            let Some(bars) = frames.get(&b.timeframe).filter(|v| v.len() >= 100) else {
                continue;
            };
            let bar = bars.last().unwrap().candle.time;

            if self.last.get(&source) != Some(&bar) {
                let rows = {
                    let mut st = db.prepare(
                        "SELECT bar,payload FROM coordinator_samples WHERE source=?1 AND outcome IS NULL ORDER BY bar LIMIT 2000",
                    )?;
                    let mapped = st.query_map([&source], |r| {
                        Ok((r.get::<_, u64>(0)?, r.get::<_, String>(1)?))
                    })?;
                    let rows = mapped.collect::<rusqlite::Result<Vec<_>>>()?;
                    rows
                };

                for (old, raw) in rows {
                    let p: Value = serde_json::from_str(&raw)?;
                    let future: Vec<_> = bars.iter().filter(|b| b.candle.time > old).collect();
                    let result = if bars[0].candle.time > old {
                        Some(json!({"state":"expired","reason":"Forward horizon not retained"}))
                    } else if future.len() >= 12 {
                        let f = future[11];
                        let side = p["side"].as_i64().unwrap() as f64;
                        let spread = if f.spread_points > 0 {
                            f.spread_points as f64 * p["point"].as_f64().unwrap()
                        } else {
                            p["spread"].as_f64().unwrap()
                        };
                        let exit = f.candle.close
                            + if side < 0. { spread } else { 0. }
                            - side * p["slippage"].as_f64().unwrap();
                        let change = (exit - p["entry"].as_f64().unwrap()) * side;
                        let net = change / p["tick"].as_f64().unwrap()
                            * if change >= 0. {
                                p["profit"].as_f64().unwrap()
                            } else {
                                p["loss"].as_f64().unwrap()
                            }
                            - p["commission"].as_f64().unwrap();
                        Some(json!({
                            "state":"scored",
                            "net":net,
                            "exit_bar":f.candle.time,
                            "scored_ms":now,
                            "cost_model":"fixed contract; broker bar spread when positive, otherwise frozen entry spread; no swaps"
                        }))
                    } else {
                        None
                    };
                    if let Some(r) = result {
                        db.execute(
                            "UPDATE coordinator_samples SET outcome=?3 WHERE source=?1 AND bar=?2 AND outcome IS NULL",
                            params![source, old, r.to_string()],
                        )?;
                    }
                }
                self.last.insert(source.clone(), bar);
            }

            // Broker history can mature between completed bars, so weights are refreshed on
            // every observe call rather than only when this member's timeframe advances.
            let weight = selected_weight(forward_stats(db, &source)?, broker_stats(db, &source)?);
            self.weights.insert(source.clone(), weight);

            if run {
                let signal = execution::evaluate(b, t, c, frames, genome(db, b, t)?.as_ref(), now);
                if signal.ready
                    && [
                        t.tick_size,
                        t.point,
                        t.tick_value_loss,
                        t.tick_value_profit,
                        t.bid,
                        t.ask,
                    ]
                    .iter()
                    .all(|v| v.is_some_and(|x| x > 0. && x.is_finite()))
                {
                    let side = signal.side as f64;
                    let slip = t.tick_size.unwrap() * c.risk.slippage_ticks;
                    let entry = if side > 0. {
                        t.ask.unwrap() + slip
                    } else {
                        t.bid.unwrap() - slip
                    };
                    let p = json!({
                        "brain":b.id,
                        "timeframe":b.timeframe,
                        "side":signal.side,
                        "entry":entry,
                        "quote_ms":t.quote_time_ms,
                        "observed_ms":now,
                        "spread":t.ask.unwrap()-t.bid.unwrap(),
                        "point":t.point,
                        "tick":t.tick_size,
                        "profit":t.tick_value_profit,
                        "loss":t.tick_value_loss,
                        "slippage":slip,
                        "commission":c.risk.commission_per_lot,
                        "signal":signal
                    });
                    db.execute(
                        "INSERT OR IGNORE INTO coordinator_samples VALUES(?1,?2,?3,NULL)",
                        params![source, bar, p.to_string()],
                    )?;
                }
            }
            out.insert(
                b.id.clone(),
                self.weights.get(&source).cloned().unwrap_or_default(),
            );
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::Candle;

    fn bars(end: u64) -> Vec<FrameBar> {
        (0..180)
            .map(|i| {
                let time = end - (179 - i) * 60;
                let p = 100. + time as f64 / 600.;
                FrameBar {
                    candle: Candle {
                        time,
                        open: p,
                        close: p + 0.1,
                        high: p + 0.3,
                        low: p - 0.3,
                        tick_volume: 100,
                    },
                    spread_points: 1,
                    real_volume: 0,
                }
            })
            .collect()
    }

    #[test]
    fn causal_maturation_idempotence_restart_and_context_isolation() {
        let db = Connection::open_in_memory().unwrap();
        let mut l = Learner::new(&db).unwrap();
        let c = Config::default();
        let b = BrainConfig {
            id: "trend".into(),
            ..Default::default()
        };
        let mut now = 1_800_000_000_000;
        let mut t = Telemetry {
            connected: true,
            symbol: c.symbol.clone(),
            login: 1,
            server: "TEST".into(),
            currency: "USD".into(),
            tick_size: Some(0.01),
            point: Some(0.01),
            tick_value_profit: Some(1.),
            tick_value_loss: Some(1.),
            tick_age_ms: Some(0),
            quote_time_ms: Some(now),
            ..Default::default()
        };
        let mut f = BTreeMap::from([("M1".into(), bars(now / 1000 - 60))]);
        let price = f["M1"].last().unwrap().candle.close;
        t.bid = Some(price);
        t.ask = Some(price + 0.01);
        let w = l.observe(&db, &t, &c, &[b.clone()], &f, true, now).unwrap();
        assert_eq!(w["trend"].samples, 0);
        l.observe(&db, &t, &c, &[b.clone()], &f, true, now).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM coordinator_samples", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            1
        );
        now += 11 * 60_000;
        t.quote_time_ms = Some(now);
        f.insert("M1".into(), bars(now / 1000 - 60));
        assert_eq!(
            l.observe(&db, &t, &c, &[b.clone()], &f, false, now).unwrap()["trend"].samples,
            0
        );
        now += 60_000;
        t.quote_time_ms = Some(now);
        f.insert("M1".into(), bars(now / 1000 - 60));
        let w = l.observe(&db, &t, &c, &[b.clone()], &f, false, now).unwrap();
        assert_eq!(w["trend"].samples, 1);
        assert_eq!(w["trend"].wins, 1);
        assert_eq!(w["trend"].evidence_source, "forward_bars");
        let mut restarted = Learner::new(&db).unwrap();
        assert_eq!(
            restarted
                .observe(&db, &t, &c, &[b.clone()], &f, false, now)
                .unwrap()["trend"]
                .samples,
            1
        );
        t.login = 2;
        assert_eq!(
            restarted
                .observe(&db, &t, &c, &[b], &f, false, now)
                .unwrap()["trend"]
                .samples,
            0
        );
    }

    #[test]
    fn closed_demo_coordinator_trade_becomes_exact_broker_evidence() {
        let db = Connection::open_in_memory().unwrap();
        let mut l = Learner::new(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');
             CREATE TABLE broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,ticket));",
        )
        .unwrap();
        let c = Config::default();
        let b = BrainConfig {
            id: "trend".into(),
            ..Default::default()
        };
        let now = 1_800_000_000_000;
        let mut t = Telemetry {
            connected: true,
            demo: true,
            account_trade_mode: Some(0),
            symbol: c.symbol.clone(),
            login: 77,
            server: "DEMO".into(),
            currency: "USD".into(),
            tick_size: Some(0.01),
            point: Some(0.01),
            tick_value_profit: Some(1.),
            tick_value_loss: Some(1.),
            tick_age_ms: Some(0),
            quote_time_ms: Some(now),
            ..Default::default()
        };
        let f = BTreeMap::from([("M1".into(), bars(now / 1000 - 60))]);
        let price = f["M1"].last().unwrap().candle.close;
        t.bid = Some(price);
        t.ask = Some(price + 0.01);
        let key = context(&t);
        let command = json!({
            "id":"cmd1",
            "signal":{
                "side":1,
                "coordination":{
                    "trading_mode":"expert",
                    "members":[
                        {"id":"trend","timeframe":"M1","report_id":"","side":1,"ready":true},
                        {"id":"gann","timeframe":"M1","report_id":"","side":-1,"ready":true}
                    ]
                }
            }
        });
        db.execute(
            "INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result) VALUES('cmd1',?1,'system','open',1,'observed',?2,?3)",
            params![key, command.to_string(), json!({"deal":100,"state":"filled"}).to_string()],
        )
        .unwrap();
        let open = json!({
            "ticket":100,"position_id":900,"magic":410600,"symbol":"XAUUSD","entry":0,
            "time_msc":now-60_000,"volume":0.10,"price":2000.0,"profit":0.0,
            "commission":-0.35,"swap":0.0,"fee":0.0,"brain":"system"
        });
        let close = json!({
            "ticket":101,"position_id":900,"magic":410600,"symbol":"XAUUSD","entry":1,
            "time_msc":now-1_000,"volume":0.10,"price":2000.2,"profit":2.0,
            "commission":-0.35,"swap":-0.10,"fee":-0.05,"brain":"system"
        });
        db.execute(
            "INSERT INTO broker_deals VALUES(?1,100,900,'system',?2)",
            params![key, open.to_string()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO broker_deals VALUES(?1,101,900,'system',?2)",
            params![key, close.to_string()],
        )
        .unwrap();

        let w = l.observe(&db, &t, &c, &[b], &f, false, now).unwrap();
        let trend = &w["trend"];
        assert_eq!(trend.broker_samples, 1);
        assert_eq!(trend.samples, 1);
        assert_eq!(trend.wins, 1);
        assert_eq!(trend.forward_samples, 0);
        assert_eq!(trend.evidence_source, "mt5_demo_broker");
        assert!((trend.mean_net_per_lot - 11.5).abs() < 1e-9);
        assert!(trend.weight() > 1.0);
        let count: u64 = db
            .query_row("SELECT COUNT(*) FROM coordinator_broker_samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        l.observe(
            &db,
            &t,
            &c,
            &[BrainConfig {
                id: "trend".into(),
                ..Default::default()
            }],
            &f,
            false,
            now + 1,
        )
        .unwrap();
        let count2: u64 = db
            .query_row("SELECT COUNT(*) FROM coordinator_broker_samples", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count2, 1);
    }
}