import {dashboardView} from './dashboard-evidence';
import type {DashboardRead} from './dashboard-evidence';
import type {Status} from './types';

const fmt=(v:number|null)=>v===null?'—':v.toLocaleString('en-US',{maximumFractionDigits:2});
type Props={status:Status|null;read:DashboardRead|null;scope:string;now:number;preview:boolean};
export function DashboardTradeFeed({status,read,scope,now,preview,onJournal}:Props&{onJournal:()=>void}){
 const view=preview?null:dashboardView(status,read,scope,now);
 return <section className="panel trades-panel" aria-label="Verified broker trade feed">
   <div className="panel-title"><b>أحدث صفقات الوسيط</b><button type="button" onClick={onJournal}>عرض السجل ›</button></div>
   <div className="preview-list">
    {preview?<><div>17:24:11　XAUUSD　Buy　0.50</div><div>17:16:30　GBPUSD　Sell　1.00</div><div className="preview-tag">DEMONSTRATION ONLY — NOT BROKER DATA</div></>:
     view?.ready&&view.trades.length?
      view.trades.map(x=><div key={x.positionId} className="broker-feed-row">
        <span>#{x.positionId} · {x.side==='BUY'?'شراء':'بيع'} · {fmt(x.lots)} lot</span>
        <strong>{x.net===null?'صافي غير موثّق':fmt(x.net)+' '+(status?.telemetry?.currency||'')}</strong>
        <small>{x.status==='CLOSED'?'إغلاق مثبت من سجل MT5':x.status==='OPEN'?'مفتوحة':x.status==='PARTIAL / OPEN'?'إغلاق جزئي':'بانتظار تسوية الوسيط'}</small>
       </div>):
      <div className="table-empty">{view?.ready?'لا توجد صفقات في سجل MT5 المربوط':view?.reason||'بانتظار بيانات Core'}</div>}
   </div>
  </section>;
}
export function DashboardExecutionQuality({status,read,scope,now,preview,onExecution}:Props&{onExecution:()=>void}){
 const view=preview?null:dashboardView(status,read,scope,now);
 const q=view?.quality;
 const items:[string,string][]=[
  ['MT5 Send P95',preview?'5 ms':q?fmt(q.sendP95)+' ms':'—'],
  ['Queue → Send P95',preview?'14 ms':q?fmt(q.queueP95)+' ms':'—'],
  ['طلبات مرفوضة',preview?'2':q?fmt(q.rejects):'—'],
  ['طلبات غير محسومة',preview?'1':q?fmt(q.unresolved):'—']
 ];
 return <section className="panel execution-quality" aria-label="Verified Core execution quality">
  <div className="panel-title"><b>جودة تنفيذ أوامر MT5</b><button type="button" onClick={onExecution}>تفاصيل الجودة ›</button></div>
  <div className="quality-list">{items.map(([label,value])=><div className="quality-row" key={label}>
    <span>{label}</span><div className="quality-meter"><i style={{width:'0%'}}/></div><small>{value}</small>
  </div>)}</div>
  <p className="safe-note">{preview?'DEMONSTRATION ONLY — SAMPLE METRICS':q?'P95 من سجل Gateway/MT5؛ ليست مدة التسوية النهائية للصفقة':view?.ready?'لا توجد عينة جودة تنفيذ موثقة حالياً':view?.reason||'بانتظار Core'}</p>
 </section>;
}
