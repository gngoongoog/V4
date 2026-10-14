use super::*;

pub fn spawn(shared: Arc<Mutex<Shared>>) {
    std::thread::spawn(move || {
        let Ok(http) = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
        else {
            return;
        };
        let mut offset = 0i64;
        let mut identity = String::new();
        loop {
            let (config, token) = {
                let s = shared.lock().unwrap();
                (s.config.clone(), s.token.clone())
            };
            if !config.telegram_enabled || token.is_none() {
                shared.lock().unwrap().status.telegram_status = "Disabled / not configured".into();
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            let token = token.unwrap();
            let current_identity = format!(
                "{}:{}:{}",
                token, config.telegram_user_id, config.telegram_chat_id
            );
            if current_identity != identity {
                offset = 0;
                identity = current_identity;
            }
            // Never log reqwest errors: their URLs include the Telegram token.
            let url = format!("https://api.telegram.org/bot{token}/getUpdates");
            let result = http
                .post(url)
                .json(&json!({"offset":offset,"timeout":20,"allowed_updates":["message"]}))
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.json::<Value>());
            match result {
                Ok(v) if v["ok"] == true => {
                    // Ignore an in-flight response if settings changed while polling.
                    {
                        let s = shared.lock().unwrap();
                        if !s.config.telegram_enabled
                            || s.config.telegram_user_id != config.telegram_user_id
                            || s.config.telegram_chat_id != config.telegram_chat_id
                            || s.token.as_deref() != Some(&token)
                        {
                            continue;
                        }
                    }
                    shared.lock().unwrap().status.telegram_status =
                        "Connected (private read-only commands)".into();
                    if let Some(updates) = v["result"].as_array() {
                        for update in updates {
                            let Some(id) = update["update_id"].as_i64() else {
                                continue;
                            };
                            offset = offset.max(id + 1);
                            let m = &update["message"];
                            if !authorized(m, &config) {
                                continue;
                            }
                            let timestamp = m["date"].as_u64().unwrap_or(0);
                            if now_ms() / 1000 > timestamp + 120 {
                                continue;
                            }
                            let raw = m["text"].as_str().unwrap_or("");
                            let first = raw.split_whitespace().next().unwrap_or("");
                            let command = first.split('@').next().unwrap_or("");
                            let command = match command {
                                "تقرير" => "/report",
                                "الحالة" => "/status",
                                "الصفقات" => "/positions",
                                "العقول" => "/brains",
                                "الفرص" | "فرص" => "/opportunities",
                                "المحاكاة" => "/paper",
                                "الأخبار" | "اخبار" => "/news",
                                "الاختبار" => "/backtest",
                                other => other,
                            };
                            let response = if command == "/news" || command == "/backtest" {
                                research_report(command).unwrap_or_else(|_| {
                                    "تعذر قراءة التقرير من المحرك؛ أعد المحاولة".into()
                                })
                            } else {
                                let mut s = shared.lock().unwrap();
                                s.status.refresh(now_ms());
                                s.status.report(command)
                            };
                            let text: String = response.chars().take(3800).collect();
                            let sent=http.post(format!("https://api.telegram.org/bot{token}/sendMessage")).json(&json!({"chat_id":config.telegram_chat_id,"text":text,"reply_markup":{"keyboard":[["/status","/report"],["/positions","/health"],["/brains","/opportunities"],["/paper","/news"],["/backtest","/help"]],"resize_keyboard":true}})).send().and_then(|r|r.error_for_status()).and_then(|r|r.json::<Value>());
                            if !matches!(sent,Ok(ref v) if v["ok"]==true) {
                                shared.lock().unwrap().status.telegram_status =
                                    "Reply failed; check connectivity / bot permissions".into();
                            }
                        }
                    }
                }
                _ => {
                    shared.lock().unwrap().status.telegram_status =
                        "Connection failed; check token, network, or existing webhook/poller"
                            .into();
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    });
}
fn research_report(command: &str) -> Result<String> {
    let c = Client::new(&data_dir())?;
    if command == "/backtest" {
        let state = c.get("/v1/backtest")?;
        let Some(id) = state["latest_id"].as_str() else {
            return Ok("لا يوجد اختبار محفوظ. شغّل الاختبار من الداشبورد أولاً.".into());
        };
        let r = c.post("/v1/backtest/report", json!({"id":id}))?;
        return Ok(format!(
            "آخر اختبار محفوظ — ليس تقرير حساب MT5\n{}",
            r["summary"].as_str().unwrap_or("غير متاح")
        ));
    }
    let v = c.get("/v1/news")?;
    let mut text = format!(
        "أخبار رسمية محفوظة\n{}\n\n",
        v["sources_text"].as_str().unwrap_or("الحالة غير متاحة")
    );
    for item in v["items"].as_array().into_iter().flatten().take(4) {
        text.push_str(&format!(
            "{}\n{} | {}\n{}\n\n",
            item["title"].as_str().unwrap_or(""),
            item["source"].as_str().unwrap_or(""),
            item["published"].as_str().unwrap_or("غير متاح"),
            item["url"].as_str().unwrap_or("")
        ));
    }
    text.push_str("وقت النشر UTC؛ العناوين ليست توصية أو تقويم أحداث مستقبلية.");
    Ok(text)
}
fn authorized(message: &Value, config: &Config) -> bool {
    message["chat"]["type"] == "private"
        && message["from"]["id"]
            .as_u64()
            .map(|v| v.to_string())
            .as_deref()
            == Some(&config.telegram_user_id)
        && message["chat"]["id"]
            .as_u64()
            .map(|v| v.to_string())
            .as_deref()
            == Some(&config.telegram_chat_id)
        && message["from"]["is_bot"] != true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_other_users_and_groups() {
        let c = Config {
            telegram_user_id: "42".into(),
            telegram_chat_id: "42".into(),
            ..Default::default()
        };
        let mut m = json!({"from":{"id":42,"is_bot":false},"chat":{"id":42,"type":"private"}});
        assert!(authorized(&m, &c));
        m["from"]["id"] = json!(43);
        assert!(!authorized(&m, &c));
        m["from"]["id"] = json!(42);
        m["chat"]["type"] = json!("group");
        assert!(!authorized(&m, &c));
    }
}
