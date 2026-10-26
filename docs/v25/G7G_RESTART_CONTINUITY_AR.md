# BRAINTRADE V25 — G7-G | بقاء سياسة Candidate وسجل الأدلة بعد إعادة تشغيل Core

## ماذا أصلحنا؟

في G7-F كنا نسجل زوج الإشارتين Frozen/Adaptive أولاً، ثم نحفظ إشارة Candidate المختومة في أمر SQLite ثانٍ. لو انقطعت العملية أو فشل تحقق Candidate بين العمليتين، قد يظهر **قرار G7-A محفوظ من دون Candidate**. هذا خلل استمرارية بحثي وليس صفقة حقيقية؛ لكنه قد يوهم التقرير أن بيانات الاختبار متكاملة.

في G7-G صارت كتابة الثلاثة مرتبطة بمعاملة SQLite ذرية واحدة:

1. إدخال `frozen_coordinator_decisions`.
2. إدخال `g7_paired_decision_evidence` ببصمة المصدر نفسها.
3. إدخال `g7f_sealed_decisions` بعد حسابها من **نسخة Candidate المحفوظة، لا من أوزان التعلّم الجديدة**.

نجاح الكل يعني `COMMIT` واحد؛ فشل أي جزء يلغي المعاملة البحثية كلها. مسار `execution::Service::observe` لا يزال يفصل أخطاء البحث عن تسوية أوامر الوسيط وإدارة المراكز. لا يتعمد النظام ملء أي شمعة تاريخية مفقودة بنتيجة لاحقة.

كذلك تغير `INSERT OR IGNORE` إلى `INSERT` الصارم في سجل Candidate: أي محاولة كتابة ثانية لنفس `run_id + bar_time` أصبحت خطأً، لا تجاهلاً صامتاً. وعرض `sealed_candidate` يبيّن الآن:

- `expected_future_source_pairs`: عدد الشموع المؤهلة بعد ختم Candidate.
- `future_candidate_decisions`: عدد إشارات Candidate المحفوظة فعلاً.
- `missing_future_candidate_receipts`: الفرق عند نقص الأدلة.
- `research_evidence_complete`: هل السجلات المحلية متكاملة، **وليس** هل النموذج رابح.

## فحص الإقلاع والاستمرارية

أضفنا اختبار Rust يفتح SQLite في ملف حقيقي، يحفظ نسخة Candidate، يغلق اتصال Core، يفتح قاعدة البيانات من جديد، يرصد شمعة مستقبلية، يغلق ويفتح مرة أخرى ويتحقق من:

- بقاء بصمة Candidate دون تغيير.
- بقاء إشارة الشمعة نفسها دون استنساخ أو ازدواج.
- عدم إدخال نتيجة جديدة عند إعادة إرسال نفس الشمعة.
- عدم تعويض دليل مفقود سابقاً بصفقة أو قرار مُختلق.

توجد أداة Python جديدة: `scripts/g7g_restart_continuity.py`.

### 1) أخذ نسخة SQLite متناسقة قبل إيقاف Core

```powershell
python scripts/g7g_restart_continuity.py snapshot --db "C:\actual\path\core.sqlite" --output "g7-before.sqlite"
```

الأداة تستخدم SQLite Backup API، وبالتالي تأخذ نسخة متناسقة **تشمل آخر التغييرات المحفوظة في WAL**، بخلاف نسخ ملف قاعدة البيانات يدوياً أثناء التشغيل.

### 2) إعادة تشغيل Core ثم أخذ نسخة ثانية

```powershell
python scripts/g7g_restart_continuity.py snapshot --db "C:\actual\path\core.sqlite" --output "g7-after.sqlite"
```

### 3) مقارنة النسختين

```powershell
python scripts/g7g_restart_continuity.py verify --before "g7-before.sqlite" --after "g7-after.sqlite" --output "g7-restart-report.json"
```

تقرأ الأداة قاعدتي البيانات دون تعديلهما، وتشغّل تدقيق G7-A وG7-F على كل منهما ثم تقارن، لكل دراسة مختومة، سجلات تسجيل الدراسة وSnapshot الأصلي وجميع قراراتها المحفوظة **حقلاً بحقل**. تقبل فقط إضافة قرارات مستقبلية أحدث من آخر شمعة مسجلة قبل إعادة التشغيل. وترفض:

- حذف قرارات قديمة أو تعديلها حتى لو أعيد حساب SHA المحلي.
- تبديل سياسة Candidate أو زمن ختمها أو معرف الحساب/الوضع.
- فقدان إشارة Candidate مقابل قرار G7 أصلي.
- إدخال شموع أقدم اصطناعياً بعد إعادة التشغيل.
- استخدام قاعدة لا توجد فيها سياسة Candidate مختومة أصلاً على أنها استمرارية ناجحة.

> **قيد علمي مهم:** تطابق النسختين يثبت الاستمرارية المحلية بين ملفين، لكنه لا يثبت بشكل مستقل أن العملية أُعيد تشغيلها فعلاً، أو أن بيانات الأسعار لم تنقطع أو أن الوسيط نفذ صفقة مربحة. لا يوجد تنفيذ Broker لذراعي Candidate وFrozen، ولا شهادة OOS أو موافقة REAL.

## أوامر الاختبار

```powershell
cargo test -p braintrade-runtime --lib g7_sealed_candidate --locked
python -m unittest discover -s tests -p "test_g7g_restart_continuity.py" -v
```

لا تغيّر هذه المرحلة اللوت، الستوبات، حجم المخاطرة أو مسارات التنفيذ، ولا تدمج أي فرع في `main`. اعتبر المرور على اختبارات GitHub شرطاً هندسياً ضرورياً وليس إثباتاً للربحية. يلزم تشغيل MT5 Demo فعلياً عند توفر جهاز ويندوز للحصول على أدلة تنفيذ سوق مستقلة.
