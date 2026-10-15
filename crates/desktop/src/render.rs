use super::*;
use slint::platform::{software_renderer::MinimalSoftwareWindow, Platform};
use slint::Rgb8Pixel;
use std::{io::Write, rc::Rc};
struct Headless {
    window: Rc<MinimalSoftwareWindow>,
}
impl Platform for Headless {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
}
pub fn render() -> anyhow::Result<()> {
    let window = MinimalSoftwareWindow::new(Default::default());
    slint::platform::set_platform(Box::new(Headless {
        window: window.clone(),
    }))?;
    let ui = Dashboard::new()?;
    ui.set_state("STOPPED".into());
    ui.set_connection("MT5 is not connected / بانتظار الاتصال".into());
    ui.set_diagnostics(
        "Set your terminal path in Settings. No trade execution in this release.".into(),
    );
    ui.set_feedback("Research 0.5.0 • RENDER FIXTURE / no live data".into());
    ui.set_regime("UPTREND".into());
    ui.set_decision("WAIT".into());
    ui.set_brains("RENDER FIXTURE / بيانات فحص فقط\nTrend M1: BUY | evidence 0.72\nTrend M5: BUY | evidence 0.61\nTrend M15: BUY | evidence 0.58\nMomentum M1: BUY\nStructure M1: NEUTRAL\n\nالنظام متوقف؛ لا مداخل افتراضية جديدة\nقوة الدليل ليست نسبة نجاح".into());
    ui.set_paper("SHADOW / محاكاة فقط\nRENDER FIXTURE / no live data\nVirtual equity: 1000.00 | realized: 0.00\nClosed virtual trades: 0\n\nNo open virtual position\n\nSpread, configured commission and slippage included.\nNo MT5 orders.".into());
    ui.set_opportunities("RENDER FIXTURE — لا توجد فرصة حالية\nمنطقة طلب | انتظار تأكيد\nالدخول والوقف والأهداف مسحوبة أثناء الإيقاف".into());
    ui.set_market_map("RENDER FIXTURE\nعقل المناطق: support / demand\nعقل الزخم: RSI 52 / اتجاه محايد\nGann: observational\nDOM: غير متاح — لا يتم اختلاق عمق السوق\nملف النشاط: ticks proxy فقط".into());
    ui.set_learning_summary("RENDER FIXTURE\nعينات: 900 | نتائج ناضجة: 870 | انتظار: 30\nChallenger: نموذج توضيحي فقط\nالتوقعات للمراقبة؛ لا تأثير على Shadow أو MT5".into());
    ui.set_learning_detail("لا بيانات سوق حقيقية ضمن فحص رسم الواجهة".into());
    ui.set_backup_names(slint::ModelRc::new(slint::VecModel::from(vec!["foundation-fixture".into()])));
    ui.set_backup_detail("RENDER FIXTURE — نسخة توضيحية فقط\nصالحة للاستعادة\nآخر نتيجة: applied\nأسرار Telegram لم تتغير".into());
    ui.set_bt_from("2026-10-01".into());
    ui.set_bt_to("2026-10-08".into());
    ui.set_bt_status("RENDER FIXTURE / لا توجد نتائج سوق حقيقية".into());
    ui.set_news_status("RENDER FIXTURE / محتوى فحص الواجهة".into());
    ui.set_news_sources("Federal Reserve | لا توجد بيانات حديثة\nECB Press | لا توجد بيانات حديثة\nECB Statistics | لا توجد بيانات حديثة".into());
    ui.set_news_empty(false);
    ui.set_news_items(slint::ModelRc::new(slint::VecModel::from(vec![NewsRow {
        title: "UI TEST — Example monetary policy headline".into(),
        detail: "Example source | 2026-10-01 14:00 UTC\nبيانات تصميم فقط، ليست أخباراً حقيقية"
            .into(),
        url: "https://www.federalreserve.gov/".into(),
    }])));

    ui.set_ev_source_ids(slint::ModelRc::new(slint::VecModel::from(vec![
        "render-meta-source".into(),
    ])));
    ui.set_ev_source_runs(slint::ModelRc::new(slint::VecModel::from(vec![
        "Meta v2 | FIXTURE / XAUUSD | 2026-10-01 → 2026-10-08".into(),
    ])));
    ui.set_ev_source_selected(0);
    ui.set_ev_ids(slint::ModelRc::new(slint::VecModel::from(vec![
        "render-evaluation".into(),
    ])));
    ui.set_ev_runs(slint::ModelRc::new(slint::VecModel::from(vec![
        "holdout | FIXTURE / XAUUSD | windows 2 | Δnet +18.40 | src render-m | render-e".into(),
    ])));
    ui.set_ev_selected(0);
    ui.set_ev_status("completed | 100% | RENDER FIXTURE — immutable chronological evidence".into());
    ui.set_ev_v1_net("+42.10".into());
    ui.set_ev_v2_net("+60.50".into());
    ui.set_ev_delta("+18.40".into());
    ui.set_ev_report("CHRONOLOGICAL EVALUATION / تقييم زمني\nRENDER FIXTURE — ليست بيانات سوق حقيقية\nPlan: Holdout 30%\nFIXTURE / 77 / XAUUSD\nWindows: 2\nMeta v1: 18 trades | net-sum 42.10 | avg return 0.21% | worst DD 1.20%\nMeta v2: 17 trades | net-sum 60.50 | avg return 0.30% | worst DD 1.10%\nAverage delta v2-v1: net +18.40 | return +0.09%\n\nPER-WINDOW EVIDENCE\nCycle 0 | development | UTC epoch 1790812800 → 1791244800\n  Meta v1: trades 11 | net 25.40 | return 0.25% | DD 1.00%\n  Meta v2: trades 10 | net 34.20 | return 0.34% | DD 0.95%\n  Δ v2-v1: net +8.80 | return +0.09% | DD -0.05%\n\nCycle 0 | holdout | UTC epoch 1791244800 → 1791417600\n  Meta v1: trades 7 | net 16.70 | return 0.17% | DD 1.20%\n  Meta v2: trades 7 | net 26.30 | return 0.26% | DD 1.10%\n  Δ v2-v1: net +9.60 | return +0.09% | DD -0.10%\n\nImmutable evidence: true | Prediction enabled: false | Promotion enabled: false".into());

    let output = std::path::Path::new("test-output").join("native-ui");
    std::fs::create_dir_all(&output)?;
    ui.set_studio_frames(std::rc::Rc::new(slint::VecModel::from(vec!["ALL".into(),"M1".into(),"H1".into()])).into());
    ui.set_studio_summary("RENDER FIXTURE — بيانات اصطناعية\nM1: 600 شمعة | H1: 600 شمعة\n40 تركيباً؛ اختبار نهائي محفوظ".into());
    ui.set_cost_detail("RENDER FIXTURE / USD\n0.1 لوت × 7 = 0.70 عمولة\nسبريد 2 نقطة = 0.20\nانزلاق 0.40 | إجمالي 1.30\nالتعادل 13 نقطة / 1.3 pip".into());
    ui.set_wyckoff_detail("RENDER FIXTURE — ليس تحليلاً حقيقياً\nM1 | C → D محتملة\nSpring + Test | دعم 98 | مقاومة 102\nالحجم: tick_activity_proxy\nاختبار أقل نشاطاً بعد اختراق الدعم والعودة\nنتائج مقاسة: 0 | ينتظر 12 شمعة لاحقة".into());
    ui.show()?;
    for (width, height, suffix) in [(1120, 820, ""), (880, 700, "-small")] {
        window.set_size(slint::PhysicalSize::new(width, height));
        for page in 0..17 {
            ui.set_page(page);
            ui.set_events("#1 | core_started\nversion: 0.5.0 • execution_enabled: false\n\nUI render check — not live trading data".into());
            slint::platform::update_timers_and_animations();
            ui.window().request_redraw();
            let mut pixels = vec![Rgb8Pixel::default(); (width * height) as usize];
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, width as usize);
            });
            let path = output.join(format!("dashboard-page-{page}{suffix}.ppm"));
            let mut file = std::fs::File::create(path)?;
            write!(file, "P6\n{width} {height}\n255\n")?;
            for pixel in pixels {
                file.write_all(&[pixel.r, pixel.g, pixel.b])?;
            }
        }
    }
    ui.hide()?;
    Ok(())
}
