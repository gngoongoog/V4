//! Stateful KTE fills, isolated from the stable Meta/CES execution algorithms.
use super::*;
fn round_stop(price:f64,side:i8,tick:f64)->f64 {if side==1 {(price/tick).floor()*tick}else{(price/tick).ceil()*tick}}
fn split(lots:f64,spec:&Contract)->Option<[f64;3]> {
    let units=(lots/spec.volume_step+1e-8).floor();let a=(units*0.4+1e-9).floor();let b=(units*0.3+1e-9).floor();
    let parts=[a*spec.volume_step,b*spec.volume_step,(units-a-b)*spec.volume_step];
    parts.iter().all(|p|*p+1e-9>=spec.volume_min && *p>0.0).then_some(parts)
}
struct Managed {
    side:i8,entry:f64,initial_stop:f64,stop:f64,targets:[f64;3],parts:[f64;3],lots:f64,fee_per_lot:f64,
    entry_time:u64,signal_bar:u64,tp1:bool,tp2:bool,fills:Vec<Fill>,stops:Vec<StopUpdate>,
}
impl Managed {
    fn remaining(&self)->f64 {self.parts.iter().sum()}
    fn pnl(&self,price:f64,lots:f64,spec:&Contract)->f64 {
        let change=(price-self.entry)*self.side as f64;
        change/spec.tick_size*lots*if change>=0.0 {spec.tick_profit}else{spec.tick_loss}-lots*self.fee_per_lot
    }
    fn quote(&self,bid:f64,spread:f64,slip:f64)->f64 {bid+if self.side==-1 {spread}else{0.0}-self.side as f64*slip}
    fn fill(&mut self,leg:Option<usize>,price:f64,c:&Candle,phase:&str,reason:&str,ambiguous:bool,ledger:&mut Ledger,spec:&Contract,risk:&RiskConfig) {
        let lots=if let Some(i)=leg {std::mem::take(&mut self.parts[i])}else{let n=self.remaining();self.parts=[0.0;3];n};
        let price=price-self.side as f64*risk.slippage_ticks*spec.tick_size;let net=self.pnl(price,lots,spec);ledger.balance+=net;
        self.fills.push(Fill{time:c.time,phase:phase.into(),price,lots,commission:lots*self.fee_per_lot,net,reason:reason.into(),ambiguous});
        ledger.mark(ledger.balance+self.pnl(price,self.remaining(),spec),c.time,risk,false);
    }
    fn targets_at(&mut self,price:f64,c:&Candle,phase:&str,ledger:&mut Ledger,spec:&Contract,risk:&RiskConfig) {
        for leg in 0..3 {if self.parts[leg]>0.0 && (price-self.targets[leg])*self.side as f64>=-1e-10 {
            self.fill(Some(leg),self.targets[leg],c,phase,["tp1","tp2","tp3"][leg],false,ledger,spec,risk);
            if leg==0 {self.tp1=true;}if leg==1 {self.tp2=true;}
        }}
    }
    /// Open is known before the range. Range ambiguity gives the stop priority.
    fn manage(&mut self,c:&Candle,spread:f64,gap:bool,ledger:&mut Ledger,spec:&Contract,risk:&RiskConfig) {
        let ask=if self.side==-1 {spread}else{0.0};let open=c.open+ask;
        if gap || (open-self.stop)*self.side as f64<=0.0 {
            self.fill(None,open,c,"open",if gap {"data_gap"}else{"stop_gap"},false,ledger,spec,risk);return;
        }
        self.targets_at(open,c,"open",ledger,spec,risk);if self.remaining()<=0.0 {return;}
        if c.time.saturating_sub(self.entry_time)>=if self.tp1 {90*60}else{45*60} {
            self.fill(None,open,c,"open","time_limit",false,ledger,spec,risk);return;
        }
        let adverse=if self.side==1 {c.low}else{c.high+ask};let favorable=if self.side==1 {c.high}else{c.low+ask};
        if (adverse-self.stop)*self.side as f64<=0.0 {
            let ambiguous=(0..3).any(|i|self.parts[i]>0.0 && (favorable-self.targets[i])*self.side as f64>=0.0);
            self.fill(None,self.stop,c,"range","stop",ambiguous,ledger,spec,risk);
        }else{self.targets_at(favorable,c,"range",ledger,spec,risk);}
    }
    fn update_stop(&mut self,c:&Candle,atr:f64,spec:&Contract) {
        if !self.tp1 || self.remaining()<=0.0 {return;}
        let mut next=self.entry;
        if self.tp2 {let trail=c.close-self.side as f64*atr;next=if self.side==1 {next.max(trail)}else{next.min(trail)};}
        next=round_stop(next,self.side,spec.tick_size);
        if (next-self.stop)*self.side as f64>1e-10 {
            self.stop=next;self.stops.push(StopUpdate{effective_time:c.time+60,price:next,reason:if self.tp2 {"atr_trail"}else{"breakeven"}.into()});
        }
    }
    fn finish(self)->Trade {
        let last=self.fills.last().expect("closed position has a fill");
        Trade{signal_bar:self.signal_bar,entry_time:self.entry_time,exit_bar:last.time,exit_phase:last.phase.clone(),side:self.side,entry:self.entry,
            exit:self.fills.iter().map(|f|f.price*f.lots).sum::<f64>()/self.lots,stop:self.initial_stop,target:self.targets[2],lots:self.lots,
            commission:self.fills.iter().map(|f|f.commission).sum(),net:self.fills.iter().map(|f|f.net).sum(),reason:last.reason.clone(),ambiguous:self.fills.iter().any(|f|f.ambiguous),
            fills:self.fills,stop_updates:self.stops,targets:self.targets.to_vec()}
    }
}
fn entry(history:&History,options:&Options,ledger:&mut Ledger,frame:&crate::kte::Frame,signal:&crate::kte::Signal,c:&Candle,previous:&Candle,spread:f64)->Option<Managed> {
    let spec=&history.contract;let risk=&options.risk;let side=signal.side;let slip=risk.slippage_ticks*spec.tick_size;
    let price=c.open+if side==1 {spread+slip}else{-slip};let stop=round_stop(signal.stop,side,spec.tick_size);let distance=(price-stop)*side as f64;
    if spread>frame.atr*risk.max_spread_atr {ledger.skip("سبريد مرتفع");return None;}
    if (c.open-previous.close).abs()>2.0*frame.atr {ledger.skip("قفزة سعرية");return None;}
    if (previous.close-signal.stop)*side as f64<=0.15*frame.atr || distance<=0.15*frame.atr {ledger.skip("مسافة وقف KTE غير صالحة");return None;}
    let targets=[1.0,2.0,3.0].map(|r|-round_stop(-(price+side as f64*r*distance),side,spec.tick_size));
    if price<=0.0 || stop<=0.0 || targets.iter().any(|v|*v<=0.0) {ledger.skip("أسعار الوقف أو الهدف غير صالحة");return None;}
    let budget=(ledger.balance*risk.risk_percent/100.0).min((ledger.balance-ledger.day_start*(1.0-risk.daily_loss_percent/100.0)).max(0.0)).min((ledger.balance-ledger.peak*(1.0-risk.max_drawdown_percent/100.0)).max(0.0));
    let loss=(distance+slip)/spec.tick_size*spec.tick_loss.max(spec.tick_profit)+risk.commission_per_lot;
    let Some(lots)=size_lots(budget,loss,spec.volume_min,spec.volume_step,risk.max_lots.min(spec.volume_max)) else{ledger.skip("اللوت الأدنى يتجاوز الميزانية");return None;};
    let Some(parts)=split(lots,spec) else{ledger.skip("لوت KTE لا يكفي لثلاثة إغلاقات جزئية");return None;};
    Some(Managed{side,entry:price,initial_stop:stop,stop,targets,parts,lots,fee_per_lot:risk.commission_per_lot,entry_time:c.time,signal_bar:previous.time,tp1:false,tp2:false,fills:vec![],stops:vec![StopUpdate{effective_time:c.time,price:stop,reason:"initial".into()}]})
}
pub(super) fn run(history:&History,options:&Options,cancel:&AtomicBool,progress:&AtomicUsize)->Result<Report,String> {
    let spec=&history.contract;let risk=&options.risk;let slip=risk.slippage_ticks*spec.tick_size;
    let bars:Vec<_>=history.bars.iter().map(|b|b.candle.clone()).collect();let frames=crate::kte::frames(&bars,spec.tick_size)?;
    let indexes:Vec<_>=(1..bars.len()).filter(|&i|bars[i].time>=options.from && bars[i].time+60<=options.to).collect();
    let Some(&first)=indexes.first() else{return Err("لا توجد شموع مكتملة ضمن الفترة المطلوبة".into());};let last=*indexes.last().unwrap();
    let mut ledger=Ledger{balance:options.initial_equity,peak:options.initial_equity,day:0,day_start:options.initial_equity,daily_locked:false,dd_locked:false,metrics:Metrics::default(),trades:vec![]};
    let mut position:Option<Managed>=None;let mut consumed=None;let mut last_exit=None;let mut gaps=0;let mut equity=vec![];
    for(step,&i)in indexes.iter().enumerate() {
        if cancel.load(Ordering::Relaxed){return Err("تم إلغاء الاختبار".into());}progress.store(step*100/indexes.len(),Ordering::Relaxed);
        let c=&bars[i];let previous=&bars[i-1];let gap=c.time!=previous.time+60;gaps+=usize::from(gap);
        let spread=options.spread_floor.max(history.bars[i-1].spread_points as f64*spec.point);
        let marked=ledger.balance+position.as_ref().map(|p|p.pnl(p.quote(c.open,spread,slip),p.remaining(),spec)).unwrap_or(0.0);ledger.mark(marked,c.time,risk,true);
        if position.is_some(){ledger.skip("صفقة مفتوحة عند الافتتاح");}
        else if gap {ledger.skip("فجوة قبل الشمعة");}
        else if ledger.daily_locked || ledger.dd_locked || ledger.balance<=0.0 {ledger.skip("حد خسارة أو تراجع");}
        else if last_exit.is_some_and(|exit|i-1<exit+3) {ledger.skip("تهدئة KTE بعد الإغلاق الكامل");}
        else if let Some(f)=frames[i-1].as_ref().filter(|f|f.ready) {
            if let Some(s)=f.signal.as_ref().filter(|s|Some(s.impulse)!=consumed) {
                // Eligible raw signal consumes impulse even if risk/spread vetoes entry.
                consumed=Some(s.impulse);position=entry(history,options,&mut ledger,f,s,c,previous,spread);
                if let Some(p)=&position {ledger.mark(ledger.balance+p.pnl(p.quote(c.open,spread,slip),p.remaining(),spec),c.time,risk,false);}
            }else{ledger.skip("لم تتحقق شروط KTE أو استُهلك الاندفاع");}
        }else{ledger.skip("إحماء أو بيانات ناقصة");}
        if let Some(p)=position.as_mut() {
            p.manage(c,spread,gap,&mut ledger,spec,risk);
            if p.remaining()<=0.0 {ledger.trades.push(position.take().unwrap().finish());last_exit=Some(i);}
            else if i!=last {if let Some(f)=frames[i].as_ref(){p.update_stop(c,f.atr,spec);}}
        }
        if i==last {if let Some(mut p)=position.take() {
            let price=c.close+if p.side==-1 {spread}else{0.0};p.fill(None,price,c,"final_close","end_of_test",false,&mut ledger,spec,risk);ledger.trades.push(p.finish());
        }}
        let marked=ledger.balance+position.as_ref().map(|p|p.pnl(p.quote(c.close,spread,slip),p.remaining(),spec)).unwrap_or(0.0);
        if !marked.is_finite(){return Err("نتيجة حسابية غير محدودة؛ راجع مواصفات الرمز".into());}ledger.mark(marked,c.time,risk,false);
        equity.push(EquityPoint{time:c.time+60,equity:marked,balance:ledger.balance});
    }
    finish(history,options,ledger,equity,bars[first].time,bars[last].time,indexes.len(),gaps,progress)
}
