import {useEffect,useMemo,useRef,useState} from 'react';
import {Activity,BarChart3,BookOpen,BrainCircuit,CalendarClock,ChartCandlestick,CheckCircle2,CircleHelp,Database,FileText,History,Home,Layers3,MessageCircle,PauseCircle,Play,Radio,RefreshCw,Settings,ShieldCheck,Siren,Target,Workflow,Zap,Wifi,WifiOff,SlidersHorizontal,Server,RotateCcw,LineChart,Clock4,BriefcaseBusiness,NotebookTabs,Eye,Sun,Cloud, Moon} from 'lucide-react';
import TradingChart from './chart';
import {PanelReadFence} from './panel-guard';
import SystemHealth,{buildHealth} from './system-health';
import EnterpriseSupervisor from './enterprise-supervisor';
import DataExplorer from './data-explorer';
import {BacktestOperations,BackupOperations,TradingSettingsOperations,BrainControlsOperations,ExecutionBindingOperations} from './operations';
import {feedEvidence} from './feed';
import {brokerEvidence} from './brain-evidence';
import {ScopedReadFence} from './scoped-read-fence';
import {MicroCapitalOperations} from './micro-capital-operations';
import {ExecutionQualityOperations} from './execution-quality-operations';
import {validatePortfolio} from './micro-capital';
import type {PageId,Status,Theme,Position} from './types';
import {DESIGN_FIXTURE} from './mock';
import {hasTauri,subscribe,coreStatus,control,readPanel,saveRisk,savePortfolio,saveSettings,handoff,shutdownCore,closeTerminal,backupAction,backtestAction,saveTrading,updateBrainControl,bindExecution} from './bridge';
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
 const [pageData,setPageData]=useState<unknown>(null);const [pageError,setPageError]=useState('');const [pageUpdatedAt,setPageUpdatedAt]=useState<number|null>(null);
 const [config,setConfig]=useState<Record<string,unknown>|null>(null);const [riskSnapshot,setRiskSnapshot]=useState<Record<string,unknown>|null>(null);
 const [learning,setLearning]=useState<Record<string,unknown>|null>(null);const [brainReport,setBrainReport]=useState<Record<string,unknown>|null>(null);const [portfolioReport,setPortfolioReport]=useState<Record<string,unknown>|null>(null);const [executionQuality,setExecutionQuality]=useState<Record<string,unknown>|null>(null);const [filter,setFilter]=useState<'all'|'buy'|'sell'>('all');const [opsTab,setOpsTab]=useState<'trade'|'orders'|'advanced'>('trade');
 const [clock,setClock]=useState('');const [lastStatusAt,setLastStatusAt]=useState<number|null>(null);const [liveLogin,setLiveLogin]=useState('');const [showStartConfirm,setShowStartConfirm]=useState(false);
 const [settingsEdit,setSettingsEdit]=useState<Record<string,unknown>|null>(null);const [telegramToken,setTelegramToken]=useState('');const [clearTelegram,setClearTelegram]=useState(false);
 const [showStopConfirm,setShowStopConfirm]=useState(false);const [showExitConfirm,setShowExitConfirm]=useState(false);
 const [allOpen,setAllOpen]=useState(false);
 const contextKey=`${status?.telemetry?.login??'unknown'}:${status?.telemetry?.server??'offline'}:${status?.telemetry?.symbol??'none'}:${status?.telemetry?.currency??'unknown'}:${status?.telemetry?.account_trade_mode??'unknown'}:${status?.telemetry?.demo??'unknown'}:${online}:${status?.gateway_fresh??false}:${status?.telemetry?.connected??false}`;
 // Any page, broker-account or preview transition revokes unresolved manual reads.
 const viewGeneration=useRef(new PanelReadFence());
 const scopedGeneration=useRef(new ScopedReadFence());
 scopedGeneration.current.setScope(contextKey);
 const viewScope=JSON.stringify([page,contextKey,preview]);
 viewGeneration.current.setScope(viewScope);
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
  void get('config','/v1/config',r=>{setConfig(r?unwrap(r.config):null);setSettingsEdit(r?unwrap(r.config):null)});
  void get('risk','/v1/risk/monitor',setRiskSnapshot);
  void get('learning','/v1/learning',setLearning);
  void get('brains','/v1/execution/brains',setBrainReport);
  const t=setInterval(()=>{
    if(document.visibilityState==='visible'){
      void get('risk','/v1/risk/monitor',setRiskSnapshot);
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
 const display=preview?DESIGN_FIXTURE:status;
 // Broker truth requires BOTH authenticated Core status and fresh MT5 data.
 const t=display?.telemetry;const feed=feedEvidence(status,online,lastStatusAt,Date.now());const trustworthy=preview||feed.quote;
 const account=trustworthy?t:null;const currency=account?.currency||'USD';const positions=account?.positions||[];
 const buys=positions.filter(p=>up(p.side)).length;const sells=positions.length-buys;
 const brokerResults=preview||!feed.broker?null:brokerEvidence(brainReport,status);
 const win=preview?76:brokerResults?.winRate??null;
 // Core daily-loss estimate is NOT verified maximum drawdown.
 const drawdown=preview?4.2:null;
 const showLatency=preview?28:null; // quote age is NOT execution latency
 const visiblePositions=positions.filter(p=>filter==='all'||(filter==='buy'?up(p.side):!up(p.side)));
 const runCommand=async(action:'start'|'stop'|'reconnect',login?:string)=>{
  if(preview){sendNotice('هذه معاينة تصميم فقط. لا يتم تنفيذ أوامر في MT5.',true);return}
  if(!hasTauri()){sendNotice('أوامر التداول تعمل من تطبيق ويندوز Tauri فقط.',true);return}
  setBusy(true);
  try{await control(action,login);sendNotice(action==='start'?'تم إرسال طلب START إلى Core':'تم إرسال الأمر إلى Core. يتم تحديث الحالة من قناة MT5.');setShowStartConfirm(false);setLiveLogin('')}catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const requestStart=()=>{if(preview){sendNotice('لا يمكن بدء التداول في وضع المعاينة',true);return}setShowStartConfirm(true)};
 const runFullExit=async()=>{
  if(preview||!hasTauri()){sendNotice('الخروج الكامل يعمل حصراً داخل تطبيق ويندوز',true);return}
  setBusy(true);
  try{await shutdownCore();setShowExitConfirm(false);await closeTerminal();}
  catch(e){sendNotice('رفض Core الخروج الكامل: '+String(e),true);setShowExitConfirm(false)}
  finally{setBusy(false)}
 };
 const runBackup=async(action:'create'|'brain_export')=>{
  if(preview||!hasTauri())return sendNotice('إنشاء النسخ يتطلب Core الحقيقي',true);
  const serial=panelSerial();setBusy(true);
  try{const outcome=unwrap(await backupAction(action));sendNotice(action==='create'?`تم حفظ نسخة محلية: ${outcome.name||outcome.path||'تم التوثيق'}`:`تصدير العقول: ${outcome.file_name||'تم'} — راجع مجلد البيانات`);publishPanel(serial,await readPanel('/v1/backups'))}
  catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const runBacktest=async(action:'start'|'cancel',request?:{from_date:string;to_date:string;initial_equity:number;spread_floor:number})=>{
  if(preview||!hasTauri())return sendNotice('الاختبارات التاريخية تتطلب Core وMT5',true);
  const serial=panelSerial();setBusy(true);
  try{await backtestAction(action,request);sendNotice(action==='start'?'بدأت مهمة التاريخ في Core؛ راقب تقدمها أدناه':'أُلغي الاختبار في Core');publishPanel(serial,await readPanel('/v1/backtest'))}
  catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const changePreview=()=>{setPreview(v=>!v);setPage('dashboard');sendNotice('وضع المعاينة منفصل تماماً عن أوامر التداول')};
 const open=(next:PageId)=>{setPage(next);setNotice('');setError('')};
 const saveRiskForm=async(risk:Record<string,unknown>)=>{
  if(!hasTauri()||preview){sendNotice('الإعدادات لا تُحفظ في وضع المعاينة',true);return}
  setBusy(true);const writeTicket=scopedGeneration.current.beginRead('config');try{await saveRisk(risk);const latest=unwrap(await readPanel('/v1/config'));if(scopedGeneration.current.isCurrent(writeTicket)){scopedGeneration.current.invalidate('risk');setConfig(unwrap(latest.config));setRiskSnapshot(null);}sendNotice('تم حفظ إعدادات المخاطر والتحقق من النسخة المسجلة في Rust Core');}catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const runSavePortfolio=async(input:Record<string,unknown>)=>{
  if(preview||!hasTauri()||status?.requested_run){sendNotice('حدود المحفظة تحتاج STOP وتطبيق Windows الحقيقي',true);return;}
  setBusy(true);
  const ticket=scopedGeneration.current.beginRead('portfolio');
  try{
   await savePortfolio(validatePortfolio(input));
   const updated=unwrap(await readPanel('/v1/execution'));
   if(scopedGeneration.current.isCurrent(ticket))setPortfolioReport(updated);
   sendNotice('تم حفظ حدود المحفظة الاختيارية في Rust Core');
  }catch(e){sendNotice('تعذر حفظ حدود المحفظة: '+String(e),true)}
  finally{setBusy(false);}
 };
 const saveGeneral=async()=>{
  if(!settingsEdit||preview){sendNotice('وضع المعاينة لا يدعم الحفظ',true);return}
  setBusy(true);const writeTicket=scopedGeneration.current.beginRead('config');try{await saveSettings(settingsEdit,telegramToken,clearTelegram);const latest=unwrap(await readPanel('/v1/config'));if(scopedGeneration.current.isCurrent(writeTicket)){scopedGeneration.current.invalidate('risk');setConfig(unwrap(latest.config));setSettingsEdit(unwrap(latest.config));setRiskSnapshot(null);setTelegramToken('');setClearTelegram(false);}sendNotice('تم حفظ الإعدادات لدى Core. إعادة الاتصال قيد التنفيذ.');}catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const runHandoff=async(active:boolean)=>{
  if(preview){sendNotice('التحويل الحقيقي غير متاح في المعاينة',true);return}
  setBusy(true);try{await handoff(active);scopedGeneration.current.invalidate('brains');setBrainReport(null);void refreshBrainEvidence();sendNotice('تحويل أولوية القناص تم تأكيده من Core');}catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const runSaveTrading=async(input:Record<string,unknown>)=>{
  if(preview||!hasTauri()){sendNotice('حفظ التكاليف يحتاج Core الأصلي',true);return}
  const serial=panelSerial();setBusy(true);
  try{
   await saveTrading(input);
   const latest=unwrap(await readPanel('/v1/trading/settings'));
   const full=unwrap(latest.config);
   publishPanel(serial,latest);if(viewGeneration.current.isCurrent(serial)){scopedGeneration.current.invalidate('config');scopedGeneration.current.invalidate('brains');setConfig(full);setBrainReport(null);void refreshBrainEvidence();}
   sendNotice('تم تأكيد وحفظ وضع المنسق والعمولة من Rust Core');
  }catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const runSaveBrain=async(brain:string,advisor:boolean,paper:boolean)=>{
  if(preview||!hasTauri()||!status?.gateway_fresh||!status.telemetry||status.requested_run){sendNotice('أوقف التداول وانتظر اتصال MT5 حديث لإدارة العقول',true);return}
  const t=status.telemetry;
  if(t.account_trade_mode!==(t.demo?0:2)){sendNotice('نوع حساب MT5 غير مؤكّد؛ لا يمكن تغيير دور العقل',true);return}
  const context=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
  const savedContext=String(unwrap(pageData).context||'');
  if(savedContext!==context){sendNotice('تغيّر الحساب؛ انتظر تحميل صفحة العقول للحساب الحالي',true);return}
  const mode=String(unwrap(pageData).trading_mode||'');
  const serial=panelSerial();setBusy(true);
  try{await updateBrainControl(context,brain,mode,advisor,paper);
   scopedGeneration.current.invalidate('brains');setBrainReport(null);
   const ticket=scopedGeneration.current.beginRead('brains');
   const refreshed=unwrap(await readPanel('/v1/execution/brains'));
   publishPanel(serial,refreshed);
   if(scopedGeneration.current.isCurrent(ticket))setBrainReport(refreshed);
   sendNotice('تم حفظ مشاركة العقل في Core للحساب والوضع الحاليين');
  }catch(e){sendNotice(String(e),true)}finally{setBusy(false)}
 };
 const runBrokerBind=async(target:'demo'|'live',deviationPoints:number,confirmLiveLogin:string)=>{
  if(preview||!hasTauri()||status?.requested_run){sendNotice('اربط حساب التنفيذ فقط بعد STOP ومن تطبيق Tauri الحقيقي',true);return}
  const serial=panelSerial();setBusy(true);
  try{
   await bindExecution(target,deviationPoints,confirmLiveLogin);
   scopedGeneration.current.invalidate('brains');setBrainReport(null);
   publishPanel(serial,await readPanel('/v1/execution'));
   void refreshBrainEvidence();
   sendNotice('تم التحقق من حساب '+target.toUpperCase()+' وربط التنفيذ بواسطة Core؛ الربط وحده لا يفتح صفقة');
  }catch(e){sendNotice('فشل ربط حساب التنفيذ: '+String(e),true)}finally{setBusy(false)}
 };
 const health=buildHealth(status,online,preview,Boolean(config?.risk)&&Boolean(riskSnapshot),lastStatusAt,Date.now());
 const pageMeta=findPage(page),PageIcon=pageMeta.icon;
 const rows=useMemo(()=>flattenRows(pageData),[pageData]);
 return <div className={'shell terminal-v15 '+(compact?'compact-nav':'')}>
 <aside className="sidebar"><div className="brand"><div className="brand-logo">BRAIN<span>TRADE</span> <b>V4</b></div><div className="brand-desc">Institutional Trading System</div></div>
  <nav id="sidebarNav" aria-label="قائمة صفحات BRAINTRADE">
   {PAGES.filter((x,i)=>allOpen||i<11).map(x=>{const Icon=x.icon;return <button type="button" key={x.id} title={x.title} className={'nav-item '+(page===x.id?'selected':'')} onClick={()=>open(x.id)}><span className="nav-glyph"><Icon size={21} strokeWidth={1.9}/></span><span>{x.title}</span></button>})}
   <button className="nav-item all-pages" onClick={()=>setAllOpen(v=>!v)}><Layers3 size={19}/><span>{allOpen?'إخفاء الصفحات الإضافية':'كل الصفحات الـ22'}</span></button>
  </nav>
  <SystemHealth health={health} onOpen={open}/>
  <div className="sidebar-footer"><button type="button" className="safe-exit-button" onClick={()=>setShowExitConfirm(true)} disabled={busy||preview}>خروج كامل آمن</button><button type="button" className="safe-exit-button" onClick={()=>void closeTerminal()} disabled={!hasTauri()}>إغلاق الواجهة فقط</button><strong>BRAIN<span>TRADE</span> V4</strong><small>Tauri 2 + React + Rust</small><button className="compact-toggle" onClick={()=>setCompact(v=>!v)}>{compact?'توسيع القائمة':'طي القائمة'}</button></div>
 </aside>
 <section className="main">
   <header className="account-strip">
    <div className="account-item broker-select"><label>Broker</label><button className="select-look" onClick={()=>open('settings')}><span className="broker-mark">MT5</span><strong>{account?.connected?'Connected broker':'Not connected'}</strong><span>⌄</span></button></div>
    <div className="account-item server-select"><label>Server</label><button className="select-look" onClick={()=>open('execution')}><strong>{account?.server||'—'}</strong><span>⌄</span></button></div>
    <div className="account-item account-switch"><label>Account</label><div className="switch-buttons"><button className={account&&!account.demo?'active':''} onClick={()=>open('execution')}>Live</button><button className={account?.demo?'active':''} onClick={()=>open('execution')}>Demo</button></div></div>
    {[['Equity',money(account?.equity,currency)],['Balance',money(account?.balance,currency)],['Margin','—'],['Free Margin',money(account?.margin_free,currency)],['Daily P&L',signed(account?.today_net,currency)],['Quote Age',preview?'28 ms':feed.quote?num(feed.quoteAgeMs,0)+' ms':'—']].map(([label,value])=><div key={label} className={'top-stat '+(label==='Daily P&L'?'daily':'')}><label>{label}</label><b>{value}</b></div>)}
    <button className="connection-pill" onClick={()=>{if((status?.requested_run||positions.length>0)&&!window.confirm('ستتم إعادة تشغيل جسر MT5. لن تُغلق الصفقات المفتوحة، لكن إدارة خروجها قد تتأخر أثناء الاتصال. تابع فقط عند الضرورة.'))return;void runCommand('reconnect')}} disabled={busy||preview}><span className={'pulse-dot '+(trustworthy?'online':'')}></span><span>{preview?'معاينة':trustworthy?'متصل':'غير متصل'}</span><RefreshCw size={16}/></button>
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
        <div className="action-row double"><button className="action-btn start" onClick={requestStart} disabled={busy||preview}><Play size={23}/><span>بدء التداول<small>Start</small></span></button><button className="action-btn stop" onClick={()=>void setShowStopConfirm(true)} disabled={busy||preview}><PauseCircle size={23}/><span>وقف الدخول<small>Stop entries</small></span></button><button className="action-btn shadow" onClick={()=>open('shadow')}><Eye size={21}/><span>ظل<small>Shadow</small></span></button></div>
        <div className="action-row triple"><button className="pill-btn" onClick={()=>open('execution')}>▣ <span>Demo</span></button><button className="pill-btn green" onClick={()=>open('execution')}>◉ <span>Live</span></button><button className="pill-btn slate" onClick={()=>open('learning')}>⚙ <span>التعلّم<small>Learning</small></span></button></div>
        <div className="action-row quadruple"><button className="mini-btn risk" onClick={()=>open('risk')}>◉ <span>المخاطر<small>Risk</small></span></button><button className="mini-btn risk-off" onClick={()=>open('risk')}>◌ <span>Limits<small>Safety</small></span></button><button className="mini-btn telegram" onClick={()=>open('telegram')}>➤ <span>تلغرام<small>Alerts</small></span></button><button className="mini-btn" onClick={()=>open('settings')}>⚙ <span>الإعدادات<small>Setup</small></span></button></div>
        </>:<div className="ops-alt"><h3>{opsTab==='orders'?'مراقبة الأوامر':'خيارات النظام المتقدمة'}</h3><p>{opsTab==='orders'?'المراكز الفعلية والتسوية تأتي من Core وMT5.':'تغيير وضع التنفيذ وإعدادات القناص يتم بعد تحقق Core.'}</p><button className="quick-link" onClick={()=>open(opsTab==='orders'?'positions':'trading')}>فتح الصفحة ذات الصلة ←</button></div>}
       </div>
       <div className="panel-title slim"><b>إشارات الذكاء الاصطناعي</b><button onClick={()=>open('brains')}>عرض الكل ›</button></div>
       <div className="brains-inline">{!preview&&feed.broker&&brainReport?.account_matches===true&&brainReport?.context===JSON.stringify([status?.telemetry?.server,status?.telemetry?.login,status?.telemetry?.symbol,status?.telemetry?.currency])&&array(brainReport?.brains).length?array(brainReport?.brains).slice(0,6).map((brain,i)=>{const activity=unwrap(brain.activity),orders=unwrap(brain.attributed_orders);return <button type="button" className="brain-line brain-link" key={String(unwrap(brain.config).id||i)} onClick={()=>open('brainControl')}><span className="brain-dot"/><span className="brain-name">{String(brain.name||unwrap(brain.config).id||'Brain')}</span><span className="brain-active">إشارات {displayCell(activity.signals)}</span><span className="brain-score">تنفيذ {displayCell(orders.fill_acknowledged)}</span></button>}):<div className="table-empty">{preview?'معاينة: بيانات العقول الحقيقية معطلة':'بانتظار سجل العقول الحقيقي من Core'}</div>}</div>
      </section>
     </div>
     <div className="activity-grid">
      <section className="panel positions-panel"><div className="panel-title"><b>المراكز النشطة</b><div className="microtabs">{(['all','buy','sell'] as const).map(k=><button className={filter===k?'chosen':''} key={k} onClick={()=>setFilter(k)}>{k==='all'?`الكل (${account?positions.length:'—'})`:k==='buy'?'شراء':'بيع'}</button>)}</div></div><div className="table-scroll"><table><thead><tr>{['#','الرمز','النوع','الحجم','سعر الدخول','السعر الحالي','الربح / الخسارة','الإجراء'].map(h=><th key={h}>{h}</th>)}</tr></thead><tbody>{visiblePositions.length?visiblePositions.map(p=><tr key={p.ticket}><td>{p.ticket}</td><td>{p.symbol}</td><td className={up(p.side)?'teal':'red'}>{p.side}</td><td>{num(p.volume,2)}</td><td>{num(p.price_open)}</td><td>{num(p.price_current)}</td><td className={p.profit>=0?'teal':'red'}>{signed(p.profit,currency)}</td><td><button className="row-action" onClick={()=>open('execution')}>التفاصيل ›</button></td></tr>):<tr><td colSpan={8} className="empty-row">لا توجد مراكز MT5 حالياً أو المصدر غير متاح</td></tr>}</tbody></table></div></section>
      <section className="panel trades-panel"><div className="panel-title"><b>أحدث الصفقات المنفذة</b><button onClick={()=>open('journal')}>عرض الكل ›</button></div><div className="preview-list">{preview?<><div>17:24:11　 XAUUSD　 Buy　　 0.50</div><div>17:16:30　 GBPUSD　 Sell　　 1.00</div><div>17:11:25　 NAS100　 Buy　　 0.20</div><div className="preview-tag">DEMONSTRATION ONLY</div></>:<div className="table-empty">الصفقات التاريخية تُقرأ من سجل Core في صفحة السجلات.</div>}</div></section>
     </div>
     <div className="bottom-grid">
      <section className="panel equity-history"><div className="panel-title"><b>أداء الحساب</b><small>رصيد • حقوق ملكية • تراجع</small></div><div className="equity-placeholder">{preview?<div className="decor-lines">╱╲╱╲╱╲╱╱╲╱╲╱</div>:<>غير متاح: تاريخ Equity الموثق غير محمّل <button onClick={()=>open('reports')}>عرض السجلات</button></>}</div></section>
      <section className="panel trade-distribution"><div className="panel-title"><b>توزيع الصفقات</b><button onClick={()=>open('journal')}>السجلات ›</button></div><div className="distribution-body"><div className="large-donut"><strong>{preview?'450':brokerResults?.samples??'—'}</strong><small>{preview?'صفقة تجريبية':'Demo مغلقة'}</small></div><div className="dist-legend"><span>رابحة <b className="teal">{preview?'342':brokerResults?.wins??'—'}</b></span><span>خاسرة <b className="red">{preview?'108':brokerResults?.losses??'—'}</b></span></div></div></section>
      <section className="panel execution-quality"><div className="panel-title"><b>جودة التنفيذ</b><button onClick={()=>open('execution')}>المراقبة ›</button></div><div className="quality-list">{[['الانزلاق السعري',preview?'0.12 pips':'—'],['الاستجابة',preview?'98.7%':'—'],['التنفيذ المقبول',preview?'2.1%':'—'],['الأوامر المرفوضة',preview?'0.8%':'—']].map(([label,value])=><div className="quality-row" key={label}><span>{label}</span><div className="quality-meter"><i style={{width:preview?'63%':'0%'}}/></div><small>{value}</small></div>)}</div></section>
      <section className="panel sys-alerts"><div className="panel-title"><b>إشعارات النظام</b><button onClick={()=>open('events')}>عرض الكل ›</button></div><div className="alerts-list"><div>{status?.message||'بانتظار حالة Rust Core...'}</div><div>{status?.telegram_status||'Telegram: غير متاح'}</div><div>{preview?'Design preview — sample signals':trustworthy?'MT5 snapshot verified':'لم يصل تحديث MT5 حديث'}</div></div></section>
      <section className="panel risk-stats"><div className="panel-title"><b>مقاييس المخاطر</b></div><div className="risk-bars">{[['Free Margin',account?.margin_free],['Drawdown',drawdown],['Position count',account?positions.length:null],['Risk limit',config?unwrap(config.risk).risk_percent:null]].map(([label,value],i)=><div className="risk-line" key={String(label)}><span>{String(label)}</span><div className="risk-meter"><i style={{width:valid(value)?`${Math.min(100,Math.abs(Number(value))*(i===0?.002:10))}%`:'0%'}}/></div><strong>{valid(value)?num(value,2):'—'}</strong></div>)}</div></section>
     </div>
    </div>:<section className="subpage">
      <div className="subpage-header"><div className="sub-heading"><div className="sub-glyph"><PageIcon size={32}/></div><div><small>{pageMeta.en.toUpperCase()} • V4 RUST CORE</small><h1>{pageMeta.title}</h1><p>صفحة تفاعلية مرتبطة بمحرك V4، والبيانات تُعرض فقط إذا وصلت من مصدر فعلي.</p></div></div><ConnectionLine connectedNow={trustworthy} preview={preview}/></div>
      <div className="subpage-stats"><article><small>EQUITY</small><strong>{money(account?.equity,currency)}</strong><span>رأس المال في الحساب</span></article><article><small>DAILY P&L</small><strong className="teal">{signed(account?.today_net,currency)}</strong><span>نتيجة جلسة MT5</span></article><article><small>POSITIONS</small><strong>{account?positions.length:'—'}</strong><span>المراكز المفتوحة</span></article><article><small>ACCOUNT</small><strong>{account?.demo?'DEMO':account?'REAL':'OFFLINE'}</strong><span>حالة الاتصال بالوسيط</span></article></div>
      <div className="subpage-layout">
      <section className="panel subpage-primary"><div className="panel-title"><b>{pageMeta.title} — بيانات وتشغيل</b><button onClick={()=>{if(pageMeta.route)refreshCurrentPanel(pageMeta.route)}} disabled={!pageMeta.route||preview}>تحديث ↻</button></div>
        {page==='risk'?<div className="feature-zone">
          <MicroCapitalOperations status={!preview&&feed.broker?status:null} risk={config?unwrap(config.risk):null}
             monitor={riskSnapshot} portfolio={unwrap(portfolioReport).portfolio} armed={status?.requested_run===true}
             disabled={busy||preview} onSavePortfolio={runSavePortfolio}/>
          <RiskEditor config={config?unwrap(config.risk):null} apply={saveRiskForm} loading={busy} armed={status?.requested_run===true}/>
          <DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/risk/monitor')}/>
         </div>:
        page==='settings'||page==='telegram'?<div className="editor-panel"><p>إعدادات V4 المحفوظة محلياً لدى Rust Core — لا تُعرض بيانات Telegram السرية.</p>
           <label>مسار MT5 Terminal64.exe<input value={String(settingsEdit?.mt5_path||'')} onChange={e=>setSettingsEdit(s=>({...s,mt5_path:e.target.value}))}/></label>
           <label>رمز التداول<input value={String(settingsEdit?.symbol||'XAUUSD')} onChange={e=>setSettingsEdit(s=>({...s,symbol:e.target.value}))}/></label>
           <label>Telegram Chat ID<input value={String(settingsEdit?.telegram_chat_id||'')} onChange={e=>setSettingsEdit(s=>({...s,telegram_chat_id:e.target.value}))}/></label>
           <label>Telegram User ID<input value={String(settingsEdit?.telegram_user_id||'')} onChange={e=>setSettingsEdit(s=>({...s,telegram_user_id:e.target.value}))}/></label>
           <label>Bot token (اكتب فقط عند الاستبدال)<input type="password" value={telegramToken} autoComplete="new-password" onChange={e=>setTelegramToken(e.target.value)}/></label>
           <label className="check-label"><input type="checkbox" checked={Boolean(settingsEdit?.telegram_enabled)} onChange={e=>setSettingsEdit(s=>({...s,telegram_enabled:e.target.checked}))}/>تفعيل تنبيهات Telegram</label>
           <label className="check-label"><input type="checkbox" checked={clearTelegram} onChange={e=>setClearTelegram(e.target.checked)}/>حذف رمز Telegram المحفوظ</label>
           <button className="save-button" disabled={busy||preview||!settingsEdit} onClick={()=>void saveGeneral()}>حفظ وإعادة ربط MT5</button>
          </div>:
        page==='trading'||page==='sniper'?<div className="feature-zone"><div className="live-indicator"><strong>Sniper Priority</strong><span>{display?.analysis?.regime||'UNAVAILABLE'}</span><b>{display?.analysis?.action||'WAIT'}</b></div><p>العقل المنفّذ الوحيد يختاره Core، وأي تبديل أثناء التشغيل يحتاج أوامر MT5 مُسوّاة وعدم وجود مراكز مفتوحة.</p><TradingSettingsOperations source={pageData} disabled={busy||preview} armed={status?.requested_run===true} apply={runSaveTrading}/><div className="quick-ops"><button disabled={busy||preview} onClick={()=>void runHandoff(true)}>تفعيل أولوية القناص</button><button disabled={busy||preview} onClick={()=>void runHandoff(false)}>تحويل التنفيذ للبديل</button></div><div className="safe-note">هذه الأزرار تستدعي قواعد التحويل الفعلية في Rust؛ لا يوجد تجاوز لفحوصات المحرك.</div><EnterpriseSupervisor preview={preview} armed={status?.requested_run===true}/><Table rows={rows}/></div>:
        page==='execution'?<div className="feature-zone"><ExecutionBindingOperations source={pageData} telemetry={status?.telemetry??null} ready={!preview&&trustworthy} armed={status?.requested_run===true} disabled={busy||preview} onBind={runBrokerBind}/>
        <ExecutionQualityOperations status={!preview&&feed.broker?status:null} report={executionQuality} disabled={preview||busy}
          onRefresh={()=>{if(preview||!hasTauri())return;const ticket=scopedGeneration.current.beginRead('executionQuality');void readPanel('/v1/execution/quality').then(v=>{if(scopedGeneration.current.isCurrent(ticket))setExecutionQuality(unwrap(v))}).catch(()=>{if(scopedGeneration.current.isCurrent(ticket))setExecutionQuality(null)})}}/>
        <p className="safe-note">لا يعني الربط أن النظام جاهز للتداول؛ START يحتاج تأكيد Core وباقي شروط المنسق والمخاطر. STOP لا يغلق المراكز.</p><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/execution')}/></div>:
        page==='backtest'?<div className="feature-zone"><BacktestOperations disabled={busy||preview} active={['loading','running'].includes(String(unwrap(pageData).phase||unwrap(unwrap(pageData).job).phase||''))} onStart={r=>runBacktest('start',r)} onCancel={()=>runBacktest('cancel')}/><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/backtest')}/></div>:
        page==='backups'?<div className="feature-zone"><BackupOperations disabled={busy||preview} create={()=>runBackup('create')} exportBrain={()=>runBackup('brain_export')}/><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>refreshCurrentPanel('/v1/backups')}/></div>:
        page==='positions'?<div className="feature-zone"><h3>مراكز MT5 الحالية</h3><Table rows={account?positions.map(p=>({...p})):[]} empty="لا توجد مراكز موثقة"/></div>:
        page==='brains'||page==='brainControl'?<div className="feature-zone"><BrainControlsOperations source={pageData} disabled={busy||preview} armed={status?.requested_run===true} onSave={runSaveBrain}/><p className="data-description">P1-A: نسب النجاح تظهر فقط بعد 30 نتيجة مغلقة موثّقة من MT5 DEMO بعد العمولة والسواب والرسوم، ومحددة بالعقل ووضع التداول. نتائج الورق ليست صفقات وسيط؛ لا توجد ترقية تلقائية ولا اعتماد على REAL.</p><Table rows={preview?[]:array(unwrap(pageData).brains).map(b=>({brain:b.name,checks:unwrap(b.activity).checks,signals:unwrap(b.activity).signals,broker_orders:unwrap(b.attributed_orders).requested,broker_fills:unwrap(b.attributed_orders).fill_acknowledged,paper_closed:unwrap(b.paper_stats).closed,verified_demo_closes:unwrap(unwrap(b.integrity).broker_evidence).sample_count,evidence_state:unwrap(unwrap(b.integrity).broker_evidence).status==='insufficient_evidence'?'INSUFFICIENT DATA':unwrap(unwrap(b.integrity).broker_evidence).status,winrate_costed:Number.isFinite(unwrap(unwrap(b.integrity).broker_evidence).win_rate)&&unwrap(unwrap(b.integrity).broker_evidence).win_rate!==null?((Number(unwrap(unwrap(b.integrity).broker_evidence).win_rate)*100).toFixed(1)+'%'):'—',net_after_costs:unwrap(unwrap(b.integrity).broker_evidence).net_booked??'—'}))} empty={pageError||'لا توجد إحصائيات عقول من المحرك'}/></div>:
        <div className="feature-zone"><DataExplorer source={pageData} error={pageError} preview={preview} updatedAt={pageUpdatedAt} onRefresh={()=>{if(pageMeta.route)refreshCurrentPanel(pageMeta.route)}}/></div>}
      </section>
      <section className="panel subpage-secondary"><div className="panel-title"><b>حالة المصدر والتنفيذ</b><span>{clock}</span></div>
       <div className="feature-zone"><div className={'live-indicator '+(!trustworthy?'offline':'')}><div><strong>{trustworthy?'MT5 CONNECTED':'MT5 OFFLINE'}</strong><p>{buttonLabel(status,preview)}</p></div><div><span>State</span><b>{display?.state||'UNAVAILABLE'}</b></div></div>
        <div className="phase-list"><div><i/><strong>Core</strong><span>{online?'خدمة Rust متصلة':'خدمة Rust غير متاحة'}</span></div><div><i className={trustworthy?'':'warn'}/><strong>Market Data</strong><span>{trustworthy?'لقطة حديثة مستلمة':'الأسعار غير متاحة أو قديمة'}</span></div><div><i className={!preview&&display?.execution_enabled?'':'warn'}/><strong>Execution</strong><span>{preview?'معاينة فقط':display?.execution_enabled?'مسار التنفيذ مصرح':'غير جاهز للتنفيذ'}</span></div><div><i className="warn"/><strong>Source Integrity</strong><span>لا تُستبدل النتائج الفعلية بأرقام نموذجية</span></div></div>
       </div><div className="quick-ops"><button onClick={()=>open('dashboard')}>الرجوع للوحة القيادة</button><button onClick={()=>open('events')}>عرض سجل النظام</button></div>
      </section></div>
    </section>}
   </main>
   <footer className="bottom-status"><div><span className={'tiny-led '+(trustworthy?'lit':'')}></span>{preview?'DESIGN PREVIEW':trustworthy?'MT5 VERIFIED':'NO VERIFIED MARKET DATA'}　•　{display?.version||'V4'}</div><div>Updates: Tauri IPC Channel · Core authoritative</div><div className="footer-tools"><button onClick={changePreview}>{preview?'عرض LIVE':'معاينة التصميم'}</button><button title="Dark theme" onClick={()=>setTheme('dark')}><Moon size={15}/></button><button title="Smoke theme" onClick={()=>setTheme('smoke')}><Cloud size={15}/></button><button title="Light theme" onClick={()=>setTheme('light')}><Sun size={15}/></button>　{clock}</div></footer>
 </section>
 {showStopConfirm&&<div className="confirm-overlay" role="dialog" aria-modal="true" aria-label="تأكيد وقف الدخول"><div className="confirm-dialog"><div className="confirm-title"><PauseCircle size={23}/>وقف فتح الصفقات الجديدة فقط</div><p>هذا الإجراء لا يغلق أي مركز مفتوح ولا يلغي أوامر الوسيط القائمة. سيبقى Core يدير الخروج والتسوية وSL/TP للمراكز الموجودة.</p><div className="confirm-actions"><button onClick={()=>setShowStopConfirm(false)}>إلغاء</button><button disabled={busy} onClick={()=>{setShowStopConfirm(false);void runCommand('stop')}}>تأكيد وقف الدخول الجديد</button></div></div></div>}
 {showExitConfirm&&<div className="confirm-overlay" role="dialog" aria-modal="true" aria-label="خروج كامل آمن"><div className="confirm-dialog"><div className="confirm-title"><ShieldCheck size={23}/>الخروج الكامل الآمن</div><p>يطلب إطفاء Core والواجهة. سيُرفض إذا كان التشغيل مستمراً، أو إذا وُجد مركز مفتوح أو أمر معلّق أو كانت حالة MT5 غير موثوقة. إن أردت ترك المحرك يدير الصفقات، اضغط «إغلاق الواجهة فقط».</p><div className="confirm-actions"><button onClick={()=>setShowExitConfirm(false)}>إلغاء</button><button disabled={busy} onClick={()=>void runFullExit()}>طلب الخروج الآمن</button></div></div></div>}
 {showStartConfirm&&<div className="confirm-overlay"><div className="confirm-dialog"><div className="confirm-title"><ShieldCheck size={23}/>تأكيد تشغيل محرك التداول</div><p>سيتم إرسال طلب التشغيل إلى Rust Core، ولن ينفذ بدون حساب MT5 صالح وفحوصات المخاطر والتحقق من الوضع.</p><label>إذا كان الحساب REAL، اكتب رقم الحساب الحقيقي للتأكيد<input autoFocus value={liveLogin} onChange={e=>setLiveLogin(e.target.value)} placeholder="رقم حساب REAL (للحساب التجريبي اتركه فارغاً)"/></label><div className="confirm-actions"><button onClick={()=>setShowStartConfirm(false)}>إلغاء</button><button className="start" disabled={busy} onClick={()=>void runCommand('start',liveLogin.trim())}>إرسال أمر START إلى Core</button></div></div></div>}
 </div>
}
