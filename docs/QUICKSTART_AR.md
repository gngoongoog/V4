# BRAINTRADE OS — دليل التطوير الحالي

المشروع حالياً ضمن **مرحلة بحث 0.4.0**، بينما رقم Cargo وحزمة Windows التجريبية ما زال **0.3.0** إلى أن يتم تحديث الإصدار بشكل رسمي. آخر حزمة Windows معلنة للمستخدم تبقى 0.2.0 Preview.

## التشغيل وربط MT5

من الإعدادات اختر ملف `terminal64.exe` لمنصة وسيطك، وسجّل الدخول داخل MT5 أول مرة. البرنامج يستخدم جسر MT5 للقراءة فقط: الحساب، الصفقات/الأوامر، الأسعار، الشموع، التاريخ، وMarket Book إذا كان الوسيط يوفره.

**BrainTrade لا يفتح أو يعدل أو يغلق صفقات MT5 حالياً.**

## الصفحات والتحليل

طبقة التحليل الحالية تشمل Legacy Meta للمحاكاة المباشرة Shadow، إضافة إلى عقول/تحليلات رصدية مثل Trend وRegime وStructure وVolatility وMeta v2 وGann ومناطق/إعدادات Market Opportunities وDOM الخاص بالوسيط وسياق Macro/News.

هذه العقول الجديدة لا تستبدل Legacy Meta في Shadow ولا تتجاوز إدارة المخاطر.

## الاختبار التاريخي

المختبر يدعم:

- Meta v1
- Meta v2 candidate
- CES v1
- KTE v1

يحفظ History والافتراضات والنتيجة معاً، ويدعم إعادة التشغيل Offline والتجارب على نفس البيانات. KTE يدعم TP1/TP2/TP3 وسجل fills/stop updates/targets. تقرير HTML يحتوي مراجعة شموع لكل صفقة بدون جلب شارت خارجي.

هناك أيضاً تقييم زمني Chronological Evaluation على بيانات محفوظة ثابتة لـMeta v1/v2 باستخدام holdout/rolling windows. هذا للتقييم البحثي وليس تحسين تلقائي أو اختيار نموذج رابح.

## Learning Foundation

النظام يسجل features من شموع مكتملة، وينتظر labels مستقبلية دقيقة 5/15/30 دقيقة بدون اختلاق بيانات ناقصة. يمكن تصدير dataset وتدريب baseline خطي خارج Core، ثم تسجيله كـchallenger فقط.

بعد توفر الأدلة المطلوبة يمكن إنشاء سجل manual review eligibility، ثم تشغيل inference للـchallenger **للمراقبة فقط**. بعد نضج الـlabel الدقيق يتم تسجيل outcome وقياسات مثل accuracy وbalanced accuracy وmacro-F1 وlog loss وBrier/calibration.

لا يوجد Champion مفعّل، ولا auto-promotion، ولا تأثير للـML على Shadow أو المخاطر أو اللوت أو MT5.

## الأخبار وTelegram

الأخبار الرسمية تستخدم Fed/ECB RSS مع حالة freshness. هي سياق معلوماتي وليست إشارة BUY/SELL تلقائية.

Telegram للاستعلامات الخاصة فقط عند إعداد التوكن محلياً. لا ترسل التوكن داخل المحادثة أو GitHub. التحقق الفعلي بالتوكن الحقيقي على جهاز Windows ما زال ضمن acceptance اليدوي.

## النسخ الاحتياطي

النسخة الاحتياطية الحالية تشمل الإعدادات وقاعدة البيانات والبحث التاريخي والتقييم الزمني وسجلات Learning/Review/Predictions/Scoring، وتستبعد الأسرار. **Full Restore غير منفذ بعد**.

## ملاحظات البيانات

- Tick Volume داخل شموع MT5 هو نشاط عند الوسيط وليس حجم تداول بورصة عالمي.
- DOM/Market Book لعقد CFD خاص بالوسيط وليس دفتر أوامر الذهب العالمي.
- نتائج backtest أو ML أو CI لا تعني نسبة ربح مضمونة.

## حالة التحقق

راجع `BUILD_HANDOFF.md` و`docs/validation.json` لمعرفة الـcommit التنفيذي المختبر ونتائج Research/Windows CI. نجاح Windows packaging على GitHub لا يساوي acceptance على حسابك الحقيقي؛ اختبار اللابتوب + MT5 + Telegram يبقى منفصلاً.
