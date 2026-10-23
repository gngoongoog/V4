//! Trading profiles and executable-quote costs; account currency, round-trip commission.
use crate::{Analysis, Candle, Config, Telemetry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradingMode {
    Scalping,
    Swing,
    #[default]
    Expert,
    Sniper,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SniperBrain { #[default] Classic, Learner }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SniperTargetMode { #[default] Fixed, Adaptive }
/// Separate signal families; these do not change the gateway polling interval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SniperStyle { #[default] Auto, Impulse, Rebound, Confirmed }
impl SniperStyle {
    pub fn label(self) -> &'static str { match self {
        Self::Auto => "تلقائي متعلّم", Self::Impulse => "اندفاع سريع",
        Self::Rebound => "ارتداد سريع", Self::Confirmed => "اندفاع مؤكد",
    }}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TradingConfig {
    pub mode: TradingMode,
    /// Opt-in priority router. Legacy saved accounts remain manual by default.
    pub priority_enabled: bool,
    /// Only Sniper OR the selected fallback is allowed to own new entries.
    pub sniper_active: bool,
    /// Secondary entry coordinator when the operator pauses the Sniper.
    pub fallback_mode: TradingMode,
    pub sniper_brain: SniperBrain,
    pub sniper_style: SniperStyle,
    pub sniper_target_mode: SniperTargetMode,
    pub sniper_min_target_pips: f64,
    pub sniper_max_target_pips: f64,
    /// Demo only: select actionable advisors adaptively and shrink oversized orders.
    pub demo_flexible: bool,
    /// Demo-only prospective exit learning; no effect on already open positions.
    pub adaptive_exits: bool,
    pub sniper_target_points: f64,
    pub sniper_stop_points: f64,
    pub sniper_cooldown_seconds: u64,
    pub sniper_max_hold_seconds: u64,
    pub pip_points: f64,
    /// Explicit price units for one pip, independent of broker quote digits.
    /// XAUUSD convention here: 1 pip = 0.10 in gold price, not account PnL.
    pub sniper_pip_price: f64,
    pub max_cost_fraction: f64,
}
impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            mode: TradingMode::Expert,
            priority_enabled: false,
            sniper_active: true,
            fallback_mode: TradingMode::Expert,
            sniper_brain: SniperBrain::Classic,
            sniper_style: SniperStyle::Auto,
            sniper_target_mode: SniperTargetMode::Fixed,
            sniper_min_target_pips: 2.0,
            sniper_max_target_pips: 10.0,
            demo_flexible: true,
            adaptive_exits: true,
            sniper_target_points: 50.,
            sniper_stop_points: 80.,
            sniper_cooldown_seconds: 0,
            sniper_max_hold_seconds: 120,
            pip_points: 10.,
            sniper_pip_price: 0.10,
            max_cost_fraction: 0.45,
        }
    }
}
impl TradingConfig {
    pub fn priority_mode(&self) -> TradingMode {
        if !self.priority_enabled { self.mode }
        else if self.sniper_active { TradingMode::Sniper }
        else { self.fallback_mode }
    }
    pub fn select_priority_owner(&mut self) {
        if self.priority_enabled {self.mode=self.priority_mode();}
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.fallback_mode == TradingMode::Sniper {
            return Err("Secondary mode must be Scalping, Swing or Expert; never Sniper".into());
        }
        if self.priority_enabled && self.mode != self.priority_mode() {
            return Err("Trading priority has conflicting execution owners".into());
        }
        for (name, v, min, max) in [
            ("Target points", self.sniper_target_points, 1., 100_000.),
            ("Stop points", self.sniper_stop_points, 1., 100_000.),
            ("Points per pip", self.pip_points, 1., 1000.),
            ("Sniper pip price", self.sniper_pip_price, 0.00001, 1000.),
            ("Cost fraction", self.max_cost_fraction, 0.01, 0.9),
            ("Adaptive min target pips", self.sniper_min_target_pips, 0.1, 1000.),
            ("Adaptive max target pips", self.sniper_max_target_pips, 0.1, 1000.),
        ] {
            if !v.is_finite() || !(min..=max).contains(&v) {
                return Err(format!("{name} must be {min}–{max}"));
            }
        }
        if self.sniper_max_target_pips <= self.sniper_min_target_pips {
            return Err("Adaptive Sniper max target must exceed min target in pips".into());
        }
        if !(0..=3600).contains(&self.sniper_cooldown_seconds) {
            return Err("Cooldown must be 0–3600 seconds".into());
        }
        if !(1..=3600).contains(&self.sniper_max_hold_seconds) {
            return Err("Sniper holding time must be 1–3600 seconds".into());
        }
        Ok(())
    }
    pub fn hold_ms(&self) -> u64 {
        match self.mode {
            TradingMode::Expert => 1_800_000,
            TradingMode::Scalping => 900_000,
            TradingMode::Swing => 259_200_000,
            TradingMode::Sniper => self.sniper_max_hold_seconds * 1000,
        }
    }
    pub fn geometry(&self, atr: f64, point: f64) -> (f64, f64) {
        match self.mode {
            TradingMode::Expert => (1.5 * atr, 3. * atr),
            TradingMode::Scalping => (1.2 * atr, 1.68 * atr),
            TradingMode::Swing => (2. * atr, 5. * atr),
            TradingMode::Sniper => (
                self.sniper_stop_points * point,
                (self.sniper_target_points / self.pip_points) * self.sniper_pip_price,
            ),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct CostPreview {
    pub currency: String,
    pub lots: f64,
    pub spread_points: f64,
    pub spread_pips: f64,
    /// Bid/Ask price difference, not a monetary amount.
    pub spread_price: f64,
    pub point_price: f64,
    pub pip_price: f64,
    pub bid: f64,
    pub ask: f64,
    pub spread_money: f64,
    pub commission: f64,
    pub slippage_money: f64,
    pub total: f64,
    /// Quote movement from the PRE-entry Bid (buy) or Ask (sell), including spread once.
    pub break_even_points: f64,
    pub break_even_pips: f64,
    /// Movement needed after the entry Ask (buy) or Bid (sell), excluding the already-paid spread.
    pub break_even_from_entry_points: f64,
    pub break_even_from_entry_pips: f64,
    pub buy_break_even_bid: f64,
    pub sell_break_even_ask: f64,
    pub commission_round_trip_per_lot: f64,
    pub slippage_ticks_per_side: f64,
    pub desired_net_target_money: f64,
}
pub fn costs(t: &Telemetry, c: &Config, lots: f64, now: u64) -> Result<CostPreview, String> {
    c.validate()?;
    if !lots.is_finite() || lots <= 0. || lots > 100. {
        return Err("Lots must be positive and at most 100".into());
    }
    if !t.connected
        || t.tick_age_ms.is_none_or(|v| v > 3000)
        || t.quote_time_ms
            .is_some_and(|v| v > now + 2000 || now.saturating_sub(v) > 3000)
    {
        return Err("Current MT5 quote required".into());
    }
    let pos = |v: Option<f64>| {
        v.filter(|v| v.is_finite() && *v > 0.)
            .ok_or_else(|| "Broker contract/quote unavailable".to_string())
    };
    let (bid, ask, point, tick, loss, profit) = (
        pos(t.bid)?,
        pos(t.ask)?,
        pos(t.point)?,
        pos(t.tick_size)?,
        pos(t.tick_value_loss)?,
        pos(t.tick_value_profit)?,
    );
    if ask < bid {
        return Err("Crossed quote".into());
    }
    let spread_price = ask - bid;
    let spread_points = spread_price / point;
    // The quote spread is the initial loss from an Ask buy / Bid sell; use the
    // broker's loss-side tick value rather than max(loss, profit).
    let spread_money = spread_price / tick * loss * lots;
    let commission = crate::commission::round_trip_charge(c.risk.commission_per_lot, lots)?;
    // Assumed slippage is separate from the actual Bid/Ask spread. A user may
    // explicitly enter 0 ticks to exclude it from a hypothetical preview.
    let slippage_money = 2. * c.risk.slippage_ticks * loss.max(profit) * lots;
    let total = spread_money + commission + slippage_money;
    // Starting from the current Bid for buys (Ask for sells), add the observed
    // spread ONCE, then only commission and hypothetical slippage in profit
    // ticks. Dividing spread_money by tick_value_profit incorrectly inflates
    // the price distance when broker profit/loss tick values differ.
    let break_even_from_entry_points = (commission + slippage_money) / (profit * lots) * tick / point;
    let break_even_points = spread_points + break_even_from_entry_points;
    let r = CostPreview {
        currency: t.currency.clone(),
        lots,
        spread_points,
        spread_pips: spread_price / c.trading.sniper_pip_price,
        spread_price,
        point_price: point,
        pip_price: c.trading.sniper_pip_price,
        bid,
        ask,
        spread_money,
        commission,
        slippage_money,
        total,
        break_even_points,
        break_even_pips: break_even_points * point / c.trading.sniper_pip_price,
        break_even_from_entry_points,
        break_even_from_entry_pips: break_even_from_entry_points * point / c.trading.sniper_pip_price,
        buy_break_even_bid: bid + break_even_points * point,
        sell_break_even_ask: ask - break_even_points * point,
        commission_round_trip_per_lot: c.risk.commission_per_lot,
        slippage_ticks_per_side: c.risk.slippage_ticks,
        desired_net_target_money: c.trading.sniper_target_points / c.trading.pip_points * c.trading.sniper_pip_price / tick * profit * lots,
    };
    if [total, break_even_points, r.buy_break_even_bid, r.sell_break_even_ask, r.desired_net_target_money]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err("Cost calculation overflow".into());
    }
    Ok(r)
}
/// A fixed user pip target is an exact gross distance from the executable
/// entry (Ask for buy / Bid for sell). Never extend it for commission, slippage,
/// spread, or exit-learning: such costs are handled by the entry cost gate.
pub fn sniper_target_from_entry(_cost: &CostPreview, desired_price_distance: f64) -> Result<f64, String> {
    if !desired_price_distance.is_finite() || desired_price_distance <= 0. {
        return Err("Sniper target price distance must be positive".into());
    }
    Ok(desired_price_distance)
}

pub fn volatility(b: &[Candle]) -> f64 {
    if b.len() < 15 {
        return 0.;
    }
    b[b.len() - 15..]
        .windows(2)
        .map(|w| {
            (w[1].high - w[1].low)
                .max((w[1].high - w[0].close).abs())
                .max((w[1].low - w[0].close).abs())
        })
        .sum::<f64>()
        / 14.
}
pub fn direction(b: &[Candle]) -> i8 {
    if b.len() < 60 {
        return 0;
    }
    let ema = |p: usize| {
        b.iter()
            .skip(1)
            .fold(b[0].close, |a, c| a + 2. / (p as f64 + 1.) * (c.close - a))
    };
    let d = ema(20) - ema(50);
    let a = volatility(b);
    if d > a * 0.15 {
        1
    } else if d < -a * 0.15 {
        -1
    } else {
        0
    }
}
/// Validate the two frames actually used by Scalping/Sniper when full research
/// was unavailable solely because its separate M15 requirement did not mature.
/// Do not waive current snapshots, completed bars or gaps to force a trade.
fn short_mode_frames_current(m: &crate::MarketData, now: u64) -> bool {
    if !m.error.is_empty() || m.captured_ms > now
        || now.saturating_sub(m.captured_ms) > 15_000 { return false; }
    for (series, seconds) in [(&m.m1, 60u64), (&m.m5, 300u64)] {
        if series.len() < 100 || series.len() > 300
            || series.iter().any(|bar| !bar.valid())
            || series.windows(2).any(|w| w[0].time >= w[1].time) { return false; }
        let Some(last)=series.last() else { return false; };
        let close_ms=last.time.saturating_add(seconds).saturating_mul(1000);
        if close_ms>now || now.saturating_sub(close_ms)>seconds*1000+20_000 { return false; }
        if series[series.len()-30..].windows(2).any(|w|w[1].time-w[0].time!=seconds) {
            return false;
        }
    }
    true
}

/// Higher frames have passed account, closure and freshness checks in Core.
pub fn select(
    mut a: Analysis,
    t: &Telemetry,
    c: &Config,
    frames: &BTreeMap<String, Vec<Candle>>,
    now: u64,
) -> Analysis {
    if c.trading.mode == TradingMode::Expert {
        return a;
    }
    a.clear_plan();
    a.action = "WAIT".into();
    let (fast, confirm, higher) = if c.trading.mode == TradingMode::Swing {
        let (Some(h1), Some(h4), Some(d1)) = (frames.get("H1"), frames.get("H4"), frames.get("D1"))
        else {
            a.current = false;
            a.reason = "Swing needs fresh completed H1/H4/D1 data".into();
            return a;
        };
        (h1, h4, Some(d1))
    } else {
        let Some(m) = t.market.as_ref() else {
            return a;
        };
        if !a.current && (!t.connected || t.symbol != c.symbol || !short_mode_frames_current(m, now)) {
            // When M1/M5 are unsafe or missing, retain the original research
            // WAIT. A missing M15 alone must not veto a valid 1m/5m trade.
            return a;
        }
        (&m.m1, &m.m5, None)
    };
    if fast.len() < 100 || confirm.len() < 100 || higher.is_some_and(|v| v.len() < 100) {
        a.current = false;
        a.reason = "100 completed bars per required timeframe needed".into();
        return a;
    }
    a.current = true;
    a.evaluated_ms = now;
    a.bar_time = fast.last().unwrap().time;
    let atr = volatility(fast);
    a.atr = Some(atr);
    let side = direction(fast);
    let n = fast.len();
    let aligned =
        side != 0 && direction(confirm) == side && higher.is_none_or(|v| direction(v) == side);
    let trigger = if c.trading.mode == TradingMode::Sniper {
        let p = &fast[n - 6..n];
        let edge = if side == 1 {
            p.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max)
        } else {
            p.iter().map(|b| b.low).fold(f64::INFINITY, f64::min)
        };
        let live = if side==1 {t.bid}else{t.ask};
        live.is_some_and(|price|(price-edge)*side as f64>atr*0.05)
    } else {
        (fast[n - 1].close - fast[n - 4].close) * side as f64 > atr * 0.3
    };
    a.reason = format!(
        "{:?}: confirmed trend + {}",
        c.trading.mode,
        if c.trading.mode == TradingMode::Sniper {
            "live-quote breakout of six completed bars"
        } else {
            "three-bar momentum"
        }
    );
    if aligned && trigger && atr > 0. {
        a.action = if side == 1 { "BUY" } else { "SELL" }.into();
    } else {
        a.reason.push_str("; waiting for aligned trigger");
    }
    if c.trading.mode == TradingMode::Sniper
        && (t.tick_age_ms.is_none_or(|v| v > 1000)
            || t.quote_time_ms
                .is_none_or(|v| v > now + 2000 || now.saturating_sub(v) > 1000))
    {
        a.gates
            .push("Sniper needs a broker tick timestamp and quote ≤ 1 second".into());
    }
    a
}
#[cfg(test)]
mod tests {
    #[test]
    fn priority_router_maintains_one_execution_owner_and_preserves_legacy() {
        let legacy: TradingConfig=serde_json::from_str(r#"{"mode":"expert"}"#).unwrap();
        assert!(!legacy.priority_enabled);
        assert_eq!(legacy.priority_mode(),TradingMode::Expert);
        let mut cfg=TradingConfig::default();
        cfg.priority_enabled=true;
        cfg.fallback_mode=TradingMode::Swing;
        cfg.select_priority_owner();
        assert_eq!(cfg.mode,TradingMode::Sniper);
        cfg.validate().unwrap();
        cfg.sniper_active=false;
        assert!(cfg.validate().is_err(),"stale Sniper owner must not be accepted");
        cfg.select_priority_owner();
        assert_eq!(cfg.mode,TradingMode::Swing);
        cfg.validate().unwrap();
        for mode in [TradingMode::Expert,TradingMode::Scalping,TradingMode::Swing] {
            cfg.fallback_mode=mode; cfg.select_priority_owner();
            assert_eq!(cfg.mode,mode); cfg.validate().unwrap();
        }
        cfg.fallback_mode=TradingMode::Sniper;
        assert!(cfg.validate().is_err(),"fallback cannot become the primary owner");
        cfg.fallback_mode=TradingMode::Expert;
        cfg.sniper_active=true;
        cfg.select_priority_owner();
        assert_eq!(cfg.mode,TradingMode::Sniper);
        let reloaded:TradingConfig=serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        reloaded.validate().unwrap();
        assert_eq!(reloaded.mode,TradingMode::Sniper);
    }

    use super::*;
    #[test]
    fn sniper_reacts_to_live_quote_without_a_new_bar() {
        let now=1_800_000_000_000;
        let bars:Vec<Candle>=(0..180).map(|i|Candle{time:now/1000-(180-i)*60,open:100.+i as f64*0.3,high:100.4+i as f64*0.3,low:99.6+i as f64*0.3,close:100.1+i as f64*0.3,tick_volume:100}).collect();
        let mut t=Telemetry{connected:true,bid:Some(154.),ask:Some(154.01),tick_age_ms:Some(0),quote_time_ms:Some(now),market:Some(crate::MarketData{captured_ms:now,m1:bars.clone(),m5:bars.clone(),m15:bars,error:String::new()}),..Default::default()};
        let mut c=Config::default();c.trading.mode=TradingMode::Sniper;
        let base=Analysis{current:true,..Default::default()};
        let wait=select(base.clone(),&t,&c,&BTreeMap::new(),now);assert_eq!(wait.action,"WAIT");
        t.bid=Some(154.2);t.ask=Some(154.21);
        let buy=select(base.clone(),&t,&c,&BTreeMap::new(),now);assert_eq!(buy.action,"BUY");assert_eq!(wait.bar_time,buy.bar_time);
        t.quote_time_ms=Some(now-1001);assert!(!select(base,&t,&c,&BTreeMap::new(),now).gates.is_empty());
    }
    fn complete_bars(now: u64, seconds: u64) -> Vec<Candle> {
        let last=(now/1000/seconds-1)*seconds;
        (0..120).map(|i|{
            let time=last-(119-i)*seconds;
            let close=100. + i as f64*0.2;
            Candle{time,open:close-0.05,high:close+0.2,low:close-0.2,close,tick_volume:100}
        }).collect()
    }
    #[test]
    fn sniper_and_scalping_need_only_fresh_completed_m1_m5_not_research_m15() {
        let now=1_800_000_000_000u64;
        let m=crate::MarketData{captured_ms:now,m1:complete_bars(now,60),m5:complete_bars(now,300),m15:Vec::new(),error:String::new()};
        let base=crate::research::analyze(Some(&m),"XAUUSD",now);
        assert!(!base.current,"Full research should still require M15");
        let t=Telemetry{connected:true,symbol:"XAUUSD".into(),bid:Some(124.),ask:Some(124.01),
            tick_age_ms:Some(0),quote_time_ms:Some(now),market:Some(m.clone()),..Default::default()};
        for mode in [TradingMode::Scalping,TradingMode::Sniper] {
            let mut c=Config::default();c.trading.mode=mode;
            let out=select(base.clone(),&t,&c,&BTreeMap::new(),now);
            assert!(out.current,"{mode:?} should not inherit unrelated M15 research veto");
            assert!(out.atr.is_some_and(|atr|atr>0.));
            assert!(out.reason.contains("confirmed trend"));
        }
        let mut expert=Config::default();expert.trading.mode=TradingMode::Expert;
        assert!(!select(base,&t,&expert,&BTreeMap::new(),now).current,
                "Expert research still requires its original M15 evidence");
    }
    #[test]
    fn short_mode_m1_m5_fallback_never_accepts_stale_incomplete_or_gapped_bars() {
        let now=1_800_000_000_000u64;
        let base=Analysis::default();
        let mut c=Config::default();c.trading.mode=TradingMode::Scalping;
        let mut t=Telemetry{connected:true,symbol:"XAUUSD".into(),tick_age_ms:Some(0),
            quote_time_ms:Some(now),market:Some(crate::MarketData{
                captured_ms:now,m1:complete_bars(now,60),m5:complete_bars(now,300),
                m15:Vec::new(),error:String::new()}),..Default::default()};
        let choose=|t:&Telemetry|select(base.clone(),t,&c,&BTreeMap::new(),now).current;
        assert!(choose(&t));
        t.connected=false;assert!(!choose(&t));
        t.connected=true;t.symbol="OTHER".into();assert!(!choose(&t));
        t.symbol="XAUUSD".into();
        t.market.as_mut().unwrap().captured_ms=now-16_000;assert!(!choose(&t));
        t.market.as_mut().unwrap().captured_ms=now;
        t.market.as_mut().unwrap().m5.last_mut().unwrap().time-=300;
        assert!(!choose(&t),"Latest M5 candle must not duplicate prior bar");
        t.market.as_mut().unwrap().m5=complete_bars(now,300);
        t.market.as_mut().unwrap().m1[110].time-=60;
        assert!(!choose(&t),"Recent M1 timestamp gap/duplicate must veto entry");
        t.market.as_mut().unwrap().m1=complete_bars(now,60);
        t.market.as_mut().unwrap().m1.last_mut().unwrap().time=now/1000;
        assert!(!choose(&t),"Unclosed M1 candle must never count as complete");
        t.market.as_mut().unwrap().m1=complete_bars(now,60);
        t.market.as_mut().unwrap().error="MT5 quote/history read failed".into();
        assert!(!choose(&t),"A reported feed error must not be bypassed");
    }

    #[test]
    fn adaptive_pip_controls_validate_and_legacy_serialization_remains_compatible() {
        let old=r#"{"mode":"sniper","sniper_brain":"learner","sniper_target_points":50,"pip_points":10}"#;
        let c:TradingConfig=serde_json::from_str(old).unwrap();
        assert_eq!(c.sniper_target_mode,SniperTargetMode::Fixed);
        assert_eq!(c.sniper_target_points/c.pip_points,5.);
        let mut a=c.clone();a.sniper_target_mode=SniperTargetMode::Adaptive;
        a.sniper_min_target_pips=3.;a.sniper_max_target_pips=11.;
        a.validate().unwrap();
        assert_eq!(serde_json::to_value(&a).unwrap()["sniper_target_mode"],"adaptive");
        a.sniper_max_target_pips=3.;assert!(a.validate().is_err());
        a.sniper_max_target_pips=f64::INFINITY;assert!(a.validate().is_err());
    }
    #[test]
    fn proportional_costs_without_double_spread() {
        let t = Telemetry {
            connected: true,
            bid: Some(100.),
            ask: Some(100.02),
            tick_age_ms: Some(0),
            point: Some(0.01),
            tick_size: Some(0.01),
            tick_value_loss: Some(1.),
            tick_value_profit: Some(1.),
            ..Default::default()
        };
        let p = costs(&t, &Config::default(), 0.1, 1000).unwrap();
        assert!((p.commission - 0.7).abs() < 1e-10);
        assert!((p.total - 1.3).abs() < 1e-9);
        assert!((p.break_even_points - 13.).abs() < 1e-8);
        assert!((p.break_even_from_entry_points - 11.).abs() < 1e-8);
        assert!((p.spread_money - 0.2).abs() < 1e-8);
        assert!((p.buy_break_even_bid - 100.13).abs() < 1e-8);
        let small = costs(&t, &Config::default(), 0.01, 1000).unwrap();
        assert!((small.commission - 0.07).abs() < 1e-10);
        assert!((small.break_even_points - p.break_even_points).abs() < 1e-8);
        assert!(costs(&t, &Config::default(), 0., 1000).is_err());
    }

    #[test]
    fn sniper_tp_pays_commission_and_slippage_but_never_doubles_spread() {
        let mut c = Config::default();
        c.risk.slippage_ticks = 0.;
        let mut t = Telemetry {
            connected: true,
            bid: Some(2000.),
            ask: Some(2000.80),
            point: Some(0.01),
            tick_size: Some(0.01),
            tick_value_loss: Some(1.),
            tick_value_profit: Some(1.),
            tick_age_ms: Some(0),
            ..Default::default()
        };
        let preview = costs(&t, &c, 0.1, 1000).unwrap();
        assert!((preview.commission - 0.7).abs() < 1e-10);
        assert!((sniper_target_from_entry(&preview, 0.50).unwrap() - 0.50).abs() < 1e-8);
        c.risk.commission_per_lot = 0.;
        let free = costs(&t, &c, 0.01, 1000).unwrap();
        assert_eq!(free.commission, 0.);
        assert!((free.spread_points - 80.).abs() < 1e-8);
        assert!((sniper_target_from_entry(&free, 0.50).unwrap() - 0.50).abs() < 1e-8);
        t.ask = Some(2000.02);
        let narrow = costs(&t, &c, 0.01, 1000).unwrap();
        assert_eq!(narrow.commission, 0.);
        assert!(narrow.spread_money < free.spread_money);
        assert!((sniper_target_from_entry(&narrow, 0.50).unwrap() - 0.50).abs() < 1e-8);
    }
    #[test]
    fn fixed_gold_pips_do_not_change_when_broker_digits_change() {
        let mut c=Config::default();
        c.trading.mode=TradingMode::Sniper;
        for point in [0.001,0.01,0.1] {
            let mut t=Telemetry {connected:true,bid:Some(2500.),ask:Some(2500.20),
                point:Some(point),tick_size:Some(point),
                tick_value_loss:Some(1.),tick_value_profit:Some(1.),tick_age_ms:Some(0),
                ..Default::default()};
            let preview=costs(&t,&c,0.1,1000).unwrap();
            assert!((preview.pip_price-0.1).abs()<1e-10);
            assert!((preview.spread_pips-2.).abs()<1e-8);
            for (pips,distance) in [(5.,0.5),(10.,1.)] {
                c.trading.sniper_target_points=pips*c.trading.pip_points;
                assert!((c.trading.geometry(1.,point).1-distance).abs()<1e-8);
                assert!((sniper_target_from_entry(&preview,distance).unwrap()-distance).abs()<1e-8);
            }
            t.ask=Some(2500.25);
            assert!((costs(&t,&c,0.1,1000).unwrap().spread_pips-2.5).abs()<1e-8);
        }
    }
    #[test]
    fn different_broker_profit_loss_tick_values_do_not_multiply_spread_in_break_even() {
        let mut c = Config::default();
        c.risk.slippage_ticks = 0.;
        let t = Telemetry {
            connected: true,
            bid: Some(2000.),
            ask: Some(2000.80),
            tick_age_ms: Some(0),
            point: Some(0.01),
            tick_size: Some(0.01),
            tick_value_loss: Some(3.),
            tick_value_profit: Some(1.),
            ..Default::default()
        };
        let v = costs(&t, &c, 0.1, 1000).unwrap();
        assert!((v.spread_points - 80.).abs() < 1e-8);
        assert!((v.spread_pips - 8.).abs() < 1e-8);
        assert!((v.commission - 0.7).abs() < 1e-8);
        assert!((v.spread_money - 24.).abs() < 1e-8);
        assert!((v.break_even_from_entry_points - 7.).abs() < 1e-8);
        assert!((v.break_even_points - 87.).abs() < 1e-8);
        assert!((v.buy_break_even_bid - 2000.87).abs() < 1e-8);
        assert!((v.sell_break_even_ask - 1999.93).abs() < 1e-8);
    }
    #[test]
    fn sniper_holding_time_is_configurable_and_validated() {
        let mut c = Config::default();
        c.trading.mode = TradingMode::Sniper;
        c.trading.sniper_max_hold_seconds = 7;
        assert_eq!(c.trading.hold_ms(), 7_000);
        c.trading.sniper_max_hold_seconds = 0;
        assert!(c.validate().is_err());
        c.trading.sniper_max_hold_seconds = 3601;
        assert!(c.validate().is_err());
    }
    #[test]
    fn swing_requires_actual_higher_frames() {
        let c = Config {
            trading: TradingConfig {
                mode: TradingMode::Swing,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            !select(
                Analysis::default(),
                &Telemetry::default(),
                &c,
                &BTreeMap::new(),
                0
            )
            .current
        );
    }
}
