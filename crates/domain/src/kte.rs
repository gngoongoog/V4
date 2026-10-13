//! KTE defaults from the user's KTE_Gold_Scalper_PRO_Improved.pine.
use crate::Candle;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Signal { pub side:i8, pub impulse:usize, pub stop:f64, pub zone_low:f64, pub zone_high:f64 }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame { pub atr:f64, pub ema20:f64, pub ema50:f64, pub regime:bool, pub ready:bool, pub signal:Option<Signal> }
struct Impulse {index:usize,side:i8,close:f64,distance:f64,low:f64,high:f64}
/// Causal M1 series. EMA seeded at first close, ATR14 at mean of first 14 TRs.
/// Candidates keep an impulse ID; the tester consumes each at most once.
pub fn frames(candles:&[Candle],tick:f64)->Result<Vec<Option<Frame>>,String> {
    if !tick.is_finite() || tick<=0.0 || candles.len()>60_000 || candles.iter().any(|c|!c.valid() || c.high>1e12 || c.time%60!=0) || candles.windows(2).any(|w|w[0].time>=w[1].time) {return Err("Invalid KTE candles or tick size".into());}
    let mut out=Vec::with_capacity(candles.len());let mut impulse:Option<Impulse>=None;
    let (mut ema20,mut ema50,mut atr,mut seed)=(0.0,0.0,0.0,0.0);let mut atrs=vec![];let mut contiguous=0;
    for (i,c) in candles.iter().enumerate() {
        if i>0 && c.time!=candles[i-1].time+60 {impulse=None;contiguous=0;} contiguous+=1;
        ema20=if i==0 {c.close}else{ema20+(c.close-ema20)*2.0/21.0};
        ema50=if i==0 {c.close}else{ema50+(c.close-ema50)*2.0/51.0};
        let tr=if i==0 {c.high-c.low}else{(c.high-c.low).max((c.high-candles[i-1].close).abs()).max((c.low-candles[i-1].close).abs())};seed+=tr;
        if i<13 {out.push(None);continue;}
        atr=if i==13 {seed/14.0}else{(atr*13.0+tr)/14.0};atrs.push(atr);
        let body=(c.close-c.open).abs();
        if atr>0.0 && body>=2.0*atr && body/(c.high-c.low).max(tick)>=0.55 {
            let side=if c.close>c.open {1}else{-1};
            let center=if side==1 {c.close-(c.close-c.low)*0.5}else{c.close+(c.high-c.close)*0.5};
            impulse=Some(Impulse{index:i,side,close:c.close,distance:body,low:center-0.4*atr,high:center+0.4*atr});
        }
        let ratio=if atrs.len()>=50 {atr/(atrs[atrs.len()-50..].iter().sum::<f64>()/50.0)}else{0.0};
        let regime=(0.75..=2.5).contains(&ratio);let ready=i>=99 && contiguous>=30 && atr>0.0;
        let signal=impulse.as_ref().and_then(|p| {
            let age=i-p.index;let slow=(c.close-p.close).abs()/age.max(1) as f64<=p.distance*0.45;
            let overlaps=c.low<=p.high && c.high>=p.low;
            let trend=if p.side==1 {c.close>ema20 && ema20>ema50}else{c.close<ema20 && ema20<ema50};
            let rejects=if p.side==1 {c.close>c.open || c.open.min(c.close)-c.low>=body}else{c.close<c.open || c.high-c.open.max(c.close)>=body};
            let intact=if p.side==1 {c.close>=p.low-0.75*atr}else{c.close<=p.high+0.75*atr};
            if ready && regime && (5..=30).contains(&age) && slow && overlaps && trend && rejects && intact {
                Some(Signal{side:p.side,impulse:p.index,stop:if p.side==1 {c.low.min(p.low)-0.35*atr}else{c.high.max(p.high)+0.35*atr},zone_low:p.low,zone_high:p.high})
            }else{None}
        });
        out.push(Some(Frame{atr,ema20,ema50,regime,ready,signal}));
    }
    Ok(out)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn fixture()->Vec<Candle> {(0..120).map(|i| {
        let p=100.0+i as f64*0.02;
        let(open,high,low,close)=if i<100 {(p,p+0.12,p-0.08,p+0.02)}else if i==100 {(102.0,103.05,101.95,103.0)}else if i<105 {(102.85,103.0,102.7,102.8)}else{(102.65,102.85,102.4,102.75)};
        Candle{time:1_728_000_000+i*60,open,high,low,close,tick_volume:100}
    }).collect()}
    #[test]
    fn known_impulse_zone_and_mirrored_short() {
        let b=fixture();let f=frames(&b,0.01).unwrap();assert!(f[104].as_ref().unwrap().signal.is_none());
        let s=f[105].as_ref().unwrap().signal.as_ref().unwrap();assert_eq!((s.side,s.impulse),(1,100));
        assert!((s.zone_low-(102.475-0.4*(0.2*13.0+1.1)/14.0)).abs()<1e-10);
        assert!((s.stop-(s.zone_low-0.35*f[105].as_ref().unwrap().atr)).abs()<1e-10);
        let reversed:Vec<_>=b.iter().map(|c|Candle{open:300.0-c.open,close:300.0-c.close,high:300.0-c.low,low:300.0-c.high,..c.clone()}).collect();
        let rf=frames(&reversed,0.01).unwrap();let sell=rf[105].as_ref().unwrap().signal.as_ref().unwrap();assert_eq!(sell.side,-1);assert!((sell.stop-(300.0-s.stop)).abs()<1e-10);
    }
    #[test]
    fn causal_prefix_and_gap_clear_impulse() {
        let b=fixture();let f=frames(&b,0.01).unwrap();assert_eq!(frames(&b[..107],0.01).unwrap(),f[..107]);
        let mut gap=b.clone();gap.remove(103);assert!(frames(&gap,0.01).unwrap()[104].as_ref().unwrap().signal.is_none());
        assert!(!f[98].as_ref().unwrap().ready);assert!(frames(&b,0.0).is_err());
    }
}
