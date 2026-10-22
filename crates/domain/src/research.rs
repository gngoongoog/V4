//! Deterministic research agents. Scores are directional evidence, never win probabilities.
use serde::{Deserialize, Serialize};

mod regime;
pub use regime::{MetaBrainV2, RegimeBrain, TrendBrain, TrendFrame};

fn micro_demo_opt_in_is_false(value: &bool) -> bool { !*value }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RiskConfig {
    pub risk_percent: f64,
    /// Explicit opt-in permits up to 5% per trade in verified MT5 Demo only.
    /// Omit false from serialized legacy backtest reports to preserve golden hashes.
    /// The broker adapter rejects this elevated budget on REAL accounts.
    #[serde(default, skip_serializing_if = "micro_demo_opt_in_is_false")]
    pub micro_demo_risk_opt_in: bool,
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
            micro_demo_risk_opt_in: false,
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
            (self.risk_percent, 0.01, if self.micro_demo_risk_opt_in { 5.0 } else { 2.0 }),
            (self.daily_loss_percent, 0.1, 10.0),
            (self.max_drawdown_percent, 0.1, 30.0),
            (self.max_spread_atr, 0.01, 1.0),
            (self.max_lots, 0.001, 100.0),
            (self.commission_per_lot, 0.0, 1000.0),
            (self.slippage_ticks, 0.0, 1000.0),
        ];
        if ranges
            .iter()
            .any(|(value, low, high)| !value.is_finite() || value < low || value > high)
        {
            return Err("Invalid risk settings: check the ranges shown in Settings".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
                .all(|value| value.is_finite() && *value > 0.0)
            && self.low <= self.open.min(self.close)
            && self.high >= self.open.max(self.close)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
                && series.windows(2).all(|window| window[0].time < window[1].time)
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opportunities: Option<crate::opportunities::MarketBrains>,
    #[serde(default)]
    pub trend_brain: TrendBrain,
    #[serde(default)]
    pub regime_brain: RegimeBrain,
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
            opportunities: None,
            trend_brain: TrendBrain::default(),
            regime_brain: RegimeBrain::default(),
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
    pub fn clear_plan(&mut self) {
        self.entry=None; self.stop=None; self.target=None; self.lots=None; self.estimated_risk=None;
    }
    pub fn text(&self) -> String {
        if !self.current {
            return format!("{} | WAIT\nالتحليل الحالي غير متاح: {}\nالمستويات والتوقعات القديمة مسحوبة حتى وصول بيانات حديثة.", self.symbol, reason_ar(&self.reason));
        }
        let votes = self
            .votes
            .iter()
            .map(|vote| {
                format!(
                    "{}: {} | evidence {:.2}\n{}",
                    vote.name,
                    direction(vote.direction),
                    vote.evidence,
                    vote.reason
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let proposal = self
            .entry
            .map(|entry| {
                format!(
                    "\nVirtual proposal: entry {:.5} | SL {:.5} | TP {:.5}\nLots {:.4} | estimated stop loss + costs {:.2} (account currency)",
                    entry,
                    self.stop.unwrap_or_default(),
                    self.target.unwrap_or_default(),
                    self.lots.unwrap_or_default(),
                    self.estimated_risk.unwrap_or_default()
                )
            })
            .unwrap_or_default();
        format!(
            "{} | {} | {}\nCurrent: {} | M1 bar UTC epoch: {}\n{}\n\nTREND BRAIN\n{}\n\nREGIME BRAIN\n{}\n\nLEGACY META INPUTS\n{}\n\nRisk gates: {}{}\n\nعقول بقواعد حسابية؛ قوة الدليل ليست نسبة نجاح. لا تُرسل صفقات إلى MT5.",
            self.symbol,
            self.regime,
            self.action,
            self.current,
            self.bar_time,
            reason_ar(&self.reason),
            self.trend_brain.text(),
            self.regime_brain.text(),
            votes,
            if self.gates.is_empty() {
                "No veto".into()
            } else {
                self.gates
                    .iter()
                    .map(|gate| reason_ar(gate))
                    .collect::<Vec<_>>()
                    .join(" | ")
            },
            proposal
        )
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
        let money = |value: Option<f64>| {
            value
                .map(|number| format!("{number:.2}"))
                .unwrap_or("Unavailable".into())
        };
        format!(
            "SHADOW / محاكاة فقط\n{}\nCurrent quote: {}\nVirtual equity: {} | realized: {:.2} | open PnL: {}\nClosed virtual trades: {} | profitable: {} | interrupted: {}\n\n{}\n\n{}\n\n{}\n\nQuote-sampled simulation with spread, configured fees and slippage. No real fills or guaranteed execution; overnight swaps excluded.",
            self.context,
            self.quote_current,
            money(if self.quote_current { self.equity } else { None }),
            self.realized,
            money(if self.quote_current { self.unrealized } else { None }),
            self.trades,
            self.wins,
            self.interrupted_trades,
            self.open,
            self.recent.join("\n\n"),
            self.note
        )
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
        .fold(series[0].close, |value, candle| {
            alpha * candle.close + (1.0 - alpha) * value
        })
}
fn atr(series: &[Candle]) -> f64 {
    let ranges: Vec<f64> = series
        .windows(2)
        .map(|window| {
            (window[1].high - window[1].low)
                .max((window[1].high - window[0].close).abs())
                .max((window[1].low - window[0].close).abs())
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
    let direction = if sign(separation, 0.15) == sign(slope, 0.03) {
        sign(separation, 0.15)
    } else {
        0
    };
    (direction, separation.abs().min(1.0))
}

/// Uses only complete bars available at this timestamp; no future indexing.
/// Trend/Regime brains are observational in v1: the legacy Meta decision path is intentionally
/// unchanged until a separate regression/ablation checkpoint validates the integration.
pub fn analyze(market: Option<&MarketData>, symbol: &str, now: u64) -> Analysis {
    let mut result = Analysis {
        symbol: symbol.into(),
        evaluated_ms: now,
        ..Default::default()
    };
    let Some(market) = market else {
        return result;
    };
    if !market.valid() || now < market.captured_ms || now - market.captured_ms > 15_000 {
        result.reason = "Invalid or stale candle snapshot".into();
        return result;
    }
    if !market.error.is_empty() {
        result.reason = market.error.clone();
        return result;
    }
    for (series, seconds) in [(&market.m1, 60), (&market.m5, 300), (&market.m15, 900)] {
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
        if series[series.len() - 30..]
            .windows(2)
            .any(|window| window[1].time - window[0].time != seconds)
        {
            result.reason = "Recent candle gaps; waiting for 30 consecutive completed bars".into();
            return result;
        }
    }
    let volatility = atr(&market.m1);
    if !volatility.is_finite() || volatility <= 0.0 {
        result.reason = "ATR unavailable or zero".into();
        return result;
    }
    result.current = true;
    result.bar_time = market.m1.last().unwrap().time;
    result.atr = Some(volatility);

    let (trend_brain, regime_brain) = regime::assess(&market.m1, &market.m5, &market.m15);
    result.regime = regime_brain.label.clone();
    result.trend_brain = trend_brain;
    result.regime_brain = regime_brain;

    // Legacy Meta inputs remain unchanged in this checkpoint.
    let trends: Vec<_> = [&market.m1, &market.m5, &market.m15]
        .iter()
        .map(|series| trend(series))
        .collect();
    for (index, name) in ["Trend M1", "Trend M5", "Trend M15"].iter().enumerate() {
        result.votes.push(Vote {
            name: name.to_string(),
            direction: trends[index].0,
            evidence: trends[index].1,
            reason: "EMA20/EMA50 separation > 0.15 ATR and EMA20 slope over 3 bars > 0.03 ATR must agree".into(),
        });
    }
    let count = market.m1.len();
    let momentum = (market.m1[count - 1].close - market.m1[count - 6].close) / volatility;
    let momentum_direction = sign(momentum, 0.5);
    result.votes.push(Vote {
        name: "Momentum M1".into(),
        direction: momentum_direction,
        evidence: (momentum.abs() / 2.0).min(1.0),
        reason: format!("5-bar change / ATR = {momentum:.3}; threshold 0.50"),
    });
    let high = market.m1[count - 21..count - 1]
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = market.m1[count - 21..count - 1]
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let close = market.m1[count - 1].close;
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

    // Legacy Meta decision path. New Trend/Regime outputs do not change entries in this checkpoint.
    let direction = trends[0].0;
    let aligned = direction != 0 && trends.iter().all(|trend| trend.0 == direction);
    if aligned
        && (momentum_direction == direction || breakout == direction)
        && momentum_direction != -direction
        && breakout != -direction
    {
        result.action = if direction == 1 { "BUY" } else { "SELL" }.into();
        result.reason =
            "Meta: all three trends agree, momentum or breakout confirms, no opposing trigger"
                .into();
    } else {
        result.reason = "Meta: wait for M1/M5/M15 agreement and a confirming trigger".into();
    }
    result
}

/// Conservative lot floor. Minimum-lot risk is never rounded upward into the budget.
pub fn size_lots(
    budget: f64,
    loss_per_lot: f64,
    min: f64,
    step: f64,
    cap: f64,
) -> Option<f64> {
    if [budget, loss_per_lot, min, step, cap]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
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
                .map(|index| {
                    let direction = if down { -1.0 } else { 1.0 };
                    let price = 2000.0 + direction * index as f64 * 0.30;
                    Candle {
                        time: ((now / 1000) / seconds) * seconds - (180 - index) * seconds,
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
        let market = fixture(now, false);
        let analysis = analyze(Some(&market), "X", now);
        assert_eq!(analysis.action, "BUY");
        assert_eq!(analysis.trend_brain.direction, 1);
        assert!(analysis.regime_brain.label.starts_with("TREND_UP"));

        let mut mixed = market.clone();
        mixed.m15 = fixture(now, true).m15;
        let analysis = analyze(Some(&mixed), "X", now);
        assert_eq!(analysis.action, "WAIT");
        assert_eq!(analysis.trend_brain.direction, 0);
        assert!(analysis.regime_brain.label.starts_with("TRANSITION"));

        mixed.m1.clear();
        assert!(!analyze(Some(&mixed), "X", now).current);
    }

    #[test]
    fn closed_bars_stale_and_gaps_are_enforced() {
        let now = 1_800_000_020_000;
        let mut market = fixture(now, false);
        assert!(!analyze(Some(&market), "X", now + 20_000).current);
        market.m1.last_mut().unwrap().time = now / 1000;
        assert!(!analyze(Some(&market), "X", now).current);
        market = fixture(now, false);
        market.m5.remove(160);
        assert!(!analyze(Some(&market), "X", now).current);
    }

    #[test]
    fn independent_brains_are_observational_in_v1() {
        let now = 1_800_000_020_000;
        let up = fixture(now, false);
        let analysis = analyze(Some(&up), "X", now);
        assert_eq!(analysis.action, "BUY");
        assert_eq!(analysis.trend_brain.version, "trend-v1");
        assert_eq!(analysis.regime_brain.version, "regime-v1");
        assert!(analysis.trend_brain.aligned);
        assert_eq!(analysis.regime, analysis.regime_brain.label);
        assert!(analysis.text().contains("TREND BRAIN"));
        assert!(analysis.text().contains("REGIME BRAIN"));
    }

    #[test]
    fn old_analysis_json_gets_new_brain_defaults() {
        let old = r#"{"current":false,"evaluated_ms":0,"bar_time":0,"symbol":"X","regime":"UNKNOWN","action":"WAIT","reason":"old","atr":null,"votes":[],"gates":[],"entry":null,"stop":null,"target":null,"lots":null,"estimated_risk":null}"#;
        let analysis: Analysis = serde_json::from_str(old).unwrap();
        assert_eq!(analysis.trend_brain.version, "trend-v1");
        assert_eq!(analysis.regime_brain.version, "regime-v1");
    }

    #[test]
    fn lot_floor_respects_risk_and_minimum() {
        assert_eq!(size_lots(25.0, 300.0, 0.01, 0.01, 1.0), Some(0.08));
        assert_eq!(size_lots(2.0, 300.0, 0.01, 0.01, 1.0), None);
        assert_eq!(size_lots(100.0, 300.0, 0.01, 0.01, 0.05), Some(0.05));
        assert_eq!(size_lots(f64::NAN, 300.0, 0.01, 0.01, 1.0), None);
    }

    #[test]
    fn elevated_micro_demo_risk_requires_explicit_opt_in() {
        let mut risk = RiskConfig::default();
        risk.risk_percent = 4.0;
        assert!(risk.validate().is_err());
        risk.micro_demo_risk_opt_in = true;
        assert!(risk.validate().is_ok());
        risk.risk_percent = 5.01;
        assert!(risk.validate().is_err());
        risk.risk_percent = 5.0;
        assert!(risk.validate().is_ok());
        let decoded: RiskConfig = serde_json::from_str(&serde_json::to_string(&risk).unwrap()).unwrap();
        assert!(decoded.micro_demo_risk_opt_in);
        assert_eq!(decoded.risk_percent, 5.0);
    }
    #[test]
    fn old_config_gets_risk_defaults() {
        let old = r#"{"schema":1,"mt5_path":"","symbol":"XAUUSD","telegram_user_id":"","telegram_chat_id":"","telegram_enabled":false}"#;
        let config: crate::Config = serde_json::from_str(old).unwrap();
        assert_eq!(config.risk.risk_percent, 0.25);
        assert!(!config.risk.micro_demo_risk_opt_in);
        assert!(config.validate().is_ok());
        assert!(!serde_json::to_value(&config.risk).unwrap().as_object().unwrap()
            .contains_key("micro_demo_risk_opt_in"),
            "disabled opt-in must not alter historical report fingerprints");
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
