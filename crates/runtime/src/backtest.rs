use crate::{atomic_write, now_ms};
use anyhow::{bail, Context, Result};
use braintrade_domain::{
    backtest::{self, Contract, History, HistoryBar, Options, Report, Strategy, DAY, MAX_BARS},
    Config, Status,
};
use chrono::{Duration, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    #[serde(default)]
    strategy: Strategy,
    from_date: String,
    to_date: String,
    initial_equity: f64,
    spread_floor: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Experiment {
    id: String,
    strategy: Strategy,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    id: String,
    from: u64,
    to: u64,
    login: u64,
    server: String,
    symbol: String,
    currency: String,
    point: f64,
    #[serde(default)]
    bars: Vec<HistoryBar>,
    #[serde(default)]
    error: String,
}
struct Job {
    id: String,
    created_ms: u64,
    updated_ms: u64,
    phase: String,
    message: String,
    options: Options,
    history: Option<History>,
    cursor: u64,
    history_start: u64,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
}
struct Completion {
    id: String,
    result: std::result::Result<(Report, History), String>,
}
pub struct Service {
    job: Option<Job>,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
}

fn date(value: &str) -> Result<u64> {
    let d = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .context("أدخل التاريخ بصيغة YYYY-MM-DD، مثال 2026-10-01")?;
    if d.format("%Y-%m-%d").to_string() != value {
        bail!("صيغة التاريخ يجب أن تكون YYYY-MM-DD");
    }
    let seconds = d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    if seconds <= 7 * DAY as i64 {
        bail!("التاريخ خارج النطاق");
    }
    Ok(seconds as u64)
}
fn required(value: Option<f64>) -> f64 {
    value.unwrap_or(0.0)
}
pub fn utc(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(seconds as i64, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| seconds.to_string())
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch("CREATE TABLE IF NOT EXISTS backtest_runs(id TEXT PRIMARY KEY,created_ms INTEGER NOT NULL,report TEXT NOT NULL,history TEXT NOT NULL);")?;
        let (tx, rx) = mpsc::channel();
        Ok(Self { job: None, tx, rx })
    }
    pub fn loading(&self) -> bool {
        self.job.as_ref().is_some_and(|j| j.phase == "loading")
    }
    fn active(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|j| j.phase == "loading" || j.phase == "running")
    }
    pub fn cancel(&mut self) {
        if let Some(j) = self.job.as_mut() {
            if j.phase == "loading" || j.phase == "running" {
                j.cancel.store(true, Ordering::Relaxed);
                j.phase = "cancelled".into();
                j.message = "تم إلغاء الاختبار؛ النتائج السابقة محفوظة".into();
                j.history = None;
            }
        }
    }
    fn fail(&mut self, message: String) {
        if let Some(j) = self.job.as_mut() {
            j.phase = "error".into();
            j.message = message;
            j.history = None;
        }
    }
    fn start(
        &mut self,
        request: StartRequest,
        c: &Config,
        s: &Status,
        open_paper: bool,
    ) -> Result<Value> {
        if self.active() {
            bail!("يوجد اختبار قيد العمل؛ انتظر أو ألغِه أولاً");
        }
        if s.requested_run {
            bail!("اضغط إيقاف النظام قبل جلب التاريخ؛ التقارير الحالية تبقى متاحة");
        }
        if open_paper {
            bail!("انتظر إغلاق الصفقة الافتراضية الحالية قبل جلب التاريخ");
        }
        if !s.gateway_fresh {
            bail!("يحتاج جلب التاريخ اتصال MT5؛ يمكنك مراجعة النتائج المحفوظة دون اتصال");
        }
        let t = s.telemetry.as_ref().context("لا توجد هوية حساب حالية")?;
        if !t.connected || t.login == 0 || t.symbol != c.symbol {
            bail!("انتظر اكتمال هوية حساب MT5 والرمز");
        }
        let options = Options {
            strategy: request.strategy,
            from: date(&request.from_date)?,
            to: date(&request.to_date)?,
            initial_equity: request.initial_equity,
            spread_floor: request.spread_floor,
            risk: c.risk.clone(),
        };
        options.validate().map_err(anyhow::Error::msg)?;
        if options.to > now_ms() / 1000 {
            bail!("لا يمكن اختبار تاريخ في المستقبل؛ تاريخ النهاية غير مشمول وبوقت UTC");
        }
        let contract = Contract {
            point: required(t.point),
            tick_size: required(t.tick_size),
            tick_loss: required(t.tick_value_loss),
            tick_profit: required(t.tick_value_profit),
            volume_min: required(t.volume_min),
            volume_step: required(t.volume_step),
            volume_max: required(t.volume_max),
        };
        contract.validate().map_err(anyhow::Error::msg)?;
        let now = now_ms();
        let start = options.from - 7 * DAY;
        self.job = Some(Job {
            id: uuid::Uuid::new_v4().to_string(),
            created_ms: now,
            updated_ms: now,
            phase: "loading".into(),
            message: "جلب شموع M1 من MT5 مع إحماء سابق للفترة".into(),
            history: Some(History {
                symbol: t.symbol.clone(),
                server: t.server.clone(),
                login: t.login,
                currency: t.currency.clone(),
                captured_ms: now,
                contract,
                bars: vec![],
            }),
            options,
            cursor: start,
            history_start: start,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicUsize::new(0)),
        });
        Ok(json!({"ok":true,"id":self.job.as_ref().unwrap().id}))
    }
    fn request(&self) -> Value {
        let Some(j) = self.job.as_ref().filter(|j| j.phase == "loading") else {
            return json!({"job":null});
        };
        let h = j.history.as_ref().unwrap();
        json!({"job":{"id":j.id,"from":j.cursor,"to":(j.cursor+DAY).min(j.options.to),"login":h.login,"server":h.server,"symbol":h.symbol,"currency":h.currency,"point":h.contract.point}})
    }
    fn chunk(&mut self, chunk: Chunk) -> Result<Value> {
        if self
            .job
            .as_ref()
            .is_none_or(|j| j.id != chunk.id || j.phase != "loading")
        {
            bail!("دفعة قديمة أو اختبار غير نشط؛ تم تجاهل البيانات");
        }
        let result = self.accept_chunk(chunk);
        if let Err(e) = &result {
            self.fail(e.to_string());
        }
        result
    }
    fn accept_chunk(&mut self, chunk: Chunk) -> Result<Value> {
        let j = self
            .job
            .as_mut()
            .filter(|j| j.phase == "loading")
            .context("لا يوجد طلب تاريخ نشط")?;
        // A late chunk from an old/cancelled job must never enter a new run.
        if chunk.id != j.id {
            bail!("معرّف دفعة التاريخ لا يطابق الاختبار النشط");
        }
        let h = j.history.as_mut().unwrap();
        if chunk.from != j.cursor || chunk.to != (j.cursor + DAY).min(j.options.to) {
            bail!("ترتيب دفعات التاريخ غير صحيح");
        }
        if (chunk.login, &chunk.server, &chunk.symbol, &chunk.currency)
            != (h.login, &h.server, &h.symbol, &h.currency)
            || !chunk.point.is_finite()
            || (chunk.point - h.contract.point).abs() > h.contract.point * 1e-8
        {
            bail!("تغيّر الحساب أو مواصفات الرمز أثناء جلب التاريخ");
        }
        if !chunk.error.is_empty() {
            bail!(
                "فشل جلب التاريخ: {}",
                chunk.error.chars().take(300).collect::<String>()
            );
        }
        if chunk.bars.len() > 1440
            || h.bars.len() + chunk.bars.len() > MAX_BARS
            || chunk.bars.iter().any(|b| {
                !b.valid(h.captured_ms)
                    || b.candle.time < chunk.from
                    || b.candle.time + 60 > chunk.to
            })
            || chunk
                .bars
                .windows(2)
                .any(|w| w[0].candle.time >= w[1].candle.time)
            || h.bars
                .last()
                .zip(chunk.bars.first())
                .is_some_and(|(a, b)| a.candle.time >= b.candle.time)
        {
            bail!("رفض تاريخ غير مكتمل أو مكرر أو خارج الفترة المطلوبة");
        }
        h.bars.extend(chunk.bars);
        j.cursor = chunk.to;
        j.updated_ms = now_ms();
        j.progress.store(
            ((j.cursor - j.history_start) * 100 / (j.options.to - j.history_start)) as usize,
            Ordering::Relaxed,
        );
        if j.cursor >= j.options.to {
            self.launch()?;
        }
        Ok(json!({"ok":true}))
    }
    fn launch(&mut self) -> Result<()> {
        let j = self.job.as_mut().context("No backtest job")?;
        let history = j.history.take().context("No historical data")?;
        history.validate().map_err(anyhow::Error::msg)?;
        j.phase = "running".into();
        j.message = "حساب الاختبار التاريخي؛ المحرك والواجهة يبقيان متاحين".into();
        j.progress.store(0, Ordering::Relaxed);
        let id = j.id.clone();
        let options = j.options.clone();
        let cancel = j.cancel.clone();
        let progress = j.progress.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(|| backtest::run(&history, &options, &cancel, &progress))
                    .unwrap_or_else(|_| Err("فشل عامل الاختبار؛ لم تنشر نتيجة ناقصة".into()))
                    .map(|r| (r, history));
            let _ = tx.send(Completion { id, result });
        });
        Ok(())
    }
    pub fn tick(&mut self, db: &Connection, now: u64) {
        if self
            .job
            .as_ref()
            .is_some_and(|j| j.phase == "loading" && now.saturating_sub(j.updated_ms) > 90_000)
        {
            self.fail(
                "انتهت مهلة جلب التاريخ من MT5. تحقق من الاتصال وتوفر التاريخ ثم أعد الطلب".into(),
            );
        }
        while let Ok(done) = self.rx.try_recv() {
            let Some(j) = self
                .job
                .as_mut()
                .filter(|j| j.id == done.id && j.phase == "running")
            else {
                continue;
            };
            match done.result {
                Err(e) => {
                    j.phase = "error".into();
                    j.message = e;
                }
                Ok((report, history)) => {
                    let saved = (|| -> Result<()> {
                        db.execute("INSERT INTO backtest_runs(id,created_ms,report,history) VALUES(?1,?2,?3,?4)",params![j.id,j.created_ms,serde_json::to_string(&report)?,serde_json::to_string(&history)?])?;
                        Ok(())
                    })();
                    match saved {
                        Ok(()) => {
                            j.phase = "completed".into();
                            j.message =
                                "اكتمل الاختبار وحُفظت النتيجة والبيانات؛ يمكن مراجعته دون MT5"
                                    .into();
                        }
                        Err(e) => {
                            j.phase = "error".into();
                            j.message = format!("تعذر حفظ النتيجة: {e}");
                        }
                    }
                }
            }
        }
    }
    fn state(&self, db: &Connection) -> Result<Value> {
        let latest: Option<String> = db
            .query_row(
                "SELECT id FROM backtest_runs ORDER BY created_ms DESC,id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(match &self.job {
            Some(j) => {
                json!({"id":j.id,"state":j.phase,"progress":j.progress.load(Ordering::Relaxed),"message":j.message,"latest_id":latest})
            }
            None => {
                json!({"state":"idle","progress":0,"message":"اختر فترة UTC وجلب التاريخ، أو افتح نتيجة محفوظة","latest_id":latest})
            }
        })
    }
    fn load(db: &Connection, id: &str) -> Result<(Report, History)> {
        let (r, h): (String, String) = db
            .query_row(
                "SELECT report,history FROM backtest_runs WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .context("النتيجة غير موجودة")?;
        Ok((serde_json::from_str(&r)?, serde_json::from_str(&h)?))
    }
    fn report(db: &Connection, id: &str) -> Result<Value> {
        let (r, _) = Self::load(db, id)?;
        Ok(
            json!({"report_id":id,"text":format!("{} → {}\n{}",utc(r.options.from),utc(r.options.to),r.text()),"summary":r.summary(),"metrics":r.metrics}),
        )
    }
    fn runs(db: &Connection) -> Result<Value> {
        let mut stmt=db.prepare("SELECT id,created_ms,json_extract(report,'$.symbol'),json_extract(report,'$.options.from'),json_extract(report,'$.options.to'),json_extract(report,'$.metrics.net'),json_extract(report,'$.algorithm') FROM backtest_runs ORDER BY created_ms DESC,id DESC LIMIT 50")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, u64>(3)?,
                    r.get::<_, u64>(4)?,
                    r.get::<_, f64>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(
            json!({"runs":rows.into_iter().map(|(id,created,symbol,from,to,net,algorithm)|json!({"id":id,"created_ms":created,"algorithm":algorithm,"label":format!("{} | {} | {} → {} | net {:.2} | {}",algorithm.split(" / ").next().unwrap_or("?"),symbol,utc(from),utc(to),net,&id[..8])})).collect::<Vec<_>>()}),
        )
    }
    fn replay(&mut self, db: &Connection, id: &str, strategy: Option<Strategy>) -> Result<Value> {
        if self.active() {
            bail!("انتظر الاختبار الحالي أو ألغِه أولاً");
        }
        let (mut report, history) = Self::load(db, id)?;
        if report.algorithm != report.options.strategy.algorithm() {
            bail!("إصدار خوارزمية النتيجة لا يطابق الإصدار الحالي");
        }
        let now = now_ms();
        self.job = Some(Job {
            id: uuid::Uuid::new_v4().to_string(),
            created_ms: now,
            updated_ms: now,
            phase: "running".into(),
            message: String::new(),
            options: {
                if let Some(strategy) = strategy {
                    report.options.strategy = strategy;
                }
                report.options
            },
            history: Some(history),
            cursor: 0,
            history_start: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicUsize::new(0)),
        });
        if let Err(e) = self.launch() {
            self.fail(e.to_string());
            return Err(e);
        }
        Ok(json!({"ok":true}))
    }
    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        db: &Connection,
        dir: &Path,
        config: &Config,
        status: &Status,
        open_paper: bool,
    ) -> Result<Value> {
        match (method, path) {
            ("GET", "/v1/backtest") => self.state(db),
            ("GET", "/v1/backtest/defaults") => {
                let today = Utc::now().date_naive();
                Ok(
                    json!({"from_date":(today-Duration::days(7)).to_string(),"to_date":today.to_string()}),
                )
            }
            ("POST", "/v1/backtest/start") => {
                self.start(serde_json::from_slice(body)?, config, status, open_paper)
            }
            ("POST", "/v1/backtest/cancel") => {
                self.cancel();
                Ok(json!({"ok":true}))
            }
            ("GET", "/v1/backtest/runs") => Self::runs(db),
            ("POST", "/v1/backtest/report") => {
                Self::report(db, &serde_json::from_slice::<Selection>(body)?.id)
            }
            ("POST", "/v1/backtest/replay") => {
                self.replay(db, &serde_json::from_slice::<Selection>(body)?.id, None)
            }
            ("POST", "/v1/backtest/experiment") => {
                let r: Experiment = serde_json::from_slice(body)?;
                self.replay(db, &r.id, Some(r.strategy))
            }
            ("POST", "/v1/backtest/export") => {
                export(db, dir, &serde_json::from_slice::<Selection>(body)?.id)
            }
            ("GET", "/v1/gateway/history/request") => Ok(self.request()),
            ("POST", "/v1/gateway/history/chunk") => self.chunk(serde_json::from_slice(body)?),
            _ => bail!("Unknown backtest route"),
        }
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn html(r: &Report) -> String {
    use std::fmt::Write;
    let values = &r.equity;
    let min = values
        .iter()
        .map(|p| p.equity)
        .fold(f64::INFINITY, f64::min);
    let max = values
        .iter()
        .map(|p| p.equity)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).max(1.0);
    let start = values.first().map(|p| p.time).unwrap_or(0);
    let duration = values
        .last()
        .map(|p| p.time.saturating_sub(start).max(1))
        .unwrap_or(1);
    let mut points = String::new();
    for p in values
        .iter()
        .step_by((values.len() / 800).max(1))
        .chain(values.last())
    {
        let _ = write!(
            points,
            "{:.2},{:.2} ",
            20.0 + 760.0 * (p.time - start) as f64 / duration as f64,
            210.0 - 180.0 * (p.equity - min) / span
        );
    }
    let mut rows = String::new();
    for t in r.trades.iter().rev().take(200) {
        let _=write!(rows,"<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.4}</td><td>{:.5}</td><td>{:.5}</td><td>{:.2}</td><td>{}{}</td></tr>",if t.side==1 {"BUY"}else{"SELL"},utc(t.entry_time),utc(t.exit_bar),t.lots,t.entry,t.exit,t.net,escape(&t.reason),if t.ambiguous{" / SL-first"}else{""});
    }
    format!(
        r##"<!doctype html><html lang="ar" dir="rtl"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>BrainTrade Backtest</title><style>body{{margin:0;background:#101827;color:#e5edf9;font:16px system-ui,sans-serif}}main{{max-width:1100px;margin:auto;padding:32px}}h1{{color:#76dfc0}}pre{{white-space:pre-wrap;line-height:1.8;background:#1b273a;padding:20px;border-radius:12px}}svg{{width:100%;background:#1b273a;border-radius:12px}}table{{width:100%;border-collapse:collapse;font-size:13px}}td,th{{padding:12px;border-bottom:1px solid #344055;text-align:right}}.scroll{{overflow:auto}}small{{color:#aabbd2}}@media print{{body{{background:white;color:black}}pre,svg{{background:#eee}}}}</style><main><h1>BRAINTRADE OS — الاختبار التاريخي</h1><p>{} → {}</p><pre>{}</pre><h2>منحنى حقوق الحساب الافتراضية</h2><svg viewBox="0 0 800 240" role="img" aria-label="Virtual equity curve"><polyline fill="none" stroke="#51cba8" stroke-width="2" points="{}"/></svg><small>أدنى {:.2} — أعلى {:.2} {}. الخط يصل العينات؛ تفاصيل كل دقيقة في equity.csv.</small><h2>افتراضات وحدود البيانات</h2><pre>{}</pre><p>أول شمعة متوفرة: {}، آخر شمعة: {}. أول/آخر دقيقة اختبار: {} / {}.</p><h2>آخر 200 صفقة — السجل الكامل في trades.csv</h2><div class="scroll"><table><thead><tr><th>الاتجاه</th><th>الدخول</th><th>دقيقة الخروج</th><th>اللوت</th><th>سعر الدخول</th><th>سعر الخروج</th><th>الصافي</th><th>السبب</th></tr></thead><tbody>{}</tbody></table></div><p>تجربة تاريخية تقديرية؛ لا أوامر إلى الوسيط ولا ادعاء ربحية مستقبلية.</p></main></html>"##,
        utc(r.options.from),
        utc(r.options.to),
        escape(&r.summary()),
        points,
        min,
        max,
        escape(&r.currency),
        escape(&r.warnings.join("\n")),
        utc(r.history_first),
        utc(r.history_last),
        utc(r.first_test_bar),
        utc(r.last_test_bar),
        rows
    )
}
fn export(db: &Connection, dir: &Path, id: &str) -> Result<Value> {
    use std::fmt::Write;
    let (r, h) = Service::load(db, id)?;
    let name = format!("backtest-{}-{}", now_ms(), uuid::Uuid::new_v4());
    let root = dir.join("exports");
    let staging = root.join(format!(".{name}.tmp"));
    fs::create_dir_all(&staging)?;
    atomic_write(
        &staging.join("report.json"),
        &serde_json::to_vec_pretty(&r)?,
    )?;
    atomic_write(&staging.join("history.json"), &serde_json::to_vec(&h)?)?;
    atomic_write(&staging.join("report.html"), html(&r).as_bytes())?;
    let mut trades="signal_bar_utc,entry_time_utc,exit_bar_utc,exit_phase,side,entry,exit,stop,target,lots,commission,net,reason,ambiguous\n".to_string();
    for t in &r.trades {
        writeln!(
            trades,
            "{},{},{},{},{},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{:.10},{},{}",
            t.signal_bar,
            t.entry_time,
            t.exit_bar,
            t.exit_phase,
            if t.side == 1 { "BUY" } else { "SELL" },
            t.entry,
            t.exit,
            t.stop,
            t.target,
            t.lots,
            t.commission,
            t.net,
            t.reason,
            t.ambiguous
        )?;
    }
    atomic_write(&staging.join("trades.csv"), trades.as_bytes())?;
    let mut curve = "time_utc,equity,balance\n".to_string();
    for p in &r.equity {
        writeln!(curve, "{},{:.10},{:.10}", p.time, p.equity, p.balance)?;
    }
    atomic_write(&staging.join("equity.csv"), curve.as_bytes())?;
    atomic_write(&staging.join("README.txt"),"افتح report.html للمراجعة. JSON يحفظ الافتراضات ومواصفات الرمز والشموع. CSV يحتوي السجل الكامل بأوقات UTC epoch. الأرقام محاكاة وليست تنفيذ وسيط. بيانات الحساب موجودة في التقرير؛ التوكن وكلمات المرور غير موجودة.\n".as_bytes())?;
    let dest = root.join(name);
    fs::rename(staging, &dest)?;
    Ok(
        json!({"path":dest,"note":"افتح report.html للتقرير والمنحنى؛ CSV/JSON يحفظان تفاصيل التجربة كاملة"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_dates_and_html_escaping() {
        assert_eq!(date("2024-10-04").unwrap() % DAY, 0);
        assert!(date("2024-02-30").is_err());
        assert!(date("2024-1-01").is_err());
        assert_eq!(escape("<script>&\""), "&lt;script&gt;&amp;&quot;");
    }
    #[test]
    fn cancelled_or_timed_out_jobs_never_accept_late_data() {
        let db = Connection::open_in_memory().unwrap();
        let mut service = Service::new(&db).unwrap();
        service.job = Some(Job {
            id: "a".into(),
            created_ms: 1,
            updated_ms: 1,
            phase: "loading".into(),
            message: String::new(),
            options: Options {
                strategy: Strategy::MetaV1,
                from: DAY * 10,
                to: DAY * 11,
                initial_equity: 1000.0,
                spread_floor: 0.2,
                risk: Default::default(),
            },
            history: None,
            cursor: 0,
            history_start: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicUsize::new(0)),
        });
        service.tick(&db, 100_000);
        assert!(!service.loading());
        assert_eq!(service.state(&db).unwrap()["state"], "error");
        service.job.as_mut().unwrap().phase = "running".into();
        service.cancel();
        assert_eq!(service.state(&db).unwrap()["state"], "cancelled");
        service
            .tx
            .send(Completion {
                id: "a".into(),
                result: Err("late".into()),
            })
            .unwrap();
        service.tick(&db, 100_001);
        assert_eq!(service.state(&db).unwrap()["state"], "cancelled");
    }
}
