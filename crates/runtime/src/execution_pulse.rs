//! Minimal quote overlay used by the Sniper execution fast path.
//! Full account/position/contract reconciliation still comes from the regular MT5 snapshot.
use anyhow::Result;
use braintrade_domain::{studio::FrameBar, trading::TradingMode, Config, Telemetry};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub(crate) struct FastContext {
    pub telemetry: Telemetry,
    pub config: Config,
    pub frames: Arc<BTreeMap<String, Vec<FrameBar>>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pulse {
    login: u64,
    server: String,
    currency: String,
    symbol: String,
    bid: f64,
    ask: f64,
    quote_time_ms: u64,
    #[serde(default)]
    source_quote_time_ms: Option<u64>,
    tick_age_ms: u64,
}

impl FastContext {
    pub fn new(
        telemetry: &Telemetry,
        config: &Config,
        frames: &BTreeMap<String, Vec<FrameBar>>,
    ) -> Self {
        Self {
            telemetry: telemetry.clone(),
            config: config.clone(),
            frames: Arc::new(frames.clone()),
        }
    }

    /// Overlay only a newer broker quote onto the most recent reconciled snapshot.
    /// Account identity, positions, margin mode, trade permissions and contract data
    /// are never accepted from the fast payload.
    pub fn apply(&self, body: &[u8], now: u64) -> Result<Self> {
        anyhow::ensure!(
            self.config.trading.mode == TradingMode::Sniper,
            "Sniper pulse is available only in Sniper mode"
        );
        let pulse: Pulse = serde_json::from_slice(body)?;
        anyhow::ensure!(self.telemetry.connected, "A reconciled MT5 snapshot is required");
        anyhow::ensure!(
            pulse.login == self.telemetry.login
                && pulse.server == self.telemetry.server
                && pulse.currency == self.telemetry.currency
                && pulse.symbol == self.telemetry.symbol,
            "Sniper pulse account or symbol changed"
        );
        anyhow::ensure!(
            pulse.bid.is_finite()
                && pulse.ask.is_finite()
                && pulse.bid > 0.0
                && pulse.ask >= pulse.bid,
            "Invalid Sniper pulse quote"
        );
        anyhow::ensure!(pulse.quote_time_ms > 0, "Sniper pulse timestamp is required");
        anyhow::ensure!(pulse.tick_age_ms <= 1000, "Sniper pulse quote is stale");
        anyhow::ensure!(self.telemetry.source_quote_time_ms.is_none()
            || pulse.source_quote_time_ms.is_some(),
            "Sniper pulse lost broker timestamp provenance");
        let source_ms = pulse.source_quote_time_ms.unwrap_or(pulse.quote_time_ms);
        anyhow::ensure!(source_ms > 0 && source_ms <= pulse.quote_time_ms,
            "Invalid Sniper broker timestamp");
        // The gateway's reported age is not sufficient: a cached tick can
        // claim age 3ms while its exchange timestamp is minutes old.
        anyhow::ensure!(
            now.saturating_sub(source_ms) <= 1500,
            "Sniper pulse exchange timestamp is stale"
        );
        anyhow::ensure!(
            source_ms <= now.saturating_add(2000),
            "Sniper pulse timestamp is in the future"
        );
        anyhow::ensure!(
            self.telemetry
                .quote_time_ms
                .is_none_or(|previous| pulse.quote_time_ms > previous),
            "Duplicate or out-of-order Sniper pulse"
        );

        anyhow::ensure!(self.telemetry.broker_quote_time_ms()
            .is_none_or(|previous| source_ms >= previous),
            "Out-of-order Sniper broker quote");

        let mut next = self.clone();
        next.telemetry.bid = Some(pulse.bid);
        next.telemetry.ask = Some(pulse.ask);
        next.telemetry.quote_time_ms = Some(pulse.quote_time_ms);
        next.telemetry.source_quote_time_ms = pulse.source_quote_time_ms;
        next.telemetry.tick_age_ms = Some(pulse.tick_age_ms);
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> FastContext {
        let mut config = Config::default();
        config.trading.mode = TradingMode::Sniper;
        FastContext::new(
            &Telemetry {
                connected: true,
                login: 17,
                server: "Demo".into(),
                currency: "USD".into(),
                symbol: "XAUUSD".into(),
                bid: Some(2500.0),
                ask: Some(2500.2),
                quote_time_ms: Some(1_000),
                tick_age_ms: Some(0),
                ..Default::default()
            },
            &config,
            &BTreeMap::new(),
        )
    }

    fn pulse() -> Vec<u8> {
        br#"{"login":17,"server":"Demo","currency":"USD","symbol":"XAUUSD","bid":2500.1,"ask":2500.3,"quote_time_ms":1001,"tick_age_ms":3}"#.to_vec()
    }

    #[test]
    fn overlays_only_a_new_quote() {
        let base = context();
        let next = base.apply(&pulse(), 1_004).unwrap();
        assert_eq!(next.telemetry.bid, Some(2500.1));
        assert_eq!(next.telemetry.ask, Some(2500.3));
        assert_eq!(next.telemetry.quote_time_ms, Some(1001));
        assert_eq!(next.telemetry.login, base.telemetry.login);
        assert!(Arc::ptr_eq(&next.frames, &base.frames));
    }

    #[test]
    fn dense_events_use_broker_time_for_freshness_and_event_time_for_dedup() {
        let mut base = context();
        base.telemetry.source_quote_time_ms = Some(1000);
        let mut packet: serde_json::Value = serde_json::from_slice(&pulse()).unwrap();
        packet["source_quote_time_ms"] = serde_json::json!(1000);
        packet["quote_time_ms"] = serde_json::json!(5000);
        let first = base.apply(&serde_json::to_vec(&packet).unwrap(), 1004).unwrap();
        assert_eq!(first.telemetry.broker_quote_time_ms(), Some(1000));
        assert_eq!(first.telemetry.quote_time_ms, Some(5000));
        assert!(first.telemetry.quote_is_fresh(1004, 1000));
        assert!(first.apply(&serde_json::to_vec(&packet).unwrap(), 1004).is_err());
        packet["quote_time_ms"] = serde_json::json!(5001);
        packet["bid"] = serde_json::json!(2500.2);
        let second = first.apply(&serde_json::to_vec(&packet).unwrap(), 1004).unwrap();
        assert_eq!(second.telemetry.bid, Some(2500.2));
        packet["quote_time_ms"] = serde_json::json!(5002);
        assert!(second.apply(&serde_json::to_vec(&packet).unwrap(), 3000).is_err());
        packet["source_quote_time_ms"] = serde_json::json!(999);
        assert!(second.apply(&serde_json::to_vec(&packet).unwrap(), 1004).is_err());
        packet.as_object_mut().unwrap().remove("source_quote_time_ms");
        assert!(second.apply(&serde_json::to_vec(&packet).unwrap(), 1004).is_err());
    }

    #[test]
    fn rejects_malformed_or_future_source_time() {
        let base = context();
        for source in [0_u64, 5000, u64::MAX] {
            let mut packet: serde_json::Value = serde_json::from_slice(&pulse()).unwrap();
            packet["source_quote_time_ms"] = serde_json::json!(source);
            assert!(base.apply(&serde_json::to_vec(&packet).unwrap(), 1004).is_err());
        }
        assert_eq!(base.telemetry.quote_time_ms, Some(1000));
    }

    #[test]
    fn rejects_identity_stale_crossed_and_duplicate_quotes() {
        let base = context();
        let wrong = br#"{"login":18,"server":"Demo","currency":"USD","symbol":"XAUUSD","bid":2500.1,"ask":2500.3,"quote_time_ms":1001,"tick_age_ms":3}"#;
        assert!(base.apply(wrong, 1_004).is_err());
        let stale = br#"{"login":17,"server":"Demo","currency":"USD","symbol":"XAUUSD","bid":2500.1,"ask":2500.3,"quote_time_ms":1001,"tick_age_ms":1001}"#;
        assert!(base.apply(stale, 1_004).is_err());
        let crossed = br#"{"login":17,"server":"Demo","currency":"USD","symbol":"XAUUSD","bid":2500.4,"ask":2500.3,"quote_time_ms":1001,"tick_age_ms":3}"#;
        assert!(base.apply(crossed, 1_004).is_err());
        let duplicate = br#"{"login":17,"server":"Demo","currency":"USD","symbol":"XAUUSD","bid":2500.1,"ask":2500.3,"quote_time_ms":1000,"tick_age_ms":3}"#;
        assert!(base.apply(duplicate, 1_004).is_err());
        let false_fresh = br#"{"login":17,"server":"Demo","currency":"USD","symbol":"XAUUSD","bid":2500.1,"ask":2500.3,"quote_time_ms":1001,"tick_age_ms":3}"#;
        assert!(base.apply(false_fresh, 100_000).is_err(),
            "Broker timestamp must corroborate reported tick age");
    }
}
