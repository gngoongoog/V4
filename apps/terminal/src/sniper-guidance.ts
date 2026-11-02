/**
 * Read-only explanations for measured Demo Sniper blockers.
 * Each code is produced by sniper_blocker_review.rs from existing samples.
 * Never converts sampled decisions to trades, missed profit, or authorizations.
 */
export type SniperAdvice={
 category:'technical'|'broker'|'risk'|'strategy'|'expected'|'unknown';
 title:string;action:string;
};
const advice:Record<string,SniperAdvice>={
 stale_quote:{category:'technical',title:'سعر غير حديث',action:'راجع توقيت آخر Tick واتصال EA/Gateway والرمز المطابق قبل التشغيل.'},
 missing_contract_data:{category:'technical',title:'معلومات العقد ناقصة',action:'افتح خصائص الرمز في MT5 وتحقق من Tick Size وTick Value وبيانات السعر.'},
 initial_tick_warmup:{category:'expected',title:'تهيئة القناص',action:'انتظر وصول ثلاث أسعار فعلية متتابعة، وتأكد أن عداد Tick يتغير.'},
 setup_not_formed:{category:'expected',title:'لا توجد إشارة مؤهلة',action:'راجع شروط الاستراتيجية والفريم والسجل؛ عدم الدخول قد يكون التصرف الصحيح.'},
 cost_vs_target:{category:'risk',title:'التكلفة تتجاوز الهدف',action:'راجع السبريد الفعلي والعمولة والهدف الصافي؛ لا تُلغِ فلتر التكلفة لتكثير الصفقات.'},
 specialist_busy_or_disabled:{category:'strategy',title:'مشاركة العقل',action:'افحص مالك التنفيذ SOLO/Expert وهل القناص مفعل أو منشغل بصفقة.'},
 engine_stopped:{category:'expected',title:'المحرك في STOP',action:'افحص حالة START وسبب الإيقاف، ولا تبدأ التنفيذ إلا على حساب تجريبي متحقق منه.'},
 daily_breaker:{category:'risk',title:'حد الخسارة اليومية',action:'راجع الخسارة المحققة وبيانات اليوم وقاطع المخاطر؛ لا تصفّر القاطع لإخفاء الخسارة.'},
 demo_only_risk:{category:'risk',title:'حد صلاحية المخاطرة',action:'راجع نوع الحساب وسياسة المخاطر؛ الإعدادات التجريبية لا تنتقل تلقائياً إلى REAL.'},
 unsettled_intent:{category:'broker',title:'أمر لم تُحسم نتيجته',action:'افتح مصالحة الوسيط وسجل MT5 لتأكيد الأمر؛ لا تعد الإرسال ولا تحذف السجل.'},
 open_position:{category:'expected',title:'مركز موجود',action:'راجع صفحة المراكز ومالك الصفقة قبل السماح بدخول آخر.'},
 mt5_trading_disabled:{category:'technical',title:'MT5 يمنع التداول',action:'افحص صلاحيات Algo Trading والحساب والرمز من منصة MT5.'},
 netting_symbol_occupied:{category:'broker',title:'رمز محجوز في Netting',action:'راجع مركز الرمز الحالي ونوع الحساب؛ لا تتجاوز ملكية مركز قائم.'},
 cooldown:{category:'expected',title:'فاصل زمني بين الصفقات',action:'راجع وقت آخر إشارة وحد Cooldown؛ لا تختصره قبل تقييم التداول المتكرر.'},
 drawdown_limit:{category:'risk',title:'حد التراجع',action:'راجع Equity والتراجع منذ القمة وسجل الإيقافات.'},
 lot_limit:{category:'risk',title:'حد اللوت',action:'راجع الحد الأدنى وخطوة اللوت والحد الذي اخترته مع هامش الوسيط.'},
 stop_risk_budget:{category:'risk',title:'مخاطرة الوقف',action:'راجع المسافة إلى وقف الخسارة وحجم اللوت والرصيد؛ لا ترفع المخاطرة لمعالجة الرفض.'},
 spread_atr_limit:{category:'risk',title:'سبريد مرتفع',action:'قارن السبريد الحي مع ATR والفريم؛ قد تكون كلفة الصفقة غير مجدية.'},
 atr_unavailable:{category:'technical',title:'شموع غير كافية',action:'راجع تحميل شموع M1 المكتملة ومصدرها قبل الاعتماد على ATR.'},
 already_processed:{category:'expected',title:'إشارة مكررة',action:'افحص سجل الإشارة الأصلية. التكرار لا يعني فرصة جديدة.'},
 netting_unsettled:{category:'broker',title:'أمر Netting غير محسوم',action:'راجع سجل الأوامر ومصالحة الوسيط قبل السماح بأمر على الرمز نفسه.'},
 advisor_paper_only:{category:'strategy',title:'عقل غير مخول بالتداول',action:'افحص الوضع ومالك التنفيذ؛ نتائج Shadow/Paper لا تعني صفقات MT5.'},
 other_execution_gate:{category:'unknown',title:'سبب تنفيذي غير معروف',action:'راجع سجل أحداث Core والتشخيص الأصلي؛ لا تفترض أن رفع القيود يعالجه.'},
 other_unready_reason:{category:'unknown',title:'سبب إشارة غير معروف',action:'احتفظ بتوقيت العينة وافحص السجل الخام في Core دون مشاركة أسرار الحساب.'}
};
const priority:Record<SniperAdvice['category'],number>={
 broker:0,technical:1,risk:2,strategy:3,expected:4,unknown:5
};
export function adviceForSniperCode(code:unknown):SniperAdvice{
 if(typeof code!=='string'||!Object.prototype.hasOwnProperty.call(advice,code))
  return {category:'unknown',title:'سبب غير مصنف',action:'راجع السجل ومصدر العينة؛ لا يمكن اقتراح تعديل آلي.'};
 return advice[code];
}
export type ClassifiedCause={code:string;label:string;count:number;advice:SniperAdvice};
export function classifySniperCauses(value:unknown):ClassifiedCause[]{
 if(!Array.isArray(value))return [];
 const seen=new Set<string>();
 return value.slice(0,32).flatMap((x:unknown)=>{
  if(!x||typeof x!=='object'||Array.isArray(x))return [];
  const r=x as Record<string,unknown>,code=r.code;
  if(typeof code!=='string'||code.length>64||seen.has(code)||
     typeof r.sampled_decisions_with_cause!=='number'||!Number.isSafeInteger(r.sampled_decisions_with_cause)||
     r.sampled_decisions_with_cause<1)return [];
  seen.add(code);
  const result:ClassifiedCause={code,
   label:typeof r.label_ar==='string'&&r.label_ar.length<=100?r.label_ar:adviceForSniperCode(code).title,
   count:r.sampled_decisions_with_cause,advice:adviceForSniperCode(code)};
  return [result];
 }).sort((a,b)=>priority[a.advice.category]-priority[b.advice.category]||b.count-a.count||a.code.localeCompare(b.code)).slice(0,6);
}
