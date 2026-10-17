//! Main coordinator consumes only current advisor members. SOLO bypasses coordination.
#[path = "execution_legacy.rs"]
mod legacy;
pub use legacy::{brain_for_magic, magic, BrainConfig, Signal, BRAINS};
use crate::{studio::{FrameBar, Genome}, trading::TradingMode, Config, Telemetry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LearnedWeight {
    pub samples:u64,
    pub wins:u64,
    pub mean_net_per_lot:f64,
    /// Exact closed-position samples credited from MT5 Demo broker deals.
    pub broker_samples:u64,
    /// Causal market-forward samples scored from later completed bars.
    pub forward_samples:u64,
    /// `mt5_demo_broker` when real broker evidence is available, otherwise `forward_bars`.
    pub evidence_source:String,
}
impl LearnedWeight {
    /// Real Demo broker evidence is allowed to influence the coordinator immediately, but
    /// remains strongly Bayesian-shrunk. Forward-bar evidence keeps the established neutral
    /// warmup until 20 matured observations. This is an adaptive evidence weight, not a
    /// calibrated trade-success probability.
    pub fn weight(&self)->f64 {
        if self.broker_samples>0 {
            let p=(self.wins as f64+10.)/(self.samples as f64+20.);
            (0.5+p).clamp(0.5,1.5)
        } else if self.samples<20 {
            1.0
        } else {
            let p=(self.wins as f64+10.)/(self.samples as f64+20.);
            (0.5+p).clamp(0.5,1.5)
        }
    }
}
pub fn evaluate(b:&BrainConfig,t:&Telemetry,c:&Config,frames:&BTreeMap<String,Vec<FrameBar>>,genome:Option<&Genome>,now:u64)->Signal {legacy::evaluate(b,t,c,frames,genome,now)}

pub fn coordinate(mut primary:Signal,c:&Config,votes:Vec<(BrainConfig,Signal,LearnedWeight)>)->Signal {
    let mut buy=0.;let mut sell=0.;let mut members=vec![];
    for (b,s,w) in votes.into_iter().filter(|(b,_,_)|b.id!="system"&&!b.enabled) {
        let weight=w.weight();
        if s.ready {if s.side==1{buy+=weight;}else if s.side==-1{sell+=weight;}}
        members.push(serde_json::json!({
            "id":b.id,
            "timeframe":b.timeframe,
            "report_id":b.report_id,
            "side":s.side,
            "ready":s.ready,
            "weight":weight,
            "samples":w.samples,
            "wins":w.wins,
            "mean_net_per_lot":w.mean_net_per_lot,
            "broker_samples":w.broker_samples,
            "forward_samples":w.forward_samples,
            "evidence_source":w.evidence_source,
            "reason":s.reason
        }));
    }
    let net=if buy+sell>0. {(buy-sell)/(buy+sell)}else{0.};
    let expert=c.trading.mode==TradingMode::Expert;
    // Expert selects direction from actual members. Other modes retain their market trigger
    // and use the same adaptive member evidence for conflict resolution.
    if expert {
        primary.side=if net>=0.20{1}else if net<= -0.20{-1}else{0};
        primary.ready=primary.side!=0&&primary.stop_distance>0.&&primary.target_distance>0.&&members.iter().any(|v|v["ready"]==true);
    } else if primary.ready {
        let against=if primary.side==1{sell}else{buy};let with=if primary.side==1{buy}else{sell};
        if against>=2.&&against>with+0.5 {primary.ready=false;}
    }
    let broker_members=members.iter().filter(|v|v["broker_samples"].as_u64().unwrap_or(0)>0).count();
    primary.reason=format!("{} | المنسق: شراء {:.2} / بيع {:.2}؛ {} أعضاء؛ {} أعضاء لديهم تعلم حقيقي من صفقات MT5 Demo المغلقة",primary.reason,buy,sell,members.len(),broker_members);
    primary.coordination=Some(serde_json::json!({
        "version":"adaptive-member-coordinator-v2",
        "trading_mode":c.trading.mode,
        "buy":buy,
        "sell":sell,
        "net":net,
        "members":members,
        "forward_weight_horizon":"12 later completed bars of each member timeframe",
        "forward_minimum_weight_samples":20,
        "broker_evidence":"exact MT5 Demo closed-position booked PnL including commission, swap and fee; credited only to ready members that supported the executed coordinator side"
    }));
    primary
}

#[cfg(test)] mod tests {
    use super::*;
    fn signal(id:&str,side:i8)->Signal {Signal{brain:id.into(),side,key:"k".into(),bar_time:1,stop_distance:1.,target_distance:2.,hold_ms:1000,reason:"fixture".into(),ready:side!=0,coordination:None}}
    #[test]fn solo_removed_from_votes_and_returned_member_restored(){
        let c=Config::default();let a=BrainConfig{id:"trend".into(),..Default::default()};let mut b=BrainConfig{id:"gann".into(),enabled:true,..Default::default()};
        let r=coordinate(signal("system",0),&c,vec![(a.clone(),signal("trend",1),Default::default()),(b.clone(),signal("gann",-1),Default::default())]);assert_eq!(r.side,1);let coordination=r.coordination.unwrap();assert_eq!(coordination["members"].as_array().unwrap().len(),1);assert_eq!(coordination["trading_mode"],"expert");
        b.enabled=false;let r=coordinate(signal("system",0),&c,vec![(a,signal("trend",1),Default::default()),(b,signal("gann",-1),Default::default())]);assert!(!r.ready);assert_eq!(r.side,0);
    }
    #[test]fn weights_need_forward_evidence_but_real_broker_samples_move_immediately(){
        assert_eq!(LearnedWeight{samples:19,wins:19,mean_net_per_lot:3.,forward_samples:19,evidence_source:"forward_bars".into(),..Default::default()}.weight(),1.);
        assert!(LearnedWeight{samples:30,wins:25,mean_net_per_lot:3.,forward_samples:30,evidence_source:"forward_bars".into(),..Default::default()}.weight()>1.);
        assert!(LearnedWeight{samples:30,wins:5,mean_net_per_lot:-3.,forward_samples:30,evidence_source:"forward_bars".into(),..Default::default()}.weight()<1.);
        let win=LearnedWeight{samples:1,wins:1,mean_net_per_lot:5.,broker_samples:1,evidence_source:"mt5_demo_broker".into(),..Default::default()};
        let loss=LearnedWeight{samples:1,wins:0,mean_net_per_lot:-5.,broker_samples:1,evidence_source:"mt5_demo_broker".into(),..Default::default()};
        assert!(win.weight()>1.0);assert!(loss.weight()<1.0);
    }
}