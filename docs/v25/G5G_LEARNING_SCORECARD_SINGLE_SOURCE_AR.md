# BRAINTRADE V25 — G5-G | وحدة مصدر الحقيقة بين تعلّم العقول وتقاريرها

## المشكلة التي جرى إثباتها

المساران read-only التاليان كانا يفحصان توازن gross + commission + swap + fee = net، لكن لا يتحققان من مطابقة reward وR-multiple مع net/risk_money:
- crates/runtime/src/learning_quality.rs — Brain Learning Quality.
- crates/runtime/src/sniper_policy_quality.rs — Sniper Policy Quality.

في G5-E أصبح المنسق والقناص يرفضان reward غير مطابق لحساب الصفقة، بينما التقرير كان يمكنه الاستمرار بإظهار نفس الصف ضمن عداد broker_samples أو closed_demo_positions. يُفقد ذلك الاتساق بين عينات التعلم والنتائج المعروضة.

## التعديل

- يمر كل سجل مؤهل عبر coordinator_learning::verified_broker_feedback، وهي البوابة المشتركة للتكاليف والـrisk_money وr_multiple وreward_v2.
- في التقرير العام يجري أيضاً فحص هوية context + trading_mode + brain + position_id + closed_ms الموجودة داخل الحمولة مقابل أعمدة SQLite، وليس مجرد التصفية داخل SQL.
- تقرير القناص يطلب context مطابقاً ويستبقي فحص السياسات الدقيقة ونسخة الاستراتيجية وregime والتذاكر.
- أي سجل ناقص أو غير مطابق يُستبعد من مجموع نتائج الأداء ومن عدد العينات ولا يُحذف من السجل الأصلي.
- يذكر التقرير صراحة broker_admission_gate وlearning_eligible_broker_samples_only=true وlegacy_unverified_samples_excluded_not_deleted=true.
- قد تقل أعداد عينات الأداء بعد الترقية لأن عينات تاريخية غير مثبتة ستُستبعد؛ لا يعني ذلك اختفاء صفقات الوسيط من Trade Journal، ولا أن الصفقة لم تُنفّذ. المقصود أن هذا **تقرير جودة التعلّم** وليس دفتر المحاسبة القانوني أو قائمة كل الأوامر.
- لا يتغير التنفيذ أو حساب اللوت أو المخاطرة أو بيانات حساب REAL، ولا يجري ترقية تلقائية لأي عقل أو نموذج.
- إحصائيات Win Rate وProfit Factor تبقى وصفية لـDemo فقط، ولا تعتبر OOS مستقلاً أو إثبات ربحية.

## تعافي السجل المعطوب وعرض الواجهة

- استبدلنا تصفية JSON غير المحمية بفحص `json_valid` قبل `json_extract` في تقريري Expert/Brain Quality وSniper Quality. السجل النصي الفاسد لم يعد يستطيع تعطيل التقرير بالكامل.
- يظهر الحقل `malformed_json_unknown_mode_rows` للصفوف ذات JSON معطوب في نفس سياق الحساب والعقل، مع بيان أننا **لا نستطيع تحديد وضع التداول** لهذه الصفوف وبالتالي لا يجوز إضافتها لعداد عينات الوضع المعروض.
- عدّلنا عبارات Brain Monitor في واجهة Windows حتى لا تصف مجموع عينات التعلّم المؤهلة على أنها جميع المراكز المنفذة أو دفتر أرباح الحساب بالكامل. تبقى أرقام دفتر الصفقات مستقلة، والسجلات الأصلية محفوظة.

## اختبارات مباشرة

- g5g_report_counts_only_verified_learning_receipts_not_forged_reward_or_missing_risk
- g5g_mismatched_broker_payload_identity_is_never_brain_performance
- g5g_sniper_scorecard_rejects_reward_poisoning_and_missing_risk
- g5g_sniper_rejects_cross_account_payload_with_correct_broker_net
- g5g_malformed_legacy_json_cannot_crash_quality_or_fake_samples
- g5g_sniper_corrupt_json_is_excluded_without_aborting_other_brains

تضمن الاختبارات أن السجلات ذات نتائج مالية متوازنة لكن مكافأة مزورة، risk مفقود، R خاطئ، حجم مخالف، أو حساب/مركز/عقل/وقت مخالف لا تدخل إحصائيات التعلم.

## القبول

يجتاز كامل مصدر Rust اختبارات cargo test --locked، مع بقية اختبارات Research/Linux/Windows/Portable على SHA النهائي عبر GitHub Actions. لا تكفي نتيجة اختبار تجريبي محلي ولا تشغيل GUI headless لإثبات نجاح أزرار Tauri على جهاز Windows الحقيقي.

G5-G تغلق عدم الاتساق بين *بيانات قابلة للتدريب* و*تقارير جودة التعلم*، وليس كل متطلبات G5 الأصلية. بقي تدقيق مدلولات أرقام الواجهة الفعلية على Windows وأدلة MT5 Demo الحقيقية وقياس OOS والربحية المنفذة.

## التالي

بعد نجاح CI يراجع الربط بين مصادر هذه التقارير والواجهة (learning quality panel/sniper policy panel) مع اختبار عزل الحساب/الوضع أثناء تبديل MT5. لا تفسر مجرد وصول أرقام للداشبورد أنها تدريب مستقل لـ17 نموذج ML.
