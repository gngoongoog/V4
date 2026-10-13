//! Deterministic research agents. Scores are directional evidence, never win probabilities.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RiskConfig {
    pub risk_percent: f64,
    pub daily_loss_percent: f64,
    pub max_drawdown_percent: f64,
    pub max_spread_atr: f64,
    pub max_lots: f64,
    pub commission_per_lot: f64,
    pub slippage_ticks: f64,
}
impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            risk_percent: 0.25,
            daily_loss_percent: 2.0,
            max_drawdown_percent: 5.0,
            max_spread_atr: 0.25,
            max_lots: 0.10,
            commission_per_lot: 7.0,
            slippage_ticks: 2.0,
        }
    }
}
impl RiskConfig {
    pub fn validate(&self) -> Result<(), String> {
        let ranges = [
            (self.risk_percent, 0.01, 2.0),
            (self.daily_loss_percent, 0.1, 10.0),
            (self.max_drawdown_percent, 0.1, 30.0),
            (self.max_spread_atr, 0.01, 1.0),
            (self.max_lots, 0.001, 100.0),
            (self.commission_per_lot, 0.0, 1000.0),
            (self.slippage_ticks, 0.0, 1000.0),
        ];
        if ranges
            .iter()
            .any(|(v, lo, hi)| !v.is_finite() || v < lo || v > hi)
        {
            return Err("Invalid risk settings: check the ranges shown in Settings".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candle {
    pub time: u64, // UTC opening time in seconds
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub tick_volume: u64,
}
impl Candle {
    pub(crate) fn valid(&self) -> bool {
        self.time > 0
            && [self.open, self.high, self.low, self.close]
                .iter()
                .all(|x| x.is_finite() && *x > 0.0)
            && self.low <= self.open.min(self.close)
            && self.high >= self.open.max(self.close)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MarketData {
    pub captured_ms: u64,
    pub m1: Vec<Candle>,
    pub m5: Vec<Candle>,
    pub m15: Vec<Candle>,
    pub error: String,
}
impl MarketData {
    pub fn valid(&self) -> bool {
        [&self.m1, &self.m5, &self.m15].iter().all(|series| {
            series.len() <= 300
                && series.iter().all(Candle::valid)
                && series.windows(2).all(|w| w[0].time < w[1].time)
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vote {
    pub name: String,
    pub direction: i8,
    pub evidence: f64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub current: bool,
    pub evaluated_ms: u64,
    pub bar_time: u64,
    pub symbol: String,
    pub regime: String,
    pub action: String,
    pub reason: String,
    pub atr: Option<f64>,
    pub votes: Vec<Vote>,
    pub gates: Vec<String>,
    pub entry: Option<f64>,
    pub stop: Option<f64>,
    pub target: Option<f64>,
    pub lots: Option<f64>,
    pub estimated_risk: Option<f64>,
}
impl Default for Analysis {
    fn default() -> Self {
        Self {
            current: false,
            evaluated_ms: 0,
            bar_time: 0,
            symbol: String::new(),
            regime: "UNKNOWN".into(),
            action: "WAIT".into(),
            reason: "Waiting for completed M1/M5/M15 candles".into(),
            atr: None,
            votes: vec![],
            gates: vec![],
            entry: None,
            stop: None,
            target: None,
            lots: None,
            estimated_risk: None,
        }
    }
}
impl Analysis {
    pub fn text(&self) -> String {
        let votes = self
            .votes
            .iter()
            .map(|v| {
                format!(
                    "{}: {} | evidence {:.2}\n{}",
                    v.name,
                    direction(v.direction),
                    v.evidence,
                    v.reason
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let proposal = self.entry.map(|entry| format!("\nVirtual proposal: entry {:.5} | SL {:.5} | TP {:.5}\nLots {:.4} | estimated stop loss + costs {:.2} (account currency)",
            entry, self.stop.unwrap_or_default(), self.target.unwrap_or_default(), self.lots.unwrap_or_default(), self.estimated_risk.unwrap_or_default())).unwrap_or_default();
        format!("{} | {} | {}\nCurrent: {} | M1 bar UTC epoch: {}\n{}\n\n{}\n\nRisk gates: {}{}\n\nعقول بقواعد حسابية؛ قوة الدليل ليست نسبة نجاح. لا تُرسل صفقات إلى MT5.",
            self.symbol, self.regime, self.action, self.current, self.bar_time, reason_ar(&self.reason), votes,
            if self.gates.is_empty() { "No veto".into() } else { self.gates.iter().map(|g| reason_ar(g)).collect::<Vec<_>>().join(" | ") }, proposal)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PaperSummary {
    pub context: String,
    pub quote_current: bool,
    pub equity: Option<f64>,
    pub realized: f64,
    pub unrealized: Option<f64>,
    pub trades: u64,
    pub wins: u64,
    pub interrupted_trades: u64,
    pub open: String,
    pub recent: Vec<String>,
    pub note: String,
}
impl PaperSummary {
    pub fn text(&self) -> String {
        let money = |v: Option<f64>| v.map(|n| format!("{n:.2}")).unwrap_or("Unavailable".into());
        format!("SHADOW / محاكاة فقط\n{}\nCurrent quote: {}\nVirtual equity: {} | realized: {:.2} | open PnL: {}\nClosed virtual trades: {} | profitable: {} | interrupted: {}\n\n{}\n\n{}\n\n{}\n\nQuote-sampled simulation with spread, configured fees and slippage. No real fills or guaranteed execution; overnight swaps excluded.",
            self.context, self.quote_current, money(if self.quote_current { self.equity } else { None }), self.realized,
            money(if self.quote_current { self.unrealized } else { None }), self.trades, self.wins,
            self.interrupted_trades, self.open, self.recent.join("\n\n"), self.note)
    }
}

pub fn direction(value: i8) -> &'static str {
    match value {
        1 => "BUY / صعود",
        -1 => "SELL / هبوط",
        _ => "NEUTRAL / محايد",
    }
}
fn sign(value: f64, threshold: f64) -> i8 {
    if value > threshold {
        1
    } else if value < -threshold {
        -1
    } else {
        0
    }
}
fn ema(series: &[Candle], period: usize) -> f64 {
    let alpha = 2.0 / (period as f64 + 1.0);
    series
        .iter()
        .skip(1)
        .fold(series[0].close, |e, c| alpha * c.close + (1.0 - alpha) * e)
}
fn atr(series: &[Candle]) -> f64 {
    let ranges: Vec<f64> = series
        .windows(2)
        .map(|w| {
            (w[1].high - w[1].low)
                .max((w[1].high - w[0].close).abs())
                .max((w[1].low - w[0].close).abs())
        })
        .collect();
    ranges[ranges.len() - 14..].iter().sum::<f64>() / 14.0
}
fn trend(series: &[Candle]) -> (i8, f64) {
    let volatility = atr(series);
    if volatility <= 0.0 {
        return (0, 0.0);
    }
    let separation = (ema(series, 20) - ema(series, 50)) / volatility;
    let slope = (ema(series, 20) - ema(&series[..series.len() - 3], 20)) / volatility;
    let dir = if sign(separation, 0.15) == sign(slope, 0.03) {
        sign(separation, 0.15)
    } else {
        0
    };
    (dir, separation.abs().min(1.0))
}

/// Uses only complete bars available at this timestamp; no future indexing.
pub fn analyze(market: Option<&MarketData>, symbol: &str, now: u64) -> Analysis {
    let mut result = Analysis {
        symbol: symbol.into(),
        evaluated_ms: now,
        ..Default::default()
    };
    let Some(m) = market else {
        return result;
    };
    if !m.valid() || now < m.captured_ms || now - m.captured_ms > 15_000 {
        result.reason = "Invalid or stale candle snapshot".into();
        return result;
    }
    if !m.error.is_empty() {
        result.reason = m.error.clone();
        return result;
    }
    for (series, seconds) in [(&m.m1, 60), (&m.m5, 300), (&m.m15, 900)] {
        if series.len() < 100 {
            result.reason = "Need at least 100 completed bars on M1, M5 and M15".into();
            return result;
        }
        let last = series.last().unwrap();
        let close_ms = last.time.saturating_add(seconds).saturating_mul(1000);
        if close_ms > now || now - close_ms > seconds * 1000 + 20_000 {
            result.reason = "Incomplete or old candles (market may be closed)".into();
            return result;
        }
        // Recent holes make consecutive-bar features unreliable; historical sessions are allowed outside this window.
        if series[series.len() - 30..]
            .windows(2)
            .any(|w| w[1].time - w[0].time != seconds)
        {
            result.reason = "Recent candle gaps; waiting for 30 consecutive completed bars".into();
            return result;
        }
    }
    let volatility = atr(&m.m1);
    if !volatility.is_finite() || volatility <= 0.0 {
        result.reason = "ATR unavailable or zero".into();
        return result;
    }
    result.current = true;
    result.bar_time = m.m1.last().unwrap().time;
    result.atr = Some(volatility);
    let trends: Vec<_> = [&m.m1, &m.m5, &m.m15].iter().map(|s| trend(s)).collect();
    for (index, name) in ["Trend M1", "Trend M5", "Trend M15"].iter().enumerate() {
        result.votes.push(Vote { name: name.to_string(), direction: trends[index].0, evidence: trends[index].1,
            reason: "EMA20/EMA50 separation > 0.15 ATR and EMA20 slope over 3 bars > 0.03 ATR must agree".into() });
    }
    let n = m.m1.len();
    let momentum = (m.m1[n - 1].close - m.m1[n - 6].close) / volatility;
    let mom_dir = sign(momentum, 0.5);
    result.votes.push(Vote {
        name: "Momentum M1".into(),
        direction: mom_dir,
        evidence: (momentum.abs() / 2.0).min(1.0),
        reason: format!("5-bar change / ATR = {momentum:.3}; threshold 0.50"),
    });
    let high = m.m1[n - 21..n - 1]
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = m.m1[n - 21..n - 1]
        .iter()
        .map(|c| c.low)
        .fold(f64::INFINITY, f64::min);
    let close = m.m1[n - 1].close;
    let breakout = if close > high + volatility * 0.05 {
        1
    } else if close < low - volatility * 0.05 {
        -1
    } else {
        0
    };
    result.votes.push(Vote {
        name: "Structure M1".into(),
        direction: breakout,
        evidence: if breakout == 0 { 0.0 } else { 1.0 },
        reason: format!(
            "Close {close:.5} versus prior 20-bar range {low:.5}..{high:.5}, buffer 0.05 ATR"
        ),
    });
    // Regime/Meta veto: all timeframes agree and at least one independent trigger supports the direction.
    let dir = trends[0].0;
    let aligned = dir != 0 && trends.iter().all(|t| t.0 == dir);
    result.regime = if aligned {
        if dir == 1 {
            "UPTREND"
        } else {
            "DOWNTREND"
        }
    } else {
        "MIXED / RANGE"
    }
    .into();
    if aligned && (mom_dir == dir || breakout == dir) && mom_dir != -dir && breakout != -dir {
        result.action = if dir == 1 { "BUY" } else { "SELL" }.into();
        result.reason =
            "Meta: all three trends agree, momentum or breakout confirms, no opposing trigger"
                .into();
    } else {
        result.reason = "Meta: wait for M1/M5/M15 agreement and a confirming trigger".into();
    }
    result
}

/// Conservative lot floor. Minimum-lot risk is never rounded upward into the budget.
pub fn size_lots(budget: f64, loss_per_lot: f64, min: f64, step: f64, cap: f64) -> Option<f64> {
    if [budget, loss_per_lot, min, step, cap]
        .iter()
        .any(|x| !x.is_finite() || *x <= 0.0)
    {
        return None;
    }
    let lots = ((budget / loss_per_lot).min(cap) / step).floor() * step;
    if lots + 1e-12 < min || lots * loss_per_lot > budget + 1e-8 || lots <= 0.0 {
        None
    } else {
        Some(lots)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    pub fn fixture(now: u64, down: bool) -> MarketData {
        let build = |seconds: u64| {
            (0..180)
                .map(|i| {
                    let direction = if down { -1.0 } else { 1.0 };
                    let price = 2000.0 + direction * i as f64 * 0.30;
                    Candle {
                        time: ((now / 1000) / seconds) * seconds - (180 - i) * seconds,
                        open: price,
                        high: price + 0.6,
                        low: price - 0.6,
                        close: price + direction * 0.25,
                        tick_volume: 100,
                    }
                })
                .collect()
        };
        MarketData {
            captured_ms: now,
            m1: build(60),
            m5: build(300),
            m15: build(900),
            error: String::new(),
        }
    }
    #[test]
    fn opposing_trends_and_missing_data_veto() {
        let now = 1_800_000_020_000;
        let m = fixture(now, false);
        assert_eq!(analyze(Some(&m), "X", now).action, "BUY");
        let mut mixed = m.clone();
        mixed.m15 = fixture(now, true).m15;
        assert_eq!(analyze(Some(&mixed), "X", now).action, "WAIT");
        mixed.m1.clear();
        assert!(!analyze(Some(&mixed), "X", now).current);
    }
    #[test]
    fn closed_bars_stale_and_gaps_are_enforced() {
        let now = 1_800_000_020_000;
        let mut m = fixture(now, false);
        assert!(!analyze(Some(&m), "X", now + 20_000).current);
        m.m1.last_mut().unwrap().time = now / 1000;
        assert!(!analyze(Some(&m), "X", now).current);
        m = fixture(now, false);
        m.m5.remove(160);
        assert!(!analyze(Some(&m), "X", now).current);
    }
    #[test]
    fn lot_floor_respects_risk_and_minimum() {
        assert_eq!(size_lots(25.0, 300.0, 0.01, 0.01, 1.0), Some(0.08));
        assert_eq!(size_lots(2.0, 300.0, 0.01, 0.01, 1.0), None);
        assert_eq!(size_lots(100.0, 300.0, 0.01, 0.01, 0.05), Some(0.05));
        assert_eq!(size_lots(f64::NAN, 300.0, 0.01, 0.01, 1.0), None);
    }
    #[test]
    fn old_config_gets_risk_defaults() {
        let old = r#"{"schema":1,"mt5_path":"","symbol":"XAUUSD","telegram_user_id":"","telegram_chat_id":"","telegram_enabled":false}"#;
        let cfg: crate::Config = serde_json::from_str(old).unwrap();
        assert_eq!(cfg.risk.risk_percent, 0.25);
        assert!(cfg.validate().is_ok());
    }
}

fn reason_ar(reason: &str) -> &str {
    match reason {
        "Meta: wait for M1/M5/M15 agreement and a confirming trigger" => {
            "العقل الرئيسي ينتظر اتفاق اتجاه الفريمات مع تأكيد الزخم أو الاختراق"
        }
        "Meta: all three trends agree, momentum or breakout confirms, no opposing trigger" => {
            "اتفاق الاتجاهات الثلاثة مع تأكيد الزخم أو الاختراق دون معارضة"
        }
        "Virtual position opened from current quote; no broker order sent" => {
            "فُتحت صفقة افتراضية؛ لم يُرسل أمر إلى الوسيط"
        }
        "System stopped — new virtual entries disabled" => "النظام متوقف؛ الدخول الافتراضي معطل",
        "One virtual position already open" => "توجد صفقة افتراضية مفتوحة",
        "MT5 already has open positions or pending orders" => "توجد صفقات أو أوامر معلقة في MT5",
        "This M1 bar has already been evaluated" => "تم تقييم شمعة الدقيقة هذه؛ بانتظار شمعة جديدة",
        "Spread exceeds configured ATR fraction" => "السبريد أعلى من الحد المحدد نسبة إلى ATR",
        "Virtual daily loss limit reached (Baghdad day)" => {
            "بلغت المحاكاة حد الخسارة اليومية بتوقيت بغداد"
        }
        "Virtual peak drawdown limit reached" => "بلغت المحاكاة حد التراجع من القمة",
        "Quote unavailable or older than 3 seconds" => "السعر غير متاح أو أقدم من 3 ثوانٍ",
        "Minimum broker lot exceeds risk budget or lot cap" => {
            "أقل لوت عند الوسيط يتجاوز ميزانية المخاطرة أو سقف اللوت"
        }
        "Broker tick values or volume limits unavailable" => {
            "قيمة الحركة السعرية أو حدود اللوت غير متاحة من الوسيط"
        }
        "Candle snapshot unavailable or older than 15 seconds"
        | "Invalid or stale candle snapshot" => "بيانات الشموع غير متاحة أو قديمة",
        "Need at least 100 completed bars on M1, M5 and M15" => {
            "نحتاج 100 شمعة مكتملة على الأقل لكل فريم"
        }
        "Incomplete or old candles (market may be closed)" => {
            "الشموع قديمة أو غير مكتملة؛ قد يكون السوق مغلقاً"
        }
        "Recent candle gaps; waiting for 30 consecutive completed bars" => {
            "توجد فجوات؛ بانتظار 30 شمعة مكتملة ومتتالية لكل فريم"
        }
        _ => reason,
    }
}
