# BRAINTRADE V4 Enterprise V25 — P0-B / P1-F Execution Quality (2026-10-24)

## العودة إلى ملف الإصلاح الأصلي
بعد P1-D/P1-E للحسابات الصغيرة، تعود الأولوية إلى دورة MT5 الصحيحة، الأسباب الدقيقة للرفض والتأخير، مصالحة UNKNOWN، جودة الحماية من التكرار، ثم اختبار الأوضاع الأربعة وتعلّم العقول من سجل Broker مغلق ومُراجع. هذه المرحلة **قراءة وتشخيص فقط** ولا توسع صلاحيات التداول.

## ما تغير
- **Rust Core:** وحدة `crates/runtime/src/execution_quality.rs` تُخرج تقرير `GET /v1/execution/quality` من الجدول الدائم `broker_commands` فقط، لأحدث **100 Intent للحساب/الرمز/العملة الحاليين**. لا توجد إحصائيات مختلطة من Paper أو Shadow.
- الطلب غير متاح لإظهار بيانات حساب آخر دون `gateway_fresh` وتطابق حساب MT5 مع إعداد ربط التنفيذ المخزّن لدى Core. فشل التحقق = `unavailable` وبيانات فارغة بدلاً من سجل آخر حساب.
- **فئات الرفض:** Core يعرض عدّاداً لـ`rejected` فقط ويفصل `unknown` و`cancelled` و`unresolved`. تصنيف Retcodes الواضحة، مثل `10019` الهامش و`10014` حجم اللوت و`10016` الستوب. إن لم تكن البيانات كافية يكتب `broker_or_unclassified`، ولا يخترع سبباً محدداً.
- **الزمن:** `queue_to_send_ms` و`send_ms` و`total_ms` مع p50/p95/p99 مبنية فقط على قيم Gateway الموجودة والصحيحة. عينة صفر = `null`، وليس 0 ms. هذه ليست latency نهائية للوسيط بعد History ولا تأكيد صفقة مغلقة.
- **الخصوصية:** لا يعيد مصدر العرض `payload` أو `result.reason` الخام أو Tokens أو Order IDs. فئات مصنفة فقط، وRetcode، والعقل، ونوع وحالة النية.
- **Windows Tauri:** أضيف `/v1/execution/quality` إلى قائمة مسارات القراءة فقط، دون أي أمر POST جديد.
- **React:** قسم منفصل «جودة التنفيذ وسبب رفض أوامر الوسيط» في صفحة **وضع تلقائي / Execution** يربط جلب البيانات باستدعاء Core الحقيقي، مع زر تحديث فعلي، حالات انقطاع وبيانات فارغة، وعدم عرض بيانات حساب سابق.

## حالات الاختبار
- Rust: حالات rejected / cancelled / unknown / filled وعزل الحساب ومصدر التعليقات، الحماية من احتساب Unknown ضمن الرفض، والـP95 دون ملء صفري، وعدم تعديل SQLite بطلب GET.
- Frontend: اختبارات الحساب/الرمز/DEMO-REAL والفريم الزمني لبيانات Core والتقارير المزورة، والتحقق من عمل الزر وظهور جدول حقيقي فقط عند تطابق التقرير.
- بوابة G1: `cargo test --locked -p braintrade-domain -p braintrade-runtime`, `python3 -m unittest discover -s tests -v`, `npm --prefix apps/terminal test`, `npm --prefix apps/terminal run build`, ثم Windows Portable CI.
- بوابة G2: **NOT TESTED** حتى يتوفر MT5 Demo على Windows: رفض وسيط حقيقي ورقم retcode، إرسال مرة واحدة، Partial وUnknown وRestart والتسوية بالـDeals؛ لا تجريب REAL.
- بوابة G3: **PARTIAL** حتى تُفحص الواجهة الرسومية وجميع صفحات الـ22 على Windows. اختبارات بناء React وحدها ليست اختبار WebView2 على جهاز المستخدم.

## ما تبقى حسب خطة الإصلاح الأولى، بدون الخلط مع مشروع Micro
- P0-B: Acceptance Runner مع MT5 Demo وسجلات Send Once / Partial / Timeout / Reconcile / history_deals_get.
- P0-C: اختبار قاطع الرفض المتكرر وBroker Quality Circuit على Normal Path وSniper Fast Pulse، دون وقف إدارة المخارج.
- P0-D/E: WebView2 22-page smoke وتفعيل/اختبار EA Native Named Pipe وMetaEditor، قياس Tick→Gateway→Broker.
- P1: Tick Replay وأداء Sniper/Scalping/Swing/Expert بعد التكاليف؛ عينات Forward Demo موثقة ونقطة فصل Train/OOS/Shadow/Demo.
- P1/P2: تقويم اقتصادي موثوق/Blackouts، Health Center، Telegram عربي، Backup/Restore drill، Rollback وإصدار Portable مثبت مع دليل القبول.

## حدود قراءة التقرير
- حالة `filled` في Journal ليست بحد ذاتها دليلاً أن سجل الصفقة المغلقة متكامل. المستندات الأخرى تستخدم `broker_deals` لمصالحة الحجم والسعر والرسوم.
- نافذة أحدث 100 Intent، وليست إحصاء تاريخياً كاملاً. الرفض المحلي قبل Queue قد يظهر في سجل إشارات العقول وليس ضمن هذه النافذة؛ لذا لا تحوّل العداد إلى Win Rate أو نسبة رفض كل الإشارات.
- `UNKNOWN` يحتاج مصالحة مع الوسيط؛ **يُمنع إعادة الإرسال التلقائي بعد timeout**.
