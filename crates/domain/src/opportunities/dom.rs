use crate::Telemetry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomBrain {
    pub available: bool,
    pub levels: usize,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub imbalance: f64,
    pub microprice_proxy: Option<f64>,
    pub direction: i8,
    pub evidence: f64,
    pub note: String,
}

/// Reads broker-provided MT5 Market Book depth only. This is resting depth, not executed
/// order flow, footprint delta, hidden liquidity or exchange-wide volume.
pub fn assess(t: &Telemetry) -> DomBrain {
    let mut out = DomBrain {
        note: "DOM غير متاح من الوسيط/الرمز؛ لا يتم تعويضه ببيانات شموع مصطنعة.".into(),
        ..Default::default()
    };
    if !t.connected || !t.dom_available || t.dom.is_empty() {
        return out;
    }

    let mut bids = t
        .dom
        .iter()
        .filter(|level| level.side == "buy")
        .collect::<Vec<_>>();
    let mut asks = t
        .dom
        .iter()
        .filter(|level| level.side == "sell")
        .collect::<Vec<_>>();
    bids.sort_by(|a, b| b.price.total_cmp(&a.price));
    asks.sort_by(|a, b| a.price.total_cmp(&b.price));
    if bids.is_empty() || asks.is_empty() {
        out.note = "دفتر السوق وصل دون جانبي شراء وبيع صالحين؛ تم سحب قراءة DOM.".into();
        return out;
    }

    let best_bid = bids[0].price;
    let best_ask = asks[0].price;
    if best_ask < best_bid || !best_bid.is_finite() || !best_ask.is_finite() {
        out.note = "أفضل أسعار DOM غير صالحة أو متقاطعة؛ القراءة مرفوضة.".into();
        return out;
    }

    // Use the nearest five levels on each side. This keeps the metric bounded and avoids
    // pretending distant book rows are equally actionable.
    let bid_depth = bids.iter().take(5).map(|level| level.volume).sum::<f64>();
    let ask_depth = asks.iter().take(5).map(|level| level.volume).sum::<f64>();
    let total = bid_depth + ask_depth;
    if total <= 0.0 || !total.is_finite() {
        out.note = "أحجام DOM القريبة غير صالحة؛ لا توجد قراءة imbalance.".into();
        return out;
    }
    let imbalance = ((bid_depth - ask_depth) / total).clamp(-1.0, 1.0);
    let direction = if imbalance >= 0.20 {
        1
    } else if imbalance <= -0.20 {
        -1
    } else {
        0
    };
    let microprice = (best_ask * bid_depth + best_bid * ask_depth) / total;

    out.available = true;
    out.levels = bids.len().min(5) + asks.len().min(5);
    out.best_bid = Some(best_bid);
    out.best_ask = Some(best_ask);
    out.bid_depth = bid_depth;
    out.ask_depth = ask_depth;
    out.imbalance = imbalance;
    out.microprice_proxy = microprice.is_finite().then_some(microprice);
    out.direction = direction;
    out.evidence = ((imbalance.abs() - 0.10) / 0.60).clamp(0.0, 1.0);
    out.note = "عمق حي من MT5 Market Book: مجموع أقرب 5 مستويات بكل جانب. يقيس أوامر معلقة ظاهرة فقط؛ ليس تدفق صفقات منفذة ولا سيولة مخفية.".into();
    out
}
