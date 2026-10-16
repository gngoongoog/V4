use anyhow::{bail,Context,Result};
use braintrade_domain::{commission::{Account,Basis,Profile},Config,Status};
use serde::Deserialize;
use serde_json::{json,Value};

pub fn view(c:&Config,s:&Status)->Value {
    let t=s.telemetry.as_ref().filter(|t|s.gateway_fresh&&t.connected);
    let profile=t.and_then(|t|c.commission_profile(t));
    json!({"config":c,"commission_editor":{
        "account":t.map(Account::from_telemetry),"saved":profile.is_some(),
        "label":profile.map(|p|p.label.as_str()).unwrap_or(""),
        "amount":profile.map(|p|p.amount).unwrap_or(c.risk.commission_per_lot),
        "basis":profile.map(|p|p.basis.clone()).unwrap_or_default(),
        "round_trip_per_lot":profile.map(|p|p.basis.round_trip(p.amount).unwrap()).unwrap_or(c.risk.commission_per_lot)
    }})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    trading:braintrade_domain::trading::TradingConfig,
    commission_per_lot:f64,
    #[serde(default)] commission_basis:Basis,
    #[serde(default)] account:Option<Account>,
    #[serde(default)] label:String,
}
pub fn update(c:&Config,s:&Status,body:&[u8])->Result<Config> {
    if s.requested_run {bail!("أوقف النظام قبل تبديل الوضع والتكاليف");}
    let u:Update=serde_json::from_slice(body)?;
    let total=u.commission_basis.round_trip(u.commission_per_lot).map_err(anyhow::Error::msg)?;
    let mut c=c.clone();c.trading=u.trading;
    if let Some(account)=u.account {
        let t=s.telemetry.as_ref().filter(|t|s.gateway_fresh&&t.connected).context("اتصل بالحساب قبل حفظ عمولته")?;
        if account!=Account::from_telemetry(t) {bail!("تغير الحساب أو الرمز؛ أعد تحميل إعدادات العمولة");}
        let profile=Profile{account,label:u.label,amount:u.commission_per_lot,basis:u.commission_basis};
        c.commission_profiles.retain(|p|p.account!=profile.account);c.commission_profiles.push(profile);
    } else {c.risk.commission_per_lot=total;}
    c.validate().map_err(anyhow::Error::msg)?;
    Ok(c)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn save_roundtrip_reload_and_reject_switch() {
        let mut s=Status::default();s.gateway_fresh=true;
        s.telemetry=Some(braintrade_domain::Telemetry{connected:true,login:17,server:"Demo".into(),currency:"USD".into(),symbol:"XAUUSD".into(),..Default::default()});
        let c=Config::default();let account=Account::from_telemetry(s.telemetry.as_ref().unwrap());
        let body=json!({"trading":c.trading,"commission_per_lot":3.5,"commission_basis":"per_side","account":account,"label":"ECN"}).to_string();
        let saved=update(&c,&s,body.as_bytes()).unwrap();
        let saved:Config=serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(view(&saved,&s)["commission_editor"]["round_trip_per_lot"],7.);
        s.telemetry.as_mut().unwrap().login=18;
        assert!(update(&c,&s,body.as_bytes()).is_err());
        assert_eq!(view(&saved,&s)["commission_editor"]["saved"],false);
    }
}
