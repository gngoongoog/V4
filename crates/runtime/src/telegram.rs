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
                shared.lock().unwrap().status.telegram_status = "معطّل أو غير مهيّأ".into();
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
            let url = format!("https://api.telegram.org/bot{token}/getUpdates");
            let result = http
                .post(url)
                .json(&json!({"offset":offset,"timeout":20,"allowed_updates":["message"]}))
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.json::<Value>());
            match result {
                Ok(v) if v["ok"] == true => {
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
                        "متصل — أوامر خاصة للقراءة فقط".into();
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
                                "القناص" => "/sniper",
                                "الاتصال" => "/health",
                                "مساعدة" => "/help",
                                other => other,
                            };
                            let response = if matches!(command,"/news"|"/backtest"|"/brains"|"/sniper") {
                                research_report(command).unwrap_or_else(|_| {
                                    "تعذر قراءة التقرير من المحرك؛ أعد المحاولة".into()
                                })
                            } else {
                                let mut s = shared.lock().unwrap();
                                s.status.refresh(now_ms());
                                s.status.report(command)
                            };
                            let text: String = response.chars().take(3800).collect();
                            let sent=http.post(format!("https://api.telegram.org/bot{token}/sendMessage")).json(&json!({"chat_id":config.telegram_chat_id,"text":text,"reply_markup":{"keyboard":[["الحالة","تقرير"],["الصفقات","الاتصال"],["العقول","القناص"],["الفرص","المحاكاة"],["الأخبار","الاختبار"],["مساعدة"]],"resize_keyboard":true}})).send().and_then(|r|r.error_for_status()).and_then(|r|r.json::<Value>());
                            if !matches!(sent,Ok(ref v) if v["ok"]==true) {
                                shared.lock().unwrap().status.telegram_status =
                                    "تعذر إرسال الرد؛ افحص الاتصال وصلاحيات البوت".into();
                            }
                        }
                    }
                }
                _ => {
                    shared.lock().unwrap().status.telegram_status =
                        "فشل الاتصال؛ افحص رمز البوت والشبكة وعدم تشغيل اتصال آخر للبوت"
                            .into();
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    });
}
fn research_report(command: &str) -> Result<String> {
    let c = Client::new(&data_dir())?;
    if command=="/brains" {
        let v=c.get("/v1/execution/brains")?;
        if v["controls_available"]!=true {return Ok("اربط الحساب وانتظر تحديث MT5 لعرض حالة العقول الحالية.".into())}
        let mut out=String::from("حالة العقول — الوضع المختار وحده ينفذ\nالإشارات مرصودة بعينة/ثانية؛ التدريب افتراضي ومنفصل عن الوسيط.\n");
        for (i,b) in v["brains"].as_array().into_iter().flatten().enumerate() {
            let name=braintrade_domain::execution::BRAINS.get(i).map(|(_,n)|*n).unwrap_or("عقل");
            out.push_str(&format!("{}\nإشارات {} | دخول منسوب {} | تعبئة مؤكدة {}\nتدريب مغلق {} / رابح {} | صافي افتراضي {:.2}\n",
                name,b["activity"]["signals"],b["attributed_orders"]["requested"],b["attributed_orders"]["fill_acknowledged"],
                b["paper_stats"]["closed"],b["paper_stats"]["wins"],b["paper_stats"]["net"].as_f64().unwrap_or(0.)));
        }return Ok(out);
    }
    if command=="/sniper" {
        let v=c.get("/v1/trading/settings")?;
        let t=&v["config"]["trading"];
        let style=match t["sniper_style"].as_str().unwrap_or("auto") {"impulse"=>"اندفاع سريع فقط","rebound"=>"ارتداد سريع فقط","confirmed"=>"اندفاع مؤكد بخمسة أسعار",_=>"تلقائي متعلّم بين الاندفاع والارتداد"};
        return Ok(format!("إعداد القناص المحفوظ\nالوضع الحالي قناص: {}\nالعقل: {}\nالأسلوب: {}\nأقصى مدة احتفاظ: {} ثانية\nفاصل الدخول: {} ثانية\nزمن القرار يقاس في صفحة التنفيذ؛ زمن تعبئة الوسيط يختلف ولا يضمنه أسلوب القناص.\nتغيير الإعدادات من الداشبورد أثناء توقف النظام.",
            if t["mode"]=="sniper" {"نعم"}else{"لا"},if t["sniper_brain"]=="learner" {"المتعلّم"}else{"الكلاسيكي — الأسلوب الفرعي لا يطبق عليه"},style,t["sniper_max_hold_seconds"],t["sniper_cooldown_seconds"]));
    }
    if command=="/backtest" {
        let state=c.get("/v1/backtest")?;
        let Some(id)=state["latest_id"].as_str() else{return Ok("لا يوجد اختبار محفوظ. شغّل الاختبار من الداشبورد أولاً.".into())};
        let r=c.post("/v1/backtest/report",json!({"id":id}))?;let m=&r["metrics"];
        return Ok(format!("آخر اختبار تاريخي — محاكاة وليست أرباح حساب MT5\nصفقات {} | رابحة {} | خاسرة {}\nصافي افتراضي {:.2} | أقصى تراجع {:.2}%\nالعمولات المحتسبة {:.2}",m["trades"],m["wins"],m["losses"],m["net"].as_f64().unwrap_or(0.),m["max_drawdown_percent"].as_f64().unwrap_or(0.),m["commissions"].as_f64().unwrap_or(0.)));
    }
    let v=c.get("/v1/news")?;
    let mut text=String::from("روابط الأخبار الرسمية المحفوظة\nالمحتوى الأصلي قد يكون بلغة المصدر؛ لا تتوفر ترجمة آلية موثوقة للعناوين.\n");
    for (i,item) in v["items"].as_array().into_iter().flatten().take(4).enumerate() {
        text.push_str(&format!("خبر {} | نشر {} بتوقيت غرينتش\n{}\n",i+1,item["published"].as_str().unwrap_or("غير متاح"),item["url"].as_str().unwrap_or("")));
    }
    if v["items"].as_array().is_none_or(|v|v.is_empty()) {text.push_str("لا توجد أخبار محفوظة حالياً.\n");}
    text.push_str("الأخبار سياق معلوماتي وليست توصية تداول.");Ok(text)
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
