//! Immutable, DEMO-only, explicit TP1/TP2/TP3 PRICE proposals.
//!
//! These are operator-supplied price levels, not trading signals or broker
//! orders. Persisted separately from the original three broker-volume plans.
//! No stage progression, order_send, stop modification or BE authorization.
use anyhow::{bail,ensure,Context,Result};
use braintrade_domain::{execution,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn grid(price:f64,tick:f64)->bool{
    if !price.is_finite()||price<=0.||!tick.is_finite()||tick<=0. {return false}
    let units=price/tick;
    units.is_finite()&&units>0.&&units<=1.0e12&&
        (units-units.round()).abs()<=1.0e-6
}
fn close(a:f64,b:f64,unit:f64)->bool{
    a.is_finite()&&b.is_finite()&&unit>0.&&(a-b).abs()<=unit*1e-6
}
fn scoped(t:&Telemetry,target:&str)->bool{
    target=="demo"&&t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&t.margin_mode==Some(2)&&t.validate()&&t.login>0
        &&!t.server.is_empty()&&!t.symbol.is_empty()&&!t.currency.is_empty()
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_price_plans(
            context TEXT NOT NULL,target TEXT NOT NULL CHECK(target='demo'),
            position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
            ticket INTEGER NOT NULL CHECK(ticket>0),
            magic INTEGER NOT NULL CHECK(magic>0),
            plan_ids_json TEXT NOT NULL,
            prices_json TEXT NOT NULL,
            captured_json TEXT NOT NULL,
            saved_ms INTEGER NOT NULL CHECK(saved_ms>0),
            PRIMARY KEY(context,target,position_identifier)
        );
        CREATE INDEX IF NOT EXISTS partial_exit_prices_recent
            ON partial_exit_price_plans(context,target,saved_ms DESC);")?;
    Ok(())
}

/// The caller must additionally require STOP, fresh bound Gateway, and no
/// unresolved Core broker commands. Price capture itself can never send.
pub fn save(db:&Connection,t:&Telemetry,target:&str,pid:u64,
    ids:&[String;3],prices:[f64;3],now:u64)->Result<Value>
{
    ensure!(scoped(t,target),"يتطلب تسعير TP حساب MT5 Demo بنظام Hedging مؤكداً");
    ensure!(t.quote_is_fresh(now,3_000),"انتظر سعراً حديثاً وموثقاً من MT5");
    ensure!(pid>0&&now>0,"معرّف المركز أو وقت الحفظ غير صالح");
    let tick=t.tick_size.context("خطوة سعر الوسيط غير متاحة")?;
    ensure!(tick.is_finite()&&tick>0.,"خطوة سعر الوسيط غير صالحة");
    ensure!(prices.iter().all(|p|grid(*p,tick)),"أسعار TP يجب أن تتطابق مع شبكة Tick الوسيط");
    let key=context(t);
    let selected:Vec<_>=t.positions.iter().filter(|p|
        p.identifier==pid||p.ticket==pid).collect();
    ensure!(selected.len()==1,"المركز غير ظاهر أو معرّفه غير فريد");
    let p=selected[0];
    ensure!(p.identifier==pid&&p.ticket>0&&p.magic>0&&p.symbol==t.symbol
        &&matches!(p.side.as_str(),"BUY"|"SELL")
        &&execution::brain_for_magic(p.magic).is_some(),"هوية المركز أو العقل غير صالحة");
    ensure!(p.price_open.is_finite()&&p.price_open>0.
        &&p.volume.is_finite()&&p.volume>0.,"سعر الدخول أو حجم المركز غير مؤكد");
    let (bid,ask)=t.bid.zip(t.ask).context("Bid/Ask غير متاحين")?;
    ensure!(bid.is_finite()&&ask.is_finite()&&bid>0.&&ask>=bid,"أسعار Bid/Ask غير صالحة");
    let reference=if p.side=="BUY"{bid}else{ask};
    let gain_start=if p.side=="BUY"{p.price_open.max(ask)}else{p.price_open.min(bid)};
    let cmp=|a:f64,b:f64| if p.side=="BUY"{a>b+tick*0.5}else{a+tick*0.5<b};
    ensure!(cmp(prices[0],gain_start)&&cmp(prices[1],prices[0])
        &&cmp(prices[2],prices[1]),"ترتيب أهداف TP لا يحقق ربحاً متزايداً عن سعر الدخول والاقتباس الحالي");
    ensure!(p.tp==0.||if p.side=="BUY"{p.tp>prices[2]+tick*0.5}
        else{p.tp+tick*0.5<prices[2]},
        "TP الوسيط الأصلي قد يغلق كامل المركز قبل اكتمال المراحل");
    let draft:(u64,u64,f64,f64,String,String)=db.query_row(
        "SELECT ticket,magic,initial_volume,step,status,payload
         FROM partial_exit_drafts WHERE context=?1 AND target='demo'
         AND position_identifier=?2",
        params![key,pid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,
                                r.get(3)?,r.get(4)?,r.get(5)?))
    ).context("مسودة TP الأصلية غير متوفرة")?;
    let (ticket,magic,initial,lot_step,state,raw)=draft;
    let original:Value=serde_json::from_str(&raw)?;
    ensure!(state=="draft_only"&&ticket==p.ticket&&magic==p.magic
        &&close(initial,p.volume,lot_step)&&original["schema"]=="partial-exit-draft-v1"
        &&original["ticket"].as_u64()==Some(ticket)
        &&original["magic"].as_u64()==Some(magic)
        &&original["position_identifier"].as_u64()==Some(pid)
        &&original["symbol"]==t.symbol&&original["side"]==p.side
        &&original["broker_orders_created"]==false
        &&original["targets_priced"]==false,
        "مسودة TP غير مستقرة أو المركز تغير منذ الحفظ");
    let mut st=db.prepare(
        "SELECT stage,plan_id,status FROM partial_exit_stage_plans
         WHERE context=?1 AND target='demo' AND position_identifier=?2
         ORDER BY stage")?;
    let stages=st.query_map(params![key,pid],|r|Ok((
        r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    ensure!(stages.len()==3&&stages.iter().enumerate().all(|(i,(stage,id,status))|
        *stage==(i+1) as u32 && *id==ids[i] &&status=="prepared_only"
        &&id.starts_with("tp-plan-")&&id.len()==40),
        "هوية مرحلة TP تغيرت أو تتطلب مراجعة");
    for id in ids {
        let used:i64=db.query_row(
            "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='close'
             AND CASE WHEN json_valid(payload) THEN
                 json_extract(payload,'$.tp_stage_plan_id') ELSE NULL END=?2",
            params![key,id],|r|r.get(0))?;
        ensure!(used==0,"يوجد أمر سابق مرتبط بخطة TP؛ لا يمكن تعديل التسعير");
    }
    let prior:Option<(String,String)>=db.query_row(
        "SELECT plan_ids_json,prices_json FROM partial_exit_price_plans
         WHERE context=?1 AND target='demo' AND position_identifier=?2",
        params![key,pid],|r|Ok((r.get(0)?,r.get(1)?))
    ).optional()?;
    if let Some((old_ids,old_prices))=prior {
        let x:Value=serde_json::from_str(&old_ids)?;
        let y:Value=serde_json::from_str(&old_prices)?;
        ensure!(x==json!(ids)&&y==json!(prices),
            "تم حفظ أسعار مختلفة مسبقاً؛ لا يمكن تغيير هوية خطة TP بصمت");
        return Ok(json!({"saved":true,"idempotent":true,"read_only":true,
            "orders_enabled":false,"breakeven_enabled":false}));
    }
    let captured=json!({"schema":"partial-exit-price-capture-v1",
        "context":key,"target":"demo","position_identifier":pid,
        "ticket":ticket,"magic":magic,"side":p.side,"symbol":t.symbol,
        "entry":p.price_open,"original_volume":initial,"lot_step":lot_step,
        "tick_size":tick,"quote_reference":reference,
        "broker_quote_ms":t.broker_quote_time_ms(),"saved_ms":now,
        "trading_enabled":false,"orders_enabled":false,
        "breakeven_enabled":false});
    db.execute(
        "INSERT INTO partial_exit_price_plans(context,target,position_identifier,
            ticket,magic,plan_ids_json,prices_json,captured_json,saved_ms)
         VALUES(?1,'demo',?2,?3,?4,?5,?6,?7,?8)",
        params![key,pid,ticket,magic,json!(ids).to_string(),
            json!(prices).to_string(),captured.to_string(),now])?;
    Ok(json!({"saved":true,"idempotent":false,"read_only":true,
        "orders_enabled":false,"breakeven_enabled":false}))
}

pub fn report(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<Value>{
    if !scoped(t,target) {
        return Ok(json!({"schema":"partial-exit-prices-v1","status":"unverified_demo_hedging",
            "rows":[],"read_only":true,"orders_enabled":false}))
    }
    let key=context(t);
    let mut st=db.prepare(
        "SELECT position_identifier,ticket,magic,plan_ids_json,prices_json,
                captured_json,saved_ms FROM partial_exit_price_plans
         WHERE context=?1 AND target='demo' ORDER BY saved_ms DESC LIMIT 50")?;
    let entries=st.query_map(params![key],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,u64>(2)?,
        r.get::<_,String>(3)?,r.get::<_,String>(4)?,
        r.get::<_,String>(5)?,r.get::<_,u64>(6)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let fresh=t.quote_is_fresh(now,3_000)&&t.bid.zip(t.ask).is_some_and(|(b,a)|
        b.is_finite()&&a.is_finite()&&b>0.&&a>=b);
    let mut rows=Vec::new();
    for (pid,ticket,magic,raw_ids,raw_prices,raw_capture,saved_ms) in entries{
        let ids=serde_json::from_str::<Value>(&raw_ids).unwrap_or(Value::Null);
        let prices=serde_json::from_str::<Value>(&raw_prices).unwrap_or(Value::Null);
        let cap=serde_json::from_str::<Value>(&raw_capture).unwrap_or(Value::Null);
        let tick=cap["tick_size"].as_f64().unwrap_or(f64::NAN);
        let intact=cap["schema"]=="partial-exit-price-capture-v1"
            &&cap["context"]==key&&cap["ticket"].as_u64()==Some(ticket)
            &&cap["magic"].as_u64()==Some(magic)&&cap["position_identifier"].as_u64()==Some(pid)
            &&cap["target"]=="demo"&&cap["orders_enabled"]==false
            &&cap["breakeven_enabled"]==false&&cap["saved_ms"].as_u64()==Some(saved_ms)
            &&matches!(cap["side"].as_str(),Some("BUY"|"SELL"))
            &&prices.as_array().is_some_and(|a|a.len()==3
                &&a.iter().all(|p|p.as_f64().is_some_and(|p|grid(p,tick))))
            &&ids.as_array().is_some_and(|a|a.len()==3&&a.iter().all(|v|
                v.as_str().is_some_and(|id|id.starts_with("tp-plan-")&&id.len()==40)));
        let p=t.positions.iter().find(|p|p.identifier==pid&&p.ticket==ticket
            &&p.magic==magic&&p.symbol==t.symbol&&cap["side"]==p.side);
        let plan_ok=if intact{
            let mut statement=db.prepare("SELECT stage,plan_id,status
                FROM partial_exit_stage_plans WHERE context=?1
                AND target='demo' AND position_identifier=?2 ORDER BY stage")?;
            let items=statement.query_map(params![key,pid],|r|Ok((
                r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?
            )))?.collect::<rusqlite::Result<Vec<_>>>()?;
            items.len()==3&&items.iter().enumerate().all(|(i,(stage,id,status))|
                *stage==(i+1) as u32&&ids[i]==*id&&status=="prepared_only")
        }else{false};
        let state=if !intact{"invalid_record"}
            else if !plan_ok{"plan_requires_review"}
            else if p.is_none(){"position_absent_review"}
            else if !fresh{"quote_stale_no_trigger"}
            else{"price_levels_saved_no_execution"};
        let buy=cap["side"]=="BUY";
        let quote=if fresh {if buy{t.bid}else{t.ask}}else{None};
        let crossed=if state=="price_levels_saved_no_execution"{
            quote.and_then(|q|prices.as_array().map(|a|
                a.iter().map(|v|v.as_f64().is_some_and(|price|
                    if buy{q>=price}else{q<=price})).collect::<Vec<_>>()))
        }else{None};
        rows.push(json!({"position_identifier":pid,"ticket":ticket,"plan_ids":ids,
            "prices":prices,"captured_ms":saved_ms,"state":state,
            "quote_crossing_only":crossed,
            "orders_enabled":false,"next_stage_authorized":false,
            "breakeven_enabled":false}));
    }
    Ok(json!({"schema":"partial-exit-prices-v1",
        "status":"demo_operator_price_proposals_only",
        "context":key,"target":"demo","read_only":true,
        "orders_enabled":false,"next_stage_authorized":false,
        "breakeven_enabled":false,"rows":rows,
        "note":"Prices are immutable operator proposals. Quote crossing is NOT proof of a trade; Core stage sends remain locked."}))
}

#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    fn t()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:222,server:"DemoA".into(),
            currency:"USD".into(),symbol:"XAUUSDm".into(),
            balance:1000.,equity:1000.,volume_min:Some(0.01),
            volume_step:Some(0.01),volume_max:Some(100.),
            tick_size:Some(0.01),bid:Some(2000.0),ask:Some(2000.20),
            tick_age_ms:Some(100),quote_time_ms:Some(9900),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.1,
                price_open:1999.5,..Default::default()}],..Default::default()}
    }
    fn db()->Connection{
        let d=Connection::open_in_memory().unwrap();
        crate::partial_exit_ledger::init(&d).unwrap();
        crate::partial_exit_stage::init(&d).unwrap();
        d.execute_batch("CREATE TABLE broker_commands(context TEXT,kind TEXT,payload TEXT);").unwrap();
        init(&d).unwrap();
        let t=t();
        crate::partial_exit_ledger::observe(&d,&t,"demo",100).unwrap();
        crate::partial_exit_stage::observe(&d,&t,"demo",100).unwrap();
        d
    }
    fn ids(db:&Connection)->[String;3]{
        let v=crate::partial_exit_stage::report(db,&t(),"demo").unwrap();
        std::array::from_fn(|i|v["rows"][i]["plan_id"].as_str().unwrap().into())
    }
    #[test]
    fn immutable_ordered_tick_aligned_demo_prices_with_idempotent_restart(){
        let d=db();let t=t();let plans=ids(&d);
        let result=save(&d,&t,"demo",907,&plans,[2001.,2002.,2003.],10_000).unwrap();
        assert_eq!(result["orders_enabled"],false);
        assert_eq!(result["idempotent"],false);
        assert_eq!(save(&d,&t,"demo",907,&plans,[2001.,2002.,2003.],10_000).unwrap()["idempotent"],true);
        assert!(save(&d,&t,"demo",907,&plans,[2001.,2002.,2004.],10_000).is_err());
        let v=report(&d,&t,"demo",10_000).unwrap();
        assert_eq!(v["rows"][0]["prices"],json!([2001.,2002.,2003.]));
        assert_eq!(v["orders_enabled"],false);
        assert_eq!(v["rows"][0]["quote_crossing_only"],json!([false,false,false]));
        assert_eq!(report(&d,&t,"live",10_000).unwrap()["status"],"unverified_demo_hedging");
        assert_eq!(report(&d,&t,"demo",20_000).unwrap()["rows"][0]["state"],"quote_stale_no_trigger");
        let mut next=t.clone();next.bid=Some(2001.5);next.ask=Some(2001.7);
        assert_eq!(report(&d,&next,"demo",10_000).unwrap()["rows"][0]["quote_crossing_only"],json!([true,false,false]));
    }
    #[test]
    fn wrong_account_side_hedging_lot_and_no_quote_fail_closed(){
        let d=db();let t=t();let plans=ids(&d);
        for variant in 0..9{
            let mut wrong=t.clone();
            match variant{
                0=>wrong.account_trade_mode=Some(2),
                1=>wrong.margin_mode=Some(0),
                2=>wrong.login+=1,
                3=>wrong.positions[0].ticket=200,
                4=>wrong.positions[0].magic=900,
                5=>wrong.positions[0].side="SELL".into(),
                6=>wrong.tick_size=None,
                7=>wrong.tick_age_ms=Some(5000),
                _=>wrong.positions[0].volume=0.07,
            }
            assert!(save(&d,&wrong,"demo",907,&plans,[2001.,2002.,2003.],10_000).is_err(),"variant {variant}");
        }
    }
    #[test]
    fn malformed_side_grid_order_original_mt5_tp_and_plan_ids_are_rejected(){
        let d=db();let mut t=t();let plans=ids(&d);
        for levels in [[2000.1,2002.,2003.],[2001.,2000.9,2003.],
            [2001.,2002.,2001.],[2001.005,2002.,2003.],
            [f64::NAN,2002.,2003.]]{
            assert!(save(&d,&t,"demo",907,&plans,levels,10_000).is_err());
        }
        let mut ids=plans.clone();ids[1]="tp-plan-invalid".into();
        assert!(save(&d,&t,"demo",907,&ids,[2001.,2002.,2003.],10_000).is_err());
        t.positions[0].tp=2002.;
        assert!(save(&d,&t,"demo",907,&plans,[2001.,2002.,2003.],10_000).is_err());
    }
    #[test]
    fn restarted_database_retains_prices_without_mutating_order_execution(){
        let path=std::env::temp_dir().join(format!("bt-tp-prices-{}.sqlite",uuid::Uuid::new_v4().simple()));
        let t=t();let saved;
        {
            let d=Connection::open(&path).unwrap();
            crate::partial_exit_ledger::init(&d).unwrap();
            crate::partial_exit_stage::init(&d).unwrap();
            d.execute_batch("CREATE TABLE broker_commands(context TEXT,kind TEXT,payload TEXT);").unwrap();
            init(&d).unwrap();
            crate::partial_exit_ledger::observe(&d,&t,"demo",100).unwrap();
            crate::partial_exit_stage::observe(&d,&t,"demo",100).unwrap();
            let ids=ids(&d);
            save(&d,&t,"demo",907,&ids,[2001.,2002.,2003.],10_000).unwrap();
            saved=report(&d,&t,"demo",10_000).unwrap()["rows"][0]["prices"].clone();
        }
        {
            let d=Connection::open(&path).unwrap();
            init(&d).unwrap();
            let v=report(&d,&t,"demo",10_000).unwrap();
            assert_eq!(v["rows"][0]["prices"],saved);
            assert_eq!(v["orders_enabled"],false);
            let used:i64=d.query_row("SELECT COUNT(*) FROM broker_commands",[],|r|r.get(0)).unwrap();
            assert_eq!(used,0);
            d.execute("UPDATE partial_exit_stage_plans SET status='review_required' WHERE stage=1",[]).unwrap();
            let bad=report(&d,&t,"demo",10_000).unwrap();
            assert_eq!(bad["rows"][0]["state"],"plan_requires_review");
            assert!(bad["rows"][0]["quote_crossing_only"].is_null());
        }
        let _=std::fs::remove_file(path);
    }
    #[test]
    fn unrelated_corrupt_broker_command_cannot_break_research_price_capture(){
        let d=db();let t=t();let plans=ids(&d);
        d.execute("INSERT INTO broker_commands(context,kind,payload)
            VALUES(?1,'close','broken-json')",[context(&t)]).unwrap();
        let r=save(&d,&t,"demo",907,&plans,[2001.,2002.,2003.],10_000).unwrap();
        assert_eq!(r["saved"],true);
        assert_eq!(r["orders_enabled"],false);
    }
    #[test]
    fn sell_prices_must_descend_beyond_broker_ask(){
        let mut t=t();t.positions[0].side="SELL".into();
        t.positions[0].price_open=2000.8;
        // Build the original SELL plan from scratch.
        let other=Connection::open_in_memory().unwrap();
        crate::partial_exit_ledger::init(&other).unwrap();
        crate::partial_exit_stage::init(&other).unwrap();
        other.execute_batch("CREATE TABLE broker_commands(context TEXT,kind TEXT,payload TEXT);").unwrap();
        init(&other).unwrap();
        crate::partial_exit_ledger::observe(&other,&t,"demo",100).unwrap();
        crate::partial_exit_stage::observe(&other,&t,"demo",100).unwrap();
        let v=crate::partial_exit_stage::report(&other,&t,"demo").unwrap();
        let plans=std::array::from_fn(|i|v["rows"][i]["plan_id"].as_str().unwrap().to_string());
        assert!(save(&other,&t,"demo",907,&plans,[1999.,1998.,1997.],10_000).is_ok());
        assert!(save(&other,&t,"demo",907,&plans,[1999.,1998.,2000.],10_000).is_err());
    }
}
