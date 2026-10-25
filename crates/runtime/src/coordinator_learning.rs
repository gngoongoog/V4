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
const FORWARD_HORIZON_BARS: u64 = 12;

// A calendar gap is not twelve completed candles. Require the exact frozen
// bar cadence before creating price-only prospective feedback.
fn forward_timeframe_seconds(timeframe: &str) -> Option<u64> {
    let (unit, count) = if let Some(v) = timeframe.strip_prefix('M') {
        (60_u64, v)
    } else if let Some(v) = timeframe.strip_prefix('H') {
        (3_600_u64, v)
    } else if let Some(v) = timeframe.strip_prefix('D') {
        (86_400_u64, v)
    } else if let Some(v) = timeframe.strip_prefix('W') {
        (604_800_u64, v)
    } else {
        return None;
    };
    let count = count.parse::<u64>().ok().filter(|n| *n > 0 && *n <= 60)?;
    unit.checked_mul(count)
}

fn contiguous_forward_horizon(bar: u64, future: &[&FrameBar], timeframe: &str) -> bool {
    let Some(step) = forward_timeframe_seconds(timeframe) else { return false; };
    future.len() == FORWARD_HORIZON_BARS as usize
        && future.iter().enumerate().all(|(index, frame)| {
            step.checked_mul(index as u64 + 1)
                .and_then(|offset| bar.checked_add(offset))
                == Some(frame.candle.time)
        })
}

// Validate stored labels again AT READ TIME. Legacy/out-of-order and incomplete
// records must never become artificial forward wins or weight changes.
// This is a frozen price/cost estimate, NEVER an MT5 fill or booked broker PnL.
fn verified_forward_feedback(
    bar: u64, payload: &Value, outcome: &Value, now: u64,
) -> Option<(f64, f64, u64)> {
    if outcome["state"] != "scored"
        || !matches!(payload["side"].as_i64(), Some(-1 | 1))
        || outcome["horizon_bars"].as_u64() != Some(FORWARD_HORIZON_BARS)
        || outcome["horizon_contiguous"] != true
    {
        return None;
    }
    let step = forward_timeframe_seconds(payload["timeframe"].as_str()?)?;
    if outcome["horizon_seconds"].as_u64() != step.checked_mul(FORWARD_HORIZON_BARS) {
        return None;
    }
    let earliest_exit = bar.checked_add(step.checked_mul(FORWARD_HORIZON_BARS)?)?;
    let exit = outcome["exit_bar"].as_u64()?;
    if exit != earliest_exit { return None; }
    let observed = payload["observed_ms"].as_u64().filter(|v| *v > 0)?;
    let scored = outcome["scored_ms"].as_u64().filter(|v| *v > 0 && *v <= now)?;
    if observed < bar.checked_mul(1_000)? || observed > exit.checked_mul(1_000)?
        || scored < exit.checked_add(step)?.checked_mul(1_000)?
        || scored < observed
    {
        return None;
    }
    for (name, strictly_positive) in [
        ("entry", true), ("tick", true), ("profit", true), ("loss", true),
        ("spread", false), ("slippage", false), ("commission", false),
    ] {
        let v = payload[name].as_f64().filter(|v| v.is_finite())?;
        if (strictly_positive && v <= 0.) || (!strictly_positive && v < 0.) { return None; }
    }
    let net = outcome["net"].as_f64().filter(|v| v.is_finite())?;
    let risk = outcome["risk_per_lot"].as_f64().filter(|v| v.is_finite() && *v >= 0.)?;
    let recorded_r = outcome["r_multiple"].as_f64().filter(|v| v.is_finite())?;
    let reward = outcome["reward"].as_f64()
        .filter(|v| v.is_finite() && (-1.0..=1.0).contains(v))?;
    let (expected_r, expected_reward) = reward_v2(net, risk);
    if !expected_r.is_finite() || !expected_reward.is_finite()
        || (recorded_r - expected_r).abs() > 1e-7_f64.max(expected_r.abs() * 1e-8)
        || (reward - expected_reward).abs() > 1e-8
    {
        return None;
    }
    Some((net, reward, scored))
}

fn forward_stats(db: &Connection, source: &str, regime: Option<&str>, now: u64) -> Result<Stats> {
    let mut st = db.prepare(
        "SELECT bar,payload,outcome FROM coordinator_samples
         WHERE source=?1 AND outcome IS NOT NULL ORDER BY bar DESC LIMIT 4000"
    )?;
    let rows = st.query_map([source], |r| Ok((
        r.get::<_, u64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut samples, mut wins, mut weighted_wins, mut weighted_net, mut weighted_reward, mut effective)
        = (0_u64, 0_u64, 0., 0., 0., 0.);
    for (bar, raw_p, raw_o) in rows {
        let (Ok(p), Ok(o)) = (
            serde_json::from_str::<Value>(&raw_p),
            serde_json::from_str::<Value>(&raw_o),
        ) else { continue; };
        if regime.is_some_and(|wanted| p["regime"].as_str() != Some(wanted)) { continue; }
        let Some((net, reward, scored)) = verified_forward_feedback(bar, &p, &o, now)
            else { continue; };
        let w = decay_weight(scored, now, FORWARD_HALF_LIFE_MS);
        samples += 1;
        if net > 0. { wins += 1; weighted_wins += w; }
        weighted_net += net * w;
        weighted_reward += reward * w;
        effective += w;
    }
    Ok(finish_stats(samples, wins, weighted_wins, weighted_net, weighted_reward, effective))
}
// Broker training labels can predate G5-B. Validate again AT READ TIME:
// upgrading the admission gate does not make legacy polluted rows trustworthy.
pub(crate) fn verified_broker_feedback(p:&Value)->Option<(f64,f64,f64)>{
    if p["evidence_source"]!="mt5_demo_broker" {return None;}
    let mut vals=[0.0_f64;6];
    for (i,name) in ["gross","commission","swap","fee","net","net_per_lot"].iter().enumerate(){
        vals[i]=p[*name].as_f64().filter(|n|n.is_finite())?;
    }
    let volume=p["closed_volume"].as_f64().filter(|v|v.is_finite()&&*v>0.)?;
    let reward=p["reward"].as_f64().filter(|v|v.is_finite()&&(-1.0..=1.0).contains(v))?;
    // A complete MT5 cost receipt is not sufficient if a historical/corrupt
    // learner reward was replaced after the position closed. Broker weights,
    // contextual mistake memory and Sniper arm scores all read this function.
    // Validate the EXACT stored reward_v2 calculation, not just its range.
    let risk=p["risk_money"].as_f64().filter(|v|v.is_finite()&&*v>=0.)?;
    let stored_r=p["r_multiple"].as_f64().filter(|v|v.is_finite())?;
    let net=vals[0]+vals[1]+vals[2]+vals[3];
    if !net.is_finite()||(net-vals[4]).abs()>1e-6_f64.max(vals[4].abs()*1e-8)
        ||(vals[4]/volume-vals[5]).abs()>1e-6_f64.max(vals[5].abs()*1e-8) {
        return None;
    }
    let (expected_r,expected_reward)=reward_v2(vals[4],risk);
    if !expected_r.is_finite()||!expected_reward.is_finite()
        ||(stored_r-expected_r).abs()>1e-6_f64.max(expected_r.abs()*1e-8)
        ||(reward-expected_reward).abs()>1e-8 {
        return None;
    }
    Some((vals[4],vals[5],reward))
}
// Financial arithmetic alone cannot prove that a broker receipt belongs to
// this actual account, mode, member, timeframe and physical MT5 position.
// Recheck immutable DB columns against the stored payload AND the original
// canonical member source key before ANY Rust weight/mistake update.
// Old noncanonical source strings are retained in SQLite but not trainable.
fn verified_broker_row(source:&str,stored_context:&str,stored_brain:&str,
    pos:u64,closed:u64,p:&Value)->Option<(f64,f64,f64)>{
    if pos==0||closed==0||p["context"]!=stored_context
        ||p["brain"]!=stored_brain
        ||p["position_id"].as_u64()!=Some(pos)
        ||p["closed_ms"].as_u64()!=Some(closed) {return None}
    let tuple:Value=serde_json::from_str(source).ok()?;
    let parts=tuple.as_array().filter(|a|a.len()==6)?;
    if parts[0]!=stored_context||parts[1]!="adaptive-member-v1"
        ||parts[3]!=stored_brain
        ||p["trading_mode"]!=parts[2]
        ||p["timeframe"]!=parts[4]
        ||p["report_id"]!=parts[5] {return None}
    verified_broker_feedback(p)
}

fn broker_stats(db:&Connection,source:&str,regime:Option<&str>,now:u64)->Result<Stats>{
    let mut st=db.prepare("SELECT context,brain,position_id,closed_ms,payload FROM coordinator_broker_samples WHERE source=?1 ORDER BY closed_ms DESC LIMIT 4000")?;
    let rows=st.query_map([source],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u64>(2)?,r.get::<_,u64>(3)?,r.get::<_,String>(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut samples,mut wins,mut weighted_wins,mut weighted_net,mut weighted_reward,mut effective)=(0u64,0u64,0.,0.,0.,0.);
    for(stored_context,stored_brain,pos,ts,raw)in rows{
        let Ok(p)=serde_json::from_str::<Value>(&raw) else{continue;};
        let sample_regime=p["regime"].as_str().unwrap_or("legacy");
        if regime.is_some_and(|wanted|wanted!=sample_regime){continue;}
        let Some((_,net_per_lot,reward))=verified_broker_row(
            source,&stored_context,&stored_brain,pos,ts,&p) else{continue;};
        if ts==0||ts>now {continue;}
        let w=decay_weight(ts,now,BROKER_HALF_LIFE_MS);
        samples+=1;
        if net_per_lot>0.{wins+=1;weighted_wins+=w;}
        weighted_net+=net_per_lot*w;
        weighted_reward+=reward*w;
        effective+=w;
    }
    Ok(finish_stats(samples,wins,weighted_wins,weighted_net,weighted_reward,effective))
}
// Repeated-mistake memory uses only exact completed Demo broker evidence already
// in coordinator_broker_samples. An observed loss is not proof that the strategy
// caused the market move. The response is a temporary, bounded preference shift.
fn contextual_stats(db:&Connection,source:&str,key:&str,now:u64)->Result<(u64,f64,f64,u64,f64)> {
    let mut st=db.prepare("SELECT context,brain,position_id,closed_ms,payload FROM coordinator_broker_samples WHERE source=?1 ORDER BY closed_ms DESC,position_id DESC LIMIT 4000")?;
    let rows=st.query_map([source],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u64>(2)?,r.get::<_,u64>(3)?,r.get::<_,String>(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let(mut count,mut sum,mut mass)=(0u64,0.,0.);
    let(mut recent_loss_streak,mut recent_cost_losses)=(0u64,0u64);
    let mut streak_unbroken=true;
    const MEMORY_HORIZON_MS:u64=14*24*60*60*1000;
    for(stored_context,stored_brain,pos,ts,raw) in rows {
        let Ok(p)=serde_json::from_str::<Value>(&raw) else{continue;};
        if p["context_key"].as_str()!=Some(key){continue;}
        let Some((net,_,reward))=verified_broker_row(
            source,&stored_context,&stored_brain,pos,ts,&p) else {continue;};
        if ts==0||ts>now {continue;}
        {
            let w=decay_weight(ts,now,BROKER_HALF_LIFE_MS);
            count+=1;sum+=reward*w;mass+=w;
        }
        // Nonfinite or missing booked net breaks the streak; never invent a loss.
        // Expiration stops stale mistakes from holding back a recovered strategy.
        if streak_unbroken {
            if ts==0 || ts>now || now.saturating_sub(ts)>MEMORY_HORIZON_MS {
                streak_unbroken=false;
            } else if net<0. {
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

/// Real Sniper pip targets only learn from the executable target sent to MT5.
/// This verifies direction and tick-normalized distance, not an imaginary TP
/// computed after the trade closed.
fn sniper_executed_target_matches(cmd: &Value, result: &Value) -> bool {
    let signal=&cmd["signal"];
    let learning=&cmd["learning"];
    let expected=signal["target_distance"].as_f64();
    let entry=result["request"]["price"].as_f64();
    let tp=result["request"]["tp"].as_f64();
    let tick=learning["tick_size"].as_f64();
    let side=signal["side"].as_i64();
    match (expected,entry,tp,tick,side) {
        (Some(expected),Some(entry),Some(tp),Some(tick),Some(side))
            if expected.is_finite() && expected>0.
                && entry.is_finite() && entry>0.
                && tp.is_finite() && tp>0.
                && tick.is_finite() && tick>0.
                && matches!(side,-1|1) =>
        {
            let distance=(tp-entry)*side as f64;
            distance>0. && (distance-expected).abs()<=tick*1.01
        }
        _=>false,
    }
}

fn open_learning_commands(db: &Connection, key: &str) -> Result<Vec<(String, String)>> {
    let mut st = db.prepare(
        // Previously reconciled system-owned Sniper trades from older builds
        // may have only a system sample. Backfill their specialist attribution
        // from existing complete MT5 booked deals, never synthetic outcomes.
        // Once a specialist row exists, it is excluded from future scans.
        "SELECT payload,result FROM broker_commands c
         WHERE c.context=?1 AND c.kind='open'
         AND CAST(json_extract(c.result,'$.deal') AS INTEGER)>0
         AND (
             NOT EXISTS(SELECT 1 FROM broker_learning_processed p
                        WHERE p.context=c.context AND p.command_id=c.id)
             OR (
                 c.brain='system'
                 AND json_extract(c.payload,'$.signal.coordination.selection.brain')='sniper_learner'
                 AND json_extract(c.payload,'$.signal.coordination.selection.policy')='sniper-ticks-v1'
                 AND NOT EXISTS(
                     SELECT 1 FROM coordinator_broker_samples s
                     WHERE s.context=c.context AND s.brain='sniper_learner'
                       AND json_extract(s.payload,'$.open_command_id')=c.id
                 )
             )
         ) ORDER BY c.created_ms,c.id",
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
        // G5-B: same immutable broker-cost and Deal-geometry verification used
        // by the trading journal. A missing profit, commission, swap, fee or
        // deal type is UNKNOWN, never a synthetic $0 learning reward.
        // Partial fills and reversal/unknown deal types cannot label an arm.
        let deals=position_deals(db,&key,position_id)?
            .into_iter().map(|raw|serde_json::from_str::<Value>(&raw))
            .collect::<std::result::Result<Vec<_>,_>>()?;
        let Some([net,gross,commission,swap,fee])=
            crate::trade_journal::booked_costs(deals.iter()) else {continue;};
        if !crate::trade_journal::fully_closed_by_deals(deals.iter()) {continue;}
        let (mut entry_volume,mut exit_volume,mut closed_ms)=(0.,0.,0u64);
        let mut reasons=Vec::new();
        for d in &deals {
            let volume=d["volume"].as_f64().unwrap();
            match d["entry"].as_i64() {
                Some(0)=>entry_volume+=volume,
                Some(1)|Some(3)=>{exit_volume+=volume;
                    closed_ms=closed_ms.max(d["time_msc"].as_u64().unwrap_or(0));
                    if let Some(reason)=d["reason"].as_i64(){
                        if !reasons.contains(&reason){reasons.push(reason);}
                    }
                },
                _=>unreachable!("fully_closed_by_deals rejects other entry types"),
            }
        }
        if closed_ms==0 || !entry_volume.is_finite() ||
            !exit_volume.is_finite() || exit_volume<=0. ||
            entry_volume+1e-8<result["volume"].as_f64().unwrap_or(0.) {continue;}
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
        let sniper_tp_preserved=coordination["selection"]["sniper_target_mode"]=="adaptive"
            && sniper_executed_target_matches(&cmd,&result);
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
        // When the system coordinator executes the Sniper Learner's selected
        // signal, the broker-owned position carries the SYSTEM magic. Credit
        // the specialist with the *same real closed Demo result* so its own
        // policy/target models train in the normal non-SOLO workflow. The
        // system retains its separately scoped record; no synthetic trade is
        // created and source_key prevents duplicates on reconciliation.
        if owner == "system"
            && mode == TradingMode::Sniper
            && selected == Some(braintrade_domain::sniper::ID)
            && coordination["selection"]["policy"] == "sniper-ticks-v1"
            && !members.iter().any(|m| m["id"] == braintrade_domain::sniper::ID)
        {
            members.push(json!({
                "id": braintrade_domain::sniper::ID,
                "timeframe": coordination["selection"]["timeframe"].as_str().unwrap_or("M1"),
                "report_id": "",
                "bar_time": cmd["signal"]["bar_time"],
                "side": executed_side,
                "ready": true,
                "weight": learning["weight_at_entry"]
            }));
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
                    } else if future.len() >= FORWARD_HORIZON_BARS as usize
                        && contiguous_forward_horizon(old, &future[..FORWARD_HORIZON_BARS as usize], &b.timeframe)
                        && forward_timeframe_seconds(&b.timeframe)
                            .is_some_and(|step| future[FORWARD_HORIZON_BARS as usize - 1]
                                .candle.time.saturating_add(step) <= now / 1_000)
                    {
                        let f = future[FORWARD_HORIZON_BARS as usize - 1];
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
                        Some(json!({"state":"scored","net":net,"risk_per_lot":risk,"r_multiple":r_multiple,"reward":reward,"exit_bar":f.candle.time,"scored_ms":now,
                            "horizon_bars":FORWARD_HORIZON_BARS,
                            "horizon_seconds":forward_timeframe_seconds(&b.timeframe).unwrap() * FORWARD_HORIZON_BARS,
                            "horizon_contiguous":true,"cost_model":"fixed contract; broker bar spread when positive, otherwise frozen entry spread; configured slippage and commission; no swaps"}))
                    } else if future.len() >= FORWARD_HORIZON_BARS as usize
                        && forward_timeframe_seconds(&b.timeframe).is_some_and(|step|
                            future[FORWARD_HORIZON_BARS as usize - 1].candle.time
                                .saturating_add(step) <= now / 1_000)
                    {
                        Some(json!({"state":"expired","reason":"Forward horizon missing ordered, contiguous completed bars"}))
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
    fn forward_labels_require_complete_causal_horizon_and_real_nondefaulted_math() {
        let db = Connection::open_in_memory().unwrap();
        let _learner = Learner::new(&db).unwrap();
        let now = 1_800_000_000_000_u64;
        let base = now / 1_000 - 30 * 60;
        let source = "account-scoped-forward";
        let make_p = |bar: u64| json!({
            "side":1,"regime":"trend_up_normal","timeframe":"M1",
            "observed_ms":(bar + 60) * 1_000,
            "entry":2000.0,"tick":0.01,"profit":1.0,"loss":1.0,
            "spread":0.02,"slippage":0.01,"commission":7.0
        });
        let make_o = |bar: u64| json!({
            "state":"scored","net":2.0,"risk_per_lot":4.0,"r_multiple":0.5,
            "reward":0.5_f64.tanh(),"exit_bar":bar+720,
            "scored_ms":(bar+780)*1_000,
            "horizon_bars":12,"horizon_seconds":720,"horizon_contiguous":true
        });
        let insert = |bar: u64, p: Value, o: Value| {
            db.execute("INSERT INTO coordinator_samples(source,bar,payload,outcome) VALUES(?1,?2,?3,?4)",
                params![source,bar,p.to_string(),o.to_string()]).unwrap();
        };
        insert(base,make_p(base),make_o(base));
        for i in 1..=12_u64 {
            let bar=base+i;
            let mut p=make_p(bar);
            let mut o=make_o(bar);
            match i {
                1=>o["net"]=Value::Null,
                2=>o["reward"]=Value::Null,
                3=>o["reward"]=json!(0.99),
                4=>o["scored_ms"]=json!(now+1),
                5=>o["exit_bar"]=json!(bar+11*60),
                6=>o["horizon_contiguous"]=json!(false),
                7=>p["observed_ms"]=json!(now+1),
                8=>o["risk_per_lot"]=json!(-1.0),
                9=>o["r_multiple"]=json!(999.0),
                10=>p["side"]=json!(0),
                11=>o["horizon_seconds"]=json!(0),
                12=>{o.as_object_mut().unwrap().remove("horizon_bars");},
                _=>unreachable!(),
            }
            insert(bar,p,o);
        }
        // A pre-upgrade row can look profitable, but cannot prove the
        // contiguous 12-bar prospective observation from its old schema.
        let legacy_bar=base+15;
        let mut legacy=make_o(legacy_bar);
        legacy.as_object_mut().unwrap().remove("horizon_contiguous");
        insert(legacy_bar,make_p(legacy_bar),legacy);
        let stats=forward_stats(&db,source,None,now).unwrap();
        assert_eq!(stats.samples,1,"Only the complete forward record may change weights");
        assert_eq!(stats.wins,1);
        assert!((stats.mean_net_per_lot-2.0).abs()<1e-9);
        let matched=forward_stats(&db,source,Some("trend_up_normal"),now).unwrap();
        assert_eq!(matched.samples,1);
        assert_eq!(forward_stats(&db,source,Some("flat"),now).unwrap().samples,0);
        let prior=selected_weight(stats.clone(),stats,Stats::default(),Stats::default(),
            &IntelligenceContext::default());
        assert_eq!(prior.forward_samples,1);
        assert_eq!(prior.broker_samples,0);
        assert!(prior.evidence_source.starts_with("forward_bars"));
    }

    #[test]
    fn gaps_cannot_generate_a_false_twelve_bar_forward_label() {
        let frames=bars(1_800_000_000);
        let bar=frames[100].candle.time;
        let contiguous: Vec<_> = frames.iter().skip(101).take(12).collect();
        assert!(contiguous_forward_horizon(bar,&contiguous,"M1"));
        assert!(!contiguous_forward_horizon(bar,&contiguous,"MN1"));
        let mut gap_frames=frames.clone();
        gap_frames[107].candle.time+=60;
        let gap: Vec<_> = gap_frames.iter().skip(101).take(12).collect();
        assert!(!contiguous_forward_horizon(bar,&gap,"M1"));
    }

    #[test]
    fn sniper_target_provenance_is_directional_and_rejects_broker_widening() {
        let cmd=json!({"signal":{"side":1,"target_distance":0.50},
            "learning":{"tick_size":0.01}});
        let exact=json!({"request":{"price":2000.0,"tp":2000.50}});
        let rounded=json!({"request":{"price":2000.0,"tp":2000.51}});
        let widened=json!({"request":{"price":2000.0,"tp":2000.75}});
        let reversed=json!({"request":{"price":2000.0,"tp":1999.50}});
        assert!(sniper_executed_target_matches(&cmd,&exact));
        assert!(sniper_executed_target_matches(&cmd,&rounded));
        assert!(!sniper_executed_target_matches(&cmd,&widened));
        assert!(!sniper_executed_target_matches(&cmd,&reversed));
        assert!(!sniper_executed_target_matches(&cmd,&json!({"request":{"price":2000.0}})));
        let sell=json!({"signal":{"side":-1,"target_distance":0.5},
            "learning":{"tick_size":0.01}});
        assert!(sniper_executed_target_matches(&sell,&reversed));
        assert!(!sniper_executed_target_matches(&sell,&exact));
    }
    #[test]
    fn context_rewards_do_not_cross_sessions_cost_buckets_or_sources() {
        let db=Connection::open_in_memory().unwrap();let _=Learner::new(&db).unwrap();
        let same=source_key("account",TradingMode::Expert,"trend","M1","").unwrap();
        let other=source_key("account",TradingMode::Expert,"trend","M1","other").unwrap();
        for (id,source,key,reward) in [(1,&same,"trend_up_normal|london|tight",0.8),
            (2,&same,"trend_up_normal|asia|tight",-0.8),(3,&same,"trend_up_normal|london|wide",-0.9),
            (4,&other,"trend_up_normal|london|tight",-1.)] {
            let report_id=if source==&other {"other"} else {""};
            db.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,'account','trend',1000,?3)",
                params![source,id,json!({"evidence_source":"mt5_demo_broker",
                    "context":"account","brain":"trend","trading_mode":"expert",
                    "timeframe":"M1","report_id":report_id,
                    "position_id":id,"closed_ms":1000,"context_key":key,
                    "gross":reward,"commission":0.0,"swap":0.0,"fee":0.0,
                    "net":reward,"net_per_lot":reward,"closed_volume":1.0,
                    "risk_money":0.0,"r_multiple":0.0,
                    "reward":reward_v2(reward,0.0).1}).to_string()]).unwrap();
        }
        let (n,reward,confidence,streak,penalty)=contextual_stats(&db,&same,"trend_up_normal|london|tight",1000).unwrap();
        assert_eq!(n,1);assert_eq!(reward,0.25);assert!(confidence>0. && confidence<0.2);
        assert_eq!(streak,0);assert_eq!(penalty,0.);
        assert_eq!(contextual_stats(&db,&other,"trend_up_normal|asia|tight",1000).unwrap().0,0);
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
            "ticket":100,"position_id":900,"magic":410600,"symbol":"XAUUSD","type":0,"entry":0,
            "time_msc":now-60_000,"volume":0.10,"price":2000.0,"profit":0.0,
            "commission":-0.35,"swap":0.0,"fee":0.0,"brain":"system"
        });
        let close = json!({
            "ticket":101,"position_id":900,"magic":410600,"symbol":"XAUUSD","type":1,"entry":1,
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

        // Corrupted broker history must NEVER become a profitable training
        // example. Missing cost components are not evidence of zero costs.
        let mut missing=close.clone();
        missing.as_object_mut().unwrap().remove("fee");
        db.execute("UPDATE broker_deals SET payload=?1 WHERE context=?2 AND ticket=101",
            params![missing.to_string(),key]).unwrap();
        let rejected=l.observe(&db,&t,&c,&[b.clone()],&f,false,now).unwrap();
        assert_eq!(rejected["trend"].broker_samples,0);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples",
            [],|row|row.get::<_,u64>(0)).unwrap(),0);
        // A complete, independent broker receipt arriving later makes the
        // SAME physical position eligible. No synthetic retry/trade is created.
        db.execute("UPDATE broker_deals SET payload=?1 WHERE context=?2 AND ticket=101",
            params![close.to_string(),key]).unwrap();
        let w = l.observe(&db, &t, &c, &[b], &f, false, now+1).unwrap();
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
            db.execute("INSERT INTO broker_deals VALUES(?1,?2,90,'trend',?3)",params![key,ticket,json!({"type":if entry==0 {0}else{1},"entry":entry,"volume":volume,"profit":profit,"commission":commission,"swap":0.,"fee":0.,"time_msc":1000+ticket,"reason":5}).to_string()]).unwrap();
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
    fn historical_incomplete_broker_rows_are_not_weights_or_mistake_memory(){
        let db=Connection::open_in_memory().unwrap();
        let _=Learner::new(&db).unwrap();
        let key="trend_up_normal|london|tight";
        let source=&source_key("demo",TradingMode::Expert,"trend","M1","").unwrap();
        let now=2_000_000_000_000u64;
        let valid=json!({"evidence_source":"mt5_demo_broker",
            "context":"demo","brain":"trend","trading_mode":"expert",
            "timeframe":"M1","report_id":"","position_id":10,
            "closed_ms":now-500,
            "gross":2.0,"commission":-0.5,"swap":0.0,"fee":0.0,
            "net":1.5,"net_per_lot":15.0,"closed_volume":0.1,
            "risk_money":0.0,"r_multiple":0.0,"reward":0.25,
            "regime":"trend_up_normal","context_key":key});
        let broken=[
            json!({"evidence_source":"mt5_demo_broker","gross":2.0,
                "commission":-0.5,"swap":0.0,"net":1.5,
                "net_per_lot":15.0,"closed_volume":0.1,"reward":0.8,
                "regime":"trend_up_normal","context_key":key}),
            json!({"evidence_source":"mt5_demo_broker","gross":2.0,
                "commission":-0.5,"swap":0.0,"fee":0.0,"net":20.0,
                "net_per_lot":200.0,"closed_volume":0.1,"reward":0.8,
                "regime":"trend_up_normal","context_key":key}),
            json!({"evidence_source":"mt5_demo_broker","gross":2.0,
                "commission":-0.5,"swap":0.0,"fee":0.0,"net":1.5,
                "net_per_lot":15.0,"closed_volume":0.1,
                "regime":"trend_up_normal","context_key":key}),
            json!({"evidence_source":"synthetic_shadow","gross":2.0,
                "commission":-0.5,"swap":0.0,"fee":0.0,"net":1.5,
                "net_per_lot":15.0,"closed_volume":0.1,"reward":0.8,
                "regime":"trend_up_normal","context_key":key}),
        ];
        for (i,row) in broken.iter().enumerate(){
            db.execute("INSERT INTO coordinator_broker_samples
                (source,position_id,context,brain,closed_ms,payload)
                VALUES(?1,?2,'demo','trend',?3,?4)",
                params![source,i+1,now-1000,row.to_string()]).unwrap();
        }
        let clean=broker_stats(&db,source,None,now).unwrap();
        assert_eq!(clean.samples,0,"Incomplete past receipts are not training evidence");
        assert_eq!(contextual_stats(&db,&source,key,now).unwrap().0,0);
        let weight=selected_weight(Stats::default(),Stats::default(),clean,Stats::default(),
            &IntelligenceContext::default());
        assert_eq!(weight.weight(),1.0,"No historical broker labels: neutral weight");
        db.execute("INSERT INTO coordinator_broker_samples
                (source,position_id,context,brain,closed_ms,payload)
                VALUES(?1,10,'demo','trend',?2,?3)",
                params![source,now-500,valid.to_string()]).unwrap();
        let updated=broker_stats(&db,source,None,now).unwrap();
        assert_eq!(updated.samples,1);
        assert!((updated.mean_net_per_lot-15.0).abs()<1e-6);
        assert_eq!(contextual_stats(&db,&source,key,now).unwrap().0,1);
        let tuned=selected_weight(Stats::default(),Stats::default(),updated,Stats::default(),
            &IntelligenceContext::default());
        assert!(tuned.weight()>1.0,"Verified positive after-cost feedback updates actual weight");
    }

    #[test]
    fn g5e_forbid_manipulated_reward_and_r_multiple_with_valid_mt5_costs() {
        let db=Connection::open_in_memory().unwrap();
        let _=Learner::new(&db).unwrap();
        let now=2_000_000_000_000u64;
        let key="trend_up_normal|london|tight";
        let source=&source_key("demo",TradingMode::Expert,"trend","M1","").unwrap();
        let exact=json!({
            "evidence_source":"mt5_demo_broker",
            "context":"demo","brain":"trend","trading_mode":"expert",
            "timeframe":"M1","report_id":"",
            "position_id":99,"closed_ms":now-500,"gross":3.0,
            "commission":-0.50,"swap":-0.10,"fee":-0.05,"net":2.35,
            "closed_volume":0.10,"net_per_lot":23.5,
            "risk_money":5.0,"r_multiple":0.47,"reward":0.47_f64.tanh(),
            "context_key":key,"regime":"trend_up_normal"
        });
        let mut corrupt_reward=exact.clone();corrupt_reward["reward"]=json!(0.99);
        let mut corrupt_r=exact.clone();corrupt_r["r_multiple"]=json!(-40.0);
        let mut missing_r=exact.clone();missing_r.as_object_mut().unwrap().remove("r_multiple");
        let mut missing_risk=exact.clone();missing_risk.as_object_mut().unwrap().remove("risk_money");
        let mut invalid_risk=exact.clone();invalid_risk["risk_money"]=json!(-1.0);
        for (i,p) in [corrupt_reward,corrupt_r,missing_r,missing_risk,invalid_risk]
            .iter().enumerate() {
            db.execute("INSERT INTO coordinator_broker_samples
                 (source,position_id,context,brain,closed_ms,payload)
                 VALUES(?1,?2,'demo','trend',?3,?4)",
                params![source,i+1,now-1000,p.to_string()]).unwrap();
        }
        assert_eq!(broker_stats(&db,source,None,now).unwrap().samples,0);
        assert_eq!(contextual_stats(&db,&source,key,now).unwrap().0,0);
        let neutral=selected_weight(Stats::default(),Stats::default(),
            broker_stats(&db,source,None,now).unwrap(),Stats::default(),
            &IntelligenceContext::default());
        assert_eq!(neutral.weight(),1.0);
        db.execute("INSERT INTO coordinator_broker_samples
             (source,position_id,context,brain,closed_ms,payload)
             VALUES(?1,99,'demo','trend',?2,?3)",
            params![source,now-500,exact.to_string()]).unwrap();
        let valid=broker_stats(&db,source,None,now).unwrap();
        assert_eq!(valid.samples,1);
        assert!((valid.mean_net_per_lot-23.5).abs()<1e-9);
        assert!((valid.mean_reward-0.47_f64.tanh()).abs()<1e-9);
        assert_eq!(contextual_stats(&db,&source,key,now).unwrap().0,1);
        let positive=selected_weight(Stats::default(),Stats::default(),
            valid,Stats::default(),&IntelligenceContext::default());
        assert!(positive.weight()>1.0,"Only exact reward lineage can tune broker weights");
        // Missing risk geometry in old data cannot be silently backfilled.
        let mut fallback=json!({
            "evidence_source":"mt5_demo_broker","gross":1.0,"commission":0.0,
            "swap":0.0,"fee":0.0,"net":1.0,"net_per_lot":10.0,
            "closed_volume":0.1,"risk_money":0.0,"r_multiple":0.0,"reward":0.25
        });
        assert!(verified_broker_feedback(&fallback).is_some(),
            "An explicit zero-risk fallback must retain reward_v2 provenance");
        fallback["reward"]=json!(0.95);
        assert!(verified_broker_feedback(&fallback).is_none());
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
        let source=source_key("account",TradingMode::Expert,"trend","M1","").unwrap();
        let other=source_key("account",TradingMode::Expert,"trend","M1","other").unwrap();
        let add=|source:&str,id:u64,age:u64,net:f64,attr:&str|{
            let report_id=if source==other.as_str() {"other"} else {""};
            let payload=json!({"evidence_source":"mt5_demo_broker","context_key":ctx,
                "context":"account","brain":"trend","trading_mode":"expert",
                "timeframe":"M1","report_id":report_id,"position_id":id,
                "closed_ms":now-age,
                "gross":net,"commission":0.0,"swap":0.0,"fee":0.0,"net":net,
                "net_per_lot":net,"closed_volume":1.0,"risk_money":0.0,
                "r_multiple":0.0,"reward":reward_v2(net,0.0).1,"attribution":attr});
            db.execute("INSERT INTO coordinator_broker_samples(source,position_id,context,brain,closed_ms,payload) VALUES(?1,?2,'account','trend',?3,?4)",
                params![source,id,now-age,payload.to_string()]).unwrap();
        };
        add(&source,1,4000,-1.,"adverse_price_move");
        add(&source,2,3000,-1.,"costs_erased_gross_profit");
        add(&source,3,2000,-1.,"costs_erased_gross_profit");
        let (count,_,_,streak,penalty)=contextual_stats(&db,&source,ctx,now).unwrap();
        assert_eq!(count,3);assert_eq!(streak,3);assert!(penalty>0.07 && penalty<=0.20);
        assert_eq!(contextual_stats(&db,&other,ctx,now).unwrap().3,0);
        assert_eq!(contextual_stats(&db,&source,"trend_down_normal|london|tight",now).unwrap().3,0);
        add(&source,4,1000,0.5,"positive_after_costs");
        let (_,_,_,streak,penalty)=contextual_stats(&db,&source,ctx,now).unwrap();
        assert_eq!(streak,0);assert_eq!(penalty,0.);
        // Historical negative samples can inform long-term statistics, but
        // cannot keep a temporary 14-day repeated-error penalty alive.
        let later=now+15*24*60*60*1000;
        assert_eq!(contextual_stats(&db,&source,ctx,later).unwrap().4,0.);
        // Three unbroken losses in a new isolated account do not affect the first.
        for i in 0..3 {add(&other,10+i,700-i*100,-1.,"adverse_price_move");}
        assert!(contextual_stats(&db,other,ctx,now).unwrap().4>0.);
        assert_eq!(contextual_stats(&db,&source,ctx,now).unwrap().4,0.);
    }



    #[test]
    fn p1k5i_demo_partial_fill_disconnect_and_sqlite_restart_are_idempotent() {
        // This exercises the genuine learner reconciliation path against
        // synthetic MT5 deal fixtures, not a live MetaTrader terminal.
        let path=std::env::temp_dir().join(
            format!("bt-demo-replay-{}.sqlite",uuid::Uuid::new_v4()));
        let config=Config::default();
        let now=1_800_000_000_000u64;
        let cutoff=now-120_000;
        let t=Telemetry {
            connected:true,demo:true,account_trade_mode:Some(0),
            server:"Broker-Demo".into(),login:901,symbol:config.symbol.clone(),
            currency:"USD".into(),tick_size:Some(0.01),
            tick_value_loss:Some(1.),tick_value_profit:Some(1.),
            ..Default::default()
        };
        let key=context(&t);
        let (artifact,hash)={
            let db=Connection::open(&path).unwrap();
            let mut learner=Learner::new(&db).unwrap();
            db.execute_batch(
                "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,
                   brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,
                   state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL);
                 CREATE TABLE broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,
                   position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,
                   PRIMARY KEY(context,ticket));"
            ).unwrap();
            let mut engine=braintrade_domain::sniper::Engine::default();
            engine.set_models(key.clone(),Default::default());
            let checkpoint=json!({"status":"captured","account_context":key,
                "cutoff_ms":cutoff,"broker_training_samples":0,
                "cached_policy_samples":0,"broker_target_samples":0,
                "cached_target_samples":0,"independent_oos":false,
                "broker_pnl":serde_json::Value::Null});
            let seal=crate::sniper_research_artifact::persist(
                &db,&t,&config,&engine,&checkpoint,cutoff).unwrap();
            let command=json!({"id":"p1k5i-cmd","brain":"system",
                "signal":{"side":1,"bar_time":10,"stop_distance":0.2,
                    "target_distance":0.2,"reason":"synthetic broker acceptance",
                    "coordination":{"trading_mode":"sniper",
                        "selection":{"brain":"sniper_learner","policy":"sniper-ticks-v1",
                            "timeframe":"M1","sniper_policy":"impulse",
                            "sniper_definition":"fixture","sniper_regime":"directional_normal",
                            "sniper_target_mode":"adaptive","sniper_target_arm":2,
                            "sniper_target_pips":5.0}}},
                "learning":{"trading_mode":"sniper","timeframe":"M1",
                    "report_id":"","tick_size":0.01,"tick_value_loss":1.0,
                    "tick_value_profit":1.0,"weight_at_entry":1.0}});
            let result=json!({"deal":9001,"volume":0.1,"price":2000.0,
                "request":{"price":2000.0,"tp":2000.2,"sl":1999.8}});
            db.execute("INSERT INTO broker_commands VALUES('p1k5i-cmd',?1,'system','open',?2,
                'observed',?3,?4)",
                params![key,now-60_000,command.to_string(),result.to_string()]).unwrap();
            let insert=|ticket:u64,entry:i64,volume:f64,profit:f64,commission:f64,
                        swap:f64,fee:f64,stamp:u64|{
                let v=json!({"ticket":ticket,"position_id":555,"magic":410600,
                    "type":if entry==0 {0}else{1},"entry":entry,"time_msc":stamp,"volume":volume,
                    "profit":profit,"commission":commission,"swap":swap,
                    "fee":fee,"brain":"system"});
                db.execute("INSERT INTO broker_deals VALUES(?1,?2,555,'system',?3)",
                    params![key,ticket,v.to_string()]).unwrap();
            };
            insert(9001,0,0.1,0.,-0.20,0.,0.,now-60_000);
            insert(9002,1,0.04,0.6,-0.10,0.,0.,now-10_000);
            learner.observe(&db,&t,&config,&[],&BTreeMap::new(),false,now).unwrap();
            assert_eq!(db.query_row(
                "SELECT COUNT(*) FROM coordinator_broker_samples",
                [],|r|r.get::<_,u64>(0)).unwrap(),0,
                "A partially closed position cannot become a completed training label");
            // Even after the final deal is recorded, REAL session feedback is
            // forbidden to import this Demo-only specialist result.
            insert(9003,1,0.06,0.9,-0.10,-0.05,-0.05,now-1_000);
            let mut real=t.clone();real.demo=false;real.account_trade_mode=Some(2);
            learner.observe(&db,&real,&config,&[],&BTreeMap::new(),false,now).unwrap();
            assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples",
                [],|r|r.get::<_,u64>(0)).unwrap(),0);
            (seal.id,seal.sha256)
        };
        {
            // Reopen the actual on-disk SQLite after Core-equivalent restart.
            let db=Connection::open(&path).unwrap();
            let mut learner=Learner::new(&db).unwrap();
            learner.observe(&db,&t,&config,&[],&BTreeMap::new(),false,now).unwrap();
            let rows:Vec<(String,f64)>=db.prepare(
                "SELECT brain,json_extract(payload,'$.net') FROM coordinator_broker_samples
                 WHERE context=?1 ORDER BY brain").unwrap()
                .query_map([&key],|r|Ok((r.get(0)?,r.get(1)?))).unwrap()
                .collect::<rusqlite::Result<_>>().unwrap();
            assert_eq!(rows.len(),2,
                "A system broker position creates two attribution labels, not two physical trades");
            assert_eq!(rows[0].0,"sniper_learner");
            assert!((rows[0].1-1.0).abs()<1e-8);
            let booked=crate::sniper_demo_booked_audit::inspect(
                &db,&key,&artifact,&hash).unwrap();
            assert_eq!(booked["physical_broker_trades"],1);
            assert_eq!(booked["local_broker_deal_receipts_reconciled"],true);
            assert!((booked["net_demo_account_currency"].as_f64().unwrap()-1.0).abs()<1e-8);
            assert_eq!(booked["matched_with_frozen_markouts"],false);
            learner.observe(&db,&t,&config,&[],&BTreeMap::new(),false,now+1).unwrap();
            assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples",
                [],|r|r.get::<_,u64>(0)).unwrap(),2);
        }
        {
            let db=Connection::open(&path).unwrap();
            let mut learner=Learner::new(&db).unwrap();
            learner.observe(&db,&t,&config,&[],&BTreeMap::new(),false,now+2).unwrap();
            assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples",
                [],|r|r.get::<_,u64>(0)).unwrap(),2);
            let mut another=t.clone();another.login+=1;
            assert!(crate::sniper_demo_booked_audit::inspect(
                &db,&context(&another),&artifact,&hash).is_err());
        }
        let _=std::fs::remove_file(path);
    }

    #[test]
    fn coordinator_owned_sniper_close_trains_the_specialist_from_exact_demo_deals_once() {
        let db = Connection::open_in_memory().unwrap();
        let _learner = Learner::new(&db).unwrap();
        db.execute_batch(
            "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,result TEXT NOT NULL DEFAULT '{}');
             CREATE TABLE broker_deals(context TEXT NOT NULL,ticket INTEGER NOT NULL,position_id INTEGER NOT NULL,brain TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,ticket));"
        ).unwrap();
        let now = 1_800_000_000_000u64;
        let t = Telemetry {
            connected:true, demo:true, account_trade_mode:Some(0),
            login:99, server:"Demo".into(), currency:"USD".into(), symbol:"XAUUSD".into(),
            tick_size:Some(0.01), tick_value_profit:Some(1.), tick_value_loss:Some(1.),
            ..Default::default()
        };
        let key = context(&t);
        let command = json!({
            "id":"sniper-coordinator-1","brain":"system",
            "signal":{
                "bar_time":1000,"side":1,"stop_distance":0.2,"target_distance":0.2,
                "reason":"confirmed quote momentum",
                "coordination":{
                    "trading_mode":"sniper",
                    "selection":{
                        "brain":"sniper_learner","policy":"sniper-ticks-v1",
                        "timeframe":"M1","sniper_policy":"impulse",
                        "sniper_definition":"fixture-pip-setup","sniper_regime":"directional_normal",
                        "sniper_target_mode":"adaptive","sniper_target_arm":2,
                        "sniper_target_pips":5.0
                    }
                }
            },
            "learning":{
                "trading_mode":"sniper","timeframe":"M1","report_id":"",
                "tick_size":0.01,"tick_value_loss":1.0,"tick_value_profit":1.0,
                "weight_at_entry":1.0
            }
        });
        let result = json!({"deal":800,"volume":0.1,"price":2000.0,
            "request":{"price":2000.0,"tp":2000.2,"sl":1999.8}});
        db.execute(
            "INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result)
             VALUES('sniper-coordinator-1',?1,'system','open',?2,'observed',?3,?4)",
            params![key,now-60_000,command.to_string(),result.to_string()]
        ).unwrap();
        for (ticket,entry,profit,commission,stamp) in [
            (800,0,0.0,-0.20,now-60_000),
            (801,1,1.5,-0.20,now-1_000)
        ] {
            let payload = json!({"ticket":ticket,"position_id":777,"magic":410600,
                "type":if entry==0 {0}else{1},"entry":entry,"time_msc":stamp,"volume":0.1,"profit":profit,
                "commission":commission,"swap":0.0,"fee":0.0,"brain":"system"});
            db.execute(
                "INSERT INTO broker_deals(context,ticket,position_id,brain,payload)
                 VALUES(?1,?2,777,'system',?3)",
                params![key,ticket,payload.to_string()]
            ).unwrap();
        }
        for _ in 0..2 {sync_demo_broker_evidence(&db,&t).unwrap();}
        let rows:Vec<(String,String)> = db.prepare(
            "SELECT brain,payload FROM coordinator_broker_samples WHERE context=?1 ORDER BY brain"
        ).unwrap().query_map([&key],|r|Ok((r.get(0)?,r.get(1)?)))
            .unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(rows.len(),2); // same physical close; distinct system and specialist evidence
        assert_eq!(rows[0].0,"sniper_learner");
        assert_eq!(rows[1].0,"system");
        let specialist:Value=serde_json::from_str(&rows[0].1).unwrap();
        assert_eq!(specialist["trade_owner"],"system");
        assert_eq!(specialist["evidence_source"],"mt5_demo_broker");
        assert_eq!(specialist["sniper_tp_preserved"],true);
        assert!((specialist["net"].as_f64().unwrap()-1.10).abs()<1e-9);
        let mut engine=braintrade_domain::sniper::Engine::default();
        crate::sniper_learning::refresh(&mut engine,&db,&t,now).unwrap();
        let policy=engine.models.values().next().unwrap();
        assert_eq!(policy[0].samples,1);
        let targets=engine.target_models.values().next().unwrap();
        assert_eq!(targets[2].samples,1);
        assert!(targets.iter().enumerate().all(|(arm,p)|arm==2||p.samples==0));
        // Idempotent re-processing never increases samples.
        sync_demo_broker_evidence(&db,&t).unwrap();
        crate::sniper_learning::refresh(&mut engine,&db,&t,now).unwrap();
        assert_eq!(engine.target_models.values().next().unwrap()[2].samples,1);
        // Migrating an older installation: the system outcome and processed
        // marker already exist, but the specialist credit was absent.
        db.execute("DELETE FROM coordinator_broker_samples WHERE brain='sniper_learner'",[]).unwrap();
        sync_demo_broker_evidence(&db,&t).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples WHERE brain='sniper_learner'",[],|r|r.get::<_,u64>(0)).unwrap(),1);
        crate::sniper_learning::refresh(&mut engine,&db,&t,now).unwrap();
        assert_eq!(engine.target_models.values().next().unwrap()[2].samples,1);
        sync_demo_broker_evidence(&db,&t).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples WHERE brain='sniper_learner'",[],|r|r.get::<_,u64>(0)).unwrap(),1);
    }

}
