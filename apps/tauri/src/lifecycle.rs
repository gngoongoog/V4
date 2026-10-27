//! Serialize Core creation against an accepted operator shutdown. Every IPC
//! path and the status worker must pass this gate before calling ensure_core.
use std::sync::Mutex;

pub(crate) struct CoreLifecycle { stopped: Mutex<bool> }
impl CoreLifecycle {
    pub(crate) const fn new()->Self { Self{stopped:Mutex::new(false)} }

    pub(crate) fn connect<T>(&self,open:impl FnOnce()->Result<T,String>)->Result<T,String>{
        let stopped=self.stopped.lock().map_err(|_|"تعذر التحقق من حالة إغلاق Core".to_string())?;
        if *stopped{return Err("تم إغلاق Core؛ أعد فتح التطبيق للاتصال مجدداً".into())}
        // Keep the lock until startup finishes. Otherwise an already-admitted
        // worker could create a new process after shutdown has been accepted.
        open()
    }

    pub(crate) fn shutdown<T>(&self,request:impl FnOnce()->Result<T,String>)->Result<T,String>{
        let mut stopped=self.stopped.lock().map_err(|_|"تعذر التحقق من حالة إغلاق Core".to_string())?;
        if *stopped{return Err("Core مغلق بالفعل".into())}
        // Core still verifies STOPPED, broker freshness and reconciled/flat
        // execution. A rejected request leaves ongoing management available.
        let result=request()?;
        *stopped=true;
        Ok(result)
    }

    pub(crate) fn stopped(&self)->bool {
        self.stopped.lock().map(|s|*s).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc,mpsc};
    use std::thread;

    #[test]
    fn rejected_shutdown_keeps_management_and_connection_available(){
        let gate=CoreLifecycle::new();
        assert!(gate.shutdown(||Err::<(),_>("open positions".into())).is_err());
        assert!(!gate.stopped());
        assert_eq!(gate.connect(||Ok(42)).unwrap(),42);
    }

    #[test]
    fn pending_shutdown_blocks_reconnect_and_accepted_shutdown_never_opens_again(){
        let gate=Arc::new(CoreLifecycle::new());
        let (entered,started)=mpsc::channel();
        let (release,wait)=mpsc::channel();
        let shutdown_gate=Arc::clone(&gate);
        let shutdown=thread::spawn(move||shutdown_gate.shutdown(||{
            entered.send(()).unwrap();wait.recv().unwrap();Ok(())
        }));
        started.recv().unwrap();
        assert!(gate.stopped.try_lock().is_err(),"startup must be excluded during shutdown RPC");
        let reconnect_gate=Arc::clone(&gate);
        let reconnect=thread::spawn(move||reconnect_gate.connect::<()>(||{
            panic!("late UI read/status worker respawned Core after shutdown")
        }));
        release.send(()).unwrap();
        shutdown.join().unwrap().unwrap();
        assert!(reconnect.join().unwrap().is_err());
        assert!(gate.stopped());
        assert!(gate.connect(||Ok(1)).is_err());
        assert!(gate.shutdown(||Ok(())).is_err());
    }
}
