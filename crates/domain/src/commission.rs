//! User-entered commissions, scoped to an exact broker account and symbol.
use crate::{Config, Telemetry};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    #[default]
    RoundTrip,
    PerSide,
}
/// Estimated brokerage commission for opening AND fully closing an order.
///
/// The configured amount is in account currency per **1.00 lot round trip**.
/// This never debits the account: the broker charges its actual commissions,
/// which can differ and must be reconciled against MT5 booked deals.
pub fn round_trip_charge(rate_per_lot: f64, lots: f64) -> Result<f64, String> {
    if !rate_per_lot.is_finite() || !(0.0..=1000.0).contains(&rate_per_lot)
        || !lots.is_finite() || lots <= 0.0 || lots > 100.0
    {
        return Err("Commission estimate requires a non-negative valid per-lot rate and positive lot size".into());
    }
    let amount=rate_per_lot*lots;
    if !amount.is_finite() {
        return Err("Commission estimate overflow".into());
    }
    Ok(amount)
}

impl Basis {
    pub fn round_trip(&self, amount: f64) -> Result<f64, String> {
        let total = amount * if *self == Self::PerSide { 2. } else { 1. };
        if !amount.is_finite() || amount < 0. || total > 1000. {
            return Err("Commission must be finite, non-negative and at most 1000 round-trip per lot".into());
        }
        Ok(total)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub login: u64,
    pub server: String,
    pub currency: String,
    pub symbol: String,
}
impl Account {
    pub fn from_telemetry(t: &Telemetry) -> Self {
        Self {
            login: t.login,
            server: t.server.clone(),
            currency: t.currency.clone(),
            symbol: t.symbol.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub account: Account,
    pub label: String,
    pub amount: f64,
    pub basis: Basis,
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        self.basis.round_trip(self.amount)?;
        if self.account.login == 0
            || self.account.server.is_empty()
            || self.account.server.len() > 200
            || self.account.currency.is_empty()
            || self.account.currency.len() > 20
            || self.account.symbol.is_empty()
            || self.account.symbol.len() > 80
            || self.label.len() > 160
        {
            return Err("Invalid commission profile account or label".into());
        }
        Ok(())
    }
}

impl Config {
    pub fn commission_profile(&self, t: &Telemetry) -> Option<&Profile> {
        let key = Account::from_telemetry(t);
        self.commission_profiles.iter().find(|p| p.account == key)
    }

    /// Return an account-scoped copy without mutating the global fallback commission.
    pub fn for_account(&self, t: &Telemetry) -> Self {
        let mut c = self.clone();
        if let Some(p) = self.commission_profile(t) {
            c.risk.commission_per_lot = p.basis.round_trip(p.amount).expect("validated profile");
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_round_trip_lot_examples_and_zero_fee_are_identical_across_sizes() {
        for (lots, expected) in [(0.01,0.07),(0.10,0.70),(1.0,7.0)] {
            assert!((round_trip_charge(7.,lots).unwrap()-expected).abs()<1e-10);
            assert_eq!(round_trip_charge(0.,lots).unwrap(),0.);
            assert!((round_trip_charge(Basis::PerSide.round_trip(3.5).unwrap(),lots).unwrap()-expected).abs()<1e-10);
        }
        for (rate,lots) in [(f64::NAN,0.1),(-1.,0.1),(7.,0.),(7.,-0.01),(7.,f64::INFINITY)] {
            assert!(round_trip_charge(rate,lots).is_err());
        }
    }

    #[test]
    fn proportional_and_account_isolation() {
        let t = Telemetry {
            login: 17,
            server: "Demo".into(),
            currency: "USD".into(),
            symbol: "XAUUSD".into(),
            ..Default::default()
        };
        let mut c = Config::default();
        c.commission_profiles.push(Profile {
            account: Account::from_telemetry(&t),
            label: "ECN".into(),
            amount: 3.5,
            basis: Basis::PerSide,
        });
        c.validate().unwrap();
        assert!((c.for_account(&t).risk.commission_per_lot * 0.1 - 0.7).abs() < 1e-12);
        c.risk.commission_per_lot = 0.;
        for other in [
            Telemetry { login: 18, ..t.clone() },
            Telemetry { server: "Real".into(), ..t.clone() },
            Telemetry { currency: "EUR".into(), ..t.clone() },
            Telemetry { symbol: "EURUSD".into(), ..t.clone() },
        ] {
            assert_eq!(c.for_account(&other).risk.commission_per_lot, 0.);
        }
        assert_eq!(c.risk.commission_per_lot, 0.);
        assert!(Basis::PerSide.round_trip(501.).is_err());
        assert!(Basis::RoundTrip.round_trip(f64::NAN).is_err());
        c.commission_profiles.push(c.commission_profiles[0].clone());
        assert!(c.validate().is_err());
    }
}
