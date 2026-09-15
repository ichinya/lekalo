//! Issue #30: foreign and custom implementation escape hatches.
//!
//! One independent, closed, versioned attachment —
//! `lekalo/implementation/v1.0.0`, identity
//! `dev.lekalo.implementation@1.0.0` — binding existing target-local
//! implementations to compiled operation symbols so complex or
//! target-specific logic stays ordinary code instead of pushing Lekalo
//! toward a universal language. Every hook contract names one command
//! or query symbol, one hook-interface contract name, and per-target
//! bindings over the closed implementation kinds `generated`, `custom`
//! (checked/custom files), `foreign` (a foreign symbol reference),
//! `external` (an external service/port), and `unsupported` (explicitly
//! unsupported for that target).
//!
//! Boundaries: the canonical input/output/error/effect contract of an
//! operation is and stays the Model's; the attachment carries no
//! schemas, no source paths, and no commands, so it can never restate
//! or covertly extend a declared contract (an acknowledged effect
//! outside the operation's declared surface rejects through
//! `implementation.effect-mismatch`). Binding existence and signature
//! checking belongs to the target adapter over the published process
//! protocol (#27); the core only classifies what the declaration means.
//! Target-specific implementations are never automatically portable:
//! the per-target portability projection reports every project target
//! without an implementation entry (`implementation.target-missing`).
//! Custom files stay user-owned: the artifact-ownership manifest
//! (#21) `custom` lifecycle already forbids overwrite and generic
//! clean, and this module adds the declaration those entries answer
//! to. Scenarios cover operations (#23), never implementations, so one
//! scenario suite applies to every implementation of an operation; the
//! projection carries the covering scenarios per symbol.
//!
//! Determinism and denial: canonical bytes are compact UTF-8 JSON with
//! byte-sorted keys; set-like collections normalize to sorted form;
//! every bound violation rejects with an explicit registered
//! diagnostic and no partial result.

pub mod canonical;
pub(crate) mod diagnostic;
pub mod version;
pub(crate) mod wire;

pub use version::{FAMILY, IDENTITY, SCHEMA_VERSION, VERSION};

use crate::diagnostics::DiagnosticSet;
use crate::ir::{Compilation, Definition};
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

use diagnostic::finish;

/// The bound source Model contract: exact accepted version plus digest
/// (the shared scenario pin vocabulary).
pub use crate::scenario::ModelRef as ModelPin;
/// One finished implementation attachment: immutable, deterministically
/// ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationDocument {
    pub(crate) project_id: SemanticId,
    pub(crate) model_ref: ModelPin,
    pub(crate) ir_digest: Sha256Digest,
    pub(crate) contracts: Vec<HookContract>,
}

impl ImplementationDocument {
    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set with no partial document.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, DiagnosticSet> {
        wire::from_value(json)
    }

    /// The canonical compact JSON bytes (byte-sorted keys, no trailing
    /// LF), bounded by [`version::MAX_CANONICAL_BYTES`].
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        canonical::to_canonical_json(self)
    }

    /// The SHA-256 digest of the canonical bytes.
    pub fn digest(&self) -> Result<Sha256Digest, DiagnosticSet> {
        canonical::digest(self)
    }

    /// The one-segment project identity the attachment binds.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound Model pin.
    pub const fn model_ref(&self) -> &ModelPin {
        &self.model_ref
    }

    /// The bound canonical IR digest.
    pub const fn ir_digest(&self) -> &Sha256Digest {
        &self.ir_digest
    }

    /// The declared hook contracts, sorted by `(symbol, contract)`.
    pub fn contracts(&self) -> &[HookContract] {
        &self.contracts
    }
}

/// One hook contract: one operation symbol, one hook-interface name,
/// its acknowledged declared effects, and its per-target bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookContract {
    /// The bound operation symbol (a command or query).
    pub(crate) symbol: String,
    /// The hook-interface contract name (`schedule.calculator/v1`).
    pub(crate) contract: String,
    /// The acknowledged declared effect references, sorted and unique.
    pub(crate) effects: Vec<String>,
    /// The target bindings, sorted by target name, unique.
    pub(crate) targets: Vec<TargetBinding>,
}

impl HookContract {
    /// The bound operation symbol.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// The hook-interface contract name.
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// The acknowledged declared effect references.
    pub fn effects(&self) -> &[String] {
        &self.effects
    }

    /// The target bindings.
    pub fn targets(&self) -> &[TargetBinding] {
        &self.targets
    }
}

/// One target binding: how one operation is implemented on one target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetBinding {
    /// The target name (never interpreted by the core).
    pub(crate) target: String,
    /// The closed implementation kind.
    pub(crate) kind: ImplementationKind,
    /// The foreign symbol reference (kind `foreign` only).
    pub(crate) symbol: Option<String>,
    /// The external port name (kind `external` only).
    pub(crate) port: Option<String>,
    /// The explicit selection marker among competing bindings.
    pub(crate) selected: bool,
}

impl TargetBinding {
    /// The target name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The closed implementation kind.
    pub const fn kind(&self) -> ImplementationKind {
        self.kind
    }

    /// The foreign symbol reference, when the kind carries one.
    pub fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    /// The external port name, when the kind carries one.
    pub fn port(&self) -> Option<&str> {
        self.port.as_deref()
    }

    /// Whether this binding is the explicit selection among competitors.
    pub const fn selected(&self) -> bool {
        self.selected
    }
}

/// The closed implementation-kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ImplementationKind {
    /// The target adapter generates the operation (explicit record).
    Generated,
    /// User-owned custom/checked files; never overwritten or cleaned.
    Custom,
    /// A foreign symbol in the target's own code.
    Foreign,
    /// An external service or port.
    External,
    /// Explicitly unsupported for this target.
    Unsupported,
}

impl ImplementationKind {
    /// The stable lowercase wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Custom => "custom",
            Self::Foreign => "foreign",
            Self::External => "external",
            Self::Unsupported => "unsupported",
        }
    }

    /// The exact wire spelling; nothing else parses.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "generated" => Self::Generated,
            "custom" => Self::Custom,
            "foreign" => Self::Foreign,
            "external" => Self::External,
            "unsupported" => Self::Unsupported,
            _ => return None,
        })
    }
}

/// The validated attachment report: the operation status verdict plus
/// the deterministic per-target portability projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationReport {
    /// Every project target binding without an implementation entry for
    /// an escape-hatched operation, as registry warnings.
    warnings: DiagnosticSet,
    /// The deterministic portability projection.
    portability: PortabilityReport,
}

impl ImplementationReport {
    /// The portability warnings (valid status; never errors).
    pub fn warnings(&self) -> &DiagnosticSet {
        &self.warnings
    }

    /// The deterministic portability projection.
    pub const fn portability(&self) -> &PortabilityReport {
        &self.portability
    }
}

/// The per-target portability of one escape-hatched operation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct SymbolPortability {
    /// The operation symbol.
    pub symbol: String,
    /// The hook-interface contract name.
    pub contract: String,
    /// The scenarios covering the operation, sorted (one suite for every
    /// implementation of the operation).
    pub scenarios: Vec<String>,
    /// One row per target in the sorted union of declared and project
    /// targets.
    pub targets: Vec<TargetPortability>,
}

/// One target row of the portability projection.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TargetPortability {
    /// The target name.
    pub target: String,
    /// The implementation status on the target.
    pub status: PortStatus,
}

/// The closed portability status: the declared implementation kind, or
/// `missing` when a project target has no entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PortStatus {
    /// The adapter generates the operation.
    Generated,
    /// User-owned custom files.
    Custom,
    /// A foreign symbol reference.
    Foreign,
    /// An external service or port.
    External,
    /// Explicitly unsupported for the target.
    Unsupported,
    /// No entry for a project target binding.
    Missing,
}

impl PortStatus {
    /// The stable lowercase wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Custom => "custom",
            Self::Foreign => "foreign",
            Self::External => "external",
            Self::Unsupported => "unsupported",
            Self::Missing => "missing",
        }
    }
}

/// The deterministic per-operation portability projection.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PortabilityReport {
    /// The bound project identity.
    pub project: String,
    /// One entry per hook contract, sorted by symbol.
    pub symbols: Vec<SymbolPortability>,
}

/// Validate the attachment against one compilation and project the
/// per-target portability.
///
/// Errors (pinned identity mismatches, unresolved operation references,
/// effect-surface extensions) return the normalized error set; a valid
/// outcome carries the `implementation.target-missing` warnings plus
/// the closed projection. The core never launches or reads target
/// code: binding existence and signature checking is the adapter's
/// protocol responsibility.
pub fn validate(
    document: &ImplementationDocument,
    compilation: &Compilation,
) -> Result<ImplementationReport, DiagnosticSet> {
    let project = &compilation.project;
    let mut errors: Vec<crate::diagnostics::Diagnostic> = Vec::new();

    // Pinned identities: the attachment answers to one exact project,
    // Model version, and canonical IR revision.
    let project_id = project.project.as_ref().map(|project| project.id.as_str());
    if project_id != Some(document.project_id.as_str()) {
        diagnostic::push_document_invalid(&mut errors, "project-pin");
    }
    if project.model_version.as_str() != document.model_ref.version.as_str() {
        diagnostic::push_document_invalid(&mut errors, "model-pin");
    }
    let canonical = canonical::ir_digest(project);
    if canonical.as_str() != document.ir_digest.as_str() {
        diagnostic::push_document_invalid(&mut errors, "ir-pin");
    }

    // Every hook contract names an existing command or query.
    for contract in &document.contracts {
        let found = resolve(project, &contract.symbol);
        let operation = found.filter(|definition| {
            matches!(definition, Definition::Command(_) | Definition::Query(_))
        });
        if operation.is_none() {
            diagnostic::push_reference_unresolved(
                &mut errors,
                &contract.symbol,
                &contract.contract,
            );
            continue;
        }
        // Acknowledged effects must stay inside the declared surface.
        let declared = declared_effects(operation.expect("checked above"));
        for reference in &contract.effects {
            if !declared.iter().any(|effect| effect == reference) {
                diagnostic::push_effect_mismatch(&mut errors, &contract.symbol, reference);
            }
        }
    }
    if errors.is_empty() {
        let portability = portability(document, compilation);
        let mut warnings: Vec<crate::diagnostics::Diagnostic> = Vec::new();
        for symbol in &portability.symbols {
            for target in &symbol.targets {
                if target.status == PortStatus::Missing {
                    diagnostic::push_target_missing(&mut warnings, &symbol.symbol, &target.target);
                }
            }
        }
        Ok(ImplementationReport {
            warnings: finish(warnings, crate::result::Status::Valid),
            portability,
        })
    } else {
        Err(finish(errors, crate::result::Status::Invalid))
    }
}

/// The deterministic per-target portability projection: one row per
/// target in the union of the declared and the project's bound targets,
/// `missing` exactly when a project target binding has no entry.
pub fn portability(
    document: &ImplementationDocument,
    compilation: &Compilation,
) -> PortabilityReport {
    let project = &compilation.project;
    let mut project_targets: Vec<&str> = project
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::TargetBinding(binding) => Some(binding.target.as_str()),
            _ => None,
        })
        .collect();
    project_targets.sort_unstable();
    project_targets.dedup();

    let covering = |symbol: &str| -> Vec<String> {
        let mut scenarios: Vec<String> = project
            .definitions
            .iter()
            .filter_map(|definition| match definition {
                Definition::Scenario(scenario) => scenario
                    .covers
                    .iter()
                    .any(|covered| covered.as_str() == symbol)
                    .then(|| scenario.id.as_str().to_owned()),
                _ => None,
            })
            .collect();
        scenarios.sort();
        scenarios.dedup();
        scenarios
    };

    let symbols = document
        .contracts
        .iter()
        .map(|contract| {
            let mut targets: Vec<&str> = contract
                .targets
                .iter()
                .map(|binding| binding.target.as_str())
                .collect();
            targets.extend(project_targets.iter().copied());
            targets.sort_unstable();
            targets.dedup();
            let rows = targets
                .into_iter()
                .map(|target| {
                    let status = contract
                        .targets
                        .iter()
                        .find(|binding| binding.target == target)
                        .map(|binding| match binding.kind {
                            ImplementationKind::Generated => PortStatus::Generated,
                            ImplementationKind::Custom => PortStatus::Custom,
                            ImplementationKind::Foreign => PortStatus::Foreign,
                            ImplementationKind::External => PortStatus::External,
                            ImplementationKind::Unsupported => PortStatus::Unsupported,
                        })
                        .unwrap_or(PortStatus::Missing);
                    TargetPortability {
                        target: target.to_owned(),
                        status,
                    }
                })
                .collect();
            SymbolPortability {
                symbol: contract.symbol.clone(),
                contract: contract.contract.clone(),
                scenarios: covering(&contract.symbol),
                targets: rows,
            }
        })
        .collect();
    PortabilityReport {
        project: document.project_id.as_str().to_owned(),
        symbols,
    }
}

/// The SHA-256 digest over one compiled project's canonical IR bytes:
/// the exact revision pin every attachment answers to.
pub fn ir_digest(project: &crate::ir::CompiledProject) -> Sha256Digest {
    canonical::ir_digest(project)
}

/// Resolve one definition by semantic id via the canonical byte order
/// (the validator's seam).
fn resolve<'def>(
    project: &'def crate::ir::CompiledProject,
    symbol: &str,
) -> Option<&'def Definition> {
    let definitions = project.definitions.as_slice();
    let index = definitions
        .binary_search_by(|definition| definition.id().as_str().as_bytes().cmp(symbol.as_bytes()))
        .ok()?;
    definitions.get(index)
}

/// The declared effect surface of one operation: a command's
/// `effects` references; a query declares none.
fn declared_effects(definition: &Definition) -> Vec<&str> {
    match definition {
        Definition::Command(command) => command
            .effects
            .iter()
            .map(|effect| effect.as_str())
            .collect(),
        _ => Vec::new(),
    }
}
