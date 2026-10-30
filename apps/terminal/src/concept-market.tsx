import {ArrowDown,ArrowUp,BriefcaseBusiness,ChartCandlestick,Clock,RefreshCw,Settings,ShieldCheck,Target,Wallet,Zap} from 'lucide-react';
import {useState} from 'react';
import TradingChart from './chart';
import {GoldSymbol} from './gold-symbol';
import type {ChartFrame} from './chart';
import type {ChartFrameMeta} from './chart-history';
import {money,num,signed,up,valid} from './format';
import type {Market,PageId,Position,Status,Telemetry,Theme} from './types';

// Only identities issued by this Core are eligible. Account-wide or other
// symbols' positions are intentionally not included in this action.
export function ownedCloseTickets(positions:Position[],symbol:string,profitOnly:boolean):number[]{
 const matches=positions.filter(p=>Number.isSafeInteger(p.ticket)&&p.ticket>0&&p.symbol===symbol&&
  Number.isSafeInteger(p.magic)&&((p.magic!>=410600&&p.magic!<=410615)||p.magic===410617||p.magic===410618)&&
  (p.side==='BUY'||p.side==='SELL')&&Number.isFinite(p.volume)&&p.volume>0&&
  (!profitOnly||(Number.isFinite(p.profit)&&p.profit>0)));
 const tickets=matches.map(p=>p.ticket);
 return new Set(tickets).size===tickets.length?tickets:[];
}

export type ManualInput={side:1|-1;lots:number;stop:number;target:number;reference:number};
export function manualDraft(side:1|-1,lotsText:string,stopText:string,targetText:string,bid:number|null,ask:number|null):ManualInput|null{
 if(!lotsText.trim()||!stopText.trim()||!targetText.trim()||bid===null||ask===null)return null;
 const lots=Number(lotsText),stop=Number(stopText),target=Number(targetText),reference=side===1?ask:bid,exit=side===1?bid:ask;
 if(![lots,stop,target,bid,ask].every(v=>Number.isFinite(v)&&v>0)||ask<bid
    ||(exit-stop)*side<=0||(target-exit)*side<=0)return null;
 return {side,lots,stop,target,reference};
}
function PendingOrders({account,preview}:{account:Telemetry|null;preview:boolean}){
 const orders=!preview&&Array.isArray(account?.pending_order_details)?account.pending_order_details:null;
 const kinds=['BUY','SELL','BUY LIMIT','SELL LIMIT','BUY STOP','SELL STOP','BUY STOP LIMIT','SELL STOP LIMIT','CLOSE BY'];
 return <section className="panel concept-pending-orders"><div className="panel-title"><b>أوامر الحساب النشطة ({orders?.length??(!preview?account?.pending_orders:undefined)??'—'})</b><small>بيانات MT5 لكل الرموز</small></div><div className="table-scroll"><table><thead><tr>{['#','الرمز','النوع','الحجم المتبقي','سعر الأمر','وقف الخسارة','أخذ الربح','Magic'].map(h=><th key={h}>{h}</th>)}</tr></thead><tbody>{orders?.length?orders.map(o=><tr key={o.ticket}><td>{o.ticket}</td><td>{o.symbol}</td><td>{kinds[o.order_type]||String(o.order_type)}</td><td>{num(o.volume_current,2)}</td><td>{num(o.price_open,2)}</td><td>{o.sl?num(o.sl,2):'—'}</td><td>{o.tp?num(o.tp,2):'—'}</td><td>{o.magic}</td></tr>):<tr><td colSpan={8} className="empty-row">{orders?'لا توجد أوامر نشطة حسب MT5':'تفاصيل أوامر MT5 غير متاحة أو غير مكتملة'}</td></tr>}</tbody></table></div><p className="concept-unavailable">هذه قائمة للعرض فقط؛ إلغاء الأوامر المعلقة وإدارتها لم يكتمل بعد.</p></section>;
}
type Props={account:Telemetry|null;status:Status|null;accountMatches:boolean;quoteFresh:boolean;preview:boolean;busy:boolean;native:boolean;
 frame:ChartFrame;onFrameChange:(frame:ChartFrame)=>void;theme:Theme;market?:Market|null;frameMeta?:ChartFrameMeta;
 open:(page:PageId)=>void;onTradingSettings:()=>void;onStart:()=>void;onStop:()=>void;onClose:(profitOnly:boolean)=>void;onCloseTicket:(ticket:number)=>void;onManual:(input:ManualInput)=>void};

export function ConceptMarketDesk({account,status,accountMatches,quoteFresh,preview,busy,native,frame,onFrameChange,theme,market,frameMeta,open,onTradingSettings,onStart,onStop,onClose,onCloseTicket,onManual}:Props){
 const [direction,setDirection]=useState<1|-1>(1);const [lots,setLots]=useState('0.01');
 const [stop,setStop]=useState('');const [target,setTarget]=useState('');const [manualError,setManualError]=useState('');
 const bid=(preview||quoteFresh)&&valid(account?.bid)?account.bid:null;
 const ask=(preview||quoteFresh)&&valid(account?.ask)?account.ask:null;
 const spread=bid!==null&&ask!==null&&ask>=bid?ask-bid:null;
 const positions=account?.positions??[],currency=account?.currency||'USD';
 const symbol=account?.symbol||'XAUUSD';
 const closeReady=!!account&&!!status&&status.requested_run===false&&status.gateway_fresh&&quoteFresh&&!preview&&!busy&&native;
 const closable=account?ownedCloseTickets(positions,symbol,false):[];
 const profitable=account?ownedCloseTickets(positions,symbol,true):[];
 const manualReady=closeReady&&accountMatches&&account?.trade_allowed===true&&account?.trade_expert===true;
 const submitManual=(side:1|-1)=>{
  const request=manualDraft(side,lots,stop,target,bid,ask);
  if(!request){setManualError('أدخل لوتاً موجباً وستوباً وهدفاً بالأسعار الصحيحة للاتجاه المختار');return}
  setManualError('');onManual(request);
 };
 return <div className="concept-market">
  <div className="concept-market-stats">
   <div><GoldSymbol className="market-symbol-icon"/><strong>{symbol}</strong><small>{symbol.toUpperCase().startsWith('XAU')?'Gold Spot / U.S. Dollar':'Trading symbol'}</small></div>
   <div><Clock/><span>سعر البيع الحالي<strong>{num(bid,2)}</strong><small>Bid من MT5</small></span></div>
   <div><ArrowUp/><span>سعر الشراء الحالي<strong>{num(ask,2)}</strong><small>Ask من MT5</small></span></div>
   <div><ChartCandlestick/><span>السبريد الحالي<strong>{num(spread,2)}</strong><small>فرق السعر</small></span></div>
   <div><BriefcaseBusiness/><span>المراكز المفتوحة<strong>{account?positions.length:'—'}</strong><small>من الحساب المتصل</small></span></div>
   <div><span className={'concept-led '+(quoteFresh&&!preview?'on':'')}/><span>حالة السوق<strong>{quoteFresh?'سعر حديث':'غير مؤكدة'}</strong><small>{quoteFresh?'تيك MT5 حديث':'بانتظار سعر حديث'}</small></span></div>
  </div>
  <div className="concept-market-layout">
   <TradingChart market={market??account?.market} symbol={symbol} quote={bid??undefined} frame={frame} onFrameChange={onFrameChange} theme={theme} frameMeta={frameMeta} preview={preview}/>
   <section className="panel concept-order-panel">
    <div className="panel-title"><b>{symbol} - التداول المباشر</b><button type="button" title="فتح إعدادات التداول" onClick={onTradingSettings}><Settings size={16}/></button></div>
    <div className="concept-manual-tabs"><button type="button" aria-pressed={direction===1} onClick={()=>setDirection(1)}><ArrowUp size={16}/> شراء</button><button type="button" aria-pressed={direction===-1} onClick={()=>setDirection(-1)}><ArrowDown size={16}/> بيع</button></div>
    <div className="concept-prices"><div className="concept-sell"><strong>{num(bid,2)}</strong><small>SELL</small></div><div><strong>{num(spread,2)}</strong><small>SPREAD</small></div><div className="concept-buy"><strong>{num(ask,2)}</strong><small>BUY</small></div></div>
    <div className="operator-grid"><label>حجم العقد (لوت)<input aria-label="لوت الأمر اليدوي" type="number" min="0.001" step="any" value={lots} onChange={e=>setLots(e.target.value)}/></label><label>وقف الخسارة (السعر)<input aria-label="وقف الأمر اليدوي" type="number" step="any" value={stop} onChange={e=>setStop(e.target.value)}/></label><label>أخذ الربح (السعر)<input aria-label="هدف الأمر اليدوي" type="number" step="any" value={target} onChange={e=>setTarget(e.target.value)}/></label></div>
    {manualError&&<p className="operator-error" role="alert">{manualError}</p>}
    <button type="button" disabled={!manualReady} onClick={()=>submitManual(direction)}>تنفيذ {direction===1?'شراء':'بيع'} بسعر السوق</button>
    <p className="concept-unavailable">الطلب يحتاج STOP وحساباً مرتبطاً وسعراً حديثاً. أدخل SL وTP كأرقام أسعار؛ يُراجع Core والوسيط اللوت والانزلاق والمخاطر، ولا يؤكد الطابور حصول التنفيذ.</p>
    <div className="concept-order-fields"><button type="button" onClick={onTradingSettings}>إعداد الهدف ووقف القناص ›</button><button type="button" onClick={()=>open('risk')}>إعداد المخاطر واللوت ›</button></div>
    <div className="concept-trading-actions"><button type="button" className="action-btn start" onClick={onStart} disabled={busy||preview||!native||!account||!status?.gateway_fresh||status?.requested_run===true}>▶ بدء التداول</button><button type="button" className="action-btn stop" onClick={onStop} disabled={preview||!native}>■ إيقاف الدخول</button></div>
   </section>
   <div className="concept-market-summary"><div><Wallet/> الرصيد <b>{money(account?.balance,currency)}</b></div><div><ShieldCheck/> الهامش الحر <b>{money(account?.margin_free,currency)}</b></div><div><Target/> الربح اليومي <b>{signed(account?.today_net,currency)}</b></div><div><Zap/> السبريد <b>{num(spread,2)}</b></div></div>
   <section className="panel concept-quick-orders"><div className="panel-title"><b>أوامر سريعة</b><Settings size={16}/></div><p className="concept-unavailable">الإغلاق يشمل مراكز البرنامج للرمز الحالي فقط، حتى 20 في الطلب، بعد STOP. الرابحة تعني الربح العائم الموجب من MT5 قبل حساب الرسوم.</p><div><button type="button" disabled={!manualReady} onClick={()=>submitManual(1)}>شراء بسوق</button><button type="button" disabled={!manualReady} onClick={()=>submitManual(-1)}>بيع بسوق</button><button disabled title="غير مدعوم حالياً">شراء معلّق</button><button disabled title="غير مدعوم حالياً">بيع معلّق</button><button type="button" disabled={!closeReady||!closable.length} onClick={()=>onClose(false)}>إغلاق مراكز البرنامج للرمز</button><button type="button" disabled={!closeReady||!profitable.length} onClick={()=>onClose(true)}>إغلاق الرابحة للرمز</button></div></section>
   <section className="panel concept-open-positions"><div className="panel-title"><b>المراكز المفتوحة ({account?positions.length:'—'})</b><button type="button" onClick={()=>open('positions')}><RefreshCw size={15}/> التفاصيل</button></div><div className="table-scroll"><table><thead><tr>{['#','الرمز','النوع','الحجم','وقف الخسارة','أخذ الربح','سعر الدخول','السعر الحالي','الربح / الخسارة','الإجراء'].map(h=><th key={h}>{h}</th>)}</tr></thead><tbody>{positions.length?positions.map(p=><tr key={p.ticket}><td>{p.ticket}</td><td>{p.symbol}</td><td className={up(p.side)?'teal':'red'}>{p.side}</td><td>{num(p.volume,2)}</td><td>{num(p.sl,2)}</td><td>{num(p.tp,2)}</td><td>{num(p.price_open,2)}</td><td>{num(p.price_current,2)}</td><td className={p.profit>=0?'teal':'red'}>{signed(p.profit,currency)}</td><td><button type="button" className="row-action" aria-label={`إغلاق المركز ${p.ticket}`} title="طلب إغلاق هذا المركز بعد التأكيد" disabled={!closeReady||!closable.includes(p.ticket)} onClick={()=>onCloseTicket(p.ticket)}>إغلاق</button></td></tr>):<tr><td colSpan={10} className="empty-row">لا توجد مراكز مؤكدة أو الحساب غير متصل</td></tr>}</tbody></table></div></section>
   <PendingOrders account={account} preview={preview}/>
  </div>
 </div>;
}
