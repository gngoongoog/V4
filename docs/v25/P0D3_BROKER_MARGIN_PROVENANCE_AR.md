# P0-D3 — توثيق مصدر Used Margin في BRAINTRADE V25

## العيب
كانت واجهة V25 تعرض دائماً `Margin —` بغضّ النظر عن حساب MT5 Demo المتصل. الكود في `terminal.tsx` يستخدم شرطة ثابتة، بينما Gateway يجلب `Free Margin` ولا يقرأ `account.margin` من الوسيط.

## الإصلاح المحدود
- `gateway/mt5_gateway.py`: أخذ `margin` من `mt5.account_info().margin`؛ تقبّل `0.0` بوصفها قيمة حقيقية، واعتبر الغياب والسالب وNaN وInfinity **غير متاح**. لا تستنتج Margin من Equity - Free Margin ولا تستخدم `order_calc_margin` لأنه مقياس تقديري مختلف.
- `crates/domain/src/lib.rs`: حقل `Option<f64>` مع `serde(default)` للحفاظ على توافق لقطات البيانات الأقدم، والتحقق من عدم كون القيمة سالبة أو غير محدودة.
- `apps/terminal/src/types.ts` و`terminal.tsx`: عرض Margin من بيانات Broker الحديثة فقط، مستقلاً عن حداثة Tick السوق، تماماً مثل Balance وEquity. إذا غابت القيمة يظهر `—` وليس صفراً مفترضاً.
- لا تغير هذه المرحلة حجم لوت المستخدم أو رافعته أو قواعد الخطر أو إرسال MT5 للأوامر أو نماذج ML.

## اختبارات
- FakeMT5: Used Margin = 0.0 يظهر كما هو، مع `XAUUSDr` غير موجود/السوق بلا Tick؛ والحقل غير الصالح/الناقص يبقى null.
- Rust: رفض Margin السالب وNaN، وقبول صفر متحقق، وبقاء بيانات JSON القديمة بلا margin قابلة للقراءة.
- Node: contract داخل `account-balance-after-hours.cjs` يمنع رجوع الشرطة الثابتة ويطلب ربط Broker الحقيقي فقط.
- GitHub Research + Windows Native + V25 Portable على HEAD الفرع؛ التجربة الحقيقية على جهاز MT5 متبقية.

## حدود المرحلة
الـ**Max Drawdown** مختلف عن Margin: لا يمكن اشتقاقه من لقطة Equity واحدة. يحتاج سلسلة مؤرخة من Equity أو سجلاً موثقاً من Broker ويجب أن يعرض **غير مثبت** عند غيابها. كذلك `Margin` الحالي ليس مقياس المخاطر الكلي، ولا يثبت جاهزية إرسال الأوامر.

## التالي حسب الأمر الأصلي
اختبار `Run-Demo-Acceptance.ps1` على Windows مع حالة MT5 Demo وتزامن Gateway/EA، ثم اختبار سجل Broker الكامل للأوامر الجزئية والمجهولة، وفحص صفحات WebView2 الـ22. لا يستخدم هذا الإصدار حساب REAL ولا يتم اختباره بإرسال أوامر حقيقية.
