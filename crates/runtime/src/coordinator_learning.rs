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
    weighted_win_rate: f64,
    effective_samples: f64,
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
    Stats{samples,wins,mean_net_per_lot:if effective>0.{weighted_net/effective}else{0.},mean_reward:if effective>0.{weighted_reward/effective}else{0.},weighted_win_rate:if effective>0.{(weighted_wins/effective).clamp(0.,1.)}else{0.5},effective_samples:effective}
}
fn forward_stats(db:&Connection,source:&str,regime:Option<&str>,now:u64)->Result<Stats>{
    let mut st=db.prepare("SELECT payload,outcome FROM coordinator_samples WHERE source=?1 AND json_extract(outcome,'$.state')='scored' ORDER BY bar DESC LIMIT 4000")?;
    let rows=st.query_map([source],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut samples,mut wins,mut weighted_wins,mut weighted_net,mut weighted_reward,mut effective)=(0u64,0u64,0.,0.,0.,0.);
    for(raw_p,raw_o)in rows{let p:Value=serde_json::from_str(&raw_p)?;let o:Value=serde_json::from_str(&raw_o)?;let sample_regime=p["regime"].as_str().unwrap_or("legacy");if regime.is_some_and(|wanted|wanted!=sample_regime){continue;}let net=o["net"].as_f64().unwrap_or(0.);let reward=o["reward"].as_f64().unwrap_or_else(||reward_v2(net,0.).1);let ts=o["scored_ms"].as_u64().or_else(||p["observed_ms"].as_u64()).unwrap_or(now);let w=decay_weight(ts,now,FORWARD_HALF_LIFE_MS);samples+=1;if net>0.{wins+=1;weighted_wins+=w;}weighted_net+=net*w;weighted_reward+=reward*w;effective+=w;}
    Ok(finish_stats(samples,wins,weighted_wins,weighted_net,weighted_reward,effective))
}
fn broker_stats(db:&Connection,source:&str,regime:Option<&str>,now:u64)->Result<Stats>{
    let mut st=db.prepare("SELECT closed_ms,payload FROM coordinator_broker_samples WHERE source=?1 ORDER BY closed_ms DESC LIMIT 4000")?;
    let rows=st.query_map([source],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut samples,mut wins,mut weighted_wins,mut weighted_net,mut weighted_reward,mut effective)=(0u64,0u64,0.,0.,0.,0.);
    for(ts,raw)in rows{let p:Value=serde_json::from_str(&raw)?;let sample_regime=p["regime"].as_str().unwrap_or("legacy");if regime.is_some_and(|wanted|wanted!=sample_regime){continue;}let net=p["net_per_lot"].as_f64().unwrap_or(0.);let reward=p["reward"].as_f64().unwrap_or_else(||reward_v2(net,0.).1);let w=decay_weight(ts,now,BROKER_HALF_LIFE_MS);samples+=1;if net>0.{wins+=1;weighted_wins+=w;}weighted_net+=net*w;weighted_reward+=reward*w;effective+=w;}
    Ok(finish_stats(samples,wins,weighted_wins,weighted_net,weighted_reward,effective))
}
// Repeated-mistake memory uses only exact completed Demo broker evidence already
// in coordinator_broker_samples. An observed loss is not proof that the strategy
// caused the market move. The response is a temporary, bounded preference shift.
fn contextual_stats(db:&Connection,source:&str,key:&str,now:u64)->Result<(u64,f64,f64,u64,f64)> {
    let mut st=db.prepare("SELECT closed_ms,payload FROM coordinator_broker_samples WHERE source=?1 ORDER BY closed_ms DESC,position_id DESC LIMIT 4000")?;
    let rows=st.query_map([source],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut count,mut sum,mut mass)=(0u64,0.,0.);
    let(mut recent_loss_streak,mut recent_cost_losses)=(0u64,0u64);
    let mut streak_unbroken=true;
    const MEMORY_HORIZON_MS:u64=14*24*60*60*1000;
    for(ts,raw) in rows {
        let p:Value=serde_json::from_str(&raw)?;
        if p["context_key"].as_str()!=Some(key){continue;}
        if let Some(reward)=p["reward"].as_f64().filter(|v|v.is_finite()) {
            let w=decay_weight(ts,now,BROKER_HALF_LIFE_MS);
            count+=1;sum+=reward*w;mass+=w;
        }
        // Nonfinite or missing booked net breaks the streak; never invent a loss.
        // Expiration stops stale mistakes from holding back a recovered strategy.
        if streak_unbroken {
            if ts==0 || ts>now || now.saturating_sub(ts)>MEMORY_HORIZON_MS {
                streak_unbroken=false;
            } else if p["net"].as_f64().is_some_and(|net|net.is_finite() && net<0.) {
                recent_loss_streak+=1;
                if p["attribution"]=="costs_erased_gross_profit" || p["attribution"]=="booked_costs" {
                    recent_cost_losses+=1;
                }
            } else {
                streak_unbroken=false;
            }
        }
    }
    // No penalty on just one or two losses: trading outcomes are noisy.
    // Three consecutive negatives in the SAME regime/session/spread context
    // introduce a mild preference penalty, with extra evidence for cost drag.
    let penalty=if recent_loss_streak>=3 {
        (0.035*(recent_loss_streak-1) as f64
           + 0.012*(recent_cost_losses.saturating_sub(1)) as f64).clamp(0.,0.20)
    } else {0.};
    Ok((count,if mass>0.{sum/mass}else{0.},confidence(mass,true),recent_loss_streak,penalty))
}
fn confidence(effective:f64,broker:bool)->f64{if effective<=0.{0.}else{let scale=if broker{8.}else{20.};(1.-(-effective/scale).exp()).clamp(0.,1.)}}
// Use at most eight effective observations from other regimes as a prior.
// Subtract the local regime first so an outcome is not counted twice.
fn pooled_stats(global:&Stats, local:&Stats)->Stats {
    if local.samples==0 || local.effective_samples<=0. {return global.clone();}
    let remaining=(global.effective_samples-local.effective_samples).max(0.);
    if remaining<=1e-9 {return local.clone();}
    let prior=remaining.min(8.);
    let mass=local.effective_samples+prior;
    let blend=|g:f64,l:f64| (l*local.effective_samples + (g*global.effective_samples-l*local.effective_samples)/remaining*prior)/mass;
    Stats{samples:global.samples,wins:global.wins,effective_samples:mass,
        mean_net_per_lot:blend(global.mean_net_per_lot,local.mean_net_per_lot),
        mean_reward:blend(global.mean_reward,local.mean_reward).clamp(-1.,1.),
        weighted_win_rate:blend(global.weighted_win_rate,local.weighted_win_rate).clamp(0.,1.)}
}
fn selected_weight(forward_global:Stats,forward_regime:Stats,broker_global:Stats,broker_regime:Stats,intelligence:&IntelligenceContext)->LearnedWeight{
    let(mut chosen,broker,evidence)=if broker_global.samples>0{if broker_regime.samples>0{(pooled_stats(&broker_global,&broker_regime),true,"mt5_demo_broker_pooled_regime")}else{(broker_global.clone(),true,"mt5_demo_broker_global")}}else if forward_regime.samples>0{(pooled_stats(&forward_global,&forward_regime),false,"forward_bars_pooled_regime")}else{(forward_global.clone(),false,"forward_bars_global")};
    if chosen.effective_samples<=0.&&chosen.samples>0{chosen.effective_samples=chosen.samples as f64;}
    LearnedWeight{samples:chosen.samples,wins:chosen.wins,mean_net_per_lot:chosen.mean_net_per_lot,mean_reward:chosen.mean_reward,weighted_win_rate:chosen.weighted_win_rate,broker_samples:broker_global.samples,forward_samples:forward_global.samples,regime_samples:if broker{broker_regime.samples}else{forward_regime.samples},effective_samples:chosen.effective_samples,confidence:confidence(chosen.effective_samples,broker),regime:intelligence.regime.clone(),session:intelligence.session.clone(),direction:intelligence.direction,atr:intelligence.atr,volatility_ratio:intelligence.volatility_ratio,spread_to_atr:intelligence.spread_to_atr,execution_quality:intelligence.execution_quality,execution_history_quality:intelligence.execution_history_quality,execution_history_samples:intelligence.execution_history_samples,quote_freshness:intelligence.quote_freshness,evidence_source:evidence.into(),..Default::default()}
}

fn execution_history_quality(db:&Connection,key:&str,t:&Telemetry,now:u64)->Result<(f64,u64)> {
    if !table_exists(db,"broker_commands")? { return Ok((0.5,0)); }
    let mut st=db.prepare("SELECT created_ms,result FROM broker_commands WHERE context=?1 AND brain='system' AND kind='open' AND result<>'{}' ORDER BY created_ms DESC LIMIT 200")?;
    let rows=st.query_map([key],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let point=t.point.filter(|v|v.is_finite()&&*v>0.).unwrap_or(0.);
    let(mut weighted,mut mass,mut samples)=(0.,0.,0u64);
    for(created_ms,raw)in rows {
        let Ok(result)=serde_json::from_str::<Value>(&raw) else { continue; };
        let state=result["state"].as_str().unwrap_or("");
        let base=match state {
            "filled"|"partial"|"placed"|"observed"=>0.70,
            "rejected"=>0.10,
            "unknown"=>0.20,
            _=>continue,
        };
        let slip_score=if matches!(state,"filled"|"partial"|"placed"|"observed") && point>0. {
            result["request"]["price"].as_f64().zip(result["price"].as_f64()).map(|(requested,actual)|{
                let pts=((actual-requested).abs()/point).max(0.);
                1.0/(1.0+pts/3.0)
            }).unwrap_or(0.5)
        } else {0.0};
        let latency_score=result["send_ms"].as_f64().filter(|v|v.is_finite()&&*v>=0.).map(|ms|1.0/(1.0+ms/250.0)).unwrap_or(0.5);
        let q=if matches!(state,"filled"|"partial"|"placed"|"observed") {(base+0.18*slip_score+0.12*latency_score).clamp(0.,1.)} else {base};
        let w=decay_weight(created_ms,now,7.0*24.0*60.0*60.0*1000.0);
        weighted+=q*w;mass+=w;samples+=1;
    }
    Ok((if mass>0.{(weighted/mass).clamp(0.,1.)}else{0.5},samples))
}

fn open_learning_commands(db: &Connection, key: &str) -> Result<Vec<(String, String)>> {
    let mut st = db.prepare(
        "SELECT payload,result FROM broker_commands c WHERE context=?1 AND kind='open' AND CAST(json_extract(result,'$.deal') AS INTEGER)>0 AND NOT EXISTS(SELECT 1 FROM broker_learning_processed p WHERE p.context=c.context AND p.command_id=c.id) ORDER BY created_ms,id",
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

/// Credit only complete closed broker positions. A partial history page or partial
/// close is not a training label; no synthetic fills or opposing-member losses.
fn sync_demo_broker_evidence(db: &Connection, t: &Telemetry) -> Result<()> {
    if !t.demo || t.account_trade_mode != Some(0) { return Ok(()); }
    if !table_exists(db,"broker_commands")? || !table_exists(db,"broker_deals")? { return Ok(()); }
    let key = context(t);
    for (raw_cmd,raw_result) in open_learning_commands(db,&key)? {
        let cmd:Value=serde_json::from_str(&raw_cmd)?;
        let result:Value=serde_json::from_str(&raw_result)?;
        let Some(open_deal)=result["deal"].as_u64().filter(|v|*v>0) else {continue;};
        let position_id:Option<u64>=db.query_row("SELECT position_id FROM broker_deals WHERE context=?1 AND ticket=?2",params![&key,open_deal],|r|r.get(0)).optional()?;
        let Some(position_id)=position_id.filter(|v|*v>0) else {continue;};
        if t.positions.iter().any(|p|p.identifier==position_id||p.ticket==position_id) {continue;}
        let (mut gross,mut commission,mut swap,mut fee,mut entry_volume,mut exit_volume,mut closed_ms)=(0.,0.,0.,0.,0.,0.,0u64);
        let mut reasons=Vec::new();
        let mut valid=true;
        for raw in position_deals(db,&key,position_id)? {
            let d:Value=serde_json::from_str(&raw)?;
            gross+=d["profit"].as_f64().unwrap_or(0.);
            commission+=d["commission"].as_f64().unwrap_or(0.);
            swap+=d["swap"].as_f64().unwrap_or(0.);
            fee+=d["fee"].as_f64().unwrap_or(0.);
            let volume=d["volume"].as_f64().unwrap_or(0.);
            match d["entry"].as_i64() {
                Some(0)=>entry_volume+=volume,
                Some(1)|Some(3)=>{exit_volume+=volume;closed_ms=closed_ms.max(d["time_msc"].as_u64().unwrap_or(0));
                    if let Some(reason)=d["reason"].as_i64(){if !reasons.contains(&reason){reasons.push(reason);}}},
                // Netting reversal mixes an opening and an exit: do not guess attribution.
                Some(2)=>valid=false,
                _=>{}
            }
        }
        let net=gross+commission+swap+fee;
        if !valid || !net.is_finite() || !exit_volume.is_finite() || exit_volume<=0.
            || !entry_volume.is_finite() || entry_volume+1e-8<result["volume"].as_f64().unwrap_or(0.) || (entry_volume-exit_volume).abs()>1e-8 {continue;}
        let coordination=&cmd["signal"]["coordination"];
        let learning=&cmd["learning"];
        let owner=cmd["brain"].as_str().unwrap_or("system");
        let mode_value=if learning["trading_mode"].is_null(){&coordination["trading_mode"]}else{&learning["trading_mode"]};
        let Ok(mode)=serde_json::from_value::<TradingMode>(mode_value.clone()) else {continue;};
        let Some(executed_side)=cmd["signal"]["side"].as_i64() else {continue;};
        let selected=coordination["selection"]["brain"].as_str();
        let intel=if coordination["intelligence"].is_object(){&coordination["intelligence"]}else{&learning["intelligence"]};
        let regime=intel["regime"].as_str().unwrap_or("legacy");
        let session=intel["session"].as_str().unwrap_or("unknown");
        let stop=result["price"].as_f64().filter(|v|*v>0.).or_else(||result["request"]["price"].as_f64()).zip(result["request"]["sl"].as_f64())
            .map(|(price,sl)|(price-sl).abs()).unwrap_or_else(||cmd["signal"]["stop_distance"].as_f64().unwrap_or(0.));
        let tick=learning["tick_size"].as_f64().or(t.tick_size).unwrap_or(0.);
        let value=learning["tick_value_loss"].as_f64().or(t.tick_value_loss).unwrap_or(0.);
        let risk_money=if tick>0. && value>0. {stop/tick*value*exit_volume} else {0.};
        let (r_multiple,reward)=reward_v2(net,risk_money);
        // Sniper target learning is causal only when the TP *submitted to the
        // broker* still represents the arm chosen by the specialist. The MT5
        // adapter may widen TP to meet stops-level rules. Keep the completed
        // trade as real strategy feedback, but do NOT assign its result to a
        // different, unexecuted pip target.
        let sniper_tp_preserved=if coordination["selection"]["sniper_target_mode"]=="adaptive" {
            let expected=cmd["signal"]["target_distance"].as_f64();
            let entry=result["request"]["price"].as_f64();
            let tp=result["request"]["tp"].as_f64();
            let tick=learning["tick_size"].as_f64();
            match (expected,entry,tp,tick) {
                (Some(expected),Some(entry),Some(tp),Some(tick))
                    if expected.is_finite()&&expected>0.&&entry.is_finite()
                        &&tp.is_finite()&&tick.is_finite()&&tick>0. =>
                {
                    (tp-entry).abs()>0. && ((tp-entry).abs()-expected).abs()<=tick*1.01
                }
                _=>false,
            }
        } else {false};
        let attribution=if gross>0. && net<=0. {"costs_erased_gross_profit"}
            else if net>0. {"positive_after_costs"} else if gross<0. {"adverse_price_move"}
            else if net<0. {"booked_costs"} else {"break_even"};
        let mut members=coordination["members"].as_array().cloned().unwrap_or_default();
        if owner!="system" && !learning["timeframe"].is_null() {
            members=vec![json!({"id":owner,"timeframe":learning["timeframe"],"report_id":learning["report_id"],
                "bar_time":cmd["signal"]["bar_time"],"side":executed_side,"ready":true,"weight":learning["weight_at_entry"]})];
        } else if learning["trading_mode"].is_string() {
            // The primary policy also receives its own outcome, including Sniper with no ready advisor.
            members.push(json!({"id":"system","timeframe":learning["timeframe"],"report_id":"",
                "bar_time":cmd["signal"]["bar_time"],"side":executed_side,"ready":true,"weight":learning["weight_at_entry"]}));
        }
        let tx=db.unchecked_transaction()?;
        for member in members {
            if member["ready"]!=true {continue;}
            let Some(id)=member["id"].as_str() else {continue;};
            if id!="system" && selected.is_some_and(|chosen|chosen!=id) {continue;}
            let Some(timeframe)=member["timeframe"].as_str() else {continue;};
            let report_id=member["report_id"].as_str().unwrap_or("");
            let source=source_key(&key,mode,id,timeframe,report_id)?;
            let bar=member["bar_time"].as_u64().unwrap_or(0);
            let supported=member["side"].as_i64()==Some(executed_side);
            if bar>0 {tx.execute("UPDATE coordinator_counterfactuals SET executed_position_id=?3,executed_side=?4,supported_execution=?5 WHERE source=?1 AND bar=?2",params![&source,bar,position_id,executed_side,if supported{1_i64}else{0_i64}])?;}
            if !supported {continue;}
            let payload=json!({"evidence_source":"mt5_demo_broker","context":key,"position_id":position_id,
                "brain":id,"timeframe":timeframe,"report_id":report_id,"trading_mode":mode,"trade_owner":owner,"executed_side":executed_side,
                "open_command_id":cmd["id"],"open_deal":open_deal,"closed_ms":closed_ms,"closed_volume":exit_volume,
                "gross":gross,"commission":commission,"swap":swap,"fee":fee,"net":net,"net_per_lot":net/exit_volume,
                "attribution":attribution,"exit_reason_codes":reasons,"weight_at_entry":member["weight"],"entry_reason":cmd["signal"]["reason"],
                "risk_money":risk_money,"r_multiple":r_multiple,"reward":reward,"regime":regime,"session":session,
                "context_key":learning["context_key"],"selection":coordination["selection"],
                "sniper_tp_preserved":sniper_tp_preserved,
                "execution_quality":intel["execution_quality"],"costs":"exact booked MT5 deal profit + commission + swap + fee"});
            tx.execute("INSERT OR IGNORE INTO coordinator_broker_samples(source,position_id,context,brain,closed_ms,payload) VALUES(?1,?2,?3,?4,?5,?6)",params![&source,position_id,&key,id,closed_ms,payload.to_string()])?;
        }
        if let Some(id)=cmd["id"].as_str() {
            tx.execute("INSERT OR IGNORE INTO broker_learning_processed(context,command_id) VALUES(?1,?2)",params![key,id])?;
        }
        tx.commit()?;
    }
    Ok(())
}

pub fn broker_review(db:&Connection,key:&str)->Result<Vec<Value>> {
    let mut st=db.prepare("SELECT payload FROM coordinator_broker_samples WHERE context=?1 ORDER BY closed_ms DESC,brain LIMIT 40")?;
    let rows=st.query_map([key],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|raw|Ok(serde_json::from_str(&raw)?)).collect()
}

pub struct Learner {
    last: BTreeMap<String, u64>,
    weights: BTreeMap<String, LearnedWeight>,
}

impl Learner {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS broker_learning_processed(context TEXT NOT NULL,command_id TEXT NOT NULL,PRIMARY KEY(context,command_id));
             CREATE TABLE IF NOT EXISTS coordinator_samples(source TEXT NOT NULL,bar INTEGER NOT NULL,payload TEXT NOT NULL,outcome TEXT,PRIMARY KEY(source,bar));
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
        let(history_quality,history_samples)=execution_history_quality(db,&key,t,now)?;
        intelligence.execution_history_quality=history_quality;
        intelligence.execution_history_samples=history_samples;
        if history_samples>=5 {
            intelligence.execution_quality=(0.65*intelligence.execution_quality+0.35*history_quality).clamp(0.,1.);
        }

        for b in brains {
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
            let mut weight=selected_weight(
                forward_stats(db,&source,None,now)?,forward_stats(db,&source,Some(&intelligence.regime),now)?,
                broker_stats(db,&source,None,now)?,broker_stats(db,&source,Some(&intelligence.regime),now)?,&intelligence);
            weight.context_key=braintrade_domain::student::context_key(&intelligence);
            if c.trading.demo_flexible && t.demo && t.account_trade_mode==Some(0) {
                (weight.context_samples,weight.context_reward,weight.context_confidence,
                 weight.context_loss_streak,weight.context_mistake_penalty)=contextual_stats(db,&source,&weight.context_key,now)?;
            }
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

    #[test]
    fn context_rewards_do_not_cross_sessions_cost_buckets_or_sources() {
        let db=Connection::open_in_memory().unwrap();let _=Learner::new(&db).unwrap();
        for (id,source,key,reward) in [(1,"same","trend_up_normal|london|tight",0.8),
            (2,"same","trend_up_normal|asia|tight",-0.8),(3,"same","trend_up_normal|london|wide",-0.9),
            (4,"other","trend_up_normal|london|tight",-1.)] {
            db.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,'account','trend',1000,?3)",
                params![source,id,json!({"context_key":key,"reward":reward}).to_string()]).unwrap();
        }
        let (n,reward,confidence,streak,penalty)=contextual_stats(&db,"same","trend_up_normal|london|tight",1000).unwrap();
        assert_eq!(n,1);assert_eq!(reward,0.8);assert!(confidence>0. && confidence<0.2);
        assert_eq!(streak,0);assert_eq!(penalty,0.);
        assert_eq!(contextual_stats(&db,"missing","trend_up_normal|london|tight",1000).unwrap().0,0);
    }

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
        assert!(w["trend"].evidence_source.starts_with("forward_bars"));
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
    #[test]
    fn execution_history_quality_learns_from_fill_slippage_and_rejections() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');"
        ).unwrap();
        let now=1_800_000_000_000u64;
        let t=Telemetry{point:Some(0.01),..Default::default()};
        let key="ctx";
        for i in 0..5u64 {
            let result=json!({"state":"filled","request":{"price":2000.0},"price":2000.0+0.01*(i as f64),"send_ms":40.0});
            db.execute("INSERT INTO broker_commands VALUES(?1,?2,'system','open',?3,'observed','{}',?4)",params![format!("f{i}"),key,now-i*1000,result.to_string()]).unwrap();
        }
        let(good,n)=execution_history_quality(&db,key,&t,now).unwrap();
        assert_eq!(n,5);
        assert!(good>0.70);
        for i in 0..5u64 {
            let result=json!({"state":"rejected","reason":"fixture"});
            db.execute("INSERT INTO broker_commands VALUES(?1,?2,'system','open',?3,'rejected','{}',?4)",params![format!("r{i}"),key,now+10_000+i,result.to_string()]).unwrap();
        }
        let(worse,n2)=execution_history_quality(&db,key,&t,now+20_000).unwrap();
        assert_eq!(n2,10);
        assert!(worse<good);
    }

    #[test]
    fn solo_closed_costs_partial_history_restart_and_live_isolation() {
        let db=Connection::open_in_memory().unwrap();let _=Learner::new(&db).unwrap();
        db.execute_batch("CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
            CREATE TABLE broker_deals(context TEXT,ticket INTEGER,position_id INTEGER,brain TEXT,payload TEXT,PRIMARY KEY(context,ticket));").unwrap();
        let t=Telemetry{demo:true,account_trade_mode:Some(0),login:17,server:"Demo".into(),symbol:"XAUUSD".into(),currency:"USD".into(),..Default::default()};
        let key=context(&t);
        let cmd=json!({"id":"solo","brain":"trend","signal":{"side":1,"bar_time":12,"stop_distance":1.,"reason":"trend ready"},
            "learning":{"trading_mode":"expert","timeframe":"M1","report_id":"","tick_size":0.01,"tick_value_loss":1.,"weight_at_entry":1.}});
        db.execute("INSERT INTO broker_commands VALUES('solo',?1,'trend','open',1,'observed',?2,?3)",params![key,cmd.to_string(),json!({"deal":1,"request":{"price":2000.,"sl":1999.}}).to_string()]).unwrap();
        let insert=|ticket:u64,entry:i64,volume:f64,profit:f64,commission:f64| {
            db.execute("INSERT INTO broker_deals VALUES(?1,?2,90,'trend',?3)",params![key,ticket,json!({"entry":entry,"volume":volume,"profit":profit,"commission":commission,"time_msc":1000+ticket,"reason":5}).to_string()]).unwrap();
        };
        insert(1,0,0.1,0.,-0.35);insert(2,1,0.04,0.2,-0.14);
        sync_demo_broker_evidence(&db,&t).unwrap();assert!(broker_review(&db,&key).unwrap().is_empty());
        insert(3,1,0.06,0.3,-0.21);
        let mut live=t.clone();live.demo=false;live.account_trade_mode=Some(2);
        sync_demo_broker_evidence(&db,&live).unwrap();assert!(broker_review(&db,&key).unwrap().is_empty());
        sync_demo_broker_evidence(&db,&t).unwrap();
        let _=Learner::new(&db).unwrap();sync_demo_broker_evidence(&db,&t).unwrap();
        let rows=broker_review(&db,&key).unwrap();assert_eq!(rows.len(),1);
        assert_eq!(rows[0]["trade_owner"],"trend");assert_eq!(rows[0]["attribution"],"costs_erased_gross_profit");
        assert!((rows[0]["net"].as_f64().unwrap()+0.2).abs()<1e-9);
        assert_eq!(rows[0]["risk_money"],10.);
        let source=source_key(&key,TradingMode::Expert,"trend","M1","").unwrap();
        let learned=selected_weight(Stats::default(),Stats::default(),broker_stats(&db,&source,None,2000).unwrap(),Stats::default(),&IntelligenceContext::default());
        assert_eq!(learned.broker_samples,1);assert!(learned.weight()<1.);
        let mut other=t.clone();other.login=18;assert!(broker_review(&db,&context(&other)).unwrap().is_empty());
    }

    #[test]
    fn sparse_regime_keeps_global_prior_without_double_counting() {
        let global=Stats{samples:101,wins:100,mean_reward:99./101.,weighted_win_rate:100./101.,effective_samples:101.,..Default::default()};
        let local=Stats{samples:1,wins:0,mean_reward:-1.,weighted_win_rate:0.,effective_samples:1.,..Default::default()};
        let pooled=pooled_stats(&global,&local);
        assert_eq!(pooled.effective_samples,9.);
        assert!((pooled.mean_reward-7./9.).abs()<1e-9);
        assert!((pooled.weighted_win_rate-8./9.).abs()<1e-9);
        let same=pooled_stats(&local,&local);
        assert_eq!(same.effective_samples,1.);
        assert_eq!(same.mean_reward,-1.);
    }

    #[test]
    fn contextual_loss_memory_is_account_scoped_recovers_on_a_win_and_expires() {
        let db=Connection::open_in_memory().unwrap();let _=Learner::new(&db).unwrap();
        let now=2_000_000_000_000u64;
        let ctx="trend_up_normal|london|tight";
        let source="demo-account-1/trend";
        let other="demo-account-2/trend";
        let mut add=|source:&str,id:u64,age:u64,net:f64,attr:&str|{
            let payload=json!({"context_key":ctx,"net":net,"reward":if net<0.{-0.5}else{0.5},"attribution":attr});
            db.execute("INSERT INTO coordinator_broker_samples(source,position_id,context,brain,closed_ms,payload) VALUES(?1,?2,'account','trend',?3,?4)",
                params![source,id,now-age,payload.to_string()]).unwrap();
        };
        add(source,1,4000,-1.,"adverse_price_move");
        add(source,2,3000,-1.,"costs_erased_gross_profit");
        add(source,3,2000,-1.,"costs_erased_gross_profit");
        let (count,_,_,streak,penalty)=contextual_stats(&db,source,ctx,now).unwrap();
        assert_eq!(count,3);assert_eq!(streak,3);assert!(penalty>0.07 && penalty<=0.20);
        assert_eq!(contextual_stats(&db,other,ctx,now).unwrap().3,0);
        assert_eq!(contextual_stats(&db,source,"trend_down_normal|london|tight",now).unwrap().3,0);
        add(source,4,1000,0.5,"positive_after_costs");
        let (_,_,_,streak,penalty)=contextual_stats(&db,source,ctx,now).unwrap();
        assert_eq!(streak,0);assert_eq!(penalty,0.);
        // Historical negative samples can inform long-term statistics, but
        // cannot keep a temporary 14-day repeated-error penalty alive.
        let later=now+15*24*60*60*1000;
        assert_eq!(contextual_stats(&db,source,ctx,later).unwrap().4,0.);
        // Three unbroken losses in a new isolated account do not affect the first.
        for i in 0..3 {add(other,10+i,700-i*100,-1.,"adverse_price_move");}
        assert!(contextual_stats(&db,other,ctx,now).unwrap().4>0.);
        assert_eq!(contextual_stats(&db,source,ctx,now).unwrap().4,0.);
    }

}
