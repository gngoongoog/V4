//! Cross-process fence for a user-requested full exit.
//!
//! A second already-open UI must not respawn Core after another UI shut it
//! down. A newly launched UI explicitly clears the marker once; ordinary
//! status polling never does. A lock serializes startup and shutdown decisions.
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use std::{fs::{self, File, OpenOptions}, path::{Path, PathBuf}};

const LOCK: &str = "full-exit.lock";
const MARKER: &str = "full-exit.requested";

fn locked(dir:&Path)->Result<File>{
    fs::create_dir_all(dir)?;
    let file=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join(LOCK))?;
    file.try_lock_exclusive().context("تعذر بدء أو إغلاق Core لأن عملية أخرى تنفذ تغييراً بحالة التشغيل")?;
    Ok(file)
}

pub(crate) fn start_guard(dir:&Path)->Result<File>{
    let file=locked(dir)?;
    if dir.join(MARKER).exists(){bail!("تم طلب الخروج الكامل؛ أعد فتح التطبيق يدوياً لبدء Core من جديد");}
    Ok(file)
}

pub(crate) fn user_start(dir:&Path)->Result<()> {
    if !dir.join(MARKER).exists(){return Ok(());}
    let _lock=locked(dir)?;
    let marker=dir.join(MARKER);
    match fs::remove_file(marker){Ok(())=>Ok(()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),Err(e)=>Err(e.into())}
}

pub(crate) struct ShutdownBarrier{_lock:File,marker:PathBuf}
impl ShutdownBarrier {
    pub(crate) fn begin(dir:&Path)->Result<Self>{
        let lock=locked(dir)?;
        let marker=dir.join(MARKER);
        let file=OpenOptions::new().create(true).write(true).truncate(false).open(&marker)?;
        file.sync_all()?;
        Ok(Self{_lock:lock,marker})
    }
    pub(crate) fn cancel(self)->Result<()> {
        match fs::remove_file(&self.marker){Ok(())=>Ok(()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),Err(e)=>Err(e.into())}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_exit_marker_fences_other_ui_starts_until_an_explicit_relaunch(){
        let dir=std::env::temp_dir().join(format!("braintrade-exit-{}",uuid::Uuid::new_v4()));
        let barrier=ShutdownBarrier::begin(&dir).unwrap();
        assert!(start_guard(&dir).is_err());
        assert!(user_start(&dir).is_err());
        drop(barrier);
        assert!(start_guard(&dir).is_err());
        user_start(&dir).unwrap();
        drop(start_guard(&dir).unwrap());
        let barrier=ShutdownBarrier::begin(&dir).unwrap();
        barrier.cancel().unwrap();
        drop(start_guard(&dir).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}
