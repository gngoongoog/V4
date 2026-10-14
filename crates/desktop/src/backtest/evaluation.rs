use super::super::*;
use anyhow::Context;
use slint::{Model, ModelRc, SharedString, VecModel};

struct Request {
    action: String,
    body: Value,
}

fn selected_source(ui: &Dashboard) -> Option<String> {
    let index = ui.get_bt_selected();
    (index >= 0)
        .then(|| ui.get_bt_ids().row_data(index as usize))
        .flatten()
        .map(|value| value.to_string())
}

fn selected_evaluation(ui: &Dashboard) -> Option<String> {
    let index = ui.get_eval_selected();
    (index >= 0)
        .then(|| ui.get_eval_ids().row_data(index as usize))
        .flatten()
        .map(|value| value.to_string())
}

fn show_report(client: &Client, weak: &slint::Weak<Dashboard>, id: &str) -> anyhow::Result<()> {
    let value = client.post("/v1/evaluation/report", json!({"id":id}))?;
    let text = value["summary"].as_str().unwrap_or("").to_owned();
    let source = value["source_id"].as_str().unwrap_or("").to_owned();
    let evaluation = &value["evaluation"];
    let v1 = &evaluation["meta_v1"];
    let v2 = &evaluation["meta_v2"];
    let v1_net = v1["net_sum"].as_f64().unwrap_or(0.0);
    let v2_net = v2["net_sum"].as_f64().unwrap_or(0.0);
    let delta = evaluation["average_delta_net_v2_minus_v1"]
        .as_f64()
        .unwrap_or(0.0);
    let windows = evaluation["windows"].as_array().map_or(0, Vec::len);
    let id = id.to_owned();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_eval_report_id(id.into());
        ui.set_eval_report(text.into());
        ui.set_eval_source(format!("Source: {source}").into());
        ui.set_eval_v1(format!("{v1_net:.2}").into());
        ui.set_eval_v2(format!("{v2_net:.2}").into());
        ui.set_eval_delta(format!("{delta:+.2} / {windows} windows").into());
    });
    Ok(())
}

fn runs(client: &Client, weak: &slint::Weak<Dashboard>) -> anyhow::Result<()> {
    let value = client.get("/v1/evaluation/runs")?;
    let values = value["runs"]
        .as_array()
        .context("Invalid chronological evaluation list")?;
    let ids = values
        .iter()
        .map(|row| row["id"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let labels = values
        .iter()
        .map(|row| {
            let id = row["id"].as_str().unwrap_or("");
            let source = row["source_id"].as_str().unwrap_or("");
            let symbol = row["symbol"].as_str().unwrap_or("");
            let windows = row["windows"].as_u64().unwrap_or(0);
            let delta = row["average_delta_net_v2_minus_v1"]
                .as_f64()
                .unwrap_or(0.0);
            format!(
                "{} | {} | {} windows | delta {:+.2} | {}",
                symbol,
                source.chars().take(8).collect::<String>(),
                windows,
                delta,
                id.chars().take(8).collect::<String>()
            )
        })
        .collect::<Vec<_>>();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        let selected = ui.get_eval_selected();
        let old = if selected >= 0 {
            ui.get_eval_ids()
                .row_data(selected as usize)
                .map(|value| value.to_string())
        } else {
            None
        };
        let next = old
            .and_then(|id| ids.iter().position(|value| value == &id))
            .map(|index| index as i32)
            .unwrap_or(if ids.is_empty() { -1 } else { 0 });
        ui.set_eval_ids(ModelRc::new(VecModel::from(
            ids.into_iter().map(SharedString::from).collect::<Vec<_>>(),
        )));
        ui.set_eval_runs(ModelRc::new(VecModel::from(
            labels
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
        ui.set_eval_selected(next);
    });
    Ok(())
}

pub fn setup(ui: &Dashboard) {
    let (tx, rx) = mpsc::channel::<Request>();
    let weak = ui.as_weak();
    ui.on_evaluation_action(move |action| {
        let Some(ui) = weak.upgrade() else { return; };
        let action = action.to_string();
        let body = match action.as_str() {
            "start" => {
                let Some(source_id) = selected_source(&ui) else {
                    ui.set_feedback("اختر Backtest Meta محفوظاً كمصدر للتقييم أولاً".into());
                    return;
                };
                let plan = if ui.get_eval_plan() == 0 {
                    let Ok(percent) = ui.get_eval_holdout_percent().trim().parse::<u8>() else {
                        ui.set_feedback("نسبة holdout يجب أن تكون رقماً صحيحاً، مثال 30".into());
                        return;
                    };
                    if !(10..=50).contains(&percent) {
                        ui.set_feedback("نسبة holdout يجب أن تكون بين 10 و50".into());
                        return;
                    }
                    json!({"mode":"holdout","holdout_percent":percent})
                } else {
                    let parsed = [
                        ui.get_eval_development_days(),
                        ui.get_eval_holdout_days(),
                        ui.get_eval_step_days(),
                    ]
                    .iter()
                    .map(|value| value.trim().parse::<u16>())
                    .collect::<Result<Vec<_>, _>>();
                    let Ok(values) = parsed else {
                        ui.set_feedback("أيام rolling يجب أن تكون أرقاماً صحيحة".into());
                        return;
                    };
                    if values[0] < 2 || values[1] < 1 || values[2] < 1 {
                        ui.set_feedback("Rolling: development >= 2، holdout >= 1، step >= 1".into());
                        return;
                    }
                    json!({
                        "mode":"rolling",
                        "development_days":values[0],
                        "holdout_days":values[1],
                        "step_days":values[2]
                    })
                };
                json!({"source_id":source_id,"plan":plan})
            }
            "load" => {
                let Some(id) = selected_evaluation(&ui) else {
                    ui.set_feedback("اختر تقييماً زمنياً محفوظاً أولاً".into());
                    return;
                };
                json!({"id":id})
            }
            "export" => {
                let id = ui.get_eval_report_id();
                if id.is_empty() {
                    ui.set_feedback("افتح تقييماً محفوظاً أولاً".into());
                    return;
                }
                json!({"id":id.to_string()})
            }
            _ => json!({}),
        };
        ui.set_eval_busy(true);
        let _ = tx.send(Request { action, body });
    });

    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut client = None;
        let mut ready = false;
        let mut last_auto_id = String::new();
        loop {
            let request = match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(request) => Some(request),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if client.is_none() {
                client = Client::new(&data_dir()).ok();
            }
            let Some(api) = client.as_ref() else {
                if request.is_some() {
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.set_eval_busy(false);
                        ui.set_feedback("انتظر اتصال المحرك ثم أعد طلب التقييم".into());
                    });
                }
                continue;
            };
            if !ready {
                let _ = runs(api, &weak);
                ready = true;
            }
            if let Some(request) = request {
                let result = (|| -> anyhow::Result<String> {
                    match request.action.as_str() {
                        "load" => {
                            show_report(
                                api,
                                &weak,
                                request.body["id"].as_str().context("Missing evaluation id")?,
                            )?;
                            Ok("فُتح دليل التقييم الزمني المحفوظ".into())
                        }
                        "refresh" => {
                            runs(api, &weak)?;
                            Ok("تم تحديث قائمة التقييمات الزمنية".into())
                        }
                        action => {
                            let value = api.post(
                                &format!("/v1/evaluation/{action}"),
                                request.body,
                            )?;
                            Ok(if let Some(path) = value["path"].as_str() {
                                format!(
                                    "{}\n{}",
                                    path,
                                    value["note"].as_str().unwrap_or("تم التصدير")
                                )
                            } else {
                                "تم استلام الطلب؛ حالة التقييم تظهر أدناه".into()
                            })
                        }
                    }
                })();
                let message = result.unwrap_or_else(|error| error.to_string());
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_eval_busy(false);
                    ui.set_feedback(message.into());
                });
            }
            match api.get("/v1/evaluation") {
                Ok(value) => {
                    let state = value["state"].as_str().unwrap_or("idle");
                    let active = state == "running";
                    let text = format!(
                        "{} | {}% | {}",
                        state,
                        value["progress"].as_u64().unwrap_or(0),
                        value["message"].as_str().unwrap_or("")
                    );
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_eval_active(active);
                        ui.set_eval_status(text.into());
                    });
                    if let Some(id) = value["latest_id"]
                        .as_str()
                        .filter(|id| *id != last_auto_id)
                    {
                        if show_report(api, &weak, id).is_ok() {
                            last_auto_id = id.into();
                            let _ = runs(api, &weak);
                        }
                    }
                }
                Err(_) => {
                    client = None;
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.set_eval_active(false);
                        ui.set_eval_status(
                            "تعذر الاتصال بالمحرك؛ حالة التقييم غير معروفة".into(),
                        );
                    });
                }
            }
        }
    });
}
