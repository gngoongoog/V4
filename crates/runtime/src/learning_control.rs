//! Durable, process-wide learning pause. It never stops broker reconciliation.
use anyhow::Result;
use rusqlite::{params,Connection,OptionalExtension};
use serde::{Deserialize,Serialize};
use serde_json::json;

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(default,deny_unknown_fields)]
pub struct Gate {
    pub frozen:bool,
    pub cutoff_ms:u64,
    pub quality_context:String,
    pub quality_target:String,
    pub quality:f64,
    pub quality_samples:u64,
    pub valid:bool,
}
impl Default for Gate {
    fn default()->Self{Self{frozen:false,cutoff_ms:0,quality_context:String::new(),quality_target:String::new(),quality:0.5,quality_samples:0,valid:true}}
}
impl Gate {
    fn valid(&self)->bool {
        self.valid && self.quality.is_finite() && (0.0..=1.0).contains(&self.quality)
            && (!self.frozen || self.cutoff_ms>0)
            && (self.quality_target.is_empty() || ["demo","live"].contains(&self.quality_target.as_str()))
    }
    pub fn cutoff(&self)->Option<u64>{self.frozen.then_some(self.cutoff_ms)}
    pub fn history_quality(&self,context:&str,target:&str)->(f64,u64){
        if self.frozen && self.valid && self.quality_context==context && self.quality_target==target {
            (self.quality,self.quality_samples)
        }else{(0.5,0)}
    }
}
pub fn load(db:&Connection)->Result<Gate>{
    db.execute_batch("CREATE TABLE IF NOT EXISTS learning_control(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);")?;
    let raw:Option<String>=db.query_row("SELECT payload FROM learning_control WHERE id=1",[],|r|r.get(0)).optional()?;
    match raw {
        None=>Ok(Gate::default()),
        Some(raw)=>match serde_json::from_str::<Gate>(&raw) {
            Ok(gate) if gate.valid()=>Ok(gate),
            // Preserve position management even if the pause record is damaged.
            // No learned evidence is admitted until the operator resumes.
            _=>Ok(Gate{frozen:true,cutoff_ms:1,valid:false,..Gate::default()})
        }
    }
}
pub fn save(db:&Connection,gate:&Gate,now:u64)->Result<()> {
    anyhow::ensure!(gate.valid(),"Invalid learning control state");
    let tx=db.unchecked_transaction()?;
    tx.execute("INSERT OR REPLACE INTO learning_control VALUES(1,?1)",[serde_json::to_string(gate)?])?;
    tx.execute("INSERT INTO events(timestamp_ms,kind,detail) VALUES(?1,'learning_control_changed',?2)",
        params![now,json!({"frozen":gate.frozen,"cutoff_ms":gate.cutoff_ms,
            "scope":"all_accounts_and_learning_services","trading_unchanged":true}).to_string()])?;
    tx.commit()?;Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_is_durable_and_bad_data_fails_closed_without_erasing_it(){
        let db=Connection::open_in_memory().unwrap();
        db.execute("CREATE TABLE events(id INTEGER PRIMARY KEY,timestamp_ms INTEGER,kind TEXT,detail TEXT)",[]).unwrap();
        assert!(!load(&db).unwrap().frozen);
        let gate=Gate{frozen:true,cutoff_ms:123,quality_context:"a".into(),quality_target:"demo".into(),quality:0.7,quality_samples:2,valid:true};
        save(&db,&gate,125).unwrap();let restored=load(&db).unwrap();
        assert_eq!(restored.cutoff(),Some(123));assert_eq!(restored.history_quality("a","demo"),(0.7,2));
        assert_eq!(restored.history_quality("b","demo"),(0.5,0));
        db.execute("UPDATE learning_control SET payload='broken'",[]).unwrap();
        let safe=load(&db).unwrap();assert!(safe.frozen);assert!(!safe.valid);assert_eq!(safe.cutoff(),Some(1));
        let raw:String=db.query_row("SELECT payload FROM learning_control",[],|r|r.get(0)).unwrap();assert_eq!(raw,"broken");
    }
}
