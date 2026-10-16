//! Append-only observed headline revisions. Publication time never grants earlier availability.
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::fmt::Write;

pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS news_observations(id INTEGER PRIMARY KEY,source TEXT NOT NULL,url TEXT NOT NULL,observed_ms INTEGER NOT NULL,payload TEXT NOT NULL);CREATE INDEX IF NOT EXISTS news_observations_lookup ON news_observations(source,url,observed_ms,id);CREATE INDEX IF NOT EXISTS news_observations_time ON news_observations(observed_ms);")?;Ok(())
}
pub fn record(db:&Connection,source:&str,items:&[super::news::Headline],observed:u64)->Result<()> {
    let tx=db.unchecked_transaction()?;
    for item in items {
        // Exclude fetched_ms from the revision identity: a refresh is not a new headline.
        let payload=json!({"title":item.title,"url":item.url,"published_ms":item.published_ms,"topic":item.topic});
        let old:Option<String>=tx.query_row("SELECT payload FROM news_observations WHERE source=?1 AND url=?2 ORDER BY id DESC LIMIT 1",params![source,item.url],|r|r.get(0)).optional()?;
        if old.as_ref().is_some_and(|v|v==&payload.to_string()){continue;}
        tx.execute("INSERT INTO news_observations(source,url,observed_ms,payload) VALUES(?1,?2,?3,?4)",params![source,item.url,observed.max(item.fetched_ms),payload.to_string()])?;
    }
    tx.commit()?;Ok(())
}
pub fn replay(db:&Connection,body:&[u8],now:u64)->Result<Value> {
    let request:Value=serde_json::from_slice(body)?;
    let at=match request["at"].as_str().filter(|v|!v.trim().is_empty()) {
        Some(text)=>u64::try_from(chrono::DateTime::parse_from_rfc3339(text).context("اكتب الوقت مع المنطقة: 2026-10-16T13:00:00+03:00")?.timestamp_millis())?,
        None=>now,
    };
    anyhow::ensure!(at>0&&at<=now,"اختر وقتًا ماضيًا أو اتركه فارغًا للحاضر");
    let hours=request["hours"].as_u64().unwrap_or(24);
    anyhow::ensure!((1..=720).contains(&hours),"نافذة العرض بين 1 و720 ساعة");
    let since=at.saturating_sub(hours*3_600_000);
    // Resolve the latest known revision BEFORE applying the window, never using future revisions.
    let mut q=db.prepare("SELECT n.source,n.observed_ms,n.payload FROM news_observations n WHERE n.observed_ms<=?1 AND n.observed_ms>=?2 AND NOT EXISTS(SELECT 1 FROM news_observations later WHERE later.source=n.source AND later.url=n.url AND later.observed_ms<=?1 AND (later.observed_ms>n.observed_ms OR (later.observed_ms=n.observed_ms AND later.id>n.id))) ORDER BY n.observed_ms DESC,n.id DESC LIMIT 201")?;
    let mut rows=q.query_map(params![at,since],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let truncated=rows.len()>200;rows.truncate(200);
    let first:Option<u64>=db.query_row("SELECT MIN(observed_ms) FROM news_observations",[],|r|r.get(0))?;
    let total:u64=db.query_row("SELECT COUNT(*) FROM news_observations",[],|r|r.get(0))?;
    let mut events=vec![];let mut text=String::new();
    let format_time=|ms:u64|chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms as i64).map(|t|t.to_rfc3339()).unwrap_or_else(||"—".into());
    let _=writeln!(text,"المتاح للنظام عند {}\nنافذة الرصد: {} ساعة | إجمالي النسخ المؤرشفة: {}",format_time(at),hours,total);
    let _=writeln!(text,"بداية الأرشيف المرصود: {}\n",first.map(format_time).unwrap_or_else(||"لا توجد أخبار مؤرشفة بعد".into()));
    for(source,observed,raw)in rows {
        let mut item:Value=serde_json::from_str(&raw)?;
        let _=writeln!(text,"{} | وصل: {}\n{}\nنشر المصدر: {}\n{}\n",source,format_time(observed),item["title"].as_str().unwrap_or(""),item["published_ms"].as_u64().map(format_time).unwrap_or_else(||"غير معروف".into()),item["url"].as_str().unwrap_or(""));
        item["source"]=json!(source);item["observed_ms"]=json!(observed);events.push(item);
    }
    if truncated{text.push_str("يُعرض أحدث 200 خبر فقط؛ ضيّق نافذة الوقت لرؤية التفاصيل المطلوبة.\n");}
    if events.is_empty(){text.push_str("لا أخبار مرصودة ضمن النافذة. هذا لا يثبت غياب أخبار السوق خارج تغطية الأرشيف.\n");}
    Ok(json!({"at_ms":at,"since_ms":since,"first_observed_ms":first,"total_revisions":total,"events":events,"truncated":truncated,"text":text,"availability_basis":"observed_ms; never publication alone","execution_effect":false}))
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn revisions_never_rewrite_the_past(){
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        let mut h=super::super::news::Headline{title:"first".into(),url:"https://www.federalreserve.gov/example".into(),published_ms:Some(1000),fetched_ms:2000,topic:"policy".into()};
        record(&db,"fed",&[h.clone()],3000).unwrap();record(&db,"fed",&[h.clone()],4000).unwrap();
        h.title="revised".into();record(&db,"fed",&[h],5000).unwrap();
        let before=replay(&db,br#"{"at":"1970-01-01T00:00:02Z"}"#,6000).unwrap();assert!(before["events"].as_array().unwrap().is_empty());
        let past=replay(&db,br#"{"at":"1970-01-01T00:00:04Z"}"#,6000).unwrap();assert_eq!(past["events"][0]["title"],"first");
        let latest=replay(&db,b"{}",6000).unwrap();assert_eq!(latest["events"][0]["title"],"revised");assert_eq!(latest["total_revisions"],2);
    }
}
