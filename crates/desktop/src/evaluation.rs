use super::*;
use anyhow::Context;
use slint::{Model, ModelRc, SharedString, VecModel};

struct Request {
    action: String,
    body: Value,
}

fn preserve_selection(
    ui: &Dashboard,
    old_index: i32,
    old_model: ModelRc<SharedString>,
    ids: &[String],
) -> i32 {
    let old = if old_index >= 0 {
        old_model
            .row_data(old_index as usize)
            .map(|value| value.to_string())
    } else {
        None
    };
    old.and_then(|id| ids.iter().position(|value| value == &id))
        .map(|index| index as i32)
        .unwrap_or(if ids.is_empty() { -1 } else { 0 })
}

fn source_runs(client: &Client, weak: &slint::Weak<Dashboard>) -> anyhow::Result<()> {
    let value = client.get("/v1/backtest/runs")?;
    let rows = value["runs"]
        .as_array()
        .context("Invalid saved backtest list")?;
    let filtered = rows
        .iter()
        .filter(|row| {
            row["algorithm"]
                .as_str()
                .is_some_and(|algorithm| algorithm.starts_with("meta-v1") || algorithm.starts_with("meta-v2"))
        })
        .collect::<Vec<_>>();
    let ids = filtered
        .iter()
        .map(|row| row["id"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let labels = filtered
        .iter()
        .map(|row| row["label"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        let next = preserve_selection(
            &ui,
            ui.get_ev_source_selected(),
            ui.get_ev_source_ids(),
            &ids,
        );
        ui.set_ev_source_ids(ModelRc::new(VecModel::from(
            ids.into_iter().map(SharedString::from).collect::<Vec<_>>(),
        )));
        ui.set_ev_source_runs(ModelRc::new(VecModel::from(
            labels
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
        ui.set_ev_source_selected(next);
    });
    Ok(())
}

fn evidence_runs(client: &Client, weak: &slint::Weak<Dashboard>) -> anyhow::Result<()> {
    let value = client.get("/v1/evaluation/runs")?;
    let rows = value["runs"]
        .as_array()
        .context("Invalid chronological evaluation list")?;
    let ids = rows
        .iter()
        .map(|row| row["id"].as_str().unwrap_or("").to_owned())
        .collect::<Vec<_>>();
    let labels = rows
        .iter()
        .map(|row| {
            let source = row["source_id"].as_str().unwrap_or("");
            let short_source = source.chars().take(8).collect::<String>();
            let id = row["id"].as_str().unwrap_or("");
            let short_id = id.chars().take(8).collect::<String>();
            let mode = row["plan"]["mode"].as_str().unwrap_or("?");
            format!(
                "{} | {} / {} | windows {} | Δnet {:.2} | src {} | {}",
                mode,
                row["server"].as_str().unwrap_or(""),
                row["symbol"].as_str().unwrap_or(""),
                row["windows"].as_u64().unwrap_or(0),
                row["average_delta_net_v2_minus_v1"]
                    .as_f64()
                    .unwrap_or(0.0),
                short_source,
                short_id
            )
        })
        .collect::<Vec<_>>();
    let _ = weak.upgrade_in_event_loop(move |ui| {
        let next = preserve_selection(&ui, ui.get_ev_selected(), ui.get_ev_ids(), &ids);
        ui.set_ev_ids(ModelRc::new(VecModel::from(
            ids.into_iter().map(SharedString::from).collect::<Vec<_>>(),
        )));
        ui.set_ev_runs(ModelRc::new(VecModel::from(
            labels
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
        ui.set_ev_selected(next);
    });
    Ok(())
}

fn window_text(value: &Value) -> String {
    let mut rows = Vec::new();
    for item in value["evaluation"]["windows"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let window = &item["window"];
        let role = window["role"].as_str().unwrap_or("?");
        let cycle = window["cycle"].as_u64().unwrap_or(0);
        let v1 = &item["meta_v1"]["metrics"];
        let v2 = &item["meta_v2"]["metrics"];
        rows.push(format!(
            "Cycle {} | {} | UTC epoch {} → {}\n  Meta v1: trades {} | net {:.2} | return {:.2}% | DD {:.2}%\n  Meta v2: trades {} | net {:.2} | return {:.2}% | DD {:.2}%\n  Δ v2-v1: net {:+.2} | return {:+.2}% | DD {:+.2}%",
            cycle,
            role,
            window["from"].as_u64().unwrap_or(0),
            window["to"].as_u64().unwrap_or(0),
            v1["trades"].as_u64().unwrap_or(0),
            v1["net"].as_f64().unwrap_or(0.0),
            v1["return_percent"].as_f64().unwrap_or(0.0),
            v1["max_drawdown_percent"].as_f64().unwrap_or(0.0),
            v2["trades"].as_u64().unwrap_or(0),
            v2["net"].as_f64().unwrap_or(0.0),
            v2["return_percent"].as_f64().unwrap_or(0.0),
            v2["max_drawdown_percent"].as_f64().unwrap_or(0.0),
            item["delta_net_v2_minus_v1"].as_f64().unwrap_or(0.0),
            item["delta_return_percent_v2_minus_v1"]
                .as_f64()
                .unwrap_or(0.0),
            item["delta_drawdown_percent_v2_minus_v1"]
                .as_f64()
                .unwrap_or(0.0),
        ));
    }
    rows.join("\n\n")
}

fn show_report(client: &Client, weak: &slint::Weak<Dashboard>, id: &str) -> anyhow::Result<()> {
    let value = client.post("/v1/evaluation/report", json!({"id": id}))?;
    let id = id.to_owned();
    let summary = value["summary"].as_str().unwrap_or("").to_owned();
    let windows = window_text(&value);
    let text = format!(
        "{}\n\nPER-WINDOW EVIDENCE / تفاصيل النوافذ\n{}\n\nImmutable evidence: {} | Prediction enabled: {} | Promotion enabled: {}",
        summary,
        if windows.is_empty() {
            "لا توجد نوافذ محفوظة".into()
        } else {
            windows
        },
        value["immutable"].as_bool().unwrap_or(false),
        value["prediction_enabled"].as_bool().unwrap_or(false),
        value["promotion_enabled"].as_bool().unwrap_or(false)
    );
    let evaluation = &value["evaluation"];
    let v1 = &evaluation["meta_v1"];
    let v2 = &evaluation["meta_v2"];
    let v1_net = format!("{:.2}", v1["net_sum"].as_f64().unwrap_or(0.0));
    let v2_net = format!("{:.2}", v2["net_sum"].as_f64().unwrap_or(0.0));
    let delta = format!(
        "{:+.2}",
        evaluation["average_delta_net_v2_minus_v1"]
            .as_f64()
            .unwrap_or(0.0)
    );
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_ev_report_id(id.into());
        ui.set_ev_report(text.into());
        ui.set_ev_v1_net(v1_net.into());
        ui.set_ev_v2_net(v2_net.into());
        ui.set_ev_delta(delta.into());
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
                let source_index = ui.get_ev_source_selected();
                let source_id = if source_index >= 0 {
                    ui.get_ev_source_ids().row_data(source_index as usize)
                } else {
                    None
                };
                let Some(source_id) = source_id else {
                    ui.set_feedback("اختر اختبار Meta محفوظاً كمصدر للتقييم".into());
                    return;
                };
                let plan = if ui.get_ev_plan() == 0 {
                    let Ok(percent) = ui.get_ev_holdout_percent().trim().parse::<u8>() else {
                        ui.set_feedback("نسبة Holdout يجب أن تكون رقماً صحيحاً، مثال 30".into());
                        return;
                    };
                    json!({"mode":"holdout","holdout_percent":percent})
                } else {
                    let Ok(development_days) = ui.get_ev_development_days().trim().parse::<u16>() else {
                        ui.set_feedback("أيام development يجب أن تكون رقماً صحيحاً".into());
                        return;
                    };
                    let Ok(holdout_days) = ui.get_ev_holdout_days().trim().parse::<u16>() else {
                        ui.set_feedback("أيام holdout يجب أن تكون رقماً صحيحاً".into());
                        return;
                    };
                    let Ok(step_days) = ui.get_ev_step_days().trim().parse::<u16>() else {
                        ui.set_feedback("أيام خطوة rolling يجب أن تكون رقماً صحيحاً".into());
                        return;
                    };
                    json!({"mode":"rolling","development_days":development_days,"holdout_days":holdout_days,"step_days":step_days})
                };
                json!({"source_id":source_id.to_string(),"plan":plan})
            }
            "load" | "export" => {
                let index = ui.get_ev_selected();
                let id = if index >= 0 {
                    ui.get_ev_ids().row_data(index as usize)
                } else {
                    None
                };
                let Some(id) = id else {
                    ui.set_feedback("اختر دليل تقييم محفوظاً أولاً".into());
                    return;
                };
                json!({"id":id.to_string()})
            }
            _ => json!({}),
        };
        ui.set_ev_busy(true);
        let _ = tx.send(Request { action, body });
    });

    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut client = None;
        let mut ready = false;
        let mut last_auto_id = String::new();
        loop {
            let request = match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(value) => Some(value),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            if client.is_none() {
                client = Client::new(&data_dir()).ok();
            }
            let Some(c) = client.as_ref() else {
                if request.is_some() {
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.set_ev_busy(false);
                        ui.set_feedback("انتظر اتصال المحرك ثم أعد الطلب".into());
                    });
                }
                continue;
            };
            if !ready {
                if source_runs(c, &weak).is_ok() && evidence_runs(c, &weak).is_ok() {
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
                                request.body["id"].as_str().context("Missing evaluation id")?,
                            )?;
                            Ok("فُتح دليل التقييم المحفوظ".into())
                        }
                        "refresh" => {
                            source_runs(c, &weak)?;
                            evidence_runs(c, &weak)?;
                            Ok("تم تحديث مصادر Meta وأدلة التقييم".into())
                        }
                        "export" => {
                            let value = c.post("/v1/evaluation/export", request.body)?;
                            Ok(format!(
                                "{}\n{}",
                                value["path"].as_str().unwrap_or(""),
                                value["note"].as_str().unwrap_or("تم التصدير")
                            ))
                        }
                        action => {
                            c.post(&format!("/v1/evaluation/{action}"), request.body)?;
                            Ok("تم استلام الطلب؛ راقب حالة التقييم أدناه".into())
                        }
                    }
                })();
                let message = match result {
                    Ok(value) => value,
                    Err(error) => error.to_string(),
                };
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_ev_busy(false);
                    ui.set_feedback(message.into());
                });
            }
            match c.get("/v1/evaluation") {
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
                        ui.set_ev_active(active);
                        ui.set_ev_status(text.into());
                    });
                    if let Some(id) = value["latest_id"]
                        .as_str()
                        .filter(|id| *id != last_auto_id)
                    {
                        if show_report(c, &weak, id).is_ok() {
                            last_auto_id = id.into();
                            let _ = evidence_runs(c, &weak);
                        }
                    }
                }
                Err(_) => {
                    client = None;
                    ready = false;
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.set_ev_active(false);
                        ui.set_ev_status(
                            "تعذر الاتصال بالمحرك؛ حالة التقييم الحالية غير معروفة".into(),
                        );
                    });
                }
            }
        }
    });
}
