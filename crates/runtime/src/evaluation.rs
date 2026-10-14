//! Persisted chronological evaluation evidence over immutable saved backtest history.
//! Evaluation is research-only: no model promotion, inference, or broker execution.
use anyhow::{bail, Context, Result};
use braintrade_domain::{
    backtest::{History, Options, Report, Strategy},
    evaluation::{evaluate, ChronologicalEvaluation, EvaluationPlan},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
};

const VERSION: &str = "chronological-evidence-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SourceIdentity {
    schema: String,
    source_run_id: String,
    source_created_ms: u64,
    algorithm: String,
    strategy: Strategy,
    options: Options,
    symbol: String,
    server: String,
    login: u64,
    currency: String,
    history_captured_ms: u64,
    history_first: u64,
    history_last: u64,
    history_bars: usize,
    contract: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    source_id: String,
    plan: EvaluationPlan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
}

struct Job {
    id: String,
    source_id: String,
    created_ms: u64,
    phase: String,
    message: String,
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

struct Completion {
    id: String,
    result: std::result::Result<(ChronologicalEvaluation, SourceIdentity), String>,
}

pub struct Service {
    job: Option<Job>,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
}

fn source(db: &Connection, id: &str) -> Result<(History, Options, SourceIdentity)> {
    let (created_ms, report_json, history_json): (u64, String, String) = db
        .query_row(
            "SELECT created_ms,report,history FROM backtest_runs WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("الاختبار المصدر غير موجود")?;
    let report: Report = serde_json::from_str(&report_json)?;
    let history: History = serde_json::from_str(&history_json)?;
    history.validate().map_err(anyhow::Error::msg)?;
    report.options.validate().map_err(anyhow::Error::msg)?;
    if !matches!(report.options.strategy, Strategy::MetaV1 | Strategy::MetaV2) {
        bail!("التقييم الزمني v1 يقبل نتيجة Meta v1 أو Meta v2 محفوظة فقط");
    }
    if report.algorithm != report.options.strategy.algorithm() {
        bail!("إصدار خوارزمية الاختبار المصدر لا يطابق المحرك الحالي");
    }
    let contract_report = serde_json::to_value(&report.contract)?;
    let contract_history = serde_json::to_value(&history.contract)?;
    if report.symbol != history.symbol
        || report.server != history.server
        || report.login != history.login
        || report.currency != history.currency
        || report.captured_ms != history.captured_ms
        || contract_report != contract_history
    {
        bail!("هوية تقرير الاختبار لا تطابق بيانات History المحفوظة");
    }
    let first = history
        .bars
        .first()
        .context("History المصدر فارغ")?
        .candle
        .time;
    let last = history
        .bars
        .last()
        .context("History المصدر فارغ")?
        .candle
        .time;
    let identity = SourceIdentity {
        schema: "backtest-source-identity-v1".into(),
        source_run_id: id.into(),
        source_created_ms: created_ms,
        algorithm: report.algorithm.clone(),
        strategy: report.options.strategy,
        options: report.options.clone(),
        symbol: history.symbol.clone(),
        server: history.server.clone(),
        login: history.login,
        currency: history.currency.clone(),
        history_captured_ms: history.captured_ms,
        history_first: first,
        history_last: last,
        history_bars: history.bars.len(),
        contract: contract_history,
    };
    Ok((history, report.options, identity))
}

fn summary(e: &ChronologicalEvaluation, source_id: &str) -> String {
    format!(
        "CHRONOLOGICAL EVALUATION / تقييم زمني\nSource run: {source_id}\nPlan: {:?}\n{} / {} / {}\nWindows: {}\nMeta v1: {} trades | net-sum {:.2} | avg return {:.2}% | worst DD {:.2}%\nMeta v2: {} trades | net-sum {:.2} | avg return {:.2}% | worst DD {:.2}%\nAverage delta v2-v1: net {:.2} | return {:.2}%\n{}\n\nكل نافذة مستقلة؛ هذه ليست نتيجة رأس مال متصل ولا إثبات ربحية.",
        e.plan,
        e.server,
        e.login,
        e.symbol,
        e.windows.len(),
        e.meta_v1.trades,
        e.meta_v1.net_sum,
        e.meta_v1.average_return_percent,
        e.meta_v1.worst_drawdown_percent,
        e.meta_v2.trades,
        e.meta_v2.net_sum,
        e.meta_v2.average_return_percent,
        e.meta_v2.worst_drawdown_percent,
        e.average_delta_net_v2_minus_v1,
        e.average_delta_return_percent_v2_minus_v1,
        e.note
    )
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS chronological_evaluations(
                id TEXT PRIMARY KEY,
                source_run_id TEXT NOT NULL,
                created_ms INTEGER NOT NULL,
                plan TEXT NOT NULL,
                source_identity TEXT NOT NULL,
                evaluation TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS chronological_evaluations_source_idx
                ON chronological_evaluations(source_run_id,created_ms);",
        )?;
        let (tx, rx) = mpsc::channel();
        Ok(Self { job: None, tx, rx })
    }

    fn active(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| job.phase == "running")
    }

    pub fn cancel(&mut self) {
        if let Some(job) = self.job.as_mut().filter(|job| job.phase == "running") {
            job.cancel.store(true, Ordering::Relaxed);
            job.phase = "cancelled".into();
            job.message = "تم إلغاء التقييم؛ الأدلة المحفوظة سابقاً لم تتغير".into();
        }
    }

    fn start(&mut self, db: &Connection, request: StartRequest, now: u64) -> Result<Value> {
        if self.active() {
            bail!("يوجد تقييم زمني قيد العمل؛ انتظر أو ألغِه أولاً");
        }
        if request.source_id.trim().is_empty() || request.source_id.len() > 100 {
            bail!("معرّف الاختبار المصدر غير صالح");
        }
        let (history, options, identity) = source(db, &request.source_id)?;
        let id = uuid::Uuid::new_v4().to_string();
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.job = Some(Job {
            id: id.clone(),
            source_id: request.source_id.clone(),
            created_ms: now,
            phase: "running".into(),
            message: "تشغيل Meta v1 وMeta v2 على نوافذ زمنية منفصلة من نفس History المحفوظ".into(),
            progress: progress.clone(),
            cancel: cancel.clone(),
        });
        let tx = self.tx.clone();
        let plan = request.plan;
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| {
                evaluate(&history, &options, &plan, &cancel, &progress)
            })
            .unwrap_or_else(|_| Err("فشل عامل التقييم؛ لم تُحفظ نتيجة ناقصة".into()))
            .map(|evaluation| (evaluation, identity));
            let _ = tx.send(Completion { id, result });
        });
        Ok(json!({
            "ok": true,
            "id": self.job.as_ref().unwrap().id,
            "source_id": request.source_id,
            "version": VERSION,
            "prediction_enabled": false,
            "promotion_enabled": false
        }))
    }

    pub fn tick(&mut self, db: &Connection) {
        while let Ok(done) = self.rx.try_recv() {
            let Some(job) = self
                .job
                .as_mut()
                .filter(|job| job.id == done.id && job.phase == "running")
            else {
                continue;
            };
            match done.result {
                Err(error) => {
                    job.phase = "error".into();
                    job.message = error;
                }
                Ok((evaluation, identity)) => {
                    let saved = (|| -> Result<()> {
                        if identity.source_run_id != job.source_id {
                            bail!("هوية المصدر تغيرت أثناء التقييم");
                        }
                        let tx = db.unchecked_transaction()?;
                        tx.execute(
                            "INSERT INTO chronological_evaluations(id,source_run_id,created_ms,plan,source_identity,evaluation)
                             VALUES(?1,?2,?3,?4,?5,?6)",
                            params![
                                job.id,
                                job.source_id,
                                job.created_ms,
                                serde_json::to_string(&evaluation.plan)?,
                                serde_json::to_string(&identity)?,
                                serde_json::to_string(&evaluation)?
                            ],
                        )?;
                        tx.commit()?;
                        Ok(())
                    })();
                    match saved {
                        Ok(()) => {
                            job.phase = "completed".into();
                            job.message = "اكتمل التقييم وحُفظ كدليل immutable مرتبط بالاختبار المصدر".into();
                            job.progress.store(100, Ordering::Relaxed);
                        }
                        Err(error) => {
                            job.phase = "error".into();
                            job.message = format!("تعذر حفظ دليل التقييم: {error}");
                        }
                    }
                }
            }
        }
    }

    fn state(&self, db: &Connection) -> Result<Value> {
        let latest: Option<String> = db
            .query_row(
                "SELECT id FROM chronological_evaluations ORDER BY created_ms DESC,id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(match &self.job {
            Some(job) => json!({
                "version": VERSION,
                "id": job.id,
                "source_id": job.source_id,
                "state": job.phase,
                "progress": job.progress.load(Ordering::Relaxed),
                "message": job.message,
                "latest_id": latest,
                "prediction_enabled": false,
                "promotion_enabled": false
            }),
            None => json!({
                "version": VERSION,
                "state": "idle",
                "progress": 0,
                "message": "اختر backtest محفوظاً وخطة holdout/rolling",
                "latest_id": latest,
                "prediction_enabled": false,
                "promotion_enabled": false
            }),
        })
    }

    fn load(db: &Connection, id: &str) -> Result<(u64, String, SourceIdentity, ChronologicalEvaluation)> {
        let (created_ms, source_id, identity_json, evaluation_json): (u64, String, String, String) = db
            .query_row(
                "SELECT created_ms,source_run_id,source_identity,evaluation FROM chronological_evaluations WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .context("دليل التقييم غير موجود")?;
        let identity: SourceIdentity = serde_json::from_str(&identity_json)?;
        let evaluation: ChronologicalEvaluation = serde_json::from_str(&evaluation_json)?;
        if identity.source_run_id != source_id {
            bail!("دليل التقييم المخزن لا يطابق معرّف المصدر");
        }
        Ok((created_ms, source_id, identity, evaluation))
    }

    fn report(db: &Connection, id: &str) -> Result<Value> {
        let (created_ms, source_id, identity, evaluation) = Self::load(db, id)?;
        Ok(json!({
            "id": id,
            "created_ms": created_ms,
            "source_id": source_id,
            "source_identity": identity,
            "evaluation": evaluation,
            "summary": summary(&evaluation, &source_id),
            "immutable": true,
            "prediction_enabled": false,
            "promotion_enabled": false
        }))
    }

    fn runs(db: &Connection) -> Result<Value> {
        let mut statement = db.prepare(
            "SELECT id,source_run_id,created_ms,evaluation FROM chronological_evaluations
             ORDER BY created_ms DESC,id DESC LIMIT 50",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let runs = rows
            .into_iter()
            .filter_map(|(id, source_id, created_ms, raw)| {
                let evaluation: ChronologicalEvaluation = serde_json::from_str(&raw).ok()?;
                Some(json!({
                    "id": id,
                    "source_id": source_id,
                    "created_ms": created_ms,
                    "plan": evaluation.plan,
                    "symbol": evaluation.symbol,
                    "server": evaluation.server,
                    "login": evaluation.login,
                    "windows": evaluation.windows.len(),
                    "meta_v1_net_sum": evaluation.meta_v1.net_sum,
                    "meta_v2_net_sum": evaluation.meta_v2.net_sum,
                    "average_delta_net_v2_minus_v1": evaluation.average_delta_net_v2_minus_v1
                }))
            })
            .collect::<Vec<_>>();
        Ok(json!({"version": VERSION, "runs": runs}))
    }

    fn export(db: &Connection, dir: &Path, id: &str) -> Result<Value> {
        let (created_ms, source_id, identity, evaluation) = Self::load(db, id)?;
        let safe = id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(80)
            .collect::<String>();
        if safe.is_empty() {
            bail!("معرّف التقييم غير صالح للتصدير");
        }
        let name = format!("chronological-evaluation-{safe}");
        let staging = dir.join("exports").join(format!(".{name}.tmp"));
        let destination = dir.join("exports").join(&name);
        if staging.exists() || destination.exists() {
            bail!("مجلد تصدير التقييم موجود مسبقاً");
        }
        fs::create_dir_all(&staging)?;
        fs::write(
            staging.join("evaluation.json"),
            serde_json::to_vec_pretty(&evaluation)?,
        )?;
        fs::write(
            staging.join("source_identity.json"),
            serde_json::to_vec_pretty(&identity)?,
        )?;
        fs::write(
            staging.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": VERSION,
                "evaluation_id": id,
                "source_run_id": source_id,
                "created_ms": created_ms,
                "immutable": true,
                "prediction_enabled": false,
                "promotion_enabled": false,
                "note": "Chronological research evidence. Independent windows; not a connected equity curve or profitability claim."
            }))?,
        )?;
        fs::rename(&staging, &destination)?;
        Ok(json!({
            "path": destination,
            "id": id,
            "source_id": source_id,
            "note": "Saved immutable chronological evidence and exact source identity."
        }))
    }

    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        db: &Connection,
        dir: &Path,
    ) -> Result<Value> {
        match (method, path) {
            ("GET", "/v1/evaluation") => self.state(db),
            ("POST", "/v1/evaluation/start") => {
                self.start(db, serde_json::from_slice(body)?, crate::now_ms())
            }
            ("POST", "/v1/evaluation/cancel") => {
                self.cancel();
                Ok(json!({"ok": true}))
            }
            ("GET", "/v1/evaluation/runs") => Self::runs(db),
            ("POST", "/v1/evaluation/report") => {
                Self::report(db, &serde_json::from_slice::<Selection>(body)?.id)
            }
            ("POST", "/v1/evaluation/export") => {
                Self::export(db, dir, &serde_json::from_slice::<Selection>(body)?.id)
            }
            _ => bail!("Unknown evaluation route"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use braintrade_domain::{
        backtest::{Contract, HistoryBar},
        Candle, RiskConfig,
    };

    const DAY: u64 = 86_400;
    const BASE: u64 = 1_728_000_000;

    fn history(days: usize) -> History {
        let count = days * DAY as usize / 60;
        History {
            symbol: "TEST".into(),
            server: "FIXTURE".into(),
            login: 77,
            currency: "USD".into(),
            captured_ms: (BASE + count as u64 * 60 + 60) * 1000,
            contract: Contract {
                point: 0.01,
                tick_size: 0.01,
                tick_loss: 1.0,
                tick_profit: 1.0,
                volume_min: 0.01,
                volume_step: 0.01,
                volume_max: 10.0,
            },
            bars: (0..count)
                .map(|index| {
                    let p = 2000.0 + index as f64 * 0.002;
                    HistoryBar {
                        candle: Candle {
                            time: BASE + index as u64 * 60,
                            open: p,
                            high: p + 0.20,
                            low: p - 0.18,
                            close: p + 0.03,
                            tick_volume: 100,
                        },
                        spread_points: 2,
                    }
                })
                .collect(),
        }
    }

    fn seed(db: &Connection) -> String {
        db.execute_batch("CREATE TABLE backtest_runs(id TEXT PRIMARY KEY,created_ms INTEGER NOT NULL,report TEXT NOT NULL,history TEXT NOT NULL);").unwrap();
        let h = history(12);
        let options = Options {
            strategy: Strategy::MetaV1,
            from: BASE + 3 * DAY,
            to: BASE + 10 * DAY,
            initial_equity: 10_000.0,
            spread_floor: 0.02,
            risk: RiskConfig::default(),
        };
        let cancel = AtomicBool::new(false);
        let progress = AtomicUsize::new(0);
        let report = braintrade_domain::backtest::run(&h, &options, &cancel, &progress).unwrap();
        let id = "source-run-1".to_string();
        db.execute(
            "INSERT INTO backtest_runs(id,created_ms,report,history) VALUES(?1,?2,?3,?4)",
            params![id, 1234u64, serde_json::to_string(&report).unwrap(), serde_json::to_string(&h).unwrap()],
        ).unwrap();
        id
    }

    #[test]
    fn source_identity_is_exact_and_rejects_unknown_source() {
        let db = Connection::open_in_memory().unwrap();
        let id = seed(&db);
        let (_, options, identity) = source(&db, &id).unwrap();
        assert_eq!(identity.source_run_id, id);
        assert_eq!(identity.history_bars, history(12).bars.len());
        assert_eq!(options.from, BASE + 3 * DAY);
        assert!(source(&db, "missing").is_err());
    }

    #[test]
    fn evidence_persists_and_exports_with_source_binding() {
        let db = Connection::open_in_memory().unwrap();
        let source_id = seed(&db);
        let mut service = Service::new(&db).unwrap();
        let (history, options, identity) = source(&db, &source_id).unwrap();
        let plan = EvaluationPlan::Holdout { holdout_percent: 30 };
        let evaluation = evaluate(
            &history,
            &options,
            &plan,
            &AtomicBool::new(false),
            &AtomicUsize::new(0),
        ).unwrap();
        let id = "evaluation-1";
        db.execute(
            "INSERT INTO chronological_evaluations(id,source_run_id,created_ms,plan,source_identity,evaluation) VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, source_id, 2222u64, serde_json::to_string(&plan).unwrap(), serde_json::to_string(&identity).unwrap(), serde_json::to_string(&evaluation).unwrap()],
        ).unwrap();
        let report = Service::report(&db, id).unwrap();
        assert_eq!(report["source_id"], "source-run-1");
        assert_eq!(report["immutable"], true);
        assert_eq!(report["prediction_enabled"], false);
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(dir.join("exports")).unwrap();
        let exported = Service::export(&db, &dir, id).unwrap();
        let path = std::path::PathBuf::from(exported["path"].as_str().unwrap());
        assert!(path.join("evaluation.json").is_file());
        assert!(path.join("source_identity.json").is_file());
        assert!(path.join("manifest.json").is_file());
        fs::remove_dir_all(dir).unwrap();
        service.cancel();
    }
}
