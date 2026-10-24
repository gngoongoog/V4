//! V20 institutional entry supervisor. One execution owner remains authoritative.
//! This module NEVER opens/closes a trade or automatically switches trading mode.
//! Its optional gate can only prevent NEW broker entries. MT5 reconciliation,
//! close orders, trailing protection and existing positions remain supervised.
use anyhow::{bail, Result};
use braintrade_domain::{trading::TradingMode, Candle, Config, Telemetry};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Settings {
    /// Strictly opt-in. Existing installations retain their original entry path.
    pub enforce_entry_gate: bool,
    pub max_quote_age_ms: u64,
    pub max_spread_atr: f64,
    pub min_closed_bars: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self { enforce_entry_gate: false, max_quote_age_ms: 2500,
               max_spread_atr: 0.30, min_closed_bars: 20 }
    }
}
impl Settings {
    fn validate(&self) -> Result<()> {
        if !(250..=3000).contains(&self.max_quote_age_ms) { bail!("Quote age limit must be 250..3000 ms"); }
        if !self.max_spread_atr.is_finite() || !(0.01..=1.0).contains(&self.max_spread_atr) {
            bail!("Spread/ATR limit must be 0.01..1.0");
        }
        if !(15..=100).contains(&self.min_closed_bars) { bail!("Minimum closed candles must be 15..100"); }
        Ok(())
    }
}
fn initialize(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS enterprise_supervisor_config (id INTEGER PRIMARY KEY CHECK(id=1), settings TEXT NOT NULL);")?;
    Ok(())
}
pub(crate) fn load(db: &Connection) -> Result<Settings> {
    initialize(db)?;
    let raw: Option<String> = db.query_row("SELECT settings FROM enterprise_supervisor_config WHERE id=1",[],|r|r.get(0)).optional()?;
    match raw {
        Some(raw) => {let v: Settings=serde_json::from_str(&raw)?;v.validate()?;Ok(v)},
        None => Ok(Settings::default()),
    }
}
/// Fail closed on unknown broker exposure. Lack of telemetry does NOT prove
/// an account is flat; settings can otherwise weaken guards while the broker
/// holds positions that were never reconciled after a disconnect.
pub(crate) fn broker_flat_is_authoritative(
    telemetry: Option<&Telemetry>, gateway_fresh: bool, bound_account_verified: bool,
) -> bool {
    gateway_fresh && bound_account_verified && telemetry.is_some_and(|t|
        t.connected && t.positions.is_empty() && t.pending_orders==0)
}
pub(crate) fn save(db: &Connection, body: &[u8], stopped: bool, broker_flat: bool, settled: bool) -> Result<Value> {
    // A gate change during a position or unresolved order would invalidate
    // monitoring evidence and may create inconsistent entry authority.
    if !stopped || !broker_flat || !settled {
        bail!("STOP, close broker positions and reconcile all MT5 commands before changing supervisor policy");
    }
    let settings:Settings=serde_json::from_slice(body)?;
    settings.validate()?;
    initialize(db)?;
    db.execute("INSERT INTO enterprise_supervisor_config(id,settings) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET settings=excluded.settings",[serde_json::to_string(&settings)?])?;
    Ok(json!({"ok":true,"settings":settings,"scope":"new_entries_only","automatic_mode_handoff":false}))
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Assessment {
    pub(crate) entry_allowed: bool,
    pub(crate) guards: Vec<String>,
    pub(crate) market_regime: &'static str,
    pub(crate) recommended_mode: Option<TradingMode>,
    pub(crate) recommendation_is_advisory: bool,
    pub(crate) observed_spread_to_atr: Option<f64>,
    pub(crate) m1_efficiency: Option<f64>,
    pub(crate) m5_efficiency: Option<f64>,
    pub(crate) trend_alignment: bool,
}
fn efficiency(bars: &[Candle]) -> Option<(f64, i8)> {
    if bars.len()<16 {return None;}
    let v=&bars[bars.len()-16..];
    let travel: f64=v.windows(2).map(|w|(w[1].close-w[0].close).abs()).sum();
    if !travel.is_finite() || travel <= 0.0 {return None;}
    let change=v.last()?.close-v.first()?.close;
    let score=(change.abs()/travel).clamp(0.0,1.0);
    Some((score, if change > 0.0 {1} else if change < 0.0 {-1} else {0}))
}
fn valid_series<'a>(frames: &'a BTreeMap<String,Vec<Candle>>, key: &str, fallback: &'a [Candle]) -> &'a [Candle] {
    frames.get(key).map(Vec::as_slice).filter(|v| !v.is_empty()).unwrap_or(fallback)
}
/// Conservative admission checks for broker candles. A fresh snapshot alone is
/// insufficient: the underlying last CLOSED candle may be hours/days old.
/// Timestamp limits include the normal duration of each timeframe and an
/// allowance for scheduling jitter (not a prediction about market hours).
fn frame_max_age_seconds(tf: &str) -> u64 {
    match tf {
        "M1" => 3 * 60,
        "M5" => 15 * 60,
        "M15" => 45 * 60,
        "H1" => 3 * 3600,
        "H4" => 12 * 3600,
        // Daily frames may cross the weekend, but quotes still must be fresh.
        "D1" => 4 * 86400,
        _ => 0,
    }
}
fn trustworthy_candles(series: &[Candle], tf: &str, min: usize, now: u64) -> Result<()> {
    if series.len() < min {
        bail!("insufficient closed candles ({}/{min})", series.len());
    }
    let checked = &series[series.len()-min..];
    let now_s = now / 1000;
    let last = checked.last().expect("minimum closed candles > 0").time;
    if last == 0 || last > now_s.saturating_add(120) ||
        now_s.saturating_sub(last) > frame_max_age_seconds(tf) {
        bail!("last closed candle is stale or in the future");
    }
    if checked.iter().any(|b| {
        b.time == 0 ||
        ![b.open,b.high,b.low,b.close].iter().all(|v|v.is_finite() && *v>0.0) ||
        b.low>b.open.min(b.close) || b.high<b.open.max(b.close) ||
        b.high < b.low
    }) {
        bail!("invalid OHLC candle evidence");
    }
    if checked.windows(2).any(|pair|pair[0].time>=pair[1].time) {
        bail!("non-monotonic or duplicate candle timestamps");
    }
    Ok(())
}
/// Pure deterministic evidence, not a model win probability or profit forecast.
pub(crate) fn assess(settings: &Settings, t: Option<&Telemetry>, c: &Config,
           frames: &BTreeMap<String,Vec<Candle>>, now: u64) -> Assessment {
    let mut guards=Vec::new();
    let mut regime="unverified";
    let mut recommendation=None;
    let mut spread_atr=None;
    let mut e1=None;
    let mut e5=None;
    let mut aligned=false;
    if let Some(t)=t {
        if !t.connected || t.login==0 || t.server.is_empty() || t.symbol!=c.symbol {
            guards.push("MT5 account identity / symbol is not verified".into());
        }
        if !t.quote_is_fresh(now, settings.max_quote_age_ms) {
            guards.push("Broker quote missing, undated or stale".into());
        }
        let quote=t.bid.zip(t.ask).filter(|(bid,ask)|bid.is_finite() && ask.is_finite() && *bid>0. && *ask>=*bid);
        if quote.is_none() { guards.push("Executable Bid/Ask invalid".into()); }
        let market=t.market.as_ref();
        if market.is_none_or(|m| (!m.error.is_empty() &&
                    !(matches!(c.trading.mode,TradingMode::Scalping|TradingMode::Sniper) &&
                    braintrade_domain::trading::missing_unrequired_m15_only(&m.error))) ||
                    m.captured_ms==0 || m.captured_ms>now.saturating_add(2000)
                    || now.saturating_sub(m.captured_ms)>15_000) {
            guards.push("Market candle snapshot missing or older than 15 seconds".into());
        }
        let m1=valid_series(frames,"M1",market.map(|m|m.m1.as_slice()).unwrap_or(&[]));
        let m5=valid_series(frames,"M5",market.map(|m|m.m5.as_slice()).unwrap_or(&[]));
        let m15=valid_series(frames,"M15",market.map(|m|m.m15.as_slice()).unwrap_or(&[]));
        let min=settings.min_closed_bars;
        let h1=frames.get("H1").map(Vec::as_slice).unwrap_or(&[]);
        let h4=frames.get("H4").map(Vec::as_slice).unwrap_or(&[]);
        let d1=frames.get("D1").map(Vec::as_slice).unwrap_or(&[]);
        let required: &[(&str,&[Candle])] = match c.trading.mode {
            TradingMode::Sniper => &[("M1",m1)],
            TradingMode::Scalping => &[("M1",m1),("M5",m5)],
            TradingMode::Expert => &[("M1",m1),("M5",m5),("M15",m15)],
            TradingMode::Swing => &[("H1",h1),("H4",h4),("D1",d1)],
        };
        for (name,series) in required {
            if let Err(e)=trustworthy_candles(series,name,min,now) {
                guards.push(format!("{name}: {e}"));
            }
        }
        // For Swing use H1 ATR, not micro-timeframe M1 volatility. This
        // prevents a valid multi-day setup from inheriting an M1 spread veto.
        let (atr_tf,atr_bars)=if c.trading.mode==TradingMode::Swing {("H1",h1)} else {("M1",m1)};
        let atr=if trustworthy_candles(atr_bars,atr_tf,min,now).is_ok() {
            braintrade_domain::trading::volatility(atr_bars)
        } else {0.0};
        if let Some((bid,ask))=quote {
            if !atr.is_finite() || atr<=0.0 {guards.push("M1 ATR unavailable to validate spread".into());}
            else {
                let ratio=(ask-bid)/atr;
                spread_atr=Some(ratio);
                if ratio>settings.max_spread_atr {guards.push(format!("Spread/ATR {ratio:.3} exceeds supervisor ceiling {:.3}",settings.max_spread_atr));}
            }
        }
        let d1=trustworthy_candles(m1,"M1",min,now).ok().and_then(|_|efficiency(m1));
        let d5=trustworthy_candles(m5,"M5",min,now).ok().and_then(|_|efficiency(m5));
        let d15=trustworthy_candles(m15,"M15",min,now).ok().and_then(|_|efficiency(m15));
        e1=d1.map(|x|x.0);
        e5=d5.map(|x|x.0);
        aligned=d1.zip(d5).is_some_and(|(a,b)|a.1!=0 && a.1==b.1 && a.0>=0.45 && b.0>=0.35);
        if let (Some(a),Some(b),Some(d))=(d1,d5,d15) {
            if aligned && a.1==d.1 && b.1==d.1 && d.0>=0.35 {
                regime="aligned_trend";recommendation=Some(TradingMode::Expert);
            } else if aligned {
                regime="short_trend";recommendation=Some(TradingMode::Scalping);
            } else if a.0>=0.65 && b.0<0.25 {
                regime="m1_impulse_unconfirmed";recommendation=Some(TradingMode::Sniper);
            } else {regime="chop_or_transition";recommendation=None;}
        }
        // Swing never receives an automatic recommendation based only on M1/M5/M15.
        // A supervised H1/H4/D1 backtest and forward run are required first.
    } else {guards.push("No reconciled MT5 telemetry".into());}
    // Never publish a trading recommendation on unverifiable account/market
    // data, even though the optional enforcement switch may still be OFF.
    if !guards.is_empty() {regime="unverified";recommendation=None;}
    Assessment {entry_allowed:guards.is_empty(),guards,market_regime:regime,
        recommended_mode:recommendation,recommendation_is_advisory:true,
        observed_spread_to_atr:spread_atr,m1_efficiency:e1,m5_efficiency:e5,trend_alignment:aligned}
}
pub(crate) fn report(db:&Connection,t:Option<&Telemetry>,c:&Config,
                 frames:&BTreeMap<String,Vec<Candle>>,now:u64)->Result<Value> {
    // Invalid or damaged persisted policy must never be advertised as a disabled
    // guard. Execution::Service loads the very same fail-closed policy on boot.
    // Keep this read-only endpoint available for diagnosis and STOPPED repair.
    let (settings, policy_error) = match load(db) {
        Ok(settings) => (settings, None),
        Err(error) => {
            let mut safe=Settings::default();
            safe.enforce_entry_gate=true;
            (safe, Some(format!("Stored supervisor policy is invalid: {error}")))
        }
    };
    let mut assessment=assess(&settings,t,c,frames,now);
    if policy_error.is_some() {
        assessment.entry_allowed=false;
        assessment.market_regime="unverified";
        assessment.recommended_mode=None;
        assessment.guards.push("Supervisor policy integrity failure; new entries blocked".into());
    }
    // This is the supervisor's advisory/optional *entry gate*, not the actual
    // broker's execution authorization. START, account identity and risk gates
    // must still independently succeed before a live order can be sent.
    let entry_gate_permits_new_entries=!settings.enforce_entry_gate || assessment.entry_allowed;
    Ok(json!({"settings":settings,"assessment":assessment,
        "active_execution_mode":c.trading.mode,
        "configuration_integrity":if policy_error.is_some() {"invalid_fail_closed"} else {"ok"},
        "configuration_error":policy_error,
        "entry_gate_enforced":settings.enforce_entry_gate,
        "entry_gate_permits_new_entries":entry_gate_permits_new_entries,
        "effective_entry_permission":entry_gate_permits_new_entries,
        "broker_execution_authorization":"not_evaluated_by_this_read_only_endpoint",
        "advisory_only":"Mode recommendations never switch execution ownership; research confidence is NOT a success probability",
        "preserves_existing_positions":true,"version":"enterprise-supervisor-v22"}))
}

#[cfg(test)]
mod tests {
 use super::*;
 fn sample(now:u64)->(Telemetry,Config) {
    let mut t=Telemetry::default();t.connected=true;t.login=20;t.server="DemoSrv".into();t.symbol="XAUUSD".into();
    t.tick_age_ms=Some(50);t.quote_time_ms=Some(now-50);t.bid=Some(3000.);t.ask=Some(3000.15);
    let mk=|n:u64,period:u64| (0..n).map(|i|Candle{time:now/1000-(n-i)*period,open:2990.+i as f64*0.25,high:2990.5+i as f64*0.25,low:2989.5+i as f64*0.25,close:2990.25+i as f64*0.25,tick_volume:100}).collect::<Vec<_>>();
    t.market=Some(braintrade_domain::MarketData{captured_ms:now,m1:mk(40,60),m5:mk(40,300),m15:mk(40,900),error:String::new()});
    (t,Config::default())
 }
 #[test] fn default_is_advisory_and_missing_data_fail_closed_if_armed(){let c=Config::default();let s=Settings::default();assert!(!s.enforce_entry_gate);assert!(!assess(&s,None,&c,&BTreeMap::new(),200_000).entry_allowed);}
 #[test] fn detects_valid_trend_but_does_not_change_owner(){let now=1_750_000_000_000;let(t,c)=sample(now);let a=assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now);assert!(a.entry_allowed,"{:?}",a.guards);assert_eq!(a.market_regime,"aligned_trend");assert_eq!(a.recommended_mode,Some(TradingMode::Expert));assert!(a.recommendation_is_advisory);}
 #[test] fn stale_price_never_becomes_new_with_fresh_heartbeat(){let now=1_750_000_000_000;let(mut t,c)=sample(now);t.tick_age_ms=Some(7000);t.quote_time_ms=Some(now-7000);assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);}
 #[test] fn no_swing_without_independent_higher_frames(){let now=1_750_000_000_000;let(t,mut c)=sample(now);c.trading.mode=TradingMode::Swing;let a=assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now);assert!(!a.entry_allowed);assert!(a.guards.iter().any(|v|v.contains("H4")));}
 #[test] fn save_requires_stopped_flat_settled_and_audits_settings(){let db=Connection::open_in_memory().unwrap();assert!(save(&db,br#"{"enforce_entry_gate":true}"#,false,true,true).is_err());assert!(save(&db,br#"{"enforce_entry_gate":true}"#,true,false,true).is_err());assert!(save(&db,br#"{"enforce_entry_gate":true}"#,true,true,false).is_err());assert!(save(&db,br#"{"enforce_entry_gate":true,"max_quote_age_ms":9999}"#,true,true,true).is_err());assert!(save(&db,br#"{"enforce_entry_gate":true}"#,true,true,true).is_ok());assert!(load(&db).unwrap().enforce_entry_gate);}
 #[test] fn unrelated_m15_does_not_veto_short_mode_but_still_vetoes_expert(){
    let now=1_750_000_000_000;
    let (mut t,mut c)=sample(now);
    c.trading.mode=TradingMode::Scalping;
    t.market.as_mut().unwrap().error="M15: waiting for 100 closed candles".into();
    assert!(assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
    c.trading.mode=TradingMode::Expert;
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
    c.trading.mode=TradingMode::Scalping;
    t.market.as_mut().unwrap().error="M15: missing; M5: old".into();
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
 }
 #[test] fn spread_guard_blocks_new_entries(){let now=1_750_000_000_000;let(mut t,c)=sample(now);t.ask=Some(3015.);let a=assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now);assert!(!a.entry_allowed);assert!(a.guards.iter().any(|v|v.contains("Spread/ATR")));}
 #[test] fn fresh_snapshot_with_old_candles_must_not_authorize_new_entries(){
    let now=1_750_000_000_000;
    let (mut t,c)=sample(now);
    let m=t.market.as_mut().unwrap();
    for x in &mut m.m1{x.time=x.time.saturating_sub(3600);}
    // Captured 0ms ago does not turn historical candles into fresh market data.
    assert_eq!(m.captured_ms,now);
    let a=assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now);
    assert!(!a.entry_allowed);
    assert!(a.guards.iter().any(|reason|reason.contains("M1")&&reason.contains("stale")));
    assert!(a.recommended_mode.is_none());
 }
 #[test] fn out_of_order_duplicate_or_malformed_bar_never_passes(){
    let now=1_750_000_000_000;
    let (mut t,c)=sample(now);
    {
        let m=t.market.as_mut().unwrap();
        m.m1[35].time=m.m1[34].time;
    }
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
    {
        let m=t.market.as_mut().unwrap();
        m.m1[35].time=m.m1[34].time+60;
        m.m1[35].high=m.m1[35].low-1.0;
    }
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
 }
 #[test] fn stale_m5_or_m15_never_produces_an_expert_recommendation(){
    let now=1_750_000_000_000;
    let (mut t,mut c)=sample(now);
    c.trading.mode=TradingMode::Sniper;
    for x in &mut t.market.as_mut().unwrap().m5{x.time=x.time.saturating_sub(36000);}
    let a=assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now);
    assert!(a.entry_allowed,"Sniper only requires fresh M1: {:?}",a.guards);
    assert!(a.recommended_mode.is_none(),"Old M5 must not recommend another mode");
 }
 #[test] fn swing_uses_h1_atr_and_requires_higher_frame_time_evidence(){
    let now=1_750_000_000_000;
    let (t,mut c)=sample(now);
    c.trading.mode=TradingMode::Swing;
    let mut frames=BTreeMap::new();
    let mk=|period:u64| (0..30u64).map(|i|Candle{time:now/1000-(30-i)*period,
        open:3000.0+i as f64,high:3001.0+i as f64,low:2999.0+i as f64,
        close:3000.5+i as f64,tick_volume:100}).collect::<Vec<_>>();
    frames.insert("H1".to_owned(),mk(3600));
    frames.insert("H4".to_owned(),mk(14400));
    frames.insert("D1".to_owned(),mk(86400));
    let a=assess(&Settings::default(),Some(&t),&c,&frames,now);
    assert!(a.entry_allowed,"Valid swing evidence denied: {:?}",a.guards);
    assert!(a.observed_spread_to_atr.is_some());
    frames.get_mut("H4").unwrap().last_mut().unwrap().time-=500000;
    let a=assess(&Settings::default(),Some(&t),&c,&frames,now);
    assert!(!a.entry_allowed);
 }

 #[test] fn broker_state_unknown_never_proves_flat(){
    let now=1_750_000_000_000;
    let (mut t,_)=sample(now);
    assert!(!broker_flat_is_authoritative(None,true,true));
    assert!(!broker_flat_is_authoritative(Some(&t),false,true));
    assert!(!broker_flat_is_authoritative(Some(&t),true,false));
    assert!(broker_flat_is_authoritative(Some(&t),true,true));
    t.pending_orders=1;
    assert!(!broker_flat_is_authoritative(Some(&t),true,true));
 }
 #[test] fn corrupt_persisted_policy_reports_blocked_instead_of_going_blank(){
    let now=1_750_000_000_000;
    let db=Connection::open_in_memory().unwrap();
    initialize(&db).unwrap();
    db.execute("INSERT INTO enterprise_supervisor_config(id,settings) VALUES(1,?1)",
        [r#"{"enforce_entry_gate":"not-a-bool"}"#]).unwrap();
    let (t,c)=sample(now);
    let state=report(&db,Some(&t),&c,&BTreeMap::new(),now).unwrap();
    assert_eq!(state["configuration_integrity"],"invalid_fail_closed");
    assert_eq!(state["entry_gate_enforced"],true);
    assert_eq!(state["entry_gate_permits_new_entries"],false);
    assert_eq!(state["assessment"]["entry_allowed"],false);
    assert!(state["assessment"]["recommended_mode"].is_null());
    assert_eq!(state["preserves_existing_positions"],true);
 }
 #[test] fn optional_entry_guard_never_claims_to_be_broker_authorization(){
    let now=1_750_000_000_000;
    let db=Connection::open_in_memory().unwrap();
    let (t,c)=sample(now);
    let state=report(&db,Some(&t),&c,&BTreeMap::new(),now).unwrap();
    assert_eq!(state["configuration_integrity"],"ok");
    assert_eq!(state["entry_gate_enforced"],false);
    assert_eq!(state["broker_execution_authorization"],"not_evaluated_by_this_read_only_endpoint");
 }

}
