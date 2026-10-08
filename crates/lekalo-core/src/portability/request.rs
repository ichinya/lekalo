//! The bounded portability request and its qualified input documents
//! (issue #19).
//!
//! The core performs no filesystem, process, or network operations: the
//! caller supplies exact bytes, and this module only decodes, versions,
//! and pins them. Selectors are validated against the closed grammar and
//! resolved against the compilation before any assessment runs.

use super::diagnostic;
use super::version;
use super::wire::{self, EvidenceDocument, Recipe, RecipeDocument, TargetsDocument};
use crate::diagnostics::DiagnosticSet;
use crate::ir::Compilation;

/// The selected assessment scope. No scope means the entire project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Scope {
    Project,
    Symbol(String),
    Module(String),
}

/// The typed, fully validated assessment request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub scope: Scope,
    pub source: Option<String>,
    pub destinations: Vec<String>,
}

/// Whether a required selector was missing or ambiguous.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectorError {
    /// No candidate exists at all.
    Missing,
    /// Several candidates exist and no explicit selection was given.
    Ambiguous,
}

impl Request {
    /// Validate the request shape against the closed selector grammar.
    pub fn validate(&self) -> Result<(), DiagnosticSet> {
        match &self.scope {
            Scope::Project => {}
            Scope::Symbol(id) | Scope::Module(id) => {
                if crate::project_fs::selection_violation(id).is_some() {
                    return Err(diagnostic::invalid("selector"));
                }
            }
        }
        if let Some(source) = &self.source {
            if !valid_target_token(source) {
                return Err(diagnostic::invalid("selector"));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for destination in &self.destinations {
            if !valid_target_token(destination) || !seen.insert(destination.as_str()) {
                return Err(diagnostic::invalid("selector"));
            }
        }
        Ok(())
    }
}

/// The closed target-token grammar of the caller identifiers: the logic
/// never switches on a specific token.
fn valid_target_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token.starts_with(|c: char| c.is_ascii_lowercase())
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Decode and validate the targets configuration bytes.
///
/// Every binding must pin a syntactically exact profile digest pair, the
/// defaults must reference configured ids exactly once, and every
/// artifact reference must stay inside the safe project-relative grammar.
pub fn parse_targets(bytes: &[u8]) -> Result<TargetsDocument, DiagnosticSet> {
    let value = wire::decode_dup_safe(bytes)?;
    // Mirror the contract's required-but-nullable binding members.
    for binding in value
        .get("targets")
        .and_then(|t| t.as_array())
        .into_iter()
        .flatten()
    {
        wire::require_members(binding, &["adapterEvidenceRef", "storageProfileRefs"])?;
    }
    let document: TargetsDocument =
        serde_json::from_value(value).map_err(|_| diagnostic::invalid("closed-shape"))?;
    if document.schema_version != version::TARGETS_SCHEMA_VERSION
        || document.identity != version::TARGETS_IDENTITY
    {
        return Err(diagnostic::unsupported_version("targets-version"));
    }
    // Mirror the contract's `minItems` exactly: an explicitly empty
    // target list or an explicitly empty default-destination list is a
    // decode refusal, never a silent reinterpretation.
    if document.targets.is_empty() {
        return Err(diagnostic::invalid("target-binding"));
    }
    if document
        .default_destinations
        .as_ref()
        .is_some_and(|list| list.is_empty())
    {
        return Err(diagnostic::invalid("default-destination"));
    }
    // Mirror the contract's `uniqueItems` exactly: duplicated defaults
    // are refused, never silently deduplicated.
    if let Some(list) = &document.default_destinations {
        let mut unique = std::collections::BTreeSet::new();
        for destination in list {
            if !unique.insert(destination.as_str()) {
                return Err(diagnostic::invalid("default-destination"));
            }
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    for binding in &document.targets {
        if !valid_target_token(&binding.target_id) || !ids.insert(binding.target_id.as_str()) {
            return Err(diagnostic::invalid("target-binding"));
        }
        for reference in [&binding.profile_document_ref]
            .into_iter()
            .chain(binding.adapter_evidence_ref.iter())
            .chain(binding.storage_profile_refs.iter())
        {
            if crate::project_fs::path_violation(reference).is_some() {
                return Err(diagnostic::invalid("artifact-ref"));
            }
        }
        if !wire::is_digest(&binding.profile_ref.source_digest)
            || !wire::is_digest(&binding.profile_ref.resolved_digest)
        {
            return Err(diagnostic::invalid("profile-pin"));
        }
        if binding.profile_ref.id.is_empty() || binding.profile_ref.version.is_empty() {
            return Err(diagnostic::invalid("profile-pin"));
        }
    }
    if let Some(source) = &document.default_source {
        if !ids.contains(source.as_str()) {
            return Err(diagnostic::invalid("default-source"));
        }
    }
    for destination in document.default_destinations.iter().flatten() {
        if !ids.contains(destination.as_str()) {
            return Err(diagnostic::invalid("default-destination"));
        }
    }
    Ok(document)
}

/// Decode and validate the evidence manifest bytes against one
/// compilation and the embedded recipe digest.
///
/// A stale or foreign project pin is refused before any assessment; the
/// manifest may then still declare incomplete families, which the
/// assessment reports as explicit evidence gaps instead of refusals.
pub fn parse_evidence(
    bytes: &[u8],
    compilation: &Compilation,
    recipe: &Recipe,
) -> Result<EvidenceDocument, DiagnosticSet> {
    let value = wire::decode_dup_safe(bytes)?;
    // Mirror the schemas' required-but-nullable members exactly: key
    // presence is mandatory even where the value may be `null`.
    if let Some(recorded_context) = value.get("recordedContext") {
        wire::require_members(recorded_context, &["revision", "note"])?;
    }
    for family in value
        .get("families")
        .and_then(|f| f.as_array())
        .into_iter()
        .flatten()
    {
        wire::require_members(family, &["artifactRef", "digest"])?;
    }
    for adapter in value
        .get("adapters")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
    {
        wire::require_members(
            adapter,
            &["identity", "version", "protocol", "capabilityDigest"],
        )?;
    }
    for mapping in value
        .get("mappings")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
    {
        wire::require_members(mapping, &["subject"])?;
    }
    for scenario in value
        .get("scenarios")
        .and_then(|s| s.as_array())
        .into_iter()
        .flatten()
    {
        wire::require_members(scenario, &["targetId", "receiptRef", "receiptDigest"])?;
    }
    let document: EvidenceDocument =
        serde_json::from_value(value).map_err(|_| diagnostic::invalid("closed-shape"))?;
    if document.schema_version != version::EVIDENCE_SCHEMA_VERSION
        || document.identity != version::EVIDENCE_IDENTITY
    {
        return Err(diagnostic::unsupported_version("evidence-version"));
    }
    if document.recipe_ref.id != version::RECIPE_REF_ID
        || document.recipe_ref.version != recipe.document_version()
        || document.recipe_ref.digest != recipe.digest
    {
        return Err(diagnostic::invalid("recipe-pin"));
    }
    let project = &compilation.project;
    let expected_id = project
        .project
        .as_ref()
        .map(|p| p.id.as_str().to_owned())
        .unwrap_or_default();
    if document.project_ref.project_id != expected_id
        || document.project_ref.model_version != project.model_version.as_str()
        || document.project_ref.ir_digest != crate::implementation::ir_digest(project).as_str()
    {
        return Err(diagnostic::invalid("project-pin"));
    }
    if document.recorded_context.state == wire::ContextState::Absent
        && (document.recorded_context.revision.is_some()
            || document.recorded_context.note.is_some())
    {
        return Err(diagnostic::invalid("recorded-context"));
    }
    let mut families = std::collections::BTreeSet::new();
    for family in &document.families {
        if !families.insert(family.family) {
            return Err(diagnostic::invalid("duplicate-family"));
        }
        if let Some(reference) = &family.artifact_ref {
            if crate::project_fs::path_violation(reference).is_some() {
                return Err(diagnostic::invalid("artifact-ref"));
            }
        }
        // The digest pin of an admitted artifact is always a syntactically
        // exact sha256 digest: a fabricated spelling never decodes.
        if let Some(digest) = &family.digest {
            if !wire::is_digest(digest) {
                return Err(diagnostic::invalid("family-pin"));
            }
        }
        if family.complete && (family.artifact_ref.is_none() || family.digest.is_none()) {
            return Err(diagnostic::invalid("family-pin"));
        }
        if !family.complete && family.artifact_ref.is_some() != family.digest.is_some() {
            return Err(diagnostic::invalid("family-pin"));
        }
    }
    let mut adapters = std::collections::BTreeSet::new();
    for adapter in &document.adapters {
        if !adapters.insert(adapter.target_id.as_str()) {
            return Err(diagnostic::invalid("duplicate-adapter"));
        }
        // A recorded snapshot is exact provenance: identity, version,
        // protocol, and the digest-pinned capability must all be present.
        // A nullable member under a recorded state is an unqualified
        // snapshot, not an admitted one.
        if adapter.state == wire::AdapterState::Recorded
            && (adapter.identity.is_none()
                || adapter.version.is_none()
                || adapter.protocol.is_none()
                || adapter.capability_digest.is_none()
                || !wire::is_digest(adapter.capability_digest.as_deref().unwrap_or_default()))
        {
            return Err(diagnostic::invalid("adapter-snapshot"));
        }
    }
    let mut mappings = std::collections::BTreeSet::new();
    for mapping in &document.mappings {
        if !mappings.insert(mapping.id.as_str()) {
            return Err(diagnostic::invalid("duplicate-mapping"));
        }
        if !valid_target_token(&mapping.target_id) {
            return Err(diagnostic::invalid("mapping-target"));
        }
        if mapping.coverage == wire::MappingCoverage::Full && mapping.definition_pins.is_empty() {
            return Err(diagnostic::invalid("mapping-pin"));
        }
    }
    // Receipt custody is one identity per (scenario, destination): two
    // records for the same join with different results or digests are a
    // contradiction the assessment must never silently reconcile.
    let mut receipts = std::collections::BTreeSet::new();
    for record in &document.scenarios {
        if !receipts.insert((record.scenario.as_str(), record.target_id.as_deref())) {
            return Err(diagnostic::invalid("duplicate-receipt"));
        }
        // A claimed result is admitted only together with a target-
        // qualified, digest-bound receipt; an untested record never
        // carries receipt custody.
        match record.state {
            wire::ScenarioReceipt::Untested => {
                if record.receipt_ref.is_some() || record.receipt_digest.is_some() {
                    return Err(diagnostic::invalid("receipt-ref"));
                }
            }
            wire::ScenarioReceipt::RecordedPass | wire::ScenarioReceipt::RecordedFail => {
                let qualified = record.target_id.is_some()
                    && record.receipt_ref.is_some()
                    && record
                        .receipt_digest
                        .as_deref()
                        .is_some_and(wire::is_digest);
                if !qualified {
                    return Err(diagnostic::invalid("receipt-ref"));
                }
                if crate::project_fs::path_violation(
                    record.receipt_ref.as_deref().unwrap_or_default(),
                )
                .is_some()
                {
                    return Err(diagnostic::invalid("artifact-ref"));
                }
            }
        }
    }
    Ok(document)
}

/// Decode the embedded recipe bytes once; a decode failure is a
/// developer fault and fails closed.
pub fn embedded_recipe() -> Result<Recipe, DiagnosticSet> {
    static RECIPE: std::sync::LazyLock<Result<Recipe, DiagnosticSet>> =
        std::sync::LazyLock::new(|| {
            let document: RecipeDocument = wire::decode(super::version::RECIPE_BYTES)
                .map_err(|_| crate::result::singleton_set("diagnostics.registry-invalid"))?;
            if document.schema_version != super::version::RECIPE_SCHEMA_VERSION
                || document.identity != super::version::RECIPE_IDENTITY
            {
                return Err(crate::result::singleton_set("diagnostics.registry-invalid"));
            }
            Ok(Recipe {
                digest: format!(
                    "sha256:{}",
                    crate::digest::sha256_hex(super::version::RECIPE_BYTES)
                ),
                document,
            })
        });
    RECIPE.clone()
}

impl Recipe {
    /// The exact recipe contract version.
    pub fn document_version(&self) -> &str {
        self.document
            .schema_version
            .rsplit('/')
            .next()
            .and_then(|v| v.strip_prefix('v'))
            .unwrap_or_default()
    }

    /// The maximum selected subjects bound.
    pub fn max_subjects(&self) -> u64 {
        self.document.limits.max_subjects
    }

    /// The dimension assignment of one definition kind, or `None` when
    /// the reviewed recipe does not admit the kind for any dimension.
    pub fn dimensions_for_kind(&self, kind: &str) -> Vec<super::wire::Dimension> {
        let mut dimensions = Vec::new();
        for (name, recipe) in &self.document.dimensions {
            if recipe.definition_kinds.iter().any(|k| k == kind) {
                if let Some(dimension) = super::wire::Dimension::parse_name(name) {
                    dimensions.push(dimension);
                }
            }
        }
        dimensions.sort();
        dimensions
    }

    /// Whether a definition kind is excluded from the denominator.
    pub fn excluded_from_denominator(&self, kind: &str) -> bool {
        self.document
            .excluded_from_denominator
            .iter()
            .any(|k| k == kind)
    }
}
