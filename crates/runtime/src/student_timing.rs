//! Separate prospective entry-timing research. Never submits or delays broker orders.
use super::*;
use braintrade_domain::student::EntryTiming;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    command_id: String,
    context: String,
    source: String,
    brain: String,
    trial: EntryTiming,
    consumed: bool,
    admitted: bool,
}
impl Record {
    fn from_command(command: &Value) -> Option<Self> {
        let m = &command["signal"]["coordination"]["student"];
        if command["target"] != "demo" || m["entry_timing"] != 1 { return None; }
        let l = &command["learning"];
        let baseline = &m["baseline"];
        let bid = l["bid"].as_f64()?;
        let ask = l["ask"].as_f64()?;
        let tick = l["tick_size"].as_f64()?;
        let profit = l["tick_value_profit"].as_f64()?;
        let loss = l["tick_value_loss"].as_f64()?;
        let stop = baseline["stop"].as_f64()?;
        let target = baseline["target"].as_f64()?;
        let side = command["signal"]["side"].as_i64()? as i8;
        if ![-1, 1].contains(&side) || [tick,profit,loss,stop,target].iter().any(|v| !v.is_finite() || *v <= 0.) { return None; }
        let now = command["created_ms"].as_u64()?;
        let template = Trial {
            opened_ms: now, side, entry: 0., stop, target,
            hold_ms: baseline["hold_ms"].as_u64()?, deadline_ms: None,
            tick_size: tick, profit_value: profit, loss_value: loss,
            commission_per_lot: command["commission_per_lot"].as_f64()?,
            exit_slippage: l["slippage_ticks"].as_f64()? * tick,
            last_quote_ms: now, samples: 0, max_gap_ms: 0, last_observed_ms: now,
            max_observation_gap_ms: 0, exclusion_reason: String::new(),
            usable: true, mfe: 0., mae: 0., outcomes: vec![None],
        };
        if !template.commission_per_lot.is_finite() || template.commission_per_lot < 0.
            || !template.exit_slippage.is_finite() || template.exit_slippage < 0. { return None; }
        let mut trial = EntryTiming::new(template, bid, ask, l["quote_time_ms"].as_u64()?);
        if l["quote_age_ms"].as_u64().is_none_or(|age| age > 1000) { trial.exclude("stale_entry_quote"); }
        let mut identity: Value = serde_json::from_str(m["source"].as_str()?).ok()?;
        identity.as_array_mut()?.get_mut(1)?.clone_from(&json!("student-entry-timing-v1"));
        Some(Self { command_id: command["id"].as_str()?.into(), context: command["context"].as_str()?.into(),
            source: identity.to_string(), brain: m["brain"].as_str()?.into(), trial, consumed: false, admitted: false })
    }
}

pub struct Learner {
    context: String,
    active: BTreeMap<String, Record>,
    studies: BTreeMap<String, ExitStudy>,
}
impl Learner {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS student_entry_trials(context TEXT NOT NULL,command_id TEXT NOT NULL,source TEXT NOT NULL,opened_ms INTEGER NOT NULL,complete INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(context,command_id));
            CREATE INDEX IF NOT EXISTS student_entry_pending ON student_entry_trials(context,complete,opened_ms);
            CREATE TABLE IF NOT EXISTS student_entry_studies(source TEXT PRIMARY KEY,context TEXT NOT NULL,payload TEXT NOT NULL);")?;
        Ok(Self { context: String::new(), active: BTreeMap::new(), studies: BTreeMap::new() })
    }
    pub fn load(&mut self, db: &Connection, key: &str) -> Result<()> {
        if self.context == key { return Ok(()); }
        self.persist(db)?;
        self.active.clear(); self.studies.clear(); self.context = key.into();
        let mut st = db.prepare("SELECT payload FROM student_entry_trials WHERE context=?1 AND complete=0")?;
        for raw in st.query_map([key], |r| r.get::<_,String>(0))? {
            let e: Record = serde_json::from_str(&raw?)?;
            self.active.insert(e.command_id.clone(), e);
        }
        let mut st = db.prepare("SELECT source,payload FROM student_entry_studies WHERE context=?1")?;
        for row in st.query_map([key], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?)))? {
            let (source,raw) = row?;
            self.studies.insert(source, serde_json::from_str(&raw)?);
        }
        Ok(())
    }
    pub fn capture(&mut self, command: &Value, now: u64) {
        if command["context"].as_str() != Some(self.context.as_str()) { return; }
        let Some(mut e) = Record::from_command(command) else { return; };
        if self.active.contains_key(&e.command_id) { return; }
        // The command is durable, but an unobserved path across restart is not recoverable.
        if now > e.trial.anchor_ms.saturating_add(350) { e.trial.exclude("timing_restart_gap"); }
        if self.active.len() >= 256 { e.trial.exclude("timing_capacity"); }
        self.active.insert(e.command_id.clone(), e);
    }
    pub fn restore(&mut self, db: &Connection, key: &str, now: u64) -> Result<()> {
        let mut st = db.prepare("SELECT c.payload FROM broker_commands c WHERE c.context=?1 AND c.kind='open' AND json_extract(c.payload,'$.signal.coordination.student.entry_timing')=1 AND NOT EXISTS(SELECT 1 FROM student_entry_trials e WHERE e.context=c.context AND e.command_id=c.id)")?;
        for raw in st.query_map([key], |r| r.get::<_,String>(0))? { self.capture(&serde_json::from_str::<Value>(&raw?)?,now); }
        Ok(())
    }
    pub fn quote(&mut self, t: &Telemetry, now: u64) {
        if !t.connected || !t.demo || t.account_trade_mode != Some(0)
            || crate::coordinator_learning::context(t) != self.context { return; }
        let (Some(bid),Some(ask),Some(ms)) = (t.bid,t.ask,t.quote_time_ms) else { return; };
        for e in self.active.values_mut() {
            e.trial.observe(bid,ask,ms,now,t.tick_age_ms.is_some_and(|age| age <= 1000));
        }
    }
    pub fn persist(&self, db: &Connection) -> Result<()> {
        let tx = db.unchecked_transaction()?;
        for e in self.active.values() {
            tx.execute("INSERT INTO student_entry_trials VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(context,command_id) DO UPDATE SET complete=excluded.complete,payload=excluded.payload",
                params![e.context,e.command_id,e.source,e.trial.anchor_ms,e.consumed,serde_json::to_string(e)?])?;
        }
        for (source,study) in &self.studies {
            tx.execute("INSERT OR REPLACE INTO student_entry_studies VALUES(?1,?2,?3)",params![source,self.context,serde_json::to_string(study)?])?;
        }
        tx.commit()?; Ok(())
    }
    pub fn finish(&mut self, db: &Connection, now: u64) -> Result<()> {
        for e in self.active.values_mut() {
            if e.consumed { continue; }
            if !e.trial.complete() && now > e.trial.deadline_ms.saturating_add(60_000) { e.trial.exclude("expired"); }
            if e.trial.complete() {
                if let Some(rewards) = e.trial.rewards() {
                    e.admitted = self.studies.entry(e.source.clone()).or_default().observe(e.trial.anchor_ms,now,rewards);
                } else if e.trial.usable { e.trial.exclude("too_few_quotes"); }
                e.consumed = true;
            }
        }
        self.persist(db)?;
        self.active.retain(|_,e| !e.consumed);
        Ok(())
    }
}

pub fn view(db: &Connection, key: &str) -> Result<Value> {
    let total: u64 = db.query_row("SELECT COUNT(*) FROM student_entry_trials WHERE context=?1",[key],|r|r.get(0))?;
    let mut st = db.prepare("SELECT source,payload FROM student_entry_studies WHERE context=?1 ORDER BY source")?;
    let studies = st.query_map([key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
        .map(|r| { let (source,raw)=r?; let s: ExitStudy=serde_json::from_str(&raw)?;
            Ok(json!({"source":source,"study":s,"suggested_delay_ms":EntryTiming::DELAYS[s.active.index().min(2)],"candidate_delay_ms":EntryTiming::DELAYS[s.challenger.index().min(2)]})) }).collect::<Result<Vec<_>>>()?;
    let admitted: u64 = studies.iter().filter_map(|v|v["study"]["total_pairs"].as_u64()).sum();
    let mut st = db.prepare("SELECT payload FROM student_entry_trials WHERE context=?1 ORDER BY opened_ms DESC LIMIT 20")?;
    let trials = st.query_map([key],|r|r.get::<_,String>(0))?.map(|r|Ok(serde_json::from_str::<Value>(&r?)?)).collect::<Result<Vec<_>>>()?;
    let mut st = db.prepare("SELECT COALESCE(json_extract(payload,'$.trial.exclusion_reason'),''),COUNT(*) FROM student_entry_trials WHERE context=?1 AND json_extract(payload,'$.trial.usable')=0 GROUP BY 1")?;
    let exclusions = st.query_map([key],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?)))?.collect::<rusqlite::Result<BTreeMap<_,_>>>()?;
    Ok(json!({"research_only":true,"total_opportunities":total,"admitted_pairs":admitted,
        "delays_ms":EntryTiming::DELAYS,"maximum_entry_lateness_ms":100,"studies":studies,"trials":trials,"exclusions":exclusions,
        "note":"Prospective quote comparisons at queued Demo opportunities, including proposals later rejected. These never delay orders or add broker PnL. One common deadline, original risk, executable sides, commission and estimated entry/exit slippage; no swaps or inferred ticks."}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn telemetry() -> Telemetry {
        Telemetry { connected:true, demo:true, account_trade_mode:Some(0),login:17,server:"Demo".into(),currency:"USD".into(),symbol:"XAUUSD".into(),
            bid:Some(100.),ask:Some(100.2),tick_age_ms:Some(0),quote_time_ms:Some(1000),..Default::default() }
    }
    fn command(t: &Telemetry) -> Value {
        let key=crate::coordinator_learning::context(t);
        json!({"id":"timing-one","context":key,"brain":"system","target":"demo","created_ms":1000,"commission_per_lot":7.,
            "signal":{"side":1,"coordination":{"student":{"entry_timing":1,"brain":"trend","source":source(&key,&json!("expert"),"trend","M1",""),"baseline":{"stop":10.,"target":20.,"hold_ms":2500}}}},
            "learning":{"bid":100.,"ask":100.2,"quote_time_ms":1000,"quote_age_ms":0,"tick_size":0.1,"tick_value_profit":1.,"tick_value_loss":1.,"slippage_ticks":1.}})
    }
    #[test]
    fn prospective_timing_survives_restart_and_never_writes_broker_results() {
        let db=Connection::open_in_memory().unwrap();
        let _=crate::execution::Service::new(&db).unwrap();
        let mut t=telemetry(); let key=crate::coordinator_learning::context(&t); let cmd=command(&t);
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload) VALUES('timing-one',?1,'system','open',1000,'cancelled',?2)",params![key,cmd.to_string()]).unwrap();
        let mut learner=Learner::new(&db).unwrap(); learner.load(&db,&key).unwrap(); learner.capture(&cmd,1000);
        t.quote_time_ms=Some(1100); learner.quote(&t,1100); learner.finish(&db,1100).unwrap();
        let mut learner=Learner::new(&db).unwrap(); learner.load(&db,&key).unwrap(); learner.restore(&db,&key,1150).unwrap();
        for now in [1250,1350,2000,2100,2500,3500] { t.quote_time_ms=Some(now); learner.quote(&t,now); }
        learner.finish(&db,3500).unwrap();
        let v=view(&db,&key).unwrap(); assert_eq!(v["total_opportunities"],1); assert_eq!(v["admitted_pairs"],1);
        assert_eq!(v["research_only"],true);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM coordinator_broker_samples",[],|r|r.get::<_,u64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],|r|r.get::<_,u64>(0)).unwrap(),1);
        let mut learner=Learner::new(&db).unwrap(); learner.load(&db,&key).unwrap(); learner.restore(&db,&key,4000).unwrap(); learner.finish(&db,4000).unwrap();
        assert_eq!(view(&db,&key).unwrap()["admitted_pairs"],1);
        t.login=99; let other=crate::coordinator_learning::context(&t); learner.load(&db,&other).unwrap(); learner.capture(&cmd,4000);
        assert!(learner.active.is_empty()); assert_eq!(view(&db,&other).unwrap()["total_opportunities"],0);
    }
    #[test]
    fn late_recovery_and_live_opportunities_cannot_become_timing_evidence() {
        let db=Connection::open_in_memory().unwrap(); let t=telemetry(); let key=crate::coordinator_learning::context(&t);
        let mut learner=Learner::new(&db).unwrap(); learner.load(&db,&key).unwrap();
        let mut cmd=command(&t); cmd["target"]=json!("live"); learner.capture(&cmd,1000); assert!(learner.active.is_empty());
        cmd["target"]=json!("demo"); learner.capture(&cmd,1400); learner.finish(&db,1400).unwrap();
        let v=view(&db,&key).unwrap(); assert_eq!(v["admitted_pairs"],0); assert_eq!(v["exclusions"]["timing_restart_gap"],1);
    }
}
