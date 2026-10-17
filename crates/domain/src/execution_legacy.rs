//! Independent, deterministic trading policies. Broker transport lives in the gateway.
#[path = "execution_liquidity.rs"]
mod liquidity;
#[path = "execution_pullback.rs"]
mod pullback;
use crate::{
    studio::{FrameBar, Genome},
    trading::{self, TradingMode},
    Candle, Config, Telemetry,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Append new identities only. Existing index -> magic mappings are durable broker ownership.
pub const BRAINS: [(&str, &str); 16] = [
    ("system", "النظام — الوضع المختار"),
    ("gann", "جان الهندسي"),
    ("wyckoff", "وايكوف"),
    ("trend", "الترند"),
    ("momentum", "الزخم"),
    ("structure", "اختراق القمم والقيعان"),
    ("zones", "الطلب والعرض والارتداد"),
    ("dom", "عمق السوق الفعلي"),
    ("profile", "ملف النشاط الحجمي"),
    ("gann_dom_profile", "جان + العمق + الملف الحجمي"),
    ("ces", "CES — الانتروبيا"),
    ("kte", "KTE — الاندفاع والارتداد"),
    ("regime", "حالة السوق — اتجاه أو نطاق"),
    ("generated", "استراتيجية مولّدة مختارة"),
    ("liquidity", "سحب السيولة — sweep + reclaim"),
    ("trend_pullback", "Trend Pullback — رجوع منظم + تأكيد"),
];
pub fn magic(id: &str) -> Option<u64> {
    BRAINS
        .iter()
        .position(|(b, _)| *b == id)
        .map(|i| 410_600 + i as u64)
}
pub fn brain_for_magic(m: u64) -> Option<&'static str> {
    m.checked_sub(410_600)
        .and_then(|i| BRAINS.get(i as usize))
        .map(|b| b.0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BrainConfig {
    pub id: String,
    pub enabled: bool,
    pub timeframe: String,
    pub lots: f64,
    pub stop_atr: f64,
    pub reward_risk: f64,
    pub max_hold_seconds: u64,
    pub cooldown_seconds: u64,
    pub use_risk_limits: bool,
    pub report_id: String,
}
impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            id: "system".into(),
            enabled: false,
            timeframe: "M1".into(),
            lots: 0.01,
            stop_atr: 1.5,
            reward_risk: 2.,
            max_hold_seconds: 1800,
            cooldown_seconds: 0,
            use_risk_limits: false,
            report_id: String::new(),
        }
    }
}
impl BrainConfig {
    pub fn validate(&self) -> Result<(), String> {
        if magic(&self.id).is_none()
            || !crate::studio::TIMEFRAMES.contains(&self.timeframe.as_str())
        {
            return Err("Unknown brain or timeframe".into());
        }
        if [self.lots, self.stop_atr, self.reward_risk]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.)
            || self.lots > 1_000_000.
            || self.stop_atr > 100.
            || self.reward_risk > 100.
            || self.max_hold_seconds > 31_536_000
            || self.cooldown_seconds > 604_800
        {
            return Err("Invalid lot, stop, target or duration".into());
        }
        if matches!(self.id.as_str(), "ces" | "kte") && self.timeframe != "M1" {
            return Err("CES/KTE v1 require M1".into());
        }
        if self.report_id.len() > 100 {
            return Err("Invalid report ID".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal {
    pub brain: String,
    pub side: i8,
    pub key: String,
    pub bar_time: u64,
    pub stop_distance: f64,
    pub target_distance: f64,
    pub hold_ms: u64,
    pub reason: String,
    pub ready: bool,
    #[serde(default)] pub coordination:Option<serde_json::Value>,
}
fn empty(b: &BrainConfig, why: &str) -> Signal {
    Signal {
        brain: b.id.clone(),
        side: 0,
        key: String::new(),
        bar_time: 0,
        stop_distance: 0.,
        target_distance: 0.,
        hold_ms: b.max_hold_seconds * 1000,
        reason: why.into(),
        ready: false,
        coordination:None,
    }
}

pub fn evaluate(
    b: &BrainConfig,
    t: &Telemetry,
    c: &Config,
    frames: &BTreeMap<String, Vec<FrameBar>>,
    genome: Option<&Genome>,
    now: u64,
) -> Signal {
    let mut s = empty(b, "بانتظار بيانات مكتملة وحديثة");
    if !t.connected
        || t.symbol != c.symbol
        || t.tick_age_ms.is_none_or(|v| v > 3000)
        || t.quote_time_ms
            .is_none_or(|v| v > now + 2000 || now.saturating_sub(v) > 3000)
    {
        return s;
    }
    if b.id == "system" {
        let candles = frames
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().map(|b| b.candle.clone()).collect()))
            .collect();
        let a = trading::select(
            crate::research::analyze(t.market.as_ref(), &t.symbol, now),
            t,
            c,
            &candles,
            now,
        );
        if !a.current || !a.gates.is_empty() {
            s.reason = if a.gates.is_empty() {
                a.reason
            } else {
                a.gates.join("; ")
            };
            return s;
        }
        s.side = match a.action.as_str() {
            "BUY" => 1,
            "SELL" => -1,
            _ => 0,
        };
        s.bar_time = a.bar_time;
        s.reason = a.reason;
        (s.stop_distance, s.target_distance) = c
            .trading
            .geometry(a.atr.unwrap_or(0.), t.point.unwrap_or(0.));
        s.hold_ms = c.trading.hold_ms();
        // One attempt per broker tick for Sniper, one per completed bar otherwise.
        s.key = if c.trading.mode == TradingMode::Sniper {
            format!("tick:{}", t.quote_time_ms.unwrap_or(0))
        } else {
            format!("bar:{}", s.bar_time)
        };
        if c.trading.mode == TradingMode::Sniper {
            s.target_distance += c.risk.commission_per_lot / t.tick_value_profit.unwrap_or(0.)
                * t.tick_size.unwrap_or(0.)
                + c.risk.slippage_ticks * t.tick_size.unwrap_or(0.);
        }
    } else {
        let Some(bars) = frames.get(&b.timeframe) else {
            return s;
        };
        if bars.len() < 100 {
            return s;
        }
        let series: Vec<Candle> = bars.iter().map(|v| v.candle.clone()).collect();
        let n = series.len();
        let last = &series[n - 1];
        let prev = &series[n - 2];
        let atr = trading::volatility(&series);
        if !atr.is_finite() || atr <= 0. {
            return s;
        }
        s.bar_time = last.time;
        s.key = format!("{}:{}", b.timeframe, last.time);
        s.stop_distance = b.stop_atr * atr;
        s.target_distance = b.reward_risk * s.stop_distance;
        let trend = trading::direction(&series);
        let (swings, zones) = crate::opportunities::zones::map(
            &series,
            &b.timeframe,
            crate::studio::seconds(&b.timeframe).unwrap(),
        );
        let gann = crate::opportunities::gann::assess(&series, &b.timeframe, &swings);
        let dom = crate::opportunities::dom::assess(t);
        let profile = crate::opportunities::features::profile(&series);
        let profile_side = if profile.available
            && prev.close <= profile.value_high
            && last.close > profile.value_high
        {
            1
        } else if profile.available
            && prev.close >= profile.value_low
            && last.close < profile.value_low
        {
            -1
        } else {
            0
        };
        s.reason = match b.id.as_str() {
            "gann" => {
                s.side = if gann.available
                    && gann.strength >= 0.25
                    && ((last.close - last.open) * gann.direction as f64) > 0.
                {
                    gann.direction
                } else {
                    0
                };
                "هندسة جان على الفريم المختار + شمعة بنفس الاتجاه؛ لا تصويت من عقول أخرى".into()
            }
            "wyckoff" => {
                let w = crate::wyckoff::analyze(bars);
                s.side = if w.confirmed { w.side } else { 0 };
                format!("{} | {} | {}", w.event, w.phase, w.volume_source)
            }
            "trend" => {
                s.side = trend;
                "EMA20/50 والميل على الفريم المختار".into()
            }
            "momentum" => {
                let d = (last.close - series[n - 6].close) / atr;
                s.side = if d > 0.5 {
                    1
                } else if d < -0.5 {
                    -1
                } else {
                    0
                };
                format!("زخم خمس شموع = {d:.3} ATR")
            }
            "structure" => {
                let prior = &series[n - 21..n - 1];
                let hi = prior
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lo = prior.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
                s.side = if last.close > hi + atr * 0.05 {
                    1
                } else if last.close < lo - atr * 0.05 {
                    -1
                } else {
                    0
                };
                "اختراق نطاق 20 شمعة مكتملة".into()
            }
            "zones" => {
                if let Some(z) = zones
                    .iter()
                    .filter(|z| {
                        !z.broken
                            && z.confirmed_at <= last.time
                            && last.low <= z.high
                            && last.high >= z.low
                    })
                    .find(|z| {
                        if z.side == 1 {
                            last.close > z.high && last.close > last.open
                        } else {
                            last.close < z.low && last.close < last.open
                        }
                    })
                {
                    s.side = z.side;
                }
                "ارتداد مؤكد من منطقة طلب أو عرض معلومة قبل الإشارة".into()
            }
            "dom" => {
                s.side = if dom.available { dom.direction } else { 0 };
                s.key = format!("tick:{}", t.quote_time_ms.unwrap_or(0));
                dom.note
            }
            "profile" => {
                s.side = profile_side;
                format!("خروج من منطقة قيمة نشاط الشموع؛ {}", profile.note)
            }
            "gann_dom_profile" => {
                s.side = if gann.available
                    && dom.available
                    && gann.direction != 0
                    && gann.direction == dom.direction
                    && profile_side == gann.direction
                {
                    gann.direction
                } else {
                    0
                };
                "توافق جان وعمق الوسيط وخروج من منطقة قيمة النشاط ضمن استراتيجية مركبة واحدة".into()
            }
            "ces" => {
                if let Ok(f) = crate::ces::frames(&series) {
                    if let Some(Some(f)) = f.last() {
                        s.side = f.side;
                    }
                }
                "قواعد CES الأصلية، دون اشتراط تصويت Meta".into()
            }
            "kte" => {
                if let Ok(f) = crate::kte::frames(&series, t.tick_size.unwrap_or(0.)) {
                    if let Some(Some(f)) = f.last() {
                        if let Some(k) = &f.signal {
                            s.side = k.side;
                            s.key = format!("impulse:{}", series[k.impulse].time);
                            let entry = if s.side == 1 { t.ask } else { t.bid }.unwrap_or(0.);
                            s.stop_distance = (entry - k.stop) * s.side as f64;
                            s.target_distance = s.stop_distance * b.reward_risk;
                        }
                    }
                }
                "قواعد KTE الأصلية؛ إشارة واحدة لكل اندفاع".into()
            }
            "regime" => {
                let path = series[n - 20..]
                    .windows(2)
                    .map(|w| (w[1].close - w[0].close).abs())
                    .sum::<f64>();
                let er = (last.close - series[n - 20].close).abs() / path.max(1e-12);
                if er >= 0.35 {
                    s.side = trend;
                } else {
                    let mean = series[n - 20..].iter().map(|b| b.close).sum::<f64>() / 20.;
                    s.side = if last.low < mean - atr && last.close > last.open {
                        1
                    } else if last.high > mean + atr && last.close < last.open {
                        -1
                    } else {
                        0
                    };
                }
                format!("كفاءة الاتجاه {er:.2}؛ اتباع اتجاه أو ارتداد نطاق")
            }
            "liquidity" => {
                let q = liquidity::assess(&series, t, atr);
                s.side = q.side;
                if q.side != 0 {
                    s.stop_distance = q.stop_distance;
                    s.target_distance = b.reward_risk * s.stop_distance;
                    s.key = format!("sweep:{}:{}", b.timeframe, last.time);
                }
                q.reason
            }
            "trend_pullback" => {
                let q = pullback::assess(&series, t, atr);
                s.side = q.side;
                if q.side != 0 {
                    s.stop_distance = q.stop_distance;
                    s.target_distance = b.reward_risk * s.stop_distance;
                    s.key = format!("pullback:{}:{}", b.timeframe, last.time);
                }
                q.reason
            }
            "generated" => {
                if let Some(g) = genome {
                    s.side = crate::studio::signal(g, bars);
                    s.stop_distance = g.stop_atr * atr;
                    s.target_distance = g.reward * s.stop_distance;
                    s.key = format!("{}:{}:{}", b.report_id, b.timeframe, last.time);
                    format!("قاعدة مختارة يدويًا: {}، نافذة {}", g.family, g.lookback)
                } else {
                    "اختر معرّف تقرير محفوظ من مختبر الاستراتيجيات".into()
                }
            }
            _ => "عقل غير متاح".into(),
        };
    }
    s.ready = matches!(s.side, -1 | 1)
        && [s.stop_distance, s.target_distance]
            .iter()
            .all(|v| v.is_finite() && *v > 0.);
    if s.ready && b.id=="system" && c.trading.mode==TradingMode::Sniper {
        match trading::costs(t,c,b.lots,now) {
            Ok(cost) if cost.total/(cost.total+cost.desired_net_target_money)<=c.trading.max_cost_fraction => {},
            Ok(_) => {s.ready=false;s.reason.push_str("; حصة التكلفة تتجاوز الحد المختار للقناص");},
            Err(error) => {s.ready=false;s.reason.push_str(&format!("; {error}"));}
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sniper_broker_policy_obeys_current_cost_fraction() {
        let now=1_800_000_020_000;
        let t=Telemetry{connected:true,symbol:"XAUUSD".into(),market:Some(crate::research::tests::fixture(now,false)),tick_age_ms:Some(0),quote_time_ms:Some(now),bid:Some(2055.8),ask:Some(2055.81),point:Some(0.01),tick_size:Some(0.01),tick_value_profit:Some(1.),tick_value_loss:Some(1.),..Default::default()};
        let mut c=Config::default();c.trading.mode=TradingMode::Sniper;
        let b=BrainConfig::default();let frames=BTreeMap::new();
        assert!(evaluate(&b,&t,&c,&frames,None,now).ready);
        c.trading.max_cost_fraction=0.01;
        let s=evaluate(&b,&t,&c,&frames,None,now);assert!(!s.ready);assert!(s.reason.contains("حصة التكلفة"));
    }
    #[test]
    fn identity_and_user_controls() {
        let mut b = BrainConfig::default();
        b.lots = 12.;
        b.cooldown_seconds = 0;
        assert!(b.validate().is_ok());
        assert!(!b.use_risk_limits);
        for (id, _) in BRAINS {
            assert_eq!(brain_for_magic(magic(id).unwrap()), Some(id));
        }
        // Existing durable ownership stays stable; new identities are appended only.
        assert_eq!(magic("generated"), Some(410_613));
        assert_eq!(magic("liquidity"), Some(410_614));
        assert_eq!(magic("trend_pullback"), Some(410_615));
        b.lots = f64::NAN;
        assert!(b.validate().is_err());
    }
    #[test]
    fn trend_has_no_other_brain_vote_dependency() {
        let now = 1_800_000_020_000;
        let m = crate::research::tests::fixture(now, false);
        let frames = BTreeMap::from([(
            "M1".into(),
            m.m1.iter()
                .map(|c| FrameBar {
                    candle: c.clone(),
                    spread_points: 1,
                    real_volume: 0,
                })
                .collect(),
        )]);
        let t = Telemetry {
            connected: true,
            symbol: "XAUUSD".into(),
            tick_age_ms: Some(0),
            quote_time_ms: Some(now),
            ..Default::default()
        };
        let b = BrainConfig {
            id: "trend".into(),
            ..Default::default()
        };
        let s = evaluate(&b, &t, &Config::default(), &frames, None, now);
        assert!(s.ready);
        assert_eq!(s.side, 1);
        let mut stale = t;
        stale.quote_time_ms = Some(now - 4000);
        assert!(!evaluate(&b, &stale, &Config::default(), &frames, None, now).ready);
    }
}