//! The pointer-level compatibility view of the OpenAPI projection
//! (issue #46, plan §5).
//!
//! Breaking wire changes are classified **once**, at the semantic
//! level, by `transport_http::compare` (`DiffClass::{Breaking,
//! NonBreaking, PolicyChange}`); this module adds **no second
//! taxonomy**. Each transport diff path is mapped onto the OpenAPI
//! document locations it touches (`endpoints/<id>/params` →
//! `/paths/<template>/<method>/parameters`, `securitySchemes/<id>` →
//! `/components/securitySchemes/<id>`, …), so a consumer reading only
//! documents sees the same classification a `wire-consumer` sees.
//! A rendered-only difference is impossible by construction: the
//! deterministic naming means `compare` reporting equal implies
//! byte-identical renders, and the suite asserts it.

use serde_json::json;

use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, Definition};
use crate::transport_http::{compare, DiffClass, DiffPath, DiffResult, TransportDocument};

use super::id::{escape_pointer, paths_pointer};
use super::types::DocumentVersion;

/// One changed transport path with its class and the OpenAPI pointer
/// locations it touches.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DocumentDiffPath {
    /// The transport diff path (`endpoints/<id>/params`).
    path: String,
    /// The reused transport compatibility class.
    class: DiffClass,
    /// The OpenAPI JSON pointers the change touches, byte-sorted.
    pointers: Vec<String>,
}

impl DocumentDiffPath {
    /// The transport diff path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The closed compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }

    /// The OpenAPI pointers the change touches.
    pub fn pointers(&self) -> &[String] {
        &self.pointers
    }
}

/// The finished document-level comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentDiffResult {
    equal: bool,
    paths: Vec<DocumentDiffPath>,
    wire_consumer_blocked: bool,
}

impl DocumentDiffResult {
    /// Whether the two attachments are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths with their pointer views, byte-sorted.
    pub fn paths(&self) -> &[DocumentDiffPath] {
        &self.paths
    }

    /// Whether the strict `wire-consumer` profile blocks this diff.
    pub fn wire_consumer_blocked(&self) -> bool {
        self.wire_consumer_blocked
    }
}

/// Compare two same-family attachments and map every changed path
/// onto the OpenAPI locations it touches. The compiled project is the
/// shared Model pin (compare refuses mixed pins), so one project
/// resolves the method and path template of every endpoint on both
/// sides. Pure and read-only.
pub fn compare_documents(
    base: &TransportDocument,
    candidate: &TransportDocument,
    project: &CompiledProject,
    version: DocumentVersion,
) -> Result<DocumentDiffResult, DiagnosticSet> {
    let _ = version;
    let semantic = compare(base, candidate)?;
    let mut paths: Vec<DocumentDiffPath> = semantic
        .paths()
        .iter()
        .map(|path| map_path(path, project))
        .collect();
    paths.sort();
    Ok(DocumentDiffResult {
        equal: semantic.equal(),
        wire_consumer_blocked: semantic.wire_consumer_blocked(),
        paths,
    })
}

/// The pointer view of one transport diff path.
fn map_path(path: &DiffPath, project: &CompiledProject) -> DocumentDiffPath {
    let tokens: Vec<&str> = path.path().split('/').collect();
    let pointers = match tokens.as_slice() {
        // Document defaults shape every operation's declared header
        // parameters: the affected locations are every operation's
        // parameter list.
        ["defaults", _] => operation_parameter_pointers(project),
        // A scheme change touches the shared scheme component and
        // every operation that requires it (the requirement object).
        ["securitySchemes", id] => vec![format!(
            "/components/securitySchemes/{}",
            escape_pointer(id)
        )],
        // Endpoint removal: the whole path item disappears.
        ["endpoints", endpoint] => endpoint_operation_pointers(project, endpoint),
        // A member change maps by member to its document location(s).
        ["endpoints", endpoint, member] => member_pointers(project, endpoint, member),
        // Anything unrecognized (defensive) maps to the document root.
        _ => vec![String::new()],
    };
    DocumentDiffPath {
        path: path.path().to_owned(),
        class: path.class(),
        pointers,
    }
}

/// The `(template, method)` of one Model endpoint symbol.
fn endpoint_operation(project: &CompiledProject, endpoint: &str) -> Option<(String, String)> {
    project
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::Endpoint(def) if def.id.as_str() == endpoint => Some((
                def.path.as_str().to_owned(),
                def.method.as_str().to_ascii_lowercase(),
            )),
            _ => None,
        })
}

/// Every endpoint's operation parameter pointer, byte-sorted.
fn operation_parameter_pointers(project: &CompiledProject) -> Vec<String> {
    let mut pointers: Vec<String> = project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Endpoint(def) => {
                endpoint_operation(project, def.id.as_str()).map(|(template, method)| {
                    format!("{}{}", paths_pointer(&template, &method), "/parameters")
                })
            }
            _ => None,
        })
        .collect();
    pointers.sort();
    pointers.dedup();
    pointers
}

/// The operation pointer(s) of one endpoint symbol.
fn endpoint_operation_pointers(project: &CompiledProject, endpoint: &str) -> Vec<String> {
    match endpoint_operation(project, endpoint) {
        Some((template, method)) => vec![paths_pointer(&template, &method)],
        None => vec![String::new()],
    }
}

/// The pointer view of one endpoint member change.
fn member_pointers(project: &CompiledProject, endpoint: &str, member: &str) -> Vec<String> {
    let Some((template, method)) = endpoint_operation(project, endpoint) else {
        return vec![String::new()];
    };
    let operation = paths_pointer(&template, &method);
    let pointer = |suffix: &str| format!("{operation}{suffix}");
    match member {
        "params" | "idempotency" | "correlation" => vec![pointer("/parameters")],
        "body" => vec![pointer("/requestBody")],
        "success" | "errors" => vec![pointer("/responses")],
        "errorDefaults" => vec![pointer("/responses"), "/components/responses".to_owned()],
        "security" => vec![pointer("/security"), pointer("/x-lekalo-scheme")],
        "pagination" => vec![pointer("/parameters"), pointer("/responses")],
        "rateLimit" => vec![pointer("/x-lekalo-rate-limit")],
        "cache" => vec![pointer("/x-lekalo-cache")],
        "apiVersion" => vec![pointer("/parameters"), pointer("/x-lekalo-api-version")],
        "capabilities" => vec![pointer("/x-lekalo-capabilities")],
        "tags" => vec![pointer("/tags")],
        "summary" => vec![pointer("/summary")],
        // Scenario coverage does not render natively; the operation
        // itself is the affected location.
        _ => vec![operation],
    }
}

/// The canonical JSON view of one diff (the CLI envelope payload).
pub fn diff_json(result: &DocumentDiffResult) -> String {
    let count = |class| {
        result
            .paths()
            .iter()
            .filter(|path| path.class() == class)
            .count()
    };
    let breaking = count(DiffClass::Breaking);
    let non_breaking = count(DiffClass::NonBreaking);
    let policy_change = count(DiffClass::PolicyChange);
    let paths: Vec<String> = result
        .paths()
        .iter()
        .map(|path| {
            json!({
                "path": path.path(),
                "class": path.class().key(),
                "pointers": path.pointers(),
            })
            .to_string()
        })
        .collect();
    format!(
        "{{\"equal\":{},\"breaking\":{},\"nonBreaking\":{},\"policyChange\":{},\"wireConsumerBlocked\":{},\"paths\":[{}]}}",
        result.equal(),
        breaking,
        non_breaking,
        policy_change,
        result.wire_consumer_blocked(),
        paths.join(","),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_views_map_to_their_document_locations() {
        let project = crate::ir::CompiledProject {
            model_version: crate::loader::ModelVersion::Current,
            project: None,
            modules: Vec::new(),
            definitions: vec![
                crate::ir::Definition::Endpoint(crate::ir::EndpointDef {
                    id: crate::ir::SymbolId::of("planner.endpoint_focus_task"),
                    common: test_common(),
                    invokes: crate::ir::SymbolId::of("planner.focus_task"),
                    method: crate::ir::HttpMethod::Post,
                    path: crate::ir::EndpointPath::of("/tasks/{task_id}/focus"),
                }),
                crate::ir::Definition::Scalar(crate::ir::ScalarDef {
                    id: crate::ir::SymbolId::of("planner.task_id"),
                    common: test_common(),
                    base: crate::ir::ScalarBase::Uuid,
                }),
            ],
        };
        // A param change maps to the operation's parameter list.
        let path = DiffPath::test_of(
            "endpoints/planner.endpoint_focus_task/params",
            DiffClass::Breaking,
        );
        let view = map_path(&path, &project);
        assert_eq!(
            view.pointers(),
            ["/paths/~1tasks~1{task_id}~1focus/post/parameters"]
        );
        // An error map change maps to the responses.
        let path = DiffPath::test_of(
            "endpoints/planner.endpoint_focus_task/errors",
            DiffClass::PolicyChange,
        );
        let view = map_path(&path, &project);
        assert_eq!(
            view.pointers(),
            ["/paths/~1tasks~1{task_id}~1focus/post/responses"]
        );
        // An unknown endpoint falls open rather than inventing a
        // pointer.
        let path = DiffPath::test_of("endpoints/planner.ghost", DiffClass::Breaking);
        let view = map_path(&path, &project);
        assert_eq!(view.pointers(), [""]);
    }

    fn test_common() -> crate::ir::Common {
        crate::ir::Common {
            version: 1,
            description: None,
            derived_from: Vec::new(),
            visibility: Some(crate::ir::Visibility::Project),
            portability: Some(crate::ir::Portability::Portable),
            renamed_from: Vec::new(),
        }
    }
}
