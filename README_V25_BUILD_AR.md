# بناء Enterprise V25 من المصدر

المدخل الحالي هو `BUILD_V25_PORTABLE.cmd` على Windows x64.
المتطلبات للمطور: Rust 1.99.1 / MSVC C++ Build Tools، Node 22، Python 3.13.
المستخدم النهائي لا يحتاج هذه الأدوات؛ يفك الحزمة ويشغّل `BrainTradeNext.exe`.

يحفظ البناء React داخل EXE عبر `custom-protocol`، ويضع Python المضمّن والبوابة وEA بجانبه.
ملفات التبعيات مقفلة؛ أي اختلاف في Cargo.lock أو package-lock.json يفشل بوابة الإصدار.
يُمنع استبدال المصدر بأرشيف CI قديم. وصفات V24 السابقة محفوظة في فرع الرجوع.

الحزمة تنتج في `target/portable/BRAINTRADE_V4_ENTERPRISE_V25_PORTABLE_Windows_x64.zip`.
تحتوي `VERIFY_SYSTEM.cmd` و`CREATE_DESKTOP_ICON.cmd` ولا تتطلب Setup إلزامياً.
WebView2 مطلوب على جهاز المستخدم، وMT5 مطلوب للتداول. EA المرفق مصدر MQ5؛ ترجمة EX5 تحتاج MetaEditor.

راجع `docs/v25/P0O_STATUS_AR.md` للنتائج وحدود الاختبار قبل الاستخدام.
