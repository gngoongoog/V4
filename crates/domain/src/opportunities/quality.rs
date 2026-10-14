use crate::{MarketData, Telemetry};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FrameQuality {
    pub timeframe: String,
    pub bars: usize,
    pub recent_gaps: usize,
    pub zero_tick_ratio: f64,
    pub last_close_age_ms: Option<u64>,
    pub ready: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedQuality {
    pub version: String,
    pub grade: String,
    pub score: f64,
    pub analysis_ready: bool,
    pub trade_plan_ready: bool,
    pub quote_fresh: bool,
    pub quote_age_ms: Option<u64>,
    pub candle_snapshot_fresh: bool,
    pub contract_specs: bool,
    pub tick_activity_proxy: bool,
    pub dom_depth: bool,
    pub executed_order_flow: bool,
    pub frames: Vec<FrameQuality>,
    pub issues: Vec<String>,
    pub note: String,
}

impl Default for FeedQuality {
    fn default() -> Self {
        Self {
            version: "data-quality-v1".into(),
            grade: "UNKNOWN".into(),
            score: 0.0,
            analysis_ready: false,
            trade_plan_ready: false,
            quote_fresh: false,
            quote_age_ms: None,
            candle_snapshot_fresh: false,
            contract_specs: false,
            tick_activity_proxy: false,
            dom_depth: false,
            executed_order_flow: false,
            frames: vec![],
            issues: vec!["لم يتم تقييم جودة البيانات بعد".into()],
            note: "درجة الجودة تقيس اكتمال وحداثة المدخلات وليست احتمال نجاح صفقة.".into(),
        }
    }
}

fn frame_quality(series: &[crate::Candle], timeframe: &str, seconds: u64, now: u64) -> FrameQuality {
    if series.is_empty() {
        return FrameQuality {
            timeframe: timeframe.into(),
            ..Default::default()
        };
    }
    let recent = &series[series.len().saturating_sub(30)..];
    let recent_gaps = recent
        .windows(2)
        .filter(|w| w[1].time.saturating_sub(w[0].time) != seconds)
        .count();
    let zero_tick_ratio = if recent.is_empty() {
        1.0
    } else {
        recent.iter().filter(|bar| bar.tick_volume == 0).count() as f64 / recent.len() as f64
    };
    let close_ms = series
        .last()
        .map(|bar| bar.time.saturating_add(seconds).saturating_mul(1000));
    let last_close_age_ms = close_ms.and_then(|close| (now >= close).then_some(now - close));
    let ready = series.len() >= 100
        && recent_gaps == 0
        && last_close_age_ms.is_some_and(|age| age <= seconds * 1000 + 20_000);
    FrameQuality {
        timeframe: timeframe.into(),
        bars: series.len(),
        recent_gaps,
        zero_tick_ratio,
        last_close_age_ms,
        ready,
    }
}

fn contract_specs_ready(t: &Telemetry) -> bool {
    match (
        t.tick_size,
        t.tick_value_loss,
        t.tick_value_profit,
        t.volume_min,
        t.volume_step,
        t.volume_max,
    ) {
        (Some(tick), Some(loss), Some(profit), Some(min), Some(step), Some(max)) => {
            [tick, loss, profit, min, step, max]
                .iter()
                .all(|v| v.is_finite() && *v > 0.0)
                && min <= max
        }
        _ => false,
    }
}

fn market_quality(market: Option<&MarketData>, now: u64) -> (bool, Vec<FrameQuality>, bool, Vec<String>) {
    let Some(market) = market else {
        return (
            false,
            vec![],
            false,
            vec!["بيانات الشموع غير متاحة".into()],
        );
    };
    let mut issues = vec![];
    let snapshot_fresh = now >= market.captured_ms && now.saturating_sub(market.captured_ms) <= 15_000;
    if !snapshot_fresh {
        issues.push("لقطة الشموع أقدم من 15 ثانية أو توقيتها غير صالح".into());
    }
    if !market.error.is_empty() {
        issues.push(format!("بوابة السوق: {}", market.error));
    }
    let frames = vec![
        frame_quality(&market.m1, "M1", 60, now),
        frame_quality(&market.m5, "M5", 300, now),
        frame_quality(&market.m15, "M15", 900, now),
    ];
    for frame in &frames {
        if frame.bars < 100 {
            issues.push(format!("{}: أقل من 100 شمعة مكتملة", frame.timeframe));
        }
        if frame.recent_gaps > 0 {
            issues.push(format!("{}: {} فجوات ضمن آخر 30 شمعة", frame.timeframe, frame.recent_gaps));
        }
        if frame.last_close_age_ms.is_none() {
            issues.push(format!("{}: وقت آخر إغلاق غير صالح", frame.timeframe));
        } else if !frame.ready {
            issues.push(format!("{}: آخر شموع مكتملة قديمة بالنسبة للفريم", frame.timeframe));
        }
    }
    let tick_activity_proxy = frames.iter().all(|frame| frame.zero_tick_ratio < 0.50);
    if !tick_activity_proxy {
        issues.push("tick-volume ضعيف/مفقود في نسبة كبيرة من الشموع؛ تأكيد النشاط الحجمي محدود".into());
    }
    let ready = market.valid()
        && snapshot_fresh
        && market.error.is_empty()
        && frames.iter().all(|frame| frame.ready);
    (ready, frames, tick_activity_proxy, issues)
}

pub fn assess(t: &Telemetry, now: u64) -> FeedQuality {
    let mut score: f64 = 1.0;
    let mut issues = vec![];
    if !t.connected {
        score -= 0.35;
        issues.push("MT5 غير متصل".into());
    }

    let quote_fresh = t.tick_age_ms.is_some_and(|age| age <= 3000)
        && matches!((t.bid, t.ask), (Some(b), Some(a)) if b.is_finite() && a.is_finite() && b > 0.0 && a >= b);
    if !quote_fresh {
        score -= 0.25;
        issues.push("السعر الحالي غير متاح أو أقدم من 3 ثوانٍ".into());
    }

    let contract_specs = contract_specs_ready(t);
    if !contract_specs {
        score -= 0.15;
        issues.push("مواصفات tick أو حدود اللوت غير مكتملة".into());
    }

    let (candles_ready, frames, tick_activity_proxy, market_issues) = market_quality(t.market.as_ref(), now);
    if !candles_ready {
        score -= 0.25;
    }
    issues.extend(market_issues);
    if !tick_activity_proxy {
        score -= 0.05;
    }

    let dom_depth = t.dom_available && !t.dom.is_empty();
    let analysis_ready = t.connected && candles_ready;
    let trade_plan_ready = analysis_ready && quote_fresh && contract_specs;
    score = score.clamp(0.0, 1.0);
    let grade = if trade_plan_ready && score >= 0.85 {
        "GOOD"
    } else if analysis_ready && score >= 0.60 {
        "DEGRADED"
    } else {
        "BLOCKED"
    };

    FeedQuality {
        version: "data-quality-v1".into(),
        grade: grade.into(),
        score,
        analysis_ready,
        trade_plan_ready,
        quote_fresh,
        quote_age_ms: t.tick_age_ms,
        candle_snapshot_fresh: t
            .market
            .as_ref()
            .is_some_and(|m| now >= m.captured_ms && now.saturating_sub(m.captured_ms) <= 15_000),
        contract_specs,
        tick_activity_proxy,
        dom_depth,
        executed_order_flow: false,
        frames,
        issues,
        note: "الدرجة تقيس جودة/قدرة البيانات فقط. tick-volume وملف الحجم proxies من MT5، وDOM إن توفر هو resting depth عند الوسيط؛ لا يوجد executed order-flow/footprint من هذا المسار.".into(),
    }
}

impl FeedQuality {
    pub fn unavailable(reason: &str) -> Self {
        Self {
            grade: "BLOCKED".into(),
            issues: vec![reason.into()],
            note: "بيانات التحليل سُحبت حتى لا تبقى قراءة قديمة معروضة كأنها حالية.".into(),
            ..Default::default()
        }
    }

    pub fn mark_quote_stale(&mut self, reason: &str) {
        self.quote_fresh = false;
        self.trade_plan_ready = false;
        self.grade = "BLOCKED".into();
        self.score = self.score.min(0.59);
        if !self.issues.iter().any(|item| item == reason) {
            self.issues.push(reason.into());
        }
    }

    pub fn text(&self) -> String {
        let quote = self
            .quote_age_ms
            .map(|age| format!("{} ms", age))
            .unwrap_or_else(|| "غير متاح".into());
        let frame_text = if self.frames.is_empty() {
            "الفريمات غير متاحة".into()
        } else {
            self.frames
                .iter()
                .map(|frame| {
                    format!(
                        "{}: bars {} | gaps {} | zero-tick {:.0}% | close-age {} | ready {}",
                        frame.timeframe,
                        frame.bars,
                        frame.recent_gaps,
                        frame.zero_tick_ratio * 100.0,
                        frame
                            .last_close_age_ms
                            .map(|age| format!("{} ms", age))
                            .unwrap_or_else(|| "—".into()),
                        frame.ready
                    )
                })
                .collect::<Vec<_>>()
                .join(" ; ")
        };
        format!(
            "DATA QUALITY BRAIN / عقل جودة البيانات\n{} | grade {} | score {:.2} (ليست نسبة نجاح)\nAnalysis ready: {} | Trade-plan inputs: {} | Quote age: {}\nCapabilities: candles {} | contract specs {} | tick-activity proxy {} | broker DOM {} | executed order-flow {}\n{}\nIssues: {}\n{}",
            self.version,
            self.grade,
            self.score,
            self.analysis_ready,
            self.trade_plan_ready,
            quote,
            self.analysis_ready,
            self.contract_specs,
            self.tick_activity_proxy,
            self.dom_depth,
            self.executed_order_flow,
            frame_text,
            if self.issues.is_empty() { "none".into() } else { self.issues.join(" • ") },
            self.note
        )
    }
}
