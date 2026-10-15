//! Manual challenger review-eligibility policy.
//! This layer never promotes a model, never enables inference and never alters Shadow.
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const POLICY: &str = "manual-review-eligibility-v1";
const MODEL_SCHEMA: &str = "braintrade-linear-softmax-v1";
const FEATURE_SCHEMA: &str = "gold-m1-feature-v1";
const LABEL_SCHEMA: &str = "gold-m1-outcome-v1";
const EVALUATION_VERSION: &str = "chronological-evaluation-v1";
const SOURCE_SCHEMA: &str = "backtest-source-identity-v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewRequest {
    model_id: String,
    evaluation_ids: Vec<String>,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ContextKey {
    server: String,
    login: u64,
    symbol: String,
    currency: String,
}

fn id_ok(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn parse_dataset_context(raw: &str) -> Option<ContextKey> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let row = value.as_array()?;
    if row.len() != 4 {
        return None;
    }
    Some(ContextKey {
        server: row[0].as_str()?.to_owned(),
        login: row[1].as_u64()?,
        symbol: row[2].as_str()?.to_owned(),
        currency: row[3].as_str()?.to_owned(),
    })
}

fn context_json(context: &ContextKey) -> Value {
    json!({
        "server": context.server,
        "login": context.login,
        "symbol": context.symbol,
        "currency": context.currency
    })
}

fn model_contract(artifact: &Value, stored_dataset_identity: &str) -> Result<BTreeSet<ContextKey>> {
    if artifact["schema"] != MODEL_SCHEMA
        || artifact["feature_schema"] != FEATURE_SCHEMA
        || artifact["label_schema"] != LABEL_SCHEMA
    {
        bail!("Registered challenger schema contract is invalid");
    }
    if artifact["status"] != "unreviewed_challenger"
        || artifact["eligible_for_promotion"] != false
        || artifact["training"]["random_split"] != false
        || artifact["training"]["purged_chronological_split"] != true
        || artifact["training"]["automatic_promotion"] != false
    {
        bail!("Challenger artifact violates manual-review policy");
    }
    let dataset_sha = artifact["dataset"]["sha256"]
        .as_str()
        .context("Challenger dataset identity missing")?;
    if dataset_sha != stored_dataset_identity {
        bail!("Stored dataset identity does not match immutable artifact");
    }
    if artifact["dataset"]["used_rows"].as_u64().unwrap_or(0) < 600 {
        bail!("Challenger does not meet minimum post-purge dataset size");
    }
    let holdout = &artifact["metrics"]["holdout"];
    if holdout["rows"].as_u64().unwrap_or(0) == 0
        || ["accuracy", "balanced_accuracy", "macro_f1", "log_loss", "mean_confidence"]
            .iter()
            .any(|key| !holdout[*key].as_f64().is_some_and(f64::is_finite))
    {
        bail!("Challenger untouched holdout metrics are missing or invalid");
    }
    let splits = artifact["dataset"]["split"]
        .as_array()
        .filter(|rows| !rows.is_empty())
        .context("Challenger chronological split metadata missing")?;
    let mut contexts = BTreeSet::new();
    for split in splits {
        if split["purge_rows_each_side"].as_u64().unwrap_or(0) < 30 {
            bail!("Challenger split purge is below the 30-row label horizon");
        }
        let raw = split["context"]
            .as_str()
            .context("Challenger split context missing")?;
        contexts.insert(parse_dataset_context(raw).context("Challenger split context is invalid")?);
    }
    if contexts.is_empty() {
        bail!("Challenger contains no usable training contexts");
    }
    Ok(contexts)
}

fn evaluation_context(source_identity: &Value, evaluation: &Value, source_run_id: &str) -> Result<ContextKey> {
    if source_identity["schema"] != SOURCE_SCHEMA
        || source_identity["source_run_id"].as_str() != Some(source_run_id)
        || evaluation["version"] != EVALUATION_VERSION
    {
        bail!("Chronological evidence identity/version contract is invalid");
    }
    let context = ContextKey {
        server: source_identity["server"]
            .as_str()
            .context("Evidence server missing")?
            .to_owned(),
        login: source_identity["login"]
            .as_u64()
            .context("Evidence login missing")?,
        symbol: source_identity["symbol"]
            .as_str()
            .context("Evidence symbol missing")?
            .to_owned(),
        currency: source_identity["currency"]
            .as_str()
            .context("Evidence currency missing")?
            .to_owned(),
    };
    if evaluation["server"].as_str() != Some(context.server.as_str())
        || evaluation["login"].as_u64() != Some(context.login)
        || evaluation["symbol"].as_str() != Some(context.symbol.as_str())
        || evaluation["currency"].as_str() != Some(context.currency.as_str())
    {
        bail!("Chronological evaluation context does not match source identity");
    }
    let windows = evaluation["windows"]
        .as_array()
        .filter(|rows| !rows.is_empty())
        .context("Chronological evidence contains no windows")?;
    let mut development = 0usize;
    let mut holdout = 0usize;
    for row in windows {
        let window = &row["window"];
        let from = window["from"].as_u64().context("Evidence window start missing")?;
        let to = window["to"].as_u64().context("Evidence window end missing")?;
        if from >= to {
            bail!("Chronological evidence has an invalid window boundary");
        }
        match window["role"].as_str() {
            Some("development") => development += 1,
            Some("holdout") => holdout += 1,
            _ => bail!("Chronological evidence contains an unknown window role"),
        }
        for strategy in ["meta_v1", "meta_v2"] {
            let first = row[strategy]["first_test_bar"]
                .as_u64()
                .context("Evidence first test bar missing")?;
            let last = row[strategy]["last_test_bar"]
                .as_u64()
                .context("Evidence last test bar missing")?;
            if first < from || last >= to || first > last {
                bail!("Chronological evidence report escaped its declared window");
            }
        }
    }
    if development == 0 || holdout == 0 {
        bail!("Chronological evidence must contain development and holdout windows");
    }
    Ok(context)
}

fn current_challenger(db: &Connection) -> Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT model_id FROM learning_slots WHERE slot='challenger'",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

fn table_exists(db: &Connection, name: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get::<_, i64>(0),
    )? != 0)
}

fn eligibility_for(db: &Connection, model_id: &str, selected: Option<&BTreeSet<String>>) -> Result<Value> {
    let (status, dataset_identity, artifact_raw): (String, String, String) = db
        .query_row(
            "SELECT status,dataset_identity,artifact FROM learning_models WHERE model_id=?1",
            [model_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("Challenger model not found")?;
    if status != "unreviewed_challenger" {
        bail!("Only an unreviewed challenger can enter review eligibility");
    }
    let artifact: Value = serde_json::from_str(&artifact_raw)?;
    let required_contexts = model_contract(&artifact, &dataset_identity)?;
    if !table_exists(db, "chronological_evaluations")? {
        return Ok(json!({
            "policy": POLICY,
            "model_id": model_id,
            "eligible_for_manual_review": false,
            "eligible_for_promotion": false,
            "prediction_enabled": false,
            "required_contexts": required_contexts.iter().map(context_json).collect::<Vec<_>>(),
            "covered_contexts": [],
            "evidence_ids": [],
            "reason": "No chronological evidence store exists yet"
        }));
    }
    let mut statement = db.prepare(
        "SELECT id,source_run_id,source_identity,evaluation FROM chronological_evaluations ORDER BY created_ms DESC,id DESC",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut evidence_by_context: BTreeMap<ContextKey, String> = BTreeMap::new();
    let mut rejected = Vec::new();
    for (id, source_run_id, source_raw, evaluation_raw) in rows {
        if selected.is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        let parsed = (|| -> Result<ContextKey> {
            let source_identity: Value = serde_json::from_str(&source_raw)?;
            let evaluation: Value = serde_json::from_str(&evaluation_raw)?;
            evaluation_context(&source_identity, &evaluation, &source_run_id)
        })();
        match parsed {
            Ok(context) if required_contexts.contains(&context) => {
                evidence_by_context.entry(context).or_insert(id);
            }
            Ok(_) => {}
            Err(error) => rejected.push(json!({"id":id,"reason":error.to_string()})),
        }
    }
    let covered = required_contexts
        .iter()
        .filter(|context| evidence_by_context.contains_key(*context))
        .cloned()
        .collect::<Vec<_>>();
    let missing = required_contexts
        .iter()
        .filter(|context| !evidence_by_context.contains_key(*context))
        .cloned()
        .collect::<Vec<_>>();
    let eligible = missing.is_empty();
    Ok(json!({
        "policy": POLICY,
        "model_id": model_id,
        "dataset_identity": dataset_identity,
        "eligible_for_manual_review": eligible,
        "eligible_for_promotion": false,
        "prediction_enabled": false,
        "required_contexts": required_contexts.iter().map(context_json).collect::<Vec<_>>(),
        "covered_contexts": covered.iter().map(context_json).collect::<Vec<_>>(),
        "missing_contexts": missing.iter().map(context_json).collect::<Vec<_>>(),
        "evidence_ids": evidence_by_context.values().cloned().collect::<Vec<_>>(),
        "rejected_evidence": rejected,
        "note": "Eligibility means prerequisites for human review are present. It is not model validation, promotion approval or a profitability claim."
    }))
}

pub fn init(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS learning_review_records(
            id TEXT PRIMARY KEY,
            model_id TEXT NOT NULL,
            created_ms INTEGER NOT NULL,
            policy TEXT NOT NULL,
            evidence_ids TEXT NOT NULL,
            eligibility TEXT NOT NULL,
            reviewer_note TEXT NOT NULL,
            UNIQUE(model_id,policy,evidence_ids)
        );
        CREATE INDEX IF NOT EXISTS learning_review_records_model_idx
            ON learning_review_records(model_id,created_ms);",
    )?;
    Ok(())
}

pub fn status(db: &Connection) -> Result<Value> {
    let Some(model_id) = current_challenger(db)? else {
        return Ok(json!({
            "policy": POLICY,
            "model_id": null,
            "eligible_for_manual_review": false,
            "eligible_for_promotion": false,
            "prediction_enabled": false,
            "reason": "No current challenger is registered"
        }));
    };
    match eligibility_for(db, &model_id, None) {
        Ok(value) => Ok(value),
        Err(error) => Ok(json!({
            "policy": POLICY,
            "model_id": model_id,
            "eligible_for_manual_review": false,
            "eligible_for_promotion": false,
            "prediction_enabled": false,
            "reason": error.to_string()
        })),
    }
}

fn create(db: &mut Connection, request: ReviewRequest, now: u64) -> Result<Value> {
    if !id_ok(&request.model_id) {
        bail!("Invalid challenger model id");
    }
    if request.note.chars().count() > 500 {
        bail!("Reviewer note is too long");
    }
    if request.evaluation_ids.is_empty() || request.evaluation_ids.len() > 32 {
        bail!("Select between 1 and 32 chronological evidence ids");
    }
    let mut ids = BTreeSet::new();
    for id in request.evaluation_ids {
        if !id_ok(&id) || !ids.insert(id) {
            bail!("Chronological evidence ids must be valid and unique");
        }
    }
    let current = current_challenger(db)?.context("No current challenger is registered")?;
    if current != request.model_id {
        bail!("Only the current challenger can be reviewed");
    }
    let eligibility = eligibility_for(db, &request.model_id, Some(&ids))?;
    if eligibility["eligible_for_manual_review"] != true {
        bail!("Selected evidence does not satisfy every challenger dataset context");
    }
    let canonical_ids = eligibility["evidence_ids"]
        .as_array()
        .context("Eligibility evidence ids missing")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if canonical_ids != ids {
        bail!("Selected evidence contains redundant or unrelated ids; submit exactly the evidence used by the policy");
    }
    let evidence_json = serde_json::to_string(&canonical_ids)?;
    let record_id = format!("review-{now}-{}", uuid::Uuid::new_v4().simple());
    let tx = db.transaction()?;
    tx.execute(
        "INSERT INTO learning_review_records(id,model_id,created_ms,policy,evidence_ids,eligibility,reviewer_note)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            record_id,
            request.model_id,
            now,
            POLICY,
            evidence_json,
            eligibility.to_string(),
            request.note.trim()
        ],
    )?;
    tx.commit()?;
    Ok(json!({
        "ok": true,
        "id": record_id,
        "model_id": request.model_id,
        "policy": POLICY,
        "eligibility": eligibility,
        "immutable": true,
        "eligible_for_promotion": false,
        "prediction_enabled": false,
        "note": "Recorded prerequisite eligibility for human review only. Model status and challenger slot were not changed."
    }))
}

fn list(db: &Connection) -> Result<Value> {
    let mut statement = db.prepare(
        "SELECT id,model_id,created_ms,policy,evidence_ids,eligibility,reviewer_note
         FROM learning_review_records ORDER BY created_ms DESC,id DESC LIMIT 50",
    )?;
    let rows = statement
        .query_map([], |row| {
            let evidence: String = row.get(4)?;
            let eligibility: String = row.get(5)?;
            Ok(json!({
                "id": row.get::<_,String>(0)?,
                "model_id": row.get::<_,String>(1)?,
                "created_ms": row.get::<_,u64>(2)?,
                "policy": row.get::<_,String>(3)?,
                "evidence_ids": serde_json::from_str::<Value>(&evidence).unwrap_or(Value::Null),
                "eligibility": serde_json::from_str::<Value>(&eligibility).unwrap_or(Value::Null),
                "reviewer_note": row.get::<_,String>(6)?,
                "immutable": true,
                "eligible_for_promotion": false,
                "prediction_enabled": false
            }))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(json!({"policy":POLICY,"records":rows,"promotion_enabled":false,"prediction_enabled":false}))
}

fn report(db: &Connection, id: &str) -> Result<Value> {
    if !id_ok(id) {
        bail!("Invalid review record id");
    }
    let (model_id, created_ms, policy, evidence, eligibility, note): (String, u64, String, String, String, String) = db
        .query_row(
            "SELECT model_id,created_ms,policy,evidence_ids,eligibility,reviewer_note FROM learning_review_records WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
        )
        .optional()?
        .context("Review eligibility record not found")?;
    Ok(json!({
        "id": id,
        "model_id": model_id,
        "created_ms": created_ms,
        "policy": policy,
        "evidence_ids": serde_json::from_str::<Value>(&evidence)?,
        "eligibility": serde_json::from_str::<Value>(&eligibility)?,
        "reviewer_note": note,
        "immutable": true,
        "eligible_for_promotion": false,
        "prediction_enabled": false
    }))
}

pub fn enrich_learning_status(db: &Connection, mut base: Value) -> Result<Value> {
    if let Some(object) = base.as_object_mut() {
        object.insert("manual_review_eligibility".into(), status(db)?);
        object.insert("manual_review_policy".into(), Value::String(POLICY.into()));
    }
    Ok(base)
}

pub fn route(method: &str, path: &str, body: &[u8], db: &mut Connection, now: u64) -> Result<Value> {
    match (method, path) {
        ("GET", "/v1/learning/review/status") => status(db),
        ("GET", "/v1/learning/review/records") => list(db),
        ("POST", "/v1/learning/review/create") => {
            create(db, serde_json::from_slice(body)?, now)
        }
        ("POST", "/v1/learning/review/report") => {
            let request: Selection = serde_json::from_slice(body)?;
            report(db, &request.id)
        }
        _ => bail!("Unknown learning review route"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric() -> Value {
        json!({"rows":100,"accuracy":0.4,"balanced_accuracy":0.4,"macro_f1":0.4,"log_loss":1.0,"mean_confidence":0.5})
    }

    fn artifact(contexts: &[ContextKey]) -> Value {
        json!({
            "schema":MODEL_SCHEMA,
            "status":"unreviewed_challenger",
            "eligible_for_promotion":false,
            "feature_schema":FEATURE_SCHEMA,
            "label_schema":LABEL_SCHEMA,
            "target":"15m.direction_0_2atr",
            "dataset":{
                "sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "used_rows":900,
                "split":contexts.iter().map(|c|json!({
                    "context":serde_json::to_string(&(c.server.as_str(),c.login,c.symbol.as_str(),c.currency.as_str())).unwrap(),
                    "purge_rows_each_side":30
                })).collect::<Vec<_>>()
            },
            "training":{"random_split":false,"purged_chronological_split":true,"automatic_promotion":false},
            "metrics":{"train":metric(),"validation":metric(),"holdout":metric()}
        })
    }

    fn evidence(context: &ContextKey) -> (String, String) {
        let source = json!({
            "schema":SOURCE_SCHEMA,
            "source_run_id":"source-1",
            "server":context.server,
            "login":context.login,
            "symbol":context.symbol,
            "currency":context.currency
        });
        let evaluation = json!({
            "version":EVALUATION_VERSION,
            "server":context.server,
            "login":context.login,
            "symbol":context.symbol,
            "currency":context.currency,
            "windows":[
                {"window":{"role":"development","from":100,"to":200},"meta_v1":{"first_test_bar":100,"last_test_bar":199},"meta_v2":{"first_test_bar":100,"last_test_bar":199}},
                {"window":{"role":"holdout","from":200,"to":300},"meta_v1":{"first_test_bar":200,"last_test_bar":299},"meta_v2":{"first_test_bar":200,"last_test_bar":299}}
            ]
        });
        (source.to_string(), evaluation.to_string())
    }

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE learning_models(model_id TEXT PRIMARY KEY,status TEXT NOT NULL,dataset_identity TEXT NOT NULL,artifact TEXT NOT NULL);CREATE TABLE learning_slots(slot TEXT PRIMARY KEY,model_id TEXT);CREATE TABLE chronological_evaluations(id TEXT PRIMARY KEY,source_run_id TEXT NOT NULL,created_ms INTEGER NOT NULL,source_identity TEXT NOT NULL,evaluation TEXT NOT NULL);").unwrap();
        init(&db).unwrap();
        db
    }

    #[test]
    fn every_training_context_requires_independent_chronological_evidence() {
        let mut db = db();
        let a = ContextKey{server:"A".into(),login:1,symbol:"XAUUSD".into(),currency:"USD".into()};
        let b = ContextKey{server:"B".into(),login:2,symbol:"XAUUSD".into(),currency:"USD".into()};
        db.execute("INSERT INTO learning_models VALUES('m1','unreviewed_challenger',?1,?2)",params!["a".repeat(64),artifact(&[a.clone(),b.clone()]).to_string()]).unwrap();
        db.execute("INSERT INTO learning_slots VALUES('challenger','m1')",[]).unwrap();
        let (source_a,eval_a)=evidence(&a);
        db.execute("INSERT INTO chronological_evaluations VALUES('e1','source-1',1,?1,?2)",params![source_a,eval_a]).unwrap();
        let first=status(&db).unwrap();
        assert_eq!(first["eligible_for_manual_review"],false);
        assert_eq!(first["covered_contexts"].as_array().unwrap().len(),1);
        assert_eq!(first["missing_contexts"].as_array().unwrap().len(),1);
        let mut source_b:Value=serde_json::from_str(&evidence(&b).0).unwrap();
        source_b["source_run_id"]=Value::String("source-2".into());
        let eval_b=evidence(&b).1;
        db.execute("INSERT INTO chronological_evaluations VALUES('e2','source-2',2,?1,?2)",params![source_b.to_string(),eval_b]).unwrap();
        let ready=status(&db).unwrap();
        assert_eq!(ready["eligible_for_manual_review"],true);
        assert_eq!(ready["eligible_for_promotion"],false);
        assert_eq!(ready["prediction_enabled"],false);
        let created=create(&mut db,ReviewRequest{model_id:"m1".into(),evaluation_ids:vec!["e1".into(),"e2".into()],note:"manual prerequisite check".into()},3).unwrap();
        assert_eq!(created["eligible_for_promotion"],false);
        assert!(db.query_row("SELECT COUNT(*) FROM learning_slots WHERE slot='champion'",[],|r|r.get::<_,i64>(0)).unwrap()==0);
        assert_eq!(db.query_row("SELECT status FROM learning_models WHERE model_id='m1'",[],|r|r.get::<_,String>(0)).unwrap(),"unreviewed_challenger");
    }

    #[test]
    fn invalid_window_bounds_are_not_accepted_as_evidence() {
        let db = db();
        let a = ContextKey{server:"A".into(),login:1,symbol:"XAUUSD".into(),currency:"USD".into()};
        db.execute("INSERT INTO learning_models VALUES('m1','unreviewed_challenger',?1,?2)",params!["a".repeat(64),artifact(&[a.clone()]).to_string()]).unwrap();
        db.execute("INSERT INTO learning_slots VALUES('challenger','m1')",[]).unwrap();
        let (source,raw)=evidence(&a);let mut evaluation:Value=serde_json::from_str(&raw).unwrap();
        evaluation["windows"][1]["meta_v2"]["last_test_bar"]=json!(300);
        db.execute("INSERT INTO chronological_evaluations VALUES('e1','source-1',1,?1,?2)",params![source,evaluation.to_string()]).unwrap();
        let result=status(&db).unwrap();
        assert_eq!(result["eligible_for_manual_review"],false);
        assert_eq!(result["evidence_ids"].as_array().unwrap().len(),0);
    }
}
