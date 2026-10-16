//! Bounded multi-symbol research reports; never a broker-execution gate.
use anyhow::{bail, Context, Result};
use braintrade_domain::Status;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::Write;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings { enabled:bool, symbols:Vec<String>, timeframe:String, revision:u64 }
impl Default for Settings {
    fn default()->Self{Self{enabled:true,symbols:vec!["EURUSD".into(),"USDJPY".into()],timeframe:"M5".into(),revision:0}}
}
pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS context_settings(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);CREATE TABLE IF NOT EXISTS correlation_reports(context TEXT PRIMARY KEY,payload TEXT NOT NULL);")?;Ok(())
}
fn settings(db:&Connection)->Result<Settings>{
    let raw:Option<String>=db.query_row("SELECT payload FROM context_settings WHERE id=1",[],|r|r.get(0)).optional()?;
    Ok(raw.map(|s|serde_json::from_str(&s)).transpose()?.unwrap_or_default())
}
fn symbols(s:&Settings,primary:&str)->Vec<String>{
    let mut out=vec![primary.to_owned()];for symbol in &s.symbols{if !out.contains(symbol){out.push(symbol.clone());}}out
}
pub fn route(method:&str,path:&str,body:&[u8],db:&Connection,status:&Status,now:u64)->Result<Value>{
    let t=status.telemetry.as_ref().filter(|t|t.connected&&status.gateway_fresh);
    let s=settings(db)?;
    match(method,path){
        ("POST","/v1/research-desk/settings")=>{
            let mut next:Settings=serde_json::from_slice(body)?;
            anyhow::ensure!(next.symbols.len()<=5&&next.symbols.iter().all(|v|!v.is_empty()&&v.len()<=64&&!v.chars().any(char::is_control)),"أدخل حتى خمسة رموز إضافية بأسمائها لدى الوسيط");
            anyhow::ensure!(["M1","M5","M15","H1"].contains(&next.timeframe.as_str()),"Unsupported correlation timeframe");
            next.symbols.sort();next.symbols.dedup();next.revision=s.revision+1;
            db.execute("INSERT OR REPLACE INTO context_settings VALUES(1,?1)",[serde_json::to_string(&next)?])?;
            Ok(json!({"ok":true,"settings":next}))
        },
        ("GET","/v1/gateway/context/request")=>{
            let job=t.filter(|_|s.enabled).map(|t|json!({"login":t.login,"server":t.server,"currency":t.currency,"symbol":t.symbol,"symbols":symbols(&s,&t.symbol),"timeframe":s.timeframe,"revision":s.revision}));
            Ok(json!({"job":job}))
        },
        ("POST","/v1/gateway/context/report")=>{
            let t=t.context("Current MT5 account required")?;let v:Value=serde_json::from_slice(body)?;
            anyhow::ensure!(s.enabled&&v["revision"]==s.revision&&v["login"]==t.login&&v["server"]==t.server&&v["currency"]==t.currency&&v["symbol"]==t.symbol&&v["timeframe"]==s.timeframe,"Account or correlation settings changed");
            let at=v["captured_ms"].as_u64().context("Capture time required")?;
            anyhow::ensure!(at<=now+2000&&now.saturating_sub(at)<=30_000&&v["window"]==240,"Invalid correlation window or stale read");
            let rows=v["series"].as_array().context("Series required")?;let wanted=symbols(&s,&t.symbol);
            anyhow::ensure!(rows.len()==wanted.len()&&rows.iter().zip(&wanted).all(|(r,w)|r["symbol"]==*w&&r["returns"].as_array().is_some_and(|a|a.len()==240&&a.iter().all(|x|x.is_null()||x.as_f64().is_some_and(f64::is_finite)))),"Invalid aligned series");
            let pairs=v["pairs"].as_array().context("Pairs required")?;
            anyhow::ensure!(pairs.len()==rows.len()*(rows.len()-1)/2&&pairs.iter().all(|p|p["samples"].as_u64().is_some_and(|n|n<=240)&&p["mask"].as_array().is_some_and(|m|m.len()==240&&m.iter().all(Value::is_boolean))&&(p["correlation"].is_null()||p["correlation"].as_f64().is_some_and(|r|r.is_finite()&&(-1.0..=1.0).contains(&r)))),"Invalid pair statistics");
            let key=crate::coordinator_learning::context(t);
            db.execute("INSERT OR REPLACE INTO correlation_reports VALUES(?1,?2)",params![key,v.to_string()])?;
            Ok(json!({"ok":true}))
        },
        ("GET","/v1/research-desk/correlation")=>{
            let key=t.map(crate::coordinator_learning::context).unwrap_or_default();
            let raw:Option<String>=db.query_row("SELECT payload FROM correlation_reports WHERE context=?1",[key],|r|r.get(0)).optional()?;
            let report:Value=raw.map(|v|serde_json::from_str(&v)).transpose()?.unwrap_or(Value::Null);
            let current=s.enabled&&t.is_some()&&report["revision"]==s.revision&&report["captured_ms"].as_u64().is_some_and(|at|at<=now+2000&&now.saturating_sub(at)<=120_000);
            let mut text=String::new();
            let _=writeln!(text,"{} | {} | 240 فترة مكتملة\n",if current{"قراءة حديثة"}else{"بانتظار قراءة حديثة؛ المعروض إن وجد محفوظ"},s.timeframe);
            for row in report["series"].as_array().into_iter().flatten(){let _=writeln!(text,"{}: {} / 240 عائد متاح — {}",row["symbol"].as_str().unwrap_or(""),row["available"],row["error"].as_str().unwrap_or(""));}
            text.push('\n');
            for p in report["pairs"].as_array().into_iter().flatten(){let r=p["correlation"].as_f64().map(|v|format!("{v:+.3}")).unwrap_or_else(||"غير متاح".into());let _=writeln!(text,"{} ↔ {} | ارتباط {} | {} عينة متزامنة",p["left"].as_str().unwrap_or(""),p["right"].as_str().unwrap_or(""),r,p["samples"]);}
            text.push_str("\nتعرض صفقات العقول حسب الرمز (اللوت غير قابل للجمع المالي بين الرموز):\n");
            for e in report["exposure"].as_array().into_iter().flatten(){let _=writeln!(text,"{} | صفقات {} | إجمالي لوت {} | صافي اتجاهي {} | {}",e["symbol"].as_str().unwrap_or(""),e["positions"],e["gross_lots"],e["net_lots"],if e["known"]==true{"مرصود"}else{"غير متاح"});}
            Ok(json!({"settings":s,"current":current,"report":report,"text":text,"execution_effect":false}))
        },
        _=>bail!("Unknown research desk route")
    }
}
