//! Causal, per-member forward-outcome learning for the coordinator. Never trains on future bars.
use anyhow::Result;
use braintrade_domain::{execution::{self,BrainConfig,LearnedWeight},studio::{FrameBar,LabReport},Config,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};
use std::collections::BTreeMap;
pub fn context(t:&Telemetry)->String{serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap()}
pub fn available_frames(t:&Telemetry,frames:&BTreeMap<String,Vec<FrameBar>>,now:u64)->BTreeMap<String,Vec<FrameBar>> {
    let mut available=frames.clone();
    if let Some(m)=&t.market {if now>=m.captured_ms&&now-m.captured_ms<=15_000 {for(tf,seconds,bs)in[("M1",60,&m.m1),("M5",300,&m.m5),("M15",900,&m.m15)] {
        if bs.len()>=100&&bs.last().is_some_and(|b|b.time+seconds<=now/1000&&now/1000-(b.time+seconds)<=seconds+20)&&bs[bs.len()-30..].windows(2).all(|w|w[1].time==w[0].time+seconds)&&available.get(tf).and_then(|b|b.last()).is_none_or(|b|b.candle.time<bs.last().unwrap().time){available.insert(tf.into(),bs.iter().map(|b|FrameBar{candle:b.clone(),spread_points:0,real_volume:0}).collect());}
    }}}available
}
pub fn genome(db:&Connection,b:&BrainConfig,t:&Telemetry)->Result<Option<braintrade_domain::studio::Genome>> {
    if b.id!="generated"||b.report_id.is_empty(){return Ok(None);}
    let raw:Option<String>=db.query_row("SELECT payload FROM studio_runs WHERE id=?1 AND context=?2 AND timeframe=?3",params![b.report_id,context(t),b.timeframe],|r|r.get(0)).optional()?;
    raw.map(|s|Ok(serde_json::from_str::<LabReport>(&s)?.selected)).transpose()
}
pub struct Learner {last:BTreeMap<String,u64>,weights:BTreeMap<String,LearnedWeight>}
impl Learner {
    pub fn new(db:&Connection)->Result<Self>{db.execute_batch("CREATE TABLE IF NOT EXISTS coordinator_samples(source TEXT NOT NULL,bar INTEGER NOT NULL,payload TEXT NOT NULL,outcome TEXT,PRIMARY KEY(source,bar));CREATE INDEX IF NOT EXISTS coordinator_pending ON coordinator_samples(source,outcome);")?;Ok(Self{last:BTreeMap::new(),weights:BTreeMap::new()})}
    pub fn observe(&mut self,db:&Connection,t:&Telemetry,c:&Config,brains:&[BrainConfig],frames:&BTreeMap<String,Vec<FrameBar>>,run:bool,now:u64)->Result<BTreeMap<String,LearnedWeight>> {
        let mut out=BTreeMap::new();
        for b in brains.iter().filter(|b|b.id!="system") {
            let source=serde_json::to_string(&(context(t),"adaptive-member-v1",c.trading.mode,&b.id,&b.timeframe,&b.report_id))?;
            let Some(bars)=frames.get(&b.timeframe).filter(|v|v.len()>=100) else{continue;};let bar=bars.last().unwrap().candle.time;
            if self.last.get(&source)!=Some(&bar) {
                let mut st=db.prepare("SELECT bar,payload FROM coordinator_samples WHERE source=?1 AND outcome IS NULL ORDER BY bar LIMIT 2000")?;
                let rows=st.query_map([&source],|r|Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                for(old,raw)in rows {let p:Value=serde_json::from_str(&raw)?;let future:Vec<_>=bars.iter().filter(|b|b.candle.time>old).collect();
                    let result=if bars[0].candle.time>old {Some(json!({"state":"expired","reason":"Forward horizon not retained"}))}else if future.len()>=12 {let f=future[11];let side=p["side"].as_i64().unwrap()as f64;let spread=if f.spread_points>0{f.spread_points as f64*p["point"].as_f64().unwrap()}else{p["spread"].as_f64().unwrap()};let exit=f.candle.close+if side<0.{spread}else{0.}-side*p["slippage"].as_f64().unwrap();let change=(exit-p["entry"].as_f64().unwrap())*side;let net=change/p["tick"].as_f64().unwrap()*if change>=0.{p["profit"].as_f64().unwrap()}else{p["loss"].as_f64().unwrap()}-p["commission"].as_f64().unwrap();Some(json!({"state":"scored","net":net,"exit_bar":f.candle.time,"scored_ms":now,"cost_model":"fixed contract; broker bar spread when positive, otherwise frozen entry spread; no swaps"}))}else{None};
                    if let Some(r)=result{db.execute("UPDATE coordinator_samples SET outcome=?3 WHERE source=?1 AND bar=?2 AND outcome IS NULL",params![source,old,r.to_string()])?;}
                }
                let weight:LearnedWeight=db.query_row("SELECT COUNT(*),COALESCE(SUM(CASE WHEN json_extract(outcome,'$.net')>0 THEN 1 ELSE 0 END),0),COALESCE(AVG(json_extract(outcome,'$.net')),0) FROM coordinator_samples WHERE source=?1 AND json_extract(outcome,'$.state')='scored'",[&source],|r|Ok(LearnedWeight{samples:r.get(0)?,wins:r.get(1)?,mean_net_per_lot:r.get(2)?}))?;
                self.weights.insert(source.clone(),weight);self.last.insert(source.clone(),bar);
            }
            if run {
                let signal=execution::evaluate(b,t,c,frames,genome(db,b,t)?.as_ref(),now);
                if signal.ready&&[t.tick_size,t.point,t.tick_value_loss,t.tick_value_profit,t.bid,t.ask].iter().all(|v|v.is_some_and(|x|x>0.&&x.is_finite())) {
                    let side=signal.side as f64;let slip=t.tick_size.unwrap()*c.risk.slippage_ticks;let entry=if side>0.{t.ask.unwrap()+slip}else{t.bid.unwrap()-slip};
                    let p=json!({"brain":b.id,"timeframe":b.timeframe,"side":signal.side,"entry":entry,"quote_ms":t.quote_time_ms,"observed_ms":now,"spread":t.ask.unwrap()-t.bid.unwrap(),"point":t.point,"tick":t.tick_size,"profit":t.tick_value_profit,"loss":t.tick_value_loss,"slippage":slip,"commission":c.risk.commission_per_lot,"signal":signal});
                    db.execute("INSERT OR IGNORE INTO coordinator_samples VALUES(?1,?2,?3,NULL)",params![source,bar,p.to_string()])?;
                }
            }
            out.insert(b.id.clone(),self.weights.get(&source).cloned().unwrap_or_default());
        }Ok(out)
    }
}
