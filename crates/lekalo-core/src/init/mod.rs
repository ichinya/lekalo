//! `lekalo init --adopt`: connect Lekalo to an existing repository
//! (issue #38).
//!
//! Adoption never moves sources, never generates beyond the minimal
//! canonical skeleton, and never overwrites: planned writes go through the
//! operating system's atomic no-clobber, identical paths are skipped, and
//! any other existing path denies the whole adoption. Detection is
//! evidence with provenance and confidence — observed modules stay
//! observations (`mode: "observed"`) because the Model contract has no
//! module-mode field, and Lekalo emits no unpublished fields. The applied
//! skeleton must pass the normal load and validation path; a gate failure
//! rolls every created file back before the gate's failure passes through.

pub mod detect;
pub mod diagnostic;
pub mod plan;
pub mod receipt;

use std::path::{Path, PathBuf};

use self::receipt::{AdoptReceipt, Confidence, Detection, Gate, ProjectIdProvenance, WriteEntry};
use crate::result::{DomainResult, Status};

/// The maximum one-segment id length the Model contract accepts.
const SEGMENT_ID_MAX: usize = 63;

/// One `lekalo init --adopt` request.
#[derive(Clone, Debug, Default)]
pub struct AdoptRequest {
    /// Explicit adoption-root selector (`--project`/`LEKALO_PROJECT`).
    pub project: Option<String>,
    /// Explicit target selection (`--target`).
    pub target: Option<String>,
    /// Explicit adapter profile selection (`--profile`; the CLI refuses an
    /// orphan profile and enforces the #28 token grammar).
    pub profile: Option<String>,
    /// Explicit project id (`--project-id`).
    pub project_id: Option<String>,
    /// Preview the plan without writing (`--dry-run`).
    pub dry_run: bool,
}

/// How the adopted root was chosen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootBasis {
    Explicit,
    WorkspaceRoot,
    Manifest,
    InvocationDirectory,
}

impl RootBasis {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::WorkspaceRoot => "workspace-root",
            Self::Manifest => "manifest",
            Self::InvocationDirectory => "invocation-directory",
        }
    }
}

/// Map a structure outcome onto the accepted failure classes exactly as
/// the loader maps it.
fn structure_failure(outcome: crate::project_fs::StructureOutcome) -> DomainResult {
    let (status, reasons) = match outcome {
        crate::project_fs::StructureOutcome::Valid(_) => unreachable!("valid has no failure"),
        crate::project_fs::StructureOutcome::Invalid(reasons) => (Status::Invalid, reasons),
        crate::project_fs::StructureOutcome::Denied(reasons) => (Status::Denied, reasons),
    };
    let diagnostics = reasons
        .into_iter()
        .map(|reason| {
            let mut diagnostic = crate::loader::error::Diagnostic::new(reason.code);
            if let Some(path) = reason.path {
                diagnostic = diagnostic.with_path(path);
            }
            diagnostic
        })
        .collect();
    crate::loader::diagnostic::failure(status, diagnostics)
}

/// The invocation-relative spelling of one ancestor level (no absolute
/// path ever leaves the process).
fn relative_spelling(level: usize) -> String {
    if level == 0 {
        ".".to_owned()
    } else {
        vec![".."; level].join("/")
    }
}

/// Resolve the adoption root: an explicit selector wins, then the unique
/// workspace root among the invocation ancestors, then the nearest
/// manifest ancestor, then the invocation directory itself. More than one
/// workspace root is ambiguous and demands explicit resolution.
fn resolve_root(selector: Option<&str>) -> Result<(PathBuf, RootBasis, Vec<String>), DomainResult> {
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
        match crate::project_fs::Fs::check_selection(selector, "structure.project-not-directory") {
            Ok(path) => return Ok((path, RootBasis::Explicit, Vec::new())),
            Err(outcome) => return Err(structure_failure(outcome)),
        }
    }
    let cwd = std::env::current_dir().map_err(|_| {
        crate::loader::diagnostic::failure(
            Status::Invalid,
            vec![crate::loader::error::Diagnostic::new(
                "structure.root-unreadable",
            )],
        )
    })?;
    let mut workspace_roots: Vec<(usize, PathBuf)> = Vec::new();
    let mut manifest_ancestor: Option<(usize, PathBuf)> = None;
    let mut current = cwd.clone();
    for level in 0..crate::project_fs::MAX_WALK_DEPTH {
        match detect::workspace_marker(&current) {
            Some(_) => workspace_roots.push((level, current.clone())),
            None => {
                if manifest_ancestor.is_none() && detect::any_manifest(&current) {
                    manifest_ancestor = Some((level, current.clone()));
                }
            }
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }
    if workspace_roots.len() > 1 {
        let candidates: Vec<String> = workspace_roots
            .iter()
            .map(|(level, _)| relative_spelling(*level))
            .collect();
        return Err(DomainResult::denied(diagnostic::ambiguous_root_set(
            &candidates,
        )));
    }
    if let Some((level, path)) = workspace_roots.first() {
        return Ok((
            path.clone(),
            RootBasis::WorkspaceRoot,
            vec![relative_spelling(*level)],
        ));
    }
    if let Some((level, path)) = manifest_ancestor {
        return Ok((path, RootBasis::Manifest, vec![relative_spelling(level)]));
    }
    Ok((cwd, RootBasis::InvocationDirectory, vec![".".to_owned()]))
}

/// The canonical project id with its provenance.
pub struct ProjectId {
    pub id: String,
    pub provenance: ProjectIdProvenance,
}

/// Sanitize one observed name into the closed one-segment id grammar
/// (`^[a-z][a-z0-9_]{0,62}$`, tool-reserved words excluded).
fn sanitize_id(raw: &str) -> Option<String> {
    let mut folded = String::with_capacity(raw.len());
    for character in raw.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            folded.push(character);
        } else if character.is_ascii_uppercase() {
            folded.push(character.to_ascii_lowercase());
        } else {
            folded.push('_');
        }
    }
    let mut collapsed = String::with_capacity(folded.len());
    let mut previous_underscore = false;
    for character in folded.chars() {
        if character == '_' {
            if !previous_underscore {
                collapsed.push(character);
            }
            previous_underscore = true;
        } else {
            collapsed.push(character);
            previous_underscore = false;
        }
    }
    let mut candidate = collapsed.trim_matches('_').to_owned();
    if candidate.starts_with(|first: char| first.is_ascii_digit()) {
        candidate = format!("p_{candidate}");
    }
    if candidate == "lekalo" || candidate == "dev" {
        candidate = format!("p_{candidate}");
    }
    let mut bounded: String = candidate.chars().take(SEGMENT_ID_MAX).collect();
    while bounded.ends_with('_') {
        bounded.pop();
    }
    if bounded.is_empty()
        || !bounded.starts_with(|first: char| first.is_ascii_lowercase())
        || !bounded
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    Some(bounded)
}

/// The closed one-segment project-id grammar
/// (`^[a-z][a-z0-9_]{0,62}$`, tool-reserved words excluded).
pub fn valid_project_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !id.is_empty()
        && id.len() <= SEGMENT_ID_MAX
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
        && id != "lekalo"
        && id != "dev"
}

/// One root-manifest project-id candidate: file name, name reader,
/// provenance source, and confidence.
type NameCandidate = (
    &'static str,
    fn(&Path) -> Option<String>,
    &'static str,
    Confidence,
);

/// Derive the adopted project id: explicit flag, then root manifest
/// names, then the root directory name. Every derivation is recorded.
fn derive_project_id(root: &Path, explicit: Option<&str>) -> Result<ProjectId, &'static str> {
    if let Some(explicit) = explicit {
        return match sanitize_id(explicit) {
            Some(id) if id == explicit => Ok(ProjectId {
                id,
                provenance: ProjectIdProvenance {
                    source: "--project-id".to_owned(),
                    original: None,
                    confidence: Confidence::High,
                },
            }),
            _ => Err("invalid-project-id"),
        };
    }
    let manifest = |name: &str| root.join(name);
    fn go_segment(path: &Path) -> Option<String> {
        detect::go_module_path(path)
            .map(|module| module.rsplit('/').next().unwrap_or(&module).to_owned())
    }
    let candidates: [NameCandidate; 4] = [
        (
            "package.json",
            detect::json_name,
            "package.json#name",
            Confidence::High,
        ),
        (
            "composer.json",
            detect::json_name,
            "composer.json#name",
            Confidence::High,
        ),
        (
            "Cargo.toml",
            detect::cargo_package_name,
            "Cargo.toml#package.name",
            Confidence::High,
        ),
        ("go.mod", go_segment, "go.mod#module", Confidence::High),
    ];
    for (name, reader, source, confidence) in candidates {
        if let Some(raw) = reader(&manifest(name)) {
            if let Some(id) = sanitize_id(&raw) {
                return Ok(ProjectId {
                    id,
                    provenance: ProjectIdProvenance {
                        source: source.to_owned(),
                        original: Some(raw),
                        confidence,
                    },
                });
            }
        }
    }
    let directory = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(id) = sanitize_id(&directory) {
        return Ok(ProjectId {
            id,
            provenance: ProjectIdProvenance {
                source: "directory-name".to_owned(),
                original: Some(directory),
                confidence: Confidence::Medium,
            },
        });
    }
    Err("derive-project-id")
}

/// The post-write gate: the normal load and validation path, in process.
///
/// Returns the loaded Model contract version on success. Any failure is
/// the exact envelope `lekalo load`/`lekalo validate` would produce.
fn adoption_gate(root: &Path) -> Result<String, DomainResult> {
    match crate::project_fs::Fs::validate_project(root) {
        crate::project_fs::StructureOutcome::Valid(_) => {}
        outcome => return Err(structure_failure(outcome)),
    }
    let fs = crate::project_fs::Fs::open(root).map_err(|_| {
        crate::loader::diagnostic::failure(
            Status::Invalid,
            vec![crate::loader::error::Diagnostic::new(
                "structure.root-unreadable",
            )],
        )
    })?;
    if let Some(code) = crate::versioning::migration_recovery_code(&fs) {
        return Err(crate::loader::diagnostic::failure(
            Status::Invalid,
            vec![crate::loader::error::Diagnostic::new(code)],
        ));
    }
    let loaded = crate::loader::load_validated_root(root.to_path_buf())?;
    let compilation = crate::ir::compile(&loaded.model).map_err(|failure| failure.into_result())?;
    let profile = match crate::validator::ValidationProfile::embedded_default() {
        Ok(profile) => profile,
        Err(_) => {
            return Err(DomainResult::invalid(
                crate::validator::registry_invariant_failure(),
            ))
        }
    };
    match crate::validator::validate(&compilation, profile) {
        Err(set) => Err(DomainResult::invalid(set)),
        Ok(_) => Ok(loaded.model.model_version.as_str().to_owned()),
    }
}

/// Sort planned writes into create / already-present / conflict classes.
struct Preflight {
    creates: Vec<plan::PlannedFile>,
    writes: Vec<WriteEntry>,
    conflicts: Vec<String>,
    already_present: usize,
}

/// The SHA-256 helper of the versioning plan surface.
fn sha256_hex(bytes: &[u8]) -> String {
    crate::versioning::plan::sha256_hex(bytes)
}

/// Run the adoption preflight over the built plan.
fn preflight(root: &Path, files: &[plan::PlannedFile]) -> Preflight {
    let mut state = Preflight {
        creates: Vec::new(),
        writes: Vec::new(),
        conflicts: Vec::new(),
        already_present: 0,
    };
    for file in files {
        let disposition = match plan::observe(root, &file.path) {
            None => {
                state.creates.push(plan::PlannedFile {
                    path: file.path.clone(),
                    bytes: file.bytes.clone(),
                });
                "create"
            }
            Some(bytes) if bytes == file.bytes => {
                state.already_present += 1;
                "already-present"
            }
            Some(_) => {
                state.conflicts.push(file.path.clone());
                "conflict"
            }
        };
        state.writes.push(WriteEntry {
            path: file.path.clone(),
            action: "create",
            sha256: sha256_hex(&file.bytes),
            disposition,
        });
    }
    state
}

/// Render the adoption receipt as the success envelope.
fn receipt(envelope: AdoptReceipt) -> DomainResult {
    let human = receipt_human(&envelope);
    let json = serde_json::to_string_pretty(&envelope).expect("receipt serializes");
    DomainResult::receipt(json, human)
}

/// The stable one-line human summary of an adoption receipt.
fn receipt_human(receipt: &AdoptReceipt) -> String {
    match receipt.mode {
        "dry-run" => format!(
            "init adopt dry-run: {} planned write(s), {} already present, {} conflict(s)",
            receipt.writes.len(),
            receipt.already_present,
            receipt.conflicts
        ),
        _ => {
            let gate = receipt
                .gate
                .as_ref()
                .map(|gate| format!("gate {}", gate.status))
                .unwrap_or_else(|| "no writes".to_owned());
            format!(
                "init adopt apply: {} created, {} already present, {} conflict(s); {gate}",
                receipt.created, receipt.already_present, receipt.conflicts
            )
        }
    }
}

/// Run `lekalo init --adopt` to completion.
pub fn adopt(request: &AdoptRequest) -> DomainResult {
    // 0. Request grammar at the public core boundary: the identical
    // closed grammars every CLI invocation must already satisfy,
    // enforced here before any root discovery, detection walk, read
    // probe, or write. A violation is the registered `cli.usage`
    // failure; the wire never echoes the rejected value.
    if let Some(target) = request.target.as_deref() {
        if !detect::valid_target_id(target) {
            return DomainResult::usage_error();
        }
    }
    if let Some(profile) = request.profile.as_deref() {
        if request.target.is_none() || !crate::target_protocol::scopes::is_token(profile) {
            return DomainResult::usage_error();
        }
    }

    // 1. Root selection and ambiguity resolution.
    let (root, basis, candidates) = match resolve_root(request.project.as_deref()) {
        Ok(resolved) => resolved,
        Err(result) => return result,
    };

    // 2. Project id derivation.
    let project = match derive_project_id(&root, request.project_id.as_deref()) {
        Ok(project) => project,
        Err(detail) => return DomainResult::denied(diagnostic::id_required_set(detail)),
    };

    // 3. Detection: read-only, bounded, evidence-first.
    let mut detection: Detection = detect::detect(&root, request.target.as_deref());
    detection.basis = basis.as_str();
    detection.candidates = candidates;

    // 4. Plan and preflight: identical bytes skip, any other existing
    // path is a no-overwrite conflict. The writer seam independently
    // refuses any value outside its closed grammar (defense in depth
    // for future in-crate consumers of the #28 bootstrap seam).
    let files = match plan::build(
        &project.id,
        request.target.as_deref(),
        request.profile.as_deref(),
    ) {
        Ok(files) => files,
        Err(set) => return DomainResult::invalid(set),
    };
    let state = preflight(&root, &files);

    // 5. Dry-run: the full plan without writing anything.
    if request.dry_run {
        return receipt(AdoptReceipt {
            status: "valid",
            operation: "init",
            mode: "dry-run",
            changed: false,
            project_id: project.id.clone(),
            project_id_source: project.provenance.clone(),
            target: request.target.clone(),
            adapter_profile: request.profile.clone(),
            profile: "default",
            already_present: state.already_present,
            conflicts: state.conflicts.len(),
            created: 0,
            writes: state.writes,
            gate: None,
            detection,
        });
    }

    // 6. No-overwrite conflicts deny the whole adoption.
    if !state.conflicts.is_empty() {
        return DomainResult::denied(diagnostic::conflict_set(&state.conflicts));
    }

    // 7. Idempotent re-run: nothing to create, nothing to gate.
    if state.creates.is_empty() {
        return receipt(AdoptReceipt {
            status: "valid",
            operation: "init",
            mode: "apply",
            changed: false,
            project_id: project.id.clone(),
            project_id_source: project.provenance.clone(),
            target: request.target.clone(),
            adapter_profile: request.profile.clone(),
            profile: "default",
            already_present: state.already_present,
            conflicts: 0,
            created: 0,
            writes: state.writes,
            gate: None,
            detection,
        });
    }

    // 8. Apply: journaled create_new writes with rollback on failure.
    let create_paths: Vec<String> = state.creates.iter().map(|file| file.path.clone()).collect();
    match plan::apply(&root, &state.creates) {
        plan::ApplyOutcome::Applied => {}
        plan::ApplyOutcome::WriteFailed { path, detail } => {
            return DomainResult::invalid(diagnostic::write_failed_set(&path, detail));
        }
        plan::ApplyOutcome::RecoveryRequired { paths } => {
            return DomainResult::invalid(diagnostic::recovery_required_set(&paths));
        }
    }

    // 9. Post-write gate: the skeleton must load and validate. A gate
    // failure rolls every created file back first.
    let model_version = match adoption_gate(&root) {
        Ok(model_version) => model_version,
        Err(result) => {
            let journal = plan::Journal::from_created(&root, &create_paths);
            let remaining = journal.rollback(&root);
            if !remaining.is_empty() {
                return DomainResult::invalid(diagnostic::recovery_required_set(&remaining));
            }
            return result;
        }
    };

    // 10. Applied receipt.
    let created = state.creates.len();
    receipt(AdoptReceipt {
        status: "valid",
        operation: "init",
        mode: "apply",
        changed: true,
        project_id: project.id.clone(),
        project_id_source: project.provenance.clone(),
        target: request.target.clone(),
        adapter_profile: request.profile.clone(),
        profile: "default",
        already_present: state.already_present,
        conflicts: 0,
        created,
        writes: state.writes,
        gate: Some(Gate {
            status: "valid",
            profile: "default",
            model_version,
        }),
        detection,
    })
}
