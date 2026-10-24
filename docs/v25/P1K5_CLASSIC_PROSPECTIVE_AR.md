# Enterprise V25 — P1-K5A Sniper Classic prospective research

المرجع: P1-K4 Commit `e530b2a0`. التعديلات في فرع `feature/v25-p1k5-classic-prospective-research-20261024` منفصل، ولا تغيّر `main` ولا V24.

## ما تمت إضافته

- مسار Rust مستقل `sniper_classic_forward.rs` لمراقبة القرارات **على لقطات MT5 الكاملة اللاحقة للتجميد فقط**. Sniper Classic يعتمد على تحليل السوق وبيانات M1، وليس له نفس عقد تيكات Sniper Learner.
- أزرار بدء/إيقاف الدراسة في Windows Sniper، من خلال IPC محدود، وCore يرفض البدء إذا لم يكن الحساب Demo موثقاً ومتطابقاً، الوضع Classic، التداول STOP، أو كانت هناك نوايا Broker غير محسومة.
- يتم تجميد إعدادات Classic؛ أي تغيير سياسة، وضع أو حساب يوقف الدراسة. يُستبعد تكرار معرّف الحدث، ووقت الوسيط السابق للتجميد، وغياب بيانات M1/Market. لا يتم إرسال أي أمر وسيط أو تسجيل أرباح Paper أو تعديل نتائج التعلم.
- تظهر فقط أعداد اللقطات المستقبلية، قرارات BUY/SELL/WAIT الخام، ونقص مصدر M1. الحساب الآخر أو Real لا يرى نتائج الحساب السابق. التقرير في ذاكرة Core ويضيع بإعادة التشغيل.
- `sniper_frozen_tick.rs` الخاص بـLearner محفوظ مستقلاً، ولا نخلط الـFast Pulse Tick مع دراسة Classic.

## قيود الدليل

لا يُعد هذا مقارنة Tick كاملة أو OOS مستقلة، لأن Classic يستند إلى تاريخ M1 الموجود أصلاً عند بداية الرصد، ولأن الرصد يعتمد على لقطات Core لا كل أحداث MT5. `independent_oos=false` و`broker_pnl=null` و`strategy_win_rate=null` افتراضياً. لا يوجد ترقية نموذج ولا ادعاء تفوق.

## بوابة قبول P1-K5A

- Rust `cargo test --locked -p braintrade-domain -p braintrade-runtime` يتضمن حالات Demo/REAL، Classic/Learner، تبديل الإعدادات والحساب، عدم backfill أو تصنيع أرباح.
- React `npm --prefix apps/terminal test` و`npm --prefix apps/terminal run build` وعزل حساب الشاشة.
- Gateway Python suite، Tauri Windows + Portable والتحقق من مطابقة Git SHA.
- MetaEditor EA، ربط MT5 Demo، Click-through واجهة Windows، بيانات Broker ذات مدة كافية، فصل تعلم/OOS زمني مستقل: **ما زالت غير منفذة**، فلا تداول REAL اعتماداً على هذه المرحلة.

## التالي

P1-K5B: بناء فصل تدريب وتقييم زمني مستقل للـLearner، مع عينات المستقبل بعد التجميد، وتحديد متطلبات الحد الأدنى للبيانات وتكلفة الدخول والخروج قبل مقارنة الأداء. يجب أن يكون تفعيل أي سياسة تداول جديدة قراراً منفصلاً، وليس نتيجة تلقائية لزيادة عدد الإشارات.
