import {useEffect,useMemo,useRef,useState} from 'react';
import {Activity,BarChart3,BookOpen,BrainCircuit,CalendarClock,ChartCandlestick,CheckCircle2,CircleHelp,Database,FileText,History,Home,Layers3,MessageCircle,PauseCircle,Play,Radio,RefreshCw,Settings,ShieldCheck,Siren,Target,Workflow,Zap,Wifi,WifiOff,SlidersHorizontal,Server,RotateCcw,LineChart,Clock4,BriefcaseBusiness,NotebookTabs,Eye,Sun,Cloud, Moon} from 'lucide-react';
import TradingChart from './chart';
import {PanelReadFence} from './panel-guard';
import SystemHealth,{buildHealth} from './system-health';
import EnterpriseSupervisor from './enterprise-supervisor';
import DataExplorer from './data-explorer';
import {BacktestOperations,BackupOperations,TradingSettingsOperations,BrainControlsOperations,ExecutionBindingOperations} from './operations';
import {feedEvidence} from './feed';
import {diagnoseMarketQuote} from './market-quote-diagnosis';
import {approvalIsCurrent,canConfirmStart} from './operator-approval';
import type {OperatorApproval} from './operator-approval';
import {brokerEvidence} from './brain-evidence';
import {ScopedReadFence} from './scoped-read-fence';
import {DashboardTradeFeed,DashboardExecutionQuality} from './dashboard-operations';
import {TradeJournalOperations} from './trade-journal-operations';
import type {DashboardRead} from './dashboard-evidence';
import {OperatorWriteFence} from './operator-write-fence';
import type {OperatorWriteTicket} from './operator-write-fence';
import {MicroCapitalOperations} from './micro-capital-operations';
import {ExecutionQualityOperations} from './execution-quality-operations';
import {ModeEvidenceOperations} from './mode-evidence-operations';
import {FrozenCoordinatorOperations,supportedFrozenMode} from './frozen-coordinator-operations';
import {QuoteClockEvidence} from './quote-clock-evidence';
import {SniperTickEvidence} from './sniper-tick-evidence';
import {SniperFrozenOperations,eligibleFrozenSniper} from './sniper-frozen-operations';
import {SniperClassicOperations,eligibleFrozenClassic} from './sniper-classic-operations';
import {validatePortfolio} from './micro-capital';
import type {PageId,Status,Theme,Position} from './types';
import {DESIGN_FIXTURE} from './mock';
import {hasTauri,subscribe,coreStatus,control,readPanel,saveRisk,savePortfolio,saveSettings,handoff,shutdownCore,closeTerminal,backupAction,backtestAction,saveTrading,saveSupervisor,updateBrainControl,bindExecution,frozenCoordinatorAction,frozenSniperAction,frozenClassicAction} from './bridge';
import {money,num,signed,up,valid} from './format';

type IconComponent=typeof Home;
const PAGES:{id:PageId;title:string;en:string;icon:IconComponent;route?:string}[]=[
 {id:'dashboard',title:'نظرة عامة',en:'Overview',icon:Home},
 {id:'trading',title:'مركز التداول',en:'Trading Center',icon:ChartCandlestick,route:'/v1/trading/settings'},
 {id:'execution',title:'وضع تلقائي',en:'Execution',icon:Target,route:'/v1/execution'},
 {id:'brains',title:'العقول',en:'AI Brains',icon:BrainCircuit,route:'/v1/execution/brains'},
 {id:'brainControl',title:'إدارة العقول',en:'Brain Control Room',icon:Layers3,route:'/v1/execution/brains'},
 {id:'sniper',title:'القناص',en:'Sniper',icon:Target,route:'/v1/trading/settings'},
 {id:'risk',title:'إدارة المخاطر',en:'Risk',icon:ShieldCheck,route:'/v1/risk/monitor'},
 {id:'positions',title:'المراكز النشطة',en:'Positions',icon:BriefcaseBusiness,route:'/v1/execution/trades'},
 {id:'analysis',title:'التحليلات',en:'Analysis',icon:LineChart,route:'/v1/opportunities/history'},
 {id:'journal',title:'السجلات',en:'Trade Journal',icon:NotebookTabs,route:'/v1/execution/trades'},
 {id:'reports',title:'التقارير',en:'Reports',icon:FileText,route:'/v1/decisions'},
 {id:'telegram',title:'التلغرام',en:'Telegram',icon:MessageCircle,route:'/v1/config'},
 {id:'settings',title:'الإعدادات',en:'Settings',icon:Settings,route:'/v1/config'},
 {id:'shadow',title:'بحث الظل',en:'Shadow Research',icon:Eye,route:'/v1/decisions'},
 {id:'learning',title:'التعلّم الحقيقي',en:'Learning',icon:BrainCircuit,route:'/v1/learning'},
 {id:'backtest',title:'الاختبار التاريخي',en:'Backtest',icon:History,route:'/v1/backtest'},
 {id:'evaluation',title:'تقييم الاستراتيجيات',en:'Evaluation',icon:BarChart3,route:'/v1/evaluation'},
 {id:'correlation',title:'الارتباطات',en:'Correlation',icon:Workflow,route:'/v1/research/correlation'},
 {id:'studio',title:'استوديو الاستراتيجيات',en:'Strategy Studio',icon:SlidersHorizontal,route:'/v1/studio'},
 {id:'news',title:'الأخبار',en:'Market News',icon:Radio,route:'/v1/news'},
 {id:'backups',title:'النسخ الاحتياطية',en:'Backups',icon:Database,route:'/v1/backups'},
 {id:'events',title:'أحداث النظام',en:'System Events',icon:CalendarClock,route:'/v1/events'},
];
const findPage=(id:PageId)=>PAGES.find(x=>x.id===id)!;
const RISK_FIELDS=[['risk_percent','نسبة المخاطرة لكل صفقة','%'],['daily_loss_percent','الحد اليومي للخسارة','%'],['max_drawdown_percent','أقصى تراجع مسموح','%'],['max_spread_atr','سقف السبريد بالنسبة إلى ATR','ATR'],['max_lots','أقصى لوت','lot'],['commission_per_lot','العمولة لكل لوت كامل','/lot'],['slippage_ticks','انزلاق السعر','ticks'],['micro_real_recovery_equity','رصيد استعادة الحماية','equity'],['micro_real_cap_percent','سقف وضع الحساب الصغير','%']] as const;
const RISK_FLAGS=[['daily_breaker_demo_enabled','قاطع الخسارة اليومي DEMO'],['daily_breaker_real_enabled','قاطع الخسارة اليومي REAL'],['micro_demo_risk_opt_in','إذن تجربة نسبة أعلى في DEMO'],['micro_real_temporary','وضع الحساب الحقيقي الصغير المؤقت']] as const;

function buttonLabel(status:Status|null,fixture:boolean){return fixture?'PREVIEW':status?.gateway_fresh?'MT5 CONNECTED':'CORE / MT5 OFFLINE'}
const array=(x:unknown):Record<string,unknown>[]=>Array.isArray(x)?x.filter(v=>v&&typeof v==='object').slice(0,60) as Record<string,unknown>[]:[];
function unwrap(x:unknown):Record<string,unknown>{return (x&&typeof x==='object'&&!Array.isArray(x))?x as Record<string,unknown>:{} }
function flattenRows(input:unknown):Record<string,unknown>[] {
 if(Array.isArray(input))return array(input);
 const obj=unwrap(input);
 for(const candidate of ['events','trades','positions','items','results','records','history','decisions','rows','setups','backtests','snapshots','jobs','evaluations']){if(Array.isArray(obj[candidate]))return array(obj[candidate])}
 return Object.entries(obj).filter(([k,v])=>typeof v!=='object'||v===null).slice(0,35).map(([field,value])=>({field,value}));
}
function displayCell(v:unknown):string{if(v===null||v===undefined)return '—';if(typeof v==='number')return num(v,Math.abs(v)>100?2:3);if(typeof v==='boolean')return v?'نعم':'لا';if(typeof v==='object')return JSON.stringify(v).slice(0,90);return String(v).slice(0,125)}
function Table({rows,empty='لا توجد بيانات مؤكدة'}:{rows:Record<string,unknown>[];empty?:string}){
 const headers=Array.from(new Set(rows.slice(0,10).flatMap(x=>Object.keys(x)))).slice(0,9);
 return <div className="table-scroll">{rows.length===0?<div className="table-empty">{empty}</div>:<table><thead><tr>{headers.map(h=><th key={h}>{h}</th>)}</tr></thead><tbody>{rows.map((row,i)=><tr key={i}>{headers.map(h=><td title={displayCell(row[h])} key={h}>{displayCell(row[h])}</td>)}</tr>)}</tbody></table>}</div>
}
function RiskEditor({config,apply,loading,armed}:{config:Record<string,unknown>|null;apply:(v:Record<string,unknown>)=>Promise<void>;loading:boolean;armed:boolean}){
 const [values,setValues]=useState<Record<string,unknown>>({});
 // serde omits default Micro fields in the Core risk JSON; restore the
 // explicit domain defaults before editing so required inputs are never blank.
 useEffect(()=>{if(config)setValues({
  micro_real_temporary:false,micro_real_recovery_equity:500,micro_real_cap_percent:10,
  micro_demo_risk_opt_in:false,daily_breaker_demo_enabled:false,
  daily_breaker_real_enabled:true,...config
 })},[config]);
 if(!config)return <div className="table-empty">إعدادات المخاطر غير متاحة. يجب الاتصال بـCore أولاً.</div>;
 return <form className="risk-editor" onSubmit={e=>{
  e.preventDefault();if(armed)return;
  if((config.daily_breaker_real_enabled===true&&values.daily_breaker_real_enabled===false)||
     (config.micro_real_temporary!==true&&values.micro_real_temporary===true)||
     (values.micro_real_temporary===true&&Number(values.micro_real_cap_percent)>Number(config.micro_real_cap_percent||0))){
   if(!window.confirm('Micro REAL أو تعطيل قاطع الحساب قد يسبب خسارة معظم رأس المال. هل تريد حفظ هذه التغييرات؟'))return;
  }
  void apply(values);
 }}>
  <div className="risk-form-grid">{RISK_FIELDS.map(([id,title,unit])=><label key={id}>{title}<span>{unit}</span><input type="number" required step="any" disabled={loading||armed} value={typeof values[id]==='number'?String(values[id]):''} onChange={e=>setValues(old=>({...old,[id]:Number(e.target.value)}))}/></label>)}</div>
  <div className="risk-flags">{RISK_FLAGS.map(([id,title])=><label key={id}><input type="checkbox" checked={values[id]===true} disabled={loading||armed} onChange={e=>setValues(old=>({...old,[id]:e.target.checked}))}/>{title}</label>)}</div>
  <div className="quick-ops">
   <button type="button" disabled={loading||armed} onClick={()=>setValues(old=>({...old,micro_real_temporary:true,micro_real_cap_percent:5,micro_real_recovery_equity:500}))}>Micro REAL · 5%</button>
   <button type="button" disabled={loading||armed} onClick={()=>setValues(old=>({...old,micro_real_temporary:true,micro_real_cap_percent:10,micro_real_recovery_equity:500}))}>Micro REAL · 10%</button>
   <button type="button" disabled={loading||armed} onClick={()=>setValues(old=>({...old,micro_real_temporary:true,micro_real_cap_percent:20,micro_real_recovery_equity:500}))}>Micro REAL · 20% (مخاطرة مرتفعة جداً)</button>
   <button type="button" disabled={loading||armed} onClick={()=>setValues(old=>({...old,micro_real_temporary:false}))}>إلغاء Micro REAL</button>
  </div>
  <p className="safe-note">هذه النسب سقف الخسارة المقدّرة عند الستوب لكل صفقة، مو هدف ربح. تقدر تعدّل السقف وEquity التعافي يدوياً. يُطبق فقط إذا سمحت حالة الحساب وسجل التعافي والهامش، ويبقى أمر الوسيط خاضعاً للتحقق. قاطع الخسارة اليومي REAL خيار مستقل.</p>
  <button type="submit" className="save-button" disabled={loading||armed}>حفظ إعدادات المخاطر في Rust Core</button>
  {armed&&<p className="safe-note">أوقف الدخول STOP أولاً لتعديل إعدادات المخاطرة واللوت.</p>}
 </form>
}
function ConnectionLine({connectedNow,preview}:{connectedNow:boolean;preview:boolean}){
 return <span className={`source-status ${connectedNow?'online':''}`}>{preview?'DESIGN PREVIEW • SAMPLE DATA':connectedNow?'LIVE VERIFIED MT5 FEED':'CORE / MT5 UNAVAILABLE'}</span>
}
export default function Terminal(){
 const [page,setPage]=useState<PageId>('dashboard');const [theme,setTheme]=useState<Theme>(()=>(localStorage.getItem('bt-theme') as Theme)||'dark');
 const [compact,setCompact]=useState(false);const [preview,setPreview]=useState(false);const [status,setStatus]=useState<Status|null>(null);
 const [online,setOnline]=useState(false);const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [notice,setNotice]=useState('');
 const [pagePanel,setPagePanel]=useState<{scope:string;data:unknown}|null>(null);
 const [dashboardRead,setDashboardRead]=useState<DashboardRead|null>(null);
 const [pageErrorSlot,setPageErrorSlot]=useState<{scope:string;message:string}|null>(null);
 const [pageTimeSlot,setPageTimeSlot]=useState<{scope:string;timestamp:number|null}|null>(null);
 const [config,setConfig]=useState<Record<string,unknown>|null>(null);const [riskSnapshot,setRiskSnapshot]=useState<Record<string,unknown>|null>(null);
 const [configScope,setConfigScope]=useState<string|null>(null);const [riskScope,setRiskScope]=useState<string|null>(null);
 const [learning,setLearning]=useState<Record<string,unknown>|null>(null);const [brainReport,setBrainReport]=useState<Record<string,unknown>|null>(null);const [frozenSlot,setFrozenSlot]=useState<{scope:string;report:Record<string,unknown>|null}|null>(null);const [portfolioReport,setPortfolioReport]=useState<Record<string,unknown>|null>(null);const [executionQuality,setExecutionQuality]=useState<Record<string,unknown>|null>(null);const [modeEvidence,setModeEvidence]=useState<Record<string,unknown>|null>(null);const [sniperAuditSlot,setSniperAuditSlot]=useState<{scope:string;report:Record<string,unknown>|null}|null>(null);const [sniperFrozenSlot,setSniperFrozenSlot]=useState<{scope:string;report:Record<string,unknown>|null}|null>(null);const [classicFrozenSlot,setClassicFrozenSlot]=useState<{scope:string;report:Record<string,unknown>|null}|null>(null);const [filter,setFilter]=useState<'all'|'buy'|'sell'>('all');const [opsTab,setOpsTab]=useState<'trade'|'orders'|'advanced'>('trade');
 const [clock,setClock]=useState('');const [lastStatusAt,setLastStatusAt]=useState<number|null>(null);const [liveLogin,setLiveLogin]=useState('');const [approval,setApproval]=useState<OperatorApproval|null>(null);
 const [settingsEdit,setSettingsEdit]=useState<Record<string,unknown>|null>(null);
 const [settingsScope,setSettingsScope]=useState<string|null>(null);
 const [telegramToken,setTelegramToken]=useState('');const [clearTelegram,setClearTelegram]=useState(false);
 const [telegramDraftScope,setTelegramDraftScope]=useState<string|null>(null);
 const dialogRef=useRef<HTMLDivElement|null>(null);
 const [allOpen,setAllOpen]=useState(false);
 const contextKey=`${status?.telemetry?.login??'unknown'}:${status?.telemetry?.server??'offline'}:${status?.telemetry?.symbol??'none'}:${status?.telemetry?.currency??'unknown'}:${status?.telemetry?.account_trade_mode??'unknown'}:${status?.telemetry?.demo??'unknown'}:${online}:${status?.gateway_fresh??false}:${status?.telemetry?.connected??false}`;
 // Config, risk and unsaved secrets/editors are never carried into another
 // MT5 account's renderer before the background refresh useEffect runs.
 const feed=feedEvidence(status,online,lastStatusAt,Date.now());
 const quoteDiagnosis=diagnoseMarketQuote(status,feed);
 const safeConfig=configScope===contextKey?config:null;
 const safeRiskSnapshot=riskScope===contextKey?riskSnapshot:null;
 const activeSettingsEdit=settingsScope===contextKey?settingsEdit:null;
 const activeTelegramToken=!preview&&telegramDraftScope===contextKey?telegramToken:'';
 const activeClearTelegram=!preview&&telegramDraftScope===contextKey?clearTelegram:false;
 const showStartConfirm=approvalIsCurrent(approval,'start',contextKey)&&!preview&&feed.broker;
 const showStopConfirm=approvalIsCurrent(approval,'stop',contextKey)&&!preview;
 const showExitConfirm=approvalIsCurrent(approval,'exit',contextKey)&&!preview;
 // Any page, broker-account or preview transition revokes unresolved manual reads.
 const viewGeneration=useRef(new PanelReadFence());
 const scopedGeneration=useRef(new ScopedReadFence());
 const mutationFence=useRef(new OperatorWriteFence());
 scopedGeneration.current.setScope(contextKey);
 const viewScope=JSON.stringify([page,contextKey,preview]);
 // Render-fence: an old account/page result must not be visible for even a
 // single React paint while the async effect schedules a replacement read.
 // The existing generation fence separately prevents late callbacks writing.
 const pageData=pagePanel?.scope===viewScope?pagePanel.data:null;
 const activeSniperAudit=sniperAuditSlot?.scope===viewScope?sniperAuditSlot.report:null;
 const activeSniperFrozen=sniperFrozenSlot?.scope===viewScope?sniperFrozenSlot.report:null;
 const activeClassicFrozen=classicFrozenSlot?.scope===viewScope?classicFrozenSlot.report:null;
 const activeFrozenCoordinator=frozenSlot?.scope===viewScope?frozenSlot.report:null;
 const pageError=pageErrorSlot?.scope===viewScope?pageErrorSlot.message:'';
 const pageUpdatedAt=pageTimeSlot?.scope===viewScope?pageTimeSlot.timestamp:null;
 const setPageData=(data:unknown)=>setPagePanel({scope:viewScope,data});
 const setPageError=(message:string)=>setPageErrorSlot({scope:viewScope,message});
 const setPageUpdatedAt=(timestamp:number|null)=>setPageTimeSlot({scope:viewScope,timestamp});
 viewGeneration.current.setScope(viewScope);
 mutationFence.current.setScope(viewScope);
 const panelSerial=()=>viewGeneration.current.beginRead();
 const publishPanel=(serial:number,result:unknown)=>{
  if(!viewGeneration.current.isCurrent(serial))return;
  setPageData(result);setPageUpdatedAt(Date.now());setPageError('');
 };
 const rejectPanel=(serial:number,cause:unknown)=>{
  if(!viewGeneration.current.isCurrent(serial))return;
  setPageData(null);setPageUpdatedAt(null);setPageError(String(cause));
 };
 const refreshCurrentPanel=(route:string)=>{
  if(preview||!hasTauri())return;
  const serial=panelSerial();
  void readPanel(route).then(data=>publishPanel(serial,data)).catch(e=>rejectPanel(serial,e));
 };
 const sendNotice=(msg:string,isError=false)=>{if(isError)setError(msg);else{setNotice(msg);setError('')}};
 const beginWrite=(action:string):OperatorWriteTicket|null=>{
  const ticket=mutationFence.current.begin(action);
  if(!ticket){sendNotice('يوجد طلب تغيير لم يُحسم بعد. انتظر رجوع Core ثم أعد القراءة قبل تنفيذ طلب آخر.',true);return null}
  setBusy(true);return ticket;
 };
 const writeFresh=(ticket:OperatorWriteTicket)=>mutationFence.current.isCurrent(ticket);
 const finishWrite=(ticket:OperatorWriteTicket)=>{
  const fresh=writeFresh(ticket);
  mutationFence.current.finish(ticket);
  setBusy(false);
  if(!fresh)sendNotice('تغيّرت الصفحة أو جلسة MT5 أثناء تنفيذ الطلب. قد يكون Core حفظ التغيير للجلسة السابقة؛ اقرأ الحالة الحالية قبل أي تعديل.',true);
 };
 const refreshBrainEvidence=async()=>{
   const ticket=scopedGeneration.current.beginRead('brains');
   try{
     const result=unwrap(await readPanel('/v1/execution/brains'));
     if(scopedGeneration.current.isCurrent(ticket))setBrainReport(result);
   }catch{
     if(scopedGeneration.current.isCurrent(ticket))setBrainReport(null);
   }
 };
 useEffect(()=>{document.body.dataset.theme=theme;localStorage.setItem('bt-theme',theme)},[theme]);
 useEffect(()=>{const t=setInterval(()=>setClock(new Date().toLocaleTimeString('en-GB',{hour12:false})),1000);setClock(new Date().toLocaleTimeString('en-GB'));return()=>clearInterval(t)},[]);
 useEffect(()=>{
  let active=true;let streamReported=false;
  if(!hasTauri()){setPreview(true);setNotice('واجهة المتصفح تعمل للمعاينة فقط. الأوامر الحقيقية تتطلب تطبيق Tauri على ويندوز.');return}
  void coreStatus().then(s=>{if(active&&!streamReported){setStatus(s);setLastStatusAt(Date.now());setOnline(true);setError('')}}).catch(e=>{if(active&&!streamReported){setOnline(false);setStatus(null);setError(String(e))}});
  void subscribe(update=>{if(!active)return;streamReported=true;setStatus(update.ok?update.status:null);setLastStatusAt(update.ok?Date.now():null);setOnline(update.ok);setError(update.ok?'':update.error||'Core غير متصل')}).catch(e=>{if(active){setStatus(null);setOnline(false);setError(String(e))}});
  return()=>{active=false};
 },[]);
 useEffect(()=>{
  if(!hasTauri())return;
  let active=true;
  const get=(channel:string,route:string,setter:(v:Record<string,unknown>|null)=>void)=>{
    const ticket=scopedGeneration.current.beginRead(channel);
    return readPanel(route).then(v=>{
      if(active&&scopedGeneration.current.isCurrent(ticket))setter(unwrap(v));
    }).catch(()=>{
      if(active&&scopedGeneration.current.isCurrent(ticket))setter(null);
    });
  };
  // Invalidate old account/mode snapshots synchronously with scope change.
  setRiskSnapshot(null);setLearning(null);setBrainReport(null);
  setConfig(null);setSettingsEdit(null);
  void get('config','/v1/config',r=>{
    setConfig(r?unwrap(r.config):null);setSettingsEdit(r?unwrap(r.config):null);
    setConfigScope(contextKey);setSettingsScope(contextKey);
  });
  void get('risk','/v1/risk/monitor',r=>{setRiskSnapshot(r);setRiskScope(contextKey)});
  void get('learning','/v1/learning',setLearning);
  void get('brains','/v1/execution/brains',setBrainReport);
  const t=setInterval(()=>{
    if(document.visibilityState==='visible'){
      void get('risk','/v1/risk/monitor',r=>{setRiskSnapshot(r);setRiskScope(contextKey)});
      void get('learning','/v1/learning',setLearning);
      void get('brains','/v1/execution/brains',setBrainReport);
    }
  },12000);
  return()=>{active=false;clearInterval(t)};
 },[contextKey]);
 useEffect(()=>{
  if(page!=='risk'||!hasTauri()||preview){setPortfolioReport(null);return;}
  let active=true;
  const get=()=>{
   const ticket=scopedGeneration.current.beginRead('portfolio');
   void readPanel('/v1/execution').then(v=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setPortfolioReport(unwrap(v));
   }).catch(()=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setPortfolioReport(null);
   });
  };
  setPortfolioReport(null);get();
  const timer=setInterval(()=>{if(document.visibilityState==='visible')get()},12000);
  return()=>{active=false;clearInterval(timer)};
 },[page,preview,contextKey]);
 useEffect(()=>{
  if(page!=='execution'||preview||!hasTauri()){setExecutionQuality(null);return;}
  let active=true;
  const get=()=>{
   const ticket=scopedGeneration.current.beginRead('executionQuality');
   void readPanel('/v1/execution/quality').then(v=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setExecutionQuality(unwrap(v));
   }).catch(()=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setExecutionQuality(null);
   });
  };
  setExecutionQuality(null);get();
  const timer=setInterval(()=>{if(document.visibilityState==='visible')get()},3000);
  return()=>{active=false;clearInterval(timer)};
 },[page,preview,contextKey]);
 useEffect(()=>{
  if(page!=='sniper'||preview||!hasTauri()){setSniperFrozenSlot(null);return;}
  let alive=true;
  const refresh=()=>{
   const ticket=scopedGeneration.current.beginRead('sniperFrozen');
   void readPanel('/v1/execution/frozen-sniper').then(v=>{
    if(alive&&scopedGeneration.current.isCurrent(ticket))
      setSniperFrozenSlot({scope:viewScope,report:unwrap(v)});
   }).catch(()=>{
    if(alive&&scopedGeneration.current.isCurrent(ticket))
      setSniperFrozenSlot({scope:viewScope,report:null});
   });
  };
  setSniperFrozenSlot(null);refresh();
  const timer=setInterval(()=>{if(document.visibilityState==='visible')refresh()},5000);
  return()=>{alive=false;clearInterval(timer)};
 },[page,preview,contextKey]);
 useEffect(()=>{
  if(page!=='sniper'||preview||!hasTauri()){setClassicFrozenSlot(null);return;}
  let alive=true;
  const refresh=()=>{
   const ticket=scopedGeneration.current.beginRead('classicFrozen');
   void readPanel('/v1/execution/frozen-classic').then(v=>{
    if(alive&&scopedGeneration.current.isCurrent(ticket))
     setClassicFrozenSlot({scope:viewScope,report:unwrap(v)});
   }).catch(()=>{
    if(alive&&scopedGeneration.current.isCurrent(ticket))
     setClassicFrozenSlot({scope:viewScope,report:null});
   });
  };
  setClassicFrozenSlot(null);refresh();
  const timer=setInterval(()=>{if(document.visibilityState==='visible')refresh()},5000);
  return()=>{alive=false;clearInterval(timer)};
 },[page,preview,contextKey]);
 useEffect(()=>{
  if(page!=='sniper'||preview||!hasTauri()){setSniperAuditSlot(null);return;}
  let alive=true;
  const refresh=()=>{
   const ticket=scopedGeneration.current.beginRead('sniperTickAudit');
   void readPanel('/v1/execution/sniper-tick-audit').then(v=>{
    if(alive&&scopedGeneration.current.isCurrent(ticket))
      setSniperAuditSlot({scope:viewScope,report:unwrap(v)});
   }).catch(()=>{
    if(alive&&scopedGeneration.current.isCurrent(ticket))
      setSniperAuditSlot({scope:viewScope,report:null});
   });
  };
  setSniperAuditSlot(null);refresh();
  const timer=setInterval(()=>{if(document.visibilityState==='visible')refresh()},3000);
  return()=>{alive=false;clearInterval(timer)};
 },[page,preview,contextKey]);
 useEffect(()=>{
  if(page!=='execution'||preview||!hasTauri()){setModeEvidence(null);return;}
  let active=true;
  const get=()=>{
   const ticket=scopedGeneration.current.beginRead('modeEvidence');
   void readPanel('/v1/execution/mode-evidence').then(v=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setModeEvidence(unwrap(v));
   }).catch(()=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setModeEvidence(null);
   });
  };
  setModeEvidence(null);get();
  const timer=setInterval(()=>{if(document.visibilityState==='visible')get()},12000);
  return()=>{active=false;clearInterval(timer)};
 },[page,preview,contextKey]);
 useEffect(()=>{
  if(!['brains','brainControl'].includes(page)||preview||!hasTauri()){setFrozenSlot(null);return;}
  let active=true;
  const get=()=>{
   const ticket=scopedGeneration.current.beginRead('frozenCoordinator');
   void readPanel('/v1/execution/frozen-coordinator').then(v=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setFrozenSlot({scope:viewScope,report:unwrap(v)});
   }).catch(()=>{
    if(active&&scopedGeneration.current.isCurrent(ticket))setFrozenSlot({scope:viewScope,report:null});
   });
  };
  setFrozenSlot(null);get();
  const interval=setInterval(()=>{if(document.visibilityState==='visible')get()},12000);
  return()=>{active=false;clearInterval(interval)};
 },[page,preview,contextKey]);
 useEffect(()=>{
  if(page==='journal'){setPageData(null);setPageUpdatedAt(null);setPageError('');return;}
   if(page==='dashboard'||!hasTauri()||preview){setPageData(null);setPageUpdatedAt(null);setPageError('');return;}
  const route=findPage(page).route;
  if(!route){setPageData(null);setPageUpdatedAt(null);setPageError('لا يوجد مصدر مرتبط بهذه الصفحة');return;}
  let active=true;let fetching=false;
  const refresh=async()=>{
    if(!active||fetching||document.visibilityState!=='visible')return;
    fetching=true;
    const serial=panelSerial();
    try{const data=await readPanel(route);if(active)publishPanel(serial,data)}
    catch(e){if(active)rejectPanel(serial,e)}
    finally{fetching=false}
  };
  setPageData(null);setPageUpdatedAt(null);setPageError('');void refresh();
  const timer=setInterval(()=>void refresh(),['events','positions','journal','brains','brainControl','execution','risk','trading','sniper'].includes(page)?3000:10000);
  const visible=()=>{if(document.visibilityState==='visible')void refresh()};
  document.addEventListener('visibilitychange',visible);
  return()=>{active=false;clearInterval(timer);document.removeEventListener('visibilitychange',visible)};
 },[page,preview,contextKey]);
 // G3: a single generation-scoped read-only bundle from the actual
 // Core account, trades and broker quality. No invented LIVE metrics.
 useEffect(()=>{
  if(page!=='dashboard'||preview||!hasTauri()){setDashboardRead(null);return}
  let alive=true,fetching=false;
  const refresh=async()=>{
   if(!alive||fetching||document.visibilityState!=='visible')return;
   fetching=true;
   const ticket=scopedGeneration.current.beginRead('dashboardBroker');
   try{
    const [execution,trades,quality]=await Promise.all([
     readPanel('/v1/execution'),readPanel('/v1/execution/trades'),
     readPanel('/v1/execution/quality')]);
    if(alive&&scopedGeneration.current.isCurrent(ticket))
     setDashboardRead({scope:contextKey,readMs:Date.now(),execution,trades,quality});
   }catch{
    if(alive&&scopedGeneration.current.isCurrent(ticket))setDashboardRead(null);
   }finally{fetching=false}
  };
  setDashboardRead(null);void refresh();
  const interval=setInterval(()=>{void refresh()},10000);
  const visible=()=>{if(document.visibilityState==='visible')void refresh()};
  document.addEventListener('visibilitychange',visible);
  return()=>{alive=false;clearInterval(interval);document.removeEventListener('visibilitychange',visible)};
 },[page,preview,contextKey]);
 const display=preview?DESIGN_FIXTURE:status;
 // Account metadata (including balance/equity) is valid with a current
 // verified MT5 broker snapshot, even when the market is CLOSED or ticks stale.
 // Trading/quote actions MUST still require fresh market prices separately.
 const t=display?.telemetry;const trustworthy=preview||feed.quote;
 const accountFresh=preview||feed.broker;
 const account=accountFresh?t:null;const currency=account?.currency||'USD';const positions=account?.positions||[];
 const buys=positions.filter(p=>up(p.side)).length;const sells=positions.length-buys;
 const brokerResults=preview||!feed.broker?null:brokerEvidence(brainReport,status);
 const win=preview?76:brokerResults?.winRate??null;
 // Core daily-loss estimate is NOT verified maximum drawdown.
 const drawdown=preview?4.2:null;
 const showLatency=preview?28:null; // quote age is NOT execution latency
 const visiblePositions=positions.filter(p=>filter==='all'||(filter==='buy'?up(p.side):!up(p.side)));
 useEffect(()=>{
  if(approval&&(approval.scope!==contextKey||preview||
    (approval.action==='start'&&!feed.broker))){
   // Revoke STOP and EXIT too; they must never revive if an old account
   // reappears after a switch or a temporary transport interruption.
   setApproval(null);setLiveLogin('');
  }
 },[approval,contextKey,feed.broker,preview]);
 // A dialog is not durable consent: broker/account/freshness changes revoke it.
 // Restore focus after closing and trap keyboard Tab within the active dialog.
 useEffect(()=>{
  if(!approval||approval.scope!==contextKey||preview)return;
  const previous=document.activeElement instanceof HTMLElement?document.activeElement:null;
  const el=dialogRef.current;
  const focusables=()=>Array.from(el?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled)')??[]);
  focusables()[0]?.focus();
  const keydown=(event:KeyboardEvent)=>{
   if(event.key==='Escape'&&!busy){event.preventDefault();setApproval(null);setLiveLogin('');return}
   if(event.key!=='Tab'||!el)return;
   const items=focusables();if(!items.length){event.preventDefault();return}
   const index=items.indexOf(document.activeElement as HTMLElement);
   if(event.shiftKey&&(index<=0)){event.preventDefault();items[items.length-1].focus()}
   else if(!event.shiftKey&&(index===items.length-1||index<0)){event.preventDefault();items[0].focus()}
  };
  document.addEventListener('keydown',keydown);
  return()=>{document.removeEventListener('keydown',keydown);previous?.focus()};
 },[approval,contextKey,preview,busy]);
 const runCommand=async(action:'start'|'stop'|'reconnect',login?:string)=>{
  if(preview||!hasTauri()){sendNotice('أوامر التداول تعمل من تطبيق Windows الحقيقي فقط',true);return}
  // Emergency STOP is deliberately exempt from the UI write queue: it must
  // remain requestable while a noncritical IPC mutation is outstanding.
  // Core remains authoritative and STOP never closes existing positions.
  if(action==='stop'){
   try{await control('stop');sendNotice('تم إرسال STOP إلى Core. لا يغلق الصفقات المفتوحة.')}
   catch(e){sendNotice('فشل طلب STOP؛ راقب حالة Core: '+String(e),true)}
   return;
  }
  if(action==='start'&&!canConfirmStart(approval,contextKey,status,feed.broker,login??'')){
   setApproval(null);setLiveLogin('');sendNotice('تغيّر الحساب أو انتهت حداثة MT5؛ افتح تأكيد START من جديد',true);return;
  }
  const mutation=beginWrite('control-'+action);if(!mutation)return;
  try{
   await control(action,login);
   if(!writeFresh(mutation))return;
   sendNotice(action==='start'?'تم إرسال طلب START إلى Core':'أرسل Core طلب إعادة الاتصال. راقب حالة MT5.');
   setApproval(null);setLiveLogin('');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const requestStart=()=>{
  if(preview||busy||!hasTauri()||!feed.broker||!status?.telemetry||status.requested_run){
   sendNotice('بدء التداول يحتاج تطبيق Windows، حساب MT5 موثوق وSTOP قبل طلب START',true);return;
  }
  setLiveLogin('');setApproval({action:'start',scope:contextKey});
 };
 const requestStop=()=>{if(preview||!hasTauri())return;setApproval({action:'stop',scope:contextKey})};
 const confirmStop=()=>{
  if(!approvalIsCurrent(approval,'stop',contextKey)){setApproval(null);sendNotice('تغيّرت الجلسة؛ أعد تأكيد STOP',true);return}
  setApproval(null);void runCommand('stop');
 };
 const runFullExit=async()=>{
  if(preview||!hasTauri()||!approvalIsCurrent(approval,'exit',contextKey)){
   setApproval(null);sendNotice('الخروج الكامل يحتاج تأكيداً جديداً لنفس جلسة Core',true);return;
  }
  const mutation=beginWrite('shutdown');if(!mutation)return;
  try{
   await shutdownCore();
   if(!writeFresh(mutation))return;
   setApproval(null);
   await closeTerminal();
  }catch(e){if(writeFresh(mutation))sendNotice('رفض Core الخروج الكامل: '+String(e),true);setApproval(null)}
  finally{finishWrite(mutation)}
 };
 const runBackup=async(action:'create'|'brain_export')=>{
  if(preview||!hasTauri())return sendNotice('إنشاء النسخ يتطلب Core الحقيقي',true);
  const mutation=beginWrite('backup-'+action);if(!mutation)return;
  const serial=panelSerial();
  try{
   const outcome=unwrap(await backupAction(action));
   if(!writeFresh(mutation))return;
   const data=await readPanel('/v1/backups');
   if(!writeFresh(mutation))return;
   publishPanel(serial,data);
   sendNotice(action==='create'?'تم إنشاء نسخة محلية لدى Core':'تم تصدير Brain Snapshot لدى Core');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runBacktest=async(action:'start'|'cancel',request?:{from_date:string;to_date:string;initial_equity:number;spread_floor:number})=>{
  if(preview||!hasTauri())return sendNotice('الاختبارات التاريخية تتطلب Core وMT5',true);
  const mutation=beginWrite('backtest-'+action);if(!mutation)return;
  const serial=panelSerial();
  try{
   await backtestAction(action,request);
   if(!writeFresh(mutation))return;
   const data=await readPanel('/v1/backtest');
   if(!writeFresh(mutation))return;
   publishPanel(serial,data);
   sendNotice(action==='start'?'بدأت مهمة الاختبار التاريخي في Core':'استلم Core طلب إلغاء الاختبار');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const changePreview=()=>{
  // Preview is not an operator account. Discard pending consent and unsaved
  // sensitive Telegram edits; never revive them after leaving demo visuals.
  setApproval(null);setLiveLogin('');
  setTelegramToken('');setClearTelegram(false);setTelegramDraftScope(null);
  setPreview(v=>!v);setPage('dashboard');
  sendNotice('وضع المعاينة منفصل تماماً عن أوامر التداول؛ أُلغيت التأكيدات ومسودات الأسرار');
 };
 const open=(next:PageId)=>{setPage(next);setNotice('');setError('')};
 const saveRiskForm=async(risk:Record<string,unknown>)=>{
  if(!hasTauri()||preview||status?.requested_run){sendNotice('تعديل المخاطر يتطلب توقف الدخول وحساب Core فعلي',true);return}
  const mutation=beginWrite('risk');if(!mutation)return;
  try{
   await saveRisk(risk);
   if(!writeFresh(mutation))return;
   const latest=unwrap(await readPanel('/v1/config'));
   if(!writeFresh(mutation))return;
   scopedGeneration.current.invalidate('risk');
   setConfig(unwrap(latest.config));setConfigScope(contextKey);setRiskSnapshot(null);
   sendNotice('تم حفظ إعدادات المخاطر والتحقق من النسخة المسجلة في Rust Core');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runSavePortfolio=async(input:Record<string,unknown>)=>{
  if(preview||!hasTauri()||status?.requested_run){sendNotice('حدود المحفظة تحتاج STOP وتطبيق Windows الحقيقي',true);return}
  const mutation=beginWrite('portfolio');if(!mutation)return;
  try{
   await savePortfolio(validatePortfolio(input));
   if(!writeFresh(mutation))return;
   const updated=unwrap(await readPanel('/v1/execution'));
   if(!writeFresh(mutation))return;
   scopedGeneration.current.invalidate('portfolio');
   setPortfolioReport(updated);
   sendNotice('تم حفظ حدود المحفظة الاختيارية في Rust Core');
  }catch(e){if(writeFresh(mutation))sendNotice('تعذر حفظ حدود المحفظة: '+String(e),true)}
  finally{finishWrite(mutation)}
 };
 const saveGeneral=async()=>{
  if(!activeSettingsEdit||preview||!hasTauri()||status?.requested_run){sendNotice('احفظ فقط بعد تحميل إعدادات Core للحساب الحالي وفي وضع STOP',true);return}
  const mutation=beginWrite('config');if(!mutation)return;
  try{
   await saveSettings(activeSettingsEdit,activeTelegramToken,activeClearTelegram);
   if(!writeFresh(mutation))return;
   const latest=unwrap(await readPanel('/v1/config'));
   if(!writeFresh(mutation))return;
   scopedGeneration.current.invalidate('risk');
   setConfig(unwrap(latest.config));setConfigScope(contextKey);
   setSettingsEdit(unwrap(latest.config));setSettingsScope(contextKey);
   setRiskSnapshot(null);setTelegramToken('');setClearTelegram(false);setTelegramDraftScope(null);
   sendNotice('تم حفظ الإعدادات لدى Core. إعادة الاتصال قيد التنفيذ.');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runHandoff=async(active:boolean)=>{
  if(preview||!hasTauri()||status?.requested_run){sendNotice('تغيير المنسق يحتاج STOP داخل التطبيق الحقيقي',true);return}
  const mutation=beginWrite('handoff');if(!mutation)return;
  try{
   await handoff(active);
   if(!writeFresh(mutation))return;
   scopedGeneration.current.invalidate('brains');setBrainReport(null);
   void refreshBrainEvidence();
   sendNotice('تحويل أولوية القناص تم تأكيده من Core');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runSaveSupervisor=async(input:Record<string,unknown>):Promise<Record<string,unknown>>=>{
  if(preview||!hasTauri()||status?.requested_run)throw new Error('تعديل سياسة المنسق يحتاج Core وSTOP');
  const mutation=beginWrite('supervisor');if(!mutation)throw new Error('طلب تغيير سابق لم يحسم');
  try{
   await saveSupervisor(input);
   if(!writeFresh(mutation))throw new Error('تبدلت جلسة MT5 أثناء حفظ السياسة');
   const report=unwrap(await readPanel('/v1/enterprise/supervisor'));
   if(!writeFresh(mutation))throw new Error('تبدلت جلسة MT5 أثناء قراءة سياسة المنسق');
   if(report.configuration_integrity!=='ok')throw new Error('Core لم يؤكد سلامة سياسة المنسق');
   return report;
  }catch(e){
   if(writeFresh(mutation))sendNotice('لم يتم اعتماد سياسة المنسق: '+String(e),true);
   throw e;
  }finally{finishWrite(mutation)}
 };
 const runSaveTrading=async(input:Record<string,unknown>)=>{
  if(preview||!hasTauri()||status?.requested_run){sendNotice('حفظ الوضع والتكاليف يحتاج STOP وCore الأصلي',true);return}
  const mutation=beginWrite('trading');if(!mutation)return;
  const serial=panelSerial();
  try{
   await saveTrading(input);
   if(!writeFresh(mutation))return;
   const latest=unwrap(await readPanel('/v1/trading/settings'));
   if(!writeFresh(mutation))return;
   const full=unwrap(latest.config);
   publishPanel(serial,latest);
   if(viewGeneration.current.isCurrent(serial)){
    scopedGeneration.current.invalidate('config');scopedGeneration.current.invalidate('brains');
    setConfig(full);setConfigScope(contextKey);setBrainReport(null);void refreshBrainEvidence();
   }
   sendNotice('تم تأكيد وحفظ وضع المنسق والعمولة من Rust Core');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runFrozenSniper=async(action:'freeze'|'stop')=>{
  const t=status?.telemetry;
  if(preview||!hasTauri()||page!=='sniper'||status?.requested_run
     ||!eligibleFrozenSniper(status,pageData)||!t?.connected){
    sendNotice('بحث Sniper Learner يحتاج STOP وMT5 Demo ووضع القناص المتعلم',true);return;
  }
  const ctx=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
  const mutation=beginWrite('sniper-frozen');if(!mutation)return;
  try{
    const result=unwrap(await frozenSniperAction(action,ctx));
    if(!writeFresh(mutation))return;
    if(result.ok!==true||result.no_broker_orders!==true && action==='freeze'){
      sendNotice('Core لم يؤكد تنفيذ الأمر البحثي',true);return
    }
    const ticket=scopedGeneration.current.beginRead('sniperFrozen');
    const read=unwrap(await readPanel('/v1/execution/frozen-sniper'));
    if(!writeFresh(mutation))return;
    if(scopedGeneration.current.isCurrent(ticket))
      setSniperFrozenSlot({scope:viewScope,report:read});
    sendNotice(action==='freeze'?'تم تثبيت نسخة Sniper Learner البحثية في Core':
      'تم إيقاف المقارنة البحثية مع بقاء تنفيذ التداول دون تغيير');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runFrozenClassic=async(action:'freeze'|'stop')=>{
  const t=status?.telemetry;
  if(preview||!hasTauri()||page!=='sniper'||status?.requested_run
     ||!eligibleFrozenClassic(status,pageData)||!t?.connected){
    sendNotice('بحث Sniper Classic يحتاج STOP وMT5 Demo ووضع Classic',true);return;
  }
  const ctx=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
  const mutation=beginWrite('classic-frozen');if(!mutation)return;
  try{
    const result=unwrap(await frozenClassicAction(action,ctx));
    if(!writeFresh(mutation))return;
    if(result.ok!==true||result.no_broker_orders!==true){
     sendNotice('Core لم يؤكد العملية البحثية',true);return
    }
    const ticket=scopedGeneration.current.beginRead('classicFrozen');
    const read=unwrap(await readPanel('/v1/execution/frozen-classic'));
    if(!writeFresh(mutation))return;
    if(scopedGeneration.current.isCurrent(ticket))
      setClassicFrozenSlot({scope:viewScope,report:read});
    sendNotice(action==='freeze'?'تم بدء دراسة Sniper Classic المستقبلية في Core':
      'تم إيقاف دراسة Classic دون التأثير على التنفيذ');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runFrozenCoordinator=async(action:'freeze'|'stop')=>{
  const t=status?.telemetry;
  if(preview||!hasTauri()||status?.requested_run||!status?.gateway_fresh
    ||!t?.connected||!t.demo||t.account_trade_mode!==0
    ||!['brains','brainControl'].includes(page)){sendNotice('التجميد البحثي يتطلب STOP وMT5 Demo موصول',true);return}
  const expected=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
  const chosenMode=unwrap(pageData).trading_mode;
  if(unwrap(pageData).context!==expected||!supportedFrozenMode(chosenMode)){
   sendNotice('سياق حساب MT5 Demo أو وضع التداول تغيّر، أو Sniper يحتاج دراسة Tick مستقلة',true);return
  }
  const mutation=beginWrite('frozen-coordinator');if(!mutation)return;
  try{
   await frozenCoordinatorAction(action,expected,chosenMode);
   if(!writeFresh(mutation))return;
   const ticket=scopedGeneration.current.beginRead('frozenCoordinator');
   const refreshed=unwrap(await readPanel('/v1/execution/frozen-coordinator'));
   if(!writeFresh(mutation))return;
   if(scopedGeneration.current.isCurrent(ticket))setFrozenSlot({scope:viewScope,report:refreshed});
   sendNotice(action==='freeze'?('تم تثبيت تجربة '+chosenMode+' البحثية في Rust Core'):'تم إيقاف تجربة '+chosenMode+' البحثية وحفظ نتائجها في Core');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runSaveBrain=async(brain:string,advisor:boolean,paper:boolean)=>{
  if(preview||!hasTauri()||!status?.gateway_fresh||!status.telemetry||status.requested_run){sendNotice('أوقف التداول وانتظر اتصال MT5 حديث لإدارة العقول',true);return}
  const t=status.telemetry;
  if(t.account_trade_mode!==(t.demo?0:2)){sendNotice('نوع حساب MT5 غير مؤكّد؛ لا يمكن تغيير دور العقل',true);return}
  const context=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
  const savedContext=String(unwrap(pageData).context||'');
  if(savedContext!==context){sendNotice('تغيّر الحساب؛ انتظر تحميل صفحة العقول للحساب الحالي',true);return}
  const mode=String(unwrap(pageData).trading_mode||'');
  const mutation=beginWrite('brain-control');if(!mutation)return;
  const serial=panelSerial();
  try{
   await updateBrainControl(context,brain,mode,advisor,paper);
   if(!writeFresh(mutation))return;
   scopedGeneration.current.invalidate('brains');setBrainReport(null);
   const ticket=scopedGeneration.current.beginRead('brains');
   const refreshed=unwrap(await readPanel('/v1/execution/brains'));
   if(!writeFresh(mutation))return;
   publishPanel(serial,refreshed);
   if(scopedGeneration.current.isCurrent(ticket))setBrainReport(refreshed);
   sendNotice('تم حفظ مشاركة العقل في Core للحساب والوضع الحاليين');
  }catch(e){if(writeFresh(mutation))sendNotice(String(e),true)}
  finally{finishWrite(mutation)}
 };
 const runBrokerBind=async(target:'demo'|'live',deviationPoints:number,confirmLiveLogin:string)=>{
  if(preview||!hasTauri()||status?.requested_run){sendNotice('اربط حساب التنفيذ فقط بعد STOP ومن تطبيق Tauri الحقيقي',true);return}
  const mutation=beginWrite('broker-binding');if(!mutation)return;
  const serial=panelSerial();
  try{
   await bindExecution(target,deviationPoints,confirmLiveLogin);
   if(!writeFresh(mutation))return;
   const refreshed=await readPanel('/v1/execution');
   if(!writeFresh(mutation))return;
   scopedGeneration.current.invalidate('brains');setBrainReport(null);
   publishPanel(serial,refreshed);
   void refreshBrainEvidence();
   sendNotice('تم التحقق من حساب '+target.toUpperCase()+' وربط التنفيذ بواسطة Core؛ الربط وحده لا يفتح صفقة');
  }catch(e){if(writeFresh(mutation))sendNotice('فشل ربط حساب التنفيذ: '+String(e),true)}
  finally{finishWrite(mutation)}
 };
 const health=buildHealth(status,online,preview,Boolean(safeConfig?.risk)&&Boolean(safeRiskSnapshot),lastStatusAt,Date.now());
 const pageMeta=findPage(page),PageIcon=pageMeta.icon;
 const rows=useMemo(()=>flattenRows(pageData),[pageData]);
 return <div className={'shell terminal-v15 '+(compact?'compact-nav':'')}>
 <aside className="sidebar"><div className="brand"><div className="brand-logo">BRAIN<span>TRADE</span> <b>V4</b></div><div className="brand-desc">Institutional Trading System</div></div>
  <nav id="sidebarNav" aria-label="قائمة صفحات BRAINTRADE">
   {PAGES.filter((x,i)=>allOpen||i<11||x.id===page).map(x=>{const Icon=x.icon;return <button type="button" key={x.id} title={x.title} aria-current={page===x.id?'page':undefined} className={'nav-item '+(page===x.id?'selected':'')} onClick={()=>open(x.id)}><span className="nav-glyph"><Icon size={21} strokeWidth={1.9}/></span><span>{x.title}</span></button>})}
   <button type="button" className="nav-item all-pages" aria-expanded={allOpen} onClick={()=>setAllOpen(v=>!v)}><Layers3 size={19}/><span>{allOpen?'إخفاء الصفحات الإضافية':'كل الصفحات الـ22'}</span></button>
  </nav>
  <SystemHealth health={health} onOpen={open}/>
  <div className="sidebar-footer"><button type="button" className="safe-exit-button" onClick={()=>setApproval({action:'exit',scope:contextKey})} disabled={busy||preview}>خروج كامل آمن</button><button type="button" className="safe-exit-button" onClick={()=>void closeTerminal()} disabled={!hasTauri()}>إغلاق الواجهة فقط</button><strong>BRAIN<span>TRADE</span> V4</strong><small>Tauri 2 + React + Rust</small><button className="compact-toggle" onClick={()=>setCompact(v=>!v)}>{compact?'توسيع القائمة':'طي القائمة'}</button></div>
 </aside>
 <section className="main">
   <header className="account-strip">
    <div className="account-item broker-select"><label>Broker</label><button className="select-look" onClick={()=>open('settings')}><span className="broker-mark">MT5</span><strong>{account?.connected?'Connected broker':'Not connected'}</strong><span>⌄</span></button></div>
    <div className="account-item server-select"><label>Server</label><button className="select-look" onClick={()=>open('execution')}><strong>{account?.server||'—'}</strong><span>⌄</span></button></div>
    <div className="account-item account-switch"><label>Account · MT5</label><div className="switch-buttons"><span aria-label="حساب الوسيط المتصل فعلياً">{account?(account.demo?'DEMO متصل':'REAL متصل'):'غير متصل'}</span><button type="button" title="فتح إعدادات الربط، وليس تبديل حساب MT5 مباشرة" onClick={()=>open('execution')}>إدارة الربط ↗</button></div></div>
    {[['Equity',money(account?.equity,currency)],['Balance',money(account?.balance,currency)],['Margin',money(account?.margin,currency)],['Free Margin',money(account?.margin_free,currency)],['Daily P&L',signed(account?.today_net,currency)],['Quote Age',preview?'28 ms':feed.quote?num(feed.quoteAgeMs,0)+' ms':'—']].map(([label,value])=><div key={label} className={'top-stat '+(label==='Daily P&L'?'daily':'')}><label>{label}</label><b>{value}</b></div>)}
    <button className="connection-pill" onClick={()=>{if((status?.requested_run||positions.length>0)&&!window.confirm('ستتم إعادة تشغيل جسر MT5. لن تُغلق الصفقات المفتوحة، لكن إدارة خروجها قد تتأخر أثناء الاتصال. تابع فقط عند الضرورة.'))return;void runCommand('reconnect')}} disabled={busy||preview}><span className={'pulse-dot '+(accountFresh?'online':'')}></span><span>{preview?'معاينة':accountFresh?'متصل':'غير متصل'}</span><RefreshCw size={16}/></button>
   </header>
   {preview&&<div className="preview-banner">◈ معاينة التصميم فقط — أرقام وأسعار وصفقات غير حقيقية — لا أوامر مالية <button onClick={changePreview}>الخروج من المعاينة</button></div>}
   {(error||notice)&&<div className={'terminal-notice '+(error?'bad':'')} role="status"><span>{error||notice}</span><button onClick={()=>{setNotice('');setError('')}}>×</button></div>}
   <main className="content">
    {page==='dashboard'?<div className="dashboard">
     <div className="kpi-row">
      <article className="kpi kpi-equity interactive" onClick={()=>open('execution')}><div className="kpi-glyph"><Layers3/></div><div className="kpi-copy"><div className="kpi-ar">حقوق الملكية</div><div className="kpi-en">Equity</div><strong>{money(account?.equity,currency)}</strong><div className="kpi-footer teal">{account?'MT5 account':'غير متصل'}</div></div></article>
      <article className="kpi kpi-profit interactive" onClick={()=>open('reports')}><div className="kpi-glyph"><BarChart3/></div><div className="kpi-copy"><div className="kpi-ar">الربح / الخسارة اليومي</div><div className="kpi-en">Daily P&amp;L</div><strong>{signed(account?.today_net,currency)}</strong><div className="kpi-footer teal">{preview?'+2.40% • PREVIEW':'نتيجة اليوم من MT5'}</div></div><div className="kpi-spark-decoration">▁▃▂▄▆▄▇</div></article>
      <article className="kpi kpi-loss interactive" onClick={()=>open('risk')}><div className="kpi-glyph"><Siren/></div><div className="kpi-copy"><div className="kpi-ar">أقصى تراجع</div><div className="kpi-en">Drawdown</div><strong>{valid(drawdown)?num(drawdown,2)+'%':'—'}</strong><div className="loss-meter"><span style={{width:valid(drawdown)?`${Math.min(100,drawdown*8)}%`:'0%'}}/></div><div className="kpi-footer red">{valid(drawdown)?'من مصدر مخاطر موثوق':'لا توجد عينة معتمدة'}</div></div></article>
      <article className="kpi kpi-positions interactive" onClick={()=>open('positions')}><div className="kpi-glyph"><BriefcaseBusiness/></div><div className="kpi-copy"><div className="kpi-ar">المراكز النشطة</div><div className="kpi-en">Active Positions</div><strong>{account?positions.length:'—'}</strong><div className="buy-sell"><span>{account?`${buys} شراء`:'— شراء'}</span><span>{account?`${sells} بيع`:'— بيع'}</span></div><div className="position-meter"><i style={{width:positions.length?`${100*buys/positions.length}%`:'0%'}}></i><i style={{width:positions.length?`${100*sells/positions.length}%`:'0%'}}></i></div></div></article>
      <article className="kpi kpi-win interactive" onClick={()=>open('learning')}><div className="kpi-glyph"><Target/></div><div className="kpi-copy"><div className="kpi-ar">معدل النجاح</div><div className="kpi-en">Win Rate</div><strong>{valid(win)?num(win,0)+'%':'—'}</strong><div className="kpi-footer">{preview?'342 / 450 • PREVIEW':brokerResults?`${brokerResults.samples} MT5 DEMO • بعد التكاليف`:'لا توجد عينات Demo مغلقة مؤهلة'}</div></div><div className="donut" style={{background:valid(win)?`conic-gradient(#04dbb4 ${win}%,#29434f 0)`:'none'}}></div></article>
      <article className="kpi kpi-latency interactive" onClick={()=>open('execution')}><div className="kpi-glyph"><Activity/></div><div className="kpi-copy"><div className="kpi-ar">زمن التنفيذ</div><div className="kpi-en">Execution latency</div><strong>{showLatency===null?'—':showLatency+' ms'}</strong><div className="kpi-footer teal">{preview?'DESIGN PREVIEW':'غير مقاس بواسطة الوسيط'}</div></div><div className="signal-bars"><i/><i/><i/><i/><i/></div></article>
     </div>
     <div className="hero-grid">
      <TradingChart market={account?.market} symbol={account?.symbol||'XAUUSD'} quote={account?.bid??undefined}/>
      <section className="panel ops-panel">
       <div className="ops-tabs" role="tablist">{[['trade','التداول المباشر'],['orders','إدارة الأوامر'],['advanced','متداول متقدم']].map(([key,label])=><button key={key} role="tab" aria-selected={opsTab===key} className={opsTab===key?'active':''} onClick={()=>setOpsTab(key as typeof opsTab)}>{label}</button>)}</div>
       <div className="ops-body">
       {opsTab==='trade'?<>
        <div className="action-row double"><button className="action-btn start" onClick={requestStart} disabled={busy||preview||!feed.broker||status?.requested_run===true}><Play size={23}/><span>بدء التداول<small>Start</small></span></button><button className="action-btn stop" onClick={requestStop} disabled={preview||!hasTauri()}><PauseCircle size={23}/><span>وقف الدخول<small>Stop entries</small></span></button><button className="action-btn shadow" onClick={()=>open('shadow')}><Eye size={21}/><span>ظل<small>Shadow</small></span></button></div>
        <div className="action-row triple"><button className="pill-btn" title="فتح شاشة ربط حساب MT5 Demo. هذا الزر لا يغيّر الحساب مباشرة" onClick={()=>open('execution')}>▣ <span>إعداد Demo</span></button><button className="pill-btn green" title="فتح شاشة ربط REAL مع تأكيد رقم الحساب؛ لا يغيّر MT5 مباشرة" onClick={()=>open('execution')}>◉ <span>إعداد Live</span></button><button className="pill-btn slate" onClick={()=>open('learning')}>⚙ <span>التعلّم<small>Learning</small></span></button></div>
        <div className="action-row quadruple"><button className="mini-btn risk" onClick={()=>open('risk')}>◉ <span>المخاطر<small>Risk</small></span></button><button className="mini-btn risk-off" onClick={()=>open('risk')}>◌ <span>Limits<small>Safety</small></span></button><button className="mini-btn telegram" onClick={()=>open('telegram')}>➤ <span>تلغرام<small>Alerts</small></span></button><button className="mini-btn" onClick={()=>open('settings')}>⚙ <span>الإعدادات<small>Setup</small></span></button></div>
        </>:<div className="ops-alt"><h3>{opsTab==='orders'?'مراقبة الأوامر':'خيارات النظام المتقدمة'}</h3><p>{opsTab==='orders'?'المراكز الفعلية والتسوية تأتي من Core وMT5.':'تغيير وضع التنفيذ وإعدادات القناص يتم بعد تحقق Core.'}</p><button className="quick-link" onClick={()=>open(opsTab==='orders'?'positions':'trading')}>فتح الصفحة ذات الصلة ←</button></div>}
       </div>
       <div className="panel-title slim"><b>إشارات الذكاء الاصطناعي</b><button onClick={()=>open('brains')}>عرض الكل ›</button></div>
       <div className="brains-inline">{!preview&&feed.broker&&brainReport?.account_matches===true&&brainReport?.context===JSON.stringify([status?.telemetry?.server,status?.telemetry?.login,status?.telemetry?.symbol,status?.telemetry?.currency])&&array(brainReport?.brains).length?array(brainReport?.brains).slice(0,6).map((brain,i)=>{const activity=unwrap(brain.activity),orders=unwrap(brain.attributed_orders);return <button type="button" className="brain-line brain-link" key={String(unwrap(brain.config).id||i)} onClick={()=>open('brainControl')}><span className="brain-dot"/><span className="brain-name">{String(brain.name||unwrap(brain.config).id||'Brain')}</span><span className="brain-active">إشارات {displayCell(activity.signals)}</span><span className="brain-score">تنفيذ {displayCell(orders.fill_acknowledged)}</span></button>}):<div className="table-empty">{preview?'معاينة: بيانات العقول الحقيقية معطلة':'بانتظار سجل العقول الحقيقي من Core'}</div>}</div>
      </section>
     </div>
     <div className="activity-grid">
      <section className="panel positions-panel"><div className="panel-title"><b>المراكز النشطة</b><div className="microtabs">{(['all','buy','sell'] as const).map(k=><button className={filter===k?'chosen':''} key={k} onClick={()=>setFilter(k)}>{k==='all'?`الكل (${account?positions.length:'—'})`:k==='buy'?'شراء':'بيع'}</button>)}</div></div><div className="table-scroll"><table><thead><tr>{['#','الرمز','النوع','الحجم','سعر الدخول','السعر الحالي','الربح / الخسارة','الإجراء'].map(h=><th key={h}>{h}</th>)}</tr></thead><tbody>{visiblePositions.length?visiblePositions.map(p=><tr key={p.ticket}><td>{p.ticket}</td><td>{p.symbol}</td><td className={up(p.side)?'teal':'red'}>{p.side}</td><td>{num(p.volume,2)}</td><td>{num(p.price_open)}</td><td>{num(p.price_current)}</td><td className={p.profit>=0?'teal':'red'}>{signed(p.profit,currency)}</td><td><button className="row-action" onClick={()=>open('execution')}>التفاصيل ›</button></td></tr>):<tr><td colSpan={8} className="empty-row">لا توجد مراكز MT5 حالياً أو المصدر غير متاح</td></tr>}</tbody></table></div></section>
      <DashboardTradeFeed status={!preview&&feed.broker?status:null} read={dashboardRead} scope={contextKey} now={Date.now()} preview={preview} onJournal={()=>open('journal')}/>
     </div>
     <div className="bottom-grid">
      <section className="panel equity-history"><div className="panel-title"><b>أداء الحساب</b><small>رصيد • حقوق ملكية • تراجع</small></div><div className="equity-placeholder">{preview?<div className="decor-lines">╱╲╱╲╱╲╱╱╲╱╲╱</div>:<>غير متاح: تاريخ Equity الموثق غير محمّل <button onClick={()=>open('reports')}>عرض السجلات</button></>}</div></section>
      <section className="panel trade-distribution"><div className="panel-title"><b>توزيع الصفقات</b><button onClick={()=>open('journal')}>السجلات ›</button></div><div className="distribution-body"><div className="large-donut"><strong>{preview?'450':brokerResults?.samples??'—'}</strong><small>{preview?'صفقة تجريبية':'Demo مغلقة'}</small></div><div className="dist-legend"><span>رابحة <b className="teal">{preview?'342':brokerResults?.wins??'—'}</b></span><span>خاسرة <b className="red">{preview?'108':brokerResults?.losses??'—'}</b></span></div></div></section>
      <DashboardExecutionQuality status={!preview&&feed.broker?status:null} read={dashboardRead} scope={contextKey} now={Date.now()} preview={preview} onExecution={()=>open('execution')}/>
      <section className="panel sys-alerts"><div className="panel-title"><b>إشعارات النظام</b><button onClick={()=>open('events')}>عرض الكل ›</button></div><div className="alerts-list"><div>{status?.message||'بانتظار حالة Rust Core...'}</div><div>{status?.telegram_status||'Telegram: غير متاح'}</div><div>{preview?'Design preview — sample signals':trustworthy?'MT5 snapshot verified':'لم يصل تحديث MT5 حديث'}</div></div></section>
      <section className="panel risk-stats"><div className="panel-title"><b>مقاييس المخاطر</b></div><div className="risk-bars">{[['Free Margin',account?.margin_free],['Drawdown',drawdown],['Position count',account?positions.length:null],['Risk limit',safeConfig?unwrap(safeConfig.risk).risk_percent:null]].map(([label,value],i)=><div className="risk-line" key={String(label)}><span>{String(label)}</span><div className="risk-meter"><i style={{width:valid(value)?`${Math.min(100,Math.abs(Number(value))*(i===0?.002:10))}%`:'0%'}}/></div><strong>{valid(value)?num(value,2):'—'}</strong></div>)}</div></section>
     </div>
    </div>:<section className="subpage">
      <div className="subpage-header"><div className="sub-heading"><div className="sub-glyph"><PageIcon size={32}/></div><div><small>{pageMeta.en.toUpperCase()} • V4 RUST CORE</small><h1>{pageMeta.title}</h1><p>صفحة تفاعلية مرتبطة بمحرك V4، والبيانات تُعرض فقط إذا وصلت من مصدر فعلي.</p></div></div><ConnectionLine connectedNow={trustworthy} preview={preview}/></div>
      <div className="subpage-stats"><article><small>EQUITY</small><strong>{money(account?.equity,currency)}</strong><span>رأس المال في الحساب</span></article><article><small>DAILY P&L</small><strong className="teal">{signed(account?.today_net,currency)}</strong><span>نتيجة جلسة MT5</span></article><article><small>POSITIONS</small><strong>{account?positions.length:'—'}</strong><span>المراكز المفتوحة</span></article><article><small>ACCOUNT</small><strong>{account?.demo?'DEMO':account?'REAL':'OFFLINE'}</strong><span>حالة الاتصال بالوسيط</span></article></div>
      <div className="subpage-layout">
      <section className="panel subpage-primary"><div className="panel-title"><b>{pageMeta.title} — بيانات وتشغيل</b><button onClick={()=>{if(pageMeta.route)refreshCurrentPanel(pageMeta.route)}} disabled={!pageMeta.route||preview||page==='journal'}>تحديث ↻</button></div>
        {page==='risk'?<div className="feature-zone">
          <MicroCapitalOperations status={!preview&&feed.broker?status:null} key={contextKey} risk={safeConfig?unwrap(safeConfig.risk):null}
             monitor={safeRiskSnapshot} portfolio={unwrap(portfolioReport).portfolio} armed={status?.requested_run===true}
             disabled={busy||preview} onSavePortfolio={runSavePortfolio}/>
          <RiskEditor key={contextKey} config={safeConfig?unwrap(safeConfig.risk):null} apply={saveRiskForm} loading={busy} armed={status?.requested_run===true}/>
          <DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/risk/monitor')}/>
         </div>:
        page==='settings'||page==='telegram'?<div className="editor-panel"><p>إعدادات V4 المحفوظة محلياً لدى Rust Core — لا تُعرض بيانات Telegram السرية.</p>
           <label>مسار MT5 Terminal64.exe<input value={String(activeSettingsEdit?.mt5_path||'')} disabled={!activeSettingsEdit||busy||preview} onChange={e=>setSettingsEdit(s=>({...s,mt5_path:e.target.value}))}/></label>
           <label>رمز التداول<input value={String(activeSettingsEdit?.symbol||'')} disabled={!activeSettingsEdit||busy||preview} onChange={e=>setSettingsEdit(s=>({...s,symbol:e.target.value}))}/></label>
           <label>Telegram Chat ID<input value={String(activeSettingsEdit?.telegram_chat_id||'')} disabled={!activeSettingsEdit||busy||preview} onChange={e=>setSettingsEdit(s=>({...s,telegram_chat_id:e.target.value}))}/></label>
           <label>Telegram User ID<input value={String(activeSettingsEdit?.telegram_user_id||'')} disabled={!activeSettingsEdit||busy||preview} onChange={e=>setSettingsEdit(s=>({...s,telegram_user_id:e.target.value}))}/></label>
           <label>Bot token (اكتب فقط عند الاستبدال)<input type="password" value={activeTelegramToken} disabled={!activeSettingsEdit||busy||preview} autoComplete="new-password" onChange={e=>{setTelegramDraftScope(contextKey);setTelegramToken(e.target.value)}}/></label>
           <label className="check-label"><input type="checkbox" checked={Boolean(activeSettingsEdit?.telegram_enabled)} disabled={!activeSettingsEdit||busy||preview} onChange={e=>setSettingsEdit(s=>({...s,telegram_enabled:e.target.checked}))}/>تفعيل تنبيهات Telegram</label>
           <label className="check-label"><input type="checkbox" disabled={!activeSettingsEdit||busy||preview} checked={activeClearTelegram} onChange={e=>{setTelegramDraftScope(contextKey);setClearTelegram(e.target.checked)}}/>حذف رمز Telegram المحفوظ</label>
           <button className="save-button" disabled={busy||preview||!activeSettingsEdit} onClick={()=>void saveGeneral()}>حفظ وإعادة ربط MT5</button>
          </div>:
        page==='trading'||page==='sniper'?<div className="feature-zone"><div className="live-indicator"><strong>Sniper Priority</strong><span>{display?.analysis?.regime||'UNAVAILABLE'}</span><b>{display?.analysis?.action||'WAIT'}</b></div><p>العقل المنفّذ الوحيد يختاره Core، وأي تبديل أثناء التشغيل يحتاج أوامر MT5 مُسوّاة وعدم وجود مراكز مفتوحة.</p>{page==='sniper'&&<><QuoteClockEvidence status={status} preview={preview}/><SniperTickEvidence status={!preview&&feed.broker?status:null} report={activeSniperAudit} preview={preview} onRefresh={()=>{if(preview||!hasTauri())return;const ticket=scopedGeneration.current.beginRead('sniperTickAudit');void readPanel('/v1/execution/sniper-tick-audit').then(v=>{if(scopedGeneration.current.isCurrent(ticket))setSniperAuditSlot({scope:viewScope,report:unwrap(v)})}).catch(()=>{if(scopedGeneration.current.isCurrent(ticket))setSniperAuditSlot({scope:viewScope,report:null})})}}/><SniperFrozenOperations status={!preview&&feed.broker?status:null} page={pageData} report={activeSniperFrozen} preview={preview} disabled={busy} onAction={runFrozenSniper} onRefresh={()=>{if(preview||!hasTauri())return;const ticket=scopedGeneration.current.beginRead('sniperFrozen');void readPanel('/v1/execution/frozen-sniper').then(v=>{if(scopedGeneration.current.isCurrent(ticket))setSniperFrozenSlot({scope:viewScope,report:unwrap(v)})}).catch(()=>{if(scopedGeneration.current.isCurrent(ticket))setSniperFrozenSlot({scope:viewScope,report:null})})}}/><SniperClassicOperations status={!preview&&feed.broker?status:null} page={pageData} report={activeClassicFrozen} preview={preview} disabled={busy} onAction={runFrozenClassic} onRefresh={()=>{if(preview||!hasTauri())return;const ticket=scopedGeneration.current.beginRead('classicFrozen');void readPanel('/v1/execution/frozen-classic').then(v=>{if(scopedGeneration.current.isCurrent(ticket))setClassicFrozenSlot({scope:viewScope,report:unwrap(v)})}).catch(()=>{if(scopedGeneration.current.isCurrent(ticket))setClassicFrozenSlot({scope:viewScope,report:null})})}}/></>}<TradingSettingsOperations source={pageData} key={viewScope} disabled={busy||preview} armed={status?.requested_run===true} apply={runSaveTrading}/><div className="quick-ops"><button disabled={busy||preview} onClick={()=>void runHandoff(true)}>تفعيل أولوية القناص</button><button disabled={busy||preview} onClick={()=>void runHandoff(false)}>تحويل التنفيذ للبديل</button></div><div className="safe-note">هذه الأزرار تستدعي قواعد التحويل الفعلية في Rust؛ لا يوجد تجاوز لفحوصات المحرك.</div><EnterpriseSupervisor preview={preview} key={contextKey} armed={status?.requested_run===true} onSave={runSaveSupervisor}/><Table rows={rows}/></div>:
        page==='execution'?<div className="feature-zone"><ExecutionBindingOperations source={pageData} key={viewScope} telemetry={status?.telemetry??null} ready={!preview&&trustworthy} armed={status?.requested_run===true} disabled={busy||preview} onBind={runBrokerBind}/>
        <ExecutionQualityOperations status={!preview&&feed.broker?status:null} report={executionQuality} disabled={preview||busy}
          onRefresh={()=>{if(preview||!hasTauri())return;const ticket=scopedGeneration.current.beginRead('executionQuality');void readPanel('/v1/execution/quality').then(v=>{if(scopedGeneration.current.isCurrent(ticket))setExecutionQuality(unwrap(v))}).catch(()=>{if(scopedGeneration.current.isCurrent(ticket))setExecutionQuality(null)})}}/>
        <ModeEvidenceOperations status={!preview&&feed.broker?status:null} report={modeEvidence} disabled={preview||busy}
          onRefresh={()=>{if(preview||!hasTauri())return;const ticket=scopedGeneration.current.beginRead('modeEvidence');void readPanel('/v1/execution/mode-evidence').then(v=>{if(scopedGeneration.current.isCurrent(ticket))setModeEvidence(unwrap(v))}).catch(()=>{if(scopedGeneration.current.isCurrent(ticket))setModeEvidence(null)})}}/>
        <p className="safe-note">لا يعني الربط أن النظام جاهز للتداول؛ START يحتاج تأكيد Core وباقي شروط المنسق والمخاطر. STOP لا يغلق المراكز.</p><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/execution')}/></div>:
        page==='backtest'?<div className="feature-zone"><BacktestOperations key={viewScope} disabled={busy||preview} active={['loading','running'].includes(String(unwrap(pageData).phase||unwrap(unwrap(pageData).job).phase||''))} onStart={r=>runBacktest('start',r)} onCancel={()=>runBacktest('cancel')}/><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/backtest')}/></div>:
        page==='backups'?<div className="feature-zone"><BackupOperations disabled={busy||preview} create={()=>runBackup('create')} exportBrain={()=>runBackup('brain_export')}/><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/backups')}/></div>:
        page==='journal'?<div className="feature-zone"><TradeJournalOperations key={viewScope} status={!preview&&feed.broker?status:null} scope={contextKey} preview={preview}/></div>:
        page==='positions'?<div className="feature-zone"><h3>مراكز MT5 الحالية</h3><Table rows={account?positions.map(p=>({...p})):[]} empty="لا توجد مراكز موثقة"/></div>:
        page==='brains'||page==='brainControl'?<div className="feature-zone"><BrainControlsOperations source={pageData} disabled={busy||preview} armed={status?.requested_run===true} onSave={runSaveBrain}/><FrozenCoordinatorOperations status={!preview&&feed.broker?status:null} source={activeFrozenCoordinator} currentMode={unwrap(pageData).trading_mode} brainContext={unwrap(pageData).context} armed={status?.requested_run===true} disabled={busy||preview} onAction={runFrozenCoordinator} onRefresh={()=>{if(preview||!hasTauri())return;const ticket=scopedGeneration.current.beginRead('frozenCoordinator');void readPanel('/v1/execution/frozen-coordinator').then(v=>{if(scopedGeneration.current.isCurrent(ticket))setFrozenSlot({scope:viewScope,report:unwrap(v)})}).catch(()=>{if(scopedGeneration.current.isCurrent(ticket))setFrozenSlot({scope:viewScope,report:null})})}}/><p className="data-description">P1-A: نسب النجاح تظهر فقط بعد 30 نتيجة مغلقة موثّقة من MT5 DEMO بعد العمولة والسواب والرسوم، ومحددة بالعقل ووضع التداول. نتائج الورق ليست صفقات وسيط؛ لا توجد ترقية تلقائية ولا اعتماد على REAL.</p><Table rows={preview?[]:array(unwrap(pageData).brains).map(b=>({brain:b.name,checks:unwrap(b.activity).checks,signals:unwrap(b.activity).signals,broker_orders:unwrap(b.attributed_orders).requested,broker_fills:unwrap(b.attributed_orders).fill_acknowledged,paper_closed:unwrap(b.paper_stats).closed,verified_demo_closes:unwrap(unwrap(b.integrity).broker_evidence).sample_count,evidence_state:unwrap(unwrap(b.integrity).broker_evidence).status==='insufficient_evidence'?'INSUFFICIENT DATA':unwrap(unwrap(b.integrity).broker_evidence).status,winrate_costed:Number.isFinite(unwrap(unwrap(b.integrity).broker_evidence).win_rate)&&unwrap(unwrap(b.integrity).broker_evidence).win_rate!==null?((Number(unwrap(unwrap(b.integrity).broker_evidence).win_rate)*100).toFixed(1)+'%'):'—',net_after_costs:unwrap(unwrap(b.integrity).broker_evidence).net_booked??'—'}))} empty={pageError||'لا توجد إحصائيات عقول من المحرك'}/></div>:
        <div className="feature-zone"><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>{if(pageMeta.route)refreshCurrentPanel(pageMeta.route)}}/></div>}
      </section>
      <section className="panel subpage-secondary"><div className="panel-title"><b>حالة المصدر والتنفيذ</b><span>{clock}</span></div>
       <div className="feature-zone"><div className={'live-indicator '+(!accountFresh?'offline':'')}><div><strong>{accountFresh?'MT5 CONNECTED':'MT5 OFFLINE'}</strong><p>{preview?'PREVIEW':feed.broker?(feed.quote?'ACCOUNT + QUOTES FRESH':'ACCOUNT CONNECTED · QUOTES STALE'):buttonLabel(status,false)}</p></div><div><span>State</span><b>{display?.state||'UNAVAILABLE'}</b></div></div>
        <div className="phase-list"><div><i/><strong>Core</strong><span>{online?'خدمة Rust متصلة':'خدمة Rust غير متاحة'}</span></div><div><i className={trustworthy?'':'warn'}/><strong>Market Data</strong><span>{preview?'معاينة تصميم فقط':quoteDiagnosis.detail}{!preview&&quoteDiagnosis.settingsRequired&&<button type="button" onClick={()=>open('settings')}>فتح إعدادات الرمز</button>}</span></div><div><i className={!preview&&display?.execution_enabled?'':'warn'}/><strong>Execution</strong><span>{preview?'معاينة فقط':display?.execution_enabled?'مسار التنفيذ مصرح':'غير جاهز للتنفيذ'}</span></div><div><i className="warn"/><strong>Source Integrity</strong><span>لا تُستبدل النتائج الفعلية بأرقام نموذجية</span></div></div>
       </div><div className="quick-ops"><button onClick={()=>open('dashboard')}>الرجوع للوحة القيادة</button><button onClick={()=>open('events')}>عرض سجل النظام</button></div>
      </section></div>
    </section>}
   </main>
   <footer className="bottom-status"><div><span className={'tiny-led '+(trustworthy?'lit':'')}></span>{preview?'DESIGN PREVIEW':trustworthy?'MT5 VERIFIED':'NO VERIFIED MARKET DATA'}　•　{display?.version||'V4'}</div><div>Updates: Tauri IPC Channel · Core authoritative</div><div className="footer-tools"><button onClick={changePreview}>{preview?'عرض LIVE':'معاينة التصميم'}</button><button title="Dark theme" onClick={()=>setTheme('dark')}><Moon size={15}/></button><button title="Smoke theme" onClick={()=>setTheme('smoke')}><Cloud size={15}/></button><button title="Light theme" onClick={()=>setTheme('light')}><Sun size={15}/></button>　{clock}</div></footer>
 </section>
 {showStopConfirm&&<div className="confirm-overlay" role="dialog" aria-modal="true" aria-label="تأكيد وقف الدخول"><div className="confirm-dialog" ref={dialogRef}><div className="confirm-title"><PauseCircle size={23}/>وقف فتح الصفقات الجديدة فقط</div><p>هذا الإجراء لا يغلق أي مركز مفتوح ولا يلغي أوامر الوسيط القائمة. سيبقى Core يدير الخروج والتسوية وSL/TP للمراكز الموجودة.</p><div className="confirm-actions"><button onClick={()=>setApproval(null)}>إلغاء</button><button onClick={confirmStop}>تأكيد وقف الدخول الجديد</button></div></div></div>}
 {showExitConfirm&&<div className="confirm-overlay" role="dialog" aria-modal="true" aria-label="خروج كامل آمن"><div className="confirm-dialog" ref={dialogRef}><div className="confirm-title"><ShieldCheck size={23}/>الخروج الكامل الآمن</div><p>يطلب إطفاء Core والواجهة. سيُرفض إذا كان التشغيل مستمراً، أو إذا وُجد مركز مفتوح أو أمر معلّق أو كانت حالة MT5 غير موثوقة. إن أردت ترك المحرك يدير الصفقات، اضغط «إغلاق الواجهة فقط».</p><div className="confirm-actions"><button onClick={()=>setApproval(null)}>إلغاء</button><button disabled={busy} onClick={()=>void runFullExit()}>طلب الخروج الآمن</button></div></div></div>}
 {showStartConfirm&&<div className="confirm-overlay" role="dialog" aria-modal="true" aria-label="تأكيد تشغيل التداول"><div className="confirm-dialog" ref={dialogRef}><div className="confirm-title"><ShieldCheck size={23}/>تأكيد تشغيل محرك التداول</div><p>سيتم إرسال طلب التشغيل إلى Rust Core، ولن ينفذ بدون حساب MT5 صالح وفحوصات المخاطر والتحقق من الوضع.</p><p>جلسة MT5: {status?.telemetry?.demo?'DEMO':'REAL'} · {status?.telemetry?.server||'—'} · {status?.telemetry?.login??'—'}</p>
 {status?.telemetry?.demo===false&&<label>للموافقة على START في REAL، اكتب رقم الحساب بالضبط<input inputMode="numeric" autoComplete="off" value={liveLogin} onChange={e=>setLiveLogin(e.target.value)} placeholder="رقم حساب MT5 REAL" /></label>}<div className="confirm-actions"><button onClick={()=>{setApproval(null);setLiveLogin('')}}>إلغاء</button><button className="start" disabled={busy||!canConfirmStart(approval,contextKey,status,feed.broker,liveLogin)} onClick={()=>void runCommand('start',liveLogin.trim())}>إرسال أمر START إلى Core</button></div></div></div>}
 </div>
}
