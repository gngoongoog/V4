//! P1-K5B foundation: research-only, strictly future quote direction labels.
//! Frozen Sniper Learner signals are observed after warmup, then measured
//! using later broker-timestamped Bid/Ask. This is NOT broker execution,
//! a net-of-cost profitability estimate, or an independent OOS certification.
use braintrade_domain::execution::Signal;
use serde_json::{json, Value};

pub const HORIZON_MS:u64=5_000;
pub const MIN_REPORTED:u64=100;

#[derive(Clone, Debug)]
struct Pending{
    source_broker_ms:u64,
    side:i8,
    mid:f64,
    tick:f64,
}
#[derive(Default)]
pub struct ForwardDirectionAudit{
    pending:Option<Pending>,
    correct:u64,wrong:u64,flat:u64,
    discarded_discontinuous:u64, rejected_invalid_quote:u64,
    candidates:u64,
}
impl ForwardDirectionAudit{
    /// A source switch, missing native events, unpaired reconciled snapshot
    /// or quote gap censors the entire unsettled label. It may never be
    /// scored against a later disconnected quote.
    pub fn invalidate_segment(&mut self){
        if self.pending.take().is_some(){self.discarded_discontinuous+=1;}
    }
    /// Only qualifying frozen signals are opened. A later ACCEPTED quote may
    /// settle the pending candidate even if the current signal is WAIT.
    /// No overlapping evaluation windows and no same-quote label/capture.
    pub fn observe(&mut self,broker_ms:u64,bid:Option<f64>,ask:Option<f64>,
                   tick_size:Option<f64>,candidate:Option<&Signal>){
        let valid=bid.zip(ask).zip(tick_size).and_then(|((bid,ask),tick)|{
            if bid.is_finite()&&ask.is_finite()&&tick.is_finite()
                &&bid>0.&&ask>=bid&&tick>0. {
                let mid=bid+(ask-bid)/2.;
                if mid.is_finite(){Some((mid,tick))}else{None}
            }else{None}
        });
        let Some((mid,tick))=valid else{
            self.rejected_invalid_quote+=1;self.invalidate_segment();return;
        };
        if let Some(pending)=&self.pending {
            if broker_ms<=pending.source_broker_ms {
                self.rejected_invalid_quote+=1;self.invalidate_segment();return;
            }
            if broker_ms-pending.source_broker_ms>=HORIZON_MS {
                let delta=(mid-pending.mid)*f64::from(pending.side);
                // Direction classification: not executable entry/exit geometry.
                // The tolerance is the larger of source/future exchange ticks.
                let threshold=pending.tick.max(tick);
                if delta>=threshold {self.correct+=1;}
                else if delta<=-threshold {self.wrong+=1;}
                else {self.flat+=1;}
                self.pending=None;
            }
            // Whether settled or not, never open a second overlapping label.
            return;
        }
        if let Some(signal)=candidate.filter(|s|
            s.ready&&matches!(s.side,1|-1)
            &&s.stop_distance.is_finite()&&s.stop_distance>0.
            &&s.target_distance.is_finite()&&s.target_distance>0.){
            self.pending=Some(Pending{
                source_broker_ms:broker_ms,side:signal.side,mid,tick,
            });
            self.candidates+=1;
        }
    }
    pub fn json(&self)->Value{
        let scored=self.correct+self.wrong+self.flat;
        json!({
            "schema":"p1k5b-forward-direction-v1",
            "fixed_horizon_ms":HORIZON_MS,
            "nonoverlapping_windows":true,
            "frozen_learner_candidates":self.candidates,
            "resolved_direction_labels":scored,
            "direction_correct":self.correct,
            "direction_opposite":self.wrong,
            "direction_flat":self.flat,
            "pending_unmatured_label":u64::from(self.pending.is_some()),
            "censored_disconnected_segments":self.discarded_discontinuous,
            "rejected_invalid_forward_quotes":self.rejected_invalid_quote,
            "minimum_labels_to_display_fraction":MIN_REPORTED,
            "observed_direction_fraction":if scored>=MIN_REPORTED{
                Some(self.correct as f64/scored as f64)
            }else{None},
            "readout_status":if scored>=MIN_REPORTED{
                "descriptive_only"
            }else{"insufficient_future_labels"},
            "independent_oos":false,
            "profitability_validated":false,
            "broker_fills_observed":false,
            "net_pnl":Value::Null,
            "note":"Frozen policy future directional markouts from accepted broker quotes. 5s midquotes ignore executable spread, commission, slippage, market impact and trading portfolio gates; direction is NOT net profitability, statistical independence or a calibrated win probability."
        })
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn signal(side:i8)->Signal{
        Signal{brain:"sniper_learner".into(),side,ready:true,
            key:"tick:100".into(),bar_time:100,
            stop_distance:0.3,target_distance:0.5,
            hold_ms:60_000,reason:String::new(),coordination:None}
    }
    #[test] fn only_later_quotes_label_nonoverlapping_frozen_direction(){
        let mut f=ForwardDirectionAudit::default();let buy=signal(1);
        f.observe(1_000,Some(2500.),Some(2500.2),Some(0.01),Some(&buy));
        f.observe(1_000,Some(2600.),Some(2600.2),Some(0.01),Some(&buy));
        assert_eq!(f.json()["resolved_direction_labels"],0);
        // Duplicate/rewound quote is censored, never scored as future.
        assert_eq!(f.json()["censored_disconnected_segments"],1);
        f.observe(7_000,Some(2500.),Some(2500.2),Some(0.01),Some(&buy));
        f.observe(11_999,Some(2500.5),Some(2500.7),Some(0.01),None);
        assert_eq!(f.json()["resolved_direction_labels"],0);
        f.observe(12_000,Some(2500.5),Some(2500.7),Some(0.01),Some(&buy));
        assert_eq!(f.json()["resolved_direction_labels"],1);
        assert_eq!(f.json()["direction_correct"],1);
        assert_eq!(f.json()["pending_unmatured_label"],0);
        assert!(f.json()["observed_direction_fraction"].is_null());
        assert_eq!(f.json()["net_pnl"],Value::Null);
        assert_eq!(f.json()["independent_oos"],false);
    }
    #[test] fn continuity_break_discards_unsettled_label_without_credit(){
        let mut f=ForwardDirectionAudit::default();let sell=signal(-1);
        f.observe(1000,Some(2500.),Some(2500.2),Some(0.01),Some(&sell));
        f.invalidate_segment();
        f.observe(10_000,Some(2400.),Some(2400.2),Some(0.01),None);
        assert_eq!(f.json()["censored_disconnected_segments"],1);
        assert_eq!(f.json()["resolved_direction_labels"],0);
        f.observe(12_000,Some(2500.),Some(2500.2),Some(0.01),Some(&sell));
        f.observe(17_000,Some(2500.),Some(2500.2),Some(0.01),None);
        assert_eq!(f.json()["direction_flat"],1);
    }
    #[test] fn invalid_forward_price_cannot_mature_pending_horizon(){
        let mut f=ForwardDirectionAudit::default();
        f.observe(10,Some(2.),Some(2.1),Some(0.01),Some(&signal(1)));
        f.observe(10_000,Some(2.),Some(1.),Some(0.01),None);
        assert_eq!(f.json()["rejected_invalid_forward_quotes"],1);
        assert_eq!(f.json()["censored_disconnected_segments"],1);
        assert_eq!(f.json()["resolved_direction_labels"],0);
    }
}
