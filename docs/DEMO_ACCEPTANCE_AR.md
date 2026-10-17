# فحص MT5 Demo الحقيقي وقياس سرعة القناص

هذا الفحص يُشغّل على جهاز Windows الذي عليه MT5 وحساب **Demo** حقيقي. الاختبارات الآلية في GitHub لا تستطيع إثبات زمن الوسيط أو أن منصة وسيطك قبلت الصفقة فعلياً.

الأداة الأساسية داخل الحزمة هي `runtime\gateway\check_runtime.py`. وضع القبول **قراءة فقط**: يتصل بـCore المحلي، يقرأ سجل التنفيذ الموجود، ولا يرسل أي POST ولا يغيّر START/STOP ولا يربط حساباً ولا يفتح/يغلق صفقة.

## الطريقة الأسهل داخل الحزمة — Acceptance Center

بعد فك ضغط BrainTrade ستجد في المجلد الرئيسي:

- `Run-Demo-Acceptance.cmd` — دبل كلك يفتح نافذة **Demo Acceptance Center**.
- `Tools\Run-Demo-Acceptance-GUI.ps1` — واجهة Windows نفسها.
- `Tools\Run-Demo-Acceptance.ps1` — CLI المتقدم للمشغّل القراءة فقط.
- `DEMO_ACCEPTANCE_AR.md` — هذه التعليمات داخل الحزمة.

شغّل `BrainTrade.exe` أولاً، اربط حساب MT5 الحالي كـDemo، وبعدها افتح `Run-Demo-Acceptance.cmd` بدون أي arguments. ستظهر نافذة فيها:

- اختيار `Native` أو `Poll` أو `Auto`.
- مدة قياس 5 / 15 / 30 / 60 دقيقة.
- الحد الأدنى المطلوب لعينات Sniper.
- خيار Reset لبدء عينة جديدة.
- بطاقات مباشرة لحالة القبول وعدد العينات و`queue_to_send` P95/P99 و`send_ms` P95.
- مقارنة عدد/P95 عينات Native وPoll.
- التشخيص وأكبر hotspot محلي والتحذيرات.
- زر لفتح مجلد النتائج.

عند اختيار `Native` تذكّرك النافذة أن يكون `BrainTradeSniperBridge` مربوطاً على الشارت ويعرض `CONNECTED`. وعند اختيار `Poll` تذكّرك بفصل/إيقاف الـEA حتى تكون المقارنة مضبوطة.

النافذة **لا تشغّل التداول ولا تفعّل START**. هي تشغّل `Tools\Run-Demo-Acceptance.ps1` كعملية منفصلة للقراءة فقط ثم تقرأ ملف JSON الناتج. إذا كان BrainTrade يتداول أثناء القياس فهذا لأنك أنت شغّلت النظام بصورة طبيعية مسبقاً؛ نافذة القياس لا تصنع إشارة ولا ترسل صفقة.

إذا مررت arguments إلى `Run-Demo-Acceptance.cmd` فإنه يحافظ على السلوك القديم ويفتح CLI مباشرة. مثال:

```bat
Run-Demo-Acceptance.cmd -Source native -WatchSeconds 3600 -MinimumFastSamples 200 -NonInteractive
```

وللاستخدام من PowerShell مباشرة:

```powershell
.\Tools\Run-Demo-Acceptance.ps1 -Source native -WatchSeconds 3600 -MinimumFastSamples 200 -NonInteractive
```

ولبدء عينة جديدة بعد تغيير الإصدار أو الحساب:

```powershell
.\Tools\Run-Demo-Acceptance.ps1 -Source native -WatchSeconds 3600 -MinimumFastSamples 200 -Reset -NonInteractive
```

## قبل الفحص

1. افتح `BrainTrade.exe` واختر `terminal64.exe` الصحيح.
2. تأكد أن MT5 داخل حساب Demo المقصود وأن Algo Trading مسموح.
3. من BrainTrade اربط الحساب الحالي كـDemo، واضبط الرمز الصحيح والعمولة.
4. لو تريد قياس القناص، اختر Sniper وشغّل النظام بصورة طبيعية. عدم وجود إشارة صالحة يعني WAIT وهذا طبيعي؛ أداة القياس لا تصنع إشارة وهمية ولا تجبر النظام على التداول.
5. لا تحوّل التنفيذ إلى Live أثناء جلسة القياس. الأداة ترفض Live وتوقف الجمع إذا تغير الحساب/السياق.

## جلسة Native OnTick يدوياً

اربط `BrainTradeSniperBridge.mq5` على شارت نفس الرمز وتأكد أنه يعرض `CONNECTED`. افتح CMD من مجلد BrainTrade وشغّل:

```bat
runtime\python\python.exe runtime\gateway\check_runtime.py --demo-acceptance --source native --watch-seconds 3600 --minimum-fast-samples 200
```

يمكنك إيقاف الجمع بـCtrl+C. العينات المحفوظة لا تضيع، وتشغيل الأمر مرة أخرى على نفس الإصدار والحساب يكمل عليها.

## جلسة fallback polling 5ms يدوياً

افصل الـEA أو أوقفه أولاً، ثم شغّل:

```bat
runtime\python\python.exe runtime\gateway\check_runtime.py --demo-acceptance --source poll --watch-seconds 3600 --minimum-fast-samples 200
```

وسم `native` أو `poll` هو وسم جلسة يحدده المشغّل. الأداة لا تدّعي أنها تعرف حالة الـEA من خارج عملية gateway؛ لذلك لا تسم جلسة Native إلا والـEA متصل، ولا تسم جلسة Poll إلا بعد فصله/إيقافه.

## أين النتائج؟

داخل مجلد BrainTrade يظهر افتراضياً:

- `acceptance-output\demo-acceptance-latest.json`
- `acceptance-output\demo-acceptance-latest.csv`
- `acceptance-output\demo-acceptance-state.json` — حالة تراكمية محلية حتى تكمل عدة جلسات.
- `acceptance-output\runtime-import-check.json`.
- `acceptance-output\acceptance-gui-stdout.log` و`acceptance-gui-stderr.log` عند تشغيل الواجهة الرسومية.

التقرير لا يحفظ رقم Login؛ يستخدم بصمة قصيرة للحساب/الخادم/الرمز/العملة حتى لا يخلط عينات حسابين.

إذا غيرت إصدار BrainTrade أو الحساب، استخدم Reset من النافذة أو `--reset` في collector أو `-Reset` في PowerShell لبدء حالة قياس جديدة.

## ما الذي يقيسه؟

لكل أمر مسجل يمكن أن يحفظ:

- `core_pulse_ms`
- `queue_to_gateway_ms`
- `authorize_ms`
- `journal_ms`
- `other_pre_send_ms`
- `pre_send_ms`
- `queue_to_send_ms`
- `send_ms`
- `total_ms`
- السبريد عند إرسال القناص إذا كان موجوداً في سجل التسعير
- `retcode` وحالة نتيجة الوسيط
- هل كانت Fast Path
- هل تم تجاوز `order_check` في Tick Sniper فقط

ثم يحسب P50/P95/P99 ويفصل عينات Native عن Poll.

## معنى هدف 20ms

الهدف الهندسي هو `queue_to_send_ms <= 20ms` على جهازك قدر الإمكان. التقرير يعرض P95 وP99 ولا يعتبر `send_ms` جزءاً من هذا الهدف؛ `send_ms` يشمل استدعاء MT5 وانتظار إقرار الوسيط ويتأثر بالشبكة والوسيط.

إذا كان P95/P99 المحلي أبطأ من 20ms، راجع أكبر مكوّن قبل تغيير أي حد:

- `authorize_ms` مرتفع: اتصال Core/التفويض المحلي هو المسيطر.
- `journal_ms` مرتفع: كتابة SQLite المتينة هي المسيطرة؛ لا تُعطّل `synchronous=FULL` لأن ذلك يحمي من إعادة إرسال عمياء بعد crash/timeout.
- `other_pre_send_ms` مرتفع: قراءات MT5/الحساب/الصفقات ومطابقة العقد قبل الإرسال هي المسيطرة.
- `send_ms` مرتفع مع queue سريع: المشكلة بعد نقطة الإرسال المحلية، وغالباً MT5/الشبكة/الوسيط وليست Core queue.

## حالات التقرير

- `INCOMPLETE`: عدد عينات Sniper المرسلة أقل من الحد المطلوب.
- `TARGET_MET`: العينة مكتملة وP95/P99 لـqueue-to-send ضمن 20ms وعقود fast/regular/journal سليمة.
- `CHARACTERIZED_SLOW`: العينة مكتملة لكن P95 أو P99 أبطأ من 20ms؛ استخدم component timings لتحديد السبب.
- `CONTRACT_FAIL`: سلوك أساسي خالف العقد، مثل Fast Sniper لم يتجاوز order_check أو regular open تجاوزه أو غابت مدة journal من أمر وصل إلى send.

هذه النتائج تصف جهازك واتصال MT5 في تلك الفترة فقط، وليست ضماناً لزمن fill أو الربحية أو سرعة ثابتة عند الوسيط.
