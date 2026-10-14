use super::*;

fn rsi(s:&[Candle])->f64 {
    if s.len()<15 {return 50.;}
    let changes=&s[s.len()-15..];let mut up=0.;let mut down=0.;
    for w in changes.windows(2) {let d=w[1].close-w[0].close;up+=d.max(0.);down+=(-d).max(0.);}
    if up+down<=f64::EPSILON {50.}else{100.*up/(up+down)}
}
pub(super) fn momentum(s:&[Candle],tf:&str,swings:&[Swing])->Momentum {
    let n=s.len();let a=atr(s);let last=s[n-1].close;
    let (roc3,roc8,acc)=if a>0. {((last-s[n-4].close)/a,(last-s[n-9].close)/a,
        (last-s[n-4].close-(s[n-4].close-s[n-7].close))/a)}else{(0.,0.,0.)};
    let r=rsi(s);let direction=if roc3>0.25&&roc8>0.4&&r>52. {1}else if roc3< -0.25&&roc8< -0.4&&r<48. {-1}else{0};
    let strength=(0.5*(roc3.abs()/1.5).min(1.)+0.3*(roc8.abs()/2.).min(1.)+0.2*((r-50.).abs()/25.).min(1.)).clamp(0.,1.);
    let mut divergence=0;
    for side in [1,-1] {
        let pivots:Vec<_>=swings.iter().filter(|p|p.side==side).rev().take(2).collect();
        if pivots.len()!=2 {continue;}
        let newer=pivots[0];let older=pivots[1];
        let ni=s.iter().position(|b|b.time==newer.time).unwrap();
        let oi=s.iter().position(|b|b.time==older.time).unwrap();
        if n-1-ni>20 || oi<14 {continue;}
        let delta=rsi(&s[..=ni])-rsi(&s[..=oi]);
        if ((newer.price-older.price)*side as f64)< -0.1*a && delta*side as f64>5. {divergence=side;}
    }
    Momentum {timeframe:tf.into(),direction,roc3_atr:roc3,roc8_atr:roc8,acceleration_atr:acc,rsi14:r,strength,divergence}
}
pub(super) fn activity(s:&[Candle])->Option<f64> {
    let n=s.len();let baseline=s[n-21..n-1].iter().map(|c|c.tick_volume as f64).sum::<f64>()/20.;
    if baseline<=0. || s[n-1].tick_volume==0 {None}else{Some((s[n-1].tick_volume as f64/baseline).min(20.))}
}
pub(super) fn profile(s:&[Candle])->Profile {
    // Deliberate proxy: distribute each candle's tick activity uniformly across
    // its range. OHLC cannot identify the actual volume traded at a given price.
    let s=&s[s.len().saturating_sub(120)..];
    let low=s.iter().map(|c|c.low).fold(f64::INFINITY,f64::min);
    let high=s.iter().map(|c|c.high).fold(f64::NEG_INFINITY,f64::max);
    let mut out=Profile {coverage_start:s[0].time,bars:s.len(),note:"توزيع تقريبي لنشاط ticks على نطاق شموع M5؛ ليس حجم تداول فعلياً عند السعر ولا أوردر فلو.".into(),..Default::default()};
    if high<=low {return out;}
    let width=(high-low)/32.;let mut bins:[f64;32]=[0.;32];
    for c in s {
        let first=((c.low-low)/width).floor().clamp(0.,31.) as usize;
        let last=((c.high-low)/width).floor().clamp(0.,31.) as usize;
        let allocation=c.tick_volume as f64/(last-first+1) as f64;
        for b in &mut bins[first..=last] {*b+=allocation;}
    }
    let total=bins.iter().sum::<f64>();if total<=0. {return out;}
    let poc=(0..32).max_by(|a,b|bins[*a].total_cmp(&bins[*b])).unwrap();
    let (mut l,mut r)=(poc,poc);let mut mass=bins[poc];
    while mass<0.7*total && (l>0||r<31) {
        if l>0&&(r==31||bins[l-1]>=bins[r+1]) {l-=1;mass+=bins[l];}else{r+=1;mass+=bins[r];}
    }
    out.available=true;out.poc=low+(poc as f64+0.5)*width;out.value_low=low+l as f64*width;out.value_high=low+(r+1) as f64*width;out
}
pub(super) fn session(s:&[Candle])->SessionContext {
    let day=s.last().unwrap().time/86400*86400;let bars:Vec<_>=s.iter().filter(|c|c.time>=day).collect();
    let total=bars.iter().map(|c|c.tick_volume as f64).sum::<f64>();
    SessionContext {day_utc:day,open:bars[0].open,high:bars.iter().map(|c|c.high).fold(f64::NEG_INFINITY,f64::max),
        low:bars.iter().map(|c|c.low).fold(f64::INFINITY,f64::min),coverage_start:bars[0].time,covers_day_open:bars[0].time==day,
        vwap_proxy:if total>0. {Some(bars.iter().map(|c|(c.high+c.low+c.close)/3.*c.tick_volume as f64).sum::<f64>()/total)}else{None}}
}
