//! Sampled Demo Sniper decisions and a prospective five-second quote comparison.
//! No orders, training, policy promotion, inferred fills or reconstructed tick paths.
use anyhow::Result;
use braintrade_domain::{trading::TradingMode, Config, Telemetry};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const INTERVAL_MS: u64 = 1_000;
const HORIZON_MS: u64 = 5_000;
const LATENESS_MS: u64 = 1_500;
const KEEP: i64 = 4_000;
const REPORT: i64 = 800;

#[derive(Serialize, Deserialize)]
struct Study {
    side: i64,
    entry: f64,
    tick: f64,
    profit_value: f64,
    loss_value: f64,
    commission: f64,
    slippage: f64,
    entry_quote_ms: u64,
    status: String,
    elapsed_ms: Option<u64>,
    exit_quote_ms: Option<u64>,
    net_per_lot: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    observed_ms: u64,
    signal_key: String,
    signal_ready: bool,
    entry_ready: bool,
    category: String,
    reason: String,
    signal_reason: String,
    gates: Vec<String>,
    style: String,
    definition: String,
    policy: String,
    regime: String,
    command_id: Option<String>,
    study: Option<Study>,
}

#[derive(Default)]
pub struct Recorder {
    context: String,
    last_ms: Option<u64>,
}

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS sniper_opportunity_samples(
          context TEXT NOT NULL,observed_ms INTEGER NOT NULL,
          pending INTEGER NOT NULL,payload TEXT NOT NULL,
          PRIMARY KEY(context,observed_ms));
         CREATE INDEX IF NOT EXISTS sniper_opportunity_pending
          ON sniper_opportunity_samples(context,pending,observed_ms);",
    )?;
    Ok(())
}

fn bounded(v: &Value, cap: usize) -> String {
    v.as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(cap).collect()
}

fn category(reason: &str, ready: bool) -> &'static str {
    if ready { return "entry_candidate"; }
    // Categorize the *first* explicit gate (or signal reason if there is no gate).
    // Preserve every raw gate separately; these categories are diagnostic, not logic.
    if reason.contains("معالجة هذه الإشارة") { "duplicate" }
    else if reason.contains("المصالحة") || reason.contains("تسويته") || reason.contains("قيد التنفيذ") { "reconciliation" }
    else if reason.contains("صفقة مفتوحة") || reason.contains("مملوك لصفقة") { "position_open" }
    else if reason.contains("غير مسموح") || reason.contains("SHADOW") || reason.contains("PAPER") || reason.contains("مشارك") { "permission" }
    else if reason.contains("فاصل الدخول") { "cooldown" }
    else if reason.contains("متوقف") { "stopped" }
    else if reason.contains("التكلفة") || reason.contains("السبريد") || reason.contains("spread") { "cost" }
    else if reason.contains("خسارة") || reason.contains("خسائر") || reason.contains("المخاطرة") || reason.contains("المحفظة") || reason.contains("لوت") { "risk" }
    else if reason.contains("بيانات") || reason.contains("سعراً حديثاً") || reason.contains("أسعار متتابعة") { "data" }
    else { "signal_wait" }
}

fn label(code: &str) -> &'static str {
    match code {
        "entry_candidate" => "إشارة مؤهلة عند التقييم",
        "duplicate" => "إشارة سبق التعامل معها",
        "reconciliation" => "أمر ينتظر التنفيذ أو المصالحة",
        "position_open" => "صفقة مفتوحة أو ملكية الرمز",
        "permission" => "إذن التداول أو مشاركة العقل",
        "cooldown" => "فاصل الدخول",
        "stopped" => "النظام متوقف",
        "cost" => "السبريد أو التكلفة",
        "risk" => "ميزانية التداول أو المخاطرة",
        "data" => "انتظار سعر أو بيانات مكتملة",
        _ => "شروط الإشارة أو سبب آخر",
    }
}

fn quote(t: &Telemetry, now: u64) -> Option<(f64, f64, u64)> {
    let (bid, ask, time) = (t.bid?, t.ask?, t.quote_time_ms?);
    if !bid.is_finite() || !ask.is_finite() || bid <= 0. || ask < bid
        || time == 0 || time > now || now - time > 1_000
        || t.tick_age_ms.is_none_or(|age| age > 1_000) { return None; }
    Some((bid, ask, time))
}

fn study(t: &Telemetry, c: &Config, signal: &Value, now: u64) -> Option<Study> {
    let side = signal["side"].as_i64()?;
    if signal["ready"] != true || ![-1, 1].contains(&side) { return None; }
    let (bid, ask, time) = quote(t, now)?;
    let (tick, profit, loss) = (t.tick_size?, t.tick_value_profit?, t.tick_value_loss?);
    if [tick, profit, loss].iter().any(|v| !v.is_finite() || *v <= 0. || *v > 1e9)
        || !c.risk.slippage_ticks.is_finite() || !(0.0..=1000.).contains(&c.risk.slippage_ticks) { return None; }
    let commission = braintrade_domain::commission::round_trip_charge(c.risk.commission_per_lot, 1.).ok()?;
    Some(Study {
        side, entry: if side == 1 { ask } else { bid }, tick,
        profit_value: profit, loss_value: loss, commission,
        slippage: 2. * c.risk.slippage_ticks * profit.max(loss),
        entry_quote_ms: time, status: "pending".into(),
        elapsed_ms: None, exit_quote_ms: None, net_per_lot: None,
    })
}

fn finish(r: &mut Record, t: &Telemetry, now: u64) {
    let Some(s) = r.study.as_mut().filter(|s| s.status == "pending") else { return; };
    let elapsed = now.saturating_sub(r.observed_ms);
    if elapsed > HORIZON_MS + LATENESS_MS {
        s.status = "missing_endpoint".into();
        return;
    }
    if elapsed < HORIZON_MS { return; }
    let Some((bid, ask, time)) = quote(t, now) else { return; };
    if time <= s.entry_quote_ms { return; }
    if t.tick_size != Some(s.tick) {
        s.status = "contract_changed".into();
        return;
    }
    let ticks = (if s.side == 1 { bid } else { ask } - s.entry) * s.side as f64 / s.tick;
    let net = ticks * if ticks >= 0. { s.profit_value } else { s.loss_value }
        - s.commission - s.slippage;
    if !net.is_finite() || net.abs() > 1e12 {
        s.status = "invalid_result".into();
        return;
    }
    s.net_per_lot = Some(net);
    s.elapsed_ms = Some(elapsed);
    s.exit_quote_ms = Some(time);
    s.status = "observed".into();
}

impl Recorder {
    /// Called only after normal snapshot reconciliation and portfolio handling.
    /// Fast Pulse never calls this. Errors are isolated by the caller.
    pub fn observe(&mut self, db: &Connection, t: &Telemetry, c: &Config,
        run: bool, decision: Option<&Value>, now: u64) -> Result<()> {
        if !t.connected || !t.demo || t.account_trade_mode != Some(0)
            || t.login == 0 || t.server.is_empty() || t.currency.is_empty()
            || t.symbol != c.symbol { return Ok(()); }
        let key = crate::coordinator_learning::context(t);
        if self.context == key && self.last_ms.is_some_and(|last| now.saturating_sub(last) < INTERVAL_MS) {
            return Ok(());
        }
        self.context = key.clone();
        self.last_ms = Some(now);
        let tx = db.unchecked_transaction()?;
        let pending = {
            let mut stmt = tx.prepare("SELECT payload FROM sniper_opportunity_samples
                WHERE context=?1 AND pending=1 ORDER BY observed_ms LIMIT 32")?;
            let rows = stmt.query_map([&key], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for raw in pending {
            let mut r: Record = serde_json::from_str(&raw)?;
            finish(&mut r, t, now);
            let pending = r.study.as_ref().is_some_and(|s| s.status == "pending");
            if !pending {
                tx.execute("UPDATE sniper_opportunity_samples SET pending=0,payload=?3
                    WHERE context=?1 AND observed_ms=?2", params![key,r.observed_ms,serde_json::to_string(&r)?])?;
            }
        }
        if run && c.trading.mode == TradingMode::Sniper {
            if let Some(d) = decision.filter(|d| d["evaluated_ms"].as_u64() == Some(now)) {
                let signal = &d["signal"];
                let gates: Vec<String> = d["gates"].as_array().into_iter().flatten()
                    .take(20).map(|v| bounded(v, 240)).collect();
                let entry_ready = d["ready"] == true;
                let reason = gates.first().cloned().unwrap_or_else(|| bounded(&signal["reason"], 320));
                let signal_key = bounded(&signal["key"], 160);
                // Exact same evaluation, context, owner and signal only. Do not
                // attribute a previous pulse order to a later blocked sample.
                let command_id: Option<String> = if entry_ready && !signal_key.is_empty() {
                    tx.query_row("SELECT id FROM broker_commands WHERE context=?1
                        AND brain='system' AND kind='open' AND created_ms=?2
                        AND json_extract(payload,'$.trading_mode')='sniper'
                        AND json_extract(payload,'$.target')='demo'
                        AND json_extract(payload,'$.signal.key')=?3
                        ORDER BY id LIMIT 1", params![key,now,signal_key], |r| r.get(0)).optional()?
                } else { None };
                let selection = &signal["coordination"]["selection"];
                let r = Record {
                    observed_ms: now, signal_key, signal_ready: signal["ready"] == true,
                    entry_ready, category: category(&reason, entry_ready).into(), reason, gates,
                    signal_reason: bounded(&signal["reason"], 320),
                    style: serde_json::to_value(c.trading.sniper_style)?.as_str().unwrap_or("").into(),
                    // Include Classic/Learner and all cost/target settings even
                    // for waiting signals which have no selection metadata.
                    definition: json!([c.trading.sniper_brain,
                        braintrade_domain::sniper::definition(t,c)]).to_string(),
                    policy: bounded(&selection["sniper_policy"], 80),
                    regime: bounded(&selection["sniper_regime"], 80),
                    command_id, study: study(t, c, signal, now),
                };
                tx.execute("INSERT OR IGNORE INTO sniper_opportunity_samples VALUES(?1,?2,?3,?4)",
                    params![key,now,r.study.is_some(),serde_json::to_string(&r)?])?;
            }
        }
        tx.execute("DELETE FROM sniper_opportunity_samples WHERE context=?1 AND observed_ms NOT IN
            (SELECT observed_ms FROM sniper_opportunity_samples WHERE context=?1 ORDER BY observed_ms DESC LIMIT ?2)",
            params![key,KEEP])?;
        tx.commit()?;
        Ok(())
    }
}

#[derive(Default)]
struct Group { observations: u64, observed: u64, positive: u64, pending: u64, unknown: u64, sum: f64 }

pub fn report(db: &Connection, context: &str) -> Result<Value> {
    report_at(db,context,crate::now_ms())
}

fn report_at(db: &Connection, context: &str, now: u64) -> Result<Value> {
    let mut stmt = db.prepare("SELECT s.payload,c.state FROM sniper_opportunity_samples s
        LEFT JOIN broker_commands c ON c.id=CASE WHEN json_valid(s.payload)
          THEN json_extract(s.payload,'$.command_id') END
          AND c.context=s.context AND c.brain='system' AND c.kind='open'
        WHERE s.context=?1 ORDER BY s.observed_ms DESC LIMIT ?2")?;
    let rows = stmt.query_map(params![context,REPORT], |r| Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut groups = BTreeMap::<(String,String,String,String,String),Group>::new();
    let mut commands = BTreeSet::new();
    let mut recent = Vec::new();
    let mut blocked = 0usize;
    let mut invalid = 0usize;
    let mut count = 0usize;
    let mut range = (None, None);
    for (raw,state) in rows {
        let Ok(mut r) = serde_json::from_str::<Record>(&raw) else { invalid += 1; continue; };
        // A disconnected/stopped feed must not leave expired studies looking
        // as if an endpoint could still arrive. This view writes nothing.
        if let Some(s) = r.study.as_mut().filter(|s| s.status == "pending") {
            if now > r.observed_ms.saturating_add(HORIZON_MS+LATENESS_MS) {
                s.status = "missing_endpoint".into();
            }
        }
        count += 1;
        range.0 = Some(r.observed_ms);
        if range.1.is_none() { range.1 = Some(r.observed_ms); }
        blocked += usize::from(!r.entry_ready);
        if state.is_some() { if let Some(id) = &r.command_id { commands.insert(id.clone()); } }
        let g = groups.entry((r.category.clone(),r.style.clone(),r.definition.clone(),r.policy.clone(),r.regime.clone())).or_default();
        g.observations += 1;
        if let Some(s) = &r.study {
            match s.status.as_str() {
                "observed" => {
                    if let Some(net) = s.net_per_lot.filter(|n| n.is_finite()) {
                        g.observed += 1; g.positive += u64::from(net > 0.); g.sum += net;
                    } else { g.unknown += 1; }
                }
                "pending" => { g.pending += 1; }
                _ => { g.unknown += 1; }
            }
        }
        if recent.len() < 12 {
            recent.push(json!({"observed_ms":r.observed_ms,"reason":r.reason,"gates":r.gates,
                "signal_reason":r.signal_reason,
                "category":r.category,"label_ar":label(&r.category),"entry_ready":r.entry_ready,
                "signal_ready":r.signal_ready,"style":r.style,"policy":r.policy,
                "command_id":r.command_id,"command_state":state,
                "study":r.study}));
        }
    }
    let mut out: Vec<Value> = groups.into_iter().map(|((category,style,definition,policy,regime),g)| json!({
        "label_ar":label(&category),"category":category,"style":style,"definition":definition,
        "policy":policy,"regime":regime,"observations":g.observations,
        "endpoint_observed":g.observed,"endpoint_positive":g.positive,
        "pending":g.pending,"unknown_endpoint":g.unknown,
        "no_directional_study":g.observations-g.observed-g.pending-g.unknown,
        "mean_hypothetical_net_per_lot":if g.observed>0 {Some(g.sum/g.observed as f64)} else {None},
    })).collect();
    out.sort_by(|a,b| b["observations"].as_u64().cmp(&a["observations"].as_u64())
        .then_with(|| a.to_string().cmp(&b.to_string())));
    let group_count = out.len();
    out.truncate(16);
    Ok(json!({"status":if context.is_empty(){"unbound"}else if count==0{"no_samples"}else{"sampled_observations_only"},
        "policy":"sniper_opportunity_observation_v1","sampled_observations":count,
        "not_entry_ready_observations":blocked,"linked_unique_commands":commands.len(),
        "invalid_rows":invalid,"first_ms":range.0,"last_ms":range.1,
        "minimum_sample_interval_ms":INTERVAL_MS,"horizon_ms":HORIZON_MS,
        "maximum_endpoint_lateness_ms":LATENESS_MS,"report_cap":REPORT,"retention_per_context":KEEP,
        "distinct_groups":group_count,"groups":out,"recent":recent,
        "account_scoped":true,"mode_scoped":"sniper","demo_only":true,
        "no_execution_effect":true,"learning_effect":false,"promotion_authorized":false,
        "note":"Sampled normal snapshots, not all pulses, unique signals, missed fills or independent trades. Primary reason is first gate; all gates remain in recent rows. Five-second executable-side quote comparison uses frozen commission and estimated two-sided slippage, no swap/fill/path inference. Unknown endpoints are not wins or losses. Grouped by reason/style/definition/policy/regime; selected ready directions only, not an unbiased strategy test."}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(db:&Connection,key:&str)->Result<Value>{super::report_at(db,key,10_000)}
    fn fixture() -> (Connection,Telemetry,Config,Value) {
        let db = Connection::open_in_memory().unwrap(); init(&db).unwrap();
        db.execute_batch("CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,created_ms INTEGER,state TEXT,payload TEXT);").unwrap();
        let mut c = Config::default(); c.trading.mode = TradingMode::Sniper;
        c.risk.commission_per_lot = 7.; c.risk.slippage_ticks = 1.;
        let t = Telemetry { connected:true,demo:true,account_trade_mode:Some(0),login:17,
            server:"Demo".into(),currency:"USD".into(),symbol:c.symbol.clone(),
            bid:Some(100.),ask:Some(100.2),tick_size:Some(0.1),tick_value_profit:Some(2.),
            tick_value_loss:Some(3.),tick_age_ms:Some(0),quote_time_ms:Some(10_000),..Default::default() };
        let d = json!({"evaluated_ms":10_000,"ready":false,"gates":["للعقل صفقة مفتوحة"],
            "signal":{"key":"tick:10000","ready":true,"side":1,"reason":"جاهز",
                "coordination":{"selection":{"sniper_definition":"v1","sniper_policy":"impulse","sniper_regime":"trend"}}}});
        (db,t,c,d)
    }
    #[test]
    fn prospective_endpoint_includes_spread_fees_and_frozen_costs() {
        let (db,mut t,mut c,d) = fixture(); let mut rec = Recorder::default();
        let key = crate::coordinator_learning::context(&t);
        rec.observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        // A change of configured fees cannot rewrite the original quote study.
        c.risk.commission_per_lot = 500.; t.bid=Some(101.2); t.ask=Some(101.4); t.quote_time_ms=Some(15_000);
        rec.observe(&db,&t,&c,false,None,15_000).unwrap();
        let r = report(&db,&key).unwrap(); let g=&r["groups"][0];
        // Buy at ask 100.2, compare at bid 101.2: 10*2 - 7 - 2*1*3 = 7.
        assert!((g["mean_hypothetical_net_per_lot"].as_f64().unwrap()-7.).abs()<1e-8);
        assert_eq!(g["endpoint_positive"],1); assert_eq!(g["category"],"position_open");
        assert_eq!(r["linked_unique_commands"],0); assert_eq!(r["learning_effect"],false);
        assert_eq!(r["no_execution_effect"],true);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],|r|r.get::<_,u64>(0)).unwrap(),0);
    }
    #[test]
    fn sell_loss_uses_loss_value_and_ask_and_missing_endpoint_is_unknown() {
        let (db,mut t,c,mut d)=fixture(); let mut rec=Recorder::default();
        let key=crate::coordinator_learning::context(&t); d["signal"]["side"]=json!(-1);
        rec.observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        t.bid=Some(100.8); t.ask=Some(101.); t.quote_time_ms=Some(15_000);
        rec.observe(&db,&t,&c,false,None,15_000).unwrap();
        assert!((report(&db,&key).unwrap()["groups"][0]["mean_hypothetical_net_per_lot"].as_f64().unwrap()+43.).abs()<1e-8);
        d["evaluated_ms"]=json!(20_000); t.quote_time_ms=Some(20_000);
        rec.observe(&db,&t,&c,true,Some(&d),20_000).unwrap();
        t.quote_time_ms=Some(27_000);
        rec.observe(&db,&t,&c,false,None,27_000).unwrap();
        let g=&report(&db,&key).unwrap()["groups"][0];
        assert_eq!(g["unknown_endpoint"],1); assert_eq!(g["endpoint_observed"],1);
    }
    #[test]
    fn throttle_demo_mode_account_and_current_decision_isolation() {
        let (db,mut t,mut c,mut d)=fixture(); let mut rec=Recorder::default();
        let key=crate::coordinator_learning::context(&t);
        rec.observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        rec.observe(&db,&t,&c,true,Some(&d),10_100).unwrap();
        t.account_trade_mode=Some(2); d["evaluated_ms"]=json!(11_000);
        rec.observe(&db,&t,&c,true,Some(&d),11_000).unwrap();
        t.account_trade_mode=Some(0); c.trading.mode=TradingMode::Swing;
        rec.observe(&db,&t,&c,true,Some(&d),12_000).unwrap();
        c.trading.mode=TradingMode::Sniper;
        rec.observe(&db,&t,&c,true,Some(&d),13_000).unwrap(); // stale decision
        t.login=18; t.quote_time_ms=Some(15_000);
        rec.observe(&db,&t,&c,false,None,15_000).unwrap();
        let r=report(&db,&key).unwrap(); assert_eq!(r["sampled_observations"],1);
        assert_eq!(r["groups"][0]["pending"],1);
        assert_eq!(report(&db,&crate::coordinator_learning::context(&t)).unwrap()["sampled_observations"],0);
    }
    #[test]
    fn future_stale_repeated_quotes_and_changed_contract_cannot_be_results() {
        let (db,mut t,c,d)=fixture(); let mut rec=Recorder::default();
        let key=crate::coordinator_learning::context(&t);
        rec.observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        rec.observe(&db,&t,&c,false,None,15_000).unwrap(); // old timestamp
        t.quote_time_ms=Some(17_000);
        rec.observe(&db,&t,&c,false,None,16_000).unwrap(); // future quote
        assert_eq!(report(&db,&key).unwrap()["groups"][0]["pending"],1);
        rec.observe(&db,&t,&c,false,None,17_000).unwrap();
        let r=report(&db,&key).unwrap(); assert_eq!(r["groups"][0]["unknown_endpoint"],1);
        assert_eq!(r["groups"][0]["mean_hypothetical_net_per_lot"],Value::Null);
        let mut record:Record=serde_json::from_str(&db.query_row("SELECT payload FROM sniper_opportunity_samples",[],|r|r.get::<_,String>(0)).unwrap()).unwrap();
        record.study.as_mut().unwrap().status="pending".into();
        t.quote_time_ms=Some(15_000); t.tick_size=Some(0.2); finish(&mut record,&t,15_000);
        assert_eq!(record.study.unwrap().status,"contract_changed");
    }
    #[test]
    fn exact_command_link_does_not_claim_an_old_pulse_order() {
        let (db,t,c,mut d)=fixture(); let mut rec=Recorder::default();
        let key=crate::coordinator_learning::context(&t);
        let p=json!({"trading_mode":"sniper","target":"demo","signal":{"key":"tick:10000"}}).to_string();
        db.execute("INSERT INTO broker_commands VALUES('old',?1,'system','open',9999,'done',?2)",params![key,p]).unwrap();
        d["ready"]=json!(true); d["gates"]=json!([]);
        rec.observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        assert_eq!(report(&db,&key).unwrap()["linked_unique_commands"],0);
        db.execute("DELETE FROM sniper_opportunity_samples",[]).unwrap();
        db.execute("INSERT INTO broker_commands VALUES('exact',?1,'system','open',10000,'cancelled',?2)",params![key,p]).unwrap();
        Recorder::default().observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        let r=report(&db,&key).unwrap(); assert_eq!(r["linked_unique_commands"],1);
        assert_eq!(r["recent"][0]["command_state"],"cancelled");
        assert_eq!(r["recent"][0]["command_id"],"exact");
    }
    #[test]
    fn waiting_signals_have_reasons_but_no_invented_direction_or_profit() {
        let (db,t,c,mut d)=fixture();
        d["signal"]["ready"]=json!(false); d["signal"]["side"]=json!(0);
        d["signal"]["reason"]=json!("القناص: الهدف لا يجتاز حصة التكلفة"); d["gates"]=json!([]);
        Recorder::default().observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        let r=report(&db,&crate::coordinator_learning::context(&t)).unwrap();
        assert_eq!(r["groups"][0]["category"],"cost");
        assert_eq!(r["groups"][0]["no_directional_study"],1);
        assert_eq!(r["groups"][0]["mean_hypothetical_net_per_lot"],Value::Null);
        assert_eq!(report(&db,"").unwrap()["status"],"unbound");
    }
    #[test]
    fn retention_is_bounded_and_read_only_view_expires_missing_feed() {
        let (db,t,c,d)=fixture(); let key=crate::coordinator_learning::context(&t);
        Recorder::default().observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        let expired=super::report_at(&db,&key,20_000).unwrap();
        assert_eq!(expired["groups"][0]["unknown_endpoint"],1);
        assert_eq!(expired["recent"][0]["study"]["status"],"missing_endpoint");
        assert_eq!(db.query_row("SELECT pending FROM sniper_opportunity_samples",[],|r|r.get::<_,u64>(0)).unwrap(),1);
        let raw:String=db.query_row("SELECT payload FROM sniper_opportunity_samples",[],|r|r.get(0)).unwrap();
        let tx=db.unchecked_transaction().unwrap();
        for time in 1..=KEEP+3 {
            tx.execute("INSERT INTO sniper_opportunity_samples VALUES(?1,?2,0,?3)",params![key,time,raw]).unwrap();
        }
        tx.commit().unwrap();
        Recorder::default().observe(&db,&t,&c,false,None,20_000).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM sniper_opportunity_samples",[],|r|r.get::<_,i64>(0)).unwrap(),KEEP);
        db.execute("INSERT INTO sniper_opportunity_samples VALUES(?1,20001,0,'broken-json')",[&key]).unwrap();
        assert_eq!(report(&db,&key).unwrap()["invalid_rows"],1);
    }
    #[test]
    fn cost_and_style_changes_keep_observation_groups_separate() {
        let (db,mut t,mut c,mut d)=fixture(); let key=crate::coordinator_learning::context(&t);
        let mut rec=Recorder::default();
        rec.observe(&db,&t,&c,true,Some(&d),10_000).unwrap();
        c.risk.commission_per_lot=8.; d["evaluated_ms"]=json!(11_000); t.quote_time_ms=Some(11_000);
        rec.observe(&db,&t,&c,true,Some(&d),11_000).unwrap();
        c.trading.sniper_style=braintrade_domain::trading::SniperStyle::Rebound;
        d["evaluated_ms"]=json!(12_000); t.quote_time_ms=Some(12_000);
        rec.observe(&db,&t,&c,true,Some(&d),12_000).unwrap();
        let r=report(&db,&key).unwrap(); assert_eq!(r["distinct_groups"],3);
        assert_eq!(r["sampled_observations"],3);
    }
}
