BRAINTRADE V25 P0 — أدوات فحص قراءة فقط

1) افتح ZIP النسخة المحمولة V24 وفكّه كاملاً في مجلد ثابت.
2) فك ZIP هذه الأدوات فوق نفس المجلد حيث BrainTradeNext.exe.
3) شغّل VERIFY_SYSTEM.cmd. سيظهر تقرير HTML وJSON داخل verification-output.
4) هذا الفحص لا يبدأ التداول ولا يتصل بالوسيط ولا يفتح/يغلق أوامر.
5) فحص DEMO الحقيقي منفصل: Run-Demo-Acceptance.cmd الموجود في Portable الأصلية.
6) نتائج Windows Build V25 وREAL وواجهة 22 صفحة وDemo end-to-end تبقى NOT TESTED.
7) لا ترسل credentials/token/private account journal. أرسل فقط ملف v25-verification.json.
