// Built-in Core identities, used only as an offline directory. This is not a
// source of control state, account results, or proof that a brain is active.
// Keep these IDs in sync with domain::execution::BRAINS.
export const BRAIN_DIRECTORY = [
 ['sniper_learner','Sniper Learner','القناص المتعلّم'],
 ['gann','Gann Brain','جان الهندسي'],
 ['wyckoff','Wyckoff Brain','وايكوف'],
 ['trend','Trend Brain','الترند'],
 ['momentum','Momentum Brain','الزخم'],
 ['structure','Structure Brain','اختراق القمم والقيعان'],
 ['zones','Zones Brain','الطلب والعرض والارتداد'],
 ['dom','Broker DOM Brain','دفتر أوامر MT5 لدى الوسيط — ليس COMEX المركزي'],
 ['profile','Candle Activity Profile','تقدير نشاط من tick_volume الشموع، لا حجم تنفيذ عند السعر'],
 ['gann_dom_profile','Gann + Broker DOM + Candle Proxy','جان + عمق الوسيط + تقدير نشاط الشموع'],
 ['ces','CES Brain','الانتروبيا'],
 ['kte','KTE Brain','الاندفاع والارتداد'],
 ['regime','Market Regime Brain','حالة السوق'],
 ['generated','Saved Strategy','استراتيجية محفوظة مختارة من المختبر'],
 ['liquidity','Liquidity Brain','سحب السيولة'],
 ['trend_pullback','Trend Pullback Brain','الرجوع المنظم'],
] as const;
