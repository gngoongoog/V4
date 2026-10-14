use super::*;

fn add(out:&mut MarketBrains,m:&MarketData,trend:&TrendBrain,regime:&RegimeBrain,z:&Zone,side:i8,pattern:&str,name:&str,trigger:f64,stop:f64,reason:&str) {
    let c=m.m1.last().unwrap();let a=atr(&m.m1);if a<=0. {return;}
    let momentum=&out.momentum[0];
    let confirming=momentum.direction==side || (momentum.acceleration_atr*side as f64>0.15 && (c.close-c.open)*side as f64>0.);
    let activity=out.tick_activity_ratio.map(|v|((v-0.6)/1.4).clamp(0.,1.)).unwrap_or(0.);
    // Four grouped feature families. Pattern variants/extra timeframes are not
    // counted as independent voters; regime is a gate, not an extra score.
    let evidence=(0.30*z.strength+0.30*trigger.clamp(0.,1.)+0.25*if confirming {momentum.strength.max(0.35)}else{0.}+0.15*activity).clamp(0.,1.);
    let mut gates=Vec::new();
    if !confirming {gates.push("الزخم لم يؤكد اتجاه النموذج".into());}
    if evidence<0.58 {gates.push("قوة الخصائص المجمعة أقل من حد الترشيح".into());}
    if regime.volatility_brain.shock {gates.push("شمعة صدمة تذبذب؛ انتظار هدوء الحركة".into());}
    if trend.frames.iter().filter(|f|f.timeframe!="M1").all(|f|f.direction== -side) {
        gates.push("اتجاه M5 وM15 يعاكس الفرصة".into());
    }
    let obstacle=out.zones.iter().filter(|p|!p.broken&&p.side== -side&&p.confirmed_at<=c.time)
        .map(|p|if side==1 {p.low}else{p.high})
        .filter(|p|(*p-c.close)*side as f64>0.)
        .min_by(|a,b|((a-c.close).abs()).total_cmp(&(b-c.close).abs()));
    out.setups.push(Setup {id:format!("{}:{}:{pattern}:{}:{side}",out.symbol,c.time,z.id),pattern:pattern.into(),name:name.into(),side,
        signal_bar:c.time,expires_ms:(c.time+120)*1000,zone_id:z.id.clone(),zone_low:z.low,zone_high:z.high,
        reference_close:c.close,atr:a,structural_stop:stop,obstacle,evidence,
        reasons:vec![reason.into(),format!("{} {} مؤكدة عند UTC epoch {}",z.timeframe,z.kind,z.confirmed_at),
            format!("موقع {:.2} / تأكيد {:.2} / زخم {:.2} / نشاط {:.2}",z.strength,trigger,momentum.strength,activity)],
        signal_gates:gates.clone(),gates,state:"WAIT_QUOTE".into(),entry:None,stop:None,targets:vec![],lots:None,estimated_risk:None,net_rr:None});
}

pub(super) fn discover(out:&mut MarketBrains,m:&MarketData,trend:&TrendBrain,regime:&RegimeBrain) {
    let n=m.m1.len();let c=&m.m1[n-1];let prev=&m.m1[n-2];let a=atr(&m.m1);if a<=0. {return;}
    let range=(c.high-c.low).max(f64::EPSILON);let body=(c.close-c.open).abs();
    let lower=c.open.min(c.close)-c.low;let upper=c.high-c.open.max(c.close);
    let zs=out.zones.clone();
    for z in &zs {
        if z.confirmed_at>c.time || (c.close-(z.low+z.high)*0.5).abs()>3.*a {continue;}
        let side=z.side;
        let hit=c.low<=z.high+0.1*a&&c.high>=z.low-0.1*a;
        let reclaim=if side==1 {c.close>z.high&&c.close>c.open}else{c.close<z.low&&c.close<c.open};
        let wick=if side==1 {lower}else{upper};
        let close_location=if side==1 {(c.close-c.low)/range}else{(c.high-c.close)/range};
        let engulf=if side==1 {prev.close<prev.open&&c.close>prev.open&&c.open<=prev.close+0.1*a}
            else{prev.close>prev.open&&c.close<prev.open&&c.open>=prev.close-0.1*a};
        let stop=if side==1 {z.low.min(c.low)-0.15*a}else{z.high.max(c.high)+0.15*a};
        if !z.broken&&hit&&reclaim&&close_location>=0.65 {
            if wick>=0.25*range&&wick>=0.8*body {
                add(out,m,trend,regime,z,side,"zone_rejection","ارتداد من منطقة",0.60+0.4*close_location,stop,"لامست الشمعة المنطقة ثم أغلقت خارجها مع ذيل رفض واضح");
            }
            let swept=if side==1 {c.low<z.low-0.05*a}else{c.high>z.high+0.05*a};
            if swept {
                add(out,m,trend,regime,z,side,"liquidity_sweep","كسر كاذب واستعادة مستوى",0.85,stop,"تجاوز السعر مستوى سابقاً ثم استرده بإغلاق مكتمل؛ نمط سعري محتمل لسحب السيولة");
            }
            if engulf || out.momentum[0].divergence==side {
                add(out,m,trend,regime,z,side,"reversal","انعكاس مؤكد عند منطقة",0.80,stop,if engulf {"ابتلاع جسم الشمعة المعاكسة عند منطقة سابقة"}else{"تباعد RSI عند قمتين أو قاعين مؤكدين مع استعادة المنطقة"});
            }
        }
        let break_side= -side;
        let level=if break_side==1 {z.high}else{z.low};
        let crossed=(c.close-level)*break_side as f64>0.15*a && (prev.close-level)*break_side as f64<=0.;
        let new_break=z.broken_at.is_none_or(|at|at==c.time+60);
        if crossed&&new_break&&body>=0.55*range&&body>=0.5*a&&out.tick_activity_ratio.is_some_and(|v|v>=1.2) {
            let stop=if break_side==1 {z.low.min(c.low)-0.15*a}else{z.high.max(c.high)+0.15*a};
            add(out,m,trend,regime,z,break_side,"breakout","اختراق بإغلاق ونشاط",0.85,stop,"إغلاق خارج مستوى مؤكد بجسم واسع ونشاط ticks أعلى من متوسطه السابق");
        }
        let recent_break=z.broken_at.is_some_and(|at|at<=c.time&&c.time-at<=10*60);
        let retest=if break_side==1 {c.low<=z.high+0.1*a&&c.close>z.high+0.1*a&&c.close>c.open}
            else{c.high>=z.low-0.1*a&&c.close<z.low-0.1*a&&c.close<c.open};
        if recent_break&&retest&&body>=0.3*range {
            let stop=if break_side==1 {z.low.min(c.low)-0.15*a}else{z.high.max(c.high)+0.15*a};
            add(out,m,trend,regime,z,break_side,"breakout_retest","اختراق ثم إعادة اختبار",0.9,stop,"كسر سابق مؤكد ثم إعادة زيارة المستوى والإغلاق في جهة الاختراق");
        }
    }
    // Range mean reversion needs BOTH a value-area reclaim and an actual nearby
    // support/demand or resistance/supply zone; profile alone cannot trigger.
    if regime.label.starts_with("RANGE")&&out.profile.available {
        for side in [1,-1] {
            let level=if side==1 {out.profile.value_low}else{out.profile.value_high};
            let reclaimed=if side==1 {c.low<level&&c.close>level&&c.close>c.open}else{c.high>level&&c.close<level&&c.close<c.open};
            if let Some(z)=zs.iter().filter(|z|!z.broken&&z.side==side&&z.confirmed_at<=c.time)
                .find(|z|((z.low+z.high)*0.5-level).abs()<0.5*a) {
                if reclaimed {
                    let stop=if side==1 {z.low.min(c.low)-0.15*a}else{z.high.max(c.high)+0.15*a};
                    add(out,m,trend,regime,z,side,"value_reclaim","عودة من طرف النطاق",0.8,stop,"سوق عرضي واستعادة طرف القيمة التقريبية عند منطقة سعرية مستقلة");
                }
            }
        }
    }
    out.setups.sort_by(|a,b|b.evidence.total_cmp(&a.evidence).then(a.id.cmp(&b.id)));
    // A single location should not look like four separate discoveries.
    let mut unique:Vec<Setup>=vec![];
    for s in out.setups.drain(..) {
        if !unique.iter().any(|old|old.side==s.side&&(old.zone_low-s.zone_low).abs()<0.25*a) {unique.push(s);}
        if unique.len()==6 {break;}
    }
    out.setups=unique;
    if let Some(top)=out.setups.first() {
        if out.setups.iter().any(|s|s.side== -top.side&&(s.evidence-top.evidence).abs()<0.10) {
            for s in &mut out.setups {s.signal_gates.push("نماذج متعاكسة متقاربة القوة؛ انتظار حسم".into());}
        }
    }
    let active=out.zones.iter().filter(|z|!z.broken).count();
    let near=out.zones.iter().filter(|z|!z.broken).min_by(|a,b|((a.low+a.high)*0.5-c.close).abs().total_cmp(&((b.low+b.high)*0.5-c.close).abs()));
    out.specialists.push(Vote {name:"عقل المناطق والقمم والقيعان".into(),direction:near.map(|z|z.side).unwrap_or(0),
        evidence:near.map(|z|z.strength).unwrap_or(0.),reason:format!("{active} منطقة فعالة من M1/M5/M15؛ القمم والقيعان تتأكد بعد شمعتين، وتُلغى المنطقة عند الإغلاق خلف حدها")});
    let mom=&out.momentum[0];
    out.specialists.push(Vote {name:"عقل الزخم والتسارع".into(),direction:mom.direction,evidence:mom.strength,
        reason:format!("زخم 3 و8 شموع بوحدة ATR مع RSI14؛ تباعد مؤكد {}، والتفاصيل تعرض كل فريم",mom.divergence)});
    out.specialists.push(Vote {name:"عقل النشاط الحجمي".into(),direction:if out.tick_activity_ratio.is_some_and(|r|r>=1.2) {sign(c.close-c.open,0.)}else{0},
        evidence:out.tick_activity_ratio.map(|r|((r-0.6)/1.4).clamp(0.,1.)).unwrap_or(0.),
        reason:out.tick_activity_ratio.map(|r|format!("نشاط الشمعة {r:.2} من متوسط 20 شمعة سابقة؛ حجم ticks وليس شراء/بيع منفذاً")).unwrap_or("حجم ticks غير متاح؛ لا تأكيد حجمي".into())});
    for (patterns,name,description) in [
        (vec!["liquidity_sweep"],"عقل السيولة السعرية","تجاوز مستوى سابق ثم استعادته؛ لا ادعاء رؤية أوامر وقف مخفية"),
        (vec!["zone_rejection","reversal"],"عقل الارتداد والانعكاس","رفض منطقة أو ابتلاع/تباعد مؤكد في موقع سعري مناسب"),
        (vec!["breakout","breakout_retest"],"عقل الاختراق وإعادة الاختبار","إغلاق ونشاط عند الكسر، أو إعادة اختبار مستوى مكسور حديثاً"),
        (vec!["value_reclaim"],"عقل العودة إلى القيمة","مرشح خاص بالسوق العرضي يحتاج منطقة سعرية واستعادة طرف نطاق القيمة")
    ] {
        let best=out.setups.iter().find(|s|patterns.contains(&s.pattern.as_str()));
        out.specialists.push(Vote {name:name.into(),direction:best.map(|s|s.side).unwrap_or(0),evidence:best.map(|s|s.evidence).unwrap_or(0.),
            reason:format!("{description}. {}",if best.is_some(){"نموذج مرصود؛ راجع شروط الدخول"}else{"لا نموذج مكتمل بهذه الشمعة"})});
    }
    let vwap=out.session.vwap_proxy;
    out.specialists.push(Vote {name:"عقل سياق الجلسة".into(),direction:vwap.map(|v|sign(c.close-v,0.2*a)).unwrap_or(0),evidence:0.,
        reason:format!("نطاق يوم UTC المتاح ومرجع سعر موزون بنشاط ticks {}. التغطية {}؛ سياق فقط وليس تصويتاً إضافياً",vwap.map(|v|format!("{v:.5}")).unwrap_or("غير متاح".into()),if out.session.covers_day_open {"من افتتاح اليوم"}else{"جزئية من الشموع المتاحة"})});
}
