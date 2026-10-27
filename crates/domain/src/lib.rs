use serde::{Deserialize, Serialize};
pub mod backtest;
pub mod trading;
pub mod sniper;
pub mod commission;
pub mod execution;
pub mod student;
pub mod studio;
pub mod wyckoff;
#[path = "backtest/evaluation.rs"]
pub mod evaluation;
pub mod ces;
pub mod kte;
pub mod research;
pub mod opportunities;
pub mod correlation;
pub mod oos;
pub use research::{Analysis, Candle, MarketData, PaperSummary, RiskConfig};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_ID: &str = env!("BRAINTRADE_BUILD_ID");
pub fn build_identity() -> serde_json::Value {
    serde_json::json!({"version":VERSION,"build_id":BUILD_ID,
        "release":env!("BRAINTRADE_RELEASE_NAME"),"marker":env!("BRAINTRADE_RELEASE_MARKER"),
        "channel":env!("BRAINTRADE_RELEASE_CHANNEL"),
        "core_protocol":env!("BRAINTRADE_CORE_PROTOCOL").parse::<u32>().unwrap()})
}
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
    #[serde(default)]
    pub commission_profiles: Vec<commission::Profile>,
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
            commission_profiles: Vec::new(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        self.risk.validate()?;
        self.trading.validate()?;
        if self.commission_profiles.len() > 256 {
            return Err("Too many commission profiles".into());
        }
        for (i, profile) in self.commission_profiles.iter().enumerate() {
            profile.validate()?;
            if self.commission_profiles[..i]
                .iter()
                .any(|other| other.account == profile.account)
            {
                return Err("Duplicate commission account profile".into());
            }
        }
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
    #[serde(default)] pub leverage: Option<u32>,
    /// Broker-reported used margin only; never synthesize from account equity.
    #[serde(default)] pub margin: Option<f64>,
    #[serde(default)] pub margin_free: Option<f64>,
    #[serde(default)] pub minimum_lot_margin: Option<f64>,
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
    /// Real MT5 time_msc; quote_time_ms remains the logical event ordering key.
    #[serde(default)]
    pub source_quote_time_ms: Option<u64>,
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
    pub fn broker_quote_time_ms(&self) -> Option<u64> {
        let event = self.quote_time_ms.filter(|v| *v > 0)?;
        match self.source_quote_time_ms {
            Some(source) if source > 0 && source <= event => Some(source),
            Some(_) => None,
            None => Some(event), // Legacy snapshots lack verified source time.
        }
    }

    pub fn quote_is_fresh(&self, now: u64, maximum_age_ms: u64) -> bool {
        self.tick_age_ms.is_some_and(|v| v <= maximum_age_ms)
            && self.broker_quote_time_ms().is_some_and(|v|
                v <= now.saturating_add(2000) && now.saturating_sub(v) <= maximum_age_ms)
    }

    pub fn validate(&self) -> bool {
        [self.balance, self.equity].iter().all(|v| v.is_finite())
            && (self.source_quote_time_ms.is_none() || self.broker_quote_time_ms().is_some())
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
                self.margin,
                self.margin_free,
                self.minimum_lot_margin,
            ]
            .iter()
            .flatten()
            .all(|v| v.is_finite())
            && self.margin.is_none_or(|v| v >= 0.0)
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
    #[serde(default)]
    pub build: serde_json::Value,
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
            build: build_identity(),
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
        let yes=|v:bool|if v {"نعم"}else{"لا"};
        let number=|v:Option<f64>|v.map(|v|format!("{v:.2}")).unwrap_or_else(||"غير متاح".into());
        let side=|v:&str|match v {"BUY"=>"شراء","SELL"=>"بيع",_=>"انتظار"};
        let heading = format!("نظام التداول {}\nالحالة: {}\nبيانات MT5 حديثة: {}\nتنفيذ الوسيط مفعّل: {}",
            self.version,match self.state {RunState::Stopped=>"متوقف",RunState::Starting=>"قيد البدء",RunState::Monitoring=>"يعمل",RunState::Degraded=>"يحتاج مراجعة الاتصال"},yes(self.gateway_fresh),yes(self.execution_enabled));
        if matches!(command,"/help"|"/start") {
            return "الأوامر المتاحة — للقراءة فقط:\nالحالة /status — حالة النظام\nتقرير /report — تقرير اليوم بتوقيت بغداد\nالصفقات /positions — المراكز المفتوحة\nالاتصال /health — صحة الاتصال\nالعقول /brains — حالة العقول وإحصاءاتها\nالقناص /sniper — أسلوب القناص وإعداداته\nالفرص /opportunities — فرص التحليل\nالمحاكاة /paper — نتائج افتراضية\nالأخبار /news — روابط الأخبار الرسمية\nالاختبار /backtest — آخر اختبار محفوظ\nمساعدة /help — هذه القائمة".into();
        }
        if !["/status","/health","/report","/positions","/brains","/opportunities","/paper"].contains(&command) {
            return "الأمر غير معروف؛ اكتب مساعدة أو /help".into();
        }
        if !self.gateway_fresh {return format!("{heading}\nبيانات الحساب الحالية غير متاحة؛ الاتصال منقطع أو آخر تحديث قديم.");}
        if command=="/paper" {
            return format!("محاكاة افتراضية فقط\nسعر حديث: {}\nالرصيد الافتراضي: {}\nالصافي المحقق افتراضياً: {:.2}\nربح افتراضي عائم: {}\nصفقات مغلقة: {} | رابحة: {} | منقطعة: {}\nتتضمن السبريد والعمولة والانزلاق المحدد؛ لا تمثل تعبئة MT5 فعلية ولا تشمل سواب المبيت.",yes(self.paper.quote_current),number(self.paper.equity.filter(|_|self.paper.quote_current)),self.paper.realized,number(self.paper.unrealized.filter(|_|self.paper.quote_current)),self.paper.trades,self.paper.wins,self.paper.interrupted_trades);
        }
        if command=="/brains" {return format!("تحليل السوق\nبيانات التحليل حديثة: {}\nالقرار: {}\nعدد أصوات التحليل: {}\nهذه خلاصة تحليل وليست تأكيد تنفيذ صفقة.",yes(self.analysis.current),side(&self.analysis.action),self.analysis.votes.len());}
        if command=="/opportunities" {
            let Some(map)=self.analysis.opportunities.as_ref().filter(|m|m.current) else {return "لا توجد فرص تحليل حديثة؛ انتظر الشموع المكتملة.".into()};
            let mut out=String::from("فرص تحليلية؛ ليست أوامر منفذة\n");
            for (i,s) in map.setups.iter().take(8).enumerate() {
                out.push_str(&format!("فرصة {}: {} | دخول {} | وقف {} | أهداف {}\n",i+1,if s.side>0 {"شراء"}else if s.side<0 {"بيع"}else{"انتظار"},number(s.entry),number(s.stop),s.targets.iter().map(|v|format!("{v:.5}")).collect::<Vec<_>>().join("، ")));
            }
            if map.setups.is_empty(){out.push_str("لا توجد فرصة مكتملة حالياً.");}return out;
        }
        let Some(t)=&self.telemetry else{return heading};
        match command {
            "/report"=>format!("{heading}\nالحساب: {} / {}\nاليوم: {} — بغداد\nالرصيد: {:.2} {}\nحقوق الحساب: {:.2} {}\nصافي سجلات اليوم شاملاً العمولة والسواب والرسوم: {}\nعدد سجلات التنفيذ، وليس الصفقات المغلقة: {}\nمراكز مفتوحة: {} | أوامر معلقة: {}",t.login,t.server,t.report_day,t.balance,t.currency,t.equity,t.currency,number(t.today_net),t.today_deals.map(|v|v.to_string()).unwrap_or_else(||"غير متاح".into()),t.positions.len(),t.pending_orders),
            "/positions"=>format!("{heading}\nمراكز مفتوحة: {}\n{}{}",t.positions.len(),t.positions.iter().take(25).map(|p|format!("#{} {} {} {:.3} لوت | ربح عائم {:.2} {} | وقف {} | هدف {}",p.ticket,p.symbol,side(&p.side),p.volume,p.profit,t.currency,p.sl,p.tp)).collect::<Vec<_>>().join("\n"),if t.positions.len()>25 {"\nعُرضت أول 25؛ التفاصيل الكاملة في الداشبورد."}else{""}),
            _=>format!("{heading}\nالحساب: {} / {}\nالأوامر هنا للاستعلام فقط. تشغيل النظام والتحكم بالعقول من الداشبورد.\nراجع سجل الداشبورد لتفاصيل أي خطأ اتصال.",t.login,t.server),
        }
    }

}

#[cfg(test)]
mod tests {
    #[test]
    fn telegram_reports_are_arabic_and_do_not_echo_raw_errors() {
        let mut s=super::Status::default();s.gateway_fresh=true;
        s.telemetry=Some(super::Telemetry::default());s.message="RAW_ENGLISH_INTERNAL_ERROR".into();
        for cmd in ["/help","/status","/report","/positions","/health","/brains","/paper","/opportunities","/invalid"] {
            let text=s.report(cmd);
            assert!(text.chars().any(|c| ('\u{0600}'..='\u{06ff}').contains(&c)),"{cmd}: {text}");
            assert!(!text.contains("RAW_ENGLISH_INTERNAL_ERROR"));
            assert!(!text.contains("Unknown command"));
        }
    }

    use super::*;
    #[test]
    fn p0d3_broker_used_margin_accepts_explicit_zero_and_refuses_negative_or_nan() {
        let mut t=Telemetry::default();
        t.margin=Some(0.0);
        assert!(t.validate());
        let mut value=serde_json::to_value(&t).unwrap();
        assert_eq!(value["margin"],0.0);
        t.margin=Some(-1.0);
        assert!(!t.validate());
        t.margin=Some(f64::NAN);
        assert!(!t.validate());
        value.as_object_mut().unwrap().remove("margin");
        let legacy:Telemetry=serde_json::from_value(value).unwrap();
        assert!(legacy.margin.is_none());
        assert!(legacy.validate(),"Old snapshots lacking Margin remain readable");
    }
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
