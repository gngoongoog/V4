//! Bounded closed-bar market specialists and quote-aware research trade plans.
//! This module discovers setups; it has no order API and is not a trained model.
use crate::{research::{size_lots, TrendBrain, RegimeBrain, Vote}, Candle, MarketData, RiskConfig, Telemetry};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
mod zones;
mod features;
mod setups;
mod gann;
pub use gann::GannFrame;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Swing {
    pub timeframe: String,
    pub time: u64,
    pub confirmed_at: u64,
    pub side: i8, // +1 swing low, -1 swing high
    pub price: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub timeframe: String,
    pub kind: String,
    pub side: i8, // +1 demand/support, -1 supply/resistance
    pub low: f64,
    pub high: f64,
    pub created_at: u64,
    pub confirmed_at: u64,
    pub touches: usize,
    pub strength: f64,
    pub broken: bool,
    pub broken_at: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Momentum {
    pub timeframe: String,
    pub direction: i8,
    pub roc3_atr: f64,
    pub roc8_atr: f64,
    pub acceleration_atr: f64,
    pub rsi14: f64,
    pub strength: f64,
    pub divergence: i8,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Profile {
    pub available: bool,
    pub poc: f64,
    pub value_low: f64,
    pub value_high: f64,
    pub coverage_start: u64,
    pub bars: usize,
    pub note: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionContext {
    pub day_utc: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub vwap_proxy: Option<f64>,
    pub coverage_start: u64,
    pub covers_day_open: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Setup {
    pub id: String,
    pub pattern: String,
    pub name: String,
    pub side: i8,
    pub signal_bar: u64,
    pub expires_ms: u64,
    pub zone_id: String,
    pub zone_low: f64,
    pub zone_high: f64,
    pub reference_close: f64,
    pub atr: f64,
    pub structural_stop: f64,
    pub obstacle: Option<f64>,
    pub evidence: f64,
    pub reasons: Vec<String>,
    pub signal_gates: Vec<String>,
    pub gates: Vec<String>,
    pub state: String,
    pub entry: Option<f64>,
    pub stop: Option<f64>,
    pub targets: Vec<f64>,
    pub lots: Option<f64>,
    pub estimated_risk: Option<f64>,
    pub net_rr: Option<f64>,
}
impl Setup {
    fn clear_quote(&mut self) {
        self.entry = None; self.stop = None; self.targets.clear();
        self.lots = None; self.estimated_risk = None; self.net_rr = None;
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MarketBrains {
    pub version: String,
    pub current: bool,
    pub bar_time: u64,
    pub computed_ms: u64,
    pub quote_ms: Option<u64>,
    pub symbol: String,
    pub zones: Vec<Zone>,
    pub swings: Vec<Swing>,
    pub momentum: Vec<Momentum>,
    pub gann: Vec<GannFrame>,
    pub tick_activity_ratio: Option<f64>,
    pub profile: Profile,
    pub session: SessionContext,
    pub specialists: Vec<Vote>,
    pub setups: Vec<Setup>,
    pub note: String,
}

pub fn scan(m: &MarketData, symbol: &str, trend: &TrendBrain, regime: &RegimeBrain, now: u64) -> MarketBrains {
    // Caller additionally enforces the shared 100-bar/freshness/gap contract.
    if !m.valid() || [&m.m1,&m.m5,&m.m15].iter().any(|s|s.len()<100) {
        return MarketBrains { note:"بانتظار شموع مكتملة كافية".into(), ..Default::default() };
    }
    let mut out = MarketBrains {version:"market-specialists-v1".into(), current:true,
        bar_time:m.m1.last().unwrap().time, computed_ms:now, symbol:symbol.into(),
        note:"قراءة من الشموع المكتملة؛ المستويات مناطق مرصودة وليست أوامر مؤسسات مؤكدة. قوة الخصائص ليست احتمال ربح.".into(), ..Default::default()};
    for (tf,seconds,bars) in [("M1",60,&m.m1),("M5",300,&m.m5),("M15",900,&m.m15)] {
        let (swings,zones)=zones::map(bars,tf,seconds);
        out.momentum.push(features::momentum(bars,tf,&swings));
        out.gann.push(gann::assess(bars,tf,&swings));
        out.swings.extend(swings); out.zones.extend(zones);
    }
    out.tick_activity_ratio=features::activity(&m.m1);
    out.profile=features::profile(&m.m5);
    out.session=features::session(&m.m1);
    setups::discover(&mut out,m,trend,regime);

    // Gann geometry is observational in v1 and does not change setup scoring or Shadow.
    let directional = out.gann.iter().filter(|frame| frame.available && frame.direction != 0).collect::<Vec<_>>();
    let consensus = if directional.len() >= 2 && directional.iter().all(|frame| frame.direction == directional[0].direction) {
        directional[0].direction
    } else { 0 };
    let evidence = if consensus == 0 { 0.0 } else {
        directional.iter().filter(|frame| frame.direction == consensus).map(|frame| frame.strength).sum::<f64>()
            / directional.iter().filter(|frame| frame.direction == consensus).count().max(1) as f64
    };
    out.specialists.push(Vote {
        name:"Gann Geometry Brain / عقل جان الهندسي".into(),
        direction:consensus,
        evidence,
        reason:if consensus == 0 {
            "لا يوجد اتفاق هندسي بين فريمين على الأقل. القياس يستخدم swing مؤكد، شبكة 1/8، وميل ATR/شمعة؛ لا يعتمد زاوية شاشة.".into()
        } else {
            format!("اتفاق هندسي على {} فريمات باتجاه {}؛ شبكة 1/8 وميل سعري مطبّع بالـATR. هذا دليل مراقبة وليس إشارة مستقلة.", directional.iter().filter(|f|f.direction==consensus).count(), if consensus==1 {"الصعود"} else {"الهبوط"})
        }
    });
    out
}

impl MarketBrains {
    pub fn withdraw(&mut self, reason: &str) {
        self.current=false; self.quote_ms=None; self.note=reason.into();
        for s in &mut self.setups { s.clear_quote(); s.state="UNAVAILABLE".into(); s.gates=vec![reason.into()]; }
    }
    pub fn expire(&mut self, now:u64, quote_fresh:bool) {
        for s in &mut self.setups {
            if now > s.expires_ms || !quote_fresh {
                s.clear_quote(); s.state=if now>s.expires_ms {"EXPIRED"}else{"WAIT_QUOTE"}.into();
                s.gates=vec![if now>s.expires_ms {"انتهت صلاحية شمعة الإشارة"}else{"نحتاج سعراً حديثاً"}.into()];
            }
        }
    }
    /// Re-price ideas on the current quote; never modifies the closed-bar cache.
    pub fn price(&mut self, t:&Telemetry, risk:&RiskConfig, budget:f64, gates:&[String], now:u64) {
        let specs=match(t.bid,t.ask,t.tick_size,t.tick_value_loss,t.tick_value_profit,t.volume_min,t.volume_step,t.volume_max) {
            (Some(b),Some(a),Some(tick),Some(loss),Some(profit),Some(min),Some(step),Some(max))
            if t.connected && t.tick_age_ms.is_some_and(|v|v<=3000) && a>=b && min<=max
                && [b,a,tick,loss,profit,min,step,max].iter().all(|v|v.is_finite()&&*v>0.) => Some((b,a,tick,loss,profit,min,step,max)),
            _=>None
        };
        self.quote_ms=specs.map(|_|now);
        for s in &mut self.setups {
            s.clear_quote(); s.gates=s.signal_gates.clone(); s.gates.extend_from_slice(gates);
            s.state="BLOCKED".into();
            if now>s.expires_ms {s.state="EXPIRED".into();s.gates.push("انتهت صلاحية الإشارة؛ انتظر شمعة جديدة".into());continue;}
            let Some((bid,ask,tick,loss,profit,min,step,max))=specs else {
                s.state="WAIT_QUOTE".into();s.gates.push("سعر حديث ومواصفات tick واللوت مطلوبة".into());continue;
            };
            let slip=risk.slippage_ticks*tick;
            let entry=if s.side==1 {ask+slip}else{bid-slip};
            let stop=if s.side==1 {(s.structural_stop/tick).floor()*tick}else{(s.structural_stop/tick).ceil()*tick};
            let distance=(entry-stop)*s.side as f64;
            if ask-bid>s.atr*risk.max_spread_atr {s.gates.push("السبريد مرتفع".into());}
            if ((bid+ask)*0.5-s.reference_close).abs()>0.35*s.atr {s.gates.push("السعر ابتعد عن شمعة التأكيد؛ لا مطاردة".into());}
            if stop<=0. || entry<=0. || distance<0.25*s.atr || distance>3.0*s.atr {
                s.gates.push("مسافة الوقف البنيوي غير مناسبة للسعر الحالي".into());continue;
            }
            let room=s.obstacle.map(|level|((level-entry)*s.side as f64-0.1*s.atr)/distance).unwrap_or(3.0);
            let final_r=room.min(3.0);
            if final_r<1.8 {s.gates.push("منطقة معاكسة قريبة؛ المساحة لا تكفي للعائد المطلوب".into());continue;}
            let targets=[1.0, (final_r+1.0)*0.5,final_r].map(|r| {
                let value=entry+s.side as f64*r*distance;
                if s.side==1 {(value/tick).floor()*tick}else{(value/tick).ceil()*tick}
            });
            let loss_per_lot=(distance+slip)/tick*loss.max(profit)+risk.commission_per_lot;
            let gain_per_lot=((targets[2]-entry)*s.side as f64-slip)/tick*profit-risk.commission_per_lot;
            let rr=gain_per_lot/loss_per_lot;
            if targets.iter().any(|p|!p.is_finite()||*p<=0.) || !rr.is_finite() || rr<1.5 {
                s.gates.push("العائد الصافي بعد التكاليف أقل من 1.5 مقابل المخاطرة".into());continue;
            }
            s.entry=Some(entry); s.stop=Some(stop); s.targets=targets.to_vec();s.net_rr=Some(rr);
            if let Some(lots)=size_lots(budget,loss_per_lot,min,step,max.min(risk.max_lots)) {
                s.lots=Some(lots);s.estimated_risk=Some(lots*loss_per_lot);
            } else {s.gates.push("اللوت الأدنى أكبر من ميزانية المخاطرة المتاحة".into());}
            if s.gates.is_empty() {s.state="READY".into();}
        }
        // At most one READY recommendation. Other patterns remain reviewable.
        let mut chosen=false;
        for s in &mut self.setups {
            if s.state=="READY" {
                if chosen {s.state="ALTERNATIVE".into();} else {chosen=true;}
            }
        }
    }
    pub fn headline(&self)->String {
        if !self.current {return "بيانات التحليل غير متاحة".into();}
        match self.setups.iter().find(|s|s.state=="READY") {
            Some(s)=>format!("{} — {}",if s.side==1 {"فرصة شراء"}else{"فرصة بيع"},s.name),
            None=>format!("{} نماذج مرصودة — انتظار التأكيد أو شروط الدخول",self.setups.len())
        }
    }
    pub fn signals_text(&self)->String {
        let mut out=format!("{}\n{}\n{}\n",self.symbol,self.headline(),self.note);
        if self.setups.is_empty() {out.push_str("لم تظهر إشارة مكتملة الشروط في آخر شمعة؛ لا تُولّد صفقة لمجرد وجود مستوى قريب.\n");}
        for s in self.setups.iter().take(6) {
            let state=match s.state.as_str(){"READY"=>"فرصة جاهزة للمراجعة","ALTERNATIVE"=>"بديل أقل ترتيباً","EXPIRED"=>"منتهية","WAIT_QUOTE"=>"انتظار سعر","UNAVAILABLE"=>"بيانات غير متاحة",_=>"مرصودة وشروط الدخول غير مكتملة"};
            let _=writeln!(out,"\n{} | {} | {}\nمنطقة {:.5} .. {:.5} | قوة الخصائص {:.2}\n{}",s.name,if s.side==1 {"BUY / شراء"}else{"SELL / بيع"},state,s.zone_low,s.zone_high,s.evidence,s.reasons.join(" • "));
            if let (Some(entry),Some(stop))=(s.entry,s.stop) {
                let _=writeln!(out,"دخول {:.5} | وقف {:.5}\nأهداف: {}\nلوت {} | خسارة مقدرة شاملة التكاليف {} | عائد/مخاطرة صافي {}",entry,stop,s.targets.iter().map(|v|format!("{v:.5}")).collect::<Vec<_>>().join(" / "),s.lots.map(|v|format!("{v:.4}")).unwrap_or("غير متاح".into()),s.estimated_risk.map(|v|format!("{v:.2}")).unwrap_or("غير متاحة".into()),s.net_rr.map(|v|format!("{v:.2}")).unwrap_or("غير متاح".into()));
            } else {let _=writeln!(out,"حد الإبطال البنيوي {:.5}؛ أرقام الدخول تنتظر سعراً وشروطاً صالحين",s.structural_stop);}
            if !s.gates.is_empty() {let _=writeln!(out,"أسباب الانتظار: {}",s.gates.join(" • "));}
            let _=writeln!(out,"شمعة UTC epoch {} | صالح حتى {} | {}",s.signal_bar,s.expires_ms/1000,s.id);
        }
        out.push_str("\nخطط تحليلية من بيانات MT5؛ لا إرسال أوامر. تُحدّث مع السعر وقد تنتهي صلاحيتها.\n");out
    }
    pub fn map_text(&self)->String {
        let mut out=String::new();
        for v in &self.specialists {let _=writeln!(out,"{} | اتجاه {} | قوة {:.2}\n{}\n",v.name,v.direction,v.evidence,v.reason);}
        for m in &self.momentum {let _=writeln!(out,"{}: RSI {:.1} | زخم 3 {:.2} ATR / 8 {:.2} ATR | تسارع {:.2} | تباعد {}",m.timeframe,m.rsi14,m.roc3_atr,m.roc8_atr,m.acceleration_atr,m.divergence);}
        let _=writeln!(out,"\nGann Geometry / هندسة جان:");
        for frame in &self.gann {
            if frame.available {
                let _=writeln!(out,"{} | اتجاه {} | قوة {:.2} | anchor {:.5} @ {} | speed {:.3} ATR/bar | 1/8 level {}/8 = {:.5} | distance {:.2} ATR",frame.timeframe,frame.direction,frame.strength,frame.anchor_price,frame.anchor_time,frame.atr_per_bar,frame.nearest_eighth,frame.nearest_level,frame.distance_atr);
            } else {
                let _=writeln!(out,"{} | بيانات swing/range غير كافية لهندسة جان",frame.timeframe);
            }
        }
        let _=writeln!(out,"\nمناطق فعالة (الأحدث بكل فريم):");
        for z in self.zones.iter().filter(|z|!z.broken).take(36) {let _=writeln!(out,"{} {} | {:.5} .. {:.5} | زيارات {} | قوة {:.2}",z.timeframe,z.kind,z.low,z.high,z.touches,z.strength);}
        let _=writeln!(out,"\nملف النشاط الحجمي: {}",self.profile.note);
        if self.profile.available {let _=writeln!(out,"POC {:.5} | نطاق قيمة تقريبي {:.5} .. {:.5}",self.profile.poc,self.profile.value_low,self.profile.value_high);}
        let _=writeln!(out,"\nنطاق يوم UTC المرصود {:.5} .. {:.5} | بداية التغطية {} | يشمل افتتاح اليوم: {}",self.session.low,self.session.high,self.session.coverage_start,self.session.covers_day_open);
        out
    }
}

fn atr(s:&[Candle])->f64 {let n=s.len();s[n-15..].windows(2).map(|w|(w[1].high-w[1].low).max((w[1].high-w[0].close).abs()).max((w[1].low-w[0].close).abs())).sum::<f64>()/14.0}
fn sign(v:f64,threshold:f64)->i8 {if v>threshold {1}else if v< -threshold {-1}else{0}}
