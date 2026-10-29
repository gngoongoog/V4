import {ArrowDown,ArrowUp,BriefcaseBusiness,ChartCandlestick,Clock,RefreshCw,Settings,ShieldCheck,Target,Wallet,Zap} from 'lucide-react';
import TradingChart from './chart';
import {GoldSymbol} from './gold-symbol';
import type {ChartFrame} from './chart';
import type {ChartFrameMeta} from './chart-history';
import {money,num,signed,up,valid} from './format';
import type {Market,PageId,Status,Telemetry,Theme} from './types';

type Props={account:Telemetry|null;status:Status|null;quoteFresh:boolean;preview:boolean;busy:boolean;native:boolean;
 frame:ChartFrame;onFrameChange:(frame:ChartFrame)=>void;theme:Theme;market?:Market|null;frameMeta?:ChartFrameMeta;
 open:(page:PageId)=>void;onTradingSettings:()=>void;onStart:()=>void;onStop:()=>void};

export function ConceptMarketDesk({account,status,quoteFresh,preview,busy,native,frame,onFrameChange,theme,market,frameMeta,open,onTradingSettings,onStart,onStop}:Props){
 const bid=(preview||quoteFresh)&&valid(account?.bid)?account.bid:null;
 const ask=(preview||quoteFresh)&&valid(account?.ask)?account.ask:null;
 const spread=bid!==null&&ask!==null&&ask>=bid?ask-bid:null;
 const positions=account?.positions??[],currency=account?.currency||'USD';
 const symbol=account?.symbol||'XAUUSD';
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
    <div className="concept-manual-tabs"><button type="button" disabled title="أوامر الشراء اليدوية غير متاحة في Core الحالي"><ArrowUp size={16}/> شراء</button><button type="button" disabled title="أوامر البيع اليدوية غير متاحة في Core الحالي"><ArrowDown size={16}/> بيع</button></div>
    <div className="concept-prices"><div className="concept-sell"><strong>{num(bid,2)}</strong><small>SELL</small></div><div><strong>{num(spread,2)}</strong><small>SPREAD</small></div><div className="concept-buy"><strong>{num(ask,2)}</strong><small>BUY</small></div></div>
    <p className="concept-unavailable">أزرار الأوامر اليدوية في التصميم غير متصلة بوظيفة تنفيذ في الإصدار الحالي. الأوضاع الآلية تُضبط من إعدادات التداول أدناه.</p>
    <div className="concept-order-fields"><button type="button" onClick={onTradingSettings}>إعداد الهدف ووقف القناص ›</button><button type="button" onClick={()=>open('risk')}>إعداد المخاطر واللوت ›</button></div>
    <div className="concept-trading-actions"><button type="button" className="action-btn start" onClick={onStart} disabled={busy||preview||!native||!account||!status?.gateway_fresh||status?.requested_run===true}>▶ بدء التداول</button><button type="button" className="action-btn stop" onClick={onStop} disabled={preview||!native}>■ إيقاف الدخول</button></div>
   </section>
   <div className="concept-market-summary"><div><Wallet/> الرصيد <b>{money(account?.balance,currency)}</b></div><div><ShieldCheck/> الهامش الحر <b>{money(account?.margin_free,currency)}</b></div><div><Target/> الربح اليومي <b>{signed(account?.today_net,currency)}</b></div><div><Zap/> السبريد <b>{num(spread,2)}</b></div></div>
   <section className="panel concept-quick-orders"><div className="panel-title"><b>أوامر سريعة</b><Settings size={16}/></div><p className="concept-unavailable">لا تتوفر أوامر يدوية أو إغلاق جماعي في Core الحالي.</p><div><button disabled title="غير مدعوم حالياً">شراء بسوق</button><button disabled title="غير مدعوم حالياً">بيع بسوق</button><button disabled title="غير مدعوم حالياً">شراء معلّق</button><button disabled title="غير مدعوم حالياً">بيع معلّق</button><button disabled title="غير مدعوم حالياً">إغلاق كل الصفقات</button></div></section>
   <section className="panel concept-open-positions"><div className="panel-title"><b>المراكز المفتوحة ({account?positions.length:'—'})</b><button type="button" onClick={()=>open('positions')}><RefreshCw size={15}/> التفاصيل</button></div><div className="table-scroll"><table><thead><tr>{['#','الرمز','النوع','الحجم','وقف الخسارة','أخذ الربح','سعر الدخول','السعر الحالي','الربح / الخسارة'].map(h=><th key={h}>{h}</th>)}</tr></thead><tbody>{positions.length?positions.map(p=><tr key={p.ticket}><td>{p.ticket}</td><td>{p.symbol}</td><td className={up(p.side)?'teal':'red'}>{p.side}</td><td>{num(p.volume,2)}</td><td>{num(p.sl,2)}</td><td>{num(p.tp,2)}</td><td>{num(p.price_open,2)}</td><td>{num(p.price_current,2)}</td><td className={p.profit>=0?'teal':'red'}>{signed(p.profit,currency)}</td></tr>):<tr><td colSpan={9} className="empty-row">لا توجد مراكز مؤكدة أو الحساب غير متصل</td></tr>}</tbody></table></div></section>
  </div>
 </div>;
}
