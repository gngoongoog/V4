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
        anyhow::ensure!(
            pulse.quote_time_ms <= now.saturating_add(2000),
            "Sniper pulse timestamp is in the future"
        );
        anyhow::ensure!(
            self.telemetry
                .quote_time_ms
                .is_none_or(|previous| pulse.quote_time_ms > previous),
            "Duplicate or out-of-order Sniper pulse"
        );

        let mut next = self.clone();
        next.telemetry.bid = Some(pulse.bid);
        next.telemetry.ask = Some(pulse.ask);
        next.telemetry.quote_time_ms = Some(pulse.quote_time_ms);
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
    }
}
