//! Account-scoped frame ingestion, asynchronous strategy learning and prospective Wyckoff scores.
use anyhow::{bail, Context, Result};
use braintrade_domain::{
    studio::{self, Frame, LabReport},
    trading, wyckoff, Candle, Config, RiskConfig, Status, Telemetry,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};
fn context(t: &Telemetry) -> String {
    serde_json::to_string(&(&t.server, t.login, &t.symbol, &t.currency)).unwrap()
}
struct Worker {
    rx: mpsc::Receiver<std::result::Result<LabReport, String>>,
    cancel: Arc<AtomicBool>,
    frame: Frame,
}
pub struct Service {
    frames: BTreeMap<(String, String), Frame>,
    queue: VecDeque<(Frame, RiskConfig)>,
    worker: Option<Worker>,
    last_error: String,
    auto: bool,
    next_request: u64,
    cursor: usize,
}
impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS studio_frames(context TEXT NOT NULL,timeframe TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,timeframe));
CREATE TABLE IF NOT EXISTS studio_runs(id TEXT PRIMARY KEY,context TEXT NOT NULL,timeframe TEXT NOT NULL,test_end INTEGER NOT NULL,created_ms INTEGER NOT NULL,payload TEXT NOT NULL,dataset TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS studio_settings(id INTEGER PRIMARY KEY CHECK(id=1),auto INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS wyckoff_observations(context TEXT NOT NULL,timeframe TEXT NOT NULL,signal_time INTEGER NOT NULL,observed_ms INTEGER NOT NULL,payload TEXT NOT NULL,outcome TEXT,PRIMARY KEY(context,timeframe,signal_time));")?;
        let mut frames = BTreeMap::new();
        let mut q = db.prepare("SELECT context,timeframe,payload FROM studio_frames")?;
        for row in q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (k, tf, p) = row?;
            frames.insert((k, tf), serde_json::from_str(&p)?);
        }
        let auto = db
            .query_row("SELECT auto FROM studio_settings WHERE id=1", [], |r| {
                r.get::<_, bool>(0)
            })
            .optional()?
            .unwrap_or(false);
        Ok(Self {
            frames,
            queue: VecDeque::new(),
            worker: None,
            last_error: String::new(),
            auto,
            next_request: 0,
            cursor: 0,
        })
    }
    pub fn live_frames(&self, t: &Telemetry, now: u64) -> BTreeMap<String, Vec<Candle>> {
        self.frames
            .iter()
            .filter(|((k, _), f)| k == &context(t) && f.live(now))
            .map(|((_, tf), f)| {
                (
                    tf.clone(),
                    f.bars.iter().map(|b| b.candle.clone()).collect(),
                )
            })
            .collect()
    }
    pub fn cancel(&mut self) {
        self.queue.clear();
        if let Some(w) = &self.worker {
            w.cancel.store(true, Ordering::Relaxed);
        }
        self.auto = false;
    }
    fn unseen(db: &Connection, f: &Frame) -> Result<bool> {
        if f.bars.len() < 400 || !f.error.is_empty() {
            return Ok(false);
        }
        let end: Option<u64> = db.query_row(
            "SELECT MAX(test_end) FROM studio_runs WHERE context=?1 AND timeframe=?2",
            params![f.context(), f.timeframe],
            |r| r.get(0),
        )?;
        Ok(end.is_none_or(|end| f.bars[f.bars.len() * 8 / 10 + 24].candle.time > end))
    }
    fn enqueue(&mut self, db: &Connection, f: &Frame, risk: &RiskConfig) -> Result<bool> {
        if !Self::unseen(db, f)?
            || self
                .queue
                .iter()
                .any(|(v, _)| v.context() == f.context() && v.timeframe == f.timeframe)
            || self.worker.as_ref().is_some_and(|w| {
                w.frame.context() == f.context() && w.frame.timeframe == f.timeframe
            })
        {
            return Ok(false);
        }
        self.queue.push_back((f.clone(), risk.clone()));
        Ok(true)
    }
    pub fn tick(&mut self, db: &Connection, c: &Config, now: u64) {
        if let Err(e) = self.advance(db, c, now) {
            self.last_error = e.to_string();
        }
    }
    fn advance(&mut self, db: &Connection, _c: &Config, now: u64) -> Result<()> {
        if let Some(w) = self.worker.as_ref() {
            match w.rx.try_recv() {
                Ok(result) => {
                    let w = self.worker.take().unwrap();
                    if w.cancel.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    match result {
                        Ok(r) => {
                            db.execute("INSERT INTO studio_runs(id,context,timeframe,test_end,created_ms,payload,dataset) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![uuid::Uuid::new_v4().to_string(),r.context,r.timeframe,r.test_end,now,serde_json::to_string(&r)?,serde_json::to_string(&w.frame)?])?;
                        }
                        Err(e) => self.last_error = e,
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.last_error = "Research worker stopped".into();
                }
            }
        }
        if let Some((frame, risk)) = self.queue.pop_front() {
            if !Self::unseen(db, &frame)? {
                return Ok(());
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            let input = frame.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(studio::learn(&input, &risk, &flag));
            });
            self.worker = Some(Worker { rx, cancel, frame });
        }
        Ok(())
    }
    fn observe(
        &self,
        db: &Connection,
        f: &Frame,
        t: &Telemetry,
        c: &Config,
        now: u64,
    ) -> Result<()> {
        if !f.error.is_empty() || f.bars.len() < 100 {
            return Ok(());
        }
        let key = f.context();
        let mut q=db.prepare("SELECT signal_time,payload FROM wyckoff_observations WHERE context=?1 AND timeframe=?2 AND outcome IS NULL")?;
        let pending = q
            .query_map(params![key, f.timeframe], |r| {
                Ok((r.get::<_, u64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (time, raw) in pending {
            let p: Value = serde_json::from_str(&raw)?;
            let boundary = p["forward_from"]
                .as_u64()
                .context("Missing forward boundary")?;
            let future: Vec<_> = f
                .bars
                .iter()
                .filter(|b| b.candle.time >= boundary)
                .collect();
            let outcome = if f.bars[0].candle.time > boundary {
                Some(json!({"state":"expired","reason":"Forward horizon was not observed"}))
            } else if future.len() >= 12 {
                let b = future[11];
                let side = p["side"].as_i64().unwrap() as f64;
                let slip = p["slip"].as_f64().unwrap();
                let exit = b.candle.close
                    + if side < 0. {
                        b.spread_points as f64 * p["point"].as_f64().unwrap() + slip
                    } else {
                        -slip
                    };
                let change = (exit - p["entry"].as_f64().unwrap()) * side;
                let net = change / p["tick"].as_f64().unwrap()
                    * if change >= 0. {
                        p["profit"].as_f64().unwrap()
                    } else {
                        p["loss"].as_f64().unwrap()
                    }
                    - p["commission"].as_f64().unwrap();
                Some(
                    json!({"state":"scored","net_per_lot":net,"exit":exit,"exit_bar":b.candle.time,"scored_ms":now,"horizon_closed_bars":12,"model":"fixed_horizon_quote_entry_ohlc_exit_no_swaps"}),
                )
            } else {
                None
            };
            if let Some(o) = outcome {
                db.execute("UPDATE wyckoff_observations SET outcome=?4 WHERE context=?1 AND timeframe=?2 AND signal_time=?3 AND outcome IS NULL",params![key,f.timeframe,time,o.to_string()])?;
            }
        }
        let w = wyckoff::analyze(&f.bars);
        if w.confirmed && f.live(now) && trading::costs(t, c, 1., now).is_ok() {
            let slip = f.contract.tick_size * c.risk.slippage_ticks;
            let entry = if w.side == 1 {
                t.ask.unwrap() + slip
            } else {
                t.bid.unwrap() - slip
            };
            let p = json!({"analysis":w,"side":w.side,"entry":entry,"forward_from":f.current_open,"tick":f.contract.tick_size,"point":f.contract.point,"profit":f.contract.tick_profit,"loss":f.contract.tick_loss,"slip":slip,"commission":c.risk.commission_per_lot,"entry_quote_ms":t.quote_time_ms});
            db.execute("INSERT OR IGNORE INTO wyckoff_observations(context,timeframe,signal_time,observed_ms,payload) VALUES(?1,?2,?3,?4,?5)",params![key,f.timeframe,w.signal_time,now,p.to_string()])?;
        }
        Ok(())
    }
    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        db: &Connection,
        c: &Config,
        s: &Status,
        now: u64,
    ) -> Result<Value> {
        let t = s
            .telemetry
            .as_ref()
            .filter(|t| t.connected && s.gateway_fresh && t.symbol == c.symbol);
        match (method, path) {
            ("GET", "/v1/gateway/research/request") => {
                let poll_ms = if c.trading.mode == trading::TradingMode::Sniper {
                    250
                } else {
                    1000
                };
                if now < self.next_request || t.is_none() {
                    return Ok(json!({"job":null,"poll_ms":poll_ms}));
                }
                self.next_request = now + 3000;
                let t = t.unwrap();
                let tf = studio::TIMEFRAMES[self.cursor % 21];
                self.cursor += 1;
                Ok(
                    json!({"job":{"login":t.login,"server":t.server,"symbol":t.symbol,"currency":t.currency,"timeframe":tf},"poll_ms":poll_ms}),
                )
            }
            ("POST", "/v1/gateway/research/frame") => {
                let t = t.context("Current account unavailable")?;
                let f: Frame = serde_json::from_slice(body)?;
                f.validate(now).map_err(anyhow::Error::msg)?;
                if f.context() != context(t) {
                    bail!("Account changed; frame rejected");
                }
                if let Some(old) = self.frames.get(&(f.context(), f.timeframe.clone())) {
                    if f.captured_ms < old.captured_ms
                        || (f.error.is_empty()
                            && old.error.is_empty()
                            && f.current_open < old.current_open)
                    {
                        bail!("Out-of-order frame rejected");
                    }
                }
                for (saved, current) in [
                    (f.contract.point, t.point),
                    (f.contract.tick_size, t.tick_size),
                    (f.contract.tick_loss, t.tick_value_loss),
                    (f.contract.tick_profit, t.tick_value_profit),
                ] {
                    if current.is_none_or(|v| (v - saved).abs() > saved * 1e-8) {
                        bail!("Broker contract changed");
                    }
                }
                self.observe(db, &f, t, c, now)?;
                db.execute("INSERT INTO studio_frames(context,timeframe,payload) VALUES(?1,?2,?3) ON CONFLICT(context,timeframe) DO UPDATE SET payload=excluded.payload",params![f.context(),f.timeframe,serde_json::to_string(&f)?])?;
                if self.auto {
                    self.enqueue(db, &f, &c.risk)?;
                }
                self.frames.insert((f.context(), f.timeframe.clone()), f);
                Ok(json!({"ok":true}))
            }
            ("POST", "/v1/studio/run") => {
                let t = t.context("Connect MT5 before research")?;
                let v: Value = serde_json::from_slice(body)?;
                let tf = v["timeframe"].as_str().unwrap_or("ALL");
                if tf != "ALL" && !studio::TIMEFRAMES.contains(&tf) {
                    bail!("Unknown timeframe");
                }
                let frames: Vec<_> = self
                    .frames
                    .iter()
                    .filter(|((k, name), _)| k == &context(t) && (tf == "ALL" || tf == name))
                    .map(|(_, f)| f.clone())
                    .collect();
                let mut queued = 0;
                let mut skipped = vec![];
                for f in frames {
                    if self.enqueue(db, &f, &c.risk)? {
                        queued += 1;
                    } else {
                        skipped.push(format!(
                            "{}: insufficient bars, queued, or no fresh final test",
                            f.timeframe
                        ));
                    }
                }
                Ok(
                    json!({"queued":queued,"skipped":skipped,"note":"Exposed final tests cannot be reused for selection"}),
                )
            }
            ("POST", "/v1/studio/auto") => {
                let v: Value = serde_json::from_slice(body)?;
                self.auto = v["enabled"].as_bool().context("enabled must be boolean")?;
                db.execute("INSERT INTO studio_settings(id,auto) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET auto=excluded.auto",[self.auto])?;
                Ok(json!({"auto":self.auto}))
            }
            ("POST", "/v1/studio/cancel") => {
                self.cancel();
                db.execute("INSERT INTO studio_settings(id,auto) VALUES(1,0) ON CONFLICT(id) DO UPDATE SET auto=0",[])?;
                Ok(json!({"cancelled":true,"auto":false}))
            }
            ("POST", "/v1/studio/report") => {
                let v: Value = serde_json::from_slice(body)?;
                let id = v["id"].as_str().context("Report id required")?;
                let raw: String =
                    db.query_row("SELECT payload FROM studio_runs WHERE id=?1", [id], |r| {
                        r.get(0)
                    })?;
                Ok(serde_json::from_str(&raw)?)
            }
            ("GET", "/v1/studio") => {
                let key = t.map(context);
                let mut frames = vec![];
                for tf in studio::TIMEFRAMES {
                    let f = key
                        .as_ref()
                        .and_then(|k| self.frames.get(&(k.clone(), tf.into())));
                    frames.push(match f{Some(f)=>json!({"timeframe":tf,"bars":f.bars.len(),"captured_ms":f.captured_ms,"live":f.live(now),"error":f.error,"wyckoff":wyckoff::analyze(&f.bars)}),None=>json!({"timeframe":tf,"bars":0,"live":false,"error":"Waiting for MT5 history"})});
                }
                let mut q=db.prepare("SELECT id,created_ms,payload FROM studio_runs WHERE context=?1 ORDER BY created_ms DESC LIMIT 60")?;
                let reports=q.query_map([key.as_deref().unwrap_or("")],|r|{let raw:String=r.get(2)?;Ok(json!({"id":r.get::<_,String>(0)?,"created_ms":r.get::<_,u64>(1)?,"report":serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null)}))})?.collect::<std::result::Result<Vec<_>,_>>()?;
                let mut q=db.prepare("SELECT timeframe,COUNT(*),SUM(CASE WHEN outcome IS NULL THEN 1 ELSE 0 END),SUM(CASE WHEN json_extract(outcome,'$.state')='scored' THEN 1 ELSE 0 END),SUM(CASE WHEN json_extract(outcome,'$.net_per_lot')>0 THEN 1 ELSE 0 END),AVG(json_extract(outcome,'$.net_per_lot')) FROM wyckoff_observations WHERE context=?1 GROUP BY timeframe")?;
                let learning=q.query_map([key.as_deref().unwrap_or("")],|r|{let n:i64=r.get(3)?;let wins:i64=r.get(4)?;let mean:Option<f64>=r.get(5)?;Ok(json!({"timeframe":r.get::<_,String>(0)?,"observations":r.get::<_,i64>(1)?,"pending":r.get::<_,i64>(2)?,"scored":n,"wins":wins,"smoothed_win_rate":(wins+1) as f64/(n+2) as f64,"mean_net_per_lot":mean,"research_evidence_positive":n>=20&&mean.is_some_and(|v|v>0.),"note":"Prospective 12-bar outcomes; frequency is not calibrated confidence"}))})?.collect::<std::result::Result<Vec<_>,_>>()?;
                Ok(
                    json!({"context":key,"connected":t.is_some(),"auto":self.auto,"working":self.worker.as_ref().map(|w|&w.frame.timeframe),"queued":self.queue.len(),"last_error":self.last_error,"frames":frames,"reports":reports,"wyckoff_learning":learning}),
                )
            }
            _ => bail!("Unknown studio route"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{backtest::Contract, studio::FrameBar};
    fn fixture() -> (Frame, Telemetry, Config) {
        let now = 1_800_000_000_000u64;
        let boundary = now / 60_000 * 60;
        let mut bars: Vec<_> = (0..110)
            .map(|i| {
                let p = 100. + (i as f64 * 0.5).sin() * 1.7;
                FrameBar {
                    candle: Candle {
                        time: boundary - (110 - i) * 60,
                        open: p,
                        high: p + 0.3,
                        low: p - 0.3,
                        close: p + 0.1,
                        tick_volume: 100,
                    },
                    spread_points: 2,
                    real_volume: 0,
                }
            })
            .collect();
        bars[108].candle = Candle {
            time: boundary - 120,
            open: 100.,
            high: 101.,
            low: 97.,
            close: 99.,
            tick_volume: 200,
        };
        bars[109].candle = Candle {
            time: boundary - 60,
            open: 98.2,
            high: 100.,
            low: 98.1,
            close: 99.8,
            tick_volume: 80,
        };
        let f = Frame {
            login: 1,
            server: "S".into(),
            symbol: "XAUUSD".into(),
            currency: "USD".into(),
            timeframe: "M1".into(),
            captured_ms: now,
            current_open: boundary,
            contract: Contract {
                point: 0.01,
                tick_size: 0.01,
                tick_loss: 1.,
                tick_profit: 1.,
                volume_min: 0.01,
                volume_step: 0.01,
                volume_max: 100.,
            },
            bars,
            error: String::new(),
        };
        let t = Telemetry {
            connected: true,
            login: 1,
            server: "S".into(),
            symbol: "XAUUSD".into(),
            currency: "USD".into(),
            bid: Some(100.),
            ask: Some(100.02),
            point: Some(0.01),
            tick_size: Some(0.01),
            tick_value_loss: Some(1.),
            tick_value_profit: Some(1.),
            tick_age_ms: Some(0),
            quote_time_ms: Some(now),
            ..Default::default()
        };
        (f, t, Config::default())
    }
    #[test]
    fn wyckoff_outcomes_are_prospective_unique_and_persisted() {
        let db = Connection::open_in_memory().unwrap();
        let service = Service::new(&db).unwrap();
        let (mut f, t, c) = fixture();
        assert!(wyckoff::analyze(&f.bars).confirmed);
        service.observe(&db, &f, &t, &c, f.captured_ms).unwrap();
        service.observe(&db, &f, &t, &c, f.captured_ms).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM wyckoff_observations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        let restarted = Service::new(&db).unwrap();
        for i in 0..12 {
            let mut b = f.bars.last().unwrap().clone();
            b.candle.time = f.current_open + i * 60;
            b.candle.open = 101.;
            b.candle.low = 100.5;
            b.candle.high = 102.;
            b.candle.close = 101.5;
            f.bars.push(b);
            restarted
                .observe(&db, &f, &t, &c, f.captured_ms + (i + 1) * 60000)
                .unwrap();
            let scored: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM wyckoff_observations WHERE outcome IS NOT NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(scored, if i == 11 { 1 } else { 0 });
        }
        let raw: String = db
            .query_row("SELECT outcome FROM wyckoff_observations", [], |r| r.get(0))
            .unwrap();
        let o: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(o["state"], "scored");
        assert!((o["net_per_lot"].as_f64().unwrap() - 137.).abs() < 1e-7);
    }
}
