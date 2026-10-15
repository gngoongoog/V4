use serde::{Deserialize, Serialize};
pub mod backtest;
pub mod trading;
pub mod execution;
pub mod studio;
pub mod wyckoff;
#[path = "backtest/evaluation.rs"]
pub mod evaluation;
pub mod ces;
pub mod kte;
pub mod research;
pub mod opportunities;
pub use research::{Analysis, Candle, MarketData, PaperSummary, RiskConfig};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const FRESH_MS: u64 = 8_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    #[default]
    Stopped,
    Starting,
    Monitoring,
    Degraded,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub mt5_path: String,
    pub symbol: String,
    pub telegram_user_id: String,
    pub telegram_chat_id: String,
    pub telegram_enabled: bool,
    #[serde(default)]
    pub risk: RiskConfig,
    #[serde(default)]
    pub trading: trading::TradingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            mt5_path: String::new(),
            symbol: "XAUUSD".into(),
            telegram_user_id: String::new(),
            telegram_chat_id: String::new(),
            telegram_enabled: false,
            risk: RiskConfig::default(),
            trading: trading::TradingConfig::default(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        self.risk.validate()?;
        self.trading.validate()?;
        if self.schema != 1 {
            return Err("Unsupported config schema".into());
        }
        if self.symbol.trim().is_empty() || self.symbol.len() > 80 {
            return Err("Enter a valid broker symbol".into());
        }
        if self.telegram_enabled {
            if self
                .telegram_user_id
                .parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .is_none()
                || self
                    .telegram_chat_id
                    .parse::<u64>()
                    .ok()
                    .filter(|v| *v > 0)
                    .is_none()
            {
                return Err("Telegram requires positive numeric user and private chat IDs".into());
            }
            if self.telegram_user_id != self.telegram_chat_id {
                return Err("This version supports your private Telegram chat only".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Position {
    #[serde(default)] pub magic: u64,
    #[serde(default)] pub identifier: u64,
    #[serde(default)] pub opened_ms: u64,
    pub ticket: u64,
    pub symbol: String,
    pub side: String,
    pub volume: f64,
    pub price_open: f64,
    pub price_current: f64,
    pub profit: f64,
    pub sl: f64,
    pub tp: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomLevel {
    pub side: String,
    pub price: f64,
    pub volume: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Telemetry {
    pub connected: bool,
    pub error: String,
    pub login: u64,
    pub server: String,
    pub currency: String,
    pub demo: bool,
    #[serde(default)] pub account_trade_mode: Option<i32>,
    #[serde(default)] pub margin_mode: Option<i32>,
    #[serde(default)] pub trade_allowed: bool,
    #[serde(default)] pub trade_expert: bool,
    pub balance: f64,
    pub equity: f64,
    pub positions: Vec<Position>,
    pub pending_orders: u32,
    pub today_net: Option<f64>,
    pub today_deals: Option<u32>,
    pub report_day: String,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub tick_age_ms: Option<u64>,
    #[serde(default)]
    pub quote_time_ms: Option<u64>,
    pub symbol: String,
    pub volume_min: Option<f64>,
    pub volume_step: Option<f64>,
    pub tick_size: Option<f64>,
    pub dom_available: bool,
    #[serde(default)]
    pub dom: Vec<DomLevel>,
    #[serde(default)]
    pub volume_max: Option<f64>,
    #[serde(default)]
    pub tick_value_loss: Option<f64>,
    #[serde(default)]
    pub tick_value_profit: Option<f64>,
    #[serde(default)]
    pub market: Option<MarketData>,
    #[serde(default)]
    pub point: Option<f64>,
}
impl Telemetry {
    pub fn validate(&self) -> bool {
        [self.balance, self.equity].iter().all(|v| v.is_finite())
            && [
                self.bid,
                self.ask,
                self.today_net,
                self.volume_min,
                self.volume_step,
                self.volume_max,
                self.tick_size,
                self.tick_value_loss,
                self.tick_value_profit,
                self.point,
            ]
            .iter()
            .flatten()
            .all(|v| v.is_finite())
            && self.market.as_ref().is_none_or(MarketData::valid)
            && self.positions.len() <= 10_000
            && self.positions.iter().all(|p| {
                [
                    p.volume,
                    p.price_open,
                    p.price_current,
                    p.profit,
                    p.sl,
                    p.tp,
                ]
                .iter()
                .all(|v| v.is_finite())
            })
            && self.dom.len() <= 200
            && self.dom.iter().all(|level| {
                matches!(level.side.as_str(), "buy" | "sell")
                    && level.price.is_finite()
                    && level.price > 0.0
                    && level.volume.is_finite()
                    && level.volume > 0.0
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub state: RunState,
    pub requested_run: bool,
    pub execution_enabled: bool,
    pub gateway_fresh: bool,
    pub last_received_ms: Option<u64>,
    pub telemetry: Option<Telemetry>,
    pub message: String,
    pub telegram_status: String,
    #[serde(default)]
    pub analysis: Analysis,
    #[serde(default)]
    pub paper: PaperSummary,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            version: VERSION.into(),
            state: RunState::Stopped,
            requested_run: false,
            execution_enabled: false,
            gateway_fresh: false,
            last_received_ms: None,
            telemetry: None,
            message: "Choose a brain and bind the Demo account on the Execution page".into(),
            telegram_status: "Not configured".into(),
            analysis: Analysis::default(),
            paper: PaperSummary::default(),
        }
    }
}
impl Status {
    pub fn refresh(&mut self, now: u64) {
        if !self.requested_run {self.execution_enabled = false;}
        if !self.requested_run { self.analysis.action="WAIT".into(); self.analysis.clear_plan(); }
        if let Some(opportunities) = self.analysis.opportunities.as_mut() {
            opportunities.expire(now, true);
            if !self.requested_run {
                opportunities.pause("النظام متوقف؛ أرقام الدخول مسحوبة حتى التشغيل وتحديث السعر");
            }
        }
        self.gateway_fresh = self
            .last_received_ms
            .map(|t| now.saturating_sub(t) <= FRESH_MS)
            .unwrap_or(false)
            && self
                .telemetry
                .as_ref()
                .map(|t| t.connected)
                .unwrap_or(false);
        self.state = if !self.requested_run {
            RunState::Stopped
        } else if self.gateway_fresh {
            RunState::Monitoring
        } else if self.last_received_ms.is_none() {
            RunState::Starting
        } else {
            RunState::Degraded
        };
        if !self.gateway_fresh {
            self.execution_enabled=false;
            self.analysis.clear_plan();
            self.analysis.current = false;
            self.analysis.action = "WAIT".into();
            self.analysis.reason = "MT5 data unavailable or stale".into();
            if let Some(opportunities) = self.analysis.opportunities.as_mut() {
                opportunities.withdraw("بيانات MT5 الحالية غير متاحة أو قديمة");
            }
            self.paper.quote_current = false;
        }
        if let Some(t) = &self.telemetry {
            let elapsed = now.saturating_sub(self.last_received_ms.unwrap_or(0));
            if t.tick_age_ms
                .is_none_or(|age| age.saturating_add(elapsed) > 3000)
            {
                self.paper.quote_current = false;
                self.analysis.clear_plan();
                self.analysis.action = "WAIT".into();
                let gate = "Quote unavailable or older than 3 seconds".to_string();
                if !self.analysis.gates.contains(&gate) {
                    self.analysis.gates.push(gate);
                }
                if let Some(opportunities) = self.analysis.opportunities.as_mut() {
                    opportunities.expire(now, false);
                }
            }
            if t.market
                .as_ref()
                .is_none_or(|m| now.saturating_sub(m.captured_ms) > 15000)
            {
                self.analysis.clear_plan();
            self.analysis.current = false;
                self.analysis.action = "WAIT".into();
                self.analysis.reason =
                    "Candle snapshot unavailable or older than 15 seconds".into();
                if let Some(opportunities) = self.analysis.opportunities.as_mut() {
                    opportunities.withdraw("بيانات الشموع غير متاحة أو أقدم من 15 ثانية");
                }
            }
        }
    }
    pub fn report(&self, command: &str) -> String {
        let heading = format!(
            "BRAINTRADE OS {}\nState: {:?}\nMT5 fresh: {}\nBroker execution enabled: {}",
            self.version, self.state, self.gateway_fresh, self.execution_enabled
        );
        if command == "/help" || command == "/start" {
            return "/status — System status\n/report — Today (Baghdad)\n/positions — MT5 positions\n/health — Connection health\n/brains — Analysis and decision\n/opportunities — Market setups with Entry/SL/TP when valid\n/paper — Virtual trades only\n/news — Official cached headlines\n/backtest — Latest saved historical test\n/help — Commands".into();
        }
        if command == "/brains" {
            return self.analysis.text();
        }
        if command == "/opportunities" {
            return self
                .analysis
                .opportunities
                .as_ref()
                .map(|value| value.signals_text())
                .unwrap_or_else(|| "لا توجد خريطة فرص حالية؛ انتظر بيانات شموع مكتملة من MT5.".into());
        }
        if command == "/paper" {
            return self.paper.text();
        }
        if !self.gateway_fresh {
            return format!("{heading}\nCurrent account data unavailable. Last snapshot is stale or disconnected.");
        }
        let Some(t) = &self.telemetry else {
            return heading;
        };
        match command {
            "/report" => format!("{heading}\nAccount: {} / {}\nDay: {} (UTC+03:00)\nBalance: {:.2} {}\nEquity: {:.2} {}\nAll-account booked deal PnL incl. commission, swap, fees: {}\nDeal records (not closed trades): {}\nOpen positions: {}\nPending orders: {}", t.login, t.server, t.report_day, t.balance, t.currency, t.equity, t.currency,
                t.today_net.map(|v| format!("{v:.2} {}",t.currency)).unwrap_or("Unavailable".into()),
                t.today_deals.map(|v| v.to_string()).unwrap_or("Unavailable".into()), t.positions.len(), t.pending_orders),
            "/positions" => {
                let rows: Vec<_> = t.positions.iter().take(25).map(|p| format!("#{} {} {} {:.2} lots | PnL {:.2} {} | SL {} TP {}", p.ticket,p.symbol,p.side,p.volume,p.profit,t.currency,p.sl,p.tp)).collect();
                format!("{heading}\nOpen positions: {}\n{}{}",t.positions.len(),rows.join("\n"),if t.positions.len()>25 {"\nFirst 25 shown; use desktop for all."} else {""})
            },
            "/status" | "/health" => format!("{heading}\nAccount: {} / {}\nTelegram: {}\n{}",t.login,t.server,self.telegram_status,self.message),
            _ => "Unknown command. Use /help".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_fails_closed_and_never_enables_execution() {
        let mut s = Status {
            requested_run: true,
            ..Default::default()
        };
        s.refresh(100);
        assert_eq!(s.state, RunState::Starting);
        s.telemetry = Some(Telemetry {
            connected: true,
            ..Default::default()
        });
        s.last_received_ms = Some(100);
        s.refresh(200);
        assert_eq!(s.state, RunState::Monitoring);
        assert!(!s.execution_enabled);
        s.refresh(9000);
        assert_eq!(s.state, RunState::Degraded);
        assert!(!s.gateway_fresh);
        s.requested_run = false;
        s.refresh(9001);
        assert_eq!(s.state, RunState::Stopped);
    }
    #[test]
    fn stale_report_does_not_present_old_balance_as_current() {
        let s = Status {
            telemetry: Some(Telemetry {
                balance: 98765.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!s.report("/report").contains("98765"));
    }
    #[test]
    fn telegram_private_identity_required() {
        let mut c = Config {
            telegram_enabled: true,
            ..Default::default()
        };
        assert!(c.validate().is_err());
        c.telegram_user_id = "123".into();
        c.telegram_chat_id = "123".into();
        assert!(c.validate().is_ok());
        c.telegram_chat_id = "456".into();
        assert!(c.validate().is_err());
    }
}
