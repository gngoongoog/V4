//! Prospective leave-one-member-out diagnostics. Frozen votes, shared future horizon, no execution effect.
use anyhow::Result;
use braintrade_domain::{studio::FrameBar, trading::TradingMode, Config, Telemetry};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fmt::Write};

pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS ablation_samples(context TEXT NOT NULL,bar INTEGER NOT NULL,payload TEXT NOT NULL,outcome TEXT,PRIMARY KEY(context,bar));CREATE INDEX IF NOT EXISTS ablation_pending ON ablation_samples(context,outcome);")?;Ok(())
}
fn direction(members:&[Value],excluded:Option<&str>)->i8 {
    let(mut buy,mut sell)=(0.,0.);
    for member in members {
        if member["ready"]!=true||member["id"].as_str()==excluded {continue;}
        let weight=member["weight"].as_f64().unwrap_or(1.);
        match member["side"].as_i64(){Some(1)=>buy+=weight,Some(-1)=>sell+=weight,_=>{}}
    }
    if buy+sell==0. {return 0;}
    let net=(buy-sell)/(buy+sell);if net>=0.2{1}else if net<= -0.2{-1}else{0}
}
fn score(side:i8,p:&Value,exit:f64,spread:f64)->f64 {
    if side==0{return 0.;}
    let slip=p["slippage"].as_f64().unwrap();
    let entry=if side>0{p["ask"].as_f64().unwrap()+slip}else{p["bid"].as_f64().unwrap()-slip};
    let exit=exit+if side<0{spread}else{0.}-side as f64*slip;
    let change=(exit-entry)*side as f64;
    change/p["tick"].as_f64().unwrap()*p[if change>=0.{"profit"}else{"loss"}].as_f64().unwrap()-p["commission"].as_f64().unwrap()
}
pub fn observe(db:&Connection,t:&Telemetry,c:&Config,run:bool,evidence:Option<&Value>,frames:&BTreeMap<String,Vec<FrameBar>>,now:u64)->Result<()> {
    if !t.connected{return Ok(());}
    let Some(bars)=frames.get("M1").filter(|b|b.len()>=100)else{return Ok(());};
    let key=crate::coordinator_learning::context(t);
    let mut query=db.prepare("SELECT bar,payload FROM ablation_samples WHERE context=?1 AND outcome IS NULL ORDER BY bar LIMIT 2000")?;
    let pending=query.query_map([&key],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for(bar,raw)in pending {
        let p:Value=serde_json::from_str(&raw)?;
        let future:Vec<_>=bars.iter().filter(|b|b.candle.time>bar).take(12).collect();
        let result=if bars[0].candle.time>bar {Some(json!({"state":"expired","reason":"Entry window no longer retained"}))}
        else if future.len()==12 {
            if future.iter().enumerate().any(|(i,b)|b.candle.time!=bar+(i as u64+1)*60)||future[11].candle.time+60>now/1000 {
                Some(json!({"state":"expired","reason":"Missing or unfinished forward bars"}))
            }else{
                let last=future[11];
                let spread=if last.spread_points>0{last.spread_points as f64*p["point"].as_f64().unwrap()}else{p["ask"].as_f64().unwrap()-p["bid"].as_f64().unwrap()};
                let variants=p["variants"].as_array().unwrap().iter().map(|v|{
                    let side=v["side"].as_i64().unwrap() as i8;
                    json!({"excluded":v["excluded"],"side":side,"net_per_lot":score(side,&p,last.candle.close,spread)})
                }).collect::<Vec<_>>();
                Some(json!({"state":"scored","scored_ms":now,"exit_bar":last.candle.time,"variants":variants}))
            }
        }else{None};
        if let Some(result)=result{db.execute("UPDATE ablation_samples SET outcome=?3 WHERE context=?1 AND bar=?2 AND outcome IS NULL",params![key,bar,result.to_string()])?;}
    }
    if !run||c.trading.mode!=TradingMode::Expert||t.tick_age_ms.is_none_or(|v|v>3000)||t.quote_time_ms.is_none_or(|v|v>now+2000||now.saturating_sub(v)>3000){return Ok(());}
    let Some(evidence)=evidence.filter(|e|e["evaluated_ms"].as_u64().is_some_and(|v|v<=now&&now-v<=2000))else{return Ok(());};
    let Some(members)=evidence["signal"]["coordination"]["members"].as_array().filter(|v|!v.is_empty())else{return Ok(());};
    if [t.bid,t.ask,t.point,t.tick_size,t.tick_value_loss,t.tick_value_profit].iter().any(|v|v.is_none_or(|v|!v.is_finite()||v<=0.))||t.ask<t.bid{return Ok(());}
    let bar=bars.last().unwrap().candle.time;
    // First valid observation for a completed bar wins; subsequent ticks never overwrite it.
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM ablation_samples WHERE context=?1 AND bar=?2)",params![key,bar],|r|r.get(0))?;
    if exists{return Ok(());}
    let mut variants=vec![json!({"excluded":Value::Null,"side":direction(members,None)})];
    for m in members {if let Some(id)=m["id"].as_str(){variants.push(json!({"excluded":id,"side":direction(members,Some(id))}));}}
    let p=json!({"version":"expert-ablation-v1","observed_ms":now,"bar":bar,"members":members,"variants":variants,"bid":t.bid,"ask":t.ask,"point":t.point,"tick":t.tick_size,"profit":t.tick_value_profit,"loss":t.tick_value_loss,"commission":c.risk.commission_per_lot,"slippage":c.risk.slippage_ticks*t.tick_size.unwrap()});
    db.execute("INSERT OR IGNORE INTO ablation_samples VALUES(?1,?2,?3,NULL)",params![key,bar,p.to_string()])?;Ok(())
}
pub fn report(db:&Connection,t:Option<&Telemetry>)->Result<Value>{
    let key=t.map(crate::coordinator_learning::context).unwrap_or_default();
    let(total,pending,expired):(u64,u64,u64)=db.query_row("SELECT COUNT(*),COALESCE(SUM(outcome IS NULL),0),COALESCE(SUM(json_extract(outcome,'$.state')='expired'),0) FROM ablation_samples WHERE context=?1",[&key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let mut q=db.prepare("SELECT outcome FROM ablation_samples WHERE context=?1 AND json_extract(outcome,'$.state')='scored' ORDER BY bar DESC LIMIT 5000")?;
    let rows=q.query_map([&key],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    // Per-member comparison uses ONLY rows in which that member belonged to the coordinator.
    let mut results:BTreeMap<String,(u64,u64,f64,f64)>=BTreeMap::new();let mut baseline=0.;
    for raw in &rows {let o:Value=serde_json::from_str(raw)?;let variants=o["variants"].as_array().unwrap();let base=&variants[0];baseline+=base["net_per_lot"].as_f64().unwrap();
        for v in variants.iter().skip(1){let id=v["excluded"].as_str().unwrap();let result=results.entry(id.into()).or_default();result.0+=1;result.1+=u64::from(v["side"]!=base["side"]);result.2+=v["net_per_lot"].as_f64().unwrap();result.3+=base["net_per_lot"].as_f64().unwrap();}
    }
    let currency=t.map(|t|t.currency.as_str()).unwrap_or("");let mut text=format!("مساهمة الأعضاء — الخبير فقط\nمرصود: {total} | انتظار 12 شمعة: {pending} | منتهي بسبب نقص البيانات: {expired}\nنتائج ناضجة معروضة: {} (آخر 5000 كحد أقصى)\n",rows.len());
    if !rows.is_empty(){let _=writeln!(text,"متوسط المنظومة الكاملة: {:.3} {} لكل لوت افتراضي",baseline/rows.len() as f64,currency);}
    let mut members=vec![];
    for(id,(n,changed,without,full))in results{let delta=(without-full)/n as f64;let _=writeln!(text,"بدون {id}: {n} عينة | تغيّر الاتجاه {changed} مرة | فرق المتوسط {delta:+.3} {currency}/lot");members.push(json!({"excluded":id,"samples":n,"changed":changed,"mean_without":without/n as f64,"mean_full_same_samples":full/n as f64,"delta_without_minus_full":delta}));}
    text.push_str("\nالفرق الموجب يعني أن حذف العقل حسّن هذه العينة الافتراضية؛ لا يتم فصله تلقائيًا. تتداخل نوافذ الـ12 شمعة؛ ليست صفقات منفذة أو عينات إحصائية مستقلة. نفس السعر والتكلفة والأفق لكل مقارنة، دون محاكاة SL/TP أو السواب.");
    Ok(json!({"total":total,"pending":pending,"expired":expired,"scored_shown":rows.len(),"members":members,"text":text,"execution_effect":false,"automatic_selection":false}))
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn removing_a_member_changes_only_its_vote(){let members=vec![json!({"id":"a","ready":true,"side":1,"weight":1.}),json!({"id":"b","ready":true,"side":-1,"weight":1.})];assert_eq!(direction(&members,None),0);assert_eq!(direction(&members,Some("b")),1);assert_eq!(direction(&members,Some("a")),-1);}
    #[test]fn prospective_samples_wait_for_twelve_completed_bars_and_survive_reload(){
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        let now=1_800_000_000_000u64;let bar=now/1000-60;
        let make=|time|FrameBar{candle:braintrade_domain::Candle{time,open:100.,high:101.,low:99.,close:100.5,tick_volume:100},spread_points:2,real_volume:0};
        let mut frames=BTreeMap::from([("M1".into(),(0..100).map(|i|make(bar-(99-i)*60)).collect::<Vec<_>>())]);
        let mut t=Telemetry{connected:true,login:1,server:"test".into(),symbol:"XAUUSD".into(),currency:"USD".into(),bid:Some(100.),ask:Some(100.02),point:Some(0.01),tick_size:Some(0.01),tick_value_profit:Some(1.),tick_value_loss:Some(1.),tick_age_ms:Some(0),quote_time_ms:Some(now),..Default::default()};
        let evidence=json!({"evaluated_ms":now,"signal":{"coordination":{"members":[{"id":"a","side":1,"ready":true,"weight":1.},{"id":"b","side":-1,"ready":true,"weight":1.}]}}});
        let c=Config::default();
        observe(&db,&t,&c,true,Some(&evidence),&frames,now).unwrap();observe(&db,&t,&c,true,Some(&evidence),&frames,now).unwrap();
        assert_eq!(report(&db,Some(&t)).unwrap()["total"],1);
        for i in 1..=11{frames.get_mut("M1").unwrap().push(make(bar+i*60));}
        observe(&db,&t,&c,false,None,&frames,now+11*60_000).unwrap();assert_eq!(report(&db,Some(&t)).unwrap()["pending"],1);
        init(&db).unwrap();frames.get_mut("M1").unwrap().push(make(bar+12*60));
        observe(&db,&t,&c,false,None,&frames,now+12*60_000).unwrap();let r=report(&db,Some(&t)).unwrap();assert_eq!(r["scored_shown"],1);assert_eq!(r["members"].as_array().unwrap().len(),2);
        t.login=2;assert_eq!(report(&db,Some(&t)).unwrap()["total"],0);
    }
    #[test]fn cost_model_applies_spread_commission_and_both_slippage_sides(){let p=json!({"bid":100.,"ask":100.02,"tick":0.01,"profit":1.,"loss":1.,"commission":7.,"slippage":0.02});assert!((score(1,&p,101.,0.02)-87.).abs()<1e-8);assert!((score(-1,&p,99.,0.02)-87.).abs()<1e-8);assert_eq!(score(0,&p,101.,0.02),0.);}
}
