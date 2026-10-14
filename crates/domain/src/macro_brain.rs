use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MacroHeadline {
    pub source: String,
    pub topic: String,
    pub title: String,
    pub published_ms: Option<u64>,
    pub source_stale: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MacroBrain {
    pub version: String,
    pub current: bool,
    pub evaluated_ms: u64,
    pub state: String,
    pub fresh_sources: usize,
    pub recent_15m: usize,
    pub recent_60m: usize,
    pub attention_30m: usize,
    pub latest_age_minutes: Option<u64>,
    pub topics: Vec<String>,
    pub note: String,
}
impl Default for MacroBrain {
    fn default() -> Self {
        Self {
            version: "macro-news-context-v1".into(),
            current: false,
            evaluated_ms: 0,
            state: "STALE".into(),
            fresh_sources: 0,
            recent_15m: 0,
            recent_60m: 0,
            attention_30m: 0,
            latest_age_minutes: None,
            topics: vec![],
            note: "أخبار رسمية غير متاحة أو قديمة. لا يوجد تقويم أحداث مستقبلية في هذه النسخة.".into(),
        }
    }
}
impl MacroBrain {
    pub fn text(&self) -> String {
        format!(
            "MACRO / NEWS CONTEXT — {}\nFresh sources: {} | 15m: {} | 60m: {} | attention 30m: {}\nLatest age: {} | Topics: {}\n{}\nلا يعطي اتجاه BUY/SELL ولا يحول العنوان إلى sentiment؛ هو سياق مخاطر فقط.",
            self.state,
            self.fresh_sources,
            self.recent_15m,
            self.recent_60m,
            self.attention_30m,
            self.latest_age_minutes.map(|v|format!("{v}m")).unwrap_or("غير متاح".into()),
            if self.topics.is_empty(){"—".into()}else{self.topics.join(" | ")},
            self.note
        )
    }
}

pub fn assess(headlines: &[MacroHeadline], fresh_sources: usize, now: u64) -> MacroBrain {
    let mut out = MacroBrain {
        evaluated_ms: now,
        fresh_sources,
        current: fresh_sources > 0,
        ..Default::default()
    };
    if fresh_sources == 0 {
        return out;
    }

    let mut latest: Option<u64> = None;
    let mut topics = Vec::<String>::new();
    for item in headlines.iter().filter(|item| !item.source_stale) {
        let Some(published) = item.published_ms.filter(|value| *value <= now) else { continue; };
        let age = now.saturating_sub(published);
        latest = Some(latest.map_or(published, |old| old.max(published)));
        if age <= 60 * 60 * 1000 {
            out.recent_60m += 1;
            if !item.topic.is_empty() && !topics.contains(&item.topic) {
                topics.push(item.topic.clone());
            }
        }
        if age <= 15 * 60 * 1000 {
            out.recent_15m += 1;
        }
        let attention_topic = item.topic.contains("سياسة نقدية")
            || item.topic.contains("تضخم")
            || item.topic.contains("توظيف");
        if attention_topic && age <= 30 * 60 * 1000 {
            out.attention_30m += 1;
        }
    }
    topics.sort();
    topics.truncate(6);
    out.topics = topics;
    out.latest_age_minutes = latest.map(|published| now.saturating_sub(published) / 60_000);
    out.state = if out.attention_30m > 0 {
        "ATTENTION".into()
    } else if out.recent_60m > 0 {
        "RECENT_NEWS".into()
    } else {
        "QUIET".into()
    };
    out.note = match out.state.as_str() {
        "ATTENTION" => "عنوان رسمي حديث ضمن سياسة نقدية/تضخم/توظيف؛ ارفع الانتباه للتقلب، لكن اتجاه الأثر غير مستنتج من العنوان.".into(),
        "RECENT_NEWS" => "توجد عناوين رسمية حديثة خلال الساعة؛ سياق فقط ولا توجد إشارة اتجاه.".into(),
        _ => "المصادر متصلة ولا توجد عناوين منشورة حديثاً خلال الساعة في الكاش الحالي.".into(),
    };
    out
}
