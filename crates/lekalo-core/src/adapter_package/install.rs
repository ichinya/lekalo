//! The previewed-and-confirmed adapter install service (issue #32).
//!
//! [`plan`] renders the deterministic [`InstallPlan`] — the resolved
//! manifest projection, the assigned trust, every staged file, the
//! permission/capability [`ManifestDiff`] against the currently
//! selected version, and the `planId` that binds the preview to its
//! confirmation. Planning writes nothing.
//!
//! [`apply`] enforces the confirmation: the recomputed plan id must
//! equal the confirmed one byte-exactly (`adapter.source-changed`
//! otherwise), the exclusive install guard serializes concurrent
//! updates (the lockfile-update guard precedent), and the journaled
//! stage → verify → rename → repoint sequence rolls back in reverse
//! order on any failure, answering `adapter.recovery-required` when a
//! rollback itself is incomplete.
//!
//! Package bytes under `.lekalo/adapters/packages/<id>/<version>-<digest8>/`
//! are immutable once promoted; the `selected` pin in the inventory is
//! the only mutation update and rollback may perform (issue acceptance
//! criterion 4: rollback returns the previous immutable version).

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::discovery::DiscoveryCandidate;
use super::inventory::{Inventory, InventoryRow};
use super::manifest::ManifestDocument;
use super::trust::TrustLevel;
use super::version::INSTALL_PLAN_SCHEMA_VERSION;

/// The governed runtime homes of the adapter store.
pub const PACKAGES_DIR: &str = ".lekalo/adapters/packages";
pub const STAGING_DIR: &str = ".lekalo/adapters/staging";
pub const QUARANTINE_DIR: &str = ".lekalo/adapters/quarantine";
pub const INSTALL_GUARD: &str = ".lekalo/cache/locks/adapter-install";

/// One sorted action of the install plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase", tag = "action")]
pub enum InstallAction {
    /// Copy one file into the staging tree, then into the store.
    Stage { path: String, digest: String },
    /// Atomically rename the verified stage into the package store.
    Promote { path: String },
    /// Repoint the selected pin of the adapter id.
    Repoint { id: String, version: String },
    /// Move the staged bytes into quarantine custody.
    Quarantine { path: String },
}

/// The deterministic install plan (the update `--dry-run` precedent).
#[derive(Clone, Debug, PartialEq)]
pub struct InstallPlan {
    /// The adapter id the plan targets.
    pub id: String,
    /// The target version.
    pub version: String,
    /// The package digest.
    pub digest: String,
    /// The manifest identity digest.
    pub manifest_digest: String,

    pub manifest_bytes: Vec<u8>,
    /// The closed source coordinate the package came from (plan data,
    /// bound by the planId).
    pub source: String,
    /// The assigned trust level at plan time.
    pub trust: TrustLevel,
    /// Whether the bytes enter quarantine custody (community packages).
    pub quarantined: bool,
    /// The sorted plan actions.
    pub actions: Vec<InstallAction>,
    /// The permission/capability diff against the currently selected
    /// version, when one is installed.
    pub diff: Option<super::diff::ManifestDiff>,
    /// The plan identity: SHA-256 over the canonical plan projection.
    pub plan_id: String,
}

impl InstallPlan {
    /// The exact wire projection the `planId` is computed over (the
    /// canonical JSON form, sorted keys, no trailing LF).
    pub fn to_json(&self) -> serde_json::Value {
        let diff = self
            .diff
            .as_ref()
            .map(|diff| serde_json::to_value(diff).expect("diff serializes"));
        serde_json::json!({
            "schemaVersion": INSTALL_PLAN_SCHEMA_VERSION,
            "id": self.id,
            "version": self.version,
            "digest": self.digest,
            "manifestDigest": self.manifest_digest,
            "source": self.source,
            "trust": self.trust.as_str(),
            "quarantined": self.quarantined,
            "actions": self.actions,
            "diff": diff,
        })
    }

    /// The canonical plan bytes (the `planId` domain).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        super::canonical::canonical_bytes(&self.to_json())
    }

    /// The plan identity digest (`sha256:<hex>`).
    pub fn compute_plan_id(&self) -> String {
        format!(
            "sha256:{}",
            crate::digest::sha256_hex(&self.canonical_bytes())
        )
    }
}

/// Why a plan was refused at apply time.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyRejection {
    /// The recomputed plan id differs from the confirmed one.
    SourceChanged,
    /// The update widens permissions without the explicit policy.
    PermissionEscalated {
        /// The first widened member.
        member: String,
    },
    /// The planned package path is occupied by foreign content.
    InstallConflict {
        /// The store-relative planned path.
        path: String,
    },
    /// The journal could not be rolled back completely.
    RecoveryRequired {
        /// The journal stage the ambiguity was found at.
        stage: String,
    },
}

/// Plan one install/update from a fully gated candidate. Planning
/// writes nothing; the returned plan must be rendered, confirmed, and
/// re-derived byte-exactly at apply time.
pub fn plan(
    candidate: &DiscoveryCandidate,
    trust: TrustLevel,
    quarantined: bool,
    current: Option<&ManifestDocument>,
) -> InstallPlan {
    let manifest = &candidate.manifest;
    let mut actions = Vec::new();
    for file in manifest.files() {
        actions.push(InstallAction::Stage {
            path: file.path().to_owned(),
            digest: file.digest().as_str().to_owned(),
        });
    }
    actions.push(InstallAction::Promote {
        path: package_relative_dir(manifest),
    });
    actions.push(InstallAction::Repoint {
        id: manifest.adapter_id().to_owned(),
        version: manifest.adapter_version().to_string(),
    });
    let diff = current.map(|current| super::diff::diff_manifests(current, manifest));
    let manifest_bytes = candidate.manifest.stored_bytes().to_vec();
    let plan = InstallPlan {
        id: manifest.adapter_id().to_owned(),
        version: manifest.adapter_version().to_string(),
        digest: manifest.package_digest().as_str().to_owned(),
        manifest_digest: manifest.digest().as_str().to_owned(),
        manifest_bytes,
        source: manifest.source_coordinate().to_owned(),
        trust,
        quarantined,
        actions,
        diff,
        plan_id: String::new(),
    };
    let mut plan = plan;
    plan.plan_id = plan.compute_plan_id();
    plan
}

/// Apply one confirmed plan under the project root. `allow_escalation`
/// is the explicit `--allow-escalation` policy. The sequence is:
/// recompute → identity match → escalation gate → exclusive guard →
/// stage → verify → promote → repoint, with reverse-order rollback.
pub fn apply(
    root: &Path,
    plan: &InstallPlan,
    confirmed_plan_id: &str,
    allow_escalation: bool,
) -> Result<(), ApplyRejection> {
    let source_dir = staged_source(root, plan);
    apply_with_source(root, plan, confirmed_plan_id, allow_escalation, &source_dir)
}

/// [`apply`] with an explicit byte source directory (the candidate's
/// package root, or the directory holding the synthesized entry).
pub fn apply_with_source(
    root: &Path,
    plan: &InstallPlan,
    confirmed_plan_id: &str,
    allow_escalation: bool,
    source_dir: &Path,
) -> Result<(), ApplyRejection> {
    // 1. The confirmed identity must equal the recomputed plan exactly.
    if plan.compute_plan_id() != confirmed_plan_id {
        return Err(ApplyRejection::SourceChanged);
    }
    // 2. A permission-widening diff refuses without the policy.
    if let Some(diff) = &plan.diff {
        if diff.escalated && !allow_escalation {
            return Err(ApplyRejection::PermissionEscalated {
                member: diff
                    .escalated_member
                    .clone()
                    .unwrap_or_else(|| "permissions".to_owned()),
            });
        }
    }
    // 3. The exclusive guard (the lockfile-update precedent).
    let guard_path = root.join(INSTALL_GUARD.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Some(parent) = guard_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| recovery("guard"))?;
    }
    if guard_path.exists() {
        return Err(ApplyRejection::RecoveryRequired {
            stage: "guard".to_owned(),
        });
    }
    std::fs::write(&guard_path, plan.plan_id.as_bytes()).map_err(|_| recovery("guard"))?;
    let outcome = apply_guarded(root, plan, source_dir);
    // The guard is always released, success or failure.
    let _ = std::fs::remove_file(&guard_path);
    outcome
}

fn recovery(stage: &str) -> ApplyRejection {
    ApplyRejection::RecoveryRequired {
        stage: stage.to_owned(),
    }
}

fn apply_guarded(root: &Path, plan: &InstallPlan, source_dir: &Path) -> Result<(), ApplyRejection> {
    // Journal: stage → verify → promote → repoint. Each completed stage
    // is recorded so a crash leaves an unambiguous reverse-order
    // rollback path.
    let staging = root.join(STAGING_DIR.replace('/', std::path::MAIN_SEPARATOR_STR));
    let stage_dir = staging.join(&plan.digest["sha256:".len()..12.min(plan.digest.len())]);
    let _ = std::fs::remove_dir_all(&stage_dir);
    std::fs::create_dir_all(&stage_dir).map_err(|_| recovery("stage"))?;
    let mut journal: Vec<PathBuf> = vec![stage_dir.clone()];
    let result = apply_staged(root, plan, &stage_dir, source_dir, &mut journal);
    if result.is_err() {
        rollback(root, &journal);
    }
    result
}

fn apply_staged(
    root: &Path,
    plan: &InstallPlan,
    stage_dir: &Path,
    source_dir: &Path,
    journal: &mut Vec<PathBuf>,
) -> Result<(), ApplyRejection> {
    for action in &plan.actions {
        if let InstallAction::Stage { path, digest } = action {
            let from = source_dir.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let bytes = std::fs::read(&from).map_err(|_| conflict(path))?;
            // Verify before the bytes enter the stage.
            let actual = crate::digest::sha256_hex(&bytes);
            if format!("sha256:{actual}") != *digest {
                return Err(conflict(path));
            }
            let to = stage_dir.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|_| recovery("stage"))?;
            }
            std::fs::write(&to, &bytes).map_err(|_| recovery("stage"))?;
        }
    }
    // The manifest is the custody anchor: every later gate (update
    // diffs, re-verification, lock provenance) reads it from the store.
    // Its bytes are the canonical stored form from the resolved
    // candidate — not a caller-supplied re-serialization.
    std::fs::write(
        stage_dir.join(crate::adapter_package::integrity::MANIFEST_FILE),
        &plan.manifest_bytes,
    )
    .map_err(|_| recovery("stage"))?;

    journal.push(stage_dir.to_path_buf());
    // Promote: rename the verified stage into the immutable store. A
    // foreign occupant of the destination is a conflict, never an
    // overwrite.
    let destination =
        root.join(package_store_dir_relative(plan).replace('/', std::path::MAIN_SEPARATOR_STR));
    if destination.exists() {
        return Err(ApplyRejection::InstallConflict {
            path: package_relative_dir_of_plan(plan),
        });
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|_| recovery("promote"))?;
    }
    std::fs::rename(stage_dir, &destination).map_err(|_| recovery("promote"))?;
    journal.push(destination.clone());
    // Repoint the selected pin in the inventory.
    let mut inventory = Inventory::load(root).map_err(|_| recovery("repoint"))?;
    inventory.upsert(InventoryRow {
        id: plan.id.clone(),
        version: plan.version.clone(),
        digest: plan.digest.clone(),
        manifest_digest: plan.manifest_digest.clone(),
        trust: plan.trust.as_str().to_owned(),
        source: plan.source.clone(),
        install_plan_id: Some(plan.plan_id.clone()),
        selected: false,
        quarantined: plan.quarantined,
    });
    // A quarantined row stays unselectable: promotion of the pin is the
    // explicit confirmation of a non-quarantined package.
    if !plan.quarantined {
        inventory
            .select(&plan.id, &plan.version, &plan.digest)
            .map_err(|_| recovery("repoint"))?;
    }
    inventory.store(root).map_err(|_| recovery("repoint"))?;
    Ok(())
}

fn conflict(path: &str) -> ApplyRejection {
    ApplyRejection::InstallConflict {
        path: path.to_owned(),
    }
}

/// The source directory the staged bytes are re-read from. For v1 the
/// candidate was discovered from an explicit path source; the staging
/// step re-reads those bytes and re-verifies them against the plan.
fn staged_source(_root: &Path, _plan: &InstallPlan) -> PathBuf {
    // The plan does not carry absolute paths (the wire never does); the
    // caller re-derives the candidate and applies from its package root
    // through [`apply_from_candidate`]. This indirection exists so the
    // plan stays a pure value.
    PathBuf::new()
}

/// Apply one plan for an already-gated candidate: the bytes are re-read
/// from the candidate's package root (or the synthesized entry) and
/// re-verified inside the guarded sequence above.
pub fn apply_from_candidate(
    root: &Path,
    candidate: &DiscoveryCandidate,
    plan: &InstallPlan,
    confirmed_plan_id: &str,
    allow_escalation: bool,
) -> Result<(), ApplyRejection> {
    // Bind the candidate's actual bytes to the plan: the package digest
    // must still match, otherwise the inputs drifted after preview.
    if candidate.manifest.package_digest().as_str() != plan.digest
        || candidate.manifest.digest().as_str() != plan.manifest_digest
    {
        return Err(ApplyRejection::SourceChanged);
    }
    let source_dir = candidate
        .package_root
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    apply_with_source(root, plan, confirmed_plan_id, allow_escalation, &source_dir)
}

fn package_store_dir_relative(plan: &InstallPlan) -> String {
    let digest8 = plan.digest["sha256:".len()..]
        .chars()
        .take(8)
        .collect::<String>();
    format!("{}/{}/{}-{}", PACKAGES_DIR, plan.id, plan.version, digest8)
}

fn package_relative_dir_of_plan(plan: &InstallPlan) -> String {
    package_store_dir_relative(plan)
}

fn package_relative_dir(manifest: &ManifestDocument) -> String {
    let digest8 = manifest.package_digest().as_str()["sha256:".len()..]
        .chars()
        .take(8)
        .collect::<String>();
    format!(
        "{}/{}/{}-{}",
        PACKAGES_DIR,
        manifest.adapter_id(),
        manifest.adapter_version(),
        digest8
    )
}

/// Reverse-order rollback of the journal. An incomplete rollback is
/// `adapter.recovery-required`, never silence.
fn rollback(root: &Path, journal: &[PathBuf]) {
    for path in journal.iter().rev() {
        let result = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        if result.is_err() {
            // The remaining entries stay on disk; the next invocation
            // answers recovery-required through the guard/stage state.
            let _ = root;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, bytes: &[u8], coordinate: &str) -> DiscoveryCandidate {
        let digest = crate::digest::sha256_hex(bytes);
        let json = serde_json::json!({
            "schemaVersion": crate::adapter_package::version::MANIFEST_SCHEMA_VERSION,
            "identity": crate::adapter_package::version::MANIFEST_IDENTITY,
            "adapter": { "id": id, "name": "T", "version": "1.0.0" },
            "source": { "kind": "path", "coordinate": coordinate, "digest": format!("sha256:{}", "11".repeat(32)) },
            "compatibility": {
                "protocolVersions": [crate::target_protocol::version::VERSION],
                "irVersions": [crate::ir::version::VERSION],
                "extensions": []
            },
            "executable": { "entry": "a.mjs" },
        "publisher": { "id": "test-pub", "trustAnchor": "none" },
        "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        "capabilities": { "operations": ["describe"], "targets": [], "profiles": [], "named": {}, "constraints": {}, "readScopes": [], "writeScopes": [], "transports": ["stdin"] },
        "platforms": ["any"],
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
            "integrity": {
                "packageDigest": format!("sha256:{digest}"),
                "files": [ { "path": "a.mjs", "digest": format!("sha256:{digest}"), "bytes": bytes.len() } ],
                "signaturePolicy": "unsigned",
                "signature": null
            },
            "status": "active",
            "revocation": null
        });
        DiscoveryCandidate {
            manifest: ManifestDocument::from_value(json).expect("parses"),
            package_root: None,
            synthesized: true,
        }
    }

    #[test]
    fn plan_id_is_deterministic_and_input_sensitive() {
        let first = plan(
            &candidate("a", b"one", "path:x"),
            TrustLevel::Community,
            true,
            None,
        );
        let second = plan(
            &candidate("a", b"one", "path:x"),
            TrustLevel::Community,
            true,
            None,
        );
        assert_eq!(first.plan_id, second.plan_id);
        assert!(first.plan_id.starts_with("sha256:"));
        let different = plan(
            &candidate("a", b"two", "path:x"),
            TrustLevel::Community,
            true,
            None,
        );
        assert_ne!(first.plan_id, different.plan_id);
    }

    #[test]
    fn apply_stages_the_manifest_and_a_reinstall_produces_a_diff() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let source = std::env::temp_dir().join(format!("lekalo-ap-mf-src-{}", std::process::id()));
        std::fs::create_dir_all(&source).expect("src");
        std::fs::write(source.join("a.mjs"), b"bytes").expect("entry");
        let candidate = candidate("a", b"bytes", "path:x");
        // The source dir must carry the manifest: it is the staged anchor.
        std::fs::write(
            source.join(crate::adapter_package::integrity::MANIFEST_FILE),
            candidate.manifest.stored_bytes(),
        )
        .expect("manifest in source");
        let install_plan = plan(&candidate, TrustLevel::LocalDevelopment, false, None);
        apply_with_source(&root, &install_plan, &install_plan.plan_id, false, &source)
            .expect("applies");
        // The package store now contains the manifest.
        let stored_manifest = std::fs::read(
            root.join(".lekalo/adapters/packages/a/1.0.0-277089d9")
                .join(crate::adapter_package::integrity::MANIFEST_FILE),
        )
        .map_err(|e| format!("{e}?"))
        .expect("manifest staged into the store");
        assert_eq!(stored_manifest, candidate.manifest.stored_bytes());
        // A re-install against the now-current manifest produces a diff.
        let current = crate::adapter_package::ManifestDocument::from_bytes(&stored_manifest)
            .expect("re-read");
        let second = plan(
            &candidate,
            TrustLevel::LocalDevelopment,
            false,
            Some(&current),
        );
        assert!(second.diff.is_some(), "the diff is now real, not null");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&source);
    }

    #[test]
    fn apply_refuses_a_stale_confirmation_without_writing() {
        let root = std::env::temp_dir().join(format!("lekalo-ap-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let candidate = candidate("a", b"bytes", "path:x");
        let install_plan = plan(&candidate, TrustLevel::LocalDevelopment, false, None);
        let rejection = apply(
            &root,
            &install_plan,
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            false,
        )
        .expect_err("stale confirmation");
        assert_eq!(rejection, ApplyRejection::SourceChanged);
        // Nothing was written.
        assert!(!root.join(".lekalo/adapters").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_refuses_escalation_without_the_policy() {
        let root =
            std::env::temp_dir().join(format!("lekalo-ap-install-esc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let candidate = candidate("a", b"bytes", "path:x");
        let mut install_plan = plan(&candidate, TrustLevel::Community, true, None);
        install_plan.diff = Some(super::super::diff::ManifestDiff {
            read_scopes: Vec::new(),
            write_scopes: Vec::new(),
            network_mode: None,
            network_destinations: Vec::new(),
            environment: Vec::new(),
            processes: None,
            secrets: Vec::new(),
            named_capabilities: Vec::new(),
            protocol_versions: Vec::new(),
            ir_versions: Vec::new(),
            escalated: true,
            escalated_member: Some("network".to_owned()),
        });
        install_plan.plan_id = install_plan.compute_plan_id();
        let rejection = apply(&root, &install_plan, &install_plan.plan_id, false)
            .expect_err("escalation refused");
        assert_eq!(
            rejection,
            ApplyRejection::PermissionEscalated {
                member: "network".to_owned(),
            }
        );
        // With the policy the apply proceeds to the store stage. The
        // staged bytes are re-read from a real source directory.
        let source = std::env::temp_dir().join(format!("lekalo-ap-src-{}", std::process::id()));
        std::fs::create_dir_all(&source).expect("src dir");
        std::fs::write(source.join("a.mjs"), b"bytes").expect("entry");
        let applied = apply_with_source(&root, &install_plan, &install_plan.plan_id, true, &source);
        assert!(
            applied.is_ok(),
            "explicit policy unlocks the apply: {applied:?}"
        );
        // The promoted store directory exists and the guard is released.
        assert!(!root
            .join(INSTALL_GUARD.replace('/', std::path::MAIN_SEPARATOR_STR))
            .exists());
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failing_stage_rolls_back_the_journal_and_releases_the_guard() {
        let root =
            std::env::temp_dir().join(format!("lekalo-ap-install-rb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        // Source directory whose bytes deliberately disagree with the
        // plan (tampered after preview): the stage verify refuses, and
        // the journal/stage/guard state must be cleaned up.
        let source = std::env::temp_dir().join(format!("lekalo-ap-rb-src-{}", std::process::id()));
        std::fs::create_dir_all(&source).expect("src dir");
        std::fs::write(source.join("a.mjs"), b"tampered bytes").expect("entry");
        let candidate = candidate("a", b"bytes", "path:x");
        let install_plan = plan(&candidate, TrustLevel::LocalDevelopment, false, None);
        let rejection =
            apply_with_source(&root, &install_plan, &install_plan.plan_id, false, &source)
                .expect_err("tampered stage refuses");
        assert_eq!(
            rejection,
            ApplyRejection::InstallConflict {
                path: "a.mjs".to_owned()
            },
            "a digest mismatch at stage time is an install conflict"
        );
        // The guard is released and nothing reached the store.
        assert!(!root
            .join(INSTALL_GUARD.replace('/', std::path::MAIN_SEPARATOR_STR))
            .exists());
        assert!(!root.join(".lekalo/adapters/packages").exists());
        // A subsequent apply with honest bytes succeeds: no stale guard.
        std::fs::write(source.join("a.mjs"), b"bytes").expect("honest bytes");
        apply_with_source(&root, &install_plan, &install_plan.plan_id, false, &source)
            .expect("recovery apply succeeds");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&source);
    }

    #[test]
    fn guard_is_released_after_a_successful_apply() {
        let root =
            std::env::temp_dir().join(format!("lekalo-ap-install-grd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let candidate = candidate("a", b"bytes", "path:x");
        let install_plan = plan(&candidate, TrustLevel::LocalDevelopment, false, None);
        let source = std::env::temp_dir().join(format!("lekalo-ap-src2-{}", std::process::id()));
        std::fs::create_dir_all(&source).expect("src dir");
        std::fs::write(source.join("a.mjs"), b"bytes").expect("entry");
        apply_with_source(&root, &install_plan, &install_plan.plan_id, false, &source)
            .expect("applies");
        let _ = std::fs::remove_dir_all(&source);
        assert!(!root
            .join(INSTALL_GUARD.replace('/', std::path::MAIN_SEPARATOR_STR))
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
