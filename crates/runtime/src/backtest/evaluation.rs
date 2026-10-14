use super::{utc, Service as BacktestService};
use crate::{atomic_write, now_ms};
use anyhow::{bail, Context, Result};
use braintrade_domain::{
    backtest::{History, Options, Report, Strategy},
    evaluation::{self, ChronologicalEvaluation, EvaluationPlan, WindowRole},
};
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
pub struct StartRequest {
    pub source_id: String,
    pub plan: EvaluationPlan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub id: String,
}

struct Job {
    id: String,
    source_id: String,
    source_created_ms: u64,
    source_identity: Value,
    plan: EvaluationPlan,
    phase: String,
    message: String,
    created_ms: u64,
    progress: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

struct Completion {
    id: String,
    result: std::result::Result<ChronologicalEvaluation, String>,
}

pub struct Service {
    job: Option<Job>,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
}

fn source_identity(source_id: &str, created_ms: u64, report: &Report, history: &History) -> Value {
    json!({
        "source_id": source_id,
        "source_created_ms": created_ms,
        "algorithm": report.algorithm,
        "options": report.options,
        "history": {
            "symbol": history.symbol,
            "server": history.server,
            "login": history.login,
            "currency": history.currency,
            "captured_ms": history.captured_ms,
            "bars": history.bars.len(),
            "first_bar": history.bars.first().map(|b| b.candle.time),
            "last_bar": history.bars.last().map(|b| b.candle.time),
            "contract": history.contract
        }
    })
}

fn summary(value: &ChronologicalEvaluation) -> String {
    let mut out = format!(
        "Chronological evaluation {}\n{} / {} / {}\nPlan: {:?}\nWindows: {}\nMeta v1: {} windows | {} trades | net sum {:.2} | avg return {:.3}% | worst DD {:.3}%\nMeta v2: {} windows | {} trades | net sum {:.2} | avg return {:.3}% | worst DD {:.3}%\nAverage delta Meta v2-v1: net {:.2} | return {:.3}%\n",
        value.version,
        value.server,
        value.login,
        value.symbol,
        value.plan,
        value.windows.len(),
        value.meta_v1.windows,
        value.meta_v1.trades,
        value.meta_v1.net_sum,
        value.meta_v1.average_return_percent,
        value.meta_v1.worst_drawdown_percent,
        value.meta_v2.windows,
        value.meta_v2.trades,
        value.meta_v2.net_sum,
        value.meta_v2.average_return_percent,
        value.meta_v2.worst_drawdown_percent,
        value.average_delta_net_v2_minus_v1,
        value.average_delta_return_percent_v2_minus_v1,
    );
    for row in &value.windows {
        let role = match row.window.role {
            WindowRole::Development => "development",
            WindowRole::Holdout => "holdout",
        };
        out.push_str(&format!(
            "Cycle {} / {} | {} → {} | v1 net {:.2}, DD {:.2}% | v2 net {:.2}, DD {:.2}% | delta {:.2}\n",
            row.window.cycle,
            role,
            utc(row.window.from),
            utc(row.window.to),
            row.meta_v1.metrics.net,
            row.meta_v1.metrics.max_drawdown_percent,
            row.meta_v2.metrics.net,
            row.meta_v2.metrics.max_drawdown_percent,
            row.delta_net_v2_minus_v1,
        ));
    }
    out.push_str("\nIndependent-window research statistics only; not a chained equity curve and not profitability evidence.");
    out
}

impl Service {
    pub fn new(db: &Connection) -> Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS backtest_evaluations(
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                created_ms INTEGER NOT NULL,
                source_created_ms INTEGER NOT NULL,
                source_identity TEXT NOT NULL,
                plan TEXT NOT NULL,
                result TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_backtest_evaluations_source_created
                ON backtest_evaluations(source_id,created_ms);",
        )?;
        let (tx, rx) = mpsc::channel();
        Ok(Self { job: None, tx, rx })
    }

    pub fn active(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| job.phase == "running")
    }

    pub fn cancel(&mut self) {
        if let Some(job) = self.job.as_mut().filter(|job| job.phase == "running") {
            job.cancel.store(true, Ordering::Relaxed);
            job.phase = "cancelled".into();
            job.message = "تم إلغاء التقييم الزمني؛ النتائج المحفوظة السابقة لم تتغير".into();
        }
    }

    pub fn start(&mut self, db: &Connection, request: StartRequest, backtest_active: bool) -> Result<Value> {
        if self.active() || backtest_active {
            bail!("يوجد اختبار أو تقييم زمني قيد العمل؛ انتظر أو ألغِه أولاً");
        }
        let source_created_ms: u64 = db
            .query_row(
                "SELECT created_ms FROM backtest_runs WHERE id=?1",
                [&request.source_id],
                |row| row.get(0),
            )
            .optional()?
            .context("اختبار المصدر غير موجود")?;
        let (report, history) = BacktestService::load(db, &request.source_id)?;
        if report.algorithm != report.options.strategy.algorithm() {
            bail!("إصدار خوارزمية المصدر لا يطابق الإصدار الحالي");
        }
        if !matches!(report.options.strategy, Strategy::MetaV1 | Strategy::MetaV2) {
            bail!("اختر نتيجة Meta v1 أو Meta v2 محفوظة كمصدر للتقييم الزمني");
        }
        history.validate().map_err(anyhow::Error::msg)?;
        let mut options: Options = report.options.clone();
        options.strategy = Strategy::MetaV1;
        let identity = source_identity(&request.source_id, source_created_ms, &report, &history);
        let id = uuid::Uuid::new_v4().to_string();
        let created_ms = now_ms();
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_progress = progress.clone();
        let worker_cancel = cancel.clone();
        let worker_id = id.clone();
        let worker_plan = request.plan.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| {
                evaluation::evaluate(&history, &options, &worker_plan, &worker_cancel, &worker_progress)
            })
            .unwrap_or_else(|_| Err("فشل عامل التقييم الزمني؛ لم تُنشر نتيجة ناقصة".into()));
            let _ = tx.send(Completion { id: worker_id, result });
        });
        self.job = Some(Job {
            id: id.clone(),
            source_id: request.source_id,
            source_created_ms,
            source_identity: identity,
            plan: request.plan,
            phase: "running".into(),
            message: "تشغيل Meta v1 وMeta v2 على نوافذ زمنية مستقلة من نفس History المحفوظ".into(),
            created_ms,
            progress,
            cancel,
        });
        Ok(json!({"ok":true,"id":id,"state":"running"}))
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
                Ok(result) => {
                    let saved = (|| -> Result<()> {
                        let tx = db.unchecked_transaction()?;
                        tx.execute(
                            "INSERT INTO backtest_evaluations(id,source_id,created_ms,source_created_ms,source_identity,plan,result)
                             VALUES(?1,?2,?3,?4,?5,?6,?7)",
                            params![
                                job.id,
                                job.source_id,
                                job.created_ms,
                                job.source_created_ms,
                                job.source_identity.to_string(),
                                serde_json::to_string(&job.plan)?,
                                serde_json::to_string(&result)?,
                            ],
                        )?;
                        tx.commit()?;
                        Ok(())
                    })();
                    match saved {
                        Ok(()) => {
                            job.phase = "completed".into();
                            job.message = "اكتمل التقييم الزمني وحُفظت النوافذ وهوية المصدر بصورة مستقلة".into();
                            job.progress.store(100, Ordering::Relaxed);
                        }
                        Err(error) => {
                            job.phase = "error".into();
                            job.message = format!("تعذر حفظ التقييم الزمني: {error}");
                        }
                    }
                }
            }
        }
    }

    pub fn state(&self, db: &Connection) -> Result<Value> {
        let latest: Option<String> = db
            .query_row(
                "SELECT id FROM backtest_evaluations ORDER BY created_ms DESC,id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(match &self.job {
            Some(job) => json!({
                "id":job.id,
                "source_id":job.source_id,
                "state":job.phase,
                "progress":job.progress.load(Ordering::Relaxed),
                "message":job.message,
                "latest_id":latest
            }),
            None => json!({
                "state":"idle",
                "progress":0,
                "message":"اختر Backtest Meta محفوظاً لبدء تقييم زمني",
                "latest_id":latest
            }),
        })
    }

    fn load(db: &Connection, id: &str) -> Result<(String, Value, ChronologicalEvaluation)> {
        let (source_id, identity, result): (String, String, String) = db
            .query_row(
                "SELECT source_id,source_identity,result FROM backtest_evaluations WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .context("التقييم الزمني غير موجود")?;
        Ok((
            source_id,
            serde_json::from_str(&identity)?,
            serde_json::from_str(&result)?,
        ))
    }

    pub fn report(db: &Connection, id: &str) -> Result<Value> {
        let (source_id, identity, result) = Self::load(db, id)?;
        Ok(json!({
            "evaluation_id":id,
            "source_id":source_id,
            "source_identity":identity,
            "summary":summary(&result),
            "result":result,
            "note":"Persisted chronological evidence; read-only and does not promote models or change Shadow."
        }))
    }

    pub fn export(db: &Connection, dir: &Path, id: &str) -> Result<Value> {
        let (source_id, identity, result) = Self::load(db, id)?;
        let name = format!("chronological-evaluation-{id}");
        let staging = dir.join("exports").join(format!(".{name}.tmp"));
        let destination = dir.join("exports").join(&name);
        if staging.exists() || destination.exists() {
            bail!("مجلد تصدير التقييم موجود مسبقاً");
        }
        fs::create_dir_all(&staging)?;
        atomic_write(
            &staging.join("evaluation.json"),
            &serde_json::to_vec_pretty(&result)?,
        )?;
        atomic_write(
            &staging.join("source_identity.json"),
            &serde_json::to_vec_pretty(&identity)?,
        )?;
        atomic_write(
            &staging.join("README.txt"),
            format!(
                "BrainTrade chronological evaluation.\nSource backtest: {source_id}\nIndependent development/holdout windows; not a chained equity curve and not profitability evidence.\nNo model promotion or broker execution is performed.\n"
            )
            .as_bytes(),
        )?;
        fs::rename(staging, &destination)?;
        Ok(json!({
            "path":destination,
            "evaluation_id":id,
            "source_id":source_id,
            "note":"evaluation.json + source_identity.json"
        }))
    }
}
