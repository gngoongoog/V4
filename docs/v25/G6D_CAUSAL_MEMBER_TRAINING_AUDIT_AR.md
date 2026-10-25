# BRAINTRADE V25 — G6-D: هل عقول Expert تتعلّم من عينات MT5 صحيحة فعلاً؟

## الغرض الحقيقي

تحتوي قرارات Frozen/Adaptive المسجلة على `adaptive.coordination.members`. كل عضو يتضمن معرف العقل وفريمه ونسخة التقرير والوزن وعدد `broker_samples`. هذه حقول تُستخدم بالفعل في قرار Coordinator وليست مؤشرات واجهة مستقلة.

أضفنا `audit_v25_g6d_causal_learning_evidence.py` لتدقيقها مع سجل `coordinator_broker_samples` بحسب المصدر الحقيقي الذي يولده Rust:
`[account_context, "adaptive-member-v1", "expert", brain_id, timeframe, report_id]`.

لكل قرار Expert بعد مهلة دراسة G6-A المسجلة مسبقاً يتحقق التدقيق من:
- أن القرار مسجل بعد انتهاء شمعة M1 ولم يستفد من وقت لاحق على نحو مصطنع.
- أن هوية العضو وفريمه وإصدار سياسته ثابتة في عملية المطابقة؛ وأن الوزن رقم موجب قابل للحساب وليس NaN.
- أن عينات Broker من حساب MT5 المطابق، ومركز ذي هوية صحيحة، و`closed_ms` مطابق، وأن Gross + Commission + Swap + Fee = Net.
- أن `risk_money` و`R-multiple` و`reward_v2` تتفق مع صافي الصفقة كما يتطلب G5-E وG6-B.
- أن **عدد عينات Broker الذي يذكره القرار** لا يمكن أن يتجاوز عدد عينات ذلك العضو المثبتة التي أُغلقت **قبل القرار أو عنده**، وليس بعده.
- تقارير منفصلة لعدد الأعضاء ذوي عينات حقيقية، وزيادات عدد العينات عبر الزمن، وعدد الأوزان التي اختلفت أول المدة عن آخرها، والعينات غير المؤهلة وأسبابها.

## لماذا لا نحتسب مجرد ارتفاع الوزن تعلماً ناجحاً؟

يستخدم Coordinator ظروف السوق والـRegime ووزن التنفيذ، وليس نتائج Broker فقط. تغير الوزن دون زيادة عدد عينات Broker قد يحدث بصورة طبيعية بسبب تغير ظروف السوق. لذلك:
- `member_sample_count_increase_observations` = رصد تغيّر كمية بيانات Broker المؤهلة التي يدّعي العضو استخدامها.
- `members_with_changed_first_to_last_displayed_weights` = اختلاف الوزن المسجل، **ليس** نسبة نجاح ولا إثبات أن التعلم تسبب بالتغيير.
- `claimed_broker_training_exceeds_prior_verified_receipts` = إنذار عن ادعاء عينات تفوق المصدر المتاح قبل القرار.

قاعدة SQLite قد تحتوي اليوم صفوفاً أُضيفت **بعد** القرار لكن بتاريخ إغلاق سابق؛ فهي لا تحفظ بالضرورة تاريخ الـINSERT الأصلي. لا يستطيع التدقيق إبطال تلاعب من هذا النوع دون سجل خارجي مختوم. لذلك الفحص يعطي **حداً أعلى محافظاً** للأدلة السابقة، لا إعادة تشغيل زمنية مؤكدة للمتعلم.

## ماذا عن Test OOS المستقل؟

حتى إن كانت كل الأعداد صحيحة، هذا لا يثبت أن نسخة Adaptive ثُبّتت عند حد تدريب مستقل؛ فالمنظومة تتعلم تدريجياً ويتغير الـRegime. التقارير تضبط دائماً:

- `training_cutoff_independently_enforced=false`
- `adaptive_policy_sealed_across_study=false`
- `local_db_insert_time_attested=false`
- `claim_that_weight_change_was_caused_by_learning=false`
- `independent_oos=false` و`model_promotion_allowed=false`
- `broker_pnl=null` في تقرير G6-D؛ الربح المنفذ فقط من G6-B ولا يوجد Broker Frozen comparator.

هذا يعني أننا أضفنا اختباراً فعلياً لصدق بيانات التعلم، وليس إعلاناً بنجاح استراتيجية الذهب.

## التشغيل

من نسخة Windows Portable اختر **5** في `Run-G6-Study.cmd` أو استخدم:

```powershell
.\Tools\Run-G6-Study.ps1 -Action Learning -Db 'C:\Path\To\core.sqlite' -Manifest 'C:\Path\To\g6a-study-manifest.json'
```

لا تُغيّر قاعدة بيانات Core؛ الأداة تقرأها فقط. إن لم تبدأ دراسة G6-A بعد ولم تجمع قرارات Expert المستقبلية، سترى أدلة غير كافية بدلاً من نسبة تعليم اصطناعية.

## الاختبارات وحدود اعتماد الإصدار

اختبارات Python تتضمن: مكافأة Broker مزورة؛ عينة أغلقت بعد القرار؛ زيادة عينات Broker حقيقية بعد cutoff؛ تغيّر الوزن دون عينات Broker؛ عضو مجهول أو وزن غير صالح؛ JSON معطوب؛ جدول Broker غير موجود. تُعاد اختبارات G6-A/G6-B. تُلزم عملية بناء Portable بوجود G6-D وتشغيل `--help` بالـPython المعزول المرفق، إضافة إلى Rust/Windows/Linux/Research CI.

لا يُسمح اعتبار النجاح الآلي بديلاً عن Demo حقيقي أو عن تدريب بنسخة مختومة ومجموعتي بيانات مستقلتين وقياس اقتصادي بعد تكاليف الوسيط.
