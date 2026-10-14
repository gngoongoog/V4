use super::*;

/// Pivots become knowable only after two completed bars to their right.
/// No pivot, zone or retest is assigned an earlier confirmation time.
pub(super) fn map(s:&[Candle], tf:&str, seconds:u64)->(Vec<Swing>,Vec<Zone>) {
    let mut swings=Vec::new();let mut zones:Vec<Zone>=Vec::new();
    for i in 16..s.len()-2 {
        let a=atr(&s[..=i]); if a<=0. {continue;}
        let c=&s[i];
        for side in [1,-1] {
            let pivot=(i-2..=i+2).filter(|j|*j!=i).all(|j|if side==1 {c.low<s[j].low}else{c.high>s[j].high});
            if !pivot {continue;}
            let confirmed=s[i+2].time+seconds;
            swings.push(Swing {timeframe:tf.into(),time:c.time,confirmed_at:confirmed,side,price:if side==1 {c.low}else{c.high}});
            let departure=if side==1 {(s[i+2].close-c.high)/a}else{(c.low-s[i+2].close)/a};
            let demand=departure>=0.8;
            let (low,high)=if demand {
                if side==1 {(c.low,c.open.max(c.close).min(c.low+1.5*a).max(c.low+0.1*a))}
                else {(c.open.min(c.close).max(c.high-1.5*a).min(c.high-0.1*a),c.high)}
            }else if side==1 {(c.low-0.12*a,c.low+0.12*a)}else{(c.high-0.12*a,c.high+0.12*a)};
            let mut visits=0;let mut inside=true;let mut broken_at=None;
            for b in &s[i+3..] {
                if (side==1 && b.close<low-0.15*a)||(side== -1 && b.close>high+0.15*a) {broken_at=Some(b.time+seconds);break;}
                let hit=b.high>=low && b.low<=high;
                if hit&&!inside {visits+=1;}
                inside=hit;
            }
            let age=(s.len()-i-3) as f64;
            let strength=(0.45+0.25*(departure.max(0.)/2.).min(1.)+0.20/(1.+age/50.)-0.07*visits as f64).clamp(0.1,0.95);
            let z=Zone {id:format!("{tf}:{}:{side}",c.time),timeframe:tf.into(),
                kind:if demand {if side==1 {"طلب"}else{"عرض"}}else if side==1 {"دعم"}else{"مقاومة"}.into(),
                side,low,high,created_at:c.time,confirmed_at:confirmed,touches:visits,strength,broken:broken_at.is_some(),broken_at};
            // Keep the stronger location when nearly identical pivots overlap.
            if let Some(existing)=zones.iter_mut().find(|old|old.side==side&&old.broken==z.broken
                && (old.low-z.low).abs()<0.2*a && (old.high-z.high).abs()<0.2*a) {
                if z.strength>existing.strength {*existing=z;}
            }else{zones.push(z);}
        }
    }
    zones.sort_by(|a,b|b.confirmed_at.cmp(&a.confirmed_at));
    // Retain broken levels too: they anchor breakout/retest patterns.
    zones.truncate(18);
    if swings.len()>40 {swings.drain(..swings.len()-40);}
    (swings,zones)
}
