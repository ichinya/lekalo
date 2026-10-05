//! Privacy-safe aggregate export. #121 is the only measurement source.
//! #100 supplies paired trial membership through EvaluationInput.
mod aggregate;
mod privacy;
mod storage;
pub mod types;

use crate::privacy::{export::DestinationSpec, refs, types::PolicyRef, vocab::DataSensitivity};
use crate::run_history::store::{Store, StoreError};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use types::*;

const DEFINITION: &[u8] =
    include_bytes!("../../../../contracts/metrics-aggregation-definition.v0.6.4.json");
const MAX_RUNS: usize = 1024;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy)]
pub enum Error {
    Invalid,
    Evaluation,
    Source,
    Privacy,
    Leak,
    Stale,
    Storage,
    Overlap,
}
impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "metrics-export.input-invalid",
            Self::Evaluation => "metrics-export.evaluation-required",
            Self::Source => "metrics-export.source-invalidated",
            Self::Privacy => "metrics-export.authorization-refused",
            Self::Leak => "metrics-export.leak-refused",
            Self::Stale => "metrics-export.preview-stale",
            Self::Storage => "metrics-export.storage-refused",
            Self::Overlap => "metrics-export.overlap-refused",
        }
    }
}

pub fn refusal(error: Error) -> crate::result::DomainResult {
    let status = if matches!(error, Error::Invalid) {
        crate::result::Status::Invalid
    } else {
        crate::result::Status::Denied
    };
    crate::loader::diagnostic::failure(
        status,
        vec![crate::loader::error::Diagnostic::new(error.code())],
    )
}

pub(crate) fn canon(v: &Value) -> String {
    format!("{}\n", crate::privacy::canonical::canonical(v))
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}
fn json_of<T: serde::Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error::Invalid)
}
fn history(project: &Path) -> Result<Store> {
    if !project.join(".lekalo/history/store.sqlite").is_file() {
        return Err(Error::Source);
    }
    Store::open(project).map_err(|_| Error::Source)
}

fn evaluation(project: &Path, path: &str) -> Result<EvaluationInput> {
    let raw = storage::read(project, path)?;
    let value = storage::parse(&raw)?;
    let mut input: EvaluationInput = serde_json::from_value(value).map_err(|_| Error::Invalid)?;
    if input.schema_version != "lekalo/evaluation-export-input/v0.6.4"
        || input.identity != "dev.lekalo.evaluation-export-input@0.6.4"
        || input.protocol != "framework-lift-paired-trials/1"
        || !input.approved
    {
        return Err(Error::Evaluation);
    }
    if input.trials.is_empty() || input.trials.len() > MAX_RUNS {
        return Err(Error::Invalid);
    }
    let mut runs = BTreeSet::new();
    let mut units = BTreeMap::new();
    for trial in &input.trials {
        if !crate::run_history::validate::is_token(&trial.unit)
            || !crate::run_history::validate::is_hex_token32(&trial.run_id)
            || !runs.insert(&trial.run_id)
            || trial.required_assertions.len() > 64
        {
            return Err(Error::Invalid);
        }
        let mut assertions = BTreeSet::new();
        if trial
            .required_assertions
            .iter()
            .any(|id| !crate::run_history::validate::is_token(id) || !assertions.insert(id))
        {
            return Err(Error::Invalid);
        }
        if !units
            .entry(&trial.unit)
            .or_insert_with(BTreeSet::new)
            .insert(trial.arm)
        {
            return Err(Error::Invalid);
        }
    }
    if units.values().any(|arms| arms.len() != 2) {
        return Err(Error::Evaluation);
    }
    for trial in &mut input.trials {
        trial.required_assertions.sort();
    }
    input.trials.sort_by(|a, b| a.run_id.cmp(&b.run_id));
    Ok(input)
}

fn snapshot(store: &Store, scope: &str, input: &EvaluationInput) -> Result<(u64, Vec<Source>)> {
    store
        .export_locked(None, |store| {
            let generation = store.generation()?;
            let mut sources = Vec::new();
            for trial in &input.trials {
                let (record, assertions) = store.get(scope, &trial.run_id)?;
                let r =
                    storage::parse(record.as_bytes()).map_err(|_| StoreError::Corrupt("record"))?;
                validate_source_shape(&r).map_err(|_| StoreError::Corrupt("record"))?;
                if r["schema_version"] != "lekalo/run-record/v0.4.0"
                    || r["identity"] != "dev.lekalo.run-record@0.4.0"
                    || r["artifactKind"] != "history.run-record"
                    || r["runId"] != trial.run_id
                    || r["scope"]["tenantScopeId"] != scope
                    || r["privacy"]["exportDisposition"] != "local-private"
                    || r["privacy"]["exportEligibility"] != "ineligible"
                {
                    return Err(StoreError::Corrupt("record"));
                }
                let assertion_digest = assertions.as_ref().map(|b| digest(b.as_bytes()));
                let a = assertions
                    .map(|b| {
                        storage::parse(b.as_bytes()).map_err(|_| StoreError::Corrupt("assertions"))
                    })
                    .transpose()?;
                if let Some(a) = &a {
                    validate_assertion_shape(a, &r)
                        .map_err(|_| StoreError::Corrupt("assertions"))?;
                    if a["schema_version"] != "lekalo/run-assertions/v0.4.0"
                        || a["runId"] != trial.run_id
                        || a["setId"] != r["assertionsRef"]["setId"]
                        || Some(r["assertionsRef"]["digest"].as_str().unwrap_or(""))
                            != assertion_digest.as_deref()
                    {
                        return Err(StoreError::Corrupt("assertions"));
                    }
                } else if !r["assertionsRef"].is_null() {
                    return Err(StoreError::Corrupt("assertions"));
                }
                sources.push(Source {
                    trial: trial.clone(),
                    record: r,
                    assertions: a,
                    record_digest: digest(record.as_bytes()),
                    assertion_digest,
                });
            }
            sources.sort_by(|a, b| a.trial.run_id.cmp(&b.trial.run_id));
            Ok((generation, sources))
        })
        .map_err(|_| Error::Source)
}

fn validate_assertion_shape(assertions: &Value, record: &Value) -> Result<()> {
    use crate::run_history::types::AssertionRow;
    if assertions["identity"] != "dev.lekalo.run-assertions@0.4.0"
        || assertions["artifactKind"] != "history.assertion-set"
        || assertions["scope"]["repositoryId"] != record["scope"]["repositoryId"]
        || assertions["scope"]["tenantScopeId"] != record["scope"]["tenantScopeId"]
        || assertions["policyRef"] != record["privacy"]["policyRef"]
        || assertions["authorityRef"] != record["privacy"]["authorityRef"]
        || assertions["dataSensitivity"] != record["privacy"]["dataSensitivity"]
        || assertions["exportDisposition"] != "local-private"
    {
        return Err(Error::Source);
    }
    let rows = assertions["rows"].as_array().ok_or(Error::Source)?;
    if record["assertionsRef"]["count"].as_u64() != Some(rows.len() as u64) {
        return Err(Error::Source);
    }
    let mut ids = BTreeSet::new();
    for row in rows {
        let typed: AssertionRow = serde_json::from_value(row.clone()).map_err(|_| Error::Source)?;
        if !crate::run_history::validate::is_token(&typed.assertion_id)
            || !ids.insert(typed.assertion_id)
        {
            return Err(Error::Source);
        }
    }
    Ok(())
}

fn validate_source_shape(record: &Value) -> Result<()> {
    use crate::run_history::types::{MetricsIn, Pilot, ProvenanceIn, StatusShape};
    static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let schema = SCHEMA.get_or_init(|| {
        serde_json::from_slice(include_bytes!(
            "../../../../contracts/run-record.schema.v0.4.0.json"
        ))
        .expect("embedded record schema")
    });
    let exact = |value: &Value, properties: &Value| -> Result<()> {
        let actual = value.as_object().ok_or(Error::Source)?;
        let expected = properties.as_object().ok_or(Error::Source)?;
        if actual.len() != expected.len() || actual.keys().any(|k| !expected.contains_key(k)) {
            return Err(Error::Source);
        }
        Ok(())
    };
    exact(record, &schema["properties"])?;
    exact(
        &record["scope"],
        &schema["$defs"]["recordScope"]["properties"],
    )?;
    exact(
        &record["privacy"],
        &schema["$defs"]["privacyBlock"]["properties"],
    )?;
    exact(
        &record["provenance"],
        &schema["$defs"]["provenance"]["properties"],
    )?;
    if !crate::run_history::validate::is_hex_token32(
        record["scope"]["repositoryId"]
            .as_str()
            .ok_or(Error::Source)?,
    ) || !schema["$defs"]["recordScope"]["properties"]["repositoryRole"]["enum"]
        .as_array()
        .ok_or(Error::Source)?
        .contains(&record["scope"]["repositoryRole"])
        || record["privacy"]["policyRef"] != json_of(PolicyRef::frozen())?
        || record["privacy"]["authorityRef"]
            != json_of(crate::privacy::types::AuthorityRef::frozen_authority())?
        || record["privacy"]["classificationContractRef"]
            != json!({"contractId":refs::CLASSIFICATION_CONTRACT_ID,"version":refs::DECISION_FAMILY_VERSION})
    {
        return Err(Error::Source);
    }
    serde_json::from_value::<Pilot>(record["pilot"].clone()).map_err(|_| Error::Source)?;
    serde_json::from_value::<StatusShape>(record["status"].clone()).map_err(|_| Error::Source)?;
    serde_json::from_value::<ProvenanceIn>(record["provenance"].clone())
        .map_err(|_| Error::Source)?;
    serde_json::from_value::<MetricsIn>(record["metrics"].clone()).map_err(|_| Error::Source)?;
    Ok(())
}

fn paired(sources: &[Source]) -> Result<()> {
    let mut units = BTreeMap::<&str, Vec<&Source>>::new();
    for s in sources {
        units.entry(&s.trial.unit).or_default().push(s);
    }
    for pair in units.values() {
        let a = &pair[0].record;
        let b = &pair[1].record;
        for pointer in [
            "/pilot",
            "/provenance/git",
            "/provenance/harness",
            "/provenance/profile",
            "/provenance/adapters",
        ] {
            if a.pointer(pointer) != b.pointer(pointer) {
                return Err(Error::Evaluation);
            }
        }
        if pair[0].trial.required_assertions != pair[1].trial.required_assertions {
            return Err(Error::Evaluation);
        }
    }
    Ok(())
}

fn projection(sources: &[Source], input: &EvaluationInput, generation: u64) -> Value {
    json!({"artifactKind":"aggregate.decision","operation":"derive-run-history-aggregates","generation":generation,"evaluation":input,
        "policyRef":PolicyRef::frozen(),"sources":sources.iter().map(|s|json!({"runId":s.trial.run_id,"recordDigest":s.record_digest,"assertionDigest":s.assertion_digest,"privacy":s.record["privacy"]})).collect::<Vec<_>>()})
}

fn labels(sources: &[Source]) -> Result<Vec<DataSensitivity>> {
    let mut found = BTreeSet::new();
    for s in sources {
        found.insert(
            s.record
                .pointer("/privacy/dataSensitivity")
                .and_then(Value::as_str)
                .ok_or(Error::Source)?,
        );
    }
    found
        .into_iter()
        .map(|v| DataSensitivity::parse(v).ok_or(Error::Privacy))
        .collect()
}

/// Preview is read-only. Confirm accepts only the reviewed exact preview digest.
pub fn export(
    project: &Path,
    input_path: &str,
    scope: &str,
    destination: DestinationSpec,
    authorization: Option<&str>,
    recipe: Option<&str>,
    confirm: Option<&str>,
) -> Result<Value> {
    let input = evaluation(project, input_path)?;
    if let Some(path) = recipe {
        if storage::parse(&storage::read(project, path)?)? != storage::parse(DEFINITION)? {
            return Err(Error::Invalid);
        }
    }
    let mut store = history(project)?;
    let (generation, sources) = snapshot(&store, scope, &input)?;
    paired(&sources)?;
    let definition_ref = ExactRef {
        identity: "dev.lekalo.metrics-aggregation-definition@0.6.4".to_owned(),
        digest: digest(DEFINITION),
    };
    let payload = canon(&json_of(aggregate::aggregate(
        &sources,
        definition_ref.clone(),
    )?)?);
    if payload.len() > 1_048_576 {
        return Err(Error::Invalid);
    }
    let payload_digest = digest(payload.as_bytes());
    let projection = projection(&sources, &input, generation);
    let supplied = authorization
        .map(|path| storage::read(project, path).and_then(|bytes| storage::parse(&bytes)))
        .transpose()?;
    let template = privacy::decision_template(
        &payload,
        &projection,
        labels(&sources)?,
        destination,
        supplied.as_ref(),
    )?;
    let decision = privacy::authorize(&template, supplied.as_ref())?;
    let allowed = decision.get("decision").and_then(Value::as_str) == Some("allow");
    let (redaction, leak_scan) = privacy::scan(&payload)?;
    let manifest = json!({"schema_version":"lekalo/metrics-export-manifest/v0.6.4","identity":"dev.lekalo.metrics-export-manifest@0.6.4","view":"public","artifactKind":"aggregate.artifact","payloadDigest":payload_digest,"payloadBytes":payload.len(),"policyRef":template["policyRef"],"authorityRef":template["authorityRef"],"privacyBytePins":{
        "policyFile":format!("sha256:{}",refs::POLICY_RAW_SHA256),"policyManifest":format!("sha256:{}",refs::TRUSTED_MANIFEST_SHA256),"classification":format!("sha256:{}",refs::CLASSIFICATION_CONTRACT_RAW_SHA256),"authorizingEvidence":format!("sha256:{}",refs::AUTHORIZING_EVIDENCE_RAW_SHA256),"authorizationSubjectProfile":format!("sha256:{}",refs::AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256),"privacyInputSchema":format!("sha256:{}",refs::INPUT_SCHEMA_RAW_SHA256),"privacyOutputSchema":format!("sha256:{}",refs::OUTPUT_SCHEMA_RAW_SHA256)},
        "definitionRef":definition_ref,"canonicalization":"sorted-utf8-compact-json-lf/1","validityProtocol":"live-history-and-current-privacy/1","offlineStatus":"unverified","minimumSamples":5,"redactionDiff":redaction,"leakScan":leak_scan,"status":"valid-at-export"});
    if payload.len() + canon(&manifest).len() > 1_048_576 {
        return Err(Error::Invalid);
    }
    privacy::scan(&canon(&manifest))?;
    let manifest_digest = digest(canon(&manifest).as_bytes());
    let package_digest = digest(
        canon(&json!({"payloadDigest":payload_digest,"manifestDigest":manifest_digest})).as_bytes(),
    );
    let preview_digest=digest(canon(&json!({"packageDigest":package_digest,"projection":projection,"authorization":supplied,"destination":destination.as_str()})).as_bytes());
    let id = format!("mx-{}", &preview_digest[7..]);
    let mut preview = json!({"schema_version":"lekalo/metrics-export-preview/v0.6.4","identity":"dev.lekalo.metrics-export-preview@0.6.4","status":if allowed{"ready"}else{"blocked"},"dryRun":true,"written":false,"payload":payload,"payloadDigest":payload_digest,"manifest":manifest,"manifestDigest":manifest_digest,"packageDigest":package_digest,"previewDigest":preview_digest,"destination":destination.as_str(),"decisionTemplate":template,"authorizationSubject":privacy::subject(&template)?,"decision":decision,"redactionDiff":redaction,"leakScan":leak_scan});
    if let Some(confirm) = confirm {
        if confirm != preview_digest {
            return Err(Error::Stale);
        }
        if !allowed {
            return Err(Error::Privacy);
        }
        storage::verify_untracked(project)?;
        // V1 release policy: one metrics aggregate-input per scope lifetime. No freely
        // overlapping/complementary queries that bypass the sample floor.
        let runs: Vec<_> = sources.iter().map(|s| s.trial.run_id.clone()).collect();
        let custody = Custody {
            schema_version: "lekalo/metrics-export-manifest/v0.6.4".to_owned(),
            identity: "dev.lekalo.metrics-export-manifest@0.6.4".to_owned(),
            view: "local".to_owned(),
            generation,
            tenant_scope_id: scope.to_owned(),
            dependent_id: id.clone(),
            evaluation: input,
            projection: projection.clone(),
            payload_digest: payload_digest.clone(),
            manifest_digest: manifest_digest.clone(),
            decision_template: template,
        };
        let custody_bytes = canon(&json_of(custody)?).into_bytes();
        if custody_bytes.len() > 1_048_576 {
            return Err(Error::Invalid);
        }
        store
            .register_metrics_export(scope, &id, &runs, generation)
            .map_err(|e| match e {
                StoreError::DependentExists => Error::Overlap,
                StoreError::CursorStale => Error::Stale,
                _ => Error::Source,
            })?;
        store
            .export_locked(Some(generation), |locked| {
                locked.resolve_dependent(scope, &id)?;
                for source in &sources {
                    let (r, a) = locked.get(scope, &source.trial.run_id)?;
                    if digest(r.as_bytes()) != source.record_digest
                        || a.as_ref().map(|v| digest(v.as_bytes())) != source.assertion_digest
                    {
                        return Err(StoreError::Corrupt("record"));
                    }
                }
                let bytes = |v: &Value| canon(v).into_bytes();
                // No manifest exists before both payload and private custody have
                // been durably written. Interrupted preparation stays unavailable.
                storage::verify_untracked(project).map_err(|_| StoreError::Io)?;
                storage::write(project, ".lekalo/privacy/.gitignore", b"*\n")
                    .map_err(|_| StoreError::Io)?;
                storage::write(
                    project,
                    &package_path(&id, "payload.json"),
                    payload.as_bytes(),
                )
                .map_err(|_| StoreError::Io)?;
                storage::write(project, &custody_path(&id), &custody_bytes)
                    .map_err(|_| StoreError::Io)?;
                storage::write(
                    project,
                    &package_path(&id, "manifest.json"),
                    &bytes(&manifest),
                )
                .map_err(|_| StoreError::Io)?;
                Ok(())
            })
            .map_err(|e| {
                if matches!(e, StoreError::Io) {
                    Error::Storage
                } else {
                    Error::Stale
                }
            })?;
        preview["status"] = json!("written");
        preview["dryRun"] = json!(false);
        preview["written"] = json!(true);
        preview["exportId"] = json!(id);
    }
    Ok(preview)
}

fn package_path(id: &str, file: &str) -> String {
    format!(".lekalo/privacy/aggregates/{}/{file}", &id[3..])
}
fn custody_path(id: &str) -> String {
    format!(
        ".lekalo/privacy/decisions/aggregate/{}/{}.json",
        &id[3..35],
        &id[35..]
    )
}

/// Current source liveness is checked independently of historical digest validity.
pub fn status(project: &Path, id: &str, scope: &str, authorization: Option<&str>) -> Result<Value> {
    if id.len() != 67
        || !id.starts_with("mx-")
        || !id[3..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(Error::Invalid);
    }
    let raw = storage::parse(&storage::read(project, &custody_path(id))?)?;
    let custody: Custody = serde_json::from_value(raw).map_err(|_| Error::Invalid)?;
    if custody.tenant_scope_id != scope {
        return Err(Error::Source);
    }
    let mut manifest =
        storage::parse(&storage::read(project, &package_path(id, "manifest.json"))?)?;
    let payload = storage::read(project, &package_path(id, "payload.json"))?;
    if custody.schema_version != "lekalo/metrics-export-manifest/v0.6.4"
        || custody.identity != "dev.lekalo.metrics-export-manifest@0.6.4"
        || custody.view != "local"
        || custody.dependent_id != id
        || digest(&payload) != custody.payload_digest
        || digest(canon(&manifest).as_bytes()) != custody.manifest_digest
    {
        return Err(Error::Invalid);
    }
    let store = history(project)?;
    let live = store
        .export_locked(None, |locked| {
            locked.resolve_dependent(scope, id)?;
            for source in custody.projection["sources"]
                .as_array()
                .ok_or(StoreError::Corrupt("dependent"))?
            {
                let (r, a) = locked.get(
                    scope,
                    source["runId"]
                        .as_str()
                        .ok_or(StoreError::Corrupt("dependent"))?,
                )?;
                if digest(r.as_bytes()) != source["recordDigest"]
                    || a.as_ref().map(|v| digest(v.as_bytes()))
                        != source["assertionDigest"].as_str().map(str::to_owned)
                {
                    return Err(StoreError::Corrupt("record"));
                }
            }
            Ok(())
        })
        .is_ok();
    let authorized = if let Some(path) = authorization {
        let value = storage::parse(&storage::read(project, path)?)?;
        privacy::authorize(&custody.decision_template, Some(&value))?
            .get("decision")
            .and_then(Value::as_str)
            == Some("allow")
    } else {
        false
    };
    manifest["status"] = json!(if !live || (authorization.is_some() && !authorized) {
        "invalidated"
    } else if authorized {
        "valid-at-export"
    } else {
        "unverified"
    });
    Ok(manifest)
}
