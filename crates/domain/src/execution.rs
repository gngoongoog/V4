//! Main coordinator consumes only current advisor members. SOLO bypasses coordination.
#[path = "execution_legacy.rs"]
mod legacy;
pub use legacy::{brain_for_magic, magic, BrainConfig, Signal, BRAINS};
use crate::{studio::{FrameBar, Genome}, trading::TradingMode, Config, Telemetry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LearnedWeight {pub samples:u64,pub wins:u64,pub mean_net_per_lot:f64}
impl LearnedWeight {
    /// Shrunk forward-outcome frequency, bounded and neutral before 20 matured observations.
    /// This is an adaptive evidence weight, not a calibrated trade-success probability.
    pub fn weight(&self)->f64 {
        if self.samples<20 {1.0} else {let p=(self.wins as f64+10.)/(self.samples as f64+20.);(0.5+p).clamp(0.5,1.5)}
    }
}
pub fn evaluate(b:&BrainConfig,t:&Telemetry,c:&Config,frames:&BTreeMap<String,Vec<FrameBar>>,genome:Option<&Genome>,now:u64)->Signal {legacy::evaluate(b,t,c,frames,genome,now)}

pub fn coordinate(mut primary:Signal,c:&Config,votes:Vec<(BrainConfig,Signal,LearnedWeight)>)->Signal {
    let mut buy=0.;let mut sell=0.;let mut members=vec![];
    for (b,s,w) in votes.into_iter().filter(|(b,_,_)|b.id!="system"&&!b.enabled) {
        let weight=w.weight();
        if s.ready {if s.side==1{buy+=weight;}else if s.side==-1{sell+=weight;}}
        members.push(serde_json::json!({"id":b.id,"timeframe":b.timeframe,"side":s.side,"ready":s.ready,"weight":weight,"samples":w.samples,"wins":w.wins,"mean_net_per_lot":w.mean_net_per_lot,"reason":s.reason}));
    }
    let net=if buy+sell>0. {(buy-sell)/(buy+sell)}else{0.};
    let expert=c.trading.mode==TradingMode::Expert;
    // Expert selects direction from actual members. Other modes retain their market trigger
    // and use adaptive member evidence for conflict resolution, except Sniper which executes its own tick trigger.
    if expert {
        primary.side=if net>=0.20{1}else if net<= -0.20{-1}else{0};
        primary.ready=primary.side!=0&&primary.stop_distance>0.&&primary.target_distance>0.&&members.iter().any(|v|v["ready"]==true);
    } else if primary.ready && c.trading.mode!=TradingMode::Sniper {
        let against=if primary.side==1{sell}else{buy};let with=if primary.side==1{buy}else{sell};
        if against>=2.&&against>with+0.5 {primary.ready=false;}
    }
    primary.reason=format!("{} | المنسق: شراء {:.2} / بيع {:.2}؛ {} أعضاء؛ أوزان تعلم من نتائج لاحقة فقط",primary.reason,buy,sell,members.len());
    primary.coordination=Some(serde_json::json!({"version":"adaptive-member-coordinator-v1","buy":buy,"sell":sell,"net":net,"members":members,"weight_horizon":"12 later completed bars of each member timeframe","minimum_weight_samples":20}));
    primary
}

#[cfg(test)] mod tests {
    use super::*;
    fn signal(id:&str,side:i8)->Signal {Signal{brain:id.into(),side,key:"k".into(),bar_time:1,stop_distance:1.,target_distance:2.,hold_ms:1000,reason:"fixture".into(),ready:side!=0,coordination:None}}
    #[test]fn solo_removed_from_votes_and_returned_member_restored(){
        let c=Config::default();let a=BrainConfig{id:"trend".into(),..Default::default()};let mut b=BrainConfig{id:"gann".into(),enabled:true,..Default::default()};
        let r=coordinate(signal("system",0),&c,vec![(a.clone(),signal("trend",1),Default::default()),(b.clone(),signal("gann",-1),Default::default())]);assert_eq!(r.side,1);assert_eq!(r.coordination.unwrap()["members"].as_array().unwrap().len(),1);
        b.enabled=false;let r=coordinate(signal("system",0),&c,vec![(a,signal("trend",1),Default::default()),(b,signal("gann",-1),Default::default())]);assert!(!r.ready);assert_eq!(r.side,0);
    }
    #[test]fn weights_need_forward_evidence_and_are_bounded(){assert_eq!(LearnedWeight{samples:19,wins:19,mean_net_per_lot:3.}.weight(),1.);assert!(LearnedWeight{samples:30,wins:25,mean_net_per_lot:3.}.weight()>1.);assert!(LearnedWeight{samples:30,wins:5,mean_net_per_lot:-3.}.weight()<1.);}
}
