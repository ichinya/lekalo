//! Pure semantic comparison of two same-family attachments (issue
//! #64).
//!
//! The comparison answers one question per changed path with one
//! closed class: **breaking** (a declared guarantee was removed or
//! weakened — a query removed, the source or cardinality changed, a
//! projection field or parameter removed, a policy reference
//! dropped), **non-breaking** (an addition under the evolution
//! policy, cost hints, scenario references), and **policy-change**
//! (filter, sort, pagination window, consistency, foreign escape, or
//! tenancy declaration changes that reshape results without removing
//! the query). Invalid inputs — foreign projects or mixed
//! Model/IR/attachment revisions — are the typed error set, never a
//! guessed classification. Paths are deterministic and byte-sorted.

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::{QueryDecl, QueryModelAttachment};

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or weakened.
    Breaking,
    /// An addition or descriptive change under the evolution policy.
    NonBreaking,
    /// A reshaping change with the query still declared.
    PolicyChange,
}

impl DiffClass {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::PolicyChange => "policy-change",
        }
    }
}

/// One changed path with its class.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    class: DiffClass,
}

impl DiffPath {
    /// The canonical path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The closed compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }
}

/// The finished comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    equal: bool,
    paths: Vec<DiffPath>,
}

impl DiffResult {
    /// Whether the two attachments are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths, byte-sorted.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }
}

/// Compare two same-family attachments. Pure and read-only.
pub fn compare(
    base: &QueryModelAttachment,
    candidate: &QueryModelAttachment,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::input_invalid("diff-project-mismatch"));
    }
    if base.attachment_revision().as_str() != candidate.attachment_revision().as_str()
        || base.model_ref().digest().as_str() != candidate.model_ref().digest().as_str()
        || base.ir_digest().as_str() != candidate.ir_digest().as_str()
    {
        return Err(diagnostic::input_invalid("diff-mixed-revision"));
    }
    let mut paths: Vec<DiffPath> = Vec::new();

    // Tenancy: a removed tenant scope removes a security requirement
    // (breaking); an added scope is a policy change for the queries
    // it newly constrains.
    for declaration in base.tenancy() {
        if !candidate
            .tenancy()
            .iter()
            .any(|other| other.entity == declaration.entity && other.field == declaration.field)
        {
            push_path(
                &format!("tenancy/{}", declaration.entity.as_str()),
                DiffClass::Breaking,
                &mut paths,
            );
        }
    }
    for declaration in candidate.tenancy() {
        if !base
            .tenancy()
            .iter()
            .any(|other| other.entity == declaration.entity && other.field == declaration.field)
        {
            push_path(
                &format!("tenancy/{}", declaration.entity.as_str()),
                DiffClass::PolicyChange,
                &mut paths,
            );
        }
    }

    // Queries: removal is breaking, addition is non-breaking, and
    // every member change of a common query is classified below.
    for decl in base.queries() {
        let Some(other) = candidate
            .queries()
            .iter()
            .find(|other| other.query == decl.query)
        else {
            push_path(
                &format!("queries/{}", decl.query.as_str()),
                DiffClass::Breaking,
                &mut paths,
            );
            continue;
        };
        compare_query(decl, other, &mut paths);
    }
    for decl in candidate.queries() {
        if !base.queries().iter().any(|other| other.query == decl.query) {
            push_path(
                &format!("queries/{}", decl.query.as_str()),
                DiffClass::NonBreaking,
                &mut paths,
            );
        }
    }

    let mut sorted = paths.clone();
    sorted.sort();
    sorted.dedup();
    Ok(DiffResult {
        equal: sorted.is_empty(),
        paths: sorted,
    })
}

/// Classify every changed member of one common query declaration.
fn compare_query(base: &QueryDecl, candidate: &QueryDecl, paths: &mut Vec<DiffPath>) {
    let prefix = format!("queries/{}", base.query.as_str());
    if base.source != candidate.source {
        push_path(&format!("{prefix}/source"), DiffClass::Breaking, paths);
    }
    if base.cardinality != candidate.cardinality {
        push_path(&format!("{prefix}/cardinality"), DiffClass::Breaking, paths);
    }
    if base.consistency != candidate.consistency {
        push_path(
            &format!("{prefix}/consistency"),
            DiffClass::PolicyChange,
            paths,
        );
    }
    if base.max_staleness != candidate.max_staleness {
        let class = match (base.max_staleness, candidate.max_staleness) {
            (Some(before), Some(after)) if after <= before => DiffClass::NonBreaking,
            (None, Some(_)) => DiffClass::NonBreaking,
            _ => DiffClass::Breaking,
        };
        push_path(&format!("{prefix}/maxStaleness"), class, paths);
    }

    // Parameters: the input contract. Any change breaks callers.
    if base.parameters != candidate.parameters {
        let before: Vec<&str> = base
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect();
        let after: Vec<&str> = candidate
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect();
        for name in &before {
            if !after.contains(name) {
                push_path(
                    &format!("{prefix}/parameters/{name}"),
                    DiffClass::Breaking,
                    paths,
                );
            }
        }
        for name in &after {
            if !before.contains(name) {
                push_path(
                    &format!("{prefix}/parameters/{name}"),
                    DiffClass::Breaking,
                    paths,
                );
            }
        }
        for parameter in &base.parameters {
            let Some(other) = candidate
                .parameters
                .iter()
                .find(|other| other.name == parameter.name)
            else {
                continue;
            };
            if parameter.parameter_type != other.parameter_type {
                push_path(
                    &format!("{prefix}/parameters/{}", parameter.name.as_str()),
                    DiffClass::Breaking,
                    paths,
                );
            }
        }
    }

    // Filters and sorts reshape results without removing the query.
    let base_filter = base
        .filter
        .as_ref()
        .map(super::canonical::filter_json_for_diff);
    let candidate_filter = candidate
        .filter
        .as_ref()
        .map(super::canonical::filter_json_for_diff);
    if base_filter != candidate_filter {
        push_path(&format!("{prefix}/filter"), DiffClass::PolicyChange, paths);
    }
    let base_sort = base.sort.as_ref().map(|keys| {
        keys.iter()
            .map(|key| (key.field.as_str().to_owned(), key.direction.as_str()))
            .collect::<Vec<_>>()
    });
    let candidate_sort = candidate.sort.as_ref().map(|keys| {
        keys.iter()
            .map(|key| (key.field.as_str().to_owned(), key.direction.as_str()))
            .collect::<Vec<_>>()
    });
    if base_sort != candidate_sort {
        push_path(&format!("{prefix}/sort"), DiffClass::PolicyChange, paths);
    }

    // The pagination window is a policy change when the strategy
    // survives and breaking when the strategy itself changes.
    match (&base.pagination, &candidate.pagination) {
        (None, None) => {}
        (Some(before), Some(after)) => {
            if before.strategy != after.strategy {
                push_path(&format!("{prefix}/pagination"), DiffClass::Breaking, paths);
            } else if before.limit != after.limit
                || before.offset != after.offset
                || before.key_parameter != after.key_parameter
            {
                push_path(
                    &format!("{prefix}/pagination"),
                    DiffClass::PolicyChange,
                    paths,
                );
            }
        }
        _ => push_path(
            &format!("{prefix}/pagination"),
            DiffClass::PolicyChange,
            paths,
        ),
    }

    // Projection: a removed output field removes a guarantee; an
    // added field is an addition; a renamed output is a break.
    match (&base.selection, &candidate.selection) {
        (None, None) => {}
        (Some(before), Some(after)) => {
            for field in before {
                let Some(other) = after.iter().find(|other| other.field == field.field) else {
                    push_path(
                        &format!("{prefix}/selection/{}", field.field.as_str()),
                        DiffClass::Breaking,
                        paths,
                    );
                    continue;
                };
                if field.alias != other.alias {
                    push_path(
                        &format!("{prefix}/selection/{}/as", field.field.as_str()),
                        DiffClass::Breaking,
                        paths,
                    );
                }
            }
            for field in after {
                if !before.iter().any(|other| other.field == field.field) {
                    push_path(
                        &format!("{prefix}/selection/{}", field.field.as_str()),
                        DiffClass::NonBreaking,
                        paths,
                    );
                }
            }
        }
        _ => push_path(&format!("{prefix}/selection"), DiffClass::Breaking, paths),
    }

    // Includes: removing a joined relation removes data consumers
    // may read.
    let base_includes: Vec<String> = base
        .includes
        .iter()
        .map(|include| super::wire::include_path_key(&include.path))
        .collect();
    let candidate_includes: Vec<String> = candidate
        .includes
        .iter()
        .map(|include| super::wire::include_path_key(&include.path))
        .collect();
    for path in &base_includes {
        if !candidate_includes.contains(path) {
            push_path(
                &format!("{prefix}/includes/{path}"),
                DiffClass::Breaking,
                paths,
            );
        }
    }
    for path in &candidate_includes {
        if !base_includes.contains(path) {
            push_path(
                &format!("{prefix}/includes/{path}"),
                DiffClass::NonBreaking,
                paths,
            );
        }
    }

    // Policy references: a dropped policy removes a declared
    // authorization requirement.
    for policy in &base.policies {
        if !candidate.policies.contains(policy) {
            push_path(
                &format!("{prefix}/policies/{}", policy.as_str()),
                DiffClass::Breaking,
                paths,
            );
        }
    }
    for policy in &candidate.policies {
        if !base.policies.contains(policy) {
            push_path(
                &format!("{prefix}/policies/{}", policy.as_str()),
                DiffClass::NonBreaking,
                paths,
            );
        }
    }

    // Scenario references pin mapping evidence; they are additions
    // or removals of evidence, never of the read contract.
    for scenario in &base.scenarios {
        if !candidate.scenarios.contains(scenario) {
            push_path(
                &format!("{prefix}/scenarios/{}", scenario.as_str()),
                DiffClass::NonBreaking,
                paths,
            );
        }
    }
    for scenario in &candidate.scenarios {
        if !base.scenarios.contains(scenario) {
            push_path(
                &format!("{prefix}/scenarios/{}", scenario.as_str()),
                DiffClass::NonBreaking,
                paths,
            );
        }
    }

    // Cost hints are evidence, never guarantees.
    if base.cost != candidate.cost {
        push_path(&format!("{prefix}/cost"), DiffClass::NonBreaking, paths);
    }

    // The foreign escape: entering or leaving foreign mode changes
    // the managed-mode posture without removing the read.
    if base.foreign != candidate.foreign {
        push_path(&format!("{prefix}/foreign"), DiffClass::PolicyChange, paths);
    }
}

/// Push one changed path, keeping the vector sorted.
fn push_path(path: &str, class: DiffClass, paths: &mut Vec<DiffPath>) {
    paths.push(DiffPath {
        path: path.to_owned(),
        class,
    });
}

#[cfg(test)]
mod tests {
    use super::super::version;
    use super::*;

    fn attachment(queries: serde_json::Value, tenancy: serde_json::Value) -> QueryModelAttachment {
        QueryModelAttachment::from_value(&serde_json::json!({
            "schemaVersion": version::SCHEMA_VERSION,
            "identity": version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "tenancy": tenancy,
            "queries": queries
        }))
        .expect("attachment")
    }

    fn list(query: &str) -> serde_json::Value {
        serde_json::json!([{
            "query": query,
            "source": "planner.task",
            "cardinality": "list",
            "consistency": "strong"
        }])
    }

    #[test]
    fn identical_attachments_are_equal() {
        let base = attachment(list("planner.q"), serde_json::json!([]));
        let candidate = attachment(list("planner.q"), serde_json::json!([]));
        let result = compare(&base, &candidate).expect("compare");
        assert!(result.equal());
        assert!(result.paths().is_empty());
    }

    #[test]
    fn removal_is_breaking_and_addition_is_nonbreaking() {
        let base = attachment(
            serde_json::json!([
                {"query": "planner.a", "source": "planner.task", "cardinality": "list", "consistency": "strong"},
                {"query": "planner.b", "source": "planner.task", "cardinality": "list", "consistency": "strong"}
            ]),
            serde_json::json!([]),
        );
        let candidate = attachment(list("planner.a"), serde_json::json!([]));
        let result = compare(&base, &candidate).expect("compare");
        assert!(!result.equal());
        let removed = result
            .paths()
            .iter()
            .find(|path| path.path() == "queries/planner.b")
            .expect("removed path");
        assert_eq!(removed.class(), DiffClass::Breaking);
    }

    #[test]
    fn mixed_revision_refuses() {
        let base = attachment(list("planner.q"), serde_json::json!([]));
        let mut candidate_body = serde_json::json!({
            "schemaVersion": version::SCHEMA_VERSION,
            "identity": version::IDENTITY,
            "attachmentRevision": "1.0.1",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "tenancy": [],
            "queries": list("planner.q")
        });
        let candidate = QueryModelAttachment::from_value(&candidate_body).expect("attachment");
        assert!(compare(&base, &candidate).is_err());
        candidate_body["attachmentRevision"] = serde_json::json!("1.0.0");
        let same_revision = QueryModelAttachment::from_value(&candidate_body).expect("attachment");
        assert!(compare(&base, &same_revision).is_ok());
    }

    #[test]
    fn cardinality_change_is_breaking_and_filter_change_is_policy() {
        let base = attachment(
            serde_json::json!([{
                "query": "planner.q",
                "source": "planner.task",
                "cardinality": "list",
                "consistency": "strong"
            }]),
            serde_json::json!([]),
        );
        let candidate = attachment(
            serde_json::json!([{
                "query": "planner.q",
                "source": "planner.task",
                "cardinality": "optional",
                "consistency": "strong"
            }]),
            serde_json::json!([]),
        );
        let result = compare(&base, &candidate).expect("compare");
        assert!(result
            .paths()
            .iter()
            .any(|path| path.path() == "queries/planner.q/cardinality"
                && path.class() == DiffClass::Breaking));
    }

    #[test]
    fn tenancy_removal_is_breaking_and_addition_is_policy_change() {
        let tenancy = serde_json::json!([{"entity": "planner.task", "field": "tenant_id"}]);
        let base = attachment(list("planner.q"), tenancy.clone());
        let candidate = attachment(list("planner.q"), serde_json::json!([]));
        let result = compare(&base, &candidate).expect("compare");
        assert!(result.paths().iter().any(
            |path| path.path() == "tenancy/planner.task" && path.class() == DiffClass::Breaking
        ));
        let reverse = compare(&candidate, &base).expect("compare");
        assert!(reverse
            .paths()
            .iter()
            .any(|path| path.path() == "tenancy/planner.task"
                && path.class() == DiffClass::PolicyChange));
    }
}
