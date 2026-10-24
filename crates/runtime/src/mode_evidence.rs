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
    Ok(json!({
        "schema":"p1h-four-mode-evidence-v1","state":"observed",
        "verified_demo_account":true,"observed_ms":now,
        "context":context,
        "account":{"login":t.login,"server":t.server,"symbol":t.symbol,
            "currency":t.currency,"mode":0},
        "modes":modes,"total_attributed_samples_not_independent":total,
        "cross_mode_position_collisions":cross_mode_positions,
        "malformed_broker_samples":malformed_rows,
        "minimum_samples_per_mode":MIN_CLOSED,"sample_cap_per_mode":240,
        "read_only":true,"broker_execution_permission":false,
        "automatic_promotion":false,"independent_oos":false,
        "caveat":"Only closed Demo coordinator samples, by exact account, symbol and trading mode. These are online-trained historical results, NOT frozen policy forward-OOS, true-tick backtests or permission to trade REAL."
    }))
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
        let value=json!({"evidence_source":kind,"trading_mode":mode,"gross":net+0.70,
            "commission":-0.70,"swap":0.0,"fee":0.0,"net":net,"closed_volume":0.1});
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
