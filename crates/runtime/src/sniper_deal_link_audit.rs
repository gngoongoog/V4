//! Read-only cross-check of Sniper gateway acknowledgements against the
//! Core's *synced* MT5 broker_deals table. No polling, MT5 API or order changes.
//! A deal ticket in an acknowledgement is NOT enough to infer a booked fill.
use anyhow::Result;
use rusqlite::{params,Connection};
use serde_json::{json,Value};
use std::collections::BTreeMap;

const LIMIT:i64=200;
#[derive(Default)]
struct Counts {
    exact:u64,
    awaiting:u64,
    no_ack_ticket:u64,
    conflicts:u64,
    unsupported:u64,
}
fn valid_positive(v:&Value)->Option<f64>{
    v.as_f64().filter(|x|x.is_finite()&&*x>0.&&*x<=1e7)
}
fn compatible(a:f64,b:f64)->bool {
    (a-b).abs()<=1e-8_f64.max(a.abs().max(b.abs())*1e-7)
}
fn broker_entry(row:&Value)->bool {
    row["entry"].as_i64()==Some(0)
}
fn classify(result:&Value, history:&[Value])->(&'static str,&'static str){
    let Some(ticket)=result["deal"].as_u64().filter(|d|*d>0) else{
        return ("no_deal_ticket_in_ack","لم يرجع رقم Deal صالح من الوسيط؛ لا توجد تعبئة مؤكدة عبر هذا الربط");
    };
    if history.is_empty(){
        return ("awaiting_synced_history","رقم Deal في الرد موجود، لكن سجل الوسيط المتزامن لا يحتويه بعد");
    }
    if history.len()!=1 {
        return ("conflicting_history","أكثر من سجل بنفس رقم Deal؛ لا يمكن اعتماد المطابقة");
    }
    let record=&history[0];
    if record["ticket"].as_u64()!=Some(ticket)
        || !broker_entry(record)
        || record["time_msc"].as_u64().unwrap_or(0)==0
        || record["position_id"].as_u64().unwrap_or(0)==0
        || valid_positive(&record["volume"]).is_none()
        || valid_positive(&record["price"]).is_none(){
        return ("unsupported_or_incomplete_deal","سجل التنفيذ غير مكتمل أو ليس دخولاً بسيطاً؛ لا نستنتج تعبئة");
    }
    if let (Some(request_type),Some(deal_type))=
        (result["request"]["type"].as_i64(),record["type"].as_i64()){
        if request_type!=deal_type{
            return ("conflicting_history","اتجاه Deal لا يطابق اتجاه أمر MT5");
        }
    }
    if let (Some(symbol),Some(actual))=
        (result["request"]["symbol"].as_str(),record["symbol"].as_str()){
        if symbol!=actual{
            return ("conflicting_history","رمز سجل Deal يختلف عن الأمر");
        }
    }
    if let Some(requested)=valid_positive(&result["request"]["volume"]){
        let observed=valid_positive(&record["volume"]).unwrap();
        if observed>requested+1e-8{
            return ("conflicting_history","حجم Deal أكبر من الحجم المطلوب في الأمر");
        }
    }
    if let Some(supplied)=result["position_id"].as_u64().filter(|v|*v>0){
        if record["position_id"].as_u64()!=Some(supplied){
            return ("conflicting_history","معرّف المركز يتعارض مع رد الأمر");
        }
    }
    ("exact_ticket_seen_in_synced_history","رقم Deal نفسه موجود بسجل MT5 المتزامن وله دخول واضح؛ ما زال التحقق من اكتمال المركز مستقلاً")
}
pub fn audit(db:&Connection,context:&str)->Result<Value>{
    if context.is_empty(){
        return Ok(json!({"status":"unbound","sampled_commands":0,
            "broker_history_verified_complete":false,"broker_effect":false}));
    }
    let mut statement=db.prepare(
        "SELECT id,state,result FROM broker_commands
         WHERE context=?1 AND kind='open'
           AND json_extract(payload,'$.trading_mode')='sniper'
         ORDER BY created_ms DESC,id DESC LIMIT ?2"
    )?;
    let commands=statement.query_map(params![context,LIMIT],|row|Ok((
        row.get::<_,String>(0)?,
        row.get::<_,String>(1)?,
        row.get::<_,Option<String>>(2)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut totals=Counts::default();
    let mut rows=Vec::new();
    let mut by_state=BTreeMap::<String,u64>::new();
    for (id,state,raw) in &commands{
        *by_state.entry(state.clone()).or_default()+=1;
        let value=raw.as_deref().and_then(|s|serde_json::from_str::<Value>(s).ok())
            .filter(|v|v.is_object()).unwrap_or(Value::Null);
        let ticket=value["deal"].as_u64().filter(|d|*d>0);
        let history=if let Some(deal_id)=ticket{
            let mut q=db.prepare(
                "SELECT payload FROM broker_deals WHERE context=?1 AND ticket=?2 LIMIT 2"
            )?;
            q.query_map(params![context,deal_id],|r|r.get::<_,String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter().filter_map(|raw|serde_json::from_str(&raw).ok())
                .collect::<Vec<Value>>()
        }else{Vec::new()};
        let (quality,note)=classify(&value,&history);
        match quality {
            "exact_ticket_seen_in_synced_history"=>totals.exact+=1,
            "awaiting_synced_history"=>totals.awaiting+=1,
            "no_deal_ticket_in_ack"=>totals.no_ack_ticket+=1,
            "conflicting_history"=>totals.conflicts+=1,
            _=>totals.unsupported+=1,
        }
        if rows.len()<20 {
            let actual=history.first();
            rows.push(json!({
                "command_id":id,
                "command_state":state,
                "ack_retcode":value["retcode"],
                "ack_deal_ticket":ticket,
                "matched_broker_deal_ticket":if quality=="exact_ticket_seen_in_synced_history"{ticket}else{None},
                "quality":quality,
                "detail_ar":note,
                "broker_position_id":if quality=="exact_ticket_seen_in_synced_history"{
                    actual.map(|a|a["position_id"].clone()).unwrap_or(Value::Null)
                }else{Value::Null},
                "broker_deal_time_msc":if quality=="exact_ticket_seen_in_synced_history"{
                    actual.map(|a|a["time_msc"].clone()).unwrap_or(Value::Null)
                }else{Value::Null},
                "local_acknowledged_ms":value["acknowledged_ms"],
                "entry_volume_from_synced_deal":if quality=="exact_ticket_seen_in_synced_history"{
                    actual.map(|a|a["volume"].clone()).unwrap_or(Value::Null)
                }else{Value::Null},
                "computed_tick_to_fill_latency_ms":Value::Null
            }));
        }
    }
    Ok(json!({
        "status":if commands.is_empty(){"no_sniper_commands"}
            else{"partial_broker_evidence_audit"},
        "source":"local_core_broker_commands_and_synced_broker_deals",
        "policy":"sniper_ack_deal_ticket_link_v1",
        "account_scoped":true,
        "mode_scoped":"sniper",
        "recent_command_limit":LIMIT,
        "sampled_commands":commands.len(),
        "state_counts":by_state,
        "exact_synced_entry_deal_ticket_links":totals.exact,
        "awaiting_synced_deal_history":totals.awaiting,
        "no_ack_deal_ticket":totals.no_ack_ticket,
        "contradictory_deal_history":totals.conflicts,
        "unsupported_or_incomplete_deals":totals.unsupported,
        "recent_entries":rows,
        "broker_history_verified_complete":false,
        "broker_deal_fill_volume_confirmed_for_whole_order":false,
        "computed_tick_to_fill_latency_ms":Value::Null,
        "broker_effect":false,
        "learning_effect":false,
        "caveat":"Only exact deal-ticket links in the same account/symbol scope count. A single linked entry deal can be partial; full position/exit completeness requires independent reconciliation. Local acknowledged_ms and broker deal time_msc are separate clock domains and cannot be subtracted to assert latency; no request-to-fill or strategy profitability proof."
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn db()->Connection {
        let d=Connection::open_in_memory().unwrap();
        d.execute_batch("CREATE TABLE broker_commands(
            id TEXT PRIMARY KEY,context TEXT,kind TEXT,created_ms INTEGER,
            state TEXT,payload TEXT,result TEXT);
            CREATE TABLE broker_deals(
            context TEXT,ticket INTEGER,position_id INTEGER,brain TEXT,payload TEXT);").unwrap();
        d
    }
    fn command(db:&Connection,id:&str,context:&str,deal:Value){
        let result=json!({"deal":deal,"retcode":10009,"acknowledged_ms":5000,
            "request":{"symbol":"XAUUSD","volume":0.04,"type":0}});
        db.execute("INSERT INTO broker_commands VALUES(?1,?2,'open',1000,'filled',?3,?4)",
            params![id,context,json!({"trading_mode":"sniper"}).to_string(),
                result.to_string()]).unwrap();
    }
    fn add_deal(db:&Connection,context:&str,ticket:u64,volume:f64,entry:i64,direction:i64){
        let payload=json!({"ticket":ticket,"position_id":99,"symbol":"XAUUSD",
            "time_msc":5001,"entry":entry,"type":direction,"price":5000.2,
            "volume":volume});
        db.execute("INSERT INTO broker_deals VALUES(?1,?2,99,'system',?3)",
            params![context,ticket,payload.to_string()]).unwrap();
    }
    #[test]
    fn exact_ticket_from_same_context_is_linked_but_not_a_full_fill_claim(){
        let db=db();
        command(&db,"s","account-a",json!(70));
        add_deal(&db,"account-a",70,0.02,0,0);
        let r=audit(&db,"account-a").unwrap();
        assert_eq!(r["exact_synced_entry_deal_ticket_links"],1);
        assert_eq!(r["broker_deal_fill_volume_confirmed_for_whole_order"],false);
        assert_eq!(r["recent_entries"][0]["entry_volume_from_synced_deal"],0.02);
        assert_eq!(r["computed_tick_to_fill_latency_ms"],Value::Null);
    }
    #[test]
    fn other_context_ticket_and_unobserved_acks_never_fake_a_deal(){
        let db=db();
        command(&db,"s","account-a",json!(70));
        add_deal(&db,"account-b",70,0.04,0,0);
        command(&db,"no-ticket","account-a",Value::Null);
        let r=audit(&db,"account-a").unwrap();
        assert_eq!(r["exact_synced_entry_deal_ticket_links"],0);
        assert_eq!(r["awaiting_synced_deal_history"],1);
        assert_eq!(r["no_ack_deal_ticket"],1);
    }
    #[test]
    fn wrong_side_and_excessive_volume_are_conflicts(){
        let db=db();
        command(&db,"wrong-side","a",json!(72));
        add_deal(&db,"a",72,0.04,0,1);
        command(&db,"overfill","a",json!(73));
        add_deal(&db,"a",73,0.05,0,0);
        let r=audit(&db,"a").unwrap();
        assert_eq!(r["contradictory_deal_history"],2);
    }
    #[test]
    fn close_deal_is_not_proof_of_open_fill_and_missing_data_fail_closed(){
        let db=db();
        command(&db,"closed","a",json!(72));
        add_deal(&db,"a",72,0.04,1,0);
        command(&db,"missing","a",json!(73));
        let malformed=json!({"ticket":73,"position_id":99,"entry":0,
            "type":0,"time_msc":0,"price":5000.,"volume":0.04});
        db.execute("INSERT INTO broker_deals VALUES('a',73,99,'system',?1)",
            [malformed.to_string()]).unwrap();
        let r=audit(&db,"a").unwrap();
        assert_eq!(r["unsupported_or_incomplete_deals"],2);
        assert_eq!(r["exact_synced_entry_deal_ticket_links"],0);
    }
    #[test]
    fn two_synced_records_for_same_ticket_remain_conflicted(){
        let db=db();
        command(&db,"dup","a",json!(72));
        add_deal(&db,"a",72,0.04,0,0);
        add_deal(&db,"a",72,0.04,0,0);
        let r=audit(&db,"a").unwrap();
        assert_eq!(r["contradictory_deal_history"],1);
    }
}
