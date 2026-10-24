# V25 P0-C — قاطع جودة الوسيط المستقل داخل Rust Core

## الهدف والتسلسل
بعد P1-D/P1-E للحسابات الصغيرة وP0-B/P1-F لتشخيص جودة التنفيذ، رجع العمل إلى المخطط الأصلي: تأمين دورة **Quote → Signal → Durable Intent → Send Once → Broker Result → Reconcile**، قبل تحسين أداء الأوضاع الأربعة أو التعلم.

## الفجوة المكتشفة
كانت بوابة `Python Gateway` تحتوي قاطعاً موجوداً أصلاً في `gateway/institutional_controls.py`: على REAL افتراضياً يمنع إرسال دخول جديد إذا وجد `UNKNOWN` أو حدثت ثلاثة رفضات وسيط موثقة ضمن 120 ثانية. لكن `Rust Core` لم يملك تحقّق جودة مستقل ومتصل بالمسار العادي ومسار `Sniper Fast Pulse`. لا يكفي الاعتماد على مكوّن واحد قبل `order_send`.

## الإصلاح P0-C
- أضيف `crates/runtime/src/broker_quality_circuit.rs`: يقرأ **جدول Core `broker_commands` الدائم** لحساب ورمز محددين بنفس هوية `server/login/symbol/currency`. لا يخلط Demo وREAL لأن Core يتحقق أيضاً من الربط الحقيقي ونوع حساب الوسيط.
- حظر REAL: إذا بقي `kind=open, state=unknown` واحد **حتى لو تجاوز دقيقتين**، يوقف دخولاً جديداً حتى يتأكد الوسيط من النتيجة ويتم تحديث السجل؛ لا يحوّل المهلة إلى دليل على عدم تنفيذ الأمر. أو إذا وقع **3+ `rejected` مع `retcode`** خلال 120 ثانية.
- حالات رفض محلية بلا `retcode` لا تُحسب على أنها رفض Broker، وأوامر `close` لا تُحسب ضمن رفض الفتح.
- `Service::observe` يحدّث الحالة من SQLite أثناء لقطة البيانات العادية ثم يمرر نتيجة السماح إلى وحدة التنفيذ.
- `PUT /v1/execution/pulse` يقرأ **حالة مخزنة بالحساب نفسه**؛ لا يجري استعلام SQLite جديداً لكل Tick، وأي تغيير حساب يمنع التصريح من ذاكرة حساب سابق.
- `POST /v1/execution/authorize` يفحص **السجل الدائم مباشرة عند التخويل النهائي**. إذا فشل الدليل أو وُجد قاطع، يعيد `authorized:false`، حتى لو مرت النية من مرحلة سابقة.
- الأوامر `close` وتسوية `unknown/partial` ومراقبة المراكز **لا تتوقف بقاطع الدخول**.
- `GET /v1/execution/quality` يُظهر `entry_circuit` قراءة فقط، مع السبب `unresolved_broker_submission` أو `repeated_broker_rejections` وعدد الحالات وتاريخ التقييم. قسم جودة التنفيذ في واجهة Windows يقرأ ذلك ولا يعطي زر تجاوز.
- Demo في هذه المرحلة يبقى **advisory** داخل Core افتراضياً؛ القاطع المستقل على REAL إلزامي، وهو متناسق مع إعداد Gateway الافتراضي. لا تغيّر هذه المرحلة حجم اللوت أو الرافعة أو إعدادات Micro REAL.

## الفحص الآلي
- **Rust Unit:** قفل `unknown` حتى المصالحة؛ ثلاث رفضات Retcode، حساب ورمز آخران، انتهاء نافذة الدقيقتين، ملف SQLite معطوب أو غائب، طلب GET لا يعدّل السجل.
- **Rust Integration:** `POST /v1/execution/authorize` يمنع دخول REAL النهائي عند Unknown/3 Rejects، ويسمح بإغلاق صفقة رغم ذلك. يفحص عزل Fast Pulse ورفض التخويل من snapshot لحساب آخر.
- **Python Legacy Regression:** شروط Supervisor القديمة لم تُحذف؛ صار الاختبار يفحص `supervisor_policy_valid && quality_allows` في كلا المسارين.
- **React Windows/Linux:** لا تُعرض حالة قاطع Rust لحساب/وضع آخر أو تقرير زمني متعارض؛ وضع Demo موسوم متابعة فقط.
- تبقى **اختبارات MT5 Demo الحقيقية NOT TESTED** لحين وصول جهاز Windows وMT5؛ الاختبار الافتراضي لا يثبت التوقيت عند الوسيط أو عدم وجود مشاكل مع كل أنواع السماسرة.

## محددات معروفة
- نافذة 120 ثانية في Core تعتمد وقت **إنشاء النية** `created_ms`، بينما Gateway يسجل زمن وصول نتيجة `recorded_ms`. ولذلك هذا قاطع مستقل احتياطي، وGateway يبقى بوابة لازمة لحظة `order_send`. توحيد ساعة المقاييس والبروتوكول من بنود تحسين P0-B اللاحقة.
- `UNKNOWN` دائم في Core حتى تسويته، وقد يتطلب فحص وساطة يدوي لحالة ناقصة غير محسومة. لا يوجد زر استعادة أو `resend` آلي يخفي هذا الغموض.

## المرحلة التالية حسب المخطط
قبول MT5 Demo عبر `DEMO_ACCEPTANCE_RUNNER` مع `MetaEditor`، و`Named Pipe EA` و`history_deals_get` و`Partial/Timeout/Reconnect`، ثم اختبار 22 صفحة في WebView2، ثم تحسين القناص/السكالبر/السوينك/الخبير بالأدلة. لا يوجد ادعاء REAL/Profitability.
