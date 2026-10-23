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
    // Synthetic render-only candles demonstrate layout fidelity in CI.
    // This function is never executed during the real dashboard loop.
    ui.set_render_fixture(true);
    // Deliberately show the priority console on CI screenshots while the
    // Core remains disconnected: visuals only, never a broker connection.
    ui.set_priority_enabled(true);
    ui.set_sniper_active(true);
    ui.set_fallback_mode(1); // Swing is the preview standby.
    ui.set_trade_mode(3);
    let visual_bars: Vec<ChartCandle> = (0..66).map(|i| {
        let t = i as f32;
        let wave = (t * 0.22).sin() * 0.13 + (t * 0.07).sin() * 0.08;
        let middle = (0.54 - wave).clamp(0.26, 0.78);
        let up = i % 7 != 0 && i % 11 != 0 && i % 3 != 0;
        let open = (middle + if up {0.027} else {-0.016}).clamp(0.17,0.88);
        let close = (middle + if up {-0.021} else {0.029}).clamp(0.16,0.89);
        let top = open.min(close);
        let bottom = open.max(close);
        ChartCandle {
            x: (t + 0.5) / 66.0,
            high_y: (top - 0.025).max(0.03),
            low_y: (bottom + 0.026).min(0.98),
            body_top: top,
            body_bottom: bottom,
            up,
            volume: ((i * 19 % 49) as f32 + 8.0) / 65.0,
        }
    }).collect();
    ui.set_chart_bars(slint::ModelRc::new(slint::VecModel::from(visual_bars)));
    ui.set_state("STOPPED".into());
    ui.set_connection("MT5 is not connected / بانتظار الاتصال".into());
    ui.set_diagnostics(
        "Set the terminal path in Settings, bind the Demo account, then START.".into(),
    );
    ui.set_feedback(
        format!(
            "Research {} • RENDER FIXTURE / no live data",
            env!("CARGO_PKG_VERSION")
        )
        .into(),
    );
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

    ui.set_corr_sources(slint::ModelRc::new(slint::VecModel::from(vec![
        "XAUUSD | 7@Demo | 1790812800 → 1791417600 | gold-fixture".into(),
        "EURUSD | 7@Demo | 1790812800 → 1791417600 | euro-fixture".into(),
        "GBPUSD | 7@Demo | 1790812800 → 1791417600 | gbp-fixture".into(),
    ])));
    ui.set_corr_source_ids(slint::ModelRc::new(slint::VecModel::from(vec![
        "gold-fixture".into(), "euro-fixture".into(), "gbp-fixture".into(),
    ])));
    ui.set_corr_source_a(0);
    ui.set_corr_source_b(1);
    ui.set_corr_reports(slint::ModelRc::new(slint::VecModel::from(vec![
        "XAUUSD × EURUSD | pairs 1/1 | common 420 | deadbeef".into(),
    ])));
    ui.set_corr_report_ids(slint::ModelRc::new(slint::VecModel::from(vec!["deadbeef".into()])));
    ui.set_corr_report_selected(0);
    ui.set_corr_status("RENDER FIXTURE — saved backtests only / no live feed".into());
    ui.set_corr_title("XAUUSD × EURUSD | 1790812800 → 1791417600 | min overlap 30 | ID deadbeef".into());
    ui.set_corr_boundary("RESEARCH ONLY — saved backtests only; no live feed, no forward-fill, no broker effect.".into());
    ui.set_corr_matrix("PAIRWISE CORRELATION / ارتباط العوائد المتزامنة\nEURUSD × XAUUSD | r 0.4210 | cov 0.00000120 | n 420".into());
    ui.set_corr_portfolio("COMMON-INTERSECTION PORTFOLIO / تقاطع كل الرموز\nSamples: 420\nVolatility: 0.00124000\nDiversification ratio: 1.3100\nAvailable: true".into());
    ui.set_corr_provenance("1: gold-fixture\n2: euro-fixture".into());

    let output = std::path::Path::new("test-output").join("native-ui");
    std::fs::create_dir_all(&output)?;
    ui.set_studio_frames(std::rc::Rc::new(slint::VecModel::from(vec!["ALL".into(),"M1".into(),"H1".into()])).into());
    ui.set_studio_summary("RENDER FIXTURE — بيانات اصطناعية\nM1: 600 شمعة | H1: 600 شمعة\n40 تركيباً؛ اختبار نهائي محفوظ".into());
    ui.set_trade_mode(3);
    ui.set_sniper_brain(1);
    ui.set_cost_detail("RENDER FIXTURE / USD\n0.1 لوت × 7 = 0.70 عمولة\nسبريد 2 نقطة = 0.20\nانزلاق 0.40 | إجمالي 1.30\nالتعادل 13 نقطة / 1.3 pip".into());
    ui.set_wyckoff_detail("RENDER FIXTURE — ليس تحليلاً حقيقياً\nM1 | C → D محتملة\nSpring + Test | دعم 98 | مقاومة 102\nالحجم: tick_activity_proxy\nاختبار أقل نشاطاً بعد اختراق الدعم والعودة\nنتائج مقاسة: 0 | ينتظر 12 شمعة لاحقة".into());
    ui.set_exec_summary("RENDER FIXTURE — Demo / stopped / no broker connected".into());
    ui.set_exec_role("advisor".into());
    ui.set_exec_names(slint::ModelRc::new(slint::VecModel::from(braintrade_domain::execution::BRAINS.iter().map(|b|slint::SharedString::from(b.1)).collect::<Vec<_>>())));
    ui.set_exec_frames(slint::ModelRc::new(slint::VecModel::from(vec!["M1".into(),"M5".into(),"H1".into()])));
    ui.set_exec_selected(3);
    ui.set_exec_detail("RENDER FIXTURE — لا أوامر سوق حقيقية\nالترند | عضو تابع للرئيسي\nSTART/STOP مستقل عن زر العضوية".into());
    ui.set_trade_journal_summary("RENDER FIXTURE — MT5 Demo / 1 صفقة".into());
    ui.set_trade_journal_rows(slint::ModelRc::new(slint::VecModel::from(vec!["#9001 | CLOSED | sniper | 1.30 USD".into()])));
    ui.set_trade_journal_selected(0);
    ui.set_trade_journal_detail("RENDER FIXTURE — صفقة مغلقة وهمية للفحص فقط\\nالعقل: sniper_learner | الوضع: sniper\\nصافي MT5 المحاكى: 1.30 USD".into());
    ui.set_student_status("RENDER FIXTURE — بيانات اصطناعية لتفقد الواجهة".into());
    ui.set_student_total("62".into()); ui.set_student_closed("58".into()); ui.set_student_pairs("41".into());
    ui.set_student_patterns("التكاليف محت الربح: 7\nرجوع ربح مرصود: 12\nربح بعد التكاليف: 27".into());
    ui.set_student_policies("trend / M1 / expert\nاختبار على صفقات جديدة • 13 / 20 مقارنة\nالمطبّق: الإعداد الأصلي | المرشح: خروج أسرع\nآخر فرق متوسط: +0.000 R\nتثبيت البديل؛ بدأ الاختبار اللاحق".into());
    ui.set_student_timing("فرص مقترحة: 62 | مقارنات دخلت الدراسة: 41\nبحث فقط — التوقيت المقترح لا يُطبّق على أوامر MT5.\nالرصد لا يغطي توقيت الدخول بالدقة المطلوبة: 5\ntrend / M1 | اختبار على صفقات جديدة • 13 / 20\nالمقترح بعد التقييم: 0 ms | المرشح: 250 ms\nفرق الصافي الافتراضي: +0.080 R".into());
    ui.set_student_journal("trend | صفقة 1001 | صافي MT5: -0.80\ntrend_up_normal / london | ارتداد بعد اتجاه\nاستكشاف 12% • عينات السياق 8 | الخروج: الإعداد الأصلي\nMFE مرصود 0.42000 / MAE 0.85000 • رصد الصفقة 24\nمقارنة الخروج: اكتملت • أكبر فجوة 250 ms".into());
    ui.show()?;
    ui.set_brain_monitor_names(slint::ModelRc::new(slint::VecModel::from(vec![slint::SharedString::from("القناص المتعلّم")])));
    ui.set_brain_monitor_summary("بيانات تصميم فقط • القناص • الحساب 77 / تجربة".into());
    ui.set_brain_monitor_detail("مراقبة/تدريب؛ الوضع المختار وحده يرسل الأوامر
آخر إشارة: شراء | بانتظار تكلفة مناسبة
زمن الحساب: 120 ميكروثانية، ليس زمن الوسيط
التدريب: مغلقة 24 / رابحة 13 / صافي افتراضي 1.20
الدخول المنسوب: 4 / تعبئة مؤكدة 3".into());
    ui.set_brain_monitor_overview("القناص | إشارات 30 | دخول منسوب 4 / تعبئة مؤكدة 3
وايكوف | إشارات 10 | تدريب مغلق 7 / رابح 4".into());
    // Exercise ALL 22 pages under each institutional palette and both viewport
    // sizes. Keep the original dark-mode full screenshots and save comparison
    // screenshots for seven operationally sensitive pages in Light/Smoke.
    for (theme,name) in [(0,"dark"),(1,"white"),(2,"smoke")] {
        ui.set_theme_choice(theme);
        for (width, height, suffix) in [(1120, 820, ""), (880, 700, "-small")] {
            window.set_size(slint::PhysicalSize::new(width, height));
            for page in 0..22 {
            ui.set_page(page);
            if [6,14,17,19,20].contains(&page) {ui.set_nav_section(1);}
            if page==21 {ui.set_nav_section(2);}
            ui.set_events(
                format!(
                    "#1 | core_started\nversion: {} • execution_enabled: false\n\nUI render check — not live trading data",
                    env!("CARGO_PKG_VERSION")
                )
                .into(),
            );
            slint::platform::update_timers_and_animations();
            ui.window().request_redraw();
            let mut pixels = vec![Rgb8Pixel::default(); (width * height) as usize];
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, width as usize);
            });
            // Render every combination; retain screenshots for comprehensive
            // dark regression and representative palette accessibility checks.
            if theme == 0 || [0,6,14,17,18,19,21].contains(&page) {
                let path = output.join(if theme == 0 {format!("dashboard-page-{page}{suffix}.ppm")} else {format!("dashboard-page-{page}{suffix}-{name}.ppm")});
                let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
                write!(file, "P6\n{width} {height}\n255\n")?;
                for pixel in pixels {
                    file.write_all(&[pixel.r, pixel.g, pixel.b])?;
                }
                file.flush()?;
            }
            }
        }
    }
    // Fidelity previews at the native 1540x920 target window size, rather than
    // forcing the user to judge the institutional layout from tiny 880px shots.
    // These screenshots are synthetic fixtures (see overlay), never live MT5.
    let reference_dir = std::path::Path::new("test-output").join("reference-preview");
    std::fs::create_dir_all(&reference_dir)?;
    for (theme, name) in [(0,"dark"),(1,"white"),(2,"smoke")] {
        ui.set_theme_choice(theme);
        ui.set_page(0);
        window.set_size(slint::PhysicalSize::new(1540, 920));
        slint::platform::update_timers_and_animations();
        ui.window().request_redraw();
        let mut pixels=vec![Rgb8Pixel::default();1540*920];
        window.draw_if_needed(|renderer| {renderer.render(&mut pixels,1540);});
        let file=reference_dir.join(format!("reference-command-center-{name}.ppm"));
        let mut writer=std::io::BufWriter::new(std::fs::File::create(file)?);
        write!(writer,"P6\n1540 920\n255\n")?;
        for pixel in pixels {writer.write_all(&[pixel.r,pixel.g,pixel.b])?;}
        writer.flush()?;
    }
    // More native real pixels for laptop and desktop breakpoints; no image
    // resizing, CSS mockups, or sample values from a real broker are involved.
    // All viewports use the same interactive Dashboard component and theme.
    let responsive_dir=std::path::Path::new("test-output").join("responsive-preview");
    std::fs::create_dir_all(&responsive_dir)?;
    for (width,height) in [(1024,768),(1280,800),(1366,768),(1600,900),(1920,1080)] {
        window.set_size(slint::PhysicalSize::new(width,height));
        for (theme,name) in [(0,"dark"),(1,"white"),(2,"smoke")] {
            ui.set_theme_choice(theme);
            ui.set_page(0);
            slint::platform::update_timers_and_animations();
            ui.window().request_redraw();
            let mut pixels=vec![Rgb8Pixel::default();(width*height) as usize];
            window.draw_if_needed(|renderer|{
                renderer.render(&mut pixels,width as usize);
            });
            let file=responsive_dir.join(format!(
                "dashboard-{width}x{height}-{name}.ppm"));
            let mut writer=std::io::BufWriter::new(std::fs::File::create(file)?);
            write!(writer,"P6\n{width} {height}\n255\n")?;
            for pixel in pixels {writer.write_all(&[pixel.r,pixel.g,pixel.b])?;}
            writer.flush()?;
        }
    }
    ui.hide()?;
    Ok(())
}
