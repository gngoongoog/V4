# BRAINTRADE V25 — G2-M | منع إعادة إرسال أمر MT5 قديم عند غياب Journal

## المشكلة المثبتة بالكود
إذا لم يجد Python Gateway سجلاً سابقاً في جدول sends، كان يوقف الأمر بحالة unknown أو placed فقط. لكن نتيجة سابقة من Core بحالة filled أو partial أو observed أو cancelled أو حالات منتهية أخرى قد تمر في مسار order_send وكأنها جديدة. أثناء الانقطاع أو بعد إعادة تشغيل Core وGateway يجب ألا يتحول حدث قديم إلى إرسال إضافي للوسيط.

## الإصلاح
- لا يسمح Gateway بتحضير إرسال جديد مع Journal مفقود إلا لأمر من حالة queued أو dispatched.
- أمر queued مسموح فقط لأن استجابة Core قد تُبنى قبل أن يغيّر Core حالة الأمر إلى dispatched في SQLite؛ تتأكد المراجعتان المتتاليتان من Core أن حالته dispatched قبل إرسال أمر فعلي.
- بقية الحالات تعيد unknown مع سبب لا لبس فيه. لا يحصل order_send أو قراءة بيانات الوسيط أو كتابة أي سجل sends جديد بسببها.
- إذا كان Journal محفوظاً فعلاً، يبقى مسار استعادة التاريخ ومطابقة intent كما هو؛ لا يتم تعطيل مصالحة split fills أو رسوم Broker الأصلية.

## اختبار الانقطاع
أُضيف اختبار Python بعشر حالات قديمة، منها partial وfilled وobserved وcancelled وclosed وexpired وrejected، ويؤكد عدم وصول التنفيذ إلى broker أو callback المصادقة أو journal. ثم يختبر إرسال queued جديد لمرة واحدة فقط وإعادة الطلب بلا إرسال ثانٍ.

## ما لا يدّعيه الاختبار
نجاح CI لا يثبت وجود جلسة MT5 Demo حقيقية، ولا يسمح بتداول REAL قبل قبول دورة Broker-native مع حساب Demo وRestart. هذا العمل مسودة PR #165 فوق G2-L #164. لم يتغير main أو مرجع Enterprise V24.