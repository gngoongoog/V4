//! Causal per-member learning for the coordinator.
//! Forward-bar evidence is retained as a fallback, while exact closed MT5 Demo broker
//! outcomes are preferred when available. Broker evidence is never synthesized.
use anyhow::Result;
use braintrade_domain::{
    execution::{self, BrainConfig, IntelligenceContext, LearnedWeight},
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
    mean_reward: f64,
    effective_samples: f64,
    weighted_win_rate: f64,
}

const FORWARD_HALF_LIFE_MS: f64 = 14.0 * 24.0 * 60.0 * 60.0 * 1000.0;
const BROKER_HALF_LIFE_MS: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 1000.0;

fn decay_weight(observed_ms:u64,now:u64,half_life_ms:f64)->f64{
    if observed_ms==0||now<=observed_ms{return 1.0;}
    (0.5_f64.powf(now.saturating_sub(observed_ms) as f64/half_life_ms)).max(0.05)
}
fn reward_v2(net:f64,risk:f64)->(f64,f64){
    if risk.is_finite()&&risk>0.&&net.is_finite(){let r=net/risk;(r,r.tanh())}
    else if net>0.{(0.,0.25)}else if net<0.{(0.,-0.25)}else{(0.,0.)}
}
fn finish_stats(samples:u64,wins:u64,weighted_wins:f64,weighted_net:f64,weighted_reward:f64,effective:f64)->Stats{
    Stats{samples,wins,mean_net_per_lot:if effective>0.{weighted_net/effective}else{0.},mean_reward:if effective>0.{weighted_reward/effective}else{0.},effective_samples:effective,weighted_win_rate:if effective>0.{weighted_wins/effective}else{0.}}
}
fn forward_stats(db:&Connection,source:&str,regime:Option<&str>,now:u64)->Result<Stats>{
    let mut st=db.prepare("SELECT payload,outcome FROM coordinator_samples WHERE source=?1 AND json_extract(outcome,'$.state')='scored' ORDER BY bar DESC LIMIT 4000")?;
    let rows=st.query_map([source],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut samples,mut wins,mut weighted_wins,mut weighted_net,mut weighted_reward,mut effective)=(0u64,0u64,0.,0.,0.,0.);
    for(raw_p,raw_o)in rows{let p:Value=serde_json::from_str(&raw_p)?;let o:Value=serde_json::from_str(&raw_o)?;let sample_regime=p["regime"].as_str().unwrap_or("legacy");if regime.is_some_and(|wanted|wanted!=sample_regime){continue;}let net=o["net"].as_f64().unwrap_or(0.);let reward=o["reward"].as_f64().unwrap_or_else(||reward_v2(net,0.).1);let ts=o["scored_ms"].as_u64().or_else(||p["observed_ms"].as_u64()).unwrap_or(now);let w=decay_weight(ts,now,FORWARD_HALF_LIFE_MS);samples+=1;wins+=u64::from(net>0.);if net>0.{weighted_wins+=w;}weighted_net+=net*w;weighted_reward+=reward*w;effective+=w;}
    Ok(finish_stats(samples,wins,weighted_wins,weighted_net,weighted_reward,effective))
}
fn broker_stats(db:&Connection,source:&str,regime:Option<&str>,now:u64)->Result<Stats>{
    let mut st=db.prepare("SELECT closed_ms,payload FROM coordinator_broker_samples WHERE source=?1 ORDER BY closed_ms DESC LIMIT 4000")?;
    let rows=st.query_map([source],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut samples,mut wins,mut weighted_wins,mut weighted_net,mut weighted_reward,mut effective)=(0u64,0u64,0.,0.,0.,0.);
    for(ts,raw)in rows{let p:Value=serde_json::from_str(&raw)?;let sample_regime=p["regime"].as_str().unwrap_or("legacy");if regime.is_some_and(|wanted|wanted!=sample_regime){continue;}let net=p["net_per_lot"].as_f64().unwrap_or(0.);let reward=p["reward"].as_f64().unwrap_or_else(||reward_v2(net,0.).1);let w=decay_weight(ts,now,BROKER_HALF_LIFE_MS);samples+=1;wins+=u64::from(net>0.);if net>0.{weighted_wins+=w;}weighted_net+=net*w;weighted_reward+=reward*w;effective+=w;}
    Ok(finish_stats(samples,wins,weighted_wins,weighted_net,weighted_reward,effective))
}
fn confidence(effective:f64,broker:bool)->f64{if effective<=0.{0.}else{let scale=if broker{8.}else{20.};(1.-(-effective/scale).exp()).clamp(0.,1.)}}
fn selected_weight(forward_global:Stats,forward_regime:Stats,broker_global:Stats,broker_regime:Stats,intelligence:&IntelligenceContext)->LearnedWeight{
    let(mut chosen,broker,evidence)=if broker_global.samples>0{if broker_regime.samples>0{(broker_regime.clone(),true,"mt5_demo_broker_regime")}else{(broker_global.clone(),true,"mt5_demo_broker_global")}}else if forward_regime.samples>=5{(forward_regime.clone(),false,"forward_bars_regime")}else{(forward_global.clone(),false,"forward_bars_global")};
    if chosen.effective_samples<=0.&&chosen.samples>0{chosen.effective_samples=chosen.samples as f64;}
    LearnedWeight{samples:chosen.samples,wins:chosen.wins,mean_net_per_lot:chosen.mean_net_per_lot,mean_reward:chosen.mean_reward,broker_samples:broker_global.samples,forward_samples:forward_global.samples,regime_samples:if broker{broker_regime.samples}else{forward_regime.samples},effective_samples:chosen.effective_samples,weighted_win_rate:chosen.weighted_win_rate,confidence:confidence(chosen.effective_samples,broker),regime:intelligence.regime.clone(),session:intelligence.session.clone(),direction:intelligence.direction,atr:intelligence.atr,volatility_ratio:intelligence.volatility_ratio,spread_to_atr:intelligence.spread_to_atr,execution_quality:intelligence.execution_quality,execution_history_quality:intelligence.execution_history_quality,quote_freshness:intelligence.quote_freshness,evidence_source:evidence.into()}
}


fn historical_execution_quality(db:&Connection,key:&str,now:u64)->Result<f64>{
    if !table_exists(db,"broker_commands")?{return Ok(0.5);}
    let mut st=db.prepare("SELECT created_ms,payload,result FROM broker_commands WHERE context=?1 AND brain='system' AND kind='open' AND result<>'{}' ORDER BY created_ms DESC LIMIT 200")?;
    let rows=st.query_map([key],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut weighted,mut mass)=(0.,0.);
    for(ts,raw_cmd,raw_result)in rows{
        let cmd:Value=serde_json::from_str(&raw_cmd)?;let result:Value=serde_json::from_str(&raw_result)?;
        let state=result["state"].as_str().unwrap_or("");
        let state_score=match state{"filled"=>1.0,"partial"=>0.75,"placed"=>0.80,"rejected"=>0.05,"unknown"=>0.20,_=>continue};
        let requested=result["request"]["price"].as_f64().unwrap_or(0.);let filled=result["price"].as_f64().unwrap_or(0.);let stop=cmd["signal"]["stop_distance"].as_f64().unwrap_or(0.);
        let slip_ratio=if requested>0.&&filled>0.&&stop>0.{((filled-requested).abs()/stop).clamp(0.,1.)}else{0.};
        let slippage_score=(1.-slip_ratio).clamp(0.,1.);
        let q=result["queue_to_send_ms"].as_f64().unwrap_or(20.);let latency_score=(1./(1.+((q-20.).max(0.)/80.))).clamp(0.2,1.0);
        let score=(0.70*state_score+0.20*slippage_score+0.10*latency_score).clamp(0.,1.);
        let w=decay_weight(ts,now,7.*24.*60.*60.*1000.);weighted+=score*w;mass+=w;
    }
    if mass<=0.{return Ok(0.5);}let observed=(weighted/mass).clamp(0.,1.);let confidence=(1.-(-mass/10.).exp()).clamp(0.,1.);Ok((0.5+(observed-0.5)*confidence).clamp(0.,1.))
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

/// Convert exact closed coordinator Demo trades into Reward V2 broker evidence.
/// Opposing members are never assigned synthetic broker losses; their ideas stay in the
/// causal counterfactual ledger and mature from later completed bars.
fn sync_demo_broker_evidence(db:&Connection,t:&Telemetry)->Result<()> {
    if !t.demo||t.account_trade_mode!=Some(0){return Ok(());}if !table_exists(db,"broker_commands")?||!table_exists(db,"broker_deals")?{return Ok(());}let key=context(t);
    for(raw_cmd,raw_result)in open_coordinator_commands(db,&key)?{let cmd:Value=serde_json::from_str(&raw_cmd)?;let result:Value=serde_json::from_str(&raw_result)?;let Some(open_deal)=result["deal"].as_u64().filter(|v|*v>0)else{continue;};let position_id:Option<u64>=db.query_row("SELECT position_id FROM broker_deals WHERE context=?1 AND ticket=?2",params![&key,open_deal],|r|r.get(0)).optional()?;let Some(position_id)=position_id.filter(|v|*v>0)else{continue;};if t.positions.iter().any(|p|p.identifier==position_id||p.ticket==position_id){continue;}
        let(mut net,mut exit_volume,mut closed_ms,mut has_exit)=(0.,0.,0u64,false);for raw in position_deals(db,&key,position_id)?{let d:Value=serde_json::from_str(&raw)?;net+=d["profit"].as_f64().unwrap_or(0.)+d["commission"].as_f64().unwrap_or(0.)+d["swap"].as_f64().unwrap_or(0.)+d["fee"].as_f64().unwrap_or(0.);if matches!(d["entry"].as_i64(),Some(1)|Some(3)){has_exit=true;let v=d["volume"].as_f64().unwrap_or(0.);if v.is_finite()&&v>0.{exit_volume+=v;}closed_ms=closed_ms.max(d["time_msc"].as_u64().unwrap_or(0));}}
        if !has_exit||!net.is_finite()||!exit_volume.is_finite()||exit_volume<=0.{continue;}let coordination=&cmd["signal"]["coordination"];let Ok(mode)=serde_json::from_value::<TradingMode>(coordination["trading_mode"].clone())else{continue;};let Some(members)=coordination["members"].as_array()else{continue;};let Some(executed_side)=cmd["signal"]["side"].as_i64()else{continue;};let net_per_lot=net/exit_volume;let intel=&coordination["intelligence"];let regime=intel["regime"].as_str().unwrap_or("legacy");let session=intel["session"].as_str().unwrap_or("unknown");let execution_quality=intel["execution_quality"].as_f64().unwrap_or(0.5);let stop=cmd["signal"]["stop_distance"].as_f64().unwrap_or(0.);let risk_money=t.tick_size.zip(t.tick_value_loss).filter(|(tick,value)|*tick>0.&&*value>0.).map(|(tick,value)|stop/tick*value*exit_volume).filter(|v|v.is_finite()&&*v>0.).unwrap_or(0.);let(r_multiple,reward)=reward_v2(net,risk_money);
        for member in members{if member["ready"]!=true{continue;}let Some(id)=member["id"].as_str()else{continue;};let Some(timeframe)=member["timeframe"].as_str()else{continue;};let report_id=member["report_id"].as_str().unwrap_or("");let source=source_key(&key,mode,id,timeframe,report_id)?;let bar=member["bar_time"].as_u64().unwrap_or(0);let supported=member["side"].as_i64()==Some(executed_side);if bar>0{db.execute("UPDATE coordinator_counterfactuals SET executed_position_id=?3,executed_side=?4,supported_execution=?5 WHERE source=?1 AND bar=?2",params![&source,bar,position_id,executed_side,if supported{1_i64}else{0_i64}])?;}if member["side"].as_i64()!=Some(executed_side){continue;}let payload=json!({"evidence_source":"mt5_demo_broker","context":key,"position_id":position_id,"brain":id,"timeframe":timeframe,"report_id":report_id,"trade_owner":"system","executed_side":executed_side,"open_command_id":cmd["id"],"open_deal":open_deal,"closed_ms":closed_ms,"closed_volume":exit_volume,"net":net,"net_per_lot":net_per_lot,"risk_money":risk_money,"r_multiple":r_multiple,"reward":reward,"regime":regime,"session":session,"execution_quality":execution_quality,"costs":"exact booked MT5 deal profit + commission + swap + fee"});db.execute("INSERT OR IGNORE INTO coordinator_broker_samples(source,position_id,context,brain,closed_ms,payload) VALUES(?1,?2,?3,?4,?5,?6)",params![&source,position_id,&key,id,closed_ms,payload.to_string()])?;}
    }Ok(())
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
             CREATE INDEX IF NOT EXISTS coordinator_broker_context ON coordinator_broker_samples(context,brain,closed_ms);
             CREATE TABLE IF NOT EXISTS coordinator_counterfactuals(source TEXT NOT NULL,bar INTEGER NOT NULL,context TEXT NOT NULL,brain TEXT NOT NULL,regime TEXT NOT NULL,side INTEGER NOT NULL,observed_ms INTEGER NOT NULL,outcome TEXT,executed_position_id INTEGER,executed_side INTEGER,supported_execution INTEGER,PRIMARY KEY(source,bar));
             CREATE INDEX IF NOT EXISTS coordinator_counterfactual_context ON coordinator_counterfactuals(context,brain,regime,bar);",
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
        let mut intelligence = execution::intelligence_context(t, frames, now);
        intelligence.execution_history_quality=historical_execution_quality(db,&key,now)?;
        intelligence.execution_quality=(0.70*intelligence.execution_quality+0.30*intelligence.execution_history_quality).clamp(0.,1.);

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
                        let stop=p["signal"]["stop_distance"].as_f64().unwrap_or(0.);
                        let tick=p["tick"].as_f64().unwrap_or(0.);
                        let value=p["loss"].as_f64().unwrap_or(0.);
                        let commission=p["commission"].as_f64().unwrap_or(0.);
                        let risk=if tick>0.&&value>0.{stop/tick*value+commission.abs()}else{0.};
                        let(r_multiple,reward)=reward_v2(net,risk);
                        Some(json!({"state":"scored","net":net,"risk_per_lot":risk,"r_multiple":r_multiple,"reward":reward,"exit_bar":f.candle.time,"scored_ms":now,"cost_model":"fixed contract; broker bar spread when positive, otherwise frozen entry spread; configured slippage and commission; no swaps"}))
                    } else {
                        None
                    };
                    if let Some(r) = result {
                        db.execute("UPDATE coordinator_samples SET outcome=?3 WHERE source=?1 AND bar=?2 AND outcome IS NULL",params![&source,old,r.to_string()])?;
                        db.execute("UPDATE coordinator_counterfactuals SET outcome=?3 WHERE source=?1 AND bar=?2 AND outcome IS NULL",params![&source,old,r.to_string()])?;
                    }
                }
                self.last.insert(source.clone(), bar);
            }

            // Broker history can mature between completed bars, so weights are refreshed on
            // every observe call rather than only when this member's timeframe advances.
            let weight=selected_weight(
                forward_stats(db,&source,None,now)?,forward_stats(db,&source,Some(&intelligence.regime),now)?,
                broker_stats(db,&source,None,now)?,broker_stats(db,&source,Some(&intelligence.regime),now)?,&intelligence);
            self.weights.insert(source.clone(),weight);

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
                        "regime":intelligence.regime.clone(),
                        "session":intelligence.session.clone(),
                        "execution_quality":intelligence.execution_quality,
                        "signal":signal
                    });
                    db.execute("INSERT OR IGNORE INTO coordinator_samples VALUES(?1,?2,?3,NULL)",params![&source,bar,p.to_string()])?;
                    db.execute("INSERT OR IGNORE INTO coordinator_counterfactuals(source,bar,context,brain,regime,side,observed_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![&source,bar,&key,&b.id,&intelligence.regime,signal.side,now])?;
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
        assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_counterfactuals",[],|r|r.get::<_,u64>(0)).unwrap(),1);
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
        assert!(w["trend"].evidence_source.starts_with("forward_bars"));
        let cf:String=db.query_row("SELECT outcome FROM coordinator_counterfactuals LIMIT 1",[],|r|r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&cf).unwrap()["state"],"scored");
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
    fn execution_history_quality_is_shrunk_and_penalizes_rejects() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');").unwrap();
        let key="ctx";let now=1_800_000_000_000u64;
        let cmd=json!({"signal":{"stop_distance":1.0}}).to_string();
        let good=json!({"state":"filled","request":{"price":2000.0},"price":2000.02,"queue_to_send_ms":12.0}).to_string();
        db.execute("INSERT INTO broker_commands VALUES('g',?1,'system','open',?2,'observed',?3,?4)",params![key,now-1000,&cmd,&good]).unwrap();
        let one_good=historical_execution_quality(&db,key,now).unwrap();
        assert!(one_good>0.5&&one_good<0.7); // one fill is useful, but strongly shrunk
        for i in 0..20u64{let bad=json!({"state":"rejected","request":{"price":2000.0},"price":0.0,"queue_to_send_ms":120.0}).to_string();db.execute("INSERT INTO broker_commands VALUES(?1,?2,'system','open',?3,'rejected',?4,?5)",params![format!("r{i}"),key,now-2000-i,&cmd,&bad]).unwrap();}
        assert!(historical_execution_quality(&db,key,now).unwrap()<0.35);
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
                "side":1,"stop_distance":0.10,
                "coordination":{
                    "trading_mode":"expert",
                    "intelligence":{"regime":"trend_up_normal","session":"london","execution_quality":0.9},
                    "members":[
                        {"id":"trend","timeframe":"M1","report_id":"","bar_time":123,"side":1,"ready":true},
                        {"id":"gann","timeframe":"M1","report_id":"","bar_time":123,"side":-1,"ready":true}
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
        assert!(trend.evidence_source.starts_with("mt5_demo_broker"));
        assert!(trend.mean_reward > 0.0);
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