use anyhow::{bail, Context, Result};
use braintrade_domain::{
    commission::{Account, Basis, Profile},
    Config, Status,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn view(c: &Config, s: &Status) -> Value {
    let t = s.telemetry.as_ref().filter(|t| s.gateway_fresh && t.connected);
    let profile = t.and_then(|t| c.commission_profile(t));
    json!({
        "config": c,
        "priority": {
            "enabled":c.trading.priority_enabled,
            "sniper_active":c.trading.sniper_active,
            "fallback_mode":c.trading.fallback_mode,
            "active_mode":c.trading.mode,
            "one_entry_coordinator":true,
            "handoff_restriction":"Flat MT5 account, no pending broker intents, fresh bound account, LIVE login reconfirmation."
        },
        "commission_editor": {
            "account": t.map(Account::from_telemetry),
            "saved": profile.is_some(),
            "label": profile.map(|p| p.label.as_str()).unwrap_or(""),
            "amount": profile.map(|p| p.amount).unwrap_or(c.risk.commission_per_lot),
            "basis": profile.map(|p| p.basis.clone()).unwrap_or_default(),
            "round_trip_per_lot": profile
                .map(|p| p.basis.round_trip(p.amount).unwrap())
                .unwrap_or(c.risk.commission_per_lot)
        }
    })
}

/// Change which already configured coordinator owns NEW entries, never both.
/// The HTTP route checks open broker positions, unresolved intents, account
/// freshness and active Live consent before committing this validated change.
pub fn priority_handoff(c:&Config, sniper_active:bool)->Result<Config> {
    anyhow::ensure!(c.trading.priority_enabled,"فعّل أولوية القناص واحفظها أولاً");
    anyhow::ensure!(c.trading.sniper_active != sniper_active,"هذا الوضع مفعّل بالفعل");
    let mut candidate=c.clone();
    candidate.trading.sniper_active=sniper_active;
    candidate.trading.select_priority_owner();
    candidate.validate().map_err(anyhow::Error::msg)?;
    Ok(candidate)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    trading: braintrade_domain::trading::TradingConfig,
    commission_per_lot: f64,
    #[serde(default)]
    commission_basis: Basis,
    /// Shared execution risk estimate; independent of observed Bid/Ask spread.
    #[serde(default)]
    slippage_ticks: Option<f64>,
    #[serde(default)]
    account: Option<Account>,
    #[serde(default)]
    label: String,
}

pub fn update(c: &Config, s: &Status, body: &[u8]) -> Result<Config> {
    if s.requested_run {
        bail!("أوقف النظام قبل تبديل الوضع والتكاليف");
    }
    let u: Update = serde_json::from_slice(body)?;
    let total = u
        .commission_basis
        .round_trip(u.commission_per_lot)
        .map_err(anyhow::Error::msg)?;
    let current = s.telemetry.as_ref().filter(|t| s.gateway_fresh && t.connected);
    let requested = u.account.or_else(|| current.map(Account::from_telemetry));
    let mut c = c.clone();
    c.trading = u.trading;
    if let Some(ticks) = u.slippage_ticks {
        c.risk.slippage_ticks = ticks;
    }
    // Keep the most recently entered value as a safe fallback for accounts that
    // do not have an exact profile yet. Exact profiles always win at runtime.
    c.risk.commission_per_lot = total;
    if let Some(account) = requested {
        let t = current.context("اتصل بالحساب قبل حفظ عمولته")?;
        if account != Account::from_telemetry(t) {
            bail!("تغير الحساب أو الرمز؛ أعد تحميل إعدادات العمولة");
        }
        let profile = Profile {
            account,
            label: u.label,
            amount: u.commission_per_lot,
            basis: u.commission_basis,
        };
        c.commission_profiles.retain(|p| p.account != profile.account);
        c.commission_profiles.push(profile);
    }
    c.validate().map_err(anyhow::Error::msg)?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> Status {
        let mut s = Status::default();
        s.gateway_fresh = true;
        s.telemetry = Some(braintrade_domain::Telemetry {
            connected: true,
            login: 17,
            server: "Demo".into(),
            currency: "USD".into(),
            symbol: "XAUUSD".into(),
            ..Default::default()
        });
        s
    }

    #[test]
    fn priority_handoff_is_atomic_and_has_exactly_one_owner() {
        let mut c=Config::default();
        assert!(priority_handoff(&c,false).is_err(),"legacy manual mode never switches");
        c.trading.priority_enabled=true;
        c.trading.fallback_mode=braintrade_domain::trading::TradingMode::Swing;
        c.trading.select_priority_owner();
        let swapped=priority_handoff(&c,false).unwrap();
        assert_eq!(swapped.trading.mode,braintrade_domain::trading::TradingMode::Swing);
        assert_eq!(c.trading.mode,braintrade_domain::trading::TradingMode::Sniper);
        assert!(priority_handoff(&swapped,false).is_err());
        let restored=priority_handoff(&swapped,true).unwrap();
        assert_eq!(restored.trading.mode,braintrade_domain::trading::TradingMode::Sniper);
        assert_eq!(restored.trading.fallback_mode,braintrade_domain::trading::TradingMode::Swing);
    }

    #[test]
    fn save_roundtrip_reload_and_reject_switch() {
        let mut s = connected();
        let c = Config::default();
        let account = Account::from_telemetry(s.telemetry.as_ref().unwrap());
        let body = json!({
            "trading": c.trading,
            "commission_per_lot": 3.5,
            "commission_basis": "per_side",
            "account": account,
            "label": "ECN"
        })
        .to_string();
        let saved = update(&c, &s, body.as_bytes()).unwrap();
        let saved: Config = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(saved.risk.commission_per_lot, 7.);
        assert_eq!(view(&saved, &s)["commission_editor"]["round_trip_per_lot"], 7.);
        s.telemetry.as_mut().unwrap().login = 18;
        assert!(update(&c, &s, body.as_bytes()).is_err());
        assert_eq!(view(&saved, &s)["commission_editor"]["saved"], false);
    }

    #[test]
    fn zero_fee_is_saved_for_exact_account_and_survives_reload_without_stale_profile() {
        let s=connected();
        let mut c=Config::default();
        let account=Account::from_telemetry(s.telemetry.as_ref().unwrap());
        c.commission_profiles.push(Profile{
            account:account.clone(),label:"old ECN".into(),
            amount:3.5,basis:Basis::PerSide,
        });
        let req=json!({
            "trading":c.trading,
            "commission_per_lot":0.0,
            "commission_basis":"round_trip",
            "account":account,
            "label":"zero fee broker"
        }).to_string();
        let updated=update(&c,&s,req.as_bytes()).unwrap();
        let reloaded:Config=serde_json::from_str(&serde_json::to_string(&updated).unwrap()).unwrap();
        let t=s.telemetry.as_ref().unwrap();
        assert_eq!(reloaded.commission_profiles.len(),1,"stale paid profile must be replaced");
        assert_eq!(reloaded.for_account(t).risk.commission_per_lot,0.);
        assert_eq!(view(&reloaded,&s)["commission_editor"]["round_trip_per_lot"],0.);
        for lots in [0.01,0.10,1.] {
            assert_eq!(braintrade_domain::commission::round_trip_charge(
                reloaded.for_account(t).risk.commission_per_lot,lots).unwrap(),0.);
        }
    }

    #[test]
    fn slippage_estimate_can_be_disabled_separately_from_broker_spread() {
        let c = Config::default();
        let s = connected();
        let body = serde_json::json!({
            "trading": c.trading,
            "commission_per_lot": 7.,
            "commission_basis": "round_trip",
            "slippage_ticks": 0.
        }).to_string();
        let updated = update(&c, &s, body.as_bytes()).unwrap();
        assert_eq!(updated.risk.commission_per_lot, 7.);
        assert_eq!(updated.risk.slippage_ticks, 0.);
    }

    #[test]
    fn legacy_desktop_save_scopes_to_current_account_automatically() {
        let s = connected();
        let c = Config::default();
        let body = json!({
            "trading": c.trading,
            "commission_per_lot": 5.25
        })
        .to_string();
        let saved = update(&c, &s, body.as_bytes()).unwrap();
        assert_eq!(saved.commission_profiles.len(), 1);
        assert_eq!(saved.commission_profiles[0].account.login, 17);
        assert_eq!(saved.commission_profiles[0].amount, 5.25);
        assert_eq!(saved.risk.commission_per_lot, 5.25);
    }
}
