//! Greenfield bootstrap: `lekalo init` without `--adopt` and
//! `lekalo module new` (issue #97).
//!
//! Bootstrap creates the minimal canonical greenfield skeleton — the
//! project document, the first empty module, the optional target
//! selection, the `.gitignore` addition for the derived `.lekalo/`
//! tree, and the opt-in editor/schema hints — and nothing else: no
//! application code, no package or tool installation, no adapter
//! execution, no lock (the lock appears only after explicit
//! resolution through `lekalo lock`).
//!
//! Every wizard decision that has a contract home is a non-interactive
//! flag (project id, model frontend, initial module, target and
//! adapter profile); the decisions without one (ownership mode,
//! integration evidence, strictness, adapter availability) stay
//! per-invocation choices of the commands that own them and are never
//! persisted as unpublished fields.
//!
//! Writes share the adoption semantics exactly: the atomic no-clobber
//! writer with journal and rollback, identical-bytes paths skip, any
//! other existing path denies the whole bootstrap, and the applied
//! skeleton must pass the normal load-and-validate gate — a gate
//! failure rolls every genuine mutation back first.

use std::path::{Path, PathBuf};

use super::receipt::{
    BootstrapReceipt, Gate, ModuleReceipt, ProjectIdProvenance, TemplateRecord, WriteEntry,
};
use super::{diagnostic, plan, structure_failure, valid_project_id, Status};

use crate::result::DomainResult;

/// The canonical model frontend the generated documents are written in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Frontend {
    /// Human-friendly block YAML in the fixed `.yaml` homes.
    Yaml,
    /// Compact JSON bytes in the same homes (adoption spelling).
    Json,
}

impl Frontend {
    /// The closed wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Json => "json",
        }
    }
}

/// The template identity every generated artifact records in its
/// receipt: the bootstrap templates are in-code and version with the
/// product, so the product version is their honest version.
const TEMPLATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The user-owned ignore file bootstrap adds the derived tree to.
const GITIGNORE_PATH: &str = ".gitignore";

/// The managed ignore line for the derived Lekalo tree. The tree is
/// committed nowhere: `lekalo/**` and `lekalo.lock` stay tracked.
const GITIGNORE_LINE: &[u8] = b"/.lekalo/\n";

/// The opt-in editor/schema hint home.
const EDITOR_HINTS_PATH: &str = ".vscode/settings.json";

/// One greenfield `lekalo init` request.
#[derive(Clone, Debug)]
pub struct BootstrapRequest {
    /// Explicit bootstrap-root selector (`--project`/`LEKALO_PROJECT`).
    pub project: Option<String>,
    /// Explicit canonical project id (`--project-id`).
    pub project_id: Option<String>,
    /// The semantic id of the first module (`--module`).
    pub module: String,
    /// The canonical model frontend of the generated documents.
    pub frontend: Frontend,
    /// Explicit target selection (`--target`).
    pub target: Option<String>,
    /// Explicit adapter profile selection (`--profile`; requires
    /// `--target`).
    pub profile: Option<String>,
    /// Write the opt-in editor/schema hints (`--editor-hints`).
    pub editor_hints: bool,
    /// Preview the plan without writing (`--dry-run`).
    pub dry_run: bool,
}

/// One `lekalo module new` request.
#[derive(Clone, Debug)]
pub struct ModuleNewRequest {
    /// Project root selector, resolved through the normal marker
    /// discovery (`--project`/`LEKALO_PROJECT`).
    pub project: Option<String>,
    /// The semantic id (and directory name) of the new module.
    pub id: String,
    /// The canonical model frontend of the generated document.
    pub frontend: Frontend,
    /// Preview the plan without writing (`--dry-run`).
    pub dry_run: bool,
}

/// The minimal `lekalo/project.yaml` of a greenfield project.
///
/// Fails closed: a project id outside the closed one-segment grammar
/// yields `None` instead of a guessed or interpolated document.
fn project_document(project_id: &str, frontend: Frontend) -> Option<plan::PlannedFile> {
    if !valid_project_id(project_id) {
        return None;
    }
    let bytes = match frontend {
        Frontend::Json => format!(
            "{{\"schema_version\":\"1.0.0\",\"definitions\":[{{\"id\":\"{project_id}\",\"kind\":\"project\",\"version\":1,\"description\":\"New Lekalo project.\"}}]}}\n"
        ),
        Frontend::Yaml => format!(
            "schema_version: \"1.0.0\"\ndefinitions:\n  - id: {project_id}\n    kind: project\n    version: 1\n    description: \"New Lekalo project.\"\n"
        ),
    }
    .into_bytes();
    Some(plan::PlannedFile {
        path: plan::PROJECT_PATH.to_owned(),
        bytes,
    })
}

/// The minimal `lekalo/modules/<id>/module.yaml`: the first (or an
/// additional) empty module — the module manifest and nothing else.
///
/// Fails closed on the closed one-segment module-id grammar, which is
/// also a legal single path segment, so the write stays inside the
/// module home.
fn module_document(module_id: &str, frontend: Frontend) -> Option<plan::PlannedFile> {
    if !valid_project_id(module_id) {
        return None;
    }
    let bytes = match frontend {
        Frontend::Json => format!(
            "{{\"schema_version\":\"1.0.0\",\"definitions\":[{{\"id\":\"{module_id}\",\"kind\":\"module\",\"version\":1,\"description\":\"Initial module.\"}}]}}\n"
        ),
        Frontend::Yaml => format!(
            "schema_version: \"1.0.0\"\ndefinitions:\n  - id: {module_id}\n    kind: module\n    version: 1\n    description: \"Initial module.\"\n"
        ),
    }
    .into_bytes();
    Some(plan::PlannedFile {
        path: format!("lekalo/modules/{module_id}/module.yaml"),
        bytes,
    })
}

/// The opt-in `.vscode/settings.json`: editor/schema hints mapping the
/// canonical model homes to the published Model 1.0.0 schema `$id`.
fn editor_hints_document() -> plan::PlannedFile {
    plan::PlannedFile {
        path: EDITOR_HINTS_PATH.to_owned(),
        bytes: concat!(
            "{\n",
            "  \"yaml.schemas\": {\n",
            "    \"https://lekalo.dev/schemas/model/1.0.0/schema.json\": [\n",
            "      \"lekalo/project.yaml\",\n",
            "      \"lekalo/modules/**/*.yaml\"\n",
            "    ]\n",
            "  }\n",
            "}\n"
        )
        .as_bytes()
        .to_vec(),
    }
}

/// Whether the observed ignore bytes already ignore the derived tree.
///
/// Both rooted (`/.lekalo/`) and unrooted (`.lekalo/`) line spellings
/// count, so a user-authored ignore is never duplicated.
fn ignores_lekalo(existing: &[u8]) -> bool {
    let text = String::from_utf8_lossy(existing);
    text.lines()
        .any(|line| line == "/.lekalo/" || line == ".lekalo/")
}

/// The exact suffix bootstrap appends to an observed ignore file that
/// does not yet ignore the derived tree: one newline separation when
/// the file does not end in one, then the managed line.
fn gitignore_suffix(existing: &[u8]) -> Vec<u8> {
    let mut suffix = Vec::with_capacity(GITIGNORE_LINE.len() + 1);
    if !existing.is_empty() && !existing.ends_with(b"\n") {
        suffix.push(b'\n');
    }
    suffix.extend_from_slice(GITIGNORE_LINE);
    suffix
}

/// The template records of the artifacts one bootstrap plans.
fn templates(request: &BootstrapRequest) -> Vec<TemplateRecord> {
    let mut records = vec![
        TemplateRecord::project(TEMPLATE_VERSION),
        TemplateRecord::module(TEMPLATE_VERSION),
    ];
    if request.target.is_some() {
        records.push(TemplateRecord::target(TEMPLATE_VERSION));
    }
    records.push(TemplateRecord::gitignore(TEMPLATE_VERSION));
    if request.editor_hints {
        records.push(TemplateRecord::editor_hints(TEMPLATE_VERSION));
    }
    records
}

/// The SHA-256 helper of the versioning plan surface.
fn sha256_hex(bytes: &[u8]) -> String {
    crate::versioning::plan::sha256_hex(bytes)
}

/// The classified preflight of one bootstrap.
struct Preflight {
    creates: Vec<plan::PlannedFile>,
    append: Option<plan::PlannedAppend>,
    conflicts: Vec<String>,
    unchanged: usize,
    writes: Vec<WriteEntry>,
}

/// Classify one planned create: `create`, `unchanged`, or `conflict`.
fn classify_create(
    root: &Path,
    file: &plan::PlannedFile,
    creates: &mut Vec<plan::PlannedFile>,
    conflicts: &mut Vec<String>,
    unchanged: &mut usize,
    writes: &mut Vec<WriteEntry>,
) {
    let disposition = match plan::observe(root, &file.path) {
        None => {
            creates.push(plan::PlannedFile {
                path: file.path.clone(),
                bytes: file.bytes.clone(),
            });
            "create"
        }
        Some(bytes) if bytes == file.bytes => {
            *unchanged += 1;
            "unchanged"
        }
        Some(_) => {
            conflicts.push(file.path.clone());
            "conflict"
        }
    };
    writes.push(WriteEntry {
        path: file.path.clone(),
        action: "create",
        sha256: sha256_hex(&file.bytes),
        disposition,
    });
}

/// Classify the `.gitignore` artifact: `create` when absent, `append`
/// when present without the managed line, `unchanged` when the derived
/// tree is already ignored.
fn classify_gitignore(
    root: &Path,
    creates: &mut Vec<plan::PlannedFile>,
    appends: &mut Vec<plan::PlannedAppend>,
    unchanged: &mut usize,
    writes: &mut Vec<WriteEntry>,
) {
    match plan::observe(root, GITIGNORE_PATH) {
        None => {
            creates.push(plan::PlannedFile {
                path: GITIGNORE_PATH.to_owned(),
                bytes: GITIGNORE_LINE.to_vec(),
            });
            writes.push(WriteEntry {
                path: GITIGNORE_PATH.to_owned(),
                action: "create",
                sha256: sha256_hex(GITIGNORE_LINE),
                disposition: "create",
            });
        }
        Some(bytes) if ignores_lekalo(&bytes) => {
            *unchanged += 1;
            writes.push(WriteEntry {
                path: GITIGNORE_PATH.to_owned(),
                action: "create",
                sha256: sha256_hex(GITIGNORE_LINE),
                disposition: "unchanged",
            });
        }
        Some(bytes) => {
            let suffix = gitignore_suffix(&bytes);
            writes.push(WriteEntry {
                path: GITIGNORE_PATH.to_owned(),
                action: "append",
                sha256: sha256_hex(&suffix),
                disposition: "append",
            });
            appends.push(plan::PlannedAppend {
                path: GITIGNORE_PATH.to_owned(),
                suffix,
            });
        }
    }
}

/// Run the greenfield preflight over the built plan.
fn preflight(root: &Path, files: &[plan::PlannedFile], gitignore: bool) -> Preflight {
    let mut state = Preflight {
        creates: Vec::new(),
        append: None,
        conflicts: Vec::new(),
        unchanged: 0,
        writes: Vec::new(),
    };
    for file in files {
        classify_create(
            root,
            file,
            &mut state.creates,
            &mut state.conflicts,
            &mut state.unchanged,
            &mut state.writes,
        );
    }
    if gitignore {
        let mut appends = Vec::new();
        classify_gitignore(
            root,
            &mut state.creates,
            &mut appends,
            &mut state.unchanged,
            &mut state.writes,
        );
        state.append = appends.into_iter().next();
    }
    state
}

/// Resolve the greenfield bootstrap root: an explicit selector wins,
/// otherwise the invocation directory itself. Greenfield never walks
/// the ancestors for workspace or manifest evidence — that discovery
/// belongs to adoption; a new project is created exactly where the
/// user stands.
fn resolve_root(selector: Option<&str>) -> Result<(PathBuf, &'static str), DomainResult> {
    let explicit = selector
        .map(str::to_owned)
        .or_else(|| std::env::var("LEKALO_PROJECT").ok());
    if let Some(selector) = explicit.as_deref() {
        if let Some(code) = crate::project_fs::selection_violation(selector) {
            return Err(crate::loader::diagnostic::failure(
                Status::Invalid,
                vec![crate::loader::error::Diagnostic::new(code)],
            ));
        }
        return match crate::project_fs::Fs::check_selection(
            selector,
            "structure.project-not-directory",
        ) {
            Ok(path) => Ok((path, "explicit")),
            Err(outcome) => Err(structure_failure(outcome)),
        };
    }
    match std::env::current_dir() {
        Ok(cwd) => Ok((cwd, "invocation-directory")),
        Err(_) => Err(crate::loader::diagnostic::failure(
            Status::Invalid,
            vec![crate::loader::error::Diagnostic::new(
                "structure.root-unreadable",
            )],
        )),
    }
}

/// Derive the greenfield project id: the explicit flag, else the
/// bootstrap root's directory name. No manifest is read — greenfield
/// owns no detection.
fn derive_project_id(
    root: &Path,
    explicit: Option<&str>,
) -> Result<super::ProjectId, &'static str> {
    if let Some(explicit) = explicit {
        return match super::sanitize_id(explicit) {
            Some(id) if id == explicit => Ok(super::ProjectId {
                id,
                provenance: ProjectIdProvenance::explicit(),
            }),
            _ => Err("invalid-project-id"),
        };
    }
    let directory = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(id) = super::sanitize_id(&directory) {
        return Ok(super::ProjectId {
            id,
            provenance: ProjectIdProvenance::directory_name(directory),
        });
    }
    Err("derive-project-id")
}

/// Render a bootstrap receipt as the success envelope.
fn receipt(envelope: BootstrapReceipt) -> DomainResult {
    let human = envelope.human_summary();
    let json = serde_json::to_string_pretty(&envelope).expect("receipt serializes");
    DomainResult::receipt(json, human)
}

/// Render a module receipt as the success envelope.
fn module_receipt(envelope: ModuleReceipt) -> DomainResult {
    let human = envelope.human_summary();
    let json = serde_json::to_string_pretty(&envelope).expect("receipt serializes");
    DomainResult::receipt(json, human)
}

/// The registered `cli.usage` refusal of a request outside its closed
/// grammar, shared by both entry points.
fn usage_refusal() -> DomainResult {
    DomainResult::usage_error()
}

/// Run greenfield `lekalo init` to completion.
pub fn bootstrap(request: &BootstrapRequest) -> DomainResult {
    // 0. Request grammar at the public core boundary, before any root
    // resolution, read, or write. The wire never echoes the rejected
    // value.
    if !valid_project_id(&request.module) {
        return usage_refusal();
    }
    if let Some(target) = request.target.as_deref() {
        if !super::detect::valid_target_id(target) {
            return usage_refusal();
        }
    }
    if let Some(profile) = request.profile.as_deref() {
        if request.target.is_none() || !crate::target_protocol::scopes::is_token(profile) {
            return usage_refusal();
        }
    }

    // 1. Root selection: the invocation directory or an explicit
    // selector. The directory must already exist; bootstrap never
    // creates project roots.
    let (root, root_basis) = match resolve_root(request.project.as_deref()) {
        Ok(resolved) => resolved,
        Err(result) => return result,
    };

    // 2. Project id derivation.
    let project = match derive_project_id(&root, request.project_id.as_deref()) {
        Ok(project) => project,
        Err(detail) => {
            return DomainResult::denied(diagnostic::bootstrap_id_required_set(detail));
        }
    };

    // 3. Plan. The writer seam independently refuses any value outside
    // its closed grammar (defense in depth for future in-crate
    // consumers of this bootstrap seam).
    let mut files = Vec::new();
    match project_document(&project.id, request.frontend) {
        Some(file) => files.push(file),
        None => return usage_refusal(),
    }
    match module_document(&request.module, request.frontend) {
        Some(file) => files.push(file),
        None => return usage_refusal(),
    }
    if let Some(target) = request.target.as_deref() {
        match plan::target_document(target, request.profile.as_deref()) {
            Some(file) => files.push(file),
            None => return usage_refusal(),
        }
    }
    if request.editor_hints {
        files.push(editor_hints_document());
    }

    // 4. Preflight over the observed tree.
    let state = preflight(&root, &files, true);

    // 5. Dry-run: the full machine-readable plan, nothing written.
    if request.dry_run {
        return receipt(BootstrapReceipt {
            status: "valid",
            operation: "init",
            mode: "dry-run",
            changed: false,
            root_basis,
            project_id: project.id.clone(),
            project_id_source: project.provenance.clone(),
            module: request.module.clone(),
            frontend: request.frontend.as_str(),
            target: request.target.clone(),
            adapter_profile: request.profile.clone(),
            editor_hints: request.editor_hints,
            added: state.creates.len() + state.append.as_ref().map_or(0, |_| 1),
            unchanged: state.unchanged,
            conflicting: state.conflicts.len(),
            writes: state.writes,
            gate: None,
            templates: templates(request),
        });
    }

    // 6. No-overwrite conflicts deny the whole bootstrap.
    if !state.conflicts.is_empty() {
        return DomainResult::denied(diagnostic::bootstrap_conflict_set(&state.conflicts));
    }

    // 7. Idempotent re-run: nothing to create or append, nothing to
    // gate.
    if state.creates.is_empty() && state.append.is_none() {
        return receipt(BootstrapReceipt {
            status: "valid",
            operation: "init",
            mode: "apply",
            changed: false,
            root_basis,
            project_id: project.id.clone(),
            project_id_source: project.provenance.clone(),
            module: request.module.clone(),
            frontend: request.frontend.as_str(),
            target: request.target.clone(),
            adapter_profile: request.profile.clone(),
            editor_hints: request.editor_hints,
            added: 0,
            unchanged: state.unchanged,
            conflicting: 0,
            writes: state.writes,
            gate: None,
            templates: templates(request),
        });
    }

    // 8. Apply: journaled no-clobber creates plus the journaled
    // `.gitignore` append, one rollback unit.
    let journal = match plan::apply_bootstrap(&root, &state.creates, state.append.as_ref()) {
        plan::ApplyOutcome::Applied(journal) => journal,
        plan::ApplyOutcome::WriteFailed { path, detail } => {
            return DomainResult::invalid(diagnostic::bootstrap_write_failed_set(&path, detail));
        }
        plan::ApplyOutcome::RecoveryRequired { paths } => {
            return DomainResult::invalid(diagnostic::bootstrap_recovery_required_set(&paths));
        }
    };

    // 9. Post-write gate: the bootstrap must load and validate through
    // the normal path; a failure rolls every genuine mutation back
    // first (the `.gitignore` truncation included).
    let model_version = match super::post_write_gate(&root) {
        Ok(model_version) => model_version,
        Err(result) => {
            let remaining = journal.rollback(&root);
            if !remaining.is_empty() {
                return DomainResult::invalid(diagnostic::bootstrap_recovery_required_set(
                    &remaining,
                ));
            }
            return result;
        }
    };

    // 10. Applied receipt.
    receipt(BootstrapReceipt {
        status: "valid",
        operation: "init",
        mode: "apply",
        changed: true,
        root_basis,
        project_id: project.id.clone(),
        project_id_source: project.provenance.clone(),
        module: request.module.clone(),
        frontend: request.frontend.as_str(),
        target: request.target.clone(),
        adapter_profile: request.profile.clone(),
        editor_hints: request.editor_hints,
        added: state.creates.len() + usize::from(state.append.is_some()),
        unchanged: state.unchanged,
        conflicting: 0,
        writes: state.writes,
        gate: Some(Gate {
            status: "valid",
            profile: "default",
            model_version,
        }),
        templates: templates(request),
    })
}

/// Run `lekalo module new` to completion.
pub fn module_new(request: &ModuleNewRequest) -> DomainResult {
    // 0. Request grammar before any resolution or write.
    if !valid_project_id(&request.id) {
        return usage_refusal();
    }

    // 1. Root resolution: the normal marker discovery — a module
    // belongs to an existing project.
    let selection = crate::loader::LoadSelection {
        project: request
            .project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let root = match crate::loader::root_for_selection(&selection) {
        Ok(root) => root,
        Err(result) => return result,
    };

    // 2. Plan and preflight over the observed module home.
    let file = match module_document(&request.id, request.frontend) {
        Some(file) => file,
        None => return usage_refusal(),
    };
    let mut state = Preflight {
        creates: Vec::new(),
        append: None,
        conflicts: Vec::new(),
        unchanged: 0,
        writes: Vec::new(),
    };
    classify_create(
        &root,
        &file,
        &mut state.creates,
        &mut state.conflicts,
        &mut state.unchanged,
        &mut state.writes,
    );

    // 3. Dry-run: the plan, nothing written.
    if request.dry_run {
        return module_receipt(ModuleReceipt {
            status: "valid",
            operation: "module",
            mode: "dry-run",
            changed: false,
            module_id: request.id.clone(),
            frontend: request.frontend.as_str(),
            added: state.creates.len(),
            unchanged: state.unchanged,
            conflicting: state.conflicts.len(),
            writes: state.writes,
            gate: None,
            templates: vec![TemplateRecord::module(TEMPLATE_VERSION)],
        });
    }

    // 4. No-overwrite conflicts deny the whole creation.
    if !state.conflicts.is_empty() {
        return DomainResult::denied(diagnostic::bootstrap_conflict_set(&state.conflicts));
    }

    // 5. Idempotent re-run.
    if state.creates.is_empty() {
        return module_receipt(ModuleReceipt {
            status: "valid",
            operation: "module",
            mode: "apply",
            changed: false,
            module_id: request.id.clone(),
            frontend: request.frontend.as_str(),
            added: 0,
            unchanged: state.unchanged,
            conflicting: 0,
            writes: state.writes,
            gate: None,
            templates: vec![TemplateRecord::module(TEMPLATE_VERSION)],
        });
    }

    // 6. Apply and gate: identical journal-and-rollback semantics.
    let journal = match plan::apply_bootstrap(&root, &state.creates, None) {
        plan::ApplyOutcome::Applied(journal) => journal,
        plan::ApplyOutcome::WriteFailed { path, detail } => {
            return DomainResult::invalid(diagnostic::bootstrap_write_failed_set(&path, detail));
        }
        plan::ApplyOutcome::RecoveryRequired { paths } => {
            return DomainResult::invalid(diagnostic::bootstrap_recovery_required_set(&paths));
        }
    };
    let model_version = match super::post_write_gate(&root) {
        Ok(model_version) => model_version,
        Err(result) => {
            let remaining = journal.rollback(&root);
            if !remaining.is_empty() {
                return DomainResult::invalid(diagnostic::bootstrap_recovery_required_set(
                    &remaining,
                ));
            }
            return result;
        }
    };
    module_receipt(ModuleReceipt {
        status: "valid",
        operation: "module",
        mode: "apply",
        changed: true,
        module_id: request.id.clone(),
        frontend: request.frontend.as_str(),
        added: state.creates.len(),
        unchanged: state.unchanged,
        conflicting: 0,
        writes: state.writes,
        gate: Some(Gate {
            status: "valid",
            profile: "default",
            model_version,
        }),
        templates: vec![TemplateRecord::module(TEMPLATE_VERSION)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both frontends render byte-stable documents for a grammar-clean
    /// id, and both fail closed on a rejected id.
    #[test]
    fn document_templates_are_deterministic_and_fail_closed() {
        let yaml = project_document("probe", Frontend::Yaml).expect("yaml project document");
        assert_eq!(yaml.path, "lekalo/project.yaml");
        assert_eq!(
            String::from_utf8(yaml.bytes).expect("utf8"),
            "schema_version: \"1.0.0\"\ndefinitions:\n  - id: probe\n    kind: project\n    version: 1\n    description: \"New Lekalo project.\"\n"
        );
        let json = project_document("probe", Frontend::Json).expect("json project document");
        assert_eq!(
            String::from_utf8(json.bytes).expect("utf8"),
            "{\"schema_version\":\"1.0.0\",\"definitions\":[{\"id\":\"probe\",\"kind\":\"project\",\"version\":1,\"description\":\"New Lekalo project.\"}]}\n"
        );
        let module = module_document("planner", Frontend::Yaml).expect("module document");
        assert_eq!(module.path, "lekalo/modules/planner/module.yaml");
        assert!(String::from_utf8(module.bytes)
            .expect("utf8")
            .contains("kind: module"));
        assert!(project_document("Bad_Id", Frontend::Yaml).is_none());
        assert!(module_document("lekalo", Frontend::Json).is_none());
        assert!(module_document("a/b", Frontend::Yaml).is_none());
    }

    /// The ignore-file classification: both managed spellings count as
    /// present, the suffix keeps exactly one newline separation, and an
    /// empty observed file degenerates to the managed line alone.
    #[test]
    fn gitignore_merge_is_exact() {
        assert!(ignores_lekalo(b"/target\n/.lekalo/\n"));
        assert!(ignores_lekalo(b".lekalo/\n"));
        assert!(!ignores_lekalo(b"/target\n"));
        assert!(!ignores_lekalo(b"foo.lekalo/\n"));
        assert_eq!(gitignore_suffix(b"/target"), b"\n/.lekalo/\n".to_vec());
        assert_eq!(gitignore_suffix(b"/target\n"), b"/.lekalo/\n".to_vec());
        assert_eq!(gitignore_suffix(b""), b"/.lekalo/\n".to_vec());
    }

    /// The editor hints are a fixed byte-stable document.
    #[test]
    fn editor_hints_are_fixed_bytes() {
        let document = editor_hints_document();
        assert_eq!(document.path, ".vscode/settings.json");
        assert_eq!(
            String::from_utf8(document.bytes).expect("utf8"),
            concat!(
                "{\n",
                "  \"yaml.schemas\": {\n",
                "    \"https://lekalo.dev/schemas/model/1.0.0/schema.json\": [\n",
                "      \"lekalo/project.yaml\",\n",
                "      \"lekalo/modules/**/*.yaml\"\n",
                "    ]\n",
                "  }\n",
                "}\n"
            )
        );
    }

    /// The preflight classifies create, unchanged, and append over a
    /// real temporary tree, including the `.gitignore` merge.
    #[test]
    fn preflight_classifies_every_artifact() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join(".gitignore"), b"/target\n").expect("seed ignore");
        let files = vec![
            project_document("probe", Frontend::Yaml).expect("project"),
            module_document("app", Frontend::Yaml).expect("module"),
        ];
        let state = preflight(root, &files, true);
        assert_eq!(state.creates.len(), 2);
        assert!(state.append.is_some());
        assert_eq!(
            state.append.as_ref().expect("append").suffix,
            b"/.lekalo/\n"
        );
        assert_eq!(state.unchanged, 0);
        assert!(state.conflicts.is_empty());
        assert_eq!(state.writes.len(), 3);
        // A second pass over the applied result is fully unchanged.
        plan::apply_bootstrap(root, &state.creates, state.append.as_ref());
        let files = vec![
            project_document("probe", Frontend::Yaml).expect("project"),
            module_document("app", Frontend::Yaml).expect("module"),
        ];
        let state = preflight(root, &files, true);
        assert!(state.creates.is_empty());
        assert!(state.append.is_none());
        assert_eq!(state.unchanged, 3);
        assert_eq!(
            std::fs::read(root.join(".gitignore")).expect("ignore bytes"),
            b"/target\n/.lekalo/\n".to_vec()
        );
    }

    /// A gate failure after a successful apply rolls the `.gitignore`
    /// append back to its exact original bytes and removes every
    /// created path.
    #[test]
    fn gate_failure_rolls_the_append_back_exactly() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::write(root.join(".gitignore"), b"/target\n").expect("seed ignore");
        let files = vec![project_document("probe", Frontend::Yaml).expect("project")];
        let state = preflight(root, &files, false);
        let journal = match plan::apply_bootstrap(
            root,
            &state.creates,
            Some(&plan::PlannedAppend {
                path: GITIGNORE_PATH.to_owned(),
                suffix: gitignore_suffix(b"/target\n"),
            }),
        ) {
            plan::ApplyOutcome::Applied(journal) => journal,
            outcome => panic!("unexpected outcome: {outcome:?}"),
        };
        assert_eq!(
            std::fs::read(root.join(".gitignore")).expect("appended"),
            b"/target\n/.lekalo/\n".to_vec()
        );
        assert!(journal.rollback(root).is_empty());
        assert_eq!(
            std::fs::read(root.join(".gitignore")).expect("restored"),
            b"/target\n".to_vec()
        );
        assert!(!root.join("lekalo").exists());
    }
}
