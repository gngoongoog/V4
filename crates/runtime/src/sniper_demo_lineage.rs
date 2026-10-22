//! Read-only exact-command lineage from the EXISTING Demo Sniper sampled decision
//! through the original Core broker intent to a reconciled, CLOSED broker position.
//! Not a counterfactual: blocked and unready samples cannot have broker PnL.
use anyhow::Result;
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use std::collections::{BTreeMap,BTreeSet};

const LIMIT:i64=600;
fn finite(x:&Value)->Option<f64>{
    x.as_f64().filter(|n|n.is_finite()&&n.abs()<=1e11)
}
fn positive(x:&Value)->Option<f64>{
    finite(x).filter(|x|*x>0.)
}
fn context_closed(sample:&Value,position:u64,cmd_id:&str,closed_ms:u64)->bool{
    sample["evidence_source"]=="mt5_demo_broker"
       && sample["brain"]=="sniper_learner"
       && sample["trading_mode"]=="sniper"
       && sample["open_command_id"]==cmd_id
       && sample["position_id"].as_u64()==Some(position)
       && sample["closed_ms"].as_u64()==Some(closed_ms)
       && sample["selection"]["brain"]=="sniper_learner"
       && sample["selection"]["policy"]=="sniper-ticks-v1"
       && positive(&sample["closed_volume"]).is_some()
       && [ "gross","commission","swap","fee","net"].iter().all(|k|finite(&sample[k]).is_some())
       && {let calculated=["gross","commission","swap","fee"].iter()
            .map(|k|finite(&sample[k]).unwrap()).sum::<f64>();
           let net=finite(&sample["net"]).unwrap();
           calculated.is_finite() && (calculated-net).abs()<=1e-6_f64.max(net.abs()*1e-8)}
}
fn code_label(k:&str)->&'static str{
    match k{
        "not_ready"=>"شروط القناص غير مكتملة",
        "ready_blocked"=>"الإشارة جاهزة لكن ممنوعة",
        "eligible_without_command"=>"اجتازت الشروط لكن بدون أمر مربوط",
        "missing_or_wrong_command"=>"رابط الأمر غير موجود أو لا يطابق التقييم",
        "command_unsettled"=>"أمر مربوط لكن دون نتيجة مغلقة",
        "broker_rejected_or_cancelled"=>"الأمر رُفض أو أُلغي قبل إثبات تنفيذ",
        "closed_demo_exact"=>"صفقة Demo مغلقة ومصالحة برقم الأمر",
        "contradictory_closed_evidence"=>"دليل الإغلاق متعارض أو مكرر",
        _=>"حالة غير معروفة",
    }
}
pub fn report(db:&Connection,context:&str)->Result<Value>{
    if context.is_empty(){
        return Ok(json!({"status":"unbound","sampled_decisions":0,"execution_effect":false}));
    }
    let mut stmt=db.prepare("SELECT observed_ms,payload FROM sniper_opportunity_samples
         WHERE context=?1 ORDER BY observed_ms DESC LIMIT ?2")?;
    let rows=stmt.query_map(params![context,LIMIT],
        |r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut count=BTreeMap::<String,u64>::new();
    let mut examples=Vec::<Value>::new();
    let mut seen_cmd=BTreeSet::<String>::new();
    let mut malformed=0u64;
    let mut closed=0u64;
    let mut net_total=0.;
    let mut wins=0u64;
    for (stamp,raw) in rows.iter(){
        let Some(obs)=serde_json::from_str::<Value>(raw).ok().filter(|o|o.is_object()) else{
            malformed+=1;continue;
        };
        let Some(ready)=obs["signal_ready"].as_bool() else {malformed+=1;continue;};
        let Some(eligible)=obs["entry_ready"].as_bool() else{malformed+=1;continue;};
        if obs["observed_ms"].as_u64()!=Some(*stamp)||eligible&&!ready {
            malformed+=1;continue;
        }
        let category=if !ready{"not_ready"}
        else if !eligible{"ready_blocked"}
        else {
            let id=obs["command_id"].as_str().filter(|s|!s.is_empty()&&s.len()<=128);
            match id {
                None=>"eligible_without_command",
                Some(id)=>{
                    if !seen_cmd.insert(id.to_owned()){
                        "contradictory_closed_evidence"
                    }else{
                        let entry:Option<(String,String,String,String,String,u64)>=db.query_row(
                            "SELECT id,state,payload,result,brain,created_ms FROM broker_commands
                             WHERE context=?1 AND id=?2 AND kind='open'",
                            params![context,id],|r|Ok((
                                r.get(0)?,r.get(1)?,r.get(2)?,
                                r.get(3)?,r.get(4)?,r.get(5)?,
                            ))).optional()?;
                        match entry {
                            None=>"missing_or_wrong_command",
                            Some((_,state,payload,result,brain,created))=>{
                                let cmd=serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null);
                                let result=serde_json::from_str::<Value>(&result).unwrap_or(Value::Null);
                                let valid=brain=="system"&&created==*stamp
                                    && cmd["trading_mode"]=="sniper"
                                    && cmd["target"]=="demo"
                                    && cmd["signal"]["key"]==obs["signal_key"]
                                    && cmd["signal"]["coordination"]["selection"]["brain"]=="sniper_learner"
                                    && cmd["signal"]["coordination"]["selection"]["policy"]=="sniper-ticks-v1";
                                if !valid{"missing_or_wrong_command"}
                                else {
                                    let mut statement=db.prepare("SELECT position_id,closed_ms,payload FROM coordinator_broker_samples
                                          WHERE context=?1 AND brain='sniper_learner'
                                            AND json_extract(payload,'$.open_command_id')=?2
                                          LIMIT 2")?;
                                    let matches=statement.query_map(params![context,id],|r|Ok((
                                        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,
                                    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
                                    if matches.len()>1{"contradictory_closed_evidence"}
                                    else if let Some((position,time,p))=matches.first(){
                                        let evidence=serde_json::from_str::<Value>(p).unwrap_or(Value::Null);
                                        if !context_closed(&evidence,*position,id,*time)
                                            || !matches!(state.as_str(),"observed"|"filled"|"partial"|"placed")
                                            || result["deal"].as_u64().unwrap_or(0)==0
                                            || evidence["open_deal"]!=result["deal"]
                                            || evidence["executed_side"]!=cmd["signal"]["side"]
                                            || evidence["selection"]["sniper_policy"]!=cmd["signal"]["coordination"]["selection"]["sniper_policy"]
                                            || evidence["selection"]["sniper_definition"]!=cmd["signal"]["coordination"]["selection"]["sniper_definition"]
                                        {"contradictory_closed_evidence"}else{
                                            let net=finite(&evidence["net"]).unwrap();
                                            closed+=1;net_total+=net;wins+=u64::from(net>0.);
                                            "closed_demo_exact"
                                        }
                                    } else if matches!(state.as_str(),"cancelled"|"rejected"|"expired"){
                                        "broker_rejected_or_cancelled"
                                    }else{
                                        "command_unsettled"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
        *count.entry(category.to_string()).or_default()+=1;
        if examples.len()<12 {
            examples.push(json!({"observed_ms":stamp,"category":category,
                "label_ar":code_label(category),"command_id":obs["command_id"],
                "source":"read_only_core_sample"}));
        }
    }
    Ok(json!({
        "policy":"sniper_exact_demo_command_outcome_v1",
        "status":if rows.len()<30{"insufficient_sampled_decisions"}else{"descriptive_lineage_only"},
        "source":"3F-U sampled decisions + broker_commands + reconciled coordinator_broker_samples",
        "account_symbol_scoped":true,"demo_only":true,
        "rows_scanned":rows.len(),"malformed_rows":malformed,
        "sampled_decisions":rows.len() as u64-malformed,
        "counts":count,"recent":examples,
        "exact_closed_demo_positions":closed,
        "closed_demo_net_after_booked_costs":if closed>0{Some(net_total)}else{None},
        "closed_demo_wins":wins,
        "closed_demo_win_rate":if closed>0{Some(wins as f64/closed as f64)}else{None},
        "no_counterfactual_broker_pnl_for_blocked":true,
        "hypothetical_price_study_not_broker_execution":true,
        "incomplete_or_unknown_commands_not_zero_profit":true,
        "profitability_or_policy_promotion_verified":false,
        "learning_effect":false,"execution_effect":false,
        "limitations":"Exact logged command identity and completed broker Demo reconciliation only. Sampled 1Hz snapshots can miss actual Fast Pulse entries; no universal coverage. Entry-eligible snapshots and hypothetical five-second price outcomes are not broker fills. A closed outcome is attributed once per exact command and exact context, but does not prove independent OOS strategy performance."
    }))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn d()->Connection{
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE sniper_opportunity_samples(context TEXT,observed_ms INTEGER,pending INTEGER,payload TEXT,PRIMARY KEY(context,observed_ms));
        CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
        CREATE TABLE coordinator_broker_samples(source TEXT,position_id INTEGER,context TEXT,brain TEXT,closed_ms INTEGER,payload TEXT);").unwrap();
        db
    }
    fn decision(db:&Connection,ctx:&str,time:u64,ready:bool,eligible:bool,id:Option<&str>){
        let p=json!({"observed_ms":time,"signal_ready":ready,"entry_ready":eligible,
            "command_id":id,"signal_key":format!("tick:{time}")});
        db.execute("INSERT INTO sniper_opportunity_samples VALUES(?1,?2,0,?3)",
            params![ctx,time,p.to_string()]).unwrap();
    }
    fn command(db:&Connection,ctx:&str,time:u64,id:&str,mode:&str,deal:u64){
        let payload=json!({"target":"demo","trading_mode":mode,
            "signal":{"key":format!("tick:{time}"),"side":1,
                "coordination":{"selection":{"brain":"sniper_learner",
                    "policy":"sniper-ticks-v1","sniper_policy":"impulse",
                    "sniper_definition":"rev1"}}}});
        db.execute("INSERT INTO broker_commands VALUES(?1,?2,'system','open',?3,'observed',?4,?5)",
            params![id,ctx,time,payload.to_string(),json!({"deal":deal}).to_string()]).unwrap();
    }
    fn closed(db:&Connection,ctx:&str,pos:u64,id:&str,net:f64){
        let p=json!({"evidence_source":"mt5_demo_broker","brain":"sniper_learner",
            "trading_mode":"sniper","open_command_id":id,"open_deal":55,
            "position_id":pos,"closed_ms":10000,"closed_volume":0.01,
            "executed_side":1,"gross":net+0.07,
            "commission":-0.07,"swap":0.0,"fee":0.0,"net":net,
            "selection":{"brain":"sniper_learner","policy":"sniper-ticks-v1",
                "sniper_policy":"impulse","sniper_definition":"rev1"}});
        db.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,?3,'sniper_learner',10000,?4)",
            params![format!("source:{pos}"),pos,ctx,p.to_string()]).unwrap();
    }
    #[test]
    fn close_demo_exact_chain_is_not_a_counterfactual_for_rejections(){
        let db=d();
        decision(&db,"a",5000,true,true,Some("cmd"));
        command(&db,"a",5000,"cmd","sniper",55);
        closed(&db,"a",900,"cmd",1.5);
        decision(&db,"a",5001,true,false,None);
        decision(&db,"a",5002,false,false,None);
        let r=report(&db,"a").unwrap();
        assert_eq!(r["counts"]["closed_demo_exact"],1);
        assert_eq!(r["counts"]["ready_blocked"],1);
        assert_eq!(r["counts"]["not_ready"],1);
        assert_eq!(r["exact_closed_demo_positions"],1);
        assert_eq!(r["closed_demo_net_after_booked_costs"],1.5);
        assert_eq!(r["closed_demo_win_rate"],1.);
        assert_eq!(r["no_counterfactual_broker_pnl_for_blocked"],true);
        assert_eq!(r["learning_effect"],false);
    }
    #[test]
    fn other_context_and_wrong_mode_never_match(){
        let db=d();
        decision(&db,"a",5000,true,true,Some("cmd"));
        command(&db,"b",5000,"cmd","sniper",55);
        closed(&db,"b",900,"cmd",100.);
        assert_eq!(report(&db,"a").unwrap()["counts"]["missing_or_wrong_command"],1);
        decision(&db,"a",6000,true,true,Some("wrongmode"));
        command(&db,"a",6000,"wrongmode","expert",55);
        assert_eq!(report(&db,"a").unwrap()["counts"]["missing_or_wrong_command"],2);
    }
    #[test]
    fn contradictory_or_duplicate_closed_samples_fail_closed(){
        let db=d();
        decision(&db,"a",5000,true,true,Some("cmd"));
        command(&db,"a",5000,"cmd","sniper",55);
        closed(&db,"a",900,"cmd",0.5);
        closed(&db,"a",901,"cmd",0.5);
        let r=report(&db,"a").unwrap();
        assert_eq!(r["counts"]["contradictory_closed_evidence"],1);
        assert_eq!(r["exact_closed_demo_positions"],0);
    }
    #[test]
    fn eligible_without_exact_link_does_not_make_up_a_fill(){
        let db=d();
        decision(&db,"a",5000,true,true,None);
        assert_eq!(report(&db,"a").unwrap()["counts"]["eligible_without_command"],1);
        decision(&db,"a",6000,true,true,Some("new"));
        command(&db,"a",6000,"new","sniper",55);
        assert_eq!(report(&db,"a").unwrap()["counts"]["command_unsettled"],1);
    }
}
