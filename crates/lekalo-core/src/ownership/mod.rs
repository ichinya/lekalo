//! Verified ownership handoffs, distinct from metadata-only mode changes.
//! Preparation is read-only. Apply re-derives the plan and executes its oracle
//! itself; a caller-authored verification receipt never grants write authority.
mod diagnostic;
mod manifest;
mod planning;
mod schema;
#[cfg(test)]
mod tests;
mod transaction;
mod verification;
pub mod wire;

pub use crate::adoption::VerificationHost;
use crate::{managed as mg, result::DomainResult};
pub use planning::plan;
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
pub(crate) use transaction::recovery_code;
pub use transaction::{apply, recovery, rollback, trace};
pub use verification::render_verification;
pub use verification::verify;
pub use wire::*;

pub type Outcome<T> = Result<T, DomainResult>;

pub fn fail(reason: &str) -> DomainResult {
    diagnostic::failure(reason)
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Outcome<T> {
    if bytes.len() > MAX_BYTES {
        return Err(fail("ownership-input-bound"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| fail("ownership-utf8"))?;
    let index = crate::loader::source::LineIndex::new(text);
    crate::loader::frontends::json::parse(text, &index).map_err(|_| fail("ownership-json"))?;
    let value = serde_json::from_slice(bytes).map_err(|_| fail("ownership-json"))?;
    schema::validate(&value)?;
    serde_json::from_value(value).map_err(|_| fail("ownership-closed-wire"))
}

pub(crate) fn bytes<T: Serialize>(value: &T) -> Vec<u8> {
    mg::canonical_bytes(value).into_bytes()
}

pub fn render<T: Serialize>(value: &T) -> DomainResult {
    DomainResult::validation(
        serde_json::json!({"status":"valid", "ownership":value}).to_string(),
        serde_json::to_string_pretty(value).expect("typed ownership document"),
        vec![],
    )
}

pub fn parse_request(bytes: &[u8]) -> Outcome<Request> {
    let request: Request = decode(bytes)?;
    if request.schema_version != format!("lekalo/ownership-transition-request/v{VERSION}") {
        return Err(fail("ownership-request-version"));
    }
    Ok(request)
}

pub fn parse_plan(bytes: &[u8]) -> Outcome<TransitionPlan> {
    // Accept the exact CLI envelope saved by `ownership plan --json > plan.json`.
    // Extra envelope fields and non-success reports never become plan authority.
    if bytes.len() > MAX_BYTES {
        return Err(fail("ownership-input-bound"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| fail("ownership-utf8"))?;
    let index = crate::loader::source::LineIndex::new(text);
    crate::loader::frontends::json::parse(text, &index).map_err(|_| fail("ownership-json"))?;
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| fail("ownership-json"))?;
    if let Some(inner) = value.get("ownership") {
        if value.as_object().map_or(0, |row| row.len()) != 2
            || value.get("status").and_then(|row| row.as_str()) != Some("valid")
        {
            return Err(fail("ownership-plan-envelope"));
        }
        return parse_plan(&self::bytes(inner));
    }
    let plan: TransitionPlan = decode(bytes)?;
    if plan.schema_version != format!("lekalo/ownership-transition-plan/v{VERSION}")
        || mg::plan_digest(&plan) != plan.plan_digest
    {
        return Err(fail("ownership-plan-digest"));
    }
    Ok(plan)
}

pub(crate) fn path_ok(path: &str) -> bool {
    let relative = path.strip_prefix(".lekalo/").unwrap_or(path);
    path.len() <= 256
        && mg::rel_path(relative)
        && mg::credential_free(path)
        && !path.split('/').any(|part| matches!(part, ".git" | ".bb"))
}

pub(crate) fn read(root: &Path, path: &str) -> Outcome<Option<Vec<u8>>> {
    if !path_ok(path) {
        return Err(fail("ownership-path"));
    }
    let fs = crate::project_fs::Fs::open(root).map_err(|_| fail("ownership-root"))?;
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    match fs.read_file_opt(dir, name, MAX_BYTES) {
        Ok(value) => Ok(value),
        Err(crate::project_fs::FsErrorKind::NotFound) => Ok(None),
        Err(_) => Err(fail("ownership-read")),
    }
}

pub(crate) fn required(root: &Path, path: &str) -> Outcome<Vec<u8>> {
    read(root, path)?.ok_or_else(|| fail("ownership-input-missing"))
}

pub fn document(root: &Path, path: &str) -> Outcome<Vec<u8>> {
    required(root, path)
}

/// Declared ownership is not a fresh conformance verdict. Consumers retain
/// that distinction rather than interpreting a mode label as a PASS gate.
pub fn status(root: &Path) -> Outcome<OwnershipStatus> {
    if transaction::barrier(root) {
        return Err(fail("ownership-recovery-required"));
    }
    let load = crate::loader::load_validated_root(root.to_path_buf())?;
    let policy = mg::parse_ownership(&required(root, mg::source::PROJECT_PATH)?)?;
    Ok(OwnershipStatus {
        schema_version: format!("lekalo/ownership-status/v{VERSION}"),
        project: load
            .model
            .project
            .as_ref()
            .ok_or_else(|| fail("ownership-project"))?
            .id
            .clone(),
        evidence_basis: "declared-ownership".to_owned(),
        legacy: policy
            .modules
            .into_iter()
            .map(|row| ModeStatus {
                module: row.module,
                mode: row.mode,
            })
            .collect(),
        assignments: state(root)?.assignments,
    })
}

pub(crate) fn state(root: &Path) -> Outcome<OwnershipState> {
    let result: OwnershipState = read(root, STATE)?
        .map(|value| decode(&value))
        .transpose()?
        .unwrap_or_default();
    if result.schema_version != format!("lekalo/ownership-state/v{VERSION}")
        || result.assignments.len() > MAX_FILES
    {
        return Err(fail("ownership-state"));
    }
    let mut scopes = std::collections::BTreeSet::new();
    let mut owners = std::collections::BTreeMap::new();
    for row in &result.assignments {
        if !mg::token(&row.module)
            || row.module.contains('.')
            || !mg::pin(&row.transition)
            || row
                .symbol
                .as_ref()
                .is_some_and(|symbol| !symbol.starts_with(&format!("{}.", row.module)))
            || !scopes.insert((&row.module, &row.symbol))
            || row.artifacts.len() > MAX_FILES
        {
            return Err(fail("ownership-state-scope"));
        }
        for claim in &row.artifacts {
            if !path_ok(&claim.binding.path)
                || !mg::pin(&claim.digest)
                || !claim
                    .binding
                    .symbol
                    .starts_with(&format!("{}.", row.module))
                || row
                    .symbol
                    .as_ref()
                    .is_some_and(|symbol| symbol != &claim.binding.symbol)
                || claim.lifecycle
                    != if row.mode == mg::Mode::Managed {
                        "generated"
                    } else {
                        "scaffolded"
                    }
            {
                return Err(fail("ownership-state-artifact"));
            }
            if owners
                .insert(&claim.binding.path, &claim.binding.symbol)
                .is_some_and(|old| old != &claim.binding.symbol)
            {
                return Err(fail("ownership-state-overlap"));
            }
        }
    }
    Ok(result)
}

/// Reconcile immutable legacy generation lineage with effective scoped
/// handoffs. The public artifact manifest remains the single cleaner input.
pub(crate) fn overlay_manifest(
    root: &Path,
    recorded: &mut std::collections::BTreeMap<String, String>,
    published: &[crate::artifacts::types::ArtifactEntry],
) -> Outcome<()> {
    let state = state(root)?;
    let mut rows: Vec<_> = state.assignments.iter().collect();
    rows.sort_by(|a, b| (&a.module, &a.symbol).cmp(&(&b.module, &b.symbol)));
    let mut effective = std::collections::BTreeMap::new();
    for row in rows {
        for claim in &row.artifacts {
            effective.insert(&claim.binding.path, claim);
        }
    }
    for claim in effective.values() {
        let entry = published
            .iter()
            .find(|entry| entry.key().path().as_str() == claim.binding.path)
            .ok_or_else(|| fail("ownership-manifest-claim-missing"))?;
        if entry.content().as_str() != claim.digest
            || entry.lifecycle().as_str() != claim.lifecycle
            || entry.key().semantic_owner().as_str() != claim.binding.symbol
        {
            return Err(fail("ownership-manifest-claim-stale"));
        }
        recorded.insert(claim.binding.path.clone(), claim.digest.clone());
    }
    Ok(())
}

/// Once a scoped handoff owns a module, legacy module-wide mutation is denied.
/// Historical managed records remain immutable provenance, never cleanup grants.
pub fn legacy_mutation_allowed(root: &Path, module: &str) -> Outcome<()> {
    if transaction::barrier(root) {
        return Err(fail("ownership-recovery-required"));
    }
    if state(root)?
        .assignments
        .iter()
        .any(|row| row.module == module)
    {
        return Err(fail("ownership-use-transition-service"));
    }
    Ok(())
}

pub fn legacy_paths_allowed<'a>(
    root: &Path,
    paths: impl IntoIterator<Item = &'a str>,
) -> Outcome<()> {
    if transaction::barrier(root) {
        return Err(fail("ownership-recovery-required"));
    }
    let state = state(root)?;
    for path in paths {
        if state
            .assignments
            .iter()
            .flat_map(|row| &row.artifacts)
            .any(|claim| claim.binding.path == path)
        {
            return Err(fail("ownership-use-transition-service"));
        }
    }
    Ok(())
}

pub(crate) fn revalidate(root: &Path, saved: &TransitionPlan) -> Outcome<()> {
    for pin in &saved.inputs {
        if read(root, &pin.path)?.as_deref().map(mg::digest) != pin.digest {
            return Err(fail("ownership-plan-stale"));
        }
    }
    let current = plan(root, saved.request.clone())?;
    if current != *saved {
        return Err(fail("ownership-plan-stale"));
    }
    Ok(())
}
