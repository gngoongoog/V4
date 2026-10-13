use super::*;
use slint::{ModelRc, VecModel};
pub fn setup(ui: &Dashboard) {
    let (tx, rx) = mpsc::channel::<String>();
    ui.on_news_action(move |a| {
        let _ = tx.send(a.to_string());
    });
    let weak = ui.as_weak();
    ui.on_open_news(move |url| {
        if !braintrade_runtime::news::allowed_article_url(&url) {
            return;
        }
        #[cfg(windows)]
        let opener = std::env::var_os("WINDIR")
            .map(std::path::PathBuf::from)
            .map(|p| p.join("explorer.exe"))
            .unwrap_or_else(|| "explorer.exe".into());
        #[cfg(not(windows))]
        let opener = std::path::PathBuf::from("xdg-open");
        if std::process::Command::new(opener)
            .arg(url.as_str())
            .spawn()
            .is_err()
        {
            if let Some(ui) = weak.upgrade() {
                ui.set_feedback("تعذر فتح رابط المصدر في المتصفح".into());
            }
        }
    });
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut client = None;
        loop {
            let action = match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(a) => Some(a),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(_) => break,
            };
            if client.is_none() {
                client = Client::new(&data_dir()).ok();
            }
            let Some(c) = client.as_ref() else {
                continue;
            };
            if let Some(a) = action {
                let response = match a.as_str() {
                    "refresh" => c.post("/v1/news/refresh", json!({})),
                    "enable" | "disable" => {
                        c.post("/v1/news/settings", json!({"enabled":a=="enable"}))
                    }
                    _ => continue,
                };
                let message = match response {
                    Ok(v) => v["note"].as_str().unwrap_or("تم").to_owned(),
                    Err(e) => e.to_string(),
                };
                let _ = weak.upgrade_in_event_loop(move |ui| ui.set_feedback(message.into()));
            }
            match c.get("/v1/news") {
                Ok(v) => {
                    let enabled = v["enabled"].as_bool().unwrap_or(false);
                    let refreshing = v["refreshing"].as_bool().unwrap_or(false);
                    let state = if v["offline"] == true {
                        "وضع دون شبكة — المحفوظ فقط"
                    } else if !enabled {
                        "التحديث التلقائي متوقف — المحفوظ فقط"
                    } else if refreshing {
                        "جارٍ تحديث المصادر…"
                    } else {
                        "تحديث تلقائي كل 5 دقائق؛ قد يتأخر نشر المصدر"
                    };
                    let state = format!("{}\n{}", state, v["storage_error"].as_str().unwrap_or(""));
                    let sources = v["sources_text"].as_str().unwrap_or("").to_owned();
                    let rows = v["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|r| NewsRow {
                            title: r["title"].as_str().unwrap_or("").into(),
                            detail: format!(
                                "{} | {}\n{}{}",
                                r["source"].as_str().unwrap_or(""),
                                r["published"].as_str().unwrap_or(""),
                                r["topic"].as_str().unwrap_or(""),
                                if r["source_stale"] == true {
                                    " | محفوظ / المصدر غير حديث"
                                } else {
                                    ""
                                }
                            )
                            .into(),
                            url: r["url"].as_str().unwrap_or("").into(),
                        })
                        .collect::<Vec<_>>();
                    let empty = rows.is_empty();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_news_enabled(enabled);
                        ui.set_news_refreshing(refreshing);
                        ui.set_news_status(state.into());
                        ui.set_news_sources(sources.into());
                        ui.set_news_empty(empty);
                        ui.set_news_items(ModelRc::new(VecModel::from(rows)));
                    });
                }
                Err(_) => {
                    client = None;
                    let _ = weak.upgrade_in_event_loop(|ui| {
                        ui.set_news_status("الاتصال بالمحرك غير متاح؛ المعروض محفوظ وقديم".into())
                    });
                }
            }
        }
    });
}
