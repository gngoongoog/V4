# BRAINTRADE V4 Enterprise V25 — تدقيق الحالة والبوابات / 2026-10-25

**المصدر المرجعي:** `BRAINTRADE_V4_V25_WORK_HANDOFF_AR.md`، فرع الرجوع `feature/v24-shortmode-history-gate-20261023`، فرع بناء وتجربة المستخدم الحالي `fix/v25-display-demo-balance-without-fresh-tick-20261025`. لا تغيير على `main` أو فرع V24.

## ما تم إثباته حديثاً

- GitHub Actions على Commit `2a95ea1afca1a168e84f47bea73db7f31137ce4c`: **Research PASS** (`40136612534`)، **Windows build PASS** (`40136612635`)، **V25 Windows Portable PASS** (`40136612533`). تشمل اختبارات تشغيل Rust Core المعزول مع Gateway خامد يتأكد من نقل IPC token وDATA_DIR ووقف إنشاء Bytecode. ليست نتائج جلسة تداول حقيقية.
- المستخدم جرّب Windows Portable على جهازه: ظهر `Rust Core ONLINE` و`MT5 Bridge CONNECTED` وحساب `DEMO`؛ الرصيد وEquity = 250 USD، وDaily P&L = 0 بحسب لقطة الشاشة. ذلك إثبات **لظهور لقطات الحساب/اتصال Gateway على جهازه** فقط، وليس اجتياز أوامر Broker.
- `Market Feed UNAVAILABLE` و`QUOTE STALE` في لقطة 2026-10-25. لا يمكن افتراض إغلاق السوق فقط؛ لقطة MT5 سابقة أظهرت `XAUUSD`، ولقطة إعدادات BRAINTRADE سابقة أظهرت `XAUUSDr`. اختلاف suffix قد يسبب انعدام بيانات رمز معين. لا يغيّر النظام الرمز تلقائياً.
- سلسلة PR #138-#150 موجودة في **فروع Draft متراكبة**، ولم تُدمج في `main`. أقسام Sniper Frozen K5A-K5J بحثية/تحليلية؛ بعضها اجتاز فحوص CI ولكنها **ليست شهادة حساب REAL ولا اختبار Forward مكتمل لدى الوسيط**.

## مطابقة التكليف إلى واقع التنفيذ

| الترتيب | البوابة / المرحلة | الحالة | الأدلة والفجوة |
| --- | --- | --- | --- |
| P0-A / G1 | Rust/Python/React/Tauri + Windows Portable | **DONE لاختبارات البناء؛ PARTIAL قبول المستخدم** | نجاح CI الثلاثي على آخر بناء، وتجربة المستخدم للـEXE والاتصال؛ يتطلب اختبار نقل البيانات والـGUI على جهازه |
| P0-B / G2 | أمر MT5 Demo: Tick → Open → Partial → Close → Deals/Fees | **PARTIAL برمجياً، NOT TESTED وساطة فعلية** | اختبارات SQLite/Mock/Replay وقاعدة عدم إعادة إرسال مجهول؛ لا إثبات من حساب Demo حيّ لكل السيناريوهات |
| P0-C / G4 | إدارة المخاطر والإغلاق/الانقطاع | **PARTIAL** | قواطع وأوضاع الحسابات الصغيرة قابلة للضبط؛ لا يوجد اختبار تحمّل Windows Demo متعدد الأيام مع مراكز مفتوحة |
| P0-D / G3 | صفحات Windows الـ22 وجميع عناصرها | **PARTIAL** | عقود React + رندر Headless وأدوات تحقق؛ لا إثبات Manual/Automated WebView2 لجميع الأزرار |
| P0-E | Gateway/Native EA / توقيت الأسعار والفريمات | **PARTIAL** | أصلحنا بيئة Gateway والرصيد؛ Tick EA/MetaEditor وقياسات تأخير وسيط فعلية لم تثبت، وهذا PR يضيف تشخيص الرمز/حداثة السعر |
| P1 / G5 | عقول متعددة، هوية النافذ، attribution والتعلم | **PARTIAL** | شيفرة أكثر من 17 هوية/دور واستقرار سجلات؛ بيانات Broker الحية الكافية لكل عقل وOOS مستقل ليست مثبتة |
| P1-K4/K5A..K5J | Sniper Classic/Learner والأرشيف والتكلفة والتقييم | **DONE كتنفيذ بحثي، PARTIAL كأدلة سوق** | PR #138-#148؛ Freeze وSHA محلي ومقارنة زمنية واتجاهية **لا تثبت Win Rate ولا PnL** |
| P1 / G6 | Scalping/Swing/Expert Forward Demo مقارنة ببدائل بعد التكاليف | **NOT TESTED اعتماداً نهائياً** | توجد منطق واستراتيجيات واختبارات صناعية، لكن لا تكفي للمقارنة الإحصائية التشغيلية |
| P2 | أخبار، Telegram، تقارير وBackup/Recovery وتقييم جودة التنفيذ | **PARTIAL** | خدمات موجودة، لكن الصحة التشغيلية الشاملة في Windows Demo تحتاج أدلة مستقلة |
| G7 | ZIP/EXE/اختصار وSHA | **DONE لآخر بناء CI، PARTIAL قبول تشغيل كامل** | المستخدم شغّل EXE، وCI تحقق من الملف والحزمة؛ مطلوب فحص كامل بعد نقل/تحديث بيانات المستخدم |

## العيوب المفتوحة حسب الأولوية

- **P0-E2 (هذه الخطوة):** حالة `Market Feed UNAVAILABLE` كانت مبهمة مع حساب Demo متصل؛ نميّز بين `SYMBOL_MISSING`, `NO_QUOTE`, `UNVERIFIED_CLOCK`, `STALE_QUOTE`, وحالة Core/MT5 OFFLINE. مثال اختلاف XAUUSD مقابل XAUUSDr يستدعي توجيهاً للإعدادات لا تعديلاً تلقائياً. الحساب يبقى مرئياً والتداول يظل محظوراً دون Tick موثوق.
- **P0-B / G2:** دليل Win Demo مباشر لصفقة Order/Deal/History بالرفض/الانقطاع/الإغلاق الجزئي والتكاليف؛ **NEEDS USER WINDOWS/MT5 Demo**، أداة `Run-Demo-Acceptance.ps1` و`gateway/check_runtime.py --demo-acceptance` موجودتان للقراءة فقط.
- **P0-E:** إثبات `BrainTradeSniperBridge.mq5` على MetaEditor وتسجيل Native Tick مقابل Poll مع زمن وتسلسل موثق؛ **NOT TESTED على منصة المستخدم**.
- **P0-D / G3:** توثيق استجابات الأزرار والصفحات الـ22 بشكل مستقل على WebView2 ونقاط ERROR/EMPTY/RECONNECT.
- **P0-D3:** لقطة الحساب الحالية تبين `Margin —` بدل قيمة Broker صريحة؛ تدقيق مصدر `margin` وحالة عدم توفره يجب أن يكون إصداراً منفصلاً، ولا يجوز اشتقاقه من الرصيد وFree Margin كحقيقة وسيط.
- **P1 / G5,G6:** تقييم متعدد الجلسات ومستقل مقارنة Forward Demo مع خط أساس والتكلفة، لا ترقية صامتة للأبحاث إلى REAL.
- **Release:** مراجعة تكامل الفروع المتراكبة وتصفية حالات PR Draft قبل الدمج بموافقة المستخدم، مع نسخة V24 مرجعية قابلة للاسترجاع.

## هذه المرحلة الصغيرة P0-E2

الملفات: `apps/terminal/src/market-quote-diagnosis.ts`، `types.ts`، `terminal.tsx`، `system-health.tsx` واختبارات `market-quote-diagnosis.cjs` و`health.cjs`. تستفيد من `telemetry.error` الموجود في Gateway دون تعديل BrokerExecutor أو إرسال صفقات.

**معايير القبول:** Node + TypeScript + Windows Portable + Research/Windows CI على HEAD للفرع، وعدم تغيير التداول عند أسعار قديمة أو Symbol غير موجود. **ليس** اختبار Market Open أو التحقق من مصدر أسعار المستخدم بشكل حي؛ هذا يحتاج لقطات Demo عند افتتاح السوق.

## الترتيب التالي بعد نجاح هذه الخطوة

1. P0-D3: حقول رصيد الحساب/Free Margin/Used Margin/Quote Age/Drawdown بمصدر Broker واضح، مع غياب قيم غير موثوقة.
2. G2/P0-B: تشغيل Acceptance Runner يقرأ أدلة حساب Demo المنفصلة بالقبول/الرفض/الإغلاق الجزئي والاستعادة وإغلاق السوق؛ لا تنفذ صفقات من أداة الجمع.
3. P0-E Native EA والمصدر/التوقيت، ثم مراجعة واجهة Windows الـ22 بالصور وتقارير JSON/HTML.
4. G5/G6 وبقية تحسينات العقول والأوضاع وفق أدلة بيانات كافية، وبعدها حزمة V25 النهائية مصدر + EXE + تقرير بوابات.

**لا اعتماد REAL ولا صفقات فعلية أثناء التطوير.**
