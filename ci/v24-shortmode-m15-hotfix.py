"""V24: allow short modes to proceed when ONLY unrelated M15 research history is absent.

Production must still prove complete/current M1 + M5, real broker identity,
valid quotes and risk approval; Expert still requires complete M15.
"""
from pathlib import Path

def replace_once(path, old, new):
    text=path.read_text(encoding="utf-8")
    if text.count(old)!=1:
        raise SystemExit(f"V24 patch fail: expected exactly one match in {path} for {old[:80]!r} (found {text.count(old)})")
    path.write_text(text.replace(old,new),encoding="utf-8")

domain=Path("crates/domain/src/trading.rs")
sup=Path("crates/runtime/src/enterprise_supervisor.rs")
replace_once(domain,
    "/// Validate the two frames actually used by Scalping/Sniper when full research",
    """/// This waiver applies ONLY to an unrelated M15 history diagnostic.
pub fn missing_unrequired_m15_only(error: &str) -> bool {
    let mut seen = false;
    for warning in error.split(';').map(str::trim).filter(|v| !v.is_empty()) {
        if !warning.starts_with("M15: ") { return false; }
        seen = true;
    }
    seen
}
/// Validate the two frames actually used by Scalping/Sniper when full research""")
replace_once(domain,
    "if !m.error.is_empty() || m.captured_ms > now",
    "if (!m.error.is_empty() && !missing_unrequired_m15_only(&m.error)) || m.captured_ms > now")
replace_once(sup,
    "if market.is_none_or(|m| !m.error.is_empty() || m.captured_ms==0 || m.captured_ms>now.saturating_add(2000)",
    """if market.is_none_or(|m| (!m.error.is_empty() &&
                    !(matches!(c.trading.mode,TradingMode::Scalping|TradingMode::Sniper) &&
                    braintrade_domain::trading::missing_unrequired_m15_only(&m.error))) ||
                    m.captured_ms==0 || m.captured_ms>now.saturating_add(2000)""")
replace_once(domain,
    "    #[test]\n    fn short_mode_m1_m5_fallback_never_accepts_stale_incomplete_or_gapped_bars()",
    """    #[test]
    fn missing_m15_error_is_not_an_unrelated_m1_m5_veto() {
        let now=1_800_000_000_000u64;
        let mut m=crate::MarketData{captured_ms:now,m1:complete_bars(now,60),
            m5:complete_bars(now,300),m15:Vec::new(),
            error:"M15: waiting for 100 closed candles".into()};
        let mut t=Telemetry{connected:true,symbol:"XAUUSD".into(),
            bid:Some(124.),ask:Some(124.01),tick_age_ms:Some(0),
            quote_time_ms:Some(now),market:Some(m.clone()),..Default::default()};
        for mode in [TradingMode::Sniper,TradingMode::Scalping] {
            let mut c=Config::default();c.trading.mode=mode;
            assert!(select(Analysis::default(),&t,&c,&BTreeMap::new(),now).current);
        }
        for warning in ["", "M1: old", "M5: old", "M15: old; M1: old", "bad history"] {
            assert!(!missing_unrequired_m15_only(warning));
        }
        m.error="M15: missing; M5: stale".into();
        t.market=Some(m);
        let mut c=Config::default();c.trading.mode=TradingMode::Scalping;
        assert!(!select(Analysis::default(),&t,&c,&BTreeMap::new(),now).current);
    }
    #[test]
    fn short_mode_m1_m5_fallback_never_accepts_stale_incomplete_or_gapped_bars()""")
replace_once(sup,
    " #[test] fn spread_guard_blocks_new_entries(){",
    """ #[test] fn unrelated_m15_does_not_veto_short_mode_but_still_vetoes_expert(){
    let now=1_750_000_000_000;
    let (mut t,mut c)=sample(now);
    c.trading.mode=TradingMode::Scalping;
    t.market.as_mut().unwrap().error="M15: waiting for 100 closed candles".into();
    assert!(assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
    c.trading.mode=TradingMode::Expert;
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
    c.trading.mode=TradingMode::Scalping;
    t.market.as_mut().unwrap().error="M15: missing; M5: old".into();
    assert!(!assess(&Settings::default(),Some(&t),&c,&BTreeMap::new(),now).entry_allowed);
 }
 #[test] fn spread_guard_blocks_new_entries(){""")
print("V24 short-mode frame separation and Rust regression tests applied")
