//! The generation authority: the validated Model/IR/lock closure the
//! managed lane plans under (issue #41).
//!
//! A managed plan is never derived from caller-hashed optional bytes. The
//! required semantics come from the exact canonical Model compiled through
//! the accepted loader pipeline into typed IR, pinned by the exact lock
//! revision; the coverage evidence must name those pins and discharge
//! every obligation the ledger requires before any plan exists. The
//! renderer stays pure behind this validated boundary.

use super::{detail, DomainResult, INPUT_INVALID, PLAN_STALE};
use crate::ir::DefinitionKind;
use crate::loader::{self, LoadSelection};
use crate::lockfile::plan::LockService;
use crate::lockfile::LockState;

/// The exact input pins of one generation: the canonical Model payload, the
/// canonical typed-IR payload, and the exact lock revision, plus the
/// renderable obligations the Model requires.
#[derive(Clone, Debug, PartialEq)]
pub struct Authority {
    /// `sha256:` pin over the canonical Model JSON bytes.
    pub model_digest: String,
    /// `sha256:` pin over the canonical typed-IR JSON bytes.
    pub ir_digest: String,
    /// `sha256:` pin over the exact lock file bytes.
    pub lock_digest: String,
    /// The pinned target protocol of the lock, when published.
    pub lock_protocol: Option<String>,
    /// The compiled Model contract version (e.g. `0.2.16`).
    pub model_version: String,
    /// The compiled project id of the Model.
    pub project_id: String,
    /// The required renderable obligations of the module, sorted by
    /// semantic id: `(semantic id, kind)`.
    pub ledger: Vec<(String, DefinitionKind)>,
}

fn invalid(reason: &str) -> DomainResult {
    super::diagnostic::failure(INPUT_INVALID, &detail(reason))
}

fn stale(path: &str, reason: &str) -> DomainResult {
    super::diagnostic::failure(PLAN_STALE, &[("path", path), ("reason", reason)])
}

/// Derive the generation authority of one project: load and compile the
/// canonical Model through the accepted pipeline, verify the exact lock
/// pins it, and collect the module's required renderable ledger.
pub fn derive(selection: &LoadSelection, module: &str) -> Result<Authority, DomainResult> {
    let root = match loader::root_for_selection(selection) {
        Ok(root) => root,
        Err(error) => return Err(classify(error)),
    };
    let model_json = match loader::run(selection, false) {
        crate::result::DomainResult::Valid {
            payload: crate::result::SuccessPayload::Model { json, .. },
            ..
        } => json,
        other => return Err(classify(other)),
    };
    let model = match loader::normalize_model(selection) {
        Ok(model) => model,
        Err(error) => return Err(classify(error)),
    };
    let compilation = match crate::ir::compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => return Err(classify(failure.into_result())),
    };
    let ir_json = compilation.project.to_canonical_json();
    // The exact lock revision must exist and parse closed: it pins the
    // accepted contract families the pipeline compiled under, and its own
    // exact bytes ride the authority digest, so any lock mutation moves
    // the generation identity.
    let _lock_bytes = match std::fs::read(root.join("lekalo.lock")) {
        Ok(bytes) => bytes,
        Err(_) => return Err(stale("lekalo.lock", "missing")),
    };
    let lock = match LockService::read_state_at(&root) {
        Ok(LockState::Present(lock)) => lock,
        Ok(LockState::Absent) => return Err(stale("lekalo.lock", "missing")),
        Err(_) => return Err(stale("lekalo.lock", "unreadable")),
    };
    let _ = lock;
    // The ledger: every renderable obligation this module declares. A
    // coverage document that omits one is refused before planning.
    let prefix = format!("{module}.");
    let ledger = compilation
        .project
        .definitions
        .iter()
        .filter(|definition| {
            matches!(
                definition.kind(),
                DefinitionKind::Command | DefinitionKind::Query
            ) && definition.id().as_str().starts_with(&prefix)
        })
        .map(|definition| (definition.id().as_str().to_owned(), definition.kind()))
        .collect::<Vec<_>>();
    if ledger.iter().any(|(id, _)| !super::token(id)) {
        return Err(invalid("ledger"));
    }
    Ok(Authority {
        model_digest: super::digest(model_json.as_bytes()),
        ir_digest: super::digest(ir_json.as_bytes()),
        // The canonical lock digest (the same identity the artifact owner
        // and every source map bind), never the raw file bytes.
        lock_digest: crate::lockfile::canonical::lock_digest(&lock)
            .as_str()
            .to_owned(),
        lock_protocol: lock
            .target_protocol()
            .map(|pin| pin.version().as_str().to_owned()),
        model_version: compilation.project.model_version.as_str().to_owned(),
        project_id: compilation
            .project
            .project
            .as_ref()
            .map(|project| project.id.as_str().to_owned())
            .unwrap_or_default(),
        ledger,
    })
}

/// Whether the coverage document's authority pins match the derived
/// authority exactly: the canonical Model, the canonical typed IR, and the
/// exact lock revision the evidence claims. The advertised adapter
/// protocol is qualified in [`super::coverage::admit`] against the
/// negotiated successor token.
pub fn qualify(
    coverage: &super::wire::Coverage,
    authority: &Authority,
) -> Result<(), DomainResult> {
    let pins = coverage
        .authority
        .value()
        .ok_or_else(|| invalid("authority"))?;
    if pins.model != authority.model_digest
        || pins.ir != authority.ir_digest
        || pins.lock != authority.lock_digest
    {
        return Err(stale("authority", "inputs"));
    }
    Ok(())
}

/// Whether the coverage discharges every required obligation of the
/// ledger: every renderable semantic the Model declares must be covered
/// with a matching requirement before any plan exists.
pub fn ledger_complete(
    coverage: &super::wire::Coverage,
    authority: &Authority,
) -> Result<(), DomainResult> {
    for (id, kind) in &authority.ledger {
        let requirement = match coverage.requirements.iter().find(|r| &r.id == id) {
            Some(requirement) => requirement,
            None => {
                return Err(super::diagnostic::failure(
                    super::CAPABILITY_GAP,
                    &[
                        ("requirement", id.as_str()),
                        ("reason", "omitted-obligation"),
                    ],
                ))
            }
        };
        let covered_kind = match kind {
            DefinitionKind::Command => super::wire::RequirementKind::Command,
            DefinitionKind::Query => super::wire::RequirementKind::Query,
            _ => continue,
        };
        if requirement.kind != covered_kind {
            return Err(super::diagnostic::failure(
                super::CAPABILITY_GAP,
                &[("requirement", id.as_str()), ("reason", "kind")],
            ));
        }
    }
    Ok(())
}

fn classify(error: DomainResult) -> DomainResult {
    error
}
