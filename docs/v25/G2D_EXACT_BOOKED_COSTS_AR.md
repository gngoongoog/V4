# BRAINTRADE V25 — G2-D | إثبات تكاليف MT5 الفعلية دون اختلاق الصفر

## المشكلة
كان `trade_journal.rs` يعتبر حقول `profit` و`commission` و`swap` و`fee` المفقودة أو غير الصالحة مساوية للصفر عند جمع الصافي التاريخي. هذا قد يجعل سجل ناقصاً يبدو ربحاً صافياً مؤكداً، أو يوحي بأن الوسيط بلا عمولة.

## الإصلاح
- يعتمد التجميع المالي على كل صفقة BUY/SELL مقيّدة في `broker_deals`: يجب أن تكون كل مكونات `profit/commission/swap/fee` أعداداً منتهية وموجودة ومجموعها منتهياً.
- قيمة الصفر الأصلية من الوسيط تظل `0.0`. أما الحقول الناقصة أو الفاسدة فتصير `null` في `broker_net_booked` و`broker_gross` و`commission` و`swap` و`fee` لجميع الصفقة، وليس صفراً مختلقاً.
- التمييز عبر `booked_cost_evidence`: `BROKER_COMPONENTS_VERIFIED` أو `MISSING_OR_INVALID_BROKER_COST`، والمراكز التي لم يصل تاريخها بعد `AWAITING_BROKER_DEALS`.
- لا تعديل على التداول أو `order_send`، ولا إسناد تخميني لتكاليف MT5 المنفصلة التي لا تحمل `position_id`.

## التحقق
اختبارات Rust الجديدة: `historical_missing_cost_cannot_masquerade_as_zero_fee_or_profit` و `nonfinite_broker_cost_does_not_enter_booked_pnl` مع البقية عبر `cargo test --locked -p braintrade-runtime`. اعتماد CI يجب أن يكون على Commit الرأس نفسه: Research وWindows وPortable.

## ما بقي لقبول G2 الحقيقي
فحص ربط MT5 Demo الحقيقي، واستخراج `Order/Deal/History` الأصلي ومقارنة `ticket/order/position_id/type/entry/volume/profit/commission/swap/fee`، بما فيها Unknown وPartial وClose بعد Restart. فحوص Fake MT5 ليست إثباتاً لنسبة نجاح أو ربحية أو إذن بدء REAL.

هذه مرحلة مستقلة بعد PR #155؛ لا تغيير في `main` أو V24.
