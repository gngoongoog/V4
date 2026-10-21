//! Immutable MT5 deal-based trade journal, separate from paper/shadow results.
//! Broker history is stored in broker_deals by the reconciliation path. We do
//! not invent net results for missing history or attribute orphan broker fees.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn number(v: &Value, k: &str) -> f64 { v[k].as_f64().filter(|v|v.is_finite()).unwrap_or(0.) }
fn millis(v: &Value) -> u64 { v["time_msc"].as_u64().unwrap_or(0) }
fn utc(ms:u64)->String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|d|d.format("%Y-%m-%d %H:%M:%S UTC").to_string()).unwrap_or_else(||"غير متاح".into())
}
fn kind(v: &Value) -> i64 { v["entry"].as_i64().unwrap_or(-1) }

fn originating_command(db: &Connection, context: &str, pid: u64, deal: u64) -> Result<Option<Value>> {
    let result: Option<String> = db.query_row(
        "SELECT payload FROM broker_commands WHERE context=?1 AND kind='open' \
         AND (CAST(json_extract(result,'$.position_id') AS INTEGER)=?2 \
           OR CAST(json_extract(result,'$.deal') AS INTEGER)=?3) \
         ORDER BY created_ms DESC LIMIT 1",
        params![context,pid,deal],
        |row|row.get(0),
    ).optional()?;
    result.map(|s|Ok(serde_json::from_str(&s)?)).transpose()
}
fn label(cmd: Option<&Value>, fallback_brain: &str) -> Value {
    let mode=cmd.and_then(|v|v["trading_mode"].as_str().or_else(||v["learning"]["trading_mode"].as_str()));
    let actual_brain=cmd.and_then(|v|v["signal"]["coordination"]["selection"]["brain"].as_str());
    json!({
        "owner":fallback_brain,
        "opening_brain":actual_brain.or_else(||cmd.and_then(|v|v["brain"].as_str())).unwrap_or("UNVERIFIED_OPENING_STRATEGY"),
        "trading_mode":mode.unwrap_or("unverified_historical"),
        "policy":cmd.and_then(|v|v["signal"]["coordination"]["selection"]["sniper_policy"].as_str()).unwrap_or(""),
        "reason":cmd.and_then(|v|v["signal"]["reason"].as_str()).unwrap_or("No matching durable entry request found"),
        "target_distance":cmd.and_then(|v|v["signal"]["target_distance"].as_f64()),
        "stop_distance":cmd.and_then(|v|v["signal"]["stop_distance"].as_f64()),
        "command_id":cmd.and_then(|v|v["id"].as_str()),
    })
}
/// Page through all reconciled real MT5 positions without ever mixing paper PnL.
pub fn list(db: &Connection, context: &str, telemetry: Option<&Telemetry>, offset: u32, limit: u32) -> Result<Value> {
    anyhow::ensure!((1..=100).contains(&limit) && offset <= 1_000_000, "Trade journal pagination out of bounds");
    if context.is_empty() {
        return Ok(json!({"total":0,"offset":offset,"limit":limit,"rows":[],
            "notice":"اربط حساب MT5 وحدد الرمز لقراءة سجل الصفقات المحفوظ لذلك الحساب"}));
    }
    let total: u64 = db.query_row(
        "SELECT COUNT(*) FROM (SELECT 1 FROM broker_deals WHERE context=?1 GROUP BY position_id)",
        [context], |row|row.get(0)
    )?;
    let mut stmt=db.prepare(
        "SELECT position_id FROM broker_deals WHERE context=?1 GROUP BY position_id \
         ORDER BY MAX(CAST(json_extract(payload,'$.time_msc') AS INTEGER)) DESC,position_id DESC LIMIT ?2 OFFSET ?3"
    )?;
    let pids: Vec<u64>=stmt.query_map(params![context,limit,offset],|row|row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut rows=Vec::new();
    for pid in &pids {
        let mut dstmt=db.prepare("SELECT payload,brain FROM broker_deals WHERE context=?1 AND position_id=?2 ORDER BY CAST(json_extract(payload,'$.time_msc') AS INTEGER),ticket")?;
        let deals=dstmt.query_map(params![context,pid],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut parsed=Vec::<Value>::new();
        let mut owners=BTreeSet::new();
        for (raw,owner) in deals {
            parsed.push(serde_json::from_str(&raw)?);
            owners.insert(owner);
        }
        if parsed.is_empty() {continue;}
        let opened=parsed.iter().find(|d|matches!(kind(d),0|2));
        let first=opened.unwrap_or(&parsed[0]);
        let has_exit=parsed.iter().any(|d|matches!(kind(d),1|2|3));
        let position=telemetry.and_then(|t|t.positions.iter().find(|p|p.identifier==*pid||p.ticket==*pid));
        let net: f64=parsed.iter().map(|d| number(d,"profit")+number(d,"commission")+number(d,"swap")+number(d,"fee")).sum();
        let gross: f64=parsed.iter().map(|d|number(d,"profit")).sum();
        let commission: f64=parsed.iter().map(|d|number(d,"commission")).sum();
        let fee: f64=parsed.iter().map(|d|number(d,"fee")).sum();
        let swap: f64=parsed.iter().map(|d|number(d,"swap")).sum();
        let open_volume:f64=parsed.iter().filter(|d|kind(d)==0).map(|d|number(d,"volume")).sum();
        let close_volume:f64=parsed.iter().filter(|d|matches!(kind(d),1|3)).map(|d|number(d,"volume")).sum();
        let state=if position.is_some() {if has_exit {"PARTIAL / OPEN"}else{"OPEN"}}
            else if has_exit {"CLOSED"}else{"AWAITING CLOSE HISTORY"};
        let opening_ticket=first["ticket"].as_u64().unwrap_or(0);
        let command=originating_command(db,context,*pid,opening_ticket)?;
        let owner=if owners.len()==1 {owners.iter().next().unwrap().as_str()}else{"mixed_history"};
        let origin=label(command.as_ref(),owner);
        rows.push(json!({
            "position_id":pid,"status":state,"symbol":first["symbol"],
            "account_currency":telemetry.map(|t|t.currency.as_str()).unwrap_or(""),
            "side":match first["type"].as_i64(){Some(0)=>"BUY",Some(1)=>"SELL",_=>"UNVERIFIED"},
            "opened_ms":millis(first),"opened_utc":utc(millis(first)),
            "latest_deal_ms":parsed.last().map(millis).unwrap_or(0),
            "latest_deal_utc":utc(parsed.last().map(millis).unwrap_or(0)),
            "entry_price":first["price"],
            "lots_opened":open_volume,"lots_closed":close_volume,
            "remaining_lots":position.map(|p|p.volume),
            "broker_net_booked":net,"broker_gross":gross,
            "commission":commission,"swap":swap,"fee":fee,
            "floating_broker_pnl":position.map(|p|p.profit),
            "exit_deal_seen":has_exit,"deals":parsed.len(),
            "origin":origin
        }));
    }
    // A just-opened position may not be in MT5 history yet (periodic sync).
    // Show it immediately using the broker snapshot, but NEVER invent net PnL.
    if offset==0 {
        if let Some(t)=telemetry {
            let known=BTreeSet::from_iter(pids.iter().copied());
            for p in &t.positions {
                if p.symbol!=t.symbol || known.contains(&p.identifier) || known.contains(&p.ticket) {continue;}
                let Some(brain)=braintrade_domain::execution::brain_for_magic(p.magic) else {continue;};
                // Never copy a different position's newest command merely
                // because both share the same owner magic. A broker position
                // without a synced entry deal must have an exact position-ID
                // match or remain UNVERIFIED until reconciliation catches up.
                let command=originating_command(db,context,
                    if p.identifier>0{p.identifier}else{p.ticket},0)?;
                rows.push(json!({
                    "position_id":if p.identifier>0{p.identifier}else{p.ticket},
                    "status":"OPEN / AWAITING DEAL SYNC","symbol":p.symbol,"side":p.side,
                    "opened_ms":p.opened_ms,"opened_utc":utc(p.opened_ms),"entry_price":p.price_open,
                    "remaining_lots":p.volume,"lots_opened":p.volume,
                    "broker_net_booked":Value::Null,"floating_broker_pnl":p.profit,
                    "origin":label(command.as_ref(),brain),"deals":0
                }));
            }
            rows.sort_by(|a,b| b["opened_ms"].as_u64().cmp(&a["opened_ms"].as_u64()));
        }
    }
    let closed=rows.iter().filter(|r|r["status"]=="CLOSED").count();
    Ok(json!({
        "total":total,"offset":offset,"limit":limit,"rows":rows,"closed_on_page":closed,
        "notice":"سجل MT5 الفعلي فقط؛ الصفقات المفتوحة تُعرض من الوسيط، والصفقات المغلقة من سجل الصفقات المؤكد. إجمالي الصافي يشمل profit + commission + swap + fee المتاحة المرتبطة بـ position_id؛ الرسوم المنفصلة غير المرتبطة لا تُنسب تخميناً. سجلات قديمة قبل الربط أو خارج نافذة مزامنة الوسيط قد تكون ناقصة."
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsynced_position_does_not_inherit_last_unrelated_trade_reason_or_mode() {
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let ctx="[\"Demo\",17,\"XAUUSD\",\"USD\"]";
        let other=json!({"id":"old-entry","brain":"system","kind":"open",
            "trading_mode":"expert","signal":{"reason":"UNRELATED_TRADE"}});
        db.execute("INSERT INTO broker_commands (id,context,brain,kind,created_ms,state,payload,result)
            VALUES ('old-entry',?1,'system','open',1000,'filled',?2,?3)",
            params![ctx,other.to_string(),json!({"position_id":700,"deal":701}).to_string()]).unwrap();
        let t=Telemetry{connected:true,symbol:"XAUUSD".into(),positions:vec![
            braintrade_domain::Position{magic:410_600,identifier:901,ticket:902,symbol:"XAUUSD".into(),side:"BUY".into(),
                volume:0.1,price_open:2000.,price_current:2001.,profit:1.,..Default::default()}
        ],..Default::default()};
        let rows=list(&db,ctx,Some(&t),0,50).unwrap();
        assert_eq!(rows["rows"][0]["origin"]["trading_mode"],"unverified_historical");
        assert_eq!(rows["rows"][0]["origin"]["opening_brain"],"UNVERIFIED_OPENING_STRATEGY");
        assert_ne!(rows["rows"][0]["origin"]["reason"],"UNRELATED_TRADE");
        assert_eq!(rows["rows"][0]["origin"]["owner"],"system");
    }

    #[test]
    fn durable_closed_trade_keeps_brain_mode_and_net_fees_after_restart() {
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let ctx="[\"Demo\",17,\"XAUUSD\",\"USD\"]";
        let cmd=json!({"id":"order1","context":ctx,"kind":"open","brain":"system",
            "trading_mode":"sniper","signal":{"reason":"quote impulse","coordination":{"selection":{"brain":"sniper_learner","sniper_policy":"impulse"}}}});
        db.execute("INSERT INTO broker_commands (id,context,brain,kind,created_ms,state,payload,result) VALUES ('order1',?1,'system','open',1000,'observed',?2,?3)",
            params![ctx,cmd.to_string(),json!({"deal":11,"position_id":99}).to_string()]).unwrap();
        for (ticket,entry,profit,commission,time) in [(11,0,0.,-0.35,1100),(12,1,2.,-0.35,2200)] {
            let d=json!({"ticket":ticket,"position_id":99,"symbol":"XAUUSD","type":0,"entry":entry,"time_msc":time,
                "volume":0.1,"price":2000.,"profit":profit,"commission":commission,"fee":0.,"swap":0.});
            db.execute("INSERT INTO broker_deals VALUES (?1,?2,99,'system',?3)",params![ctx,ticket,d.to_string()]).unwrap();
        }
        let v=list(&db,ctx,None,0,100).unwrap();
        assert_eq!(v["total"],1);
        assert_eq!(v["rows"][0]["status"],"CLOSED");
        assert!((v["rows"][0]["broker_net_booked"].as_f64().unwrap()-1.3).abs()<1e-9);
        assert_eq!(v["rows"][0]["origin"]["trading_mode"],"sniper");
        assert_eq!(v["rows"][0]["origin"]["opening_brain"],"sniper_learner");
        let again=list(&db,ctx,None,0,100).unwrap();
        assert_eq!(again["rows"][0]["origin"]["policy"],"impulse");
    }
}
