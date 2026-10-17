//! Main coordinator consumes only current advisor members. SOLO bypasses coordination.
#[path = "execution_legacy.rs"]
mod legacy;
pub use legacy::{brain_for_magic, magic, BrainConfig, Signal, BRAINS};
use crate::{studio::{FrameBar, Genome}, trading::{self, TradingMode}, Config, Telemetry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LearnedWeight {
    pub samples:u64,
    pub wins:u64,
    pub mean_net_per_lot:f64,
    /// Reward V2: bounded cost-adjusted R signal.
    pub mean_reward:f64,
    /// Exact closed-position samples credited from MT5 Demo broker deals.
    pub broker_samples:u64,
    /// Causal market-forward samples scored from later completed bars.
    pub forward_samples:u64,
    /// Samples matching the currently observed market regime.
    pub regime_samples:u64,
    /// Time-decayed evidence mass; recent observations count more.
    pub effective_samples:f64,
    /// Recency-weighted win rate. Old wins/losses fade with the same evidence decay.
    pub weighted_win_rate:f64,
    /// Evidence confidence, not a calibrated probability of trade success.
    pub confidence:f64,
    /// Current market context copied from the reconciled learner snapshot.
    pub regime:String,
    pub session:String,
    pub direction:i8,
    pub atr:f64,
    pub volatility_ratio:f64,
    pub spread_to_atr:f64,
    pub execution_quality:f64,
    /// Shrunk quality learned from recent real broker execution outcomes.
    pub execution_history_quality:f64,
    pub quote_freshness:f64,
    /// Exact broker evidence when available, otherwise causal forward evidence.
    pub evidence_source:String,
}
impl LearnedWeight {
    /// V3 combines Bayesian win evidence with cost-adjusted Reward V2 and shrinks every
    /// learned adjustment toward neutral when the evidence is uncertain.
    pub fn weight(&self)->f64 {
        if self.samples==0 {return 1.0;}
        let p=if self.effective_samples>0. {
            (self.weighted_win_rate.clamp(0.,1.)*self.effective_samples+8.)/(self.effective_samples+16.)
        } else {
            (self.wins as f64+8.)/(self.samples as f64+16.)
        };
        let raw=1.0+(p-0.5)*0.9+self.mean_reward.clamp(-1.,1.)*0.45;
        let mut confidence=self.confidence.clamp(0.,1.);
        if self.broker_samples>0 {confidence=confidence.max(0.15);}
        (1.0+(raw-1.0)*confidence).clamp(0.55,1.60)
    }
    pub fn intelligence(&self)->IntelligenceContext {
        IntelligenceContext{
            regime:if self.regime.is_empty(){"unknown".into()}else{self.regime.clone()},
            session:if self.session.is_empty(){"unknown".into()}else{self.session.clone()},
            direction:self.direction,atr:self.atr,volatility_ratio:self.volatility_ratio,
            spread_to_atr:self.spread_to_atr,execution_quality:self.execution_quality,
            execution_history_quality:self.execution_history_quality,quote_freshness:self.quote_freshness,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct IntelligenceContext {
    pub regime:String,pub session:String,pub direction:i8,pub atr:f64,
    pub volatility_ratio:f64,pub spread_to_atr:f64,pub execution_quality:f64,pub execution_history_quality:f64,pub quote_freshness:f64,
}
impl Default for IntelligenceContext {
    fn default()->Self{Self{regime:"unknown".into(),session:"unknown".into(),direction:0,atr:0.,volatility_ratio:1.,spread_to_atr:1.,execution_quality:0.5,execution_history_quality:0.5,quote_freshness:0.5}}
}
fn session_name(now:u64)->String{
    let hour=(now/3_600_000%24) as u32;
    match hour{0..=5=>"asia",6..=11=>"london",12..=15=>"london_new_york_overlap",16..=20=>"new_york",_=>"off_hours"}.into()
}
pub fn intelligence_context(t:&Telemetry,frames:&BTreeMap<String,Vec<FrameBar>>,now:u64)->IntelligenceContext{
    let chosen=frames.get("M5").or_else(||frames.get("M1")).or_else(||frames.values().find(|bars|bars.len()>=60));
    let Some(bars)=chosen else{return IntelligenceContext{session:session_name(now),..Default::default()};};
    let candles:Vec<_>=bars.iter().map(|bar|bar.candle.clone()).collect();
    let atr=trading::volatility(&candles);let direction=trading::direction(&candles);let tail=candles.len().min(60);
    let mean_range=if tail>0{candles[candles.len()-tail..].iter().map(|bar|(bar.high-bar.low).max(0.)).sum::<f64>()/tail as f64}else{0.};
    let volatility_ratio=if atr>0.&&mean_range>0.{(atr/mean_range).clamp(0.,5.)}else{1.};
    let vol=if volatility_ratio>=1.35{"volatile"}else if volatility_ratio<=0.75{"calm"}else{"normal"};
    let trend=if direction>0{"trend_up"}else if direction<0{"trend_down"}else{"range"};
    let spread=t.bid.zip(t.ask).map(|(bid,ask)|(ask-bid).max(0.)).unwrap_or(f64::INFINITY);
    let spread_to_atr=if atr>0.&&spread.is_finite(){(spread/atr).clamp(0.,10.)}else{10.};
    let age=t.tick_age_ms.unwrap_or(3000).min(3000) as f64;let quote_freshness=(1.-age/3000.).clamp(0.,1.);
    let spread_quality=(1.-spread_to_atr*4.).clamp(0.,1.);let execution_quality=(0.65*spread_quality+0.35*quote_freshness).clamp(0.,1.);
    IntelligenceContext{regime:format!("{trend}_{vol}"),session:session_name(now),direction,atr,volatility_ratio,spread_to_atr,execution_quality,execution_history_quality:0.5,quote_freshness}
}
pub fn evaluate(b:&BrainConfig,t:&Telemetry,c:&Config,frames:&BTreeMap<String,Vec<FrameBar>>,genome:Option<&Genome>,now:u64)->Signal {legacy::evaluate(b,t,c,frames,genome,now)}

pub fn coordinate(mut primary:Signal,c:&Config,votes:Vec<(BrainConfig,Signal,LearnedWeight)>)->Signal {
    let intelligence=votes.iter().map(|(_,_,w)|w.intelligence()).find(|ctx|ctx.regime!="unknown").unwrap_or_default();
    let mut buy=0.;let mut sell=0.;let mut confidence_sum=0.;let mut ready_count=0u64;let mut members=vec![];
    for (b,s,w) in votes.into_iter().filter(|(b,_,_)|b.id!="system"&&!b.enabled) {
        let weight=w.weight();
        if s.ready {if s.side==1{buy+=weight;}else if s.side==-1{sell+=weight;}confidence_sum+=w.confidence.clamp(0.,1.);ready_count+=1;}
        members.push(serde_json::json!({"id":b.id,"timeframe":b.timeframe,"report_id":b.report_id,"bar_time":s.bar_time,"signal_key":s.key,"side":s.side,"ready":s.ready,"weight":weight,"samples":w.samples,"wins":w.wins,"mean_net_per_lot":w.mean_net_per_lot,"mean_reward":w.mean_reward,"weighted_win_rate":w.weighted_win_rate,"broker_samples":w.broker_samples,"forward_samples":w.forward_samples,"regime_samples":w.regime_samples,"effective_samples":w.effective_samples,"confidence":w.confidence,"regime":w.regime,"execution_history_quality":w.execution_history_quality,"evidence_source":w.evidence_source,"reason":s.reason}));
    }
    let total=buy+sell;let net=if total>0.{(buy-sell)/total}else{0.};
    let evidence_confidence=if ready_count>0{confidence_sum/ready_count as f64}else{0.};
    let disagreement=if total>0.{1.-net.abs()}else{1.};
    let uncertainty=(0.55*(1.-evidence_confidence)+0.45*disagreement).clamp(0.,1.);
    let meta_edge=net.abs()*intelligence.execution_quality.clamp(0.,1.)*(1.-0.5*uncertainty);
    let required_net=(0.20+0.05*(1.-intelligence.execution_quality.clamp(0.,1.))+0.05*uncertainty).clamp(0.20,0.30);
    let expert=c.trading.mode==TradingMode::Expert;
    if expert {
        primary.side=if net>=required_net{1}else if net<=-required_net{-1}else{0};
        primary.ready=primary.side!=0&&primary.stop_distance>0.&&primary.target_distance>0.&&members.iter().any(|v|v["ready"]==true);
    } else if primary.ready {
        let against=if primary.side==1{sell}else{buy};let with=if primary.side==1{buy}else{sell};
        if against>=2.&&against>with+0.5 {primary.ready=false;}
    }
    let broker_members=members.iter().filter(|v|v["broker_samples"].as_u64().unwrap_or(0)>0).count();
    primary.reason=format!("{} | Intelligence V3: شراء {:.2} / بيع {:.2}؛ regime={}؛ quality {:.2}؛ uncertainty {:.2}؛ {} أعضاء؛ {} بتعلم MT5 Demo",primary.reason,buy,sell,intelligence.regime,intelligence.execution_quality,uncertainty,members.len(),broker_members);
    primary.coordination=Some(serde_json::json!({
        "version":"braintrade-intelligence-v3","trading_mode":c.trading.mode,"buy":buy,"sell":sell,"net":net,"required_net":required_net,"meta_edge":meta_edge,"evidence_confidence":evidence_confidence,"uncertainty":uncertainty,"intelligence":intelligence,"members":members,
        "reward_model":"Reward V2: net broker/forward R after costs, bounded for robust weighting",
        "recency_model":"time-decayed evidence; recent observations contribute more than old observations",
        "regime_model":"trend/range plus volatility regime selects relevant evidence when available",
        "execution_quality_model":"soft meta-threshold from spread/ATR, quote freshness and shrunk recent real execution outcomes; broker/cost checks remain authoritative",
        "counterfactual_model":"actionable advisor alternatives mature causally after 12 completed bars and never masquerade as broker fills",
        "forward_weight_horizon":"12 later completed bars of each member timeframe",
        "broker_evidence":"exact MT5 Demo closed-position booked PnL including commission, swap and fee; credited only to ready members that supported the executed coordinator side"
    }));
    primary
}

#[cfg(test)] mod tests {
    use super::*;
    fn signal(id:&str,side:i8)->Signal {Signal{brain:id.into(),side,key:"k".into(),bar_time:1,stop_distance:1.,target_distance:2.,hold_ms:1000,reason:"fixture".into(),ready:side!=0,coordination:None}}
    fn learned(wins:u64,samples:u64,reward:f64,confidence:f64,quality:f64)->LearnedWeight{LearnedWeight{wins,samples,mean_reward:reward,confidence,effective_samples:samples as f64,weighted_win_rate:if samples>0{wins as f64/samples as f64}else{0.},execution_quality:quality,execution_history_quality:0.5,regime:"trend_up_normal".into(),session:"london".into(),evidence_source:"fixture".into(),..Default::default()}}
    #[test]fn solo_removed_from_votes_and_returned_member_restored(){
        let c=Config::default();let a=BrainConfig{id:"trend".into(),..Default::default()};let mut b=BrainConfig{id:"gann".into(),enabled:true,..Default::default()};
        let r=coordinate(signal("system",0),&c,vec![(a.clone(),signal("trend",1),learned(20,30,0.2,0.8,1.)),(b.clone(),signal("gann",-1),Default::default())]);assert_eq!(r.side,1);let coordination=r.coordination.unwrap();assert_eq!(coordination["members"].as_array().unwrap().len(),1);assert_eq!(coordination["version"],"braintrade-intelligence-v3");
        b.enabled=false;let r=coordinate(signal("system",0),&c,vec![(a,signal("trend",1),Default::default()),(b,signal("gann",-1),Default::default())]);assert!(!r.ready);assert_eq!(r.side,0);
    }
    #[test]fn reward_and_confidence_change_weight_without_one_sample_dominance(){
        assert!(learned(24,30,0.45,0.8,1.).weight()>1.);assert!(learned(6,30,-0.45,0.8,1.).weight()<1.);
        let one=LearnedWeight{samples:1,wins:1,mean_reward:1.,broker_samples:1,confidence:0.02,..Default::default()}.weight();assert!(one>1.&&one<1.2);
    }
    #[test]fn poorer_execution_quality_requires_more_consensus(){
        let c=Config::default();let a=BrainConfig{id:"trend".into(),..Default::default()};let b=BrainConfig{id:"gann".into(),..Default::default()};
        let good=coordinate(signal("system",0),&c,vec![(a.clone(),signal("trend",1),learned(18,25,0.2,0.8,1.)),(b.clone(),signal("gann",-1),learned(10,25,-0.1,0.8,1.))]);
        let poor=coordinate(signal("system",0),&c,vec![(a,signal("trend",1),learned(18,25,0.2,0.8,0.)),(b,signal("gann",-1),learned(10,25,-0.1,0.8,0.))]);
        assert!(good.coordination.as_ref().unwrap()["required_net"].as_f64().unwrap()<poor.coordination.as_ref().unwrap()["required_net"].as_f64().unwrap());
    }
    #[test]fn recency_weighted_wins_override_stale_raw_win_count(){
        let stale=LearnedWeight{samples:100,wins:90,effective_samples:5.0,weighted_win_rate:0.20,confidence:1.0,mean_reward:0.0,..Default::default()};
        assert!(stale.weight()<1.0);
        let recent=LearnedWeight{samples:100,wins:10,effective_samples:5.0,weighted_win_rate:0.80,confidence:1.0,mean_reward:0.0,..Default::default()};
        assert!(recent.weight()>1.0);
    }

}
