//! Scenario run-record evidence ingest (issue #47, plan S9).
//!
//! One scenario run produces one durable `lekalo/scenario-run/v0.4.0`
//! document in the adjudicated ingest home
//! (`.lekalo/import/scenario-runs/`), written by the generated test's
//! reporter. This module is the closed-shape custody for those
//! documents: bounded parsing over an already-read document, the closed
//! outcome vocabulary (an `unsupported` row can never roll up as a
//! pass), and the deterministic summary the verify component reports.
//!
//! The module is pure: reading the ingest directory stays with the
//! caller (the verify pipeline), exactly like every other evidence
//! ingest in this crate.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;

/// The exact wire discriminator of the run-record contract.
pub const SCHEMA_VERSION: &str = "lekalo/scenario-run/v0.4.0";

/// The exact contract identity.
pub const IDENTITY: &str = "dev.lekalo.scenario-run@0.4.0";

/// The ingest home of the run records (an adjudicated `.lekalo/import`
/// home, never the protected ir/cache homes).
pub const INGEST_DIR: &str = ".lekalo/import/scenario-runs";

/// The closed set of top-level members, in canonical byte-sorted order.
const TOP_LEVEL_KEYS: &[&str] = &[
    "assertions",
    "binding_mode",
    "identity",
    "profile",
    "runner",
    "scenario",
    "schema_version",
    "started_by",
    "test",
];

/// The closed outcome vocabulary (plan §7). An `unsupported` outcome is
/// honest absence: it never rolls up as a pass.
pub const OUTCOMES: &[&str] = &["pass", "fail", "unsupported", "infrastructure", "degraded"];

/// One validated scenario run record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRecord {
    /// The stable scenario identity.
    pub scenario_id: String,
    /// The explicit scenario contract version.
    pub scenario_version: String,
    /// The pinned compiled-IR digest the scenario declared.
    pub ir_digest: String,
    /// The runner identity and version the run used.
    pub runner: (String, String),
    /// The test identity, path, and byte fingerprint of the run.
    pub test: (String, String, String),
    /// The binding mode the test was generated/checked under.
    pub binding_mode: String,
    /// The sorted operation symbols the run executed.
    pub operations: Vec<String>,
    /// One bounded outcome row per executed assertion.
    pub assertions: Vec<AssertionRow>,
}

/// One assertion outcome row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssertionRow {
    /// The observed then-step (or null for scenario-level rows).
    pub step_id: Option<String>,
    /// The step the assertion observes.
    pub observes: Option<String>,
    /// The closed assertion kind (or `scenario`/`given:` rows).
    pub kind: String,
    /// The closed outcome.
    pub outcome: String,
}

/// The deterministic roll-up of one run record's rows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RunSummary {
    /// Rows that passed.
    pub passed: usize,
    /// Rows that failed on an assertion.
    pub failed: usize,
    /// Rows that recorded an explicit unsupported outcome.
    pub unsupported: usize,
    /// Rows that failed on infrastructure.
    pub infrastructure: usize,
    /// Rows recorded as degraded.
    pub degraded: usize,
}

impl RunSummary {
    /// Whether every row passed and none was unsupported — the only
    /// state a run may be counted as covered execution.
    pub fn all_passed(&self) -> bool {
        self.passed > 0
            && self.failed == 0
            && self.unsupported == 0
            && self.infrastructure == 0
            && self.degraded == 0
    }

    /// Whether any row blocks: an assertion failure or an
    /// infrastructure failure.
    pub fn has_blocking_failure(&self) -> bool {
        self.failed > 0 || self.infrastructure > 0
    }
}

impl RunRecord {
    /// The deterministic roll-up of this record's rows.
    pub fn summary(&self) -> RunSummary {
        let mut summary = RunSummary::default();
        for row in &self.assertions {
            match row.outcome.as_str() {
                "pass" => summary.passed += 1,
                "fail" => summary.failed += 1,
                "unsupported" => summary.unsupported += 1,
                "infrastructure" => summary.infrastructure += 1,
                "degraded" => summary.degraded += 1,
                _ => {}
            }
        }
        summary
    }

    /// Normalize one decoded JSON document into a validated run record,
    /// or return the typed rejection set. Pure.
    pub fn from_value(json: &Json) -> Result<RunRecord, DiagnosticSet> {
        let object = json.as_object().ok_or_else(|| run_invalid("shape"))?;
        let keys: Vec<&str> = object.keys().map(String::as_str).collect();
        if keys != TOP_LEVEL_KEYS {
            return Err(run_invalid(if object.len() == TOP_LEVEL_KEYS.len() {
                "member-order"
            } else {
                "member-set"
            }));
        }
        if object.get("schema_version").and_then(Json::as_str) != Some(SCHEMA_VERSION) {
            return Err(run_invalid("schema-version"));
        }
        if object.get("identity").and_then(Json::as_str) != Some(IDENTITY) {
            return Err(run_invalid("identity"));
        }
        let scenario = object
            .get("scenario")
            .and_then(Json::as_object)
            .ok_or_else(|| run_invalid("scenario-shape"))?;
        if scenario.keys().map(String::as_str).collect::<Vec<&str>>()
            != ["id", "ir_digest", "operations", "symbols", "version"]
        {
            return Err(run_invalid("scenario-members"));
        }
        let scenario_id = bounded_token(scenario.get("id"), "scenario-id")?;
        let scenario_version = bounded_token(scenario.get("version"), "scenario-version")?;
        let ir_digest = scenario
            .get("ir_digest")
            .and_then(Json::as_str)
            .ok_or_else(|| run_invalid("ir-digest"))?;
        if !crate::lockfile::types::Sha256Digest::parse(ir_digest).is_ok() {
            return Err(run_invalid("ir-digest"));
        }
        for member in ["operations", "symbols"] {
            let items = scenario
                .get(member)
                .and_then(Json::as_array)
                .ok_or_else(|| run_invalid("scenario-lists"))?;
            if items.len() > 64 {
                return Err(run_invalid("scenario-lists"));
            }
            for item in items {
                if item.as_str().map(bounded_token_text).is_none() {
                    return Err(run_invalid("scenario-lists"));
                }
            }
        }
        let runner = object
            .get("runner")
            .and_then(Json::as_object)
            .ok_or_else(|| run_invalid("runner-shape"))?;
        if runner.keys().map(String::as_str).collect::<Vec<&str>>() != ["id", "version"] {
            return Err(run_invalid("runner-members"));
        }
        let runner_id = bounded_token(runner.get("id"), "runner-id")?;
        let runner_version = bounded_token(runner.get("version"), "runner-version")?;
        let profile = object
            .get("profile")
            .ok_or_else(|| run_invalid("profile"))?;
        match profile {
            Json::Null => {}
            Json::Object(map) => {
                if map.keys().map(String::as_str).collect::<Vec<&str>>()
                    != ["digest", "id", "version"]
                {
                    return Err(run_invalid("profile-members"));
                }
            }
            _ => return Err(run_invalid("profile")),
        }
        let test = object
            .get("test")
            .and_then(Json::as_object)
            .ok_or_else(|| run_invalid("test-shape"))?;
        if test.keys().map(String::as_str).collect::<Vec<&str>>() != ["fingerprint", "id", "path"] {
            return Err(run_invalid("test-members"));
        }
        let test_id = bounded_token(test.get("id"), "test-id")?;
        let test_path = bounded_token(test.get("path"), "test-path")?;
        let test_fingerprint = test
            .get("fingerprint")
            .and_then(Json::as_str)
            .ok_or_else(|| run_invalid("test-fingerprint"))?;
        if !crate::lockfile::types::Sha256Digest::parse(test_fingerprint).is_ok() {
            return Err(run_invalid("test-fingerprint"));
        }
        let binding_mode = bounded_token(object.get("binding_mode"), "binding-mode")?;
        if !matches!(
            binding_mode.as_str(),
            "generated" | "scaffolded" | "checked"
        ) {
            return Err(run_invalid("binding-mode"));
        }
        let operations = {
            let items = scenario
                .get("operations")
                .and_then(Json::as_array)
                .ok_or_else(|| run_invalid("scenario-lists"))?;
            let mut operations = Vec::with_capacity(items.len());
            for item in items {
                operations.push(bounded_token(Some(item), "scenario-lists")?);
            }
            operations
        };
        bounded_token(object.get("started_by"), "started-by")?;
        let assertions = object
            .get("assertions")
            .and_then(Json::as_array)
            .ok_or_else(|| run_invalid("assertions-shape"))?;
        if assertions.is_empty() || assertions.len() > 1024 {
            return Err(run_invalid("assertions-bound"));
        }
        let mut rows = Vec::with_capacity(assertions.len());
        for row in assertions {
            let row = row
                .as_object()
                .ok_or_else(|| run_invalid("assertion-shape"))?;
            if row.keys().any(|key| {
                !["detail", "kind", "observes", "outcome", "step_id"].contains(&key.as_str())
            }) {
                return Err(run_invalid("assertion-members"));
            }
            let kind = bounded_token(row.get("kind"), "assertion-kind")?;
            let outcome = bounded_token(row.get("outcome"), "assertion-outcome")?;
            if !OUTCOMES.contains(&outcome.as_str()) {
                return Err(run_invalid("assertion-outcome"));
            }
            let optional_token = |key: &str| -> Result<Option<String>, DiagnosticSet> {
                match row.get(key) {
                    // Review F-12: the wire schema requires step_id and
                    // observes on every assertion row — absent is refused,
                    // while an explicit null stays the legal scenario-level
                    // spelling.
                    None => Err(run_invalid("assertion-members")),
                    Some(Json::Null) => Ok(None),
                    Some(value) => Ok(Some(bounded_token(Some(value), "assertion-step")?)),
                }
            };
            rows.push(AssertionRow {
                step_id: optional_token("step_id")?,
                observes: optional_token("observes")?,
                kind,
                outcome,
            });
        }
        Ok(RunRecord {
            scenario_id,
            scenario_version,
            ir_digest: ir_digest.to_owned(),
            runner: (runner_id, runner_version),
            test: (test_id, test_path, test_fingerprint.to_owned()),
            binding_mode,
            operations,
            assertions: rows,
        })
    }
}

fn bounded_token(value: Option<&Json>, detail: &'static str) -> Result<String, DiagnosticSet> {
    let text = value
        .and_then(Json::as_str)
        .ok_or_else(|| run_invalid(detail))?;
    bounded_token_text(text);
    if text.is_empty() || text.chars().count() > 256 || text.chars().any(char::is_control) {
        return Err(run_invalid(detail));
    }
    Ok(text.to_owned())
}

fn bounded_token_text(text: &str) -> bool {
    !text.is_empty() && text.chars().count() <= 256 && !text.chars().any(char::is_control)
}

/// The provenance/stamp context one trace export needs (plan S10,
/// extended by review F-7 with the manifest header fields).
#[derive(Clone, Debug)]
pub struct TraceContext {
    /// The manifest revision the exported relations confirm under.
    pub manifest_revision: String,
    /// The optional gate identity exporting the edges.
    pub gate: Option<String>,
    /// The project id the manifest names.
    pub project: String,
    /// The manifest id of the exported document.
    pub manifest_id: String,
    /// The source Model digest the manifest header pins.
    pub model_digest: String,
}

impl TraceContext {
    /// One context for `manifest_revision`, without a gate.
    pub fn new(manifest_revision: impl Into<String>) -> Self {
        Self {
            manifest_revision: manifest_revision.into(),
            gate: None,
            project: "planner".to_owned(),
            manifest_id: "lekalo-trace-scenario".to_owned(),
            model_digest: format!("sha256:{}", "0".repeat(64)),
        }
    }
}

/// The typed trace edges of one run record (issue #47, plan S10): the
/// `verifies` edges the record's identities license — native_test →
/// scenario, and native_test → each executed operation symbol — plus,
/// when a gate context is present, the `evidences` edges gate →
/// native_test and gate → scenario. Every relation is validated through
/// the production legality matrix, the canonical id recomputation, and
/// the confirmed-status policy, so an illegal edge is a construction
/// refusal, never an exported lie.
pub fn trace_relations(
    record: &RunRecord,
    context: &TraceContext,
) -> Result<Vec<crate::trace::relation::Relation>, DiagnosticSet> {
    use crate::trace::id;
    use crate::trace::node::NodeKind;
    use crate::trace::provenance::{Confidence, Origin, Provenance, Status};
    use crate::trace::relation::{Relation, RelationKind};
    // Trace node ids are prefixed semantic ids (plan §7/§10): the native
    // test, the scenario, each executed operation symbol, and the gate
    // all carry their closed kind prefix.
    let test_node = format!("native_test:{}", record.test.0);
    let scenario_node = format!("scenario:{}", record.scenario_id);
    if !id::is_node_id(&test_node) || !id::is_node_id(&scenario_node) {
        return Err(run_invalid("trace-node"));
    }
    let provenance = Provenance {
        origin: Origin::Declared,
        source_system: crate::trace::node::ExternalSystem::SourceNative
            .as_str()
            .to_owned(),
        source_revision: context.manifest_revision.clone(),
        source_digest: record.test.2.clone(),
        recorded_by: "lekalo.core".to_owned(),
        source_path: Some(record.test.1.clone()),
    };
    provenance
        .validate()
        .map_err(|_| run_invalid("trace-provenance"))?;
    let build = |kind: RelationKind,
                 from: &str,
                 from_kind: NodeKind,
                 to: &str,
                 to_kind: NodeKind,
                 occurrence: u64|
     -> Result<Relation, DiagnosticSet> {
        if !id::is_node_id(from) || !id::is_node_id(to) {
            return Err(run_invalid("trace-node"));
        }
        let occurrence = format!("scenario-run-{occurrence}");
        if !id::is_occurrence(&occurrence) {
            return Err(run_invalid("trace-occurrence"));
        }
        if !kind.endpoints_legal(from_kind, to_kind) {
            return Err(run_invalid("trace-endpoints"));
        }
        let relation = Relation {
            relation_id: Relation::canonical_id(kind, from, to, &occurrence),
            relation_kind: kind,
            from_node: from.to_owned(),
            to_node: to.to_owned(),
            occurrence,
            provenance: provenance.clone(),
            confidence: Confidence::Exact,
            status: Status::Confirmed,
            evidence_refs: vec![test_node.to_owned()],
        };
        relation
            .validate(&context.manifest_revision)
            .map_err(|_| run_invalid("trace-relation"))?;
        Ok(relation)
    };
    let mut relations = Vec::new();
    let mut occurrence = 0u64;
    occurrence += 1;
    relations.push(build(
        RelationKind::Verifies,
        &test_node,
        NodeKind::NativeTest,
        &scenario_node,
        NodeKind::Scenario,
        occurrence,
    )?);
    for operation in &record.operations {
        occurrence += 1;
        let symbol_node = format!("symbol:{operation}");
        relations.push(build(
            RelationKind::Verifies,
            &test_node,
            NodeKind::NativeTest,
            &symbol_node,
            NodeKind::Symbol,
            occurrence,
        )?);
    }
    if let Some(gate) = &context.gate {
        let gate_node = format!("gate:{gate}");
        if !id::is_node_id(&gate_node) {
            return Err(run_invalid("trace-node"));
        }
        for (to, to_kind) in [
            (&test_node, NodeKind::NativeTest),
            (&scenario_node, NodeKind::Scenario),
        ] {
            occurrence += 1;
            relations.push(build(
                RelationKind::Evidences,
                &gate_node,
                NodeKind::Gate,
                to,
                to_kind,
                occurrence,
            )?);
        }
    }
    Ok(relations)
}

/// The exported trace manifest document of validated run records (issue
/// #47 review F-7): assembles the closed trace-manifest wire document —
/// native_test / scenario / symbol / gate nodes plus the verified
/// relations — and proves it through the established production
/// mechanism ([`crate::trace::TraceManifest::parse`]). The manifest is
/// always `partial` with an explicit missing-requirement gap: scenario
/// evidence licenses the test→scenario segment, never a full
/// requirement chain.
pub fn trace_manifest_document(
    records: &[RunRecord],
    context: &TraceContext,
) -> Result<serde_json::Value, DiagnosticSet> {
    use crate::trace::node::NodeKind;
    use crate::trace::relation::{Relation, RelationKind};
    use serde_json::json;
    if records.is_empty() {
        return Err(run_invalid("trace-records-empty"));
    }
    let mut nodes = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();
    let mut occurrence = 0u64;
    let mut seen_nodes = std::collections::BTreeSet::new();
    let push_node = |nodes: &mut Vec<Json>, seen: &mut std::collections::BTreeSet<String>, node: Json| {
        if seen.insert(node["nodeId"].as_str().unwrap_or_default().to_owned()) {
            nodes.push(node);
        }
    };
    for record in records {
        let test_node = format!("native_test:{}", record.test.0);
        let scenario_node = format!("scenario:{}", record.scenario_id);
        push_node(
            &mut nodes,
            &mut seen_nodes,
            json!({
                "nodeId": test_node,
                "nodeKind": "native_test",
                "testId": record.test.0,
                "path": record.test.1,
                "evidenceDigest": record.test.2,
            }),
        );
        push_node(
            &mut nodes,
            &mut seen_nodes,
            json!({
                "nodeId": scenario_node,
                "nodeKind": "scenario",
                "scenarioId": record.scenario_id,
            }),
        );
        for operation in &record.operations {
            push_node(
                &mut nodes,
                &mut seen_nodes,
                json!({
                    "nodeId": format!("symbol:{operation}"),
                    "nodeKind": "symbol",
                    "semanticId": operation,
                }),
            );
        }
        let mut record_relations = trace_relations(record, context)?;
        // Global occurrence uniqueness: re-stamp per document and derive
        // the canonical id over the final tuple.
        for relation in &mut record_relations {
            occurrence += 1;
            relation.occurrence = format!("scenario-run-{occurrence}");
            relation.relation_id = Relation::canonical_id(
                relation.relation_kind,
                &relation.from_node,
                &relation.to_node,
                &relation.occurrence,
            );
            relation
                .validate(&context.manifest_revision)
                .map_err(|_| run_invalid("trace-relation"))?;
        }
        relations.extend(record_relations);
    }
    if let Some(gate) = &context.gate {
        let gate_node = format!("gate:{gate}");
        push_node(
            &mut nodes,
            &mut seen_nodes,
            json!({
                "nodeId": gate_node,
                "nodeKind": "gate",
                "gateId": gate,
            }),
        );
        // The evidences edges need the gate node present; rebuild the
        // gate half once the node set is complete.
        relations.retain(|relation| relation.relation_kind != RelationKind::Evidences);
        for record in records {
            let from = format!("gate:{gate}");
            for (to, to_kind) in [
                (format!("native_test:{}", record.test.0), NodeKind::NativeTest),
                (format!("scenario:{}", record.scenario_id), NodeKind::Scenario),
            ] {
                occurrence += 1;
                let mut relation = Relation {
                    relation_id: String::new(),
                    relation_kind: RelationKind::Evidences,
                    from_node: from.clone(),
                    to_node: to,
                    occurrence: format!("scenario-run-{occurrence}"),
                    provenance: relations
                        .first()
                        .expect("relations non-empty")
                        .provenance
                        .clone(),
                    confidence: crate::trace::provenance::Confidence::Exact,
                    status: crate::trace::provenance::Status::Confirmed,
                    evidence_refs: vec![format!("native_test:{}", record.test.0)],
                };
                relation.relation_id = Relation::canonical_id(
                    relation.relation_kind,
                    &relation.from_node,
                    &relation.to_node,
                    &relation.occurrence,
                );
                relation
                    .validate(&context.manifest_revision)
                    .map_err(|_| run_invalid("trace-relation"))?;
                let _ = to_kind;
                relations.push(relation);
            }
        }
    }
    // Partial manifests require at least one explicit gap: the exported
    // segment covers test→scenario→symbol, never a requirement chain.
    let anchor = nodes[0]["nodeId"].clone();
    let document = json!({
        "schemaVersion": "lekalo/trace-manifest/v0.2.16",
        "identity": crate::trace::version::IDENTITY,
        "manifestId": context.manifest_id,
        "projectRef": context.project,
        "completeness": "partial",
        "sourceRevision": context.manifest_revision,
        "modelRef": {
            "schemaVersion": "0.2.16",
            "digest": context.model_digest,
        },
        "exportProfile": "requirement-to-test",
        "nodes": nodes,
        "relations": relations.iter().map(|relation| serde_json::to_value(relation).expect("relation serializes")).collect::<Vec<Json>>(),
        "gaps": [{
            "gapKind": "missing-requirement",
            "status": "candidate",
            "anchorNode": anchor,
        }],
    });
    // Production custody: the established parser re-validates every
    // member, the canonical order, the endpoint legality, and the
    // completeness policy over the exact assembled bytes.
    let bytes = serde_json::to_vec_pretty(&document).expect("document serializes");
    crate::trace::TraceManifest::parse(&bytes)
        .map_err(|_| run_invalid("trace-manifest"))?;
    Ok(document)
}

/// The fatal set for one run-record violation: the registered
/// `scenario.run-record-invalid` diagnostic with a fixed class token.
pub fn run_invalid(detail: &'static str) -> DiagnosticSet {
    let mut data = crate::diagnostics::DataObject::new();
    data.insert(
        "detail".to_owned(),
        crate::scenario::diagnostic::token(detail),
    );
    match crate::scenario::diagnostic::one("scenario.run-record-invalid", None, data) {
        Ok(diagnostic) => crate::scenario::diagnostic::invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid() -> Json {
        // Canonical form: byte-sorted member order at every level, exactly
        // like the reporter's canonical writer emits.
        json!({
            "assertions": [
                { "kind": "result", "observes": "run", "outcome": "pass", "step_id": "output" },
                { "kind": "entity_state", "observes": "run", "outcome": "pass", "step_id": "state" }
            ],
            "binding_mode": "generated",
            "identity": "dev.lekalo.scenario-run@0.4.0",
            "profile": null,
            "runner": { "id": "node:test", "version": "24.13.0" },
            "schema_version": "lekalo/scenario-run/v0.4.0",
            "scenario": {
                "id": "planner.scenario.focus_happy",
                "ir_digest": format!("sha256:{}", "1".repeat(64)),
                "operations": ["planner.focus_task"],
                "symbols": [],
                "version": "0.2.16"
            },
            "started_by": "lekalo-scenario-harness",
            "test": {
                "fingerprint": format!("sha256:{}", "2".repeat(64)),
                "id": "planner.scenario.focus_happy",
                "path": "src/generated/node-typescript/scenario-tests/planner/planner.scenario.focus_happy.test.ts"
            }
        })
    }

    #[test]
    fn parses_a_valid_record_and_rolls_up() {
        let record = RunRecord::from_value(&valid()).expect("valid record");
        assert_eq!(record.scenario_id, "planner.scenario.focus_happy");
        assert_eq!(
            record.runner,
            ("node:test".to_owned(), "24.13.0".to_owned())
        );
        assert_eq!(record.binding_mode, "generated");
        let summary = record.summary();
        assert_eq!(summary.passed, 2);
        assert!(summary.all_passed());
        assert!(!summary.has_blocking_failure());
    }

    #[test]
    fn unsupported_rows_never_roll_up_as_passed() {
        let mut document = valid();
        document["assertions"][0]["outcome"] = json!("unsupported");
        let record = RunRecord::from_value(&document).expect("valid record");
        let summary = record.summary();
        assert_eq!(summary.unsupported, 1);
        assert!(!summary.all_passed(), "unsupported is never a pass");
        assert!(!summary.has_blocking_failure());
    }

    #[test]
    fn failing_and_infrastructure_rows_block() {
        let mut document = valid();
        document["assertions"][0]["outcome"] = json!("fail");
        assert!(RunRecord::from_value(&document)
            .expect("valid")
            .summary()
            .has_blocking_failure());
        document["assertions"][0]["outcome"] = json!("infrastructure");
        assert!(RunRecord::from_value(&document)
            .expect("valid")
            .summary()
            .has_blocking_failure());
    }

    #[test]
    fn unknown_members_outcomes_and_digests_are_refused() {
        let mutated = |mutate: &dyn Fn(&mut Json)| {
            let mut document = valid();
            mutate(&mut document);
            assert!(RunRecord::from_value(&document).is_err());
        };
        mutated(&|d| {
            d["extra"] = json!(1);
        });
        mutated(&|d| {
            d["schema_version"] = json!("lekalo/scenario-run/v0.2.16");
        });
        mutated(&|d| {
            d["identity"] = json!("dev.lekalo.scenario-run@0.2.16");
        });
        mutated(&|d| {
            d["assertions"][0]["outcome"] = json!("skipped");
        });
        mutated(&|d| {
            d["scenario"]["ir_digest"] = json!("sha256:short");
        });
        mutated(&|d| {
            d["test"]["fingerprint"] = json!("nothash");
        });
        mutated(&|d| {
            d["binding_mode"] = json!("mystery");
        });
        mutated(&|d| {
            d["assertions"] = json!([]);
        });
        mutated(&|d| {
            d["profile"] = json!("default");
        });
    }

    #[test]
    fn assertion_rows_require_the_schema_members_present() {
        // Review F-12: the wire schema requires step_id and observes on
        // every assertion row; the parser refuses what the schema
        // refuses instead of silently widening the closed shape.
        let mut absent = valid();
        absent["assertions"][0]
            .as_object_mut()
            .expect("row object")
            .remove("step_id");
        assert!(RunRecord::from_value(&absent).is_err());
        let mut absent = valid();
        absent["assertions"][0]
            .as_object_mut()
            .expect("row object")
            .remove("observes");
        assert!(RunRecord::from_value(&absent).is_err());
        // An explicit null stays the legal scenario-level spelling.
        let mut nullable = valid();
        nullable["assertions"][0]["step_id"] = json!(null);
        nullable["assertions"][0]["observes"] = json!(null);
        assert!(RunRecord::from_value(&nullable).is_ok());
    }

    #[test]
    fn identity_and_ingest_dir_are_pinned() {
        assert_eq!(INGEST_DIR, ".lekalo/import/scenario-runs");
        assert_eq!(SCHEMA_VERSION, "lekalo/scenario-run/v0.4.0");
        assert_eq!(IDENTITY, "dev.lekalo.scenario-run@0.4.0");
    }

    #[test]
    fn trace_relations_export_the_verifies_and_evidences_edges() {
        let record = RunRecord::from_value(&valid()).expect("valid record");
        let context = TraceContext::new("3".repeat(64));
        let relations = trace_relations(&record, &context).expect("legal edges");
        // native_test -> scenario + native_test -> each operation symbol.
        assert_eq!(relations.len(), 2);
        assert_eq!(
            relations[0].from_node,
            "native_test:planner.scenario.focus_happy"
        );
        assert_eq!(
            relations[0].to_node,
            "scenario:planner.scenario.focus_happy"
        );
        assert_eq!(relations[1].to_node, "symbol:planner.focus_task");
        for relation in &relations {
            assert!(matches!(
                relation.status,
                crate::trace::provenance::Status::Confirmed
            ));
            assert_eq!(
                relation.evidence_refs,
                ["native_test:planner.scenario.focus_happy"]
            );
        }
        // With a gate context the evidences edges ride along.
        let mut gated = TraceContext::new("3".repeat(64));
        gated.gate = Some("planner.gate.scenario".to_owned());
        let relations = trace_relations(&record, &gated).expect("legal edges");
        assert_eq!(relations.len(), 4);
        assert_eq!(relations[2].from_node, "gate:planner.gate.scenario");
        assert_eq!(
            relations[3].to_node,
            "scenario:planner.scenario.focus_happy"
        );
    }

    #[test]
    fn trace_relations_refuse_non_node_endpoints() {
        let mut document = valid();
        document["scenario"]["id"] = json!("not a node id!");
        let record = RunRecord::from_value(&document).expect("valid record");
        assert!(trace_relations(&record, &TraceContext::new("rev")).is_err());
    }

    #[test]
    fn trace_manifest_document_builds_a_parse_valid_manifest_with_rows() {
        // Review F-7: the exported relations persist through the
        // established trace manifest mechanism — the builder assembles
        // the closed document and the production parser re-validates it.
        let record = RunRecord::from_value(&valid()).expect("valid record");
        let context = TraceContext {
            manifest_revision: "3".repeat(64),
            gate: Some("planner.gate.scenario".to_owned()),
            project: "planner".to_owned(),
            manifest_id: "lekalo-trace-scenario".to_owned(),
            model_digest: format!("sha256:{}", "4".repeat(64)),
        };
        let document = trace_manifest_document(&[record], &context).expect("valid manifest");
        assert_eq!(document["schemaVersion"], json!("lekalo/trace-manifest/v0.2.16"));
        assert_eq!(document["projectRef"], json!("planner"));
        assert_eq!(document["completeness"], json!("partial"));
        // The kinds the scenario evidence licenses, explicitly present.
        let kinds: Vec<&str> = document["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .map(|node| node["nodeKind"].as_str().expect("kind"))
            .collect();
        assert!(kinds.contains(&"native_test"));
        assert!(kinds.contains(&"scenario"));
        assert!(kinds.contains(&"symbol"));
        assert!(kinds.contains(&"gate"));
        // The verifies/evidences rows are observable in the manifest.
        let relations = document["relations"].as_array().expect("relations");
        assert_eq!(relations.len(), 4);
        assert!(relations
            .iter()
            .any(|relation| relation["relationKind"] == json!("verifies")
                && relation["toNode"] == json!("scenario:planner.scenario.focus_happy")));
        assert!(relations
            .iter()
            .any(|relation| relation["relationKind"] == json!("evidences")));
        // Production custody: the established parser accepts the bytes.
        let bytes = serde_json::to_vec_pretty(&document).expect("serialize");
        let parsed = crate::trace::TraceManifest::parse(&bytes).expect("parse-valid manifest");
        assert_eq!(parsed.report().relation_count, 4);
        // The closed query answers through the same document.
        assert!(parsed.manifest().relations.len() == 4);
    }

    #[test]
    fn trace_manifest_document_requires_records() {
        let context = TraceContext::new("3".repeat(64));
        assert!(trace_manifest_document(&[], &context).is_err());
    }
}
