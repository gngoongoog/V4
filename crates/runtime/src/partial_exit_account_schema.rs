//! One-time, lossless widening of old TP research tables from Demo-only
//! rows to user-selected Demo/Live account-scoped rows.
//!
//! This changes database constraints ONLY. It does not authorize trading or
//! modify existing TP stage states, broker orders, reconciliation or receipts.
use anyhow::{ensure,Result};
use rusqlite::{Connection,OptionalExtension};

pub(crate) fn allow_selected_account(db:&Connection, table:&str, index:&str)->Result<()>{
    // Never interpolate a caller-provided identifier into DDL.
    ensure!(matches!(table,
        "partial_exit_price_plans"|
        "partial_exit_shadow_tp1"|
        "partial_exit_shadow_close_intents"|
        "partial_exit_stage_settlements"),
        "Unknown TP account migration table");
    let ddl:Option<String>=db.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
        [table],|r|r.get(0)).optional()?;
    let Some(ddl)=ddl else{
        anyhow::bail!("Expected TP table missing before account migration: {table}");
    };
    // Already updated, including a second startup after migration.
    if ddl.contains("CHECK(target IN ('demo','live'))"){return Ok(())}
    ensure!(ddl.matches("CHECK(target='demo')").count()==1,
        "Unsupported TP table schema; preserving original without modification: {table}");
    let from=format!("{table}(");
    ensure!(ddl.matches(&from).count()==1,
        "Unsupported TP table declaration; preserving original: {table}");
    let temporary=format!("{table}__account_upgrade");
    let new_ddl=ddl.replacen(&from,&format!("{temporary}("),1)
        .replacen("CHECK(target='demo')",
            "CHECK(target IN ('demo','live'))",1);
    // An interruption rolls back the whole rebuild; all columns are copied
    // unchanged and the original index is recreated after its old table drops.
    let tx=db.unchecked_transaction()?;
    tx.execute_batch(&format!(
        "{new_ddl};
         INSERT INTO {temporary} SELECT * FROM {table};
         DROP TABLE {table};
         ALTER TABLE {temporary} RENAME TO {table};
         {index};"
    ))?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn old_demo_only_schema_migrates_once_without_losing_immutable_rows(){
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE partial_exit_price_plans(
                context TEXT NOT NULL,target TEXT NOT NULL CHECK(target='demo'),
                position_identifier INTEGER NOT NULL,payload TEXT NOT NULL,
                PRIMARY KEY(context,target,position_identifier));
             CREATE INDEX partial_exit_prices_recent
                ON partial_exit_price_plans(context,target);"
        ).unwrap();
        db.execute(
            "INSERT INTO partial_exit_price_plans VALUES('same','demo',8,'original')",
            []).unwrap();
        let index="CREATE INDEX IF NOT EXISTS partial_exit_prices_recent
            ON partial_exit_price_plans(context,target)";
        allow_selected_account(&db,"partial_exit_price_plans",index).unwrap();
        allow_selected_account(&db,"partial_exit_price_plans",index).unwrap();
        assert_eq!(db.query_row("SELECT payload FROM partial_exit_price_plans
            WHERE context='same' AND target='demo' AND position_identifier=8",
            [],|r|r.get::<_,String>(0)).unwrap(),"original");
        db.execute("INSERT INTO partial_exit_price_plans VALUES(
            'same','live',8,'independent')",[]).unwrap();
        assert_eq!(db.query_row("SELECT COUNT(*) FROM partial_exit_price_plans",
            [],|r|r.get::<_,i64>(0)).unwrap(),2);
        assert!(db.query_row("SELECT name FROM sqlite_master WHERE type='index'
            AND name='partial_exit_prices_recent'",[],
            |r|r.get::<_,String>(0)).is_ok());
        assert!(allow_selected_account(&db,"broker_commands",index).is_err());
    }
    #[test]
    fn unknown_schema_fails_without_destructive_rebuild(){
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE partial_exit_shadow_tp1(
            context TEXT,target TEXT,position_identifier INTEGER);").unwrap();
        assert!(allow_selected_account(&db,"partial_exit_shadow_tp1",
            "CREATE INDEX dummy ON partial_exit_shadow_tp1(context)").is_err());
        assert!(db.query_row("SELECT COUNT(*) FROM partial_exit_shadow_tp1",[],
            |r|r.get::<_,i64>(0)).is_ok());
    }
}
