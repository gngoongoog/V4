# V25 P0-E — دليل Native Sniper EA / MetaEditor (عربي)

## الحالة الحقيقية
تم تعديل مصدر EA `mt5/BrainTradeSniperBridge.mq5` ومطابقته مع Python Gateway وRust Core. اختبارات CI المجرّدة لا تثبت نجاح MetaEditor على جهازك، ولا تضمن وصول OnTick الحقيقي، ولا تثبت سرعة وسيط أو ربحية. `ea_metaeditor_compile=NOT TESTED` لحين الترجمة الحقيقية في Windows مع MT5.

## سبب إصلاح الحساب والجلسة
سابقاً كان `g_cursor_msc / g_cursor_count` يتابع منذ آخر Tick؛ عند تبديل حساب/خادم/عملة/وضع MT5 بسرعة، أو تدوير رمز الاتصال، قد يبقى المؤشر مرتبطاً بالجلسة السابقة. أصبح `EnsurePipe()` يراجع `ACCOUNT_LOGIN / ACCOUNT_SERVER / ACCOUNT_CURRENCY / ACCOUNT_TRADE_MODE` قبل استعمال الـPipe؛ ويغلق الجلسة القديمة ويصفر مؤشر CopyTicks عند تغيّر الهوية أو تدوير الرمز `sniper.token`. لا يوجد إرسال أوامر تداول من EA.

وأصبح Python Gateway، قبل أي Pulse ناتج من Native **أو** fallback Poll، يقرأ هوية حساب MT5 الفعلية ويقارنها بلقطة Core الموثقة. عند اختلافها يمنع إرسال Pulse من بيانات قديمة ويطلب تحديث Snapshot كامل. ويبقى تحقق Core النهائي وGateway Send Once لازماً.

## محتويات V25 Portable
- `mt5/Experts/BrainTradeSniperBridge.mq5`: مصدر EA فقط.
- `Compile-Sniper-EA.cmd`: فحص افتراضي؛ لا ينسخ أو يترجم دون اختيار المسارات.
- `Tools/Compile-Sniper-EA.ps1`: يأخذ المسار الصريح لـ`metaeditor64.exe` ومجلد **MT5 Data Folder** (افتحه من MT5 > File > Open Data Folder).
- `acceptance-output/native-ea/metaeditor-compile.json`: نتيجة فحص أو ترجمة، مع قيمة `PASS/FAIL/NOT TESTED`. وجود EX5 قديم لا يكفي.
- `Run-Demo-Acceptance.cmd`: قياس read-only للأدلة بعد توصيل Demo.

## طريقة الترجمة على Windows، بعد توفر MT5
1. افتح MetaTrader 5 المطلوب على حساب **DEMO**، ثم `File > Open Data Folder` وانسخ المسار.
2. افتح `Compile-Sniper-EA.cmd` مرة واحدة: يعرض `InspectOnly` وشرح المسارات؛ لا يغيّر MT5.
3. افتح PowerShell داخل مجلد Portable وشغّل، مع استبدال المسارات بمساراتك الفعلية:

```powershell
.\Tools\Compile-Sniper-EA.ps1 -MetaEditorPath "C:\Program Files\YourBroker MT5\metaeditor64.exe" -TerminalDataDir "C:\Users\You\AppData\Roaming\MetaQuotes\Terminal\YOUR_TERMINAL_ID"
```

4. يحتفظ السكربت بنسخة احتياطية من مصدر MQ5 مختلف قبل استبداله، ويطلب MetaEditor ترجمة `MQL5\Experts\BrainTradeSniperBridge.mq5`. ثم يتأكد من ظهور `BrainTradeSniperBridge.ex5` **جديد الحجم والتاريخ** ويحفظ SHA-256.
5. إذا ظهر `FAIL` راجع `acceptance-output/native-ea/metaeditor.log`، ولا تعتبر الـEA جاهزاً. MetaEditor ليس ضمن Portable ولا يمكن تعويضه بمحاكاة.
6. بعد `PASS`، من MT5 > Navigator > Expert Advisors قم بإضافة EA إلى شارت رمز الذهب المطابق لإعداد BrainTrade. تأكد أن `CONNECTED` يظهر على الشارت. هذا يعني اتصال Pipe فقط، لا يعني وجود صفقة أو تشغيل START.
7. افتح BrainTradeNext.exe، اربط Demo الصحيح، وجمّع نتائج Native وPoll بشكل منفصل من `Run-Demo-Acceptance.cmd`. وضع Poll يتطلب فصل EA، أما Native يتطلب ثبوت `CONNECTED`.
8. اختبر تبديل حساب Demo ثم الرجوع أثناء عدم وجود أوامر غير محسومة، وراقب رفض Tick قديم. لا تُجر هذا التغيير أثناء وجود صفقات حقيقية أو أوامر Pending.

## معايير قبول P0-E
- `MetaEditor compile`: **NOT TESTED** ما لم يُسجل EX5 جديد عبر MetaEditor.
- `Native pipe account isolation`: اختبارات Python/MQL5 مصدرية فقط حتى نكررها عملياً على Windows مع MT5.
- `Native vs Poll P95`: لا يمكن مقارنتها إلا من عينات OnTick موثقة على جهازك.
- `MT5 Demo lifecycle`: مستقلة عن كل ما سبق؛ تحتاج إرسال مرة واحدة، تذكرة وسيط، تأكيد Deal، إغلاق ومصاريف، ومصالحة `UNKNOWN` بلا إعادة إرسال.
- `REAL ready`: **لا**؛ النجاح الهندسي وحده ليس ترخيصاً للتداول الحقيقي.

## لا تفعل
لا تنسخ EX5 قديم وتعتبره Compile ناجحاً، ولا تعطل حماية الحساب أو الكتابة المتينة للـSQLite لتحقيق 20ms، ولا تشارك `sniper.token` أو `ipc.key` أو بيانات الحساب.
