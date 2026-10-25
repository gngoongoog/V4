# BRAINTRADE V25 — G5-H | منع عرض حساب MT5 سابق في Brain Monitor

## سبب الإصلاح

واجهة Windows تحدّث بيانات الحساب من GET /v1/status، بينما تعرض Brain Monitor استجابة مستقلة من GET /v1/execution/brains كل 3 ثوان. قد يصل الرد القديم بعد تبديل حساب MT5 أو فقدان الاتصال، فيبدو وكأن إحصائيات الحساب السابق تخص الحساب الجديد.

## التعديل

- crates/desktop/src/brain_monitor.rs: هوية الحساب مستمدة من Status.telemetry الحالية بشرط Gateway Fresh واتصال مثبت، login إيجابي، وسيرفر ورمز وعملة غير فارغين.
- crates/desktop/src/main.rs: عند تبديل الحساب أو انقطاع Core، يسحب التقرير السابق ويعطّل أزرار Brain Monitor مباشرة.
- Worker يراجع السياق في بداية الطلب وعند وصول الرد؛ يفحص response.context، صلاحية وضع التداول، وcontrols_available. أي اختلاف يلغي عرض الرد.
- طلبات الحفظ والتجميد لا تُرسل من صفحة تملك سياقاً قديمًا. Backend يستمر بحارس context/mode المستقل ولا تتغير صلاحيات التنفيذ.
- فشل قراءة البيانات يمسح نتائج Brain Monitor القديمة بدلاً من ترك رقم غير موثق معروضاً كأنه حي.

## اختبارات الانحدار

- g5h_old_responses_are_not_authoritative_after_account_switch
- g5h_status_requires_fresh_proven_account_not_only_old_telemetry

## الحدود

لا يتم تعديل main أو V24، أو تنفيذ MT5، أو إعدادات المخاطر/الرافعة/اللوت. تحتاج المرحلة نجاح CI عبر Research وLinux وWindows وPortable ورندر Slint على SHA النهائي. الاختبار المحلي على جهاز Windows وحساب MT5 Demo الفعلي شرط منفصل، كما يبقى اختبار OOS الاقتصادي شرطاً مستقلاً لتقييم العقول. لا نسمّي بيانات ورقية أرباح وسيط حقيقية.
