# BRAINTRADE V25 — G2/P0 Demo Preflight (قراءة فقط)

## الفكرة
فحص محلي مباشر من `Rust Core → Gateway → بيانات حساب MetaTrader 5` **دون تنفيذ أي أمر**، وبالذات مفيد أثناء إغلاق سوق الذهب أو عندما يظهر `Quote Stale`. لا يخلط بين اتصال Demo ونجاح التنفيذ عند الوسيط.

## التشغيل على Windows
1. افتح برنامج `BrainTradeNext.exe` وسجّل الدخول إلى حسابك التجريبي Exness Demo مثل المعتاد. أبقِ التداول الآلي على **STOP**.
2. افتح PowerShell من مجلد الحزمة، وشغّل:
   `powershell -NoProfile -ExecutionPolicy Bypass -File .\Tools\Run-Demo-Acceptance.ps1 -PreflightOnly`
3. يكتب الأداة داخل مجلد `acceptance-output`:
   - `demo-readiness-latest.json`
   - `demo-readiness-latest.html`
4. هذه الملفات لا تتضمن رقم الحساب أو اسم السيرفر أو كلمات السر أو مفتاح IPC أو أرصدة العميل. يكتفي التقرير بحالة التحقق من Demo، والرمز وتطابقه، وحداثة Tick، وإشارة وجود Margin من الوسيط، وبوابات `NOT_TESTED`.
5. إذا لم توجد أسعار بسبب العطلة تظهر `QUOTE_STALE_OR_SESSION_INACTIVE`، ولا تستنتج أن السوق مغلق قطعاً. إذا كان رمز MT5 مختلفاً عن إعداد Core تظهر `CONFIGURED_SYMBOL_MISMATCH`. لا تغيّر الأداة رمز التداول ولا تستدعي `order_send`، ولا تطلب EA.

## حدود الإثبات
- `CURRENT_DEMO_ACCOUNT_AND_QUOTE_OBSERVED` لا تعني جاهزية التداول الحقيقي ولا نجاح استراتيجية ولا تحقق التنفيذ الجزئي/العمولة.
- حقل `broker_lifecycle_acceptance` يبقى **NOT_TESTED** بغض النظر عن نتيجة الاتصال؛ يحتاج إثباتاً مستقلاً لوثائق `Order → Deal → Partial → Close → Broker History` من حساب MT5 Demo.
- الأداة تعمل بالقراءة من `/v1/status` و`/v1/config` عبر اتصال Core محلي مؤمّن بمفتاح IPC؛ لا تستخدم الشبكة الخارجية أو أي API يرسل صفقة.
- مسار `-PreflightOnly` لا يطلب مصدر `native/poll` ولا عدد عينات Sniper ولا يقوم بأية تغييرات في إعدادات الحساب.
- الاختبارات على GitHub تستخدم بيانات مختلقة ولا تثبت اتصال منصة Exness الحقيقية على جهاز المستخدم.

## الاختبار والمرحلة التالية
- `python3 -m unittest tests.test_demo_readiness -v` أو `python3 -m unittest discover -s tests -v`.
- نجاح `Research` و`Windows build` و`V25 Windows Portable` على Commit هذا الإصدار.
- بعد رفع تقرير Preflight من جهاز Windows، استكمل بوابة G2 ذات حالات رفض التنفيذ والإغلاق الجزئي والأوامر غير المعلومة، ثم اختبارات Native EA وواجهة WebView2 كاملة. لا توجد صفقات REAL أثناء التطوير.
