//! Read-only CLI wrapper for the challenger performance review policy.
use anyhow::Result;
use braintrade_runtime::learning_performance;
use rusqlite::{Connection, OpenFlags};
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| braintrade_runtime::data_dir().join("events.sqlite"));
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    println!("{}", serde_json::to_string_pretty(&learning_performance::review(&db)?)?);
    Ok(())
}
