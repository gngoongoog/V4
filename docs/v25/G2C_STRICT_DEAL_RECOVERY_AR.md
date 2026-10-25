# BRAINTRADE V25 — G2-C | MT5 Deal Recovery Classification
**المرحلة:** P0-B / G2 — استعادة التنفيذ دون مضاعفة الأوامر
**فرع العمل:** `feature/v25-g2c-strict-deal-reconciliation-20261025`
**المرجع:** `feature/v25-g2b-demo-binding-proof-20261025`

## الخلل الذي عالجناه
كانت دالة `gateway/execution.py::BrokerExecutor._reconcile` تستعيد الأمر `unknown/placed/partial` من `history_deals_get` بناءً على هوية الأمر (Magic/Symbol/Order أو Comment) وسلامة الحجم الإجمالي. لكنها لم تتحقق من **اتجاه BUY/SELL الفعلي** و**نوع DEAL_ENTRY_IN/OUT/OUT_BY** و**وحدة هوية المركز الفيزيائي position_id** لكل Deal مطابق.

تطابق التعليق أو رقم الأمر وحده ليس إثباتاً أن سجل التاريخ يمثل *فتحاً* مقابل *إغلاق* الأمر المحدد. لو جرى الجمع خطأً، قد تصبح نتيجة مجهولة `filled`، ويصبح حجم التنفيذ مدخلاً غير صحيح لاحتساب PnL والتعلّم.

## تعديل الكود
- بعد مطابقة هوية الأمر وقبل ترقية الحالة، تتطلب الاستعادة وجود `type`, `entry`, و`position_id` لكل Broker Deal، إلى جانب Ticket/Volume.
- `request.type` BUY يلزم `DEAL_TYPE_BUY`، وSELL يلزم `DEAL_TYPE_SELL`. أي صفقة مخالفة تبقي الأمر على حالته السابقة.
- `kind=open` يسمح فقط `DEAL_ENTRY_IN`. `kind=close` يقبل `DEAL_ENTRY_OUT` أو `DEAL_ENTRY_OUT_BY`، ولا يقبل `IN` أو `INOUT` دون إثبات مستقل.
- كل Deal يجب أن يحمل `position_id > 0`، وكل الأجزاء المطابقة لنفس Intent يجب أن تشترك بنفس `position_id`.
- تبقى حماية منع إعادة إرسال `order_send` وسجل SQLite الأصلي من دون تغيير، وكذلك منطق Risk/Small Accounts والقناص السريع.
- التقرير عند عدم اكتمال الأدلة هو **pending/unknown/partial السابق**، وليس `filled` اصطناعياً.

## اختبارات المحاكاة المدرجة
`python -m unittest tests.test_broker_execution tests.test_broker_reconciliation_strict tests.test_execution_recovery -v`

الحالات: Timeout/Restart ثم History، أمر `DONE` بلا Deal Ticket، Partial ثم وصول الجزء المتأخر، جهة تداول معاكسة، OUT أو INOUT بدلاً من فتح، نقص نوع الـDeal، انقسام غير منطقي عبر مركزين، وCLOSE لا يتقبل إلا خروجاً بجهة صحيحة. التأكد دائماً أن عدد استدعاءات `order_send` لا يزيد عن واحد لنفس Intent.

## حدود الإثبات وما بقي
هذه اختبارات **Fake MT5** وقراءة مصادر؛ لا تعني نجاح G2 الحقيقي. يحتاج الإغلاق النهائي:
1. تشغيل Windows Portable على الحساب التجريبي والتحقق من ربط الرمز المحدد فعلياً (مثلاً `XAUUSD` مقابل suffix الوسيط).
2. جمع عينات أصلية Order/Deal/History مع Tick وBid/Ask وأرقام تنفيذ منقّحة لصفقات فتح وإغلاق وجزئي ورفض (دون أية كلمات مرور).
3. مطابقة `DEAL_TYPE`, `DEAL_ENTRY`, `position_id`, `order`, `ticket`, `volume`, `commission`, `swap`, و`fee` مقابل سجل Core بعد Restart.
4. التأكد من أن `Unknown` لا يرسل أمر ثاني، وأن الصفقات المفتوحة تبقى قابلة للإدارة حتى لو STOP يمنع مداخل جديدة.
5. تأكيد MetaEditor EA والواجهة الـ22 في Windows واختبارات G4/G5/G6 قبل نشر V25 أو قبول REAL.

**تعليمات الإصدار:** لا تدمج إلى `main` ولا تعدل مرجع V24 قبل نجاح فحوص Commit النهائي وموافقة المستخدم. G2 يبقى `NOT TESTED` من جهة التعامل الفعلي مع Broker.
