//! Read-only, account-bound MT5 tick HVN research handoff.
//! No broker orders, magic identity, learning weight or trading signals.
use anyhow::{anyhow,ensure,Result};
use braintrade_domain::Status;
use serde_json::{json,Value};
use uuid::Uuid;

const EXPIRY_MS:u64=25_000;
#[derive(Clone)]
struct Pending{
    id:String,
    context:String,
    requested_ms:u64,
    window_ms:u64,
    dispatched:bool,
}
#[derive(Default)]
pub struct Service{
    pending:Option<Pending>,
    last:Option<(String,u64,Value)>,
    note:String,
}
fn context(s:&Status)->Option<String>{
    let t=s.telemetry.as_ref()?;
    if !s.gateway_fresh || !t.connected || t.login==0 ||
        t.server.is_empty() || t.symbol.is_empty() || t.currency.is_empty(){return None;}
    Some(serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).ok()?)
}
fn valid_result(report:&Value, pending:&Pending, s:&Status)->Result<()>{
    let t=s.telemetry.as_ref().ok_or_else(||anyhow!("Broker unavailable"))?;
    ensure!(report["read_only"]==true && report["trade_authorized"]==false &&
        report["broker_only"]==true && report["exchange_wide"]==false,
        "HVN provenance and no-trade contract mismatch");
    let id=&report["identity"];
    ensure!(id["server"]==t.server && id["login"]==t.login &&
        id["symbol"]==t.symbol && id["currency"]==t.currency,
        "MT5 account identity changed during HVN calculation");
    ensure!(report["window_ms"]==pending.window_ms,"HVN window mismatch");
    let source=report["source"].as_str().unwrap_or("");
    ensure!(matches!(source,"mt5_quote_tick_activity"|"mt5_broker_trade_volume_real"|"none"),
        "Unsupported HVN source");
    let available=report["available"].as_bool().ok_or_else(||anyhow!("HVN availability missing"))?;
    if available {
        ensure!(source!="none","Available HVN cannot have no source");
        let unit=report["unit"].as_str().unwrap_or("");
        let expected_unit=if source=="mt5_quote_tick_activity" {
            "observed_quote_events"
        }else{"broker_reported_real_volume"};
        ensure!(unit==expected_unit,"HVN reported activity unit mismatches its source");
        let observed=report["observations"].as_u64()
            .ok_or_else(||anyhow!("HVN observation count missing"))?;
        let trade_events=report["trade_events_observed"].as_u64()
            .ok_or_else(||anyhow!("HVN trade event evidence missing"))?;
        let quote_events=report["quote_events_observed"].as_u64()
            .ok_or_else(||anyhow!("HVN quote event evidence missing"))?;
        let selected=if source=="mt5_quote_tick_activity"{quote_events}else{trade_events};
        ensure!((80..=40_000).contains(&observed) && observed==selected &&
            trade_events<=40_000 && quote_events<=40_000,
            "HVN selected observations disagree with source-specific evidence");
        let distinct=report["distinct_levels"].as_u64()
            .ok_or_else(||anyhow!("HVN distinct level count missing"))?;
        ensure!((5..=2_000).contains(&distinct),"HVN distinct levels invalid");
        let total=report["total_activity"].as_f64().unwrap_or(0.);
        ensure!(total.is_finite() && total>0.,"HVN total activity invalid");
        if source=="mt5_quote_tick_activity" {
            ensure!((total-observed as f64).abs()<0.00001,
                "HVN quote activity must equal observed quote events");
        }
        let nodes=report["nodes"].as_array().ok_or_else(||anyhow!("HVN nodes missing"))?;
        ensure!(!nodes.is_empty() && nodes.len()<=8, "HVN node count invalid");
        ensure!(report["observed_to_ms"].as_u64().is_some_and(|v|v<=pending.requested_ms),
            "HVN used future ticks");
        ensure!(report["observed_from_ms"].as_u64().is_some_and(|v|
            v<=report["observed_to_ms"].as_u64().unwrap_or(0) &&
            v>=pending.requested_ms.saturating_sub(pending.window_ms)),
            "HVN source timestamps outside requested causal window");
        for node in nodes {
            let price=node["price"].as_f64().unwrap_or(0.);
            let amount=node["activity"].as_f64().unwrap_or(0.);
            let share=node["share"].as_f64().unwrap_or(-1.);
            ensure!(price.is_finite()&&price>0.&&amount.is_finite()&&amount>0.
                &&amount<=total &&share.is_finite()&&(0.0..=1.0).contains(&share) &&
                (share-amount/total).abs()<=0.000002,
                "HVN node activity/share cannot be reconciled with source total");
        }
    } else {
        ensure!(source=="none" && report["nodes"].as_array().is_some_and(|v|v.is_empty()),
            "Unavailable HVN must contain no nodes and no source");
    }
    Ok(())
}
impl Service{
    pub fn pending(&self,now:u64)->bool {
        // A dispatched MT5 history read may still be executing *after* the
        // deadline. An unsent timed-out request is harmless to retire, but an
        // acknowledged Gateway dispatch must never fail open into START.
        self.pending.as_ref().is_some_and(|p|
            p.dispatched || now.saturating_sub(p.requested_ms)<=EXPIRY_MS)
    }
    pub fn route(&mut self,method:&str,path:&str,body:&[u8],s:&Status,now:u64)->Result<Value>{
        if self.pending.as_ref().is_some_and(|p|now.saturating_sub(p.requested_ms)>EXPIRY_MS){
            if self.pending.as_ref().is_some_and(|p|p.dispatched){
                // Gateway is possibly still busy in copy_ticks_range. There
                // is no reliable cancellation acknowledgment yet; fail closed.
                self.note="انتهت مهلة HVN بعد إرسال الطلب؛ لا يمكن بدء التداول حتى تأكيد النتيجة أو خروج كامل وإعادة تشغيل آمنة".into();
            }else{
                self.pending=None;
                self.note="انتهت مهلة HVN قبل استلام Gateway للطلب؛ لم يبدأ جمع تيكات".into();
            }
        }
        let ctx=context(s);
        if self.pending.as_ref().is_some_and(|p|ctx.as_ref()!=Some(&p.context)){
            if self.pending.as_ref().is_some_and(|p|p.dispatched){
                self.note="تغيّر حساب MT5 بعد إرسال HVN؛ ستبقى محاولة START محجوبة حتى تصالح Gateway أو إعادة التشغيل الآمنة".into();
            }else{
                self.pending=None;
                self.note="تغيّر حساب MT5 قبل بدء قراءة HVN؛ تم إلغاء الطلب غير المرسل".into();
            }
        }
        match(method,path){
            ("GET","/v1/hvn")=>{
                // Retain the last account-scoped research result for audit,
                // but do not mistake a historic node for fresh market data.
                // Data age uses the actual latest observed tick, NOT the Core
                // completion time (which may follow a slow copy_ticks_range).
                let report=self.last.as_ref().and_then(|(k,_,r)|
                    if ctx.as_ref()==Some(k){Some(r.clone())}else{None});
                let age_ms=report.as_ref().and_then(|r|r["observed_to_ms"].as_u64())
                    .map(|t|now.saturating_sub(t));
                let max_fresh_ms=report.as_ref().and_then(|r|r["window_ms"].as_u64())
                    .unwrap_or(60_000).min(300_000);
                let stale=age_ms.is_some_and(|age|age>max_fresh_ms);
                Ok(json!({"schema":"hvn-research-v1","read_only":true,
                    "trade_authorized":false,"connected":ctx.is_some(),
                    "pending":self.pending.is_some(),
                    "report":report,"report_age_ms":age_ms,"report_stale":stale,
                    "note":self.note}))
            }
            ("POST","/v1/hvn/request")=>{
                ensure!(!s.requested_run,"أوقف الدخول STOP قبل قراءة تيكات HVN البحثية");
                let t=s.telemetry.as_ref().ok_or_else(||anyhow!("MT5 not connected"))?;
                let key=ctx.ok_or_else(||anyhow!("MT5 account needs verified fresh identity"))?;
                ensure!(t.positions.is_empty() && t.pending_orders==0,
                    "HVN research requires a flat, order-free broker account");
                ensure!(self.pending.is_none(),"HVN research request already pending");
                let v:Value=serde_json::from_slice(body)?;
                let window=v["window_ms"].as_u64().unwrap_or(600_000);
                ensure!((60_000..=900_000).contains(&window),"HVN window must be 1 to 15 minutes");
                let id=Uuid::new_v4().to_string();
                self.pending=Some(Pending{id:id.clone(),context:key,requested_ms:now,
                    window_ms:window,dispatched:false});
                self.note="تم طلب قراءة Tick تاريخية من وسيط MT5؛ بحث فقط بلا أوامر".into();
                Ok(json!({"queued":true,"id":id,"read_only":true,"trade_authorized":false}))
            }
            ("GET","/v1/gateway/hvn/request")=>{
                let Some(p)=self.pending.as_mut() else{return Ok(json!({"job":null}));};
                if p.dispatched || s.requested_run {return Ok(json!({"job":null}));}
                let t=s.telemetry.as_ref().ok_or_else(||anyhow!("MT5 not ready"))?;
                p.dispatched=true;
                Ok(json!({"job":{"id":p.id,"server":t.server,"login":t.login,
                    "symbol":t.symbol,"currency":t.currency,"now_ms":p.requested_ms,
                    "window_ms":p.window_ms},"read_only":true}))
            }
            ("POST","/v1/gateway/hvn/result")=>{
                let pending=self.pending.as_ref().cloned().ok_or_else(||anyhow!("No outstanding HVN request"))?;
                ensure!(pending.dispatched,"HVN job not yet dispatched");
                let v:Value=serde_json::from_slice(body)?;
                ensure!(v["id"].as_str()==Some(pending.id.as_str()),"HVN job ID mismatch");
                // A matched completion from the authenticated Gateway means
                // its blocking copy_ticks_range call has returned. Release
                // the START fence; that does NOT certify account freshness.
                let report=match v.get("report"){
                    Some(report)=>report,
                    None=>{
                        self.pending=None;
                        self.note="اكتملت محاولة قراءة HVN لكن لم يصل تقرير صالح؛ لا تُعرض نتيجة".into();
                        return Err(anyhow!("HVN report missing"));
                    }
                };
                if s.requested_run || ctx.as_ref()!=Some(&pending.context){
                    self.pending=None;
                    self.note="اكتملت قراءة HVN لكن تغيرت حالة MT5 أو انتهت حداثة الحساب؛ النتيجة مرفوضة".into();
                    return Ok(json!({"ok":false,"discarded":true,"read_only":true,"trade_authorized":false}));
                }
                if let Err(error)=valid_result(report,&pending,s){
                    self.pending=None;
                    self.note="اكتملت قراءة HVN لكن تقرير البيانات لم يجتز التحقق؛ النتيجة مرفوضة".into();
                    return Err(error);
                }
                self.last=Some((pending.context.clone(),now,report.clone()));
                self.pending=None;
                self.note="استلم Core نتيجة بحث HVN المطابقة لهوية الوسيط، دون تحويلها إلى إشارة تداول".into();
                Ok(json!({"ok":true,"read_only":true,"trade_authorized":false}))
            }
            _=>Err(anyhow!("Unknown HVN research route"))
        }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Telemetry;
    fn status()->Status{Status{gateway_fresh:true,requested_run:false,
        telemetry:Some(Telemetry{connected:true,server:"Demo".into(),login:42,
            currency:"USD".into(),symbol:"XAUUSDm".into(),..Default::default()}),
        ..Default::default()}}
    #[test]fn request_is_strictly_stopped_flat_broker_only(){
        let mut h=Service::default();let now=10_000;
        let mut s=status();s.requested_run=true;
        assert!(h.route("POST","/v1/hvn/request",b"{}",&s,now).is_err());
        s.requested_run=false;s.telemetry.as_mut().unwrap().pending_orders=1;
        assert!(h.route("POST","/v1/hvn/request",b"{}",&s,now).is_err());
        s.telemetry.as_mut().unwrap().pending_orders=0;
        assert_eq!(h.route("POST","/v1/hvn/request",b"{}",&s,now).unwrap()["queued"],true);
        assert!(h.pending(now));
        assert!(h.route("POST","/v1/hvn/request",b"{}",&s,now).is_err());
    }
    #[test]fn account_change_and_future_nodes_never_enter_report(){
        let now=100_000;let mut h=Service::default();let s=status();
        let id=h.route("POST","/v1/hvn/request",b"{}",&s,now).unwrap()["id"].as_str().unwrap().to_string();
        assert!(h.route("GET","/v1/gateway/hvn/request",b"",&s,now).unwrap()["job"].is_object());
        let report=json!({"available":true,"read_only":true,"trade_authorized":false,
            "broker_only":true,"exchange_wide":false,"source":"mt5_quote_tick_activity",
            "unit":"observed_quote_events","observations":80,
            "quote_events_observed":80,"trade_events_observed":0,
            "distinct_levels":10,"total_activity":80.0,
            "window_ms":600000,"identity":{"server":"Demo","login":42,"currency":"USD","symbol":"XAUUSDm"},
            "nodes":[{"price":2000.0,"activity":16.0,"share":0.2}],
            "observed_from_ms":1000,"observed_to_ms":now+1});
        let body=json!({"id":id,"report":report}).to_string();
        assert!(h.route("POST","/v1/gateway/hvn/result",body.as_bytes(),&s,now).is_err());
        assert!(h.route("GET","/v1/hvn",b"",&s,now).unwrap()["report"].is_null());
        assert!(!h.pending(now),"Matched but invalid result proves Gateway history read ended");
        let id=h.route("POST","/v1/hvn/request",b"{}",&s,now+2).unwrap()["id"].as_str().unwrap().to_owned();
        h.route("GET","/v1/gateway/hvn/request",b"",&s,now+2).unwrap();
        let mut other=s.clone();other.telemetry.as_mut().unwrap().login=99;
        let result=h.route("POST","/v1/gateway/hvn/result",
            json!({"id":id,"report":{}}).to_string().as_bytes(),&other,now+3).unwrap();
        assert_eq!(result["discarded"],true);
        assert!(!h.pending(now+3),"Matched Gateway completion must release work fence, not accept wrong-account levels");
        assert!(h.route("GET","/v1/hvn",b"",&other,now+3).unwrap()["report"].is_null());
    }
    #[test]fn stale_account_snapshot_drops_completed_job_without_trapping_start(){
        let mut h=Service::default();let now=120_000;let s=status();
        let id=h.route("POST","/v1/hvn/request",b"{}",&s,now).unwrap()["id"].as_str().unwrap().to_owned();
        h.route("GET","/v1/gateway/hvn/request",b"",&s,now).unwrap();
        let mut stale=s.clone();stale.gateway_fresh=false;
        let reply=h.route("POST","/v1/gateway/hvn/result",
            json!({"id":id,"report":{}}).to_string().as_bytes(),&stale,now+EXPIRY_MS+10).unwrap();
        assert_eq!(reply["discarded"],true);
        assert!(!h.pending(now+EXPIRY_MS+10));
        assert!(h.route("GET","/v1/hvn",b"",&s,now+EXPIRY_MS+11).unwrap()["report"].is_null());
    }
    #[test]fn completed_or_nondispatched_job_has_bounded_retry_semantics(){
        let now=100_000;let mut h=Service::default();let s=status();
        h.route("POST","/v1/hvn/request",b"{}",&s,now).unwrap();
        let late=now+EXPIRY_MS+1;
        assert!(!h.pending(late),"Unsent expired request must not block START");
        h.route("GET","/v1/hvn",b"",&s,late).unwrap();
        assert!(!h.pending(late),"Unsent expired request can be cancelled safely");
        assert!(h.route("POST","/v1/hvn/request",b"{}",&s,late).is_ok());
        assert!(h.route("GET","/v1/gateway/hvn/request",b"",&s,late).unwrap()["job"].is_object());
        h.route("GET","/v1/hvn",b"",&s,late+EXPIRY_MS+1).unwrap();
        assert!(h.pending(late+EXPIRY_MS+1),"Dispatched job may still be inside MT5 history even after timeout");
        let mut wrong=s.clone();wrong.telemetry.as_mut().unwrap().login=43;
        h.route("GET","/v1/hvn",b"",&wrong,late+EXPIRY_MS+2).unwrap();
        assert!(h.pending(late+EXPIRY_MS+2),"Account switch must not release a dispatched read lock");
    }
    #[test]fn source_evidence_unit_and_share_must_reconcile(){
        let s=status();
        let pending=Pending{
            id:"job".into(),context:context(&s).unwrap(),requested_ms:100_000,
            window_ms:600_000,dispatched:true,
        };
        let valid=json!({"available":true,"read_only":true,"trade_authorized":false,
            "broker_only":true,"exchange_wide":false,"source":"mt5_quote_tick_activity",
            "unit":"observed_quote_events","observations":80,
            "quote_events_observed":80,"trade_events_observed":0,
            "distinct_levels":10,"total_activity":80.0,
            "window_ms":600000,"identity":{"server":"Demo","login":42,"currency":"USD","symbol":"XAUUSDm"},
            "nodes":[{"price":2000.0,"activity":16.0,"share":0.2}],
            "observed_from_ms":1000,"observed_to_ms":100_000});
        assert!(valid_result(&valid,&pending,&s).is_ok());
        for (field, value) in [
            ("unit",json!("broker_reported_real_volume")),
            ("observations",json!(79)),
            ("quote_events_observed",json!(79)),
            ("distinct_levels",json!(3000)),
            ("total_activity",json!(81.0)),
            ("observed_from_ms",json!(200_000)),
        ] {
            let mut corrupted=valid.clone();corrupted[field]=value;
            assert!(valid_result(&corrupted,&pending,&s).is_err(),"{field}");
        }
        let mut corrupt_share=valid.clone();
        corrupt_share["nodes"][0]["share"]=json!(0.9);
        assert!(valid_result(&corrupt_share,&pending,&s).is_err());
        let mut corrupt_activity=valid.clone();
        corrupt_activity["nodes"][0]["activity"]=json!(100.0);
        assert!(valid_result(&corrupt_activity,&pending,&s).is_err());
    }
    #[test]fn success_is_read_only_and_context_scoped(){
        let now=100_000;let mut h=Service::default();let s=status();
        let id=h.route("POST","/v1/hvn/request",b"{}",&s,now).unwrap()["id"].as_str().unwrap().to_string();
        h.route("GET","/v1/gateway/hvn/request",b"",&s,now).unwrap();
        let report=json!({"available":true,"read_only":true,"trade_authorized":false,
            "broker_only":true,"exchange_wide":false,"source":"mt5_quote_tick_activity",
            "unit":"observed_quote_events","observations":80,
            "quote_events_observed":80,"trade_events_observed":0,
            "distinct_levels":10,"total_activity":80.0,
            "window_ms":600000,"identity":{"server":"Demo","login":42,"currency":"USD","symbol":"XAUUSDm"},
            "nodes":[{"price":2000.0,"activity":16.0,"share":0.2}],
            "observed_from_ms":1000,"observed_to_ms":now});
        assert!(h.route("POST","/v1/gateway/hvn/result",json!({"id":id,"report":report}).to_string().as_bytes(),&s,now).is_ok());
        let result=h.route("GET","/v1/hvn",b"",&s,now).unwrap();
        assert_eq!(result["report"]["nodes"][0]["price"],2000.);
        assert_eq!(result["trade_authorized"],false);
        assert_eq!(result["report_age_ms"],0);
        assert_eq!(result["report_stale"],false);
        // Cached source may be reviewed, but must not be silently represented
        // as current once the evidence-age window has elapsed.
        let aged=h.route("GET","/v1/hvn",b"",&s,now+600_001).unwrap();
        assert_eq!(aged["report"]["nodes"][0]["price"],2000.);
        assert_eq!(aged["report_stale"],true);
        assert_eq!(aged["report_age_ms"],600_001);
        let mut other=s.clone();other.telemetry.as_mut().unwrap().symbol="XAUUSD".into();
        assert!(h.route("GET","/v1/hvn",b"",&other,now).unwrap()["report"].is_null());
    }
}
