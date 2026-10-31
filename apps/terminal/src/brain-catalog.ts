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
 ['dom','Market Depth Brain','عمق السوق الفعلي'],
 ['profile','Volume Profile Brain','ملف النشاط الحجمي'],
 ['gann_dom_profile','Gann + Depth + Profile','جان والعمق والملف الحجمي'],
 ['ces','CES Brain','الانتروبيا'],
 ['kte','KTE Brain','الاندفاع والارتداد'],
 ['regime','Market Regime Brain','حالة السوق'],
 ['generated','Generated Strategy','استراتيجية مولّدة مختارة'],
 ['liquidity','Liquidity Brain','سحب السيولة'],
 ['trend_pullback','Trend Pullback Brain','الرجوع المنظم'],
] as const;
