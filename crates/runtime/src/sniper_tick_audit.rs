//! P1-K2: passive, bounded Sniper accepted-pulse provenance, not a trade sampler.
//! The Core observes only the quotes actually accepted by its Fast Pulse.
//! It CANNOT determine how many market ticks were missed between observations.
//! Deliberately memory-only: no SQLite writes or broker-path IO per hot pulse.
use braintrade_domain::Telemetry;
use serde::Deserialize;
use serde_json::{json,Value};

const OBSERVATION_GAP_MS:u64=1500;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    // Only fields additional to the already validated Fast Pulse contract
    // are interpreted; unknown trader-controlled content is not trusted.
    #[serde(default)]
    quote_source:Option<String>,
    #[serde(default)]
    native_dropped_total:Option<u64>,
}
#[derive(Default)]
pub struct Audit {
    context:String,
    first_observed_ms:Option<u64>,
    last_observed_ms:Option<u64>,
    last_source_ms:Option<u64>,
    last_event_ms:Option<u64>,
    last_source:String,
    native_baseline:Option<u64>,
    native_last:Option<u64>,
    accepted:u64,
    native:u64,
    poll:u64,
    legacy_unknown:u64,
    source_switches:u64,
    wide_observed_gaps:u64,
    maximum_observed_gap_ms:u64,
    same_broker_millisecond:u64,
    reported_native_overflow_delta:u64,
    native_counter_resets:u64,
}
fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
impl Audit {
    pub fn clear_on_identity_change(&mut self,t:&Telemetry){
        let key=context(t);
        if !self.context.is_empty() && self.context!=key {
            *self=Self::default();
        }
    }
    pub fn observe(&mut self,t:&Telemetry,payload:&[u8],now:u64){
        if !t.connected||!t.demo||t.account_trade_mode!=Some(0)
            ||t.login==0||t.server.is_empty()||t.currency.is_empty()
            ||t.symbol.is_empty(){return}
        let key=context(t);
        if self.context!=key{*self=Self::default();self.context=key;}
        // Payload is a verified, accepted Pulse. The wire fields above are
        // optional for older gateways: they remain unknown rather than Native.
        let extras:Value=serde_json::from_slice(payload).unwrap_or(Value::Null);
        let src=extras["quote_source"].as_str().unwrap_or("unknown");
        let src=match src {"native"=>"native","poll"=>"poll",_=>"unknown"};
        let Some(source_ms)=t.source_quote_time_ms else{
            self.legacy_unknown+=1;self.accepted+=1;
            self.last_event_ms=t.quote_time_ms;
            self.last_observed_ms=Some(now);self.first_observed_ms.get_or_insert(now);
            return
        };
        if self.last_source_ms.is_some_and(|prev|source_ms<prev) {return}
        if let Some(prev)=self.last_source_ms{
            let gap=source_ms-prev;
            self.maximum_observed_gap_ms=self.maximum_observed_gap_ms.max(gap);
            if gap>OBSERVATION_GAP_MS{self.wide_observed_gaps+=1}
            if gap==0{self.same_broker_millisecond+=1}
        }
        if !self.last_source.is_empty() && self.last_source!=src{
            self.source_switches+=1;
        }
        match src {
            "native"=>self.native+=1,"poll"=>self.poll+=1,_=>self.legacy_unknown+=1
        }
        // The mailbox counter is process-global, not per-account. Take an
        // initial baseline on this bound session; never attribute past drops.
        if let Some(drops)=extras["native_dropped_total"].as_u64(){
            if let Some(old)=self.native_last{
                if drops>=old{self.reported_native_overflow_delta=
                    self.reported_native_overflow_delta.saturating_add(drops-old);}
                else{self.native_counter_resets+=1;}
            }else{self.native_baseline=Some(drops)}
            self.native_last=Some(drops);
        }
        self.accepted+=1;self.last_observed_ms=Some(now);
        self.first_observed_ms.get_or_insert(now);
        self.last_source_ms=Some(source_ms);
        self.last_event_ms=t.quote_time_ms;
        self.last_source=src.to_string();
    }
    pub fn report(&self,account:Option<&Telemetry>,now:u64)->Value{
        let Some(t)=account.filter(|t|t.connected&&t.demo
            &&t.account_trade_mode==Some(0)&&!self.context.is_empty()
            &&context(t)==self.context)
        else{return json!({"schema":"p1k2-sniper-accepted-pulse-audit-v1",
            "status":"unavailable","observed_market_tick_count":Value::Null,
            "read_only":true,"no_broker_orders":true})};
        json!({
            "schema":"p1k2-sniper-accepted-pulse-audit-v1",
            "status":if self.accepted>0{"observed"}else{"insufficient"},
            "context":self.context,
            "account":{"server":t.server,"login":t.login,"symbol":t.symbol,"currency":t.currency},
            "reported_at_ms":now,
            "first_accepted_ms":self.first_observed_ms,
            "last_accepted_ms":self.last_observed_ms,
            "last_broker_quote_ms":self.last_source_ms,
            "last_event_key_ms":self.last_event_ms,
            "last_observed_source":self.last_source,
            "accepted_pulses":self.accepted,
            "accepted_native_pulses":self.native,
            "accepted_poll_fallback_pulses":self.poll,
            "accepted_unknown_origin_pulses":self.legacy_unknown,
            "native_poll_source_switches":self.source_switches,
            "observed_quote_gaps_gt_1500ms":self.wide_observed_gaps,
            "largest_observed_quote_gap_ms":self.maximum_observed_gap_ms,
            "same_broker_millisecond_pulses":self.same_broker_millisecond,
            "reported_native_mailbox_overflow_since_baseline":if self.native_baseline.is_some(){
                Some(self.reported_native_overflow_delta)}else{None},
            "native_mailbox_counter_resets":self.native_counter_resets,
            "observed_market_tick_count":Value::Null,
            "estimated_missing_market_ticks":Value::Null,
            "broker_tick_delivery_complete":false,
            "persistence":"memory_only_reset_on_process_restart_or_broker_identity_change",
            "read_only":true,"no_broker_orders":true,"learning_effect":false,
            "note":"Only accepted Core pulse events after a verified quote; quote gaps are time intervals NOT counts of missing market ticks. Overflow counts are gateway mailbox drops after this account's first observed baseline, not exchange feed completeness. No Tick-forward policy comparison or PnL."
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn t(login:u64,source:u64,key:u64)->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login,server:"Demo".into(),currency:"USD".into(),
            symbol:"XAUUSD".into(),source_quote_time_ms:Some(source),
            quote_time_ms:Some(key),..Default::default()}
    }
    fn pulse(src:&str,drops:Option<u64>)->Vec<u8>{
        let mut p=json!({"quote_source":src});
        if let Some(d)=drops{p["native_dropped_total"]=json!(d)}
        p.to_string().into_bytes()
    }
    #[test]
    fn native_poll_gap_dense_clock_and_drop_delta_are_descriptive_only(){
        let mut a=Audit::default();
        a.observe(&t(17,1000,1000),&pulse("native",Some(9)),1002);
        a.observe(&t(17,1000,1001),&pulse("native",Some(10)),1003);
        a.observe(&t(17,1100,1100),&pulse("poll",Some(12)),1102);
        a.observe(&t(17,4000,4000),&pulse("native",Some(12)),4001);
        let r=a.report(Some(&t(17,4000,4000)),4005);
        assert_eq!(r["accepted_pulses"],4);
        assert_eq!(r["accepted_native_pulses"],3);
        assert_eq!(r["accepted_poll_fallback_pulses"],1);
        assert_eq!(r["native_poll_source_switches"],2);
        assert_eq!(r["same_broker_millisecond_pulses"],1);
        assert_eq!(r["observed_quote_gaps_gt_1500ms"],1);
        assert_eq!(r["largest_observed_quote_gap_ms"],2900);
        assert_eq!(r["reported_native_mailbox_overflow_since_baseline"],3);
        assert_eq!(r["estimated_missing_market_ticks"],Value::Null);
        assert_eq!(r["broker_tick_delivery_complete"],false);
    }
    #[test]
    fn account_change_and_legacy_quote_do_not_inherit_native_evidence(){
        let mut a=Audit::default();
        a.observe(&t(17,1000,1000),&pulse("native",Some(300)),1003);
        a.clear_on_identity_change(&t(18,1100,1100));
        assert_eq!(a.report(Some(&t(17,1000,1000)),1003)["status"],"unavailable");
        a.observe(&t(18,1100,1100),&pulse("poll",Some(305)),1102);
        let r=a.report(Some(&t(18,1100,1100)),1103);
        assert_eq!(r["accepted_pulses"],1);
        assert_eq!(r["reported_native_mailbox_overflow_since_baseline"],0);
        assert_eq!(a.report(Some(&t(17,1000,1000)),1103)["status"],"unavailable");
        let mut legacy=t(18,1110,1110);legacy.source_quote_time_ms=None;
        a.observe(&legacy,&[],1111);
        assert_eq!(a.report(Some(&legacy),1112)["accepted_unknown_origin_pulses"],1);
    }
    #[test]
    fn missing_or_lower_drop_counter_is_not_invented_loss(){
        let mut a=Audit::default();
        a.observe(&t(17,100,100),&pulse("native",None),100);
        a.observe(&t(17,101,101),&pulse("native",Some(90)),101);
        a.observe(&t(17,102,102),&pulse("native",Some(5)),102);
        let r=a.report(Some(&t(17,102,102)),103);
        assert_eq!(r["native_mailbox_counter_resets"],1);
        assert_eq!(r["reported_native_mailbox_overflow_since_baseline"],0);
    }
    #[test]
    fn real_disconnected_and_mismatched_account_never_show_old_evidence(){
        let mut a=Audit::default();
        a.observe(&t(17,100,100),&pulse("native",Some(1)),100);
        let mut real=t(17,100,100);real.demo=false;
        assert_eq!(a.report(Some(&real),101)["status"],"unavailable");
        assert_eq!(a.report(Some(&t(18,100,100)),101)["status"],"unavailable");
        assert_eq!(a.report(None,101)["status"],"unavailable");
    }
}
