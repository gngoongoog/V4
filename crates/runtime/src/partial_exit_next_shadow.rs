//! Read-only post-TP1/TP2 ladder eligibility from *verified broker history*.
//!
//! The old immutable TP drafts deliberately become review_required after
//! physical position volume changes. Do not reset those rows. Historical
//! broker stage settlements are the ONLY route to evaluating subsequent
//! research targets. This module creates NO broker or shadow orders.
use anyhow::Result;
use braintrade_domain::{execution,Telemetry};
use rusqlite::{params,Connection};
use serde_json::{json,Value};
use std::collections::BTreeSet;

fn key(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn near(a:f64,b:f64,step:f64)->bool{
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
// Display only a volume already proven to be on the broker lot grid.
// For example 0.05 - 0.03 is 0.020000000000000004 in binary floats;
// never use this helper to make an off-grid close eligible.
fn grid_amount(volume:f64,step:f64)->f64{
    if !volume.is_finite()||!step.is_finite()||step<=0. {return volume}
    let steps=volume/step;
    if !steps.is_finite(){return volume}
    let snapped=steps.round()*step;
    if near(volume,snapped,step){
        if near(snapped,0.,step){0.0}else{snapped}
    }else{volume}
}
fn account(t:&Telemetry,target:&str)->bool{
    target=="demo"&&t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&t.margin_mode==Some(2)&&t.validate()&&t.login>0
        &&!t.server.is_empty()&&!t.symbol.is_empty()&&!t.currency.is_empty()
}
/// Stateless research gate. A "target crossed" result is NOT permission to
/// place a trade. It must never be passed to the gateway/authorizer.
pub fn report(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<Value>{
    if !account(t,target){
        return Ok(json!({"schema":"partial-exit-next-shadow-gate-v1",
            "status":"unverified_demo_hedging","rows":[],
            "read_only":true,"orders_enabled":false,
            "next_stage_authorized":false,"breakeven_enabled":false}))
    }
    let context=key(t);
    // Empty accounts must not pay for a full broker Deal-history audit on
    // every dashboard refresh. A saved prior TP seal is required.
    let has_prior:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM partial_exit_stage_settlements
         WHERE context=?1 AND target='demo' LIMIT 1)",
        params![context],|r|r.get(0))?;
    if !has_prior {
        return Ok(json!({"schema":"partial-exit-next-shadow-gate-v1",
            "status":"no_broker_verified_previous_stage","context":context,
            "target":"demo","rows":[],"read_only":true,
            "orders_enabled":false,"next_stage_authorized":false,
            "breakeven_enabled":false}))
    }
    let progress=crate::partial_exit_progress::report(db,t,target)?;
    if progress["status"]!="account_scoped_historical_settlements" {
        return Ok(json!({"schema":"partial-exit-next-shadow-gate-v1",
            "status":"settlement_unavailable","context":context,"target":"demo",
            "read_only":true,"orders_enabled":false,"rows":[]}))
    }
    let history=progress["rows"].as_array();
    let fresh=t.quote_is_fresh(now,3_000)
        &&t.source_quote_time_ms.is_some_and(|ms|ms>0&&ms<=now.saturating_add(2_000))
        &&t.bid.zip(t.ask).is_some_and(|(b,a)|b.is_finite()&&a.is_finite()&&b>0.&&a>=b);
    let mut st=db.prepare(
        "SELECT position_identifier,ticket,magic,plan_ids_json,prices_json,
                captured_json,saved_ms FROM partial_exit_price_plans
         WHERE context=?1 AND target='demo'
         ORDER BY saved_ms DESC LIMIT 25")?;
    let input=st.query_map(params![context],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,u64>(2)?,
        r.get::<_,String>(3)?,r.get::<_,String>(4)?,
        r.get::<_,String>(5)?,r.get::<_,u64>(6)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    let mut rows=Vec::new();
    for (pid,ticket,magic,raw_ids,raw_prices,raw_cap,saved) in input{
        let ids=serde_json::from_str::<Value>(&raw_ids).unwrap_or(Value::Null);
        let prices=serde_json::from_str::<Value>(&raw_prices).unwrap_or(Value::Null);
        let cap=serde_json::from_str::<Value>(&raw_cap).unwrap_or(Value::Null);
        let stages:Vec<(u32,String,f64,String)>={
            let mut query=db.prepare(
                "SELECT stage,plan_id,planned_volume,status
                 FROM partial_exit_stage_plans WHERE context=?1 AND target='demo'
                 AND position_identifier=?2 ORDER BY stage")?;
            let loaded=query.query_map(params![context,pid],|r|Ok((
                r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?
            )))?.collect::<rusqlite::Result<Vec<_>>>()?;
            loaded
        };
        if stages.len()!=3||!ids.is_array()||!prices.is_array() {
            continue
        }
        let listed=ids.as_array().unwrap();
        let levels=prices.as_array().unwrap();
        let stage_identity=listed.len()==3&&levels.len()==3
            &&stages.iter().enumerate().all(|(i,(num,id,vol,_))|
                *num==(i+1) as u32&&listed[i]==*id
                &&id.starts_with("tp-plan-")&&id.len()==40
                &&vol.is_finite()&&*vol>0.);
        if !stage_identity{continue}
        let Some((draft_status,initial,step,draft_raw))=db.query_row(
            "SELECT status,initial_volume,step,payload
             FROM partial_exit_drafts WHERE context=?1 AND target='demo'
             AND position_identifier=?2",
            params![context,pid],|r|Ok((
                r.get::<_,String>(0)?,r.get::<_,f64>(1)?,
                r.get::<_,f64>(2)?,r.get::<_,String>(3)?
            ))
        ).ok() else{continue};
        let draft=serde_json::from_str::<Value>(&draft_raw).unwrap_or(Value::Null);
        let p=t.positions.iter().filter(|p|p.identifier==pid||p.ticket==ticket)
            .collect::<Vec<_>>();
        let tick=cap["tick_size"].as_f64().unwrap_or(f64::NAN);
        let original_ok=crate::partial_exit_prices::capture_geometry_valid(&cap,&prices)
            &&cap["schema"]=="partial-exit-price-capture-v1"
            &&cap["context"]==context&&cap["target"]=="demo"
            &&cap["position_identifier"].as_u64()==Some(pid)
            &&cap["ticket"].as_u64()==Some(ticket)
            &&cap["magic"].as_u64()==Some(magic)
            &&cap["symbol"]==t.symbol
            &&cap["saved_ms"].as_u64()==Some(saved)
            &&cap["orders_enabled"]==false
            &&cap["breakeven_enabled"]==false
            &&matches!(cap["side"].as_str(),Some("BUY"|"SELL"))
            &&draft["schema"]=="partial-exit-draft-v1"
            &&draft["position_identifier"].as_u64()==Some(pid)
            &&draft["ticket"].as_u64()==Some(ticket)
            &&draft["magic"].as_u64()==Some(magic)
            &&draft["symbol"]==t.symbol
            &&draft["side"]==cap["side"]
            &&draft["broker_orders_created"]==false
            &&near(cap["original_volume"].as_f64().unwrap_or(f64::NAN),initial,step)
            &&near(draft["entry_volume"].as_f64().unwrap_or(f64::NAN),initial,step)
            &&near(draft["volume_step"].as_f64().unwrap_or(f64::NAN),step,step)
            &&near(cap["lot_step"].as_f64().unwrap_or(f64::NAN),step,step)
            &&t.volume_step.is_some_and(|s|near(s,step,step))
            &&t.tick_size.is_some_and(|v|near(v,tick,tick))
            &&tick.is_finite()&&tick>0.
            &&initial.is_finite()&&initial>0.
            &&stages.iter().map(|s|s.2).sum::<f64>().is_finite()
            &&near(stages.iter().map(|s|s.2).sum::<f64>(),initial,step);
        // The *prior* stages must have independent, still-current order,
        // Deal and residual-snapshot seals. An old synthetic record or a
        // previously revoked seal cannot advance the ladder.
        let sealed=|stage:u32|history.and_then(|rows|rows.iter().find(|r|
            r["position_identifier"].as_u64()==Some(pid)
            &&r["stage"].as_u64()==Some(stage as u64)
            &&r["plan_id"]==stages[(stage-1) as usize].1))
            .filter(|r|r["state"]=="historical_stage_sealed"
                &&r["next_stage_authorized"]==false
                &&r["breakeven_triggered"]==false);
        let prior_count=if sealed(1).is_some(){
            if sealed(2).is_some(){2}else{1}
        }else{0};
        // TP2 and TP3 are *always* false until at least one prior stage is
        // proven with exact MT5 order, Deal and changed position volume.
        if prior_count==0 {continue}
        let stage=(prior_count+1) as usize;
        if stage>3{continue}
        let previous=sealed(prior_count).unwrap();
        let previous_at=previous["sealed_ms"].as_u64().unwrap_or(0);
        let volume_exited=stages.iter().take(prior_count as usize)
            .map(|s|s.2).sum::<f64>();
        let remaining=grid_amount(initial-volume_exited,step);
        // A historical stage may have a correct Deal ID yet a corrupted
        // recorded residual. Never let that old inconsistency promote TP2/3.
        let previous_residual_proven=near(
            previous["remaining_volume_at_seal"].as_f64().unwrap_or(f64::NAN),
            remaining,step);
        let active=p.len()==1&&p[0].identifier==pid&&p[0].ticket==ticket
            &&p[0].magic==magic&&p[0].symbol==t.symbol
            &&cap["side"]==p[0].side
            &&execution::brain_for_magic(magic).is_some()
            &&near(p[0].volume,remaining,step)
            &&near(p[0].price_open,cap["entry"].as_f64().unwrap_or(f64::NAN),tick)
            // A changed original MT5 full-position TP cannot be treated as
            // the same immutable research proposal, even if moved farther.
            &&near(p[0].tp,cap["original_broker_tp"].as_f64().unwrap_or(f64::NAN),tick)
            &&levels[2].as_f64().is_some_and(|last|
                last.is_finite()&&last>0.
                &&(p[0].tp==0.||if cap["side"]=="BUY"{
                    p[0].tp>last+tick*0.5
                }else{p[0].tp+tick*0.5<last}));
        // Original immutable rows become review_required after a *verified*
        // partial fill; never reset them. A price crossing is still only a
        // shadow observation, not a command or Stage 2/3 authorization.
        let previous_review_expected=draft_status=="volume_changed_unverified"
            &&stages.iter().all(|s|s.3=="review_required");
        let current_lots=stages[stage-1].2;
        let expected_after=grid_amount(remaining-current_lots,step);
        let grid=t.volume_min.zip(t.volume_max).is_some_and(|(min,max)|
            min.is_finite()&&min>0.&&max.is_finite()&&max>=min
            &&current_lots>=min-step*1e-6&&current_lots<=max+step*1e-6
            &&(stage==3&&near(expected_after,0.,step)
               ||stage<3&&expected_after>=min-step*1e-6)
            &&near(current_lots/step,(current_lots/step).round(),1.)
            &&(stage==3&&near(expected_after,0.,step)
                ||stage<3&&near(expected_after/step,(expected_after/step).round(),1.)));
        let target_price=levels[stage-1].as_f64().unwrap_or(f64::NAN);
        let source=t.source_quote_time_ms.unwrap_or(0);
        let quote_after_history=fresh&&saved>0&&previous_at>0
            &&source>saved&&source>previous_at;
        let quote=if cap["side"]=="BUY"{t.bid}else{t.ask};
        let price_crossed=quote.is_some_and(|q|q.is_finite()&&
            target_price.is_finite()&&target_price>0.
            &&if cap["side"]=="BUY"{q>=target_price}else{q<=target_price});
        // The only Core closes allowed on this ticket are the exact
        // history-verified earlier TP stage commands. Any unknown, cancelled,
        // malformed, or extra close prevents a research "ready" display.
        let prior_ids=(1..=prior_count).filter_map(|n|
            sealed(n).and_then(|r|r["core_command_id"].as_str().map(str::to_owned)))
            .collect::<BTreeSet<_>>();
        let commands= {
            let mut st=db.prepare(
                "SELECT id,payload FROM broker_commands WHERE context=?1 AND kind='close'")?;
            let loaded=st.query_map(params![context],|r|Ok((
                r.get::<_,String>(0)?,r.get::<_,String>(1)?
            )))?.collect::<rusqlite::Result<Vec<_>>>()?;
            loaded
        };
        let mut related=BTreeSet::new();
        let mut unknown=false;
        for (id,raw) in commands{
            let cmd=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
            let position=cmd["position"].as_u64();
            if position.is_none() {unknown=true;continue}
            if position==Some(ticket)||cmd["expected_position_identifier"].as_u64()==Some(pid){
                related.insert(id);
            }
        }
        let only_previous=related==prior_ids&&prior_ids.len()==prior_count as usize&&!unknown;
        let reason=if !original_ok{"original_identity_changed"}
            else if !previous_residual_proven{"previous_stage_residual_not_proven"}
            else if !previous_review_expected{"original_plan_review_state_mismatch"}
            else if !active{"residual_position_not_verified"}
            else if !grid{"broker_lot_grid_invalid"}
            else if !only_previous{"unverified_or_competing_close_command"}
            else if !quote_after_history{"broker_quote_not_after_previous_settlement"}
            else if !price_crossed{"next_target_not_crossed"}
            else{"historical_prerequisites_met_shadow_only"};
        rows.push(json!({
            "position_identifier":pid,"ticket":ticket,
            "stage":stage,"plan_id":stages[stage-1].1,
            "target_price":target_price,
            "expected_close_lots":current_lots,
            "expected_position_volume":remaining,
            "expected_remaining_volume":if stage==3{0.0}else{expected_after},
            "observed_quote_source_ms":source,
            "previous_broker_sealed_ms":previous_at,
            "state":reason,
            "historical_condition_only":reason=="historical_prerequisites_met_shadow_only",
            "read_only":true,"orders_enabled":false,
            "next_stage_authorized":false,"breakeven_enabled":false
        }));
    }
    Ok(json!({
        "schema":"partial-exit-next-shadow-gate-v1",
        "status":"verified_history_research_only",
        "context":context,"target":"demo","rows":rows,
        "read_only":true,"orders_enabled":false,
        "next_stage_authorized":false,"breakeven_enabled":false,
        "note":"A verified earlier MT5 stage and exact residual are necessary to consider a later price crossing. This report creates no trades, order IDs, approval, or stop changes."
    }))
}


#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::Position;
    fn t()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:222,server:"DemoA".into(),
            currency:"USD".into(),symbol:"XAUUSDm".into(),
            balance:1000.,equity:1000.,volume_min:Some(0.01),
            volume_step:Some(0.01),volume_max:Some(100.),
            tick_size:Some(0.01),bid:Some(2000.),ask:Some(2000.2),
            tick_age_ms:Some(100),quote_time_ms:Some(9900),
            source_quote_time_ms:Some(9900),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.10,
                price_open:1999.5,..Default::default()}],..Default::default()}
    }
    fn fixture(d:&Connection){
        crate::partial_exit_ledger::init(d).unwrap();
        crate::partial_exit_stage::init(d).unwrap();
        crate::partial_exit_prices::init(d).unwrap();
        crate::partial_exit_progress::init(d).unwrap();
        d.execute_batch(
            "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT,
                brain TEXT,kind TEXT,created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
             CREATE TABLE broker_deals(context TEXT,ticket INTEGER,position_id INTEGER,
                brain TEXT,payload TEXT,PRIMARY KEY(context,ticket));
             CREATE TABLE broker_history_sync(context TEXT,target TEXT,until_ms INTEGER,
                synced_ms INTEGER,PRIMARY KEY(context,target));").unwrap();
        let original=t();
        crate::partial_exit_ledger::observe(d,&original,"demo",100).unwrap();
        crate::partial_exit_stage::observe(d,&original,"demo",100).unwrap();
        let plans=crate::partial_exit_stage::report(d,&original,"demo").unwrap();
        let ids=std::array::from_fn(|i|plans["rows"][i]["plan_id"].as_str().unwrap().to_string());
        crate::partial_exit_prices::save(d,&original,"demo",907,
            &ids,[2001.,2002.,2003.],10_000).unwrap();
        d.execute("INSERT INTO broker_history_sync VALUES(?1,'demo',11000,11001)",
            [key(&original)]).unwrap();
    }
    fn fill(d:&Connection,n:u32){
        let base=t();let context=key(&base);
        let plans=crate::partial_exit_stage::report(d,&base,"demo").unwrap();
        let plan=plans["rows"][(n-1) as usize]["plan_id"].as_str().unwrap();
        let lots=[0.05,0.03][(n-1) as usize];
        let previous=[0.1,0.05][(n-1) as usize];
        let command=json!({"id":format!("core-close-{n}"),"kind":"close",
            "context":context,"target":"demo","login":222,
            "server":"DemoA","currency":"USD","symbol":"XAUUSDm",
            "brain":"gann","magic":410601,"position":87,
            "tp_stage":n,"tp_stage_plan_id":plan,"lots":lots,
            "created_ms":10030+n as u64,
            "expected_position_identifier":907,
            "expected_position_volume":previous,
            "expected_position_side":"BUY",
            "tp_target_price":([2001.0,2002.0][(n-1) as usize]),
            "expected_position_entry":1999.5,"expected_broker_tp":0.0});
        let result=json!({"state":"filled","recovered_from_history":true,
            "volume":lots,"position_id":907,"order":700+n,
            "deal":900+n,"deals":[900+n],
            "request":{"volume":lots,"position":87,"type":1,
                "magic":410601,"symbol":"XAUUSDm"}});
        d.execute("INSERT INTO broker_commands VALUES(?1,?2,'gann','close',?3,
            'observed',?4,?5)",
            params![format!("core-close-{n}"),context,10030+n as u64,
                command.to_string(),result.to_string()]).unwrap();
        let deal=json!({"ticket":900+n,"order":700+n,"position_id":907,
            "symbol":"XAUUSDm","magic":410601,"entry":1,
            "type":1,"time_msc":10090+n as u64,
            "price":2000.0,"volume":lots,"profit":1.0,
            "commission":-0.03,"swap":0.0,"fee":0.0});
        d.execute("INSERT INTO broker_deals VALUES(?1,?2,907,'gann',?3)",
            params![context,900+n,deal.to_string()]).unwrap();
    }
    fn reconcile(d:&Connection,t:&Telemetry,now:u64){
        crate::partial_exit_ledger::observe(d,t,"demo",now).unwrap();
        crate::partial_exit_stage::observe(d,t,"demo",now).unwrap();
        crate::partial_exit_progress::observe(d,t,"demo",now).unwrap();
    }
    fn first(d:&Connection,t:&Telemetry,now:u64)->Value{
        report(d,t,"demo",now).unwrap()["rows"][0].clone()
    }
    fn tp2_fixture()->(Connection,Telemetry){
        let d=Connection::open_in_memory().unwrap();
        fixture(&d);
        fill(&d,1);
        let mut t=t();t.positions[0].volume=0.05;
        t.bid=Some(2002.2);t.ask=Some(2002.4);
        t.quote_time_ms=Some(10300);t.source_quote_time_ms=Some(10300);
        reconcile(&d,&t,10200);
        assert_eq!(crate::partial_exit_progress::report(&d,&t,"demo")
            .unwrap()["rows"][0]["state"],"historical_stage_sealed");
        (d,t)
    }
    #[test]
    fn lot_grid_normalization_never_rounds_invalid_off_grid_residual(){
        assert_eq!(grid_amount(0.05_f64-0.03_f64,0.01),0.02);
        assert_eq!(grid_amount(0.1_f64-0.05_f64-0.03_f64,0.01),0.02);
        assert_eq!(grid_amount(0.02_f64-0.02_f64,0.01),0.);
        assert_eq!(grid_amount(0.025,0.01),0.025,
            "An off-grid amount must never be silently rounded into eligibility");
        assert_eq!(grid_amount(f64::NAN,0.01).is_nan(),true);
    }
    #[test]
    fn missing_actual_tp1_broker_fill_never_advances_to_tp2(){
        let d=Connection::open_in_memory().unwrap();
        fixture(&d);
        let mut t=t();t.bid=Some(2002.2);t.ask=Some(2002.4);
        t.source_quote_time_ms=Some(10300);t.quote_time_ms=Some(10300);
        let v=report(&d,&t,"demo",10350).unwrap();
        assert_eq!(v["status"],"no_broker_verified_previous_stage");
        assert!(v["rows"].as_array().unwrap().is_empty());
        assert_eq!(v["orders_enabled"],false);
        assert_eq!(v["next_stage_authorized"],false);
    }
    #[test]
    fn broker_confirmed_tp1_unlocks_only_historical_research_condition_for_tp2(){
        let (d,t)=tp2_fixture();
        let v=report(&d,&t,"demo",10350).unwrap();
        assert_eq!(v["rows"][0]["stage"],2);
        assert_eq!(v["rows"][0]["expected_close_lots"],0.03);
        assert_eq!(v["rows"][0]["expected_position_volume"],0.05);
        assert_eq!(v["rows"][0]["expected_remaining_volume"],0.02);
        assert_eq!(v["rows"][0]["state"],"historical_prerequisites_met_shadow_only");
        assert_eq!(v["rows"][0]["historical_condition_only"],true);
        assert_eq!(v["rows"][0]["orders_enabled"],false);
        assert_eq!(v["rows"][0]["next_stage_authorized"],false);
        assert_eq!(v["rows"][0]["breakeven_enabled"],false);
        let draft:String=d.query_row(
            "SELECT status FROM partial_exit_drafts WHERE position_identifier=907",
            [],|r|r.get(0)).unwrap();
        let stage:String=d.query_row(
            "SELECT status FROM partial_exit_stage_plans WHERE position_identifier=907 AND stage=2",
            [],|r|r.get(0)).unwrap();
        assert_eq!(draft,"volume_changed_unverified");
        assert_eq!(stage,"review_required",
            "Old plan is NOT silently restored even when TP1 broker fill was proven");
    }
    #[test]
    fn tp3_requires_a_second_exact_broker_fill_and_all_original_close_lots(){
        let (d,mut t)=tp2_fixture();
        fill(&d,2);
        t.positions[0].volume=0.02;
        t.bid=Some(2003.2);t.ask=Some(2003.4);
        t.quote_time_ms=Some(10700);t.source_quote_time_ms=Some(10700);
        reconcile(&d,&t,10600);
        let v=first(&d,&t,10750);
        assert_eq!(v["stage"],3);
        assert_eq!(v["state"],"historical_prerequisites_met_shadow_only");
        assert_eq!(v["expected_close_lots"],0.02);
        assert_eq!(v["expected_position_volume"],0.02);
        assert_eq!(v["expected_remaining_volume"],0.0);
        assert_eq!(v["orders_enabled"],false);
        assert_eq!(v["next_stage_authorized"],false);
    }
    #[test]
    fn post_fill_corrupt_deal_extra_close_wrong_volume_or_old_quote_fails_closed(){
        for case in 0..6{
            let (d,mut t)=tp2_fixture();
            match case{
                0=>{d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.order',999)",
                    []).unwrap();},
                1=>{d.execute("INSERT INTO broker_commands VALUES('extra',?1,'gann',
                    'close',10230,'dispatched',?2,'{}')",
                    params![key(&t),json!({"position":87}).to_string()]).unwrap();},
                2=>{t.positions[0].volume=0.04;},
                3=>{t.source_quote_time_ms=Some(10190);t.quote_time_ms=Some(10190);},
                4=>{t.positions[0].magic=999;},
                _=>{t.positions[0].tp=2002.5;},
            }
            let v=report(&d,&t,"demo",10350).unwrap();
            if case==0 {assert!(v["rows"].as_array().unwrap().is_empty(),
                "A corrupted prior Deal invalidates the whole next-stage ladder");
            }else{
                assert_ne!(v["rows"][0]["state"],"historical_prerequisites_met_shadow_only",
                    "case {case}");
            }
            assert_eq!(v["orders_enabled"],false);
        }
    }
    #[test]
    fn moved_farther_broker_tp_or_tampered_prices_never_unlock_tp2_research(){
        let (d,mut t)=tp2_fixture();
        t.positions[0].tp=2050.;
        assert_eq!(first(&d,&t,10350)["state"],
            "residual_position_not_verified");
        t.positions[0].tp=0.;
        d.execute("UPDATE partial_exit_price_plans
            SET prices_json='[2001,2000.5,2003]'",[]).unwrap();
        assert_eq!(first(&d,&t,10350)["state"],
            "original_identity_changed");
        d.execute("UPDATE partial_exit_price_plans
            SET prices_json='[2001,2002,2003]',
                captured_json=json_remove(captured_json,'$.original_broker_tp')",
            []).unwrap();
        assert_eq!(first(&d,&t,10350)["state"],
            "original_identity_changed");
    }
    #[test]
    fn tampered_historical_residual_never_unlocks_later_research_stage(){
        let (d,t)=tp2_fixture();
        assert_eq!(first(&d,&t,10350)["state"],
            "historical_prerequisites_met_shadow_only");
        d.execute("UPDATE partial_exit_stage_settlements
            SET remaining_volume=0.07 WHERE stage=1",[]).unwrap();
        let after=report(&d,&t,"demo",10350).unwrap();
        assert!(after["rows"].as_array().unwrap().is_empty(),
            "A corrupt previous stage must no longer be historically sealed");
    }
    #[test]
    fn no_cross_account_or_real_ladder_evidence_even_with_same_position_ids(){
        let (d,t)=tp2_fixture();
        let mut foreign=t.clone();foreign.login+=1;
        assert!(report(&d,&foreign,"demo",10350).unwrap()["rows"].as_array().unwrap().is_empty());
        foreign=t.clone();foreign.demo=false;foreign.account_trade_mode=Some(2);
        assert_eq!(report(&d,&foreign,"live",10350).unwrap()["status"],
            "unverified_demo_hedging");
    }
}
