//! Official RSS headlines; context only, no sentiment, economic calendar or execution signal.
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use quick_xml::{events::Event, Reader};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, io::Read, sync::mpsc, time::Duration};
const MAX_BYTES: usize = 1_048_576;
const REFRESH_MS: u64 = 300_000;
const STALE_MS: u64 = 900_000;
#[derive(Clone, Copy)]
struct Feed {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    host: &'static str,
}
const FEEDS: [Feed; 3] = [
    Feed {
        id: "fed",
        name: "Federal Reserve — Monetary Policy",
        url: "https://www.federalreserve.gov/feeds/press_monetary.xml",
        host: "www.federalreserve.gov",
    },
    Feed {
        id: "ecb",
        name: "ECB — Press / Speeches",
        url: "https://www.ecb.europa.eu/rss/press.html",
        host: "www.ecb.europa.eu",
    },
    Feed {
        id: "ecb-stat",
        name: "ECB — Statistics",
        url: "https://www.ecb.europa.eu/rss/statpress.html",
        host: "www.ecb.europa.eu",
    },
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Headline {
    pub title: String,
    pub url: String,
    pub published_ms: Option<u64>,
    pub fetched_ms: u64,
    pub topic: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Cache {
    id: String,
    last_attempt_ms: u64,
    last_success_ms: u64,
    error: String,
    items: Vec<Headline>,
}
#[derive(Serialize, Deserialize)]
struct Saved {
    enabled: bool,
    sources: Vec<Cache>,
}
struct Update {
    id: &'static str,
    at: u64,
    result: std::result::Result<Vec<Headline>, String>,
}
pub struct Service {
    saved: Saved,
    rx: Option<mpsc::Receiver<Update>>,
    next_ms: u64,
    last_start_ms: u64,
    offline: bool,
    storage_error: String,
}
fn safe_url(url: &str, host: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some(host)
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
    })
}
pub fn allowed_article_url(url: &str) -> bool {
    FEEDS.iter().any(|f| safe_url(url, f.host))
}
fn clean(s: &str, limit: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
fn published(s: &str, now: u64) -> Option<u64> {
    let d = DateTime::parse_from_rfc2822(s.trim())
        .or_else(|_| DateTime::parse_from_rfc3339(s.trim()))
        .ok()?;
    let n = u64::try_from(d.timestamp_millis()).ok()?;
    (n > 0 && n <= now.saturating_add(300_000)).then_some(n)
}
fn topic(title: &str) -> &'static str {
    let s = title.to_lowercase();
    if ["monetary", "fomc", "interest rate"]
        .iter()
        .any(|w| s.contains(w))
    {
        "سياسة نقدية / كلمات عنوان"
    } else if ["inflation", "price index"].iter().any(|w| s.contains(w)) {
        "تضخم / كلمات عنوان"
    } else if ["employment", "labour", "labor"]
        .iter()
        .any(|w| s.contains(w))
    {
        "توظيف / كلمات عنوان"
    } else {
        "عام / غير مصنف"
    }
}
fn parse(xml: &str, feed: Feed, at: u64) -> Result<Vec<Headline>> {
    if xml.len() > MAX_BYTES {
        bail!("Feed exceeds 1 MiB limit");
    }
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<String> = vec![];
    let mut root = false;
    let mut channel = false;
    let mut in_item = false;
    let mut title = String::new();
    let mut link = String::new();
    let mut date = String::new();
    let mut result = vec![];
    let mut seen = HashSet::new();
    loop {
        let mut text = None;
        match reader.read_event().context("Invalid RSS XML")? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if stack.is_empty() {
                    if name != "rss" || root {
                        bail!("Expected one RSS root");
                    }
                    root = true;
                }
                if stack.len() >= 32 {
                    bail!("RSS nesting exceeds limit");
                }
                if name == "channel" && stack.len() == 1 {
                    channel = true;
                }
                if name == "item" && stack == ["rss", "channel"] {
                    in_item = true;
                    title.clear();
                    link.clear();
                    date.clear();
                }
                stack.push(name);
            }
            Event::End(e) => {
                if in_item && e.name().as_ref() == b"item" && stack.len() == 3 {
                    in_item = false;
                    let title = clean(&title, 500);
                    let link = clean(&link, 2048);
                    if !title.is_empty() && safe_url(&link, feed.host) && seen.insert(link.clone())
                    {
                        result.push(Headline {
                            topic: topic(&title).into(),
                            title,
                            url: link,
                            published_ms: published(&date, at),
                            fetched_ms: at,
                        });
                    }
                }
                stack.pop().context("Unbalanced XML")?;
            }
            Event::Text(e) => text = Some(e.decode()?.into_owned()),
            Event::CData(e) => text = Some(e.decode()?.into_owned()),
            Event::GeneralRef(e) => {
                let raw = format!("&{};", e.decode()?);
                text = Some(quick_xml::escape::unescape(&raw)?.into_owned());
            }
            Event::DocType(_) => bail!("RSS DTD is not supported"),
            Event::Eof => break,
            _ => {}
        }
        if let Some(t) = text {
            if in_item && stack.len() == 4 {
                let dest = match stack.last().map(String::as_str) {
                    Some("title") => Some(&mut title),
                    Some("link") => Some(&mut link),
                    Some("pubDate") => Some(&mut date),
                    _ => None,
                };
                if let Some(dest) = dest {
                    if dest.len() + t.len() <= 8192 {
                        dest.push_str(&t);
                    } else {
                        bail!("RSS field exceeds limit");
                    }
                }
            }
        }
    }
    if !root || !channel || !stack.is_empty() || result.is_empty() {
        bail!("No valid official RSS headlines; previous cache retained");
    }
    result.sort_by_key(|i| std::cmp::Reverse(i.published_ms.unwrap_or(0)));
    result.truncate(60);
    Ok(result)
}
fn fetch(http: &reqwest::blocking::Client, feed: Feed, at: u64) -> Result<Vec<Headline>> {
    let response = http
        .get(feed.url)
        .send()
        .context("Source request failed")?
        .error_for_status()
        .context("Source returned an HTTP error")?;
    if !safe_url(response.url().as_str(), feed.host) {
        bail!("Unexpected source redirect");
    }
    let mut bytes = vec![];
    response
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .context("Source read failed")?;
    if bytes.len() > MAX_BYTES {
        bail!("Feed exceeds 1 MiB limit");
    }
    parse(
        std::str::from_utf8(&bytes).context("Source is not UTF-8")?,
        feed,
        at,
    )
}
fn stamp(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .filter(|_| ms > 0)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "غير متاح".into())
}
impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS news_cache(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);")?;
        use rusqlite::OptionalExtension;
        let row: Option<String> = db
            .query_row("SELECT payload FROM news_cache WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let mut saved = if let Some(row) = row {
            serde_json::from_str(&row).context("Stored news cache invalid")?
        } else {
            Saved {
                enabled: true,
                sources: vec![],
            }
        };
        saved.sources.retain(|s| FEEDS.iter().any(|f| f.id == s.id));
        for f in FEEDS {
            if !saved.sources.iter().any(|s| s.id == f.id) {
                saved.sources.push(Cache {
                    id: f.id.into(),
                    ..Default::default()
                });
            }
        }
        Ok(Self {
            saved,
            rx: None,
            next_ms: 0,
            last_start_ms: 0,
            offline: std::env::var_os("BRAINTRADE_NEWS_OFFLINE").is_some(),
            storage_error: String::new(),
        })
    }
    fn persist(&mut self, db: &Connection) -> Result<()> {
        db.execute("INSERT INTO news_cache(id,payload) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![serde_json::to_string(&self.saved)?])?;
        self.storage_error.clear();
        Ok(())
    }
    fn begin(&mut self, now: u64) {
        if self.rx.is_some() || self.offline || !self.saved.enabled {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.last_start_ms = now;
        std::thread::spawn(move || {
            let http = reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(12))
                .user_agent("BrainTrade/0.3.0 official RSS reader")
                .redirect(reqwest::redirect::Policy::custom(|a| {
                    if a.previous().len() >= 3 {
                        return a.error("Redirect limit");
                    }
                    if a.previous().first().is_some_and(|first| {
                        a.url().scheme() == "https" && a.url().host_str() == first.host_str()
                    }) {
                        a.follow()
                    } else {
                        a.stop()
                    }
                }))
                .build();
            for feed in FEEDS {
                let at = crate::now_ms();
                let result = match &http {
                    Ok(h) => fetch(h, feed, at).map_err(|e| clean(&e.to_string(), 180)),
                    Err(_) => Err("HTTP client unavailable".into()),
                };
                if tx
                    .send(Update {
                        id: feed.id,
                        at,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    }
    fn apply(&mut self, u: Update) {
        if let Some(s) = self.saved.sources.iter_mut().find(|s| s.id == u.id) {
            s.last_attempt_ms = u.at;
            match u.result {
                Ok(items) => {
                    s.items = items;
                    s.last_success_ms = u.at;
                    s.error.clear();
                }
                Err(e) => s.error = e,
            }
        }
    }
    pub fn tick(&mut self, db: &Connection, now: u64) {
        loop {
            let Some(rx) = &self.rx else {
                break;
            };
            match rx.try_recv() {
                Ok(u) => {
                    self.apply(u);
                    if let Err(e) = self.persist(db) {
                        self.storage_error = format!("تعذر حفظ الأخبار: {e}");
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.next_ms = now.saturating_add(REFRESH_MS);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        if now >= self.next_ms {
            self.begin(now);
        }
    }
    pub fn snapshot(&self, now: u64) -> Value {
        let mut items = vec![];
        let mut sources = vec![];
        let mut lines = vec![];
        let mut urls = HashSet::new();
        for feed in FEEDS {
            let s = self.saved.sources.iter().find(|s| s.id == feed.id).unwrap();
            let stale = !self.saved.enabled
                || self.offline
                || s.last_success_ms == 0
                || now < s.last_success_ms
                || now.saturating_sub(s.last_success_ms) > STALE_MS;
            let state = if !s.error.is_empty() {
                "تعذر التحديث — المحفوظ فقط"
            } else if stale {
                "لا توجد بيانات حديثة"
            } else {
                "اتصال حديث — راجع تاريخ كل خبر"
            };
            lines.push(format!(
                "{} | {}\nآخر نجاح: {} | آخر محاولة: {}{}",
                feed.name,
                state,
                stamp(s.last_success_ms),
                stamp(s.last_attempt_ms),
                if s.error.is_empty() {
                    String::new()
                } else {
                    format!("\n{}", s.error)
                }
            ));
            sources.push(json!({"id":feed.id,"name":feed.name,"url":feed.url,"stale":stale,"error":s.error,"last_attempt_ms":s.last_attempt_ms,"last_success_ms":s.last_success_ms,"items":s.items.len()}));
            for item in &s.items {
                if !urls.insert(item.url.clone()) {
                    continue;
                }
                let mut v = serde_json::to_value(item).unwrap();
                v["source"] = json!(feed.name);
                v["source_stale"] = json!(stale || !s.error.is_empty());
                v["published"] = json!(item
                    .published_ms
                    .map(stamp)
                    .unwrap_or("وقت نشر غير متاح / غير صالح".into()));
                items.push(v);
            }
        }
        items.sort_by_key(|i| std::cmp::Reverse(i["published_ms"].as_u64().unwrap_or(0)));
        items.truncate(100);
        json!({"enabled":self.saved.enabled,"offline":self.offline,"refreshing":self.rx.is_some(),"sources":sources,"sources_text":lines.join("\n\n"),"items":items,"storage_error":self.storage_error,"note":"عناوين منشورة وليست تقويماً للأحداث القادمة. التصنيف بالكلمات لا يحدد الأثر أو اتجاه السعر؛ لا يدخل في قرار Meta أو الاختبار التاريخي."})
    }
    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        db: &Connection,
    ) -> Result<Value> {
        let now = crate::now_ms();
        match (method, path) {
            ("GET", "/v1/news") => Ok(self.snapshot(now)),
            ("POST", "/v1/news/refresh") => {
                if self.offline || !self.saved.enabled {
                    bail!("تحديث الأخبار متوقف");
                }
                if self.rx.is_some() {
                    return Ok(json!({"ok":true,"note":"التحديث جارٍ"}));
                }
                if now.saturating_sub(self.last_start_ms) < 60_000 {
                    bail!("انتظر دقيقة بين محاولات التحديث اليدوي");
                }
                self.begin(now);
                Ok(json!({"ok":true,"note":"بدأ تحديث المصادر"}))
            }
            ("POST", "/v1/news/settings") => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Settings {
                    enabled: bool,
                }
                let s: Settings = serde_json::from_slice(body)?;
                let old = self.saved.enabled;
                self.saved.enabled = s.enabled;
                if let Err(e) = self.persist(db) {
                    self.saved.enabled = old;
                    return Err(e);
                }
                if !s.enabled {
                    self.rx = None;
                } else {
                    self.next_ms = now;
                }
                Ok(
                    json!({"ok":true,"note":if s.enabled{"تم تفعيل تحديث الأخبار"}else{"تم إيقاف تحديث الأخبار؛ المحفوظ يبقى متاحاً"}}),
                )
            }
            _ => bail!("Unknown news route"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const NOW: u64 = 1_800_000_000_000;
    fn xml(items: &str) -> String {
        format!("<rss version=\"2.0\"><channel><title>Feed</title>{items}</channel></rss>")
    }
    fn item(title: &str, url: &str, date: &str) -> String {
        format!("<item><title>{title}</title><link>{url}</link><pubDate>{date}</pubDate></item>")
    }
    #[test]
    fn parse_entities_dates_dedup_and_official_links() {
        let a = item(
            "Rates &amp; &lt;policy&gt; &#49;",
            "https://www.federalreserve.gov/a",
            "Wed, 01 Oct 2025 14:00:00 +0000",
        );
        let b = item(
            "<![CDATA[Other]]>",
            "https://www.federalreserve.gov/b",
            "not a date",
        );
        let evil = item(
            "External",
            "https://www.federalreserve.gov.evil.test/a",
            "2025-10-01T14:00:00Z",
        );
        let out = parse(&xml(&(a.clone() + &a + &b + &evil)), FEEDS[0], NOW).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].title, "Rates & <policy> 1");
        assert!(out[0].published_ms.is_some());
        assert_eq!(out[1].published_ms, None);
        assert!(!allowed_article_url("file:///tmp/x"));
        assert!(!allowed_article_url("https://www.ecb.europa.eu@evil.test"));
        assert_eq!(published("2100-01-01T00:00:00Z", NOW), None);
    }
    #[test]
    fn invalid_feed_cannot_become_fresh_empty_news() {
        for s in [
            "<html>blocked</html>",
            "<rss><channel>",
            "<!DOCTYPE rss [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><rss><channel/></rss>",
            "<rss><channel/></rss>",
        ] {
            assert!(parse(s, FEEDS[0], NOW).is_err());
        }
    }
    #[test]
    fn failure_retains_cache_and_restart_preserves_disabled_setting() {
        let db = Connection::open_in_memory().unwrap();
        let mut s = Service::new(&db).unwrap();
        s.saved.enabled = false;
        let rows = parse(
            &xml(&item(
                "Policy",
                "https://www.federalreserve.gov/a",
                "2025-10-01T00:00:00Z",
            )),
            FEEDS[0],
            NOW,
        )
        .unwrap();
        s.apply(Update {
            id: "fed",
            at: NOW,
            result: Ok(rows),
        });
        s.apply(Update {
            id: "fed",
            at: NOW + 1000,
            result: Err("Timeout".into()),
        });
        s.persist(&db).unwrap();
        let s = Service::new(&db).unwrap();
        assert!(!s.saved.enabled);
        assert_eq!(s.saved.sources[0].items.len(), 1);
        assert_eq!(s.saved.sources[0].last_success_ms, NOW);
        assert_eq!(s.snapshot(NOW + 2000)["items"][0]["source_stale"], true);
        assert_eq!(s.snapshot(NOW + STALE_MS + 1)["sources"][0]["stale"], true);
    }
    #[test]
    #[ignore = "explicit public-network check"]
    fn live_official_feeds() {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        for feed in FEEDS {
            let items = fetch(&http, feed, crate::now_ms()).unwrap();
            assert!(!items.is_empty());
            println!("{}: {} valid items", feed.id, items.len());
        }
    }
}
