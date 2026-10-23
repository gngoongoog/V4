'use strict';
/* BRAINTRADE NEXT visual client. All sample quotes/statistics are explicitly preview-only.
   Production data is read solely from the existing authenticated local Rust Core. */
const $=id=>document.getElementById(id);
const fmt=(n,d=2)=>typeof n==='number'&&Number.isFinite(n)?n.toLocaleString('en-US',{minimumFractionDigits:d,maximumFractionDigits:d}):'—';
const money=(n,c='USD')=>typeof n==='number'&&Number.isFinite(n)?`${n<0?'-':''}$${fmt(Math.abs(n),2)}`:'—';
const native=!!(window.chrome&&window.chrome.webview);
const nav=[
  ['dashboard','⌂','نظرة عامة'],['trading','↗','مركز التداول'],['execution','◎','وضع تلقائي'],
  ['brains','◉','العقول'],['risk','⬡','إدارة المخاطر'],['positions','ϟ','التنفيذ'],
  ['analysis','▥','التحليلات'],['journal','▤','السجلات'],['reports','➤','التقارير'],['telegram','✉','التلغرام'],['settings','⚙','الإعدادات']
];
const state={page:'dashboard',theme:'dark',preview:!native,tf:'m5',status:null,ready:false,apiError:'',connected:false,
  ma:true,crosshair:true,positionsFilter:'all',tab:'trade',lastStatusTime:0,
  sample:null,chartHover:null,modalResolve:null,pending:new Map(),id:0,busy:false,
  risk:null,history:null};
const MOCK=Object.freeze({
  sample:true,version:'4.2.1 Preview',state:'MONITORING',gateway_fresh:true,requested_run:true,execution_enabled:true,telegram_status:'connected',
  telemetry:{connected:true,login:7003124,server:'ICMarkets-Live06',currency:'USD',demo:false,balance:51200,equity:52430.18,margin_free:46109.73,today_net:1230.18,bid:2479.54,ask:2479.56,tick_age_ms:28,symbol:'XAUUSD',
    positions:[{ticket:10342,symbol:'XAUUSD',side:'Buy',volume:.50,price_open:2476.12,price_current:2479.56,profit:172,sl:2472.5,tp:2486.8},
       {ticket:10341,symbol:'GBPUSD',side:'Sell',volume:1,price_open:1.2854,price_current:1.2841,profit:130,sl:1.289,tp:1.278},
       {ticket:10338,symbol:'NAS100',side:'Buy',volume:.2,price_open:19430.5,price_current:19412.3,profit:-36.4,sl:19360,tp:19600}],market:{}},
  analysis:{regime:'UPTREND',action:'BUY',current:true},paper:{},
  ui:{drawdown:4.2,winrate:76,winTotal:450,wins:342,losses:108,latency:28,margin:6320.45}
});
function genCandles(){let a=[],n=108,base=2478.3;for(let i=0;i<n;i++){
  let wave=2.5*Math.sin(i*.19)+1.05*Math.sin(i*.51)+.5*Math.sin(i*.07+2);
  let next=2478.8+wave+ (i/n)*1.1;
  let open=base,close=next,dist=.22+(Math.sin(i*21.73)+1)*.23;
  a.push({time:1782000000+i*300,open,close,high:Math.max(open,close)+dist,low:Math.min(open,close)-dist,tick_volume:Math.floor(18+35*Math.abs(Math.sin(i*2.1))+32*Math.abs(Math.sin(i*.52))) });base=next;
}return a}
const PREVIEW_CANDLES=genCandles();
function toast(text,bad=false){const e=document.createElement('div');e.className='toast'+(bad?' bad':'');e.textContent=text;$('toastStack').append(e);setTimeout(()=>e.remove(),4800)}
function escapeHtml(s){return String(s??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;').replaceAll("'",'&#039;')}
function getStore(key, fallback){try{return localStorage.getItem(key)||fallback}catch{return fallback}}
function saveStore(key,val){try{localStorage.setItem(key,val)}catch{}}
const nativeCall=(verb,path,body)=>new Promise((resolve,reject)=>{
  if(!native){reject(new Error('Windows Core bridge is unavailable in standalone preview'));return}
  const id=++state.id;let abort=setTimeout(()=>{state.pending.delete(id);reject(new Error('انتهت مهلة الاتصال بمحرك Rust'))},10500);
  state.pending.set(id,{resolve:x=>{clearTimeout(abort);resolve(x)},reject:x=>{clearTimeout(abort);reject(x)}});
  window.chrome.webview.postMessage({id,verb,path,body:body||{}});
});
if(native)window.chrome.webview.addEventListener('message',evt=>{
  const m=evt.data;if(!m||!state.pending.has(m.id))return;
  const entry=state.pending.get(m.id);state.pending.delete(m.id);
  if(m.ok)entry.resolve(m.data);else entry.reject(new Error(m.error||'Request failed'));
});
function sample(){return state.preview?MOCK:(state.connected?state.status:null)}
function trustedStatus(data){if(!data||typeof data!=='object')return null;
  if(!data.gateway_fresh || !data.telemetry?.connected)return null;
  if(typeof data.telemetry.equity!=='number'||typeof data.telemetry.balance!=='number')return null;
  return data;
}
function setPreview(v){state.preview=!!v;$('previewBanner').hidden=!state.preview;$('previewBtn').textContent=state.preview?'إيقاف المعاينة':'معاينة التصميم';updateDashboard();toast(state.preview?'هذه معاينة تصميم بأرقام وهمية ومعلّمة — لا يوجد إرسال أوامر':'تم الخروج من المعاينة، تظهر بيانات MT5 الحقيقية فقط')}
function initTheme(){state.theme=getStore('braintrade.next.theme','dark');document.body.dataset.theme=state.theme}
function setTheme(theme){state.theme=theme;document.body.dataset.theme=theme;saveStore('braintrade.next.theme',theme);renderChart();renderMiniCharts()}
function createNav(){const el=$('sidebarNav');el.textContent='';for(const [id,glyph,label] of nav){const b=document.createElement('button');b.className='nav-item'+(id===state.page?' selected':'');b.type='button';b.dataset.go=id;b.innerHTML=`<span class="nav-glyph">${glyph}</span><span>${label}</span>`;el.append(b)}}
function showPage(page){if(!nav.some(p=>p[0]===page)&&!['shadow','logs'].includes(page))page='dashboard';state.page=page;createNav();$('dashboard').hidden=page!=='dashboard';$('subpage').hidden=page==='dashboard';if(page!=='dashboard')renderSubpage(page);else{updateDashboard()}$('content').scrollTop=0}
function tfCandles(s){if(state.preview)return PREVIEW_CANDLES;
  const market=s?.telemetry?.market;if(!market)return [];
  const tf=state.tf;const arr=market[tf];return Array.isArray(arr)?arr.filter(x=>x&&Number.isFinite(x.open)&&Number.isFinite(x.close)&&Number.isFinite(x.high)&&Number.isFinite(x.low)).slice(-120):[];
}
function paintAccount(s){let t=s?.telemetry,preview=state.preview;
  $('brokerName').textContent=preview?'IC Markets':t?.server?.split('-')[0]||'MT5 Offline';
  $('serverName').textContent=t?.server||'—';
  $('topEquity').textContent=money(t?.equity);$('topBalance').textContent=money(t?.balance);
  $('topFree').textContent=money(t?.margin_free);$('topMargin').textContent=preview?money(MOCK.ui.margin):'—';
  $('topDaily').textContent=money(t?.today_net);$('topLatency').textContent=preview?'▂ 28 ms':'—';
  $('topLatency').title=preview?'معاينة توضيحية':'لا تتوفر قياسات Latency حقيقية';
  $('connectionDot').classList.toggle('online',!!s);$('connectionLabel').textContent=preview?'معاينة':s?'متصل':'غير متصل';
  $('liveBadge').classList.toggle('active',!!s&&!t?.demo);$('demoBadge').classList.toggle('active',!!s&&!!t?.demo);
}
function paintKPIs(s){const t=s?.telemetry||{};const preview=state.preview;
  $('equityValue').textContent=money(t.equity);$('equityDelta').textContent=preview?'▲ +2.40% اليوم':'حساب MT5 • حقوق فعلية';
  $('dailyValue').textContent=money(t.today_net);$('dailyPercent').textContent=preview?'▲ +2.40%':'صافي يوم الوسيط';
  $('drawdownValue').textContent=preview?'4.2%':'—';$('drawdownNote').textContent=preview?'12% الحد الأقصى':'يلزم سجل Equity لحساب التراجع';$('drawdownMeter').style.width=preview?'42%':'0%';
  const pos=t.positions||[];$('activePositions').textContent=s?String(pos.length):'—';let buys=pos.filter(x=>String(x.side).toLowerCase().includes('buy')).length;
  $('buyCount').textContent= s?`${buys} شراء`:'— شراء';$('sellCount').textContent=s?`${pos.length-buys} بيع`:'— بيع';$('buyMeter').style.width=(pos.length?100*buys/pos.length:0)+'%';$('sellMeter').style.width=(pos.length?100*(pos.length-buys)/pos.length:0)+'%';
  $('winRateValue').textContent=preview?'76%':'—';$('winRateNote').textContent=preview?'342 / 450 صفقة':'بانتظار سجل الصفقات المغلقة';$('winDonut').style.borderTopColor=preview?'#00e5b7':'#426275';
  $('latencyValue').textContent=preview?'28 ms':'—';$('latencyNote').textContent=preview?'ممتاز':'يلزم قياس إرسال وتنفيذ حقيقي';
}
function canvas2d(canvas){const width=canvas.clientWidth,height=canvas.clientHeight;if(!width||!height)return null;
  const dpr=Math.min(window.devicePixelRatio||1,2);canvas.width=Math.floor(width*dpr);canvas.height=Math.floor(height*dpr);
  const ctx=canvas.getContext('2d');ctx.setTransform(dpr,0,0,dpr,0,0);return {ctx,w:width,h:height};}
function renderChart(){const s=sample(),candles=tfCandles(s),cv=$('candles'),result=canvas2d(cv);if(!result)return;
  const {ctx,w,h}=result;ctx.clearRect(0,0,w,h);const last=candles.slice(-100);$('chartEmpty').hidden=last.length>=3;
  const left=5,right=59,top=11,bottom=26,plotW=w-left-right,plotH=h-top-bottom;
  ctx.strokeStyle=getComputedStyle(document.body).getPropertyValue('--border').trim()||'#244252';ctx.lineWidth=.65;
  for(let i=0;i<=5;i++){let y=top+i*plotH/5;ctx.beginPath();ctx.moveTo(left,y);ctx.lineTo(w-right+3,y);ctx.stroke()}
  for(let i=0;i<=8;i++){let x=left+i*plotW/8;ctx.beginPath();ctx.moveTo(x,top);ctx.lineTo(x,top+plotH);ctx.stroke()}
  if(last.length<3)return;
  let pMin=Math.min(...last.map(x=>x.low)),pMax=Math.max(...last.map(x=>x.high));const pad=Math.max((pMax-pMin)*.09,.05);pMin-=pad;pMax+=pad;
  const y=p=>top+(pMax-p)/(pMax-pMin)*plotH,step=plotW/last.length;
  const maxVolume=Math.max(1,...last.map(x=>x.tick_volume||0));
  last.forEach((c,i)=>{const x=left+step*(i+.5),isUp=c.close>=c.open,color=isUp?'#03dec2':'#ff566b';ctx.strokeStyle=color;ctx.fillStyle=color;ctx.lineWidth=1;
    ctx.beginPath();ctx.moveTo(x,y(c.high));ctx.lineTo(x,y(c.low));ctx.stroke();let bodyTop=y(Math.max(c.open,c.close)),bodyBottom=y(Math.min(c.open,c.close));ctx.fillRect(x-step*.29,bodyTop,Math.max(2.4,step*.58),Math.max(1.5,bodyBottom-bodyTop));
    const volumeH=((c.tick_volume||0)/maxVolume)*21;ctx.fillStyle=isUp?'#158f7c':'#a24a5a';ctx.globalAlpha=.75;ctx.fillRect(x-step*.28,top+plotH-volumeH,Math.max(2,step*.55),volumeH);ctx.globalAlpha=1;
  });
  if(state.ma){const moving=(period,color)=>{ctx.strokeStyle=color;ctx.lineWidth=1.15;ctx.beginPath();let first=true;last.forEach((c,i)=>{if(i<period)return;let sum=0;for(let j=i-period+1;j<=i;j++)sum+=last[j].close;const xx=left+step*(i+.5),yy=y(sum/period);if(first){ctx.moveTo(xx,yy);first=false}else ctx.lineTo(xx,yy)});ctx.stroke()};moving(9,'#ffc55e');moving(24,'#40a5ff');moving(46,'#b87ffa')}
  ctx.fillStyle='#a2b4c2';ctx.font='10px Segoe UI, sans-serif';ctx.textAlign='right';for(let i=0;i<=5;i++){let val=pMax-(pMax-pMin)*i/5;ctx.fillText(fmt(val,2),w-2,top+plotH*i/5+4)}
  ctx.textAlign='center';const interval=Math.max(1,Math.floor(last.length/7));last.forEach((c,i)=>{if(i%interval===0){const date=new Date((Number(c.time)>1e12?c.time:c.time*1000));let label=Number.isFinite(date.getTime())?date.toLocaleTimeString('en-US',{hour:'2-digit',minute:'2-digit',hour12:false}):'—';ctx.fillText(label,left+step*(i+.5),h-5)}});
  const quote=last[last.length-1].close,yy=y(quote);ctx.strokeStyle='#03d9ba';ctx.setLineDash([3,3]);ctx.beginPath();ctx.moveTo(left,yy);ctx.lineTo(w-right,yy);ctx.stroke();ctx.setLineDash([]);ctx.fillStyle='#05d6b7';ctx.fillRect(w-right,yy-9,right-4,18);ctx.fillStyle='#001a20';ctx.textAlign='center';ctx.font='bold 10px Segoe UI';ctx.fillText(fmt(quote,2),w-right/2-2,yy+3);
  // Trade markers are an illustration only; real fills aren't mapped to candle times without linked evidence.
  if(state.preview)for(const [i,t] of [[15,'S'],[35,'B'],[59,'S'],[88,'B']]){let c=last[i];if(!c)continue;ctx.fillStyle=t==='B'?'#06dea9':'#f64566';let x=left+step*(i+.5),yy=y(t==='B'?c.low:c.high)+(t==='B'?13:-14);ctx.fillRect(x-8,yy-10,16,18);ctx.fillStyle='white';ctx.fillText(t,x,yy+2)}
  if(state.crosshair&&state.chartHover){const {x,y:my}=state.chartHover;ctx.strokeStyle='#84d2ebaa';ctx.lineWidth=.8;ctx.beginPath();ctx.moveTo(x,top);ctx.lineTo(x,top+plotH);ctx.moveTo(left,my);ctx.lineTo(w-right,my);ctx.stroke();let idx=Math.max(0,Math.min(last.length-1,Math.floor((x-left)/step)));const c=last[idx];$('chartTooltip').hidden=false;$('chartTooltip').textContent=`O ${fmt(c.open)}  H ${fmt(c.high)}  L ${fmt(c.low)}  C ${fmt(c.close)}`;$('chartTooltip').style.left=Math.min(w-218,Math.max(10,x+11))+'px';$('chartTooltip').style.top=Math.max(5,my-42)+'px'}else $('chartTooltip').hidden=true;
}
function spark(canvas,color,vals){const q=canvas2d(canvas);if(!q)return;const{ctx,w,h}=q;const low=Math.min(...vals),high=Math.max(...vals),range=high-low||1;
  ctx.strokeStyle=color;ctx.lineWidth=2;ctx.shadowColor=color;ctx.shadowBlur=8;ctx.beginPath();vals.forEach((v,i)=>{const x=2+i*(w-4)/(vals.length-1),y=h-4-(v-low)/range*(h-9);i?ctx.lineTo(x,y):ctx.moveTo(x,y)});ctx.stroke();ctx.shadowBlur=0;}
function renderMiniCharts(){const result=canvas2d($('equityChart'));if(!result)return;const{ctx,w,h}=result;ctx.strokeStyle='#1e4556';ctx.lineWidth=.6;for(let i=0;i<5;i++){ctx.beginPath();ctx.moveTo(0,8+i*(h-14)/4);ctx.lineTo(w,8+i*(h-14)/4);ctx.stroke()}
 if(!state.preview)return;const samples=Array.from({length:35},(_,i)=>28+1.08*i+Math.sin(i*.48)*2+Math.cos(i*.23)*1.6);const low=23,high=70;
 ctx.strokeStyle='#00ccfa';ctx.lineWidth=2;ctx.shadowBlur=7;ctx.shadowColor='#17bafd';ctx.beginPath();samples.forEach((v,i)=>{const x=i/(samples.length-1)*(w-12)+6,y=h-9-(v-low)/(high-low)*(h-19);i?ctx.lineTo(x,y):ctx.moveTo(x,y)});ctx.stroke();ctx.shadowBlur=0;
 for(let i=0;i<34;i++){const x=6+i/34*(w-12),v=4+10*Math.abs(Math.sin(i*.71));ctx.fillStyle=i%3===0?'#ff586b':'#176b9e';ctx.fillRect(x,h-4-v,4,v)}
}
function renderPositions(s){const pos=s?.telemetry?.positions||[],filter=state.positionsFilter;
  $('allPositionsCount').textContent=pos.length;
  const list=pos.filter(x=>filter==='all'||String(x.side).toLowerCase().includes(filter));
  $('positionsRows').innerHTML=list.length?list.map(p=>`<tr><td>${escapeHtml(p.ticket)}</td><td>${escapeHtml(p.symbol)}</td><td class="${String(p.side).toLowerCase().includes('buy')?'buy':'sell'}">${escapeHtml(p.side)}</td><td>${fmt(p.volume,2)}</td><td>${fmt(p.price_open,2)}</td><td>${fmt(p.price_current,2)}</td><td class="${p.profit>=0?'buy':'sell'}">${money(p.profit)}</td><td><button class="row-action" type="button" data-go="execution">إدارة ↗</button></td></tr>`).join(''):`<tr class="empty-row"><td colspan="8">${state.preview?'لا توجد مراكز مطابقة لهذا المرشح':'لا توجد مراكز MT5 موثوقة للعرض'}</td></tr>`;
}
function renderJournal(){let rows=state.preview?[['17:24:11','XAUUSD','Buy','0.50','2479.56'],['17:16:30','GBPUSD','Sell','1.00','1.28410'],['17:11:25','NAS100','Buy','0.20','19412.30'],['16:35:42','XAUUSD','Sell','0.30','2476.80'],['16:24:55','EURUSD','Buy','1.00','1.09214']]:[];
  $('journalRows').innerHTML=rows.length?rows.map(r=>`<tr>${r.map((v,i)=>`<td class="${i===2?(v==='Buy'?'buy':'sell'):''}">${escapeHtml(v)}</td>`).join('')}</tr>`).join(''):'<tr class="empty-row"><td colspan="5">سجل MT5 الحقيقي متاح بصفحة السجلات؛ لا نعرض صفقات متخيلة</td></tr>';
}
function renderBrainTable(s){const demo=state.preview;const rows=demo?[
  ['Brain 1 - Trend Following',87],['Brain 2 - Mean Reversion',82],['Brain 3 - Sniper',78],['Brain 4 - Order Flow',85],['Risk Manager',null],['Execution Engine',null]]:[];
  let real=s?.analysis;const container=$('brainsInline');
  if(demo){container.innerHTML=rows.map(([label,n],i)=>`<div class="brain-line"><i class="indicator"></i><span class="insight">${escapeHtml(label)}</span><b>نشط</b><span>${n==null?'—':n+'%'}</span><canvas class="brain-spark" data-seed="${i}"></canvas><span>⚙</span></div>`).join('');container.querySelectorAll('canvas').forEach((cv,i)=>spark(cv,'#00eabb',Array.from({length:14},(_,j)=>Math.sin(j*.75+i)+j*.28+Math.cos(j*1.7+i)*.8)));}
  else{const active=!!real?.current;container.innerHTML=`<div class="brain-line"><i class="indicator ${active?'':'off'}"></i><span class="insight">Core Coordinator</span><b>${active?'نشط':'انتظار'}</b><span>—</span><span>${escapeHtml(real?.action||'WAIT')}</span><span>⚙</span></div><div style="font-size:11px;color:var(--muted);padding:14px 6px;line-height:1.6">نسب أداء العقول تتطلب نتائج صفقات معتمدة؛ لا تُستنتج من الإشارات.</div>`;}
}
function renderQuality(){const q=state.preview?[['الانزلاق السعري','0.12 pips',68,'#10eab5'],['سرعة التنفيذ','98.7%',94,'#0be4ad'],['معدل إعادة التنفيذ','2.1%',20,'#f2b93b'],['نسبة الأوامر المرفوضة','0.8%',10,'#f45a69']]:[['الانزلاق السعري','—',0,'#60aeb9'],['سرعة التنفيذ','—',0,'#60aeb9'],['إعادة التنفيذ','—',0,'#60aeb9'],['الرفض','—',0,'#60aeb9']];
  $('qualityList').innerHTML=q.map(([a,b,n,c])=>`<div class="quality-row"><span>${a}</span><i style="--pct:${n}%;--tone:${c}"></i><b>${b}</b></div>`).join('');}
function renderAlerts(s){const rows=state.preview?['XAUUSD صفقة شراء +$172.00','GBPUSD تم إغلاق الصفقة','تحديث نموذج Brain 2','تحديث أرباح اليوم +$1,230.18']:s?[`Core ${s.state||'WAIT'}`,s.message||'حالة الحساب مستقرة',s.gateway_fresh?'MT5 telemetry fresh':'بيانات MT5 قديمة أو غير متاحة',`Telegram ${s.telegram_status||'—'}`]:['لا اتصال بمحرك التداول','افتح التطبيق الأصلي أو اربط منصة MT5'];$('alertsList').innerHTML=rows.map((x,i)=>`<div class="alerts-line"><time>${state.preview?['17:24','17:18','17:11','16:58'][i]||'—':'—'}</time><i>${i?'i':'✓'}</i><span title="${escapeHtml(x)}">${escapeHtml(x)}</span></div>`).join('')}
function renderRiskBars(){const demo=state.preview;let lines=demo?[['مستوى الهامش','320%',80,'#08dfad'],['التعرض الحالي','2.4%',45,'#1faaf7'],['المخاطرة اليومية','6.1%',67,'#ffbd3c'],['السحب الأقصى','4.2%',39,'#fb5168']]:[['مستوى الهامش','—',0,'#10e8b8'],['التعرض الحالي','—',0,'#1cafff'],['المخاطرة اليومية','—',0,'#ffc54a'],['السحب الأقصى','—',0,'#f64f70']];$('riskBars').innerHTML=lines.map(([a,b,n,c])=>`<div class="risk-line"><span>${a}</span><div class="risk-meter"><i style="--pct:${n}%;--tone:${c}"></i></div><b>${b}</b></div>`).join('')}
function renderTradeDistribution(){let d=state.preview?MOCK.ui:null;const pct=d?.winrate||0;
  $('tradeDonut').style.background=`conic-gradient(#00dfb8 0 ${pct}%,#ff5969 ${pct}% ${pct?100:0}%,#2b4553 0)`;
  $('tradeTotal').textContent=d?d.winTotal:'—';$('tradeWins').textContent=d?d.wins:'—';$('tradeLosses').textContent=d?d.losses:'—';}
function paintQuotes(s){let t=s?.telemetry;const bars=tfCandles(s);let c=bars.length?bars[bars.length-1]:null;
  $('chartSymbol').textContent=t?.symbol||'XAUUSD';$('ohlc').innerHTML=c?`O <span>${fmt(c.open,2)}</span>　H <span>${fmt(c.high,2)}</span>　L <span>${fmt(c.low,2)}</span>　C <span>${fmt(c.close,2)}</span>　${fmt(c.close-c.open,2)}`:'O —　H —　L —　C —';
  $('timeframes').querySelectorAll('button').forEach(b=>{b.classList.toggle('selected',b.dataset.tf===state.tf);let available=state.preview||!!t?.market?.[b.dataset.tf]?.length;b.disabled=!available});
}
function updateDashboard(){let s=sample();$('previewBanner').hidden=!state.preview;paintAccount(s);paintKPIs(s);paintQuotes(s);renderChart();renderPositions(s);renderJournal();renderBrainTable(s);renderQuality();renderAlerts(s);renderRiskBars();renderTradeDistribution();renderMiniCharts();
  $('equityEmpty').style.display=state.preview?'none':'grid';$('statusText').textContent=state.preview?'◈ DESIGN PREVIEW / جميع القيم المعروضة توضيحية':s?`● CORE ${escapeHtml(s.state)} — ${s.telemetry?.demo?'DEMO':'REAL'} / ${s.telemetry?.login||'—'}`:`○ CORE / MT5 OFFLINE — ${state.apiError||'بانتظار الاتصال'}`;
  $('startBtn').disabled=state.preview||!state.connected||state.busy||!!state.status?.requested_run;
  $('stopBtn').disabled=state.preview||!state.connected||state.busy||!state.status?.requested_run;
  if(state.preview){spark($('dailySpark'),'#09e7c6',Array.from({length:20},(_,i)=>Math.sin(i*.9)*2+i*.53));}else{const cv=$('dailySpark');const c=canvas2d(cv);c?.ctx.clearRect(0,0,c.w,c.h);}
}
function pageLabel(page){return nav.find(x=>x[0]===page)?.[2]||page}
function pageConfig(page,s){const t=s?.telemetry;const broker=s&&!!t;
  const cards={
   trading:[['وضع التداول النشط',s?.analysis?.action||'WAIT'],['سوق الذهب',t?.symbol||'—']],
   execution:[['حالة Core',s?.state||'غير متصل'],['الحساب المربوط',t?`${t.login} • ${t.demo?'DEMO':'REAL'}`:'غير متصل']],
   brains:[['العقل الرئيسي',s?.analysis?.action||'WAIT'],['السوق',s?.analysis?.regime||'—']],
   risk:[['حقوق الحساب',money(t?.equity)],['الهامش الحر',money(t?.margin_free)]],
   positions:[['عدد المراكز',broker?String(t.positions?.length||0):'—'],['الصافي اليومي',money(t?.today_net)]],
   analysis:[['اتجاه السوق',s?.analysis?.regime||'غير متاح'],['آخر قرار',s?.analysis?.action||'WAIT']],
   journal:[['بيانات الوسيط',broker?'MT5 متصل':'غير متصل'],['مراجعة الصفقات','سجل موثق فقط']],
   reports:[['تقارير الحساب','فصل الأداء الفعلي عن Shadow'],['البيانات','تتطلب جلسة MT5']],
   telegram:[['حالة Telegram',s?.telegram_status||'—'],['التفعيل','من الواجهة الكاملة']],
   settings:[['حالة النظام',s?.state||'OFFLINE'],['الثيمات','Dark / Smoke / Light']],
   shadow:[['التجارب الافتراضية','مستقلة عن MT5'],['الحماية','لا ترسل أوامر للوسيط']],
   logs:[['آخر حالة',s?.message||'—'],['السجلات','Core audit logs']]
  };
  return cards[page]||[];
}
function renderSubpage(page){const s=sample(),t=s?.telemetry;const description={dashboard:'',trading:'إدارة الاستراتيجيات والقناص واختيار وضع التداول.',execution:'ربط الوسيط، التفعيل، ومراجعة أوامر التنفيذ.',brains:'العقول والتحليلات وتقدم التعلم الفعلي.',risk:'سقوف الهامش واللوت ومخاطر التداول.',positions:'المراكز المفتوحة وإجراءات إدارة التنفيذ.',analysis:'قراءة إشارات وتحليل اتجاه الأسواق.',journal:'سجل صفقات الوسيط وأدلة النتائج.',reports:'تقارير الأداء والبيانات.',telegram:'قناة الإشعارات والتحكم المصرّح به.',settings:'التخصيص والتشغيل والاتصال.',shadow:'التجارب المنفصلة عن التنفيذ.',logs:'سجلات القرارات والاتصال.'};
  const sections=pageConfig(page,s).map(([title,value])=>`<article class="feature-card"><h2>${escapeHtml(title)}</h2><p>${escapeHtml(value)}</p></article>`).join('');
  const fields=page==='settings'?`<div style="display:flex;gap:8px;flex-wrap:wrap"><button type="button" data-theme="dark">الداكن</button><button type="button" data-theme="smoke">الدخاني</button><button type="button" data-theme="light">الأبيض</button></div>`:'';
  const details=page==='positions'&&t?.positions?.length?`<pre>${escapeHtml(t.positions.map(p=>`${p.ticket} | ${p.symbol} ${p.side} ${fmt(p.volume)} | ${money(p.profit)}`).join('\n'))}</pre>`
 :page==='brains'?`<pre>${escapeHtml(JSON.stringify(s?.analysis||{},null,2))}</pre>`
 :page==='analysis'?`<pre>${escapeHtml(JSON.stringify({analysis:s?.analysis||null},null,2))}</pre>`:'';
  $('subpage').innerHTML=`<header class="subpage-header"><h1>${escapeHtml(pageLabel(page))}</h1><p>${escapeHtml(description[page]||'المحرك والتداول')}</p></header><div class="subpage-grid">${sections}<article class="feature-card"><h2>واجهة التشغيل الكاملة</h2><p>تبقى جميع الوظائف المتقدمة والصفحات الـ22 في تطبيق V4 الأصلي. افتحها من هنا بدلاً من عرض أزرار تجريبية لا تعمل.</p><button id="subClassic" type="button">فتح نافذة V4 الكاملة ↗</button> <button type="button" data-go="dashboard">العودة للوحة الرئيسية</button>${fields}${details}</article></div>`;
  const btn=$('subClassic');btn.addEventListener('click',openClassic);
}
async function refresh(){if(state.preview||!native)return;
  try{const r=await nativeCall('GET','/v1/status');state.status=r;state.connected=!!trustedStatus(r);state.apiError='';state.lastStatusTime=Date.now();}
  catch(e){state.connected=false;state.status=null;state.apiError=e.message}
  if(state.page==='dashboard')updateDashboard();else renderSubpage(state.page);
}
function openClassic(){if(!native){toast('هذا الزر يفتح التطبيق الأصلي عند تشغيل نسخة Windows');return}nativeCall('APP','classic').catch(e=>toast(e.message,true))}
function modal(title,message,needLogin=false){return new Promise(resolve=>{state.modalResolve=resolve;$('modalTitle').textContent=title;$('modalMessage').textContent=message;$('confirmLabel').hidden=!needLogin;$('confirmLogin').value='';$('modalLayer').hidden=false;$('acceptModal').focus()})}
function dismissModal(value){$('modalLayer').hidden=true;const res=state.modalResolve;state.modalResolve=null;res?.(value)}
async function brokerAction(action){if(state.preview){toast('معاينة التصميم لا تسمح بإرسال أوامر التداول',true);return}
  if(!native||!state.connected){toast('لا توجد جلسة Rust Core وMT5 موثوقة',true);return}
  let s=state.status;let loginNeeded=action==='start'&&s?.telemetry?.demo===false;
  let confirm=await modal(action==='start'?'تأكيد بدء التداول':'إيقاف النظام',
    loginNeeded?'أنت على حساب REAL. تأكد من المخاطر واكتب رقم الحساب الحالي قبل بدء النظام.':action==='start'?'ستُرسل عملية START إلى Core، الذي يتحقق من حساب الوسيط والمخاطر قبل أي تنفيذ.':'سيتم إيقاف الدخول الجديد. إدارة المراكز القائمة تخضع لقواعد Core.',loginNeeded);
  if(!confirm)return;
  if(loginNeeded&&String(confirm)!==String(s.telemetry?.login)){toast('رقم تأكيد حساب REAL غير مطابق',true);return}
  state.busy=true;updateDashboard();try{await nativeCall('POST',`/v1/${action}`,action==='start'?{confirm_live_login:loginNeeded?String(confirm):''}:{});toast(`تم إرسال ${action==='start'?'START':'STOP'} وتأكيده من Core`);await refresh();}catch(e){toast(e.message,true)}finally{state.busy=false;updateDashboard()}
}
function setupEvents(){
 // Explicit timeframe handlers keep a visible selected state after pointer actions.
 document.querySelectorAll('[data-tf]').forEach(button=>button.addEventListener('click',event=>{
   event.stopPropagation();
   state.tf=button.dataset.tf;
   document.querySelectorAll('[data-tf]').forEach(x=>x.classList.toggle('selected',x===button));
   paintQuotes(sample());renderChart();
 }));document.addEventListener('click',e=>{
  const v=e.target.closest('[data-go]');if(v){showPage(v.dataset.go);return}
  const t=e.target.closest('[data-theme]');if(t){setTheme(t.dataset.theme);toast('تم تغيير الثيم');return}
  const tf=e.target.closest('[data-tf]');if(tf){state.tf=tf.dataset.tf;paintQuotes(sample());renderChart();return}
  const tab=e.target.closest('[data-op-tab]');if(tab){state.tab=tab.dataset.opTab;document.querySelectorAll('.ops-tabs button').forEach(x=>x.classList.toggle('active',x===tab));$('opsControls').hidden=state.tab!=='trade';$('opsSecondary').hidden=state.tab==='trade';if(state.tab!=='trade'){$('opsSecondary').innerHTML=state.tab==='orders'?'<strong>إدارة الأوامر</strong><p>يمكن عرض الأوامر وتنفيذ إجراءات الوسيط من شاشة التنفيذ الأصلية، المربوطة بتأكيدات Core.</p><button type="button" data-go="positions">المراكز والأوامر ↗</button>':'<strong>وضع المتداول المتقدم</strong><p>إعدادات الاستراتيجيات والقناص والأوضاع الفرعية محفوظة داخل نظام V4 الأصلي.</p><button type="button" data-go="trading">مركز التداول ↗</button>';}return}
  const fil=e.target.closest('[data-positions]');if(fil){state.positionsFilter=fil.dataset.positions;document.querySelectorAll('[data-positions]').forEach(x=>x.classList.toggle('chosen',x===fil));renderPositions(sample());return}
  const card=e.target.closest('article[data-go]');if(card)showPage(card.dataset.go);
});
 document.querySelectorAll('article[data-go]').forEach(card=>{card.setAttribute('role','button');card.tabIndex=0;card.addEventListener('keydown',e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();showPage(card.dataset.go)}})});
 $('assistantTile').onclick=()=>showPage('brains');$('startBtn').onclick=()=>brokerAction('start');$('stopBtn').onclick=()=>brokerAction('stop');
 $('previewBtn').onclick=()=>setPreview(!state.preview);$('leavePreview').onclick=()=>setPreview(false);
 $('classicBtn').onclick=openClassic;$('expandChart').onclick=()=>{document.querySelector('.chart-panel')?.classList.toggle('fullscreen');setTimeout(renderChart,110)};
 $('maToggle').onclick=()=>{state.ma=!state.ma;renderChart();toast(state.ma?'تم إظهار المتوسطات':'تم إخفاء المتوسطات')};
 $('crosshairToggle').onclick=()=>{state.crosshair=!state.crosshair;state.chartHover=null;renderChart();toast(state.crosshair?'مؤشر تتبع الشموع مفعّل':'مؤشر تتبع الشموع متوقف')};
 $('indicatorToggle').onclick=()=>toast('المتوسطات 9 / 24 / 46 متاحة من زر ⌁؛ المؤشرات الأخرى داخل صفحة التحليلات');
 $('candles').addEventListener('mousemove',e=>{if(!state.crosshair)return;const r=e.currentTarget.getBoundingClientRect();state.chartHover={x:e.clientX-r.left,y:e.clientY-r.top};renderChart()});
 $('candles').addEventListener('mouseleave',()=>{state.chartHover=null;renderChart()});
 $('cancelModal').onclick=()=>dismissModal(null);$('closeModal').onclick=()=>dismissModal(null);
 $('acceptModal').onclick=()=>dismissModal($('confirmLabel').hidden?true:$('confirmLogin').value.trim());
 $('modalLayer').addEventListener('click',e=>{if(e.target===$('modalLayer'))dismissModal(null)});
 window.addEventListener('keydown',e=>{if(e.key==='Escape'){$('chartFrame').closest('.chart-panel').classList.remove('fullscreen');if(!$('modalLayer').hidden)dismissModal(null);renderChart()}});
 window.addEventListener('resize',()=>{renderChart();renderMiniCharts();renderBrainTable(sample())});
}
function init(){initTheme();createNav();setupEvents();$('previewBanner').hidden=!state.preview;showPage('dashboard');$('clock').textContent=new Date().toLocaleTimeString('en-GB');setInterval(()=>{$('clock').textContent=new Date().toLocaleTimeString('en-GB')},1000);if(native){refresh();setInterval(refresh,2200)}}
init();