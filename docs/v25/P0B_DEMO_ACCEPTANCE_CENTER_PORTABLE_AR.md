# Enterprise V25 — P0-B/G2: Demo Acceptance Center Portable

## الهدف
العودة لمخطط إصلاح P0-B لإثبات دورة MT5 Demo دون اختراع نتائج من CI، مع حماية عدم تكرار إرسال الأوامر والتسوية. هذه المرحلة **أداة جمع أدلة للقراءة فقط**؛ لا تقوم بعمل صفقات أو تغيير START أو الحساب أو حدود المخاطر.

## عيب مؤكد وصُلح
كانت ملفات `scripts/Run-Demo-Acceptance.cmd` و`Run-Demo-Acceptance.ps1` و`Run-Demo-Acceptance-GUI.ps1` والدليل العربي موجودة في المستودع، لكن `scripts/package_v25.py` لم تنقلها إلى حزمة V25 Portable. تم تصحيح ذلك بحيث تشمل الحزمة:
- `Run-Demo-Acceptance.cmd` في المجلد الرئيسي، لفتح GUI بالدبل كلك، أو CLI عند تمرير parameters.
- `Tools/Run-Demo-Acceptance.ps1` و`Tools/Run-Demo-Acceptance-GUI.ps1`.
- `DEMO_ACCEPTANCE_AR.md` ودليل عربي يميّز مقاييس الزمن عن دورة الوسيط.
- `runtime/gateway/check_runtime.py` وPython ضمن Runtime (موجودان أساساً).

أُضيف فحص `scripts/check_v25_acceptance_package.py` يعمل **offline وبدون تنفيذ MT5**، ويتأكد من الحزمة واكتمال مسارات القراءة فقط قبل خروج ZIP، وكذلك بعد نقل المجلد إلى مسار Windows يحتوي مسافات. ينشئ CI نتائج `test-output/v25/demo-acceptance-package.json` و`demo-acceptance-relocated.json`.

## حماية نطاق حساب Demo والخصوصية
- `gateway/check_runtime.py` الآن يتحقق من `gateway_fresh` و`connected` و`demo` و`account_trade_mode=0` وتطابق `server/login/symbol/currency` في Telemetry مع Core Binding، ويوقف الجلسة إن تغير الحساب أو الرمز أو وضع التداول أو إصدار Core.
- `--base-url` مقيد بعنوان HTTP محلي `127.0.0.1` أو `::1` أو `localhost` مع منفذ، بلا userinfo أو path أو query. لا يُتبع تحويل HTTP Redirect؛ لا يجوز تمرير مفتاح `ipc.key` لأي خادم بعيد.
- ZIP المشاركة من الواجهة يستثني ملفات CSV وState الخام التي قد تتضمن IDs وتعليقات الوسيط، ويشارك فقط التقارير المجمعة. **ملفات التشخيص المحلية الخام تبقى في جهاز المستخدم** ما لم يقرر مشاركتها يدوياً بعد التنقيح.

## فصل بوابتي القبول
- `TARGET_MET` أو `CHARACTERIZED_SLOW` في التقرير تعكس **قياس زمن القناص المحلي** فقط، وليست تأكيد Fill أو أرباح.
- `broker_lifecycle_acceptance.status` يبقى `NOT TESTED` حتى تتوفر أدلة موثقة من MT5 Demo على Windows لدورة Send → Broker ACK → Deal → Close → `history_deals_get`، تشمل Ticket وعمولة دخول/خروج وسواب ورسوم وصافي محقق؛ وأدلة Partial/Unknown/Restart بدون Double Send.
- تطبيق Windows Portable وMetaEditor وNamed Pipe EA لا يُعتبرون مثبتين لمجرد نجاح هذه الفحوصات البنيوية.

## خطوات المشغّل عند توفر Windows
1. فك ضغط أحدث حزمة V25 كاملةً، شغّل `BrainTradeNext.exe` واربط حساب **Demo** وتأكد من حالة MT5 والـEA.
2. شغّل `VERIFY_SYSTEM.cmd` للتأكد من الملفات والـRuntime.
3. افتح `Run-Demo-Acceptance.cmd`. اختر Native فقط عند ثبوت الاتصال بالـEA، أو Poll عندما يكون مفصولاً. لا تحتاج هذه الأداة كلمة سر أو Token.
4. اجمع النتائج. إرسال `BrainTrade-Demo-Acceptance-*.zip` من خيار المشاركة يعطي تقريراً أقل خصوصية من ملفات السجل المحلية. لا ترسل `ipc.key` أو ملفات SQLite.
5. أكمل سيناريوهات Broker Demo الحقيقية على نحو منضبط، ثم دوّن PASS/FAIL لكل بند. غياب الأدلة = `NOT TESTED`، وليس نجاحاً مفترضاً.

## نتائج متوقعة من CI
- `python3 -m unittest discover -s tests -v`: اختبار منع تسريب IPC، منع عينات وضع/حساب مختلف، وجود الملفات بعد نقلها، واستبعاد البيانات الخام من ZIP.
- `npm --prefix apps/terminal test` و`cargo test --locked` يجب ألا يتراجعا.
- `scripts/Build-V25-Portable-Windows.ps1`: يثبت الوجود البنيوي وبدء Core الآمن، ويبني الأرشيف ببصماته. نجاحه **لا يثبت تشغيل جلسة MT5 Demo حقيقية**.

**الأولوية التالية:** إثبات G2 على Demo حقيقي، ثم P0-D فحص 22 صفحة وP0-E ترجمة/اتصال Native EA، مع الالتزام بالخطة الأصلية ومراجعة العقول والتعلم بعد ذلك.
