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
    ui.set_feedback("Research 0.3.0 • RENDER FIXTURE / no live data".into());
    ui.set_regime("UPTREND".into());
    ui.set_decision("WAIT".into());
    ui.set_brains("RENDER FIXTURE / بيانات فحص فقط\nTrend M1: BUY | evidence 0.72\nTrend M5: BUY | evidence 0.61\nTrend M15: BUY | evidence 0.58\nMomentum M1: BUY\nStructure M1: NEUTRAL\n\nالنظام متوقف؛ لا مداخل افتراضية جديدة\nقوة الدليل ليست نسبة نجاح".into());
    ui.set_paper("SHADOW / محاكاة فقط\nRENDER FIXTURE / no live data\nVirtual equity: 1000.00 | realized: 0.00\nClosed virtual trades: 0\n\nNo open virtual position\n\nSpread, configured commission and slippage included.\nNo MT5 orders.".into());
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
    ui.show()?;
    for (width, height, suffix) in [(1120, 820, ""), (880, 700, "-small")] {
        window.set_size(slint::PhysicalSize::new(width, height));
        for page in 0..9 {
            ui.set_page(page);
            ui.set_events("#1 | core_started\nversion: 0.3.0 • execution_enabled: false\n\nUI render check — not live trading data".into());
            slint::platform::update_timers_and_animations();
            ui.window().request_redraw();
            let mut pixels = vec![Rgb8Pixel::default(); (width * height) as usize];
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, width as usize);
            });
            let path = format!("dashboard-page-{page}{suffix}.ppm");
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
