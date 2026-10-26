# BRAINTRADE V25 — G7-F | Candidate مستقل وثابت داخل Rust Core

## ما الذي تغيّر؟

أضفنا إلى Rust Core `crates/runtime/src/g7_sealed_candidate.rs`، وهو مسار تجريبي **لا يرسل أي أوامر MT5**، ولا يتدخل بإدارة المخاطر واللوت والخروج.

لم نجمّد عقل Adaptive الحيّ، لأن ذلك سيغير سلوك التداول الأساسي دون اختيار المستخدم. بدلاً من ذلك، أنشأنا **نسخة Candidate مستقلة** يمكن ختمها بعد مرحلة التدريب؛ تُقيّم الشموع المستقبلية بنفس دالة تقييم Frozen ولكن باستخدام أوزانها وإعداداتها المختومة، وليس بيانات التعلم اللاحقة. يمكن إذن مراقبة ثلاثة مسارات منفصلة:

- **Frozen baseline:** نسخة السياسة المجمدة عند بدء G7.
- **Sealed Candidate:** نسخة مختومة صراحة عند بداية الاختبار، بدون تحديثات مستقبلية؛ لا أوامر Broker.
- **Online Adaptive:** سياسة التداول الحالية التي يمكن أن تستمر بالتعلّم والتنفيذ في Demo حسب إعدادات المستخدم؛ **ليست مختومة**.

أي اختلاف بين هذه الإشارات ليس بالضرورة اختلاف أرباح؛ Frozen وCandidate لا ينفذان صفقات وساطة فعلية.

## استخدام مسار Core الموجود

بعد `freeze` وقبل **أي قرار M1 جديد لنفس الدراسة**، وعندما تكون المنصة على STOP والأوامر معلقة قد سُوِّيت، والحساب Demo متصل وحديث:

```http
POST /v1/execution/frozen-coordinator
Content-Type: application/json

{"action":"seal_candidate","context":"THE_EXACT_CONTEXT_FROM_CORE","mode":"expert"}
```

يجب إرسال السياق الأصلي الذي تعرضه Core، ولا تغيره يدوياً. يقبل الوضع `expert` أو `scalping` أو `swing` وفق الوضع الحالي. مسار Sniper لا يمكن تقييمه بدقة باستخدام M1 ويحتاج دراسة Tick منفصلة.

إجراء `seal_candidate` يفحص الهوية وDemo والاتصال ووضع STOP، ويربط نسخة Candidate بـ `run_id` الحقيقي لدراسة G7 الأصلية. يرفض محاولة الختم بعد قرار مستقبلي مسجل لنفس الدراسة، ويمنع ختم نفس الدراسة مرتين، ويطابق SHA-256 الأصلية لنسخة Frozen.

بعد الختم، `GET /v1/execution/frozen-coordinator` يحتوي حالة `sealed_candidate` وعدد القرارات المستقبلية. عند الرصد، تكتب Core إلى `g7f_sealed_decisions` إشارة Candidate المختومة وقرار Adaptive الملاحظ مع البصمات وتوقيت الشمعة. لا ينشئ هذا المسار أوامر تداول جديدة أو يُمرّر الإشارة إلى المتعلم.

## تدقيق الملفات الأصلية

على قاعدة Core الأصلية، نفذ:

```bash
python scripts/g7f_sealed_candidate_audit.py --db "C:/actual/path/core.sqlite" --output "g7-f-source-audit.json"
```

يفحص هذا الملف G7-A أولاً، ثم حساب الدراسة والتاريخ والبصمة المختومة، وتطابق جميع إشارات Candidate المستقبلية مع قرارات G7 الأصلية، ويرفض الصفوف الناقصة أو المعدلة أو المتلاعب فيها حتى لو أعيد احتساب بصمتها المحلية بشكل انتقائي.

تقرير G7-E يضيف الآن:
- `sealed_candidate_replay_integrity`: سلامة مسار الإشارات المحلية.
- `sealed_candidate_is_read_only_shadow`: يبيّن أن Candidate منفصلة عن أوامر Broker.
- `adaptive_policy_sealed=false`: تذكير مقصود بأن Adaptive الفعلي لم يُجمد.

## حدود إثبات التعلم

هذا فصل برمجي **لقرارات Candidate فقط** عن التعلّم الجاري، وليس إثباتاً أن Adaptive لم يتلقَّ أي تحديث أو أن جميع مسارات التدريب أصبحت ثابتة. فحوص SHA محلية؛ لا تحل محل ختم توقيت خارجي أو السجل الأصلي المستقل من الوسيط. المقارنة لا تزال Frozen/Candidate قرارات غير منفذة مقابل Adaptive، ولا توجد تجربة A/B Broker عشوائية.

تبقى إحصاءات G7-E وصفية، ولا تسمح لأي خطوة بإطلاق REAL أو اعتماد نموذج جديد آلياً. لا تغيير في التداول أو المخاطر أو `main`.

## اختبار Rust وPython

```bash
cargo test -p braintrade-runtime --lib g7_sealed_candidate --locked
python -m unittest discover -s tests -p "test_g7f_sealed_candidate_audit.py" -v
```

قبول المرحلة برمجياً يقتضي نجاح جميع فحوص G7 وResearch وWindows لنفس SHA، وأي ادعاء بنتائج مالية فعلية يحتاج MT5 Demo وبيانات مستقبلية حقيقية. دراسة Candidate المكتملة يجب أن تظهر حالات Missing/Tampered بصراحة دون ادعاء أرباح لها.
