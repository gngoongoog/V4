//! P1-H: four-mode, account-scoped, broker-closed Demo evidence coverage.
 //! Not backtest equity, not Paper/Shadow, not a policy recommendation.
use anyhow::Result;
use braintrade_domain::{trading::TradingMode,Telemetry};
use rusqlite::{params,Connection};
use serde_json::{json,Value};

const MIN_CLOSED:u64=30;

fn no_account()->Value{
    json!({"schema":"p1h-four-mode-evidence-v1","state":"unavailable",
        "verified_demo_account":false,"modes":[],
        "reason":"Requires currently bound, fresh MT5 DEMO identity",
        "read_only":true,"broker_execution_permission":false,
        "automatic_promotion":false})
}

/// Caller must independently verify Core account binding and gateway freshness.
/// The four reports all use *one* MT5 server/login/symbol/currency context.
pub(crate) fn report(db:&Connection,telemetry:Option<&Telemetry>,now:u64)->Result<Value>{
    let Some(t)=telemetry.filter(|t|t.connected&&t.demo
        &&t.account_trade_mode==Some(0)&&t.login>0
        &&!t.server.is_empty()&&!t.symbol.is_empty()&&!t.currency.is_empty())
    else{return Ok(no_account())};
    let context=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))?;
    let exists:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='coordinator_broker_samples')",
        [],|r|r.get(0))?;
    // This catches the same reconciled position being attributed to two
    // independent trading modes. Don't treat duplicates as separate trades.
    let cross_mode_positions:u64=if exists{
        db.query_row(
            "SELECT COUNT(*) FROM (
                SELECT position_id FROM coordinator_broker_samples
                WHERE context=?1 AND brain='system'
                    AND CASE WHEN json_valid(payload) THEN json_extract(payload,'$.evidence_source') END='mt5_demo_broker'
                    AND CASE WHEN json_valid(payload) THEN json_extract(payload,'$.trading_mode') END IN ('sniper','scalping','swing','expert')
                GROUP BY position_id
                HAVING COUNT(DISTINCT json_extract(payload,'$.trading_mode'))>1
            )",params![context],|r|r.get(0))?
    }else{0};
    let malformed_rows:u64=if exists{
        db.query_row(
            "SELECT COUNT(*) FROM coordinator_broker_samples
             WHERE context=?1 AND brain='system' AND NOT json_valid(payload)",
            params![context],|r|r.get(0))?
    }else{0};
    let mut modes=Vec::new();
    let mut total=0u64;
    for mode in [TradingMode::Sniper,TradingMode::Scalping,TradingMode::Swing,TradingMode::Expert]{
        let value=serde_json::to_value(mode)?;
        let label=value.as_str().unwrap_or("unverified");
        // JSON1 would reject malformed payloads inside learning_quality's
        // mode-filter query. Quarantine the entire mode matrix instead:
        // corrupted account evidence is never interpreted as zero losses.
        let evidence=if malformed_rows==0{
            crate::learning_quality::audit(db,&context,mode,"system")?
        }else{json!({"broker_samples":0,"invalid_samples":0,
            "duplicate_position_samples":0,"distinct_strategy_sources":0})};
        let count=evidence["broker_samples"].as_u64().unwrap_or(0);
        total+=count;
        let invalid=evidence["invalid_samples"].as_u64().unwrap_or(0);
        let duplicates=evidence["duplicate_position_samples"].as_u64().unwrap_or(0);
        let sources=evidence["distinct_strategy_sources"].as_u64().unwrap_or(0);
        let enough=count>=MIN_CLOSED;
        let consistent=enough&&invalid==0&&duplicates==0&&sources==1
            &&cross_mode_positions==0&&malformed_rows==0;
        let reason=if cross_mode_positions>0{"cross_mode_position_collision"}
            else if malformed_rows>0{"malformed_account_broker_evidence"}
            else if invalid>0||duplicates>0{"invalid_or_duplicate_broker_samples"}
            else if !enough{"insufficient_closed_demo_deals"}
            else if sources!=1{"mixed_or_unverified_strategy_sources"}
            else{"descriptive_evidence_only"};
        modes.push(json!({
            "mode":label,"status":if consistent{"descriptive_only"}else{"insufficient_or_contaminated"},
            "reason":reason,"closed_costed_samples":count,
            "invalid_samples":invalid,"duplicate_positions":duplicates,
            "distinct_strategy_sources":sources,"minimum_samples":MIN_CLOSED,
            "net_after_costs":if count>0&&invalid==0&&duplicates==0
                &&cross_mode_positions==0&&malformed_rows==0{
                evidence["all"]["net_booked"].clone()
            }else{Value::Null},
            "win_rate_after_costs":if consistent{evidence["all"]["win_rate"].clone()}else{Value::Null},
            "recent_mean_per_lot":if consistent{evidence["recent"]["mean_net_per_lot"].clone()}else{Value::Null},
            "source":"reconciled_closed_mt5_demo_coordinator_samples",
            "sample_cap_per_mode":240,
            "online_learning_could_have_seen_these_results":true,
            "independent_oos":false,"promotion_authorized":false,
        }));
    }
    let inactive_forward=inactive_mode_forward(db,&context,now)?;
    Ok(json!({
        "schema":"p1h-four-mode-evidence-v1","state":"observed",
        "verified_demo_account":true,"observed_ms":now,
        "context":context,
        "account":{"login":t.login,"server":t.server,"symbol":t.symbol,
            "currency":t.currency,"mode":0},
        "modes":modes,"inactive_forward":inactive_forward,
        "total_attributed_samples_not_independent":total,
        "cross_mode_position_collisions":cross_mode_positions,
        "malformed_broker_samples":malformed_rows,
        "minimum_samples_per_mode":MIN_CLOSED,"sample_cap_per_mode":240,
        "read_only":true,"broker_execution_permission":false,
        "automatic_promotion":false,"independent_oos":false,
        "caveat":"Only closed Demo coordinator samples, by exact account, symbol and trading mode. These are online-trained historical results, NOT frozen policy forward-OOS, true-tick backtests or permission to trade REAL."
    }))
}

/// Shadow observations are prospective candle estimates, not booked MT5
/// PnL, not a broker trade, and not independent frozen/OOS proof. Scope to
/// exact Demo account context and source tuple mode, never fuzzy SQL LIKE.
fn inactive_mode_forward(db:&Connection,context:&str,now:u64)->Result<Value>{
    let exists:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='coordinator_samples')",
        [],|r|r.get(0))?;
    let mut modes=Vec::new();
    for mode in [TradingMode::Sniper,TradingMode::Scalping,TradingMode::Swing,TradingMode::Expert] {
        let label=serde_json::to_value(mode)?.as_str().unwrap_or("").to_owned();
        let (count,scored,last):(u64,u64,Option<u64>)=if exists && mode!=TradingMode::Sniper {
            db.query_row(
                "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN
                        (CASE WHEN json_valid(outcome) THEN json_extract(outcome,'$.state') END)='scored'
                        THEN 1 ELSE 0 END),0),
                    MAX(CASE WHEN json_valid(payload)
                        THEN CAST(json_extract(payload,'$.observed_ms') AS INTEGER) END)
                FROM coordinator_samples
                WHERE CASE WHEN json_valid(source) THEN json_extract(source,'$[0]') END=?1
                    AND CASE WHEN json_valid(source) THEN json_extract(source,'$[1]') END='inactive-mode-forward-v1'
                    AND CASE WHEN json_valid(source) THEN json_extract(source,'$[2]') END=?2
                    AND CASE WHEN json_valid(payload) THEN
                        json_extract(payload,'$.observation_origin') END='inactive_mode_forward'
                    AND CASE WHEN json_valid(payload) THEN
                        json_extract(payload,'$.study_kind') END=
                        CASE WHEN ?2='expert' THEN 'inactive_expert_advisor_signal_not_consensus'
                        ELSE 'inactive_primary_mode_signal' END
                    AND (CASE WHEN json_valid(source) THEN json_extract(source,'$[3]') END)
                        IS NOT NULL
                    AND ((?2='expert' AND
                        (CASE WHEN json_valid(source) THEN json_extract(source,'$[3]') END)
                            NOT IN ('system','sniper_learner'))
                      OR (?2 IN ('scalping','swing') AND
                        (CASE WHEN json_valid(source) THEN json_extract(source,'$[3]') END)='system'))",
                params![context,label],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))
            )?
        } else {(0,0,None)};
        modes.push(json!({
            "mode":label,
            "status":if mode==TradingMode::Sniper {"tick_native_not_supported"}
                else if count==0 {"no_prospective_samples"}
                else if last.is_some_and(|ms|now.saturating_sub(ms)<=300_000)
                    {"recent_historical_observation"}else{"historical_observation_stale"},
            "samples":count,"scored_after_future_bars":scored,
            "last_observed_ms":last,
            "horizon":match mode{
                TradingMode::Sniper=>"tick_native_not_studied",
                TradingMode::Scalping=>"12_completed_M1_bars_approx_12_minutes",
                TradingMode::Swing=>"12_completed_H4_bars_approx_48_hours",
                TradingMode::Expert=>"12_completed_advisor_timeframe_bars",
            },
            "source":if mode==TradingMode::Sniper {"not_studied_from_candle_snapshots"}
                else{"inactive_mode_forward_on_bound_demo_completed_bars"},
            "broker_orders":false,"demo_broker_fills":false,
            "independent_frozen_oos":false,"promoted_to_execution":false
        }));
    }
    Ok(json!({"modes":modes,"source":"prospective_inactive_mode_candle_forward_only",
        "scope":"exact_current_demo_account_symbol_not_real_account",
        "training_note":"Forward samples are matured with later completed bars and isolated in inactive-mode-forward-v1; not broker fills, tick-native Sniper, frozen OOS proof or automatic execution model promotion.",
        "broker_orders":false,"can_promote_to_live":false}))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn db()->Connection{
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE coordinator_broker_samples(source TEXT NOT NULL,
            position_id INTEGER NOT NULL,context TEXT NOT NULL,brain TEXT NOT NULL,
            closed_ms INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source,position_id));").unwrap();db
    }
    fn telemetry(demo:bool)->Telemetry{
        Telemetry{connected:true,demo,account_trade_mode:Some(if demo{0}else{2}),
            login:17,server:"DemoServer".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),..Default::default()}
    }
    fn add(db:&Connection,t:&Telemetry,mode:&str,source:&str,pos:u64,net:f64,kind:&str){
        let context=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap();
        let value=json!({"evidence_source":kind,"trading_mode":mode,
            "context":context,"brain":"system","position_id":pos,
            "closed_ms":pos*1000,"gross":net+0.70,
            "commission":-0.70,"swap":0.0,"fee":0.0,"net":net,
            "closed_volume":0.1,"net_per_lot":net/0.1,
            "risk_money":5.0,"r_multiple":net/5.0,
            "reward":(net/5.0).tanh()});
        db.execute("INSERT INTO coordinator_broker_samples VALUES(?1,?2,?3,'system',?4,?5)",
            params![source,pos,context,pos*1000,value.to_string()]).unwrap();
    }
    #[test]
    fn four_modes_are_distinct_and_only_closed_costed_demo_is_descriptive(){
        let db=db();let t=telemetry(true);
        for idx in 1..=35u64{
            add(&db,&t,"sniper","stable-sniper",idx,1.0,"mt5_demo_broker");
            add(&db,&t,"scalping","stable-scalp",idx+100,2.0,"mt5_demo_broker");
        }
        add(&db,&t,"swing","paper",200,9999.,"paper");
        let mut unrelated=t.clone();unrelated.login=18;
        add(&db,&unrelated,"expert","other-account",300,9999.,"mt5_demo_broker");
        let v=report(&db,Some(&t),1234).unwrap();
        assert_eq!(v["state"],"observed");
        assert_eq!(v["modes"].as_array().unwrap().len(),4);
        assert_eq!(v["modes"][0]["mode"],"sniper");
        assert_eq!(v["modes"][0]["closed_costed_samples"],35);
        assert_eq!(v["modes"][0]["status"],"descriptive_only");
        assert_eq!(v["modes"][0]["net_after_costs"],35.);
        assert_eq!(v["modes"][1]["mode"],"scalping");
        assert_eq!(v["modes"][1]["closed_costed_samples"],35);
        assert_eq!(v["modes"][2]["closed_costed_samples"],0);
        assert_eq!(v["modes"][2]["win_rate_after_costs"],Value::Null);
        assert_eq!(v["modes"][3]["closed_costed_samples"],0);
        assert_eq!(v["independent_oos"],false);
        assert_eq!(v["automatic_promotion"],false);
    }
    #[test]
    fn inactive_mode_forward_samples_are_scoped_and_never_counted_as_demo_broker_fills(){
        let db=db();let t=telemetry(true);
        db.execute_batch("CREATE TABLE coordinator_samples(source TEXT NOT NULL,
            bar INTEGER NOT NULL,payload TEXT NOT NULL,outcome TEXT,
            PRIMARY KEY(source,bar));").unwrap();
        let key=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap();
        let other=serde_json::to_string(&(&t.server,999u64,&t.symbol,&t.currency)).unwrap();
        for (context,mode,bar,origin,outcome) in [
            (key.as_str(),"swing",110u64,"inactive_mode_forward",Some(json!({"state":"scored","net":3.}))),
            (key.as_str(),"swing",120u64,"inactive_mode_forward",None),
            (key.as_str(),"expert",100u64,"active_mode_forward",Some(json!({"state":"scored"}))),
            (other.as_str(),"swing",130u64,"inactive_mode_forward",Some(json!({"state":"scored"}))),
        ]{
            let (member,timeframe)=if mode=="expert" {("trend","M1")} else {("system","H4")};
            let source=serde_json::to_string(&(context,
                if origin=="active_mode_forward" {"adaptive-member-v1"}else{"inactive-mode-forward-v1"},
                mode,member,timeframe,"")).unwrap();
            let kind=if mode=="expert" {"inactive_expert_advisor_signal_not_consensus"}
                else{"inactive_primary_mode_signal"};
            let value=json!({"observation_origin":origin,"study_kind":kind,
                "observed_ms":bar*1000u64,"live_broker_orders":false});
            db.execute("INSERT INTO coordinator_samples VALUES(?1,?2,?3,?4)",
                params![source,bar,value.to_string(),outcome.map(|v|v.to_string())]).unwrap();
        }
        let v=report(&db,Some(&t),125_000).unwrap();
        assert_eq!(v["inactive_forward"]["modes"][2]["mode"],"swing");
        assert_eq!(v["inactive_forward"]["modes"][2]["samples"],2);
        assert_eq!(v["inactive_forward"]["modes"][2]["scored_after_future_bars"],1);
        assert_eq!(v["inactive_forward"]["modes"][2]["broker_orders"],false);
        assert_eq!(v["inactive_forward"]["modes"][3]["samples"],0,
            "Current active-mode observations must never masquerade as background learning");
        // Changing the declared study kind cannot make a Trend advisor a
        // valid primary-mode observation (nor vice versa).
        let wrong=serde_json::to_string(&(key.as_str(),"inactive-mode-forward-v1","scalping","trend","M1","")).unwrap();
        let fake=json!({"observation_origin":"inactive_mode_forward",
            "study_kind":"inactive_primary_mode_signal","observed_ms":125000,
            "live_broker_orders":false}).to_string();
        db.execute("INSERT INTO coordinator_samples VALUES(?1,125,?2,NULL)",
            params![wrong,fake]).unwrap();
        let checked=report(&db,Some(&t),125_000).unwrap();
        assert_eq!(checked["inactive_forward"]["modes"][1]["samples"],0,
            "A secondary advisor cannot impersonate Scalping's primary entry policy");
        db.execute("INSERT INTO coordinator_samples VALUES('not JSON',126,'{}',NULL)",[]).unwrap();
        assert!(report(&db,Some(&t),125_000).is_ok(),
            "Malformed research source must not break account mode evidence");
        assert_eq!(v["inactive_forward"]["modes"][0]["status"],"tick_native_not_supported");
        assert_eq!(v["modes"][2]["closed_costed_samples"],0,
            "Forward candle outcomes must not become booked MT5 Demo fills");
    }
    #[test]
    fn cross_mode_same_position_rejects_all_modes_as_independent_evidence(){
        let db=db();let t=telemetry(true);
        for idx in 1..=30u64{
            add(&db,&t,"sniper","sniper-policy",idx,1.,"mt5_demo_broker");
            add(&db,&t,"scalping","scalp-policy",idx+100,1.,"mt5_demo_broker");
        }
        add(&db,&t,"expert","another-source",1,1.,"mt5_demo_broker");
        let v=report(&db,Some(&t),55).unwrap();
        assert_eq!(v["cross_mode_position_collisions"],1);
        assert_eq!(v["modes"][0]["status"],"insufficient_or_contaminated");
        assert_eq!(v["modes"][0]["win_rate_after_costs"],Value::Null);
        assert_eq!(v["modes"][0]["reason"],"cross_mode_position_collision");
    }
    #[test]
    fn malformed_broker_payload_is_quarantined_without_sqlite_json_error(){
        let db=db();let t=telemetry(true);
        let context=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap();
        db.execute("INSERT INTO coordinator_broker_samples VALUES('bad',1,?1,'system',1,'{not valid json')",
            params![context]).unwrap();
        let r=report(&db,Some(&t),100).unwrap();
        assert_eq!(r["state"],"observed");
        assert_eq!(r["malformed_broker_samples"],1);
        assert_eq!(r["modes"][0]["reason"],"malformed_account_broker_evidence");
        assert_eq!(r["modes"][0]["win_rate_after_costs"],Value::Null);
        assert_eq!(r["modes"][0]["net_after_costs"],Value::Null);
    }
    #[test]
    fn malformed_or_mixed_policy_fails_closed_and_never_uses_real_or_stale_account(){
        let db=db();let mut t=telemetry(true);
        for idx in 1..=31u64{
            add(&db,&t,"expert",if idx<=15{"revision-v1"}else{"revision-v2"},
                idx,1.,"mt5_demo_broker");
        }
        let v=report(&db,Some(&t),55).unwrap();
        assert_eq!(v["modes"][3]["reason"],"mixed_or_unverified_strategy_sources");
        assert_eq!(v["modes"][3]["win_rate_after_costs"],Value::Null);
        assert_eq!(report(&db,None,55).unwrap()["state"],"unavailable");
        t.demo=false;t.account_trade_mode=Some(2);
        assert_eq!(report(&db,Some(&t),55).unwrap()["modes"].as_array().unwrap().len(),0);
        t.demo=true;t.account_trade_mode=Some(0);t.connected=false;
        assert_eq!(report(&db,Some(&t),55).unwrap()["verified_demo_account"],false);
    }
}
