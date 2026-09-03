//! The Model migration graph, step catalog, and registry binding (issue #9).
//!
//! Migration is explicit and one-step at a time: the registry declares the
//! edges, this module owns the sealed compiled catalog, and planning walks
//! the unique declared chain executing every step in order. A chain is never
//! collapsed into a single rewrite, and every intermediate output is
//! re-validated with its target version before the next step runs.
//!
//! The catalog and the registry artifact are bound one-to-one: an edge with
//! no implementation and an implementation with no edge both make the
//! embedded registry invalid (`versioning.registry-invalid`), as does any
//! family that declares edges without a compiled catalog.

use super::family::{self, ModelContract};
use super::registry::{
    ChangeClassification, FamilyRegistry, MigrationEdge, RegistryError, VersionRegistry,
};
use super::version::ContractVersion;
use crate::loader::ModelVersion;

/// The stable identity of the one real Model migration step.
pub const MODEL_STEP_0_1_0_TO_1_0_0: &str = "model-0.1.0-to-1.0.0@1";

mod private {
    /// Seal [`super::MigrationStep`] to this crate's catalog.
    pub trait SealedStep {}
    impl SealedStep for crate::versioning::model_v0_1_0_to_v1_0_0::ModelV0_1_0ToV1_0_0 {}
}

/// One compiled, Model-only migration step.
///
/// Sealed: library callers can execute and inspect the shipped catalog but
/// cannot inject arbitrary transforms, so a plan can never run code the
/// registry does not declare. Implementations are pure: they read snapshot
/// bytes and return new bytes; they never touch the filesystem.
pub trait MigrationStep: private::SealedStep + Sync {
    /// The stable implementation identity; equal to a registry edge id.
    fn id(&self) -> &'static str;
    /// The exact finite source Model version.
    fn from(&self) -> ModelVersion;
    /// The exact finite target Model version.
    fn to(&self) -> ModelVersion;
    /// The change classification, mirroring the bound registry edge.
    fn classification(&self) -> ChangeClassification;
    /// The declared per-file loss tokens; empty means byte-preserving.
    fn loss(&self) -> &'static [&'static str];
    /// The representability preflight over the normalized source project.
    /// Zero-write: violations abort planning before any edit exists.
    fn preflight(&self, model: &crate::loader::NormalizedModel) -> Result<(), StepFailure>;
    /// Rewrite one document's bytes from `from()` to `to()`. The snapshot's
    /// parsed version token span locates the edit; every other byte is
    /// preserved exactly.
    fn transform(&self, document: &super::plan::DocumentSnapshot) -> Result<Vec<u8>, StepFailure>;
}

/// The compiled catalog: every step the binary can execute, in identity
/// order. Exactly one real Model step exists today.
pub fn catalog() -> &'static [&'static dyn MigrationStep] {
    static CATALOG: &[&dyn MigrationStep] = &[&super::model_v0_1_0_to_v1_0_0::ModelV0_1_0ToV1_0_0];
    CATALOG
}

fn step_by_id(id: &str) -> Option<&'static dyn MigrationStep> {
    catalog().iter().copied().find(|step| step.id() == id)
}

/// The finite Model versions, for binding registry edges to steps.
fn finite_model(text: &str) -> Option<ModelVersion> {
    match text {
        "0.1.0" => Some(ModelVersion::V0_1_0),
        "1.0.0" => Some(ModelVersion::V1_0_0),
        _ => None,
    }
}

/// Bind the embedded registry to the compiled catalog, one-to-one.
pub(crate) fn validate_registry_binding(registry: &VersionRegistry) -> Result<(), RegistryError> {
    let mut details: Vec<String> = Vec::new();

    // Model: exact id sets must match; endpoints must match too, so a
    // re-declared edge with drifted versions cannot hide behind its id.
    let registry_ids: Vec<&str> = registry
        .model()
        .edges()
        .iter()
        .map(|edge| edge.id.as_str())
        .collect();
    let catalog_ids: Vec<&str> = catalog().iter().map(|step| step.id()).collect();
    for id in &registry_ids {
        if !catalog_ids.contains(id) {
            details.push(format!("registry edge {id} has no compiled implementation"));
        }
    }
    for id in &catalog_ids {
        if !registry_ids.contains(id) {
            details.push(format!("compiled step {id} has no registry edge"));
        }
    }
    for edge in registry.model().edges() {
        let Some(step) = step_by_id(&edge.id) else {
            continue;
        };
        let (Some(edge_from), Some(edge_to)) = (
            finite_model(edge.from.as_str()),
            finite_model(edge.to.as_str()),
        ) else {
            details.push(format!(
                "registry edge {} declares a non-finite Model version",
                edge.id
            ));
            continue;
        };
        if step.from() != edge_from {
            details.push(format!(
                "registry edge {} source {} disagrees with implementation {}",
                edge.id,
                edge_from.as_str(),
                step.from().as_str()
            ));
        }
        if step.to() != edge_to {
            details.push(format!(
                "registry edge {} target {} disagrees with implementation {}",
                edge.id,
                edge_to.as_str(),
                step.to().as_str()
            ));
        }
    }

    // No other family may declare edges without a compiled catalog; the
    // closed catalog is Model-only today.
    for edge in registry.ir().edges() {
        details.push(format!(
            "ir edge {} has no compiled implementation",
            edge.id
        ));
    }
    for edge in registry.protocol().edges() {
        details.push(format!(
            "protocol edge {} has no compiled implementation",
            edge.id
        ));
    }

    if details.is_empty() {
        Ok(())
    } else {
        details.sort();
        Err(RegistryError { details })
    }
}

/// Why a step refused to transform a snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StepFailure {
    /// The source model cannot be represented under the target version.
    /// Owner-authored semantic edits are required; nothing was rewritten.
    Precondition {
        /// Sorted, stable violations (`id` + closed `rule` token).
        violations: Vec<PreconditionViolation>,
    },
    /// The version token cannot be rewritten byte-safely in this document.
    NotFileMigratable {
        /// The logical project-relative path.
        path: String,
        /// The closed detail token.
        detail: NotMigratableDetail,
    },
}

/// The closed rule vocabulary of the representability preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PreconditionRule {
    /// A project ID is not one legal Model 1.0.0 segment.
    ProjectGrammar,
    /// A project or module ID uses a reserved word.
    Reserved,
    /// A module ID is not one legal Model 1.0.0 segment.
    ModuleGrammar,
    /// A symbol ID is not exactly two legal Model 1.0.0 segments.
    SymbolGrammar,
    /// A symbol ID's first segment is not its declared module ID.
    Qualification,
    /// A module import is not a legal Model 1.0.0 module ID.
    ImportGrammar,
}

impl PreconditionRule {
    /// The stable wire token used in plan diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectGrammar => "project-grammar",
            Self::Reserved => "reserved",
            Self::ModuleGrammar => "module-grammar",
            Self::SymbolGrammar => "symbol-grammar",
            Self::Qualification => "qualification",
            Self::ImportGrammar => "import-grammar",
        }
    }
}

/// One sorted precondition violation.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PreconditionViolation {
    /// The offending identifier, echoed verbatim (bounded upstream).
    pub id: String,
    /// The closed rule that refused it.
    pub rule: PreconditionRule,
}

/// The closed detail vocabulary of [`StepFailure::NotFileMigratable`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotMigratableDetail {
    /// The parsed source version does not match the step's source version.
    UnexpectedSourceVersion,
    /// The version token span does not contain exactly one source token.
    TokenNotUnique,
    /// The rewritten document does not re-parse with the target version.
    ReparseFailed,
}

impl NotMigratableDetail {
    /// The stable wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnexpectedSourceVersion => "unexpected-source-version",
            Self::TokenNotUnique => "token-not-unique",
            Self::ReparseFailed => "reparse-failed",
        }
    }
}

/// Resolve the unique declared chain of edges inside one family registry.
/// Edges are cloned so plans own their chain.
pub fn family_chain<F: family::ContractFamily>(
    family: &FamilyRegistry<F>,
    from: &ContractVersion<F>,
    to: &ContractVersion<F>,
) -> Result<Vec<MigrationEdge<F>>, ChainError> {
    if from == to {
        return Ok(Vec::new());
    }
    let paths = family.all_paths(from);
    let chains = paths.get(to).map_or(&[][..], Vec::as_slice);
    match chains {
        [] => Err(ChainError::NoPath),
        [chain] => {
            let mut edges = Vec::with_capacity(chain.len());
            for id in chain {
                edges.push(
                    family
                        .edges()
                        .iter()
                        .find(|edge| &edge.id == id)
                        .expect("path edges come from the registry")
                        .clone(),
                );
            }
            Ok(edges)
        }
        _ => Err(ChainError::Ambiguous),
    }
}

/// Resolve the unique declared chain of edges between two finite Model
/// versions. Edges are cloned so plans own their chain.
pub fn model_chain(
    registry: &VersionRegistry,
    from: ModelVersion,
    to: ModelVersion,
) -> Result<Vec<MigrationEdge<ModelContract>>, ChainError> {
    let from_version = ContractVersion::<ModelContract>::from(from);
    let to_version = ContractVersion::<ModelContract>::from(to);
    family_chain(registry.model(), &from_version, &to_version)
}

/// Why no migration chain exists between two versions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChainError {
    /// The registry declares no path (stable exit-5 refusal).
    NoPath,
    /// The registry declares several paths — a registry fault (exit 1).
    Ambiguous,
}

impl ChainError {
    /// The stable reason code for this refusal.
    pub const fn reason_code(self) -> &'static str {
        match self {
            Self::NoPath => "versioning.no-migration-path",
            Self::Ambiguous => "versioning.registry-invalid",
        }
    }
}
