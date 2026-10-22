# Stage 3F-L — Offline comparison with caller-supplied MT5 Demo deal evidence

This stage adds a read-only research endpoint:

`POST /v1/research/demo-deal-audit`

It extends the **synthetic** Stage 3F-K bridge. The request contains `bridge` with precisely the Stage 3F-K input (`admission` and `lifecycle`) and `evidence`, a separately scoped list of *caller-supplied* normalized MT5 Demo deal records. The endpoint has **no access to MT5 or broker data by itself**, and **no new orders, disk writes, risk setting changes or learner updates**.

## Required provenance / explicit caveats

- `evidence.source` must be `caller_supplied_mt5_demo_deals`. This is **a label, not a verification** of the origin. The request must declare `demo=true`, account ID, server, symbol, currency, a capture timestamp, whether the caller claims complete history, and up to 5000 exact-ticket deal rows.
- Each deal supplies positive ticket and position IDs; optional, explicitly mapped synthetic intent ID; direction `in`, `out`, or `inout`; positive volume and price; deal time; and exact numeric `profit`, `commission`, `swap`, and `fee`. Duplicated deal tickets, invalid timestamps, nonfinite economics and non-Demo accounts are rejected.
- Account/symbol MUST match the independently validated synthetic bridge before comparing. Unknown intent IDs, unmapped deals, `inout` netting/reversals, multiple position IDs attributed to one synthetic intent, missing exit history or different volumes are **reported separately**. No match is guessed from symbol, timestamp, trade direction or ticket adjacency.
- An exact volume match in a *complete history claimed by the caller* is only `volume_parity_claimed_unverified`. Without complete history, the status is `insufficient_or_ambiguous_evidence` even when volumes happen to agree. If complete history is claimed but deals are missing or cumulative volumes conflict, the status is `discrepancy`.
- The reported sum of `profit + commission + swap + fee` is **only** the total of the linked, included, non-INOUT deal rows supplied to this endpoint. It is not authenticated booked broker profit and is not combined with synthetic paper reward. Unsupported or unlinked deal rows cannot be silently added.
- At all times the response declares `actual_broker_verified=false`, `profitability_verified=false`, `broker_reads=0`, and `mt5_commands=0`.

## Example request

```json
{
  "bridge": {
    "admission": {
      "account":"demo_77","symbol":"XAUUSD",
      "quote":{"bid":5000.0,"ask":5000.2,"age_ms":50},
      "contract":{"volume_min":0.01,"volume_max":1.0,"volume_step":0.01,
        "tick_size":0.1,"tick_value_loss":1.0,"estimated_margin_per_lot":100.0},
      "limits":{"equity":500,"free_margin":300,"max_positions":2,
        "max_total_lots":0.1,"max_stop_risk_percent":2,
        "commission_roundtrip_per_lot":7,"slippage_ticks_per_side":1},
      "requests":[{"id":"A","side":"buy","lots":0.04,"stop_price":4999.2}]
    },
    "lifecycle": {
      "account":"demo_77","symbol":"XAUUSD",
      "events":[
        {"event":"submit","id":"A","lots":0.04},
        {"event":"dispatch","id":"A"},
        {"event":"fill","id":"A","deal_id":"synthetic1","lots":0.04},
        {"event":"close","id":"A","deal_id":"synthetic2","lots":0.04}
      ]
    }
  },
  "evidence": {
    "source":"caller_supplied_mt5_demo_deals",
    "account":"demo_77","server":"demo_server","symbol":"XAUUSD",
    "currency":"USD","demo":true,"captured_ms":10000,"complete_history":true,
    "deals":[
      {"ticket":101,"position_id":77,"intent_id":"A","direction":"in",
        "volume":0.04,"price":5000.2,"time_msc":1500,
        "profit":0,"commission":-0.14,"swap":0,"fee":0},
      {"ticket":102,"position_id":77,"intent_id":"A","direction":"out",
        "volume":0.04,"price":5001.0,"time_msc":2500,
        "profit":3.2,"commission":-0.14,"swap":0,"fee":0}
    ]
  }
}
```

This produces a *reported* 0.04-in / 0.04-out volume match with unverified provenance. It does **not** establish the hypothetical admission was approved in the real terminal or the deal mapping came from a trustworthy independent journal.

## Remaining work for institutional-grade verification

1. Implement a secure, user-approved read-only exporter that obtains **fresh account-verified** MT5 Demo order/deal history and preserves exact broker deal tickets, position IDs, original command IDs and timestamps.
2. Independently reconcile partial fills, close-by, `inout`, netting, hedged accounts, delayed history and broker account currencies before classifying actual positions as closed.
3. Compare broker order_check, order_calc_margin, order_calc_profit, execution price/slippage and real roundtrip costs against the synthetic model; record unmatched/unknown states, **without** silently correcting model assumptions.
4. Add out-of-sample samples and forward shadow experiments. Green Rust/Windows/Research CI only checks software behavior, **not** trading edge or real capital safety.

Unit tests cover claimed volume parity, incomplete history, contradictory complete history, rejected Real or cross-account evidence, duplicate ticket denial, ambiguous INOUT, orphan deals and foreign-intent mappings. Merge only after exact-head CI completes.
