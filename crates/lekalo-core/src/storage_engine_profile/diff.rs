//! Pure semantic comparison of two same-family storage-engine profiles
//! (issue #117).
//!
//! The comparison answers two questions per changed path: which surface
//! changed — **capability**, **engine** (the identity block), **test**
//! (the lifecycle section), or **wire** (the contract envelope) — and
//! one closed compatibility class: **breaking** (a declared guarantee
//! was removed or narrowed), **non-breaking** (an addition or a
//! widening), or **policy-change** (an explicit owner decision, such as
//! an evidence-kind change). A capability downgrading `full → partial`
//! or `partial → unsupported` is breaking; upgrades are non-breaking;
//! an evidence-reference change alone is policy-change. Foreign
//! projects are the typed error set, never a guessed classification.
//! Paths are deterministic and byte-sorted.

use super::portability::ALL_IDS;
use super::{diagnostic, StorageEngineProfile, Support};
use crate::diagnostics::DiagnosticSet;

/// The closed surface of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffLayer {
    /// The capability map.
    Capability,
    /// The engine identity block.
    Engine,
    /// The test lifecycle section.
    Test,
    /// The contract envelope.
    Wire,
}

impl DiffLayer {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Capability => "capability",
            Self::Engine => "engine",
            Self::Test => "test",
            Self::Wire => "wire",
        }
    }
}

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or narrowed.
    Breaking,
    /// An addition or widening under the evolution policy.
    NonBreaking,
    /// An explicit owner decision with unchanged guarantees.
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

/// One changed path with its surface and class.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    layer: DiffLayer,
    class: DiffClass,
}

impl DiffPath {
    /// The canonical path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The changed surface.
    pub const fn layer(&self) -> DiffLayer {
        self.layer
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
    /// Whether the two profiles are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths, byte-sorted.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }
}

/// Compare two same-family profiles. Pure and read-only.
pub fn compare(
    base: &StorageEngineProfile,
    candidate: &StorageEngineProfile,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::diff_invalid("diff-project-mismatch"));
    }
    let mut paths: Vec<DiffPath> = Vec::new();
    if base.attachment_revision().as_str() != candidate.attachment_revision().as_str() {
        push(
            &mut paths,
            "wire/attachmentRevision",
            DiffLayer::Wire,
            DiffClass::PolicyChange,
        );
    }
    if base.model_ref().version().as_str() != candidate.model_ref().version().as_str()
        || base.model_ref().digest().as_str() != candidate.model_ref().digest().as_str()
    {
        push(
            &mut paths,
            "wire/modelRef",
            DiffLayer::Wire,
            DiffClass::PolicyChange,
        );
    }
    if base.ir_digest().as_str() != candidate.ir_digest().as_str() {
        push(
            &mut paths,
            "wire/irRef",
            DiffLayer::Wire,
            DiffClass::PolicyChange,
        );
    }
    compare_engine(base, candidate, &mut paths);
    compare_capabilities(base, candidate, &mut paths);
    compare_test_lifecycle(base, candidate, &mut paths);
    paths.sort();
    Ok(DiffResult {
        equal: paths.is_empty(),
        paths,
    })
}

/// Record one changed path.
fn push(paths: &mut Vec<DiffPath>, path: &str, layer: DiffLayer, class: DiffClass) {
    paths.push(DiffPath {
        path: path.to_owned(),
        layer,
        class,
    });
}

/// The engine identity comparison: any member change is a policy
/// change except the engine token and the exact version, whose change
/// is breaking (a different engine generation is a different contract).
fn compare_engine(
    base: &StorageEngineProfile,
    candidate: &StorageEngineProfile,
    paths: &mut Vec<DiffPath>,
) {
    let base_engine = base.engine();
    let candidate_engine = candidate.engine();
    if base_engine.engine() != candidate_engine.engine()
        || base_engine.engine_version() != candidate_engine.engine_version()
    {
        push(
            paths,
            "engine/identity",
            DiffLayer::Engine,
            DiffClass::Breaking,
        );
        return;
    }
    if base_engine.variant() != candidate_engine.variant() {
        push(
            paths,
            "engine/variant",
            DiffLayer::Engine,
            DiffClass::Breaking,
        );
    }
    if base_engine.sql_mode() != candidate_engine.sql_mode() {
        push(
            paths,
            "engine/sqlMode",
            DiffLayer::Engine,
            DiffClass::PolicyChange,
        );
    }
    if base_engine.default_storage_engine() != candidate_engine.default_storage_engine() {
        push(
            paths,
            "engine/defaultStorageEngine",
            DiffLayer::Engine,
            DiffClass::PolicyChange,
        );
    }
    if base_engine.charset() != candidate_engine.charset()
        || base_engine.collation() != candidate_engine.collation()
    {
        push(
            paths,
            "engine/charsetCollation",
            DiffLayer::Engine,
            DiffClass::PolicyChange,
        );
    }
    if base_engine.time_zone() != candidate_engine.time_zone() {
        push(
            paths,
            "engine/timeZone",
            DiffLayer::Engine,
            DiffClass::PolicyChange,
        );
    }
}

/// The support ordering: narrower is a left-to-candidate downgrade.
fn support_rank(support: Support) -> u8 {
    match support {
        Support::Full => 2,
        Support::Partial => 1,
        Support::Unsupported => 0,
    }
}

/// The capability map comparison. Absence is `unknown`: a removed
/// record is a downgrade, an added record is an upgrade.
fn compare_capabilities(
    base: &StorageEngineProfile,
    candidate: &StorageEngineProfile,
    paths: &mut Vec<DiffPath>,
) {
    for id in ALL_IDS {
        let base_support = base.capability(id).map(|capability| capability.support());
        let candidate_support = candidate
            .capability(id)
            .map(|capability| capability.support());
        if base_support == candidate_support {
            continue;
        }
        let base_rank = base_support.map(support_rank).unwrap_or(u8::MAX);
        let candidate_rank = candidate_support.map(support_rank).unwrap_or(u8::MAX);
        let class = if candidate_rank > base_rank {
            DiffClass::NonBreaking
        } else {
            DiffClass::Breaking
        };
        push(
            paths,
            &format!("capability/{}", id.key()),
            DiffLayer::Capability,
            class,
        );
    }
}

/// The test lifecycle comparison: prefix or capability narrowing is
/// breaking, evidence-only changes are policy.
fn compare_test_lifecycle(
    base: &StorageEngineProfile,
    candidate: &StorageEngineProfile,
    paths: &mut Vec<DiffPath>,
) {
    let base_lifecycle = base.test_lifecycle();
    let candidate_lifecycle = candidate.test_lifecycle();
    if base_lifecycle.test_schema_prefix() != candidate_lifecycle.test_schema_prefix() {
        push(
            paths,
            "test/schemaPrefix",
            DiffLayer::Test,
            DiffClass::Breaking,
        );
    }
    for (name, base_capability, candidate_capability) in [
        (
            "create",
            base_lifecycle.create(),
            candidate_lifecycle.create(),
        ),
        ("drop", base_lifecycle.drop(), candidate_lifecycle.drop()),
    ] {
        if support_rank(base_capability.support()) != support_rank(candidate_capability.support()) {
            let class = if support_rank(candidate_capability.support())
                > support_rank(base_capability.support())
            {
                DiffClass::NonBreaking
            } else {
                DiffClass::Breaking
            };
            push(paths, &format!("test/{name}"), DiffLayer::Test, class);
        }
    }
}
