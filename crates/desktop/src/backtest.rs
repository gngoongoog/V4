use super::*;
use anyhow::Context;
use slint::{Model, ModelRc, SharedString, VecModel};
mod meta_compare;

fn strategy(index: i32) -> &'static str { match index { 1 => "ces_v1", 2 => "kte_v1", _ => "meta_v1" } }

struct Request {
    action: String,
    body: Value,
}
fn show_report(client: &Client, weak: &slint::Weak<Dashboard>, id: &str) -> anyhow::Result<()> {
    let v = client.post("/v1/backtest/report", json!({"id":id}))?;
    let id = id.to_owned();
    let text = v["text"].as_str().unwrap_or("").to_owned();
    let m = &v["metrics"];
    let net = format!("{:.2}", m["net"].as_f64().unwrap_or(0.0));
    let trades = format!("{}", m["trades"].as_u64().unwrap_or(0));
    let drawdown = format!("{:.2}%", m["max_drawdown_percent"].as_f64().unwrap_or(0.0));
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_bt_report_id(id.into());
        ui.set_bt_report(text.into());
        ui.set_bt_net(net.into());
        ui.set_bt_trades(trades.into());
        ui.set_bt_drawdown(drawdown.into());
    });
    Ok(())
}
fn runs(client: &Client, weak: &slint::Weak<Dashboard>) -> anyhow::Result<()> {
    let v = client.get("/v1/backtest/runs")?;
    let values = v["runs"]
        .as_array()
        .context("Invalid saved backtest list")?;
    let ids = values
        .iter()
        .map(|r| r["id"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let labels = values
        .iter()
        .map(|r| r["label"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        let selected = ui.get_bt_selected();
        let old = if selected >= 0 {
            ui.get_bt_ids()
                .row_data(selected as usize)
                .map(|v| v.to_string())
        } else {
            None
        };
        let next = old
            .and_then(|id| ids.iter().position(|v| v == &id))
            .map(|i| i as i32)
            .unwrap_or(if ids.is_empty() { -1 } else { 0 });
        ui.set_bt_ids(ModelRc::new(VecModel::from(
            ids.into_iter().map(SharedString::from).collect::<Vec<_>>(),
        )));
        ui.set_bt_runs(ModelRc::new(VecModel::from(
            labels
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
        ui.set_bt_selected(next);
    });
    Ok(())
}
pub fn setup(ui: &Dashboard) {
    let (tx, rx) = mpsc::channel::<Request>();
    let weak = ui.as_weak();
    ui.on_backtest_action(move|action|{
        let Some(ui)=weak.upgrade() else{return;};let action=action.to_string();
        let body=if action=="start" {
            let Ok(initial_equity)=ui.get_bt_equity().trim().parse::<f64>() else {ui.set_feedback("أدخل رأس مال رقمي، مثال 10000".into());return;};
            let Ok(spread_floor)=ui.get_bt_spread().trim().parse::<f64>() else {ui.set_feedback("أدخل السبريد بوحدة السعر، مثال 0.20 للذهب".into());return;};
            if !initial_equity.is_finite() || !spread_floor.is_finite(){ui.set_feedback("أدخل أرقاماً محدودة".into());return;}
            json!({"strategy":strategy(ui.get_bt_strategy()),"from_date":ui.get_bt_from().to_string(),"to_date":ui.get_bt_to().to_string(),"initial_equity":initial_equity,"spread_floor":spread_floor})
        } else if action=="load" || action=="replay" || action=="experiment" || action=="compare" {
            let index=ui.get_bt_selected();let id=if index>=0 {ui.get_bt_ids().row_data(index as usize)}else{None};
            let Some(id)=id else{ui.set_feedback("اختر نتيجة محفوظة أولاً".into());return;};if action=="experiment"{json!({"id":id.to_string(),"strategy":strategy(ui.get_bt_strategy())})}else{json!({"id":id.to_string()})}
        } else if action=="export" {
            let id=ui.get_bt_report_id();if id.is_empty(){ui.set_feedback("افتح نتيجة محفوظة أولاً".into());return;};json!({"id":id.to_string()})
        }else{json!({})};
        ui.set_bt_busy(true);let _=tx.send(Request {action,body});
    });
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut client = None;
        let mut ready = false;
        let mut last_auto_id = String::new();
        loop {
            let request = match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(r) => Some(r),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if client.is_none() {
                client = Client::new(&data_dir()).ok();
            }
            let Some(c) = client.as_ref() else {
                if request.is_some() {
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.set_bt_busy(false);
                        ui.set_feedback("انتظر اتصال المحرك ثم أعد الطلب".into());
                    });
                }
                continue;
            };
            if !ready {
                if let Ok(defaults) = c.get("/v1/backtest/defaults") {
                    let from = defaults["from_date"].as_str().unwrap_or("").to_owned();
                    let to = defaults["to_date"].as_str().unwrap_or("").to_owned();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_bt_from(from.into());
                        ui.set_bt_to(to.into());
                    });
                    let _ = runs(c, &weak);
                    ready = true;
                }
            }
            if let Some(request) = request {
                let result = (|| -> anyhow::Result<String> {
                    match request.action.as_str() {
                        "load" => {
                            show_report(
                                c,
                                &weak,
                                request.body["id"].as_str().context("Missing selection")?,
                            )?;
                            Ok("فُتحت النتيجة المحفوظة".into())
                        }
                        "refresh" => {
                            runs(c, &weak)?;
                            Ok("تم تحديث قائمة الاختبارات".into())
                        }
                        "compare" => {
                            let id = request.body["id"].as_str().context("Missing selection")?;
                            let text = meta_compare::run(c, id)?;
                            let display = text.clone();
                            let _ = weak.upgrade_in_event_loop(move |ui| {
                                ui.set_bt_report(display.into());
                            });
                            Ok("اكتملت مقارنة Meta v1 / Meta v2 وحُفظت ملفاتها داخل مجلد التصدير".into())
                        }
                        action => {
                            let v = c.post(&format!("/v1/backtest/{action}"), request.body)?;
                            Ok(if let Some(path) = v["path"].as_str() {
                                if action == "export" {
                                    let report = std::path::Path::new(path).join("report.html");
                                    #[cfg(windows)]
                                    let opener = std::env::var_os("WINDIR")
                                        .map(std::path::PathBuf::from)
                                        .map(|p| p.join("explorer.exe"))
                                        .unwrap_or_else(|| "explorer.exe".into());
                                    #[cfg(not(windows))]
                                    let opener = std::path::PathBuf::from("xdg-open");
                                    std::process::Command::new(opener)
                                        .arg(report)
                                        .spawn()
                                        .with_context(|| {
                                            format!(
                                                "تم حفظ التقرير في {path} لكن تعذر فتحه تلقائياً"
                                            )
                                        })?;
                                }
                                format!("{}\n{}", path, v["note"].as_str().unwrap_or(""))
                            } else {
                                "تم استلام الطلب؛ التقدم يظهر بصفحة الاختبار".into()
                            })
                        }
                    }
                })();
                let message = match result {
                    Ok(s) => s,
                    Err(e) => e.to_string(),
                };
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_bt_busy(false);
                    ui.set_feedback(message.into());
                });
            }
            match c.get("/v1/backtest") {
                Ok(v) => {
                    let state = v["state"].as_str().unwrap_or("idle");
                    let active = state == "loading" || state == "running";
                    let text = format!(
                        "{} | {}% | {}",
                        state,
                        v["progress"].as_u64().unwrap_or(0),
                        v["message"].as_str().unwrap_or("")
                    );
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_bt_active(active);
                        ui.set_bt_status(text.into());
                    });
                    if let Some(id) = v["latest_id"].as_str().filter(|id| *id != last_auto_id) {
                        if show_report(c, &weak, id).is_ok() {
                            last_auto_id = id.into();
                            let _ = runs(c, &weak);
                        }
                    }
                }
                Err(_) => {
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.set_bt_status("تعذر الاتصال بالمحرك؛ الحالة الحالية غير معروفة".into());
                        ui.set_bt_active(false);
                    });
                }
            }
        }
    });
}
