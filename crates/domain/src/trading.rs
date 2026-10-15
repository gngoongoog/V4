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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TradingConfig {
    pub mode: TradingMode,
    pub sniper_target_points: f64,
    pub sniper_stop_points: f64,
    pub sniper_cooldown_seconds: u64,
    pub pip_points: f64,
    pub max_cost_fraction: f64,
}
impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            mode: TradingMode::Expert,
            sniper_target_points: 50.,
            sniper_stop_points: 80.,
            sniper_cooldown_seconds: 30,
            pip_points: 10.,
            max_cost_fraction: 0.45,
        }
    }
}
impl TradingConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, v, min, max) in [
            ("Target points", self.sniper_target_points, 1., 100_000.),
            ("Stop points", self.sniper_stop_points, 1., 100_000.),
            ("Points per pip", self.pip_points, 1., 1000.),
            ("Cost fraction", self.max_cost_fraction, 0.01, 0.9),
        ] {
            if !v.is_finite() || !(min..=max).contains(&v) {
                return Err(format!("{name} must be {min}–{max}"));
            }
        }
        if !(1..=3600).contains(&self.sniper_cooldown_seconds) {
            return Err("Cooldown must be 1–3600 seconds".into());
        }
        Ok(())
    }
    pub fn hold_ms(&self) -> u64 {
        match self.mode {
            TradingMode::Expert => 1_800_000,
            TradingMode::Scalping => 900_000,
            TradingMode::Swing => 259_200_000,
            TradingMode::Sniper => 120_000,
        }
    }
    pub fn geometry(&self, atr: f64, point: f64) -> (f64, f64) {
        match self.mode {
            TradingMode::Expert => (1.5 * atr, 3. * atr),
            TradingMode::Scalping => (1.2 * atr, 1.68 * atr),
            TradingMode::Swing => (2. * atr, 5. * atr),
            TradingMode::Sniper => (
                self.sniper_stop_points * point,
                self.sniper_target_points * point,
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
    pub spread_money: f64,
    pub commission: f64,
    pub slippage_money: f64,
    pub total: f64,
    pub break_even_points: f64,
    pub break_even_pips: f64,
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
    let value = loss.max(profit);
    let spread_points = (ask - bid) / point;
    let spread_money = (ask - bid) / tick * value * lots;
    let commission = c.risk.commission_per_lot * lots;
    let slippage_money = 2. * c.risk.slippage_ticks * value * lots;
    let total = spread_money + commission + slippage_money;
    let break_even_points = total / (profit * lots) * tick / point;
    let r = CostPreview {
        currency: t.currency.clone(),
        lots,
        spread_points,
        spread_pips: spread_points / c.trading.pip_points,
        spread_money,
        commission,
        slippage_money,
        total,
        break_even_points,
        break_even_pips: break_even_points / c.trading.pip_points,
        desired_net_target_money: c.trading.sniper_target_points * point / tick * profit * lots,
    };
    if [total, break_even_points, r.desired_net_target_money]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err("Cost calculation overflow".into());
    }
    Ok(r)
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
        if !a.current {
            return a;
        }
        let Some(m) = t.market.as_ref() else {
            return a;
        };
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
    let last = &fast[n - 1];
    let aligned =
        side != 0 && direction(confirm) == side && higher.is_none_or(|v| direction(v) == side);
    let trigger = if c.trading.mode == TradingMode::Sniper {
        let p = &fast[n - 7..n - 1];
        let edge = if side == 1 {
            p.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max)
        } else {
            p.iter().map(|b| b.low).fold(f64::INFINITY, f64::min)
        };
        (last.close - edge) * side as f64 > 0. && (last.close - last.open) * side as f64 > atr * 0.2
    } else {
        (last.close - fast[n - 4].close) * side as f64 > atr * 0.3
    };
    a.reason = format!(
        "{:?}: confirmed trend + {}",
        c.trading.mode,
        if c.trading.mode == TradingMode::Sniper {
            "six-bar breakout"
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
    use super::*;
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
        assert!(costs(&t, &Config::default(), 0., 1000).is_err());
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
