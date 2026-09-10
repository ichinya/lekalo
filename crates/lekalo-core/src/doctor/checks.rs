//! The closed doctor check vocabulary and its read-only runners (issue
//! #92).
//!
//! Every runner is a pure projection over the accepted seams: the #7
//! loader (with the cache bypassed, so a doctor run never writes), the #10
//! lock verifier (pure), the #20 cache health facts, the artifact drift
//! check, and the read-only platform probes. Doctor never spawns adapters,
//! never repairs, and never writes: the underlying refusals surface as
//! preserved registry rule ids and closed next-action recipe ids.

use std::path::{Path, PathBuf};

use super::model::{
    Check, CheckState, GitFacts, GitState, LockRevision, Phase, TraceManifestEvidence,
};
use super::version::MAX_CHECK_DIAGNOSTICS;
use crate::loader::LoadSelection;
use crate::result::DomainResult;

/// The closed safe-fix recipe catalog: every `nextAction` names one of
/// these ids, and `--fix` renders the full bounded steps. Advice only.
pub(crate) fn recipe(id: &'static str) -> Option<(&'static str, bool, Vec<&'static str>)> {
    match id {
        "fix-structure" => Some((
            id,
            false,
            vec![
                "Read the preserved structure diagnostics of this report.",
                "Correct the named layout or path violation by hand.",
                "Re-run `lekalo doctor` to confirm the project is valid.",
            ],
        )),
        "create-lock" => Some((
            id,
            true,
            vec![
                "Move the unusable `lekalo.lock` aside (invalid state only).",
                "Run `lekalo lock` to create the committed lock.",
                "Commit `lekalo.lock` with the project.",
            ],
        )),
        "preview-lock-update" => Some((
            id,
            false,
            vec![
                "Run `lekalo update --dry-run` to preview the exact plan.",
                "Apply it explicitly with `lekalo update --apply sha256:PLAN_ID`.",
            ],
        )),
        "validate" => Some((
            id,
            false,
            vec![
                "Run `lekalo validate` for the full diagnostic list.",
                "Fix the reported semantic findings and re-run `lekalo doctor`.",
            ],
        )),
        "migrate-model" => Some((
            id,
            true,
            vec![
                "Run `lekalo migrate --to model/<target> --dry-run` to preview.",
                "Apply the migration with the same command without `--dry-run`.",
            ],
        )),
        "resolve-adapters" => Some((
            id,
            false,
            vec![
                "Declare the missing adapter, generator, profile, or capability",
                "in the resolution request and re-create the lock with",
                "`lekalo lock` after the candidates are available locally.",
            ],
        )),
        "regenerate" => Some((
            id,
            false,
            vec![
                "Run `lekalo generate --check` for the exact drift findings.",
                "Regenerate the named artifacts with the locked generator.",
                "Re-run `lekalo generate --check` until the verdict is clean.",
            ],
        )),
        "clear-cache" => Some((
            id,
            true,
            vec![
                "Run `lekalo cache status` to confirm the reported state.",
                "Run `lekalo cache clear --yes`; the cache is derived data and",
                "is rebuilt from the canonical inputs on the next run.",
            ],
        )),
        "recover-migration" => Some((
            id,
            true,
            vec![
                "A migration journal is holding the cache fail-closed.",
                "Recover it with `lekalo migrate --rollback sha256:PLAN_ID`.",
            ],
        )),
        "install-git" => Some((
            id,
            false,
            vec![
                "Install Git or fix PATH resolution so `git` executes.",
                "Re-run `lekalo doctor`; the built-in gates need no Git.",
            ],
        )),
        "init-git" => Some((
            id,
            true,
            vec![
                "Run `git init` inside the project root if change tracking",
                "is wanted; it is optional for every Lekalo command.",
            ],
        )),
        "grant-write" => Some((
            id,
            false,
            vec![
                "Verify the operating-system read/write permissions of the",
                "reported Lekalo home; doctor itself never writes.",
            ],
        )),
        "trace-validate" => Some((
            id,
            false,
            vec![
                "Run `lekalo trace validate <manifest>` on each supplied",
                "manifest and fix the reported wire or gap findings.",
            ],
        )),
        _ => None,
    }
}

/// The preserved, bounded registry rule ids of one underlying refusal.
pub(crate) fn ids_of(result: &DomainResult) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for diagnostic in result.diagnostics() {
        let id = diagnostic.id().to_owned();
        if !ids.contains(&id) {
            ids.push(id);
            if ids.len() == MAX_CHECK_DIAGNOSTICS {
                break;
            }
        }
    }
    ids
}

fn ids_from_codes(codes: &[&str]) -> Vec<String> {
    codes
        .iter()
        .take(MAX_CHECK_DIAGNOSTICS)
        .map(|code| (*code).to_owned())
        .collect()
}

fn action(id: &'static str) -> Option<&'static str> {
    debug_assert!(recipe(id).is_some(), "next actions name catalog recipes");
    Some(id)
}

/// `project.root`: the selection resolves and the layout validates.
/// Returns the check plus the validated root on success.
pub(crate) fn project_root(selection: &LoadSelection) -> (Check, Option<PathBuf>) {
    let blocked = |ids: Vec<String>| {
        (
            Check {
                id: "project.root",
                state: CheckState::Blocked,
                required: true,
                reason: Some("invalid"),
                next_action: action("fix-structure"),
                diagnostics: ids,
                notes: Vec::new(),
            },
            None,
        )
    };
    match crate::loader::root_for_selection(selection) {
        Err(result) => blocked(ids_of(&result)),
        Ok(root) => match crate::project_fs::Fs::open(&root) {
            Err(_) => blocked(ids_from_codes(&["structure.root-unreadable"])),
            Ok(_) => (
                Check {
                    id: "project.root",
                    state: CheckState::Ok,
                    required: true,
                    reason: Some("valid"),
                    next_action: None,
                    diagnostics: Vec::new(),
                    notes: Vec::new(),
                },
                Some(root),
            ),
        },
    }
}

/// Split one structure outcome into its layout half (`project.root`) and
/// its confinement half (`fs.confinement`) by rule-code family.
fn split_structure(reasons: &[crate::project_fs::StructureReason]) -> (Vec<String>, Vec<String>) {
    let mut layout = Vec::new();
    let mut confinement = Vec::new();
    for reason in reasons {
        if reason.code.starts_with("structure.path-") || reason.code == "structure.selection-alias"
        {
            if !confinement.iter().any(|id| id == reason.code) {
                confinement.push(reason.code.to_owned());
            }
        } else if !layout.iter().any(|id| id == reason.code) {
            layout.push(reason.code.to_owned());
        }
    }
    (layout, confinement)
}

/// `fs.confinement`: path-confinement denials and the read-only flag of
/// the project root. Read permission is implied by the structure pass.
pub(crate) fn fs_confinement(
    outcome: &crate::project_fs::StructureOutcome,
    root: Option<&Path>,
) -> Check {
    let (layout, confinement) = match outcome {
        crate::project_fs::StructureOutcome::Valid(_) => (Vec::new(), Vec::new()),
        crate::project_fs::StructureOutcome::Invalid(reasons) => split_structure(reasons),
        crate::project_fs::StructureOutcome::Denied(reasons) => split_structure(reasons),
    };
    let read_only = root
        .and_then(|root| std::fs::metadata(root).ok())
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false);
    let (state, reason, next) = if !confinement.is_empty() {
        (
            CheckState::Blocked,
            Some("confinement"),
            action("fix-structure"),
        )
    } else if read_only {
        (
            CheckState::Degraded,
            Some("read-only-root"),
            action("grant-write"),
        )
    } else {
        (CheckState::Ok, Some("confined"), None)
    };
    Check {
        id: "fs.confinement",
        state,
        required: true,
        reason,
        next_action: next,
        diagnostics: if layout.is_empty() {
            confinement
        } else {
            let mut ids = confinement;
            for code in layout
                .into_iter()
                .take(MAX_CHECK_DIAGNOSTICS.saturating_sub(ids.len()))
            {
                ids.push(code);
            }
            ids
        },
        notes: Vec::new(),
    }
}

/// The model/IR/load outcome shared by the version, references, and
/// bindings checks.
pub(crate) struct ModelOutcome {
    pub model_version: Option<String>,
    pub version: Check,
    pub references: Check,
}

/// `model.version` + `model.references`: load and compile with the cache
/// bypassed, classify preserved ids, and run the default validation
/// profile for the reference rules.
pub(crate) fn model_checks(selection: &LoadSelection) -> ModelOutcome {
    let version_check = |state, reason, next, ids: Vec<String>| Check {
        id: "model.version",
        state,
        required: true,
        reason,
        next_action: next,
        diagnostics: ids,
        notes: Vec::new(),
    };
    let references_check = |state, reason, next, ids: Vec<String>| Check {
        id: "model.references",
        state,
        required: true,
        reason,
        next_action: next,
        diagnostics: ids,
        notes: Vec::new(),
    };
    match crate::cache::load_compiled(selection, true) {
        Err(result) => {
            let ids = ids_of(&result);
            let version_fault = ids.iter().any(|id| id.starts_with("versioning."));
            ModelOutcome {
                model_version: None,
                version: if version_fault {
                    version_check(
                        CheckState::Blocked,
                        Some("unsupported"),
                        action("migrate-model"),
                        ids.clone(),
                    )
                } else {
                    version_check(
                        CheckState::Unknown,
                        Some("upstream-unavailable"),
                        action("fix-structure"),
                        ids.clone(),
                    )
                },
                references: if version_fault {
                    references_check(
                        CheckState::Unknown,
                        Some("upstream-unavailable"),
                        action("migrate-model"),
                        Vec::new(),
                    )
                } else {
                    references_check(
                        CheckState::Blocked,
                        Some("unresolved"),
                        action("validate"),
                        ids,
                    )
                },
            }
        }
        Ok((model, compilation)) => {
            let version = model.model_version.as_str().to_owned();
            let outcome = match crate::validator::ValidationProfile::embedded_default() {
                Err(_) => Err(crate::diagnostics::DiagnosticSet::empty()),
                Ok(profile) => crate::validator::validate(&compilation, profile),
            };
            match outcome {
                Err(set) => {
                    let ids: Vec<String> = set
                        .reason_ids()
                        .into_iter()
                        .take(MAX_CHECK_DIAGNOSTICS)
                        .map(str::to_owned)
                        .collect();
                    ModelOutcome {
                        model_version: Some(version),
                        version: version_check(CheckState::Ok, Some("accepted"), None, Vec::new()),
                        references: references_check(
                            CheckState::Blocked,
                            Some("unresolved"),
                            action("validate"),
                            ids,
                        ),
                    }
                }
                Ok(report) => {
                    let ids: Vec<String> = report
                        .diagnostics()
                        .reason_ids()
                        .into_iter()
                        .take(MAX_CHECK_DIAGNOSTICS)
                        .map(str::to_owned)
                        .collect();
                    let degraded = !report.diagnostics().is_empty();
                    ModelOutcome {
                        model_version: Some(version),
                        version: version_check(CheckState::Ok, Some("accepted"), None, Vec::new()),
                        references: if degraded {
                            references_check(
                                CheckState::Degraded,
                                Some("warnings"),
                                action("validate"),
                                ids,
                            )
                        } else {
                            references_check(CheckState::Ok, Some("resolved"), None, Vec::new())
                        },
                    }
                }
            }
        }
    }
}

/// The typed lock facts shared by the lock, adapter, capability, binding,
/// and artifact checks.
pub(crate) struct LockFacts {
    pub revision: LockRevision,
    pub check: Check,
    pub adapters: usize,
    pub generators: usize,
    pub profiles: usize,
    pub partial_capabilities: bool,
    pub unsatisfied_capabilities: bool,
}

impl LockFacts {
    pub(crate) fn none() -> Self {
        Self {
            revision: LockRevision {
                state: "absent",
                digest: None,
            },
            check: Check {
                id: "lock.freshness",
                state: CheckState::Unknown,
                required: true,
                reason: Some("upstream-unavailable"),
                next_action: action("fix-structure"),
                diagnostics: Vec::new(),
                notes: Vec::new(),
            },
            adapters: 0,
            generators: 0,
            profiles: 0,
            partial_capabilities: false,
            unsatisfied_capabilities: false,
        }
    }
}

/// The typed lock facts of a genuinely absent lock file.
fn absent_lock_facts() -> LockFacts {
    LockFacts {
        revision: LockRevision {
            state: "absent",
            digest: None,
        },
        check: Check {
            id: "lock.freshness",
            state: CheckState::Degraded,
            required: true,
            reason: Some("absent"),
            next_action: action("create-lock"),
            diagnostics: ids_from_codes(&["lock.missing"]),
            notes: Vec::new(),
        },
        ..LockFacts::none()
    }
}

/// The typed lock facts of a lock that exists but could not be read,
/// parsed, or request-matched structurally: the check stays unknown with
/// the preserved registry rule ids of the underlying refusal (never a
/// hardcoded stand-in), and the revision block records `invalid` — never
/// `absent` for a lock surface that exists.
fn refused_lock_facts(ids: Vec<String>) -> LockFacts {
    LockFacts {
        revision: LockRevision {
            state: "invalid",
            digest: None,
        },
        check: Check {
            id: "lock.freshness",
            state: CheckState::Unknown,
            required: true,
            reason: Some("upstream-unavailable"),
            next_action: action("fix-structure"),
            diagnostics: ids,
            notes: Vec::new(),
        },
        ..LockFacts::none()
    }
}

/// `lock.freshness`: absent, stale, invalid, or fresh — each state
/// distinguished with its own reason and next action, never repaired.
/// The revision block spells the same state the verifier reached:
/// `absent` only for a genuinely missing lock, `fresh`/`stale`/`invalid`
/// for a present one per the verification verdict.
pub(crate) fn lock_facts(selection: &LoadSelection, root: &Path) -> LockFacts {
    let registry = match crate::versioning::VersionRegistry::embedded() {
        Err(_) => return refused_lock_facts(ids_from_codes(&["versioning.registry-invalid"])),
        Ok(registry) => registry,
    };
    match crate::lockfile::plan::LockService::read_state_at(root) {
        // The documented absent race: the file vanished between the
        // entry probe and the read. A genuinely missing lock, not a
        // refusal.
        Err(crate::lockfile::LockFailure::Missing) => absent_lock_facts(),
        // Every other refusal keeps its real diagnostic identity; a
        // lock that exists but is unreadable or invalid is `invalid`,
        // never `absent`.
        Err(failure) => refused_lock_facts(ids_of(&DomainResult::from(&failure))),
        Ok(crate::lockfile::LockState::Absent) => absent_lock_facts(),
        Ok(crate::lockfile::LockState::Present(lock)) => {
            let request = match crate::lockfile::plan::LockService::load_request(selection) {
                Err(failure) => {
                    return refused_lock_facts(ids_of(&DomainResult::from(&failure)));
                }
                Ok(request) => request,
            };
            let digest = crate::lockfile::types::Lockfile::digest(&lock)
                .as_str()
                .to_owned();
            let adapters = lock.adapters().len();
            let generators = lock.generators().len();
            let profiles = lock.profiles().len();
            let mut partial = false;
            let mut unsatisfied = false;
            for capability in lock.capabilities() {
                match capability.support() {
                    crate::lockfile::types::Support::Full => {}
                    crate::lockfile::types::Support::Partial => partial = true,
                    crate::lockfile::types::Support::Unsupported
                    | crate::lockfile::types::Support::Unknown => unsatisfied = true,
                }
            }
            let (check, revision_state) = match crate::lockfile::verify::LockVerifier::verify(
                &lock,
                &request,
                &crate::lockfile::verify::RuntimeInventory::empty(),
                registry,
                crate::lockfile::verify::LockRequirement::Optional,
            ) {
                crate::lockfile::verify::LockVerdict::Satisfied { .. } => (
                    Check {
                        id: "lock.freshness",
                        state: CheckState::Ok,
                        required: true,
                        reason: Some("fresh"),
                        next_action: None,
                        diagnostics: Vec::new(),
                        notes: Vec::new(),
                    },
                    "fresh",
                ),
                crate::lockfile::verify::LockVerdict::Refused(
                    crate::lockfile::LockFailure::Stale,
                ) => (
                    Check {
                        id: "lock.freshness",
                        state: CheckState::Degraded,
                        required: true,
                        reason: Some("stale"),
                        next_action: action("preview-lock-update"),
                        diagnostics: ids_from_codes(&["lock.stale"]),
                        notes: Vec::new(),
                    },
                    "stale",
                ),
                crate::lockfile::verify::LockVerdict::Refused(other) => {
                    let ids = ids_of(&DomainResult::from(&other));
                    (
                        Check {
                            id: "lock.freshness",
                            state: CheckState::Blocked,
                            required: true,
                            reason: Some("invalid"),
                            next_action: action("create-lock"),
                            diagnostics: ids,
                            notes: Vec::new(),
                        },
                        "invalid",
                    )
                }
            };
            LockFacts {
                revision: LockRevision {
                    state: revision_state,
                    digest: Some(digest),
                },
                check,
                adapters,
                generators,
                profiles,
                partial_capabilities: partial,
                unsatisfied_capabilities: unsatisfied,
            }
        }
    }
}

/// `adapters.inventory`: locked adapter and generator identities with
/// their digests; declared targets without any resolved adapter are the
/// recorded blocker with its next action.
pub(crate) fn adapters_inventory(facts: &LockFacts, targets_declared: bool) -> Check {
    let base = |state, reason, next, ids: Vec<String>| Check {
        id: "adapters.inventory",
        state,
        required: targets_declared,
        reason: Some(reason),
        next_action: next,
        diagnostics: ids,
        notes: Vec::new(),
    };
    if matches!(facts.check.state, CheckState::Unknown) {
        return base(
            CheckState::Unknown,
            "no-lock",
            action("create-lock"),
            Vec::new(),
        );
    }
    if targets_declared && facts.adapters == 0 {
        return base(
            CheckState::Blocked,
            "targets-without-adapters",
            action("resolve-adapters"),
            Vec::new(),
        );
    }
    if facts.adapters == 0 && facts.generators == 0 {
        return base(CheckState::Ok, "none-required", None, Vec::new());
    }
    base(CheckState::Ok, "resolved", None, Vec::new())
}

/// `capabilities.profiles`: the locked capability/profile resolution.
pub(crate) fn capabilities_profiles(facts: &LockFacts, targets_declared: bool) -> Check {
    let base = |state, reason, next: Option<&'static str>| Check {
        id: "capabilities.profiles",
        state,
        required: targets_declared,
        reason: Some(reason),
        next_action: next,
        diagnostics: Vec::new(),
        notes: Vec::new(),
    };
    if matches!(facts.check.state, CheckState::Unknown) {
        return base(CheckState::Unknown, "no-lock", action("create-lock"));
    }
    if targets_declared && facts.profiles == 0 {
        return base(
            CheckState::Blocked,
            "targets-without-profile",
            action("resolve-adapters"),
        );
    }
    if facts.unsatisfied_capabilities {
        return base(
            CheckState::Blocked,
            "unsatisfied-capability",
            action("resolve-adapters"),
        );
    }
    if facts.partial_capabilities {
        return base(
            CheckState::Degraded,
            "partial-support",
            action("resolve-adapters"),
        );
    }
    if facts.profiles == 0 {
        return base(CheckState::Ok, "none-declared", None);
    }
    base(CheckState::Ok, "resolved", None)
}

/// `cache.health`: the bounded cache facts; corruption degrades (the
/// cache is never canonical) and path denials block.
pub(crate) fn cache_health(root: &Path) -> Check {
    let base = |state, reason: &'static str, next: Option<&'static str>, ids: Vec<String>| Check {
        id: "cache.health",
        state,
        required: false,
        reason: Some(reason),
        next_action: next,
        diagnostics: ids,
        notes: Vec::new(),
    };
    match crate::cache::health_facts(root) {
        crate::cache::CacheHealth::Denied { code, .. } => base(
            CheckState::Blocked,
            "confinement",
            action("fix-structure"),
            vec![code.to_owned()],
        ),
        crate::cache::CacheHealth::Facts(health) => {
            let state = health.state;
            match state {
                "ok" => base(CheckState::Ok, "ok", None, Vec::new()),
                "empty" | "missing" => base(CheckState::Ok, state, None, Vec::new()),
                "corrupt" | "quarantined" => base(
                    CheckState::Degraded,
                    state,
                    action("clear-cache"),
                    Vec::new(),
                ),
                "locked" => base(
                    CheckState::Degraded,
                    "locked",
                    action("recover-migration"),
                    Vec::new(),
                ),
                _ => base(
                    CheckState::Degraded,
                    "unreadable",
                    action("grant-write"),
                    Vec::new(),
                ),
            }
        }
    }
}

/// `bindings.freshness`: committed bindings ride the model validation;
/// the derived consumer bindings home is checked for presence against the
/// declared generators (bounded v1 presence semantics, ADR-0032).
pub(crate) fn bindings_freshness(root: &Path, references: &Check, facts: &LockFacts) -> Check {
    let base = |state, reason: &'static str, next: Option<&'static str>, ids: Vec<String>| Check {
        id: "bindings.freshness",
        state,
        required: true,
        reason: Some(reason),
        next_action: next,
        diagnostics: ids,
        notes: Vec::new(),
    };
    if matches!(references.state, CheckState::Blocked | CheckState::Unknown) {
        return base(
            CheckState::Unknown,
            "upstream-unavailable",
            action("validate"),
            Vec::new(),
        );
    }
    let binding_generators = facts.generators > 0;
    let mut generated = false;
    if let Ok(fs) = crate::project_fs::Fs::open(root) {
        generated = matches!(
            fs.entry_type(".lekalo/consumer/bindings", ""),
            Ok(crate::project_fs::EntryType::Directory)
        );
    }
    if binding_generators && !generated {
        return base(
            CheckState::Degraded,
            "not-generated",
            action("regenerate"),
            Vec::new(),
        );
    }
    if binding_generators {
        return base(CheckState::Ok, "present", None, Vec::new());
    }
    base(CheckState::Ok, "none-required", None, Vec::new())
}

/// `artifacts.drift`: the accepted ownership-manifest check, mapped onto
/// the doctor states. Drift degrades with a recipe; it never blocks here.
pub(crate) fn artifacts_drift(selection: &LoadSelection) -> Check {
    let base = |state, reason: &'static str, next: Option<&'static str>, ids: Vec<String>| Check {
        id: "artifacts.drift",
        state,
        required: true,
        reason: Some(reason),
        next_action: next,
        diagnostics: ids,
        notes: Vec::new(),
    };
    match crate::artifacts::GenerateService::check(selection) {
        Ok(receipt) => {
            let blocking = receipt.counts.stale
                + receipt.counts.manual_drift
                + receipt.counts.missing
                + receipt.counts.orphan;
            if blocking > 0 || receipt.verdict == "reported" && receipt.counts.reported > 0 {
                base(
                    CheckState::Degraded,
                    "drift",
                    action("regenerate"),
                    Vec::new(),
                )
            } else if receipt.verdict == "reported" {
                base(
                    CheckState::Degraded,
                    "reported",
                    action("regenerate"),
                    Vec::new(),
                )
            } else {
                base(CheckState::Ok, "clean", None, Vec::new())
            }
        }
        Err(crate::artifacts::ArtifactFailure::Drift(_)) => base(
            CheckState::Degraded,
            "drift",
            action("regenerate"),
            Vec::new(),
        ),
        Err(crate::artifacts::ArtifactFailure::StaleManifest) => base(
            CheckState::Degraded,
            "stale",
            action("regenerate"),
            Vec::new(),
        ),
        Err(crate::artifacts::ArtifactFailure::Lock(crate::lockfile::LockFailure::Missing)) => {
            base(
                CheckState::Unknown,
                "no-lock",
                action("create-lock"),
                Vec::new(),
            )
        }
        Err(crate::artifacts::ArtifactFailure::Lock(crate::lockfile::LockFailure::Stale)) => base(
            CheckState::Degraded,
            "stale-lock",
            action("preview-lock-update"),
            Vec::new(),
        ),
        Err(crate::artifacts::ArtifactFailure::Lock(other)) => {
            let ids = ids_of(&DomainResult::from(&other));
            base(
                CheckState::Blocked,
                "lock-refused",
                action("create-lock"),
                ids,
            )
        }
        Err(crate::artifacts::ArtifactFailure::Loader(result)) => {
            let ids = ids_of(&result);
            base(
                CheckState::Blocked,
                "project-invalid",
                action("fix-structure"),
                ids,
            )
        }
        Err(crate::artifacts::ArtifactFailure::Structure { code, .. }) => base(
            CheckState::Blocked,
            "confinement",
            action("fix-structure"),
            vec![code.to_owned()],
        ),
        Err(crate::artifacts::ArtifactFailure::Io(_)) => base(
            CheckState::Degraded,
            "unreadable",
            action("grant-write"),
            Vec::new(),
        ),
        Err(_) => base(
            CheckState::Blocked,
            "invalid-manifest",
            action("regenerate"),
            Vec::new(),
        ),
    }
}

/// `tools.gates`: the native tool availability (Git, the one external
/// tool Lekalo itself may invoke) handed over by the read-only CLI
/// adapter; the built-in gates ship with this binary.
pub(crate) fn tools_gates(facts: &GitFacts) -> Check {
    let base = |state, reason: &'static str, next: Option<&'static str>| Check {
        id: "tools.gates",
        state,
        required: false,
        reason: Some(reason),
        next_action: next,
        diagnostics: Vec::new(),
        notes: Vec::new(),
    };
    match facts.state {
        GitState::Available => base(CheckState::Ok, "available", None),
        GitState::Unavailable => base(
            CheckState::Degraded,
            "git-unavailable",
            action("install-git"),
        ),
        GitState::NotARepository => {
            base(CheckState::Degraded, "not-a-repository", action("init-git"))
        }
    }
}

/// `platform.limits`: informational, never blocking. The case-sensitivity
/// probe stats the case-flipped project root spelling (read-only); the
/// remaining facts are static per target platform.
pub(crate) fn platform_limits(root: Option<&Path>) -> Check {
    let mut notes: Vec<&'static str> = Vec::new();
    let case = root.and_then(case_sensitivity);
    notes.extend(case);
    if cfg!(target_os = "windows") {
        notes.push("windows-path-syntax");
        notes.push("symlink-privilege-required");
    } else {
        notes.push("posix-path-syntax");
    }
    if cfg!(target_os = "macos") {
        notes.push("unicode-normalization-sensitive");
    }
    notes.sort_unstable();
    notes.dedup();
    Check {
        id: "platform.limits",
        state: CheckState::Ok,
        required: false,
        reason: Some("informational"),
        next_action: None,
        diagnostics: Vec::new(),
        notes,
    }
}

/// The closed probe outcome spellings.
fn case_sensitivity(root: &Path) -> Option<&'static str> {
    let name = root.file_name()?.to_str()?;
    let mut flipped = name.to_owned();
    unsafe {
        let bytes = flipped.as_bytes_mut();
        let changed = bytes.iter_mut().any(|byte| {
            if byte.is_ascii_lowercase() {
                *byte = byte.to_ascii_uppercase();
                true
            } else if byte.is_ascii_uppercase() {
                *byte = byte.to_ascii_lowercase();
                true
            } else {
                false
            }
        });
        if !changed {
            return None;
        }
    }
    let candidate = root.parent()?.join(&flipped);
    if candidate.symlink_metadata().is_ok() {
        Some("case-insensitive-filesystem")
    } else {
        Some("case-sensitive-filesystem")
    }
}

/// `integrations.hlv`: the optional trace-manifest evidence supplied by
/// the caller. Absent evidence degrades and never fails the core.
pub(crate) fn integrations(traces: &[TraceManifestEvidence]) -> Check {
    let base = |state,
                reason: &'static str,
                next: Option<&'static str>,
                ids: Vec<String>,
                notes: Vec<&'static str>| Check {
        id: "integrations.hlv",
        state,
        required: false,
        reason: Some(reason),
        next_action: next,
        diagnostics: ids,
        notes,
    };
    if traces.is_empty() {
        return base(
            CheckState::Degraded,
            "not-supplied",
            action("trace-validate"),
            Vec::new(),
            Vec::new(),
        );
    }
    let mut ids: Vec<String> = Vec::new();
    let mut gaps = 0usize;
    let mut refs: Vec<&'static str> = Vec::new();
    for manifest in traces {
        for id in &manifest.reason_ids {
            if !ids.contains(id) && ids.len() < MAX_CHECK_DIAGNOSTICS {
                ids.push(id.clone());
            }
        }
        gaps += manifest.gaps;
        for reference in &manifest.external_refs {
            let token = match reference.as_str() {
                "openspec" => "ref-openspec",
                "hlv" => "ref-hlv",
                "aifhub" => "ref-aifhub",
                "source-native" => "ref-source-native",
                "lekalo" => "ref-lekalo",
                _ => continue,
            };
            if !refs.contains(&token) {
                refs.push(token);
            }
        }
    }
    refs.sort_unstable();
    if !ids.is_empty() {
        return base(
            CheckState::Blocked,
            "invalid-manifest",
            action("trace-validate"),
            ids,
            refs,
        );
    }
    if gaps > 0 {
        return base(
            CheckState::Degraded,
            "trace-gaps",
            action("trace-validate"),
            Vec::new(),
            refs,
        );
    }
    base(CheckState::Ok, "evidence-current", None, Vec::new(), refs)
}

/// The resolved inputs of one panel; one struct so the panel signature
/// stays closed as the check vocabulary grows.
pub(crate) struct PanelContext<'a> {
    pub selection: &'a LoadSelection,
    pub project_check: Check,
    pub root: Option<&'a Path>,
    pub structure: &'a crate::project_fs::StructureOutcome,
    pub model: &'a ModelOutcome,
    pub facts: &'a LockFacts,
    pub git: &'a GitFacts,
    pub traces: &'a [TraceManifestEvidence],
    pub targets_declared: bool,
    pub kind: super::model::ReportKind,
    pub phase: Option<Phase>,
}

/// The closed check panel of one report kind; readiness marks the
/// phase-required checks. Every check runs read-only.
pub(crate) fn panel(ctx: &PanelContext) -> Vec<Check> {
    let (selection, project_check, root, structure, model, facts, git, traces) = (
        ctx.selection,
        &ctx.project_check,
        ctx.root,
        ctx.structure,
        ctx.model,
        ctx.facts,
        ctx.git,
        ctx.traces,
    );
    let (targets_declared, kind, phase) = (ctx.targets_declared, ctx.kind, ctx.phase);
    let optional = ["cache.health", "integrations.hlv", "platform.limits"];
    let required = |id: &'static str| match kind {
        super::model::ReportKind::Status => false,
        super::model::ReportKind::Doctor => !optional.contains(&id),
        _ => phase
            .map(|phase| phase.required_checks().contains(&id))
            .unwrap_or(false),
    };
    let upstream = |id: &'static str| Check {
        id,
        state: CheckState::Unknown,
        required: required(id),
        reason: Some("upstream-unavailable"),
        next_action: action("fix-structure"),
        diagnostics: Vec::new(),
        notes: Vec::new(),
    };
    if matches!(kind, super::model::ReportKind::Status) {
        // `status` is the freshness panel plus the revisions block.
        let Some(root_path) = root else {
            return vec![
                upstream("artifacts.drift"),
                upstream("bindings.freshness"),
                upstream("cache.health"),
                upstream("lock.freshness"),
            ];
        };
        let mut lock = facts.check.clone();
        lock.required = false;
        let mut cache = cache_health(root_path);
        cache.required = false;
        let mut bindings = bindings_freshness(root_path, &model.references, facts);
        bindings.required = false;
        let mut artifacts = artifacts_drift(selection);
        artifacts.required = false;
        return vec![artifacts, bindings, cache, lock];
    }
    let Some(root_path) = root else {
        return vec![
            upstream("adapters.inventory"),
            upstream("artifacts.drift"),
            upstream("bindings.freshness"),
            upstream("cache.health"),
            upstream("capabilities.profiles"),
            upstream("fs.confinement"),
            upstream("integrations.hlv"),
            upstream("lock.freshness"),
            upstream("model.references"),
            upstream("model.version"),
            Check {
                id: "platform.limits",
                state: CheckState::Ok,
                required: false,
                reason: Some("informational"),
                next_action: None,
                diagnostics: Vec::new(),
                notes: Vec::new(),
            },
            project_check.clone(),
            tools_gates(git),
        ];
    };
    let mut project = project_check.clone();
    project.required = required("project.root");
    let mut confinement = fs_confinement(structure, Some(root_path));
    confinement.required = required("fs.confinement");
    let mut version = model.version.clone();
    version.required = required("model.version");
    let mut references = model.references.clone();
    references.required = required("model.references");
    let mut lock = facts.check.clone();
    lock.required = required("lock.freshness");
    let mut adapters = adapters_inventory(facts, targets_declared);
    adapters.required = required("adapters.inventory");
    let mut capabilities = capabilities_profiles(facts, targets_declared);
    capabilities.required = required("capabilities.profiles");
    let mut cache = cache_health(root_path);
    cache.required = required("cache.health");
    let mut bindings = bindings_freshness(root_path, &references, facts);
    bindings.required = required("bindings.freshness");
    let mut artifacts = artifacts_drift(selection);
    artifacts.required = required("artifacts.drift");
    let mut gates = tools_gates(git);
    gates.required = required("tools.gates");
    let mut integrations = integrations(traces);
    integrations.required = required("integrations.hlv");
    vec![
        adapters,
        artifacts,
        bindings,
        cache,
        capabilities,
        confinement,
        integrations,
        lock,
        references,
        version,
        platform_limits(Some(root_path)),
        project,
        gates,
    ]
}
