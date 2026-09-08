//! Issue #36: the read-only OpenSpec requirement traceability integration.
//!
//! One independent, closed, versioned attachment —
//! `lekalo/requirements/v1.0.0`, identity `dev.lekalo.requirements@1.0.0` —
//! binds Lekalo semantic symbols to canonical requirement identities owned
//! by requirement providers (the `openspec` provider kind reads the
//! on-disk `openspec/specs/**` and `openspec/changes/**` trees; no OpenSpec
//! CLI is ever invoked), and one derived, read-only report wire —
//! `lekalo/requirements-report/v1.0.0` — carries the resolved catalog,
//! per-reference resolution statuses, coverage gaps, conflicts, and the
//! changed-requirement impact.
//!
//! Authority and boundaries (ADR-0001/ADR-0026): OpenSpec owns
//! requirements and change intent; Lekalo only reads. This module never
//! writes, never copies requirement text into the semantic model or the
//! IR (the Model grammar and published schemas are untouched), never
//! treats a generated summary as a canonical requirement, and never
//! silently overwrites a conflict — conflicting active changes surface as
//! an explicit gate denial. The integration projects into the neutral #22
//! trace manifest instead of defining an OpenSpec-specific wire. Absence
//! of a provider tree is legal for standalone use.
//!
//! Determinism and denial: attachment canonical bytes are compact UTF-8
//! JSON with byte-sorted keys; the report is a pure function of the
//! attachment, the compiled Model, and the provider trees. Every bound
//! and every semantic contradiction rejects with an explicit registered
//! diagnostic and no partial result.

pub mod diagnostic;
mod json;
pub(crate) mod provider;
pub mod report;
pub mod trace;
pub mod version;
pub(crate) mod wire;

use crate::loader::LoadSelection;
use crate::lockfile::types::Sha256Digest;
use crate::result::DomainResult;
use crate::scenario::id::SemanticId;

pub use diagnostic::io_failure;
pub use report::{ReferenceRow, Report, Resolution, ResolutionVerdict};

/// The closed requirement-reference relation vocabulary.
///
/// `derived_from` is requirement provenance (the Model-level word);
/// `implements` is the neutral #22 trace relation spelling. Both project
/// into `implements` edges of the trace manifest, because the closed #22
/// endpoint matrix admits exactly one symbol→requirement kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Relation {
    /// The symbol derives from the requirement (provenance).
    DerivedFrom,
    /// The symbol implements the requirement.
    Implements,
}

impl Relation {
    /// The closed v1 relations in canonical order.
    pub const KEYS: [Self; 2] = [Self::DerivedFrom, Self::Implements];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DerivedFrom => "derived_from",
            Self::Implements => "implements",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// The closed requirement provider kind vocabulary. v1 ships exactly the
/// on-disk OpenSpec provider; more kinds require a reviewed successor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ProviderKind {
    /// The on-disk OpenSpec artifacts provider (`specs/` and `changes/`).
    Openspec,
}

impl ProviderKind {
    /// The closed v1 kinds in canonical order.
    pub const KEYS: [Self; 1] = [Self::Openspec];

    /// The wire spelling.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Openspec => "openspec",
        }
    }

    /// Parse the wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|key| key.as_str() == text)
    }
}

/// One namespaced requirement source: the provider declaration. Sources
/// are the namespace of every requirement reference, so several providers
/// (several OpenSpec trees) coexist without id collisions.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProviderDecl {
    /// The namespaced source id of every reference resolved here.
    pub source: String,
    /// The provider implementation kind.
    pub kind: ProviderKind,
    /// The project-relative logical root of the provider tree.
    pub root: String,
}

/// One requirement reference: the explicit, read-only link between one
/// semantic symbol and one namespaced requirement identity, pinned to the
/// exact revision the author resolved.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct RequirementLink {
    /// The semantic id of the linked symbol.
    pub symbol: String,
    /// The relation the declaration claims.
    pub relation: Relation,
    /// The provider namespace.
    pub source: String,
    /// The requirement id inside the provider namespace.
    pub requirement: String,
    /// The pinned exact revision (`sha256:<64 lowercase hex>` of the
    /// requirement body).
    pub revision: Sha256Digest,
}

/// The bound source Model contract: exact accepted version plus digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRef {
    /// The exact Model contract version spelling (`0.1.0` or `1.0.0`).
    pub model_version: String,
    /// The SHA-256 over the canonical Model payload bytes.
    pub digest: Sha256Digest,
}

/// One finished requirements attachment: immutable, deterministically
/// ordered, and safe to share across threads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementsAttachment {
    project_id: SemanticId,
    model_ref: ModelRef,
    providers: Vec<ProviderDecl>,
    references: Vec<RequirementLink>,
}

impl RequirementsAttachment {
    /// Assemble from validated parts (crate internal); collections are
    /// stored in the caller's canonical order.
    pub(crate) fn assemble(
        project_id: SemanticId,
        model_ref: ModelRef,
        providers: Vec<ProviderDecl>,
        references: Vec<RequirementLink>,
    ) -> Self {
        Self {
            project_id,
            model_ref,
            providers,
            references,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &serde_json::Value) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        wire::from_value(json)
    }

    /// Parse one attachment document (exact UTF-8 JSON bytes) or return
    /// the terminal rejection set.
    pub fn parse(bytes: &[u8]) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        if bytes.len() > version::MAX_DOC_BYTES {
            return Err(diagnostic::export_limit("document-bytes", "max=8388608"));
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| diagnostic::io_failure("invalid-encoding"))?;
        let json = json::parse(text)?;
        Self::from_value(&json)
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound source Model contract.
    pub const fn model_ref(&self) -> &ModelRef {
        &self.model_ref
    }

    /// Every provider declaration, canonically ordered by source id.
    pub fn providers(&self) -> &[ProviderDecl] {
        &self.providers
    }

    /// Every requirement reference, canonically ordered by the full link
    /// tuple.
    pub fn references(&self) -> &[RequirementLink] {
        &self.references
    }

    /// The canonical payload bytes (compact JSON, byte-sorted keys,
    /// canonical collections, no trailing LF), or a typed refusal beyond
    /// the payload bound.
    pub fn canonical_bytes(&self) -> Result<String, crate::diagnostics::DiagnosticSet> {
        canonical_attachment_bytes(self)
    }

    /// Resolve this attachment against the project selection and the
    /// on-disk provider trees. Pure and read-only: the provider walk uses
    /// the confined no-follow filesystem capability and never writes.
    ///
    /// Terminal failures (invalid reference targets, invalid provider
    /// trees, unavailable trees that references depend on, Model pin
    /// custody mismatches, and loader/IR failures passed through
    /// unchanged) return `Err`. A resolution that completed carries its
    /// report plus the gate verdict: references may be stale, missing, or
    /// conflicted while the report itself stays the deliverable.
    pub fn resolve(&self, selection: &LoadSelection) -> Result<Resolution, DomainResult> {
        let root = crate::loader::root_for_selection(selection)?;
        let model_json = match crate::loader::run(selection, false) {
            DomainResult::Valid {
                payload: crate::result::SuccessPayload::Model { json, .. },
                ..
            } => json,
            other => return Err(other),
        };
        let model = crate::loader::normalize_model(selection)?;
        let compilation = crate::ir::compile(&model).map_err(|failure| failure.into_result())?;

        // Custody: the attachment binds exactly one project and one
        // Model state; anything else refuses before any resolution.
        if self.project_id.as_str()
            != compilation
                .project
                .project
                .as_ref()
                .map(|project| project.id.as_str())
                .unwrap_or_default()
        {
            return Err(DomainResult::denied(diagnostic::model_ref_mismatch(
                "project-id",
            )));
        }
        if self.model_ref.model_version != model.model_version.as_str() {
            return Err(DomainResult::denied(diagnostic::model_ref_mismatch(
                "model-version",
            )));
        }
        if self.model_ref.digest.as_str() != sha256_digest(model_json.as_bytes()) {
            return Err(DomainResult::denied(diagnostic::model_ref_mismatch(
                "model-digest",
            )));
        }

        // Reference targets must resolve inside the bound model and the
        // declared namespaces before any filesystem work.
        let mut symbols = std::collections::BTreeSet::new();
        if let Some(project) = &compilation.project.project {
            symbols.insert(project.id.as_str().to_owned());
        }
        for module in &compilation.project.modules {
            symbols.insert(module.id.as_str().to_owned());
        }
        for definition in &compilation.project.definitions {
            symbols.insert(definition.id().as_str().to_owned());
        }
        let sources: std::collections::BTreeSet<&str> = self
            .providers
            .iter()
            .map(|provider| provider.source.as_str())
            .collect();
        for link in &self.references {
            if !symbols.contains(link.symbol.as_str()) {
                return Err(DomainResult::invalid(diagnostic::ref_unknown(
                    "unknown-symbol",
                    Some(&link.symbol),
                )));
            }
            if !sources.contains(link.source.as_str()) {
                return Err(DomainResult::invalid(diagnostic::ref_unknown(
                    "unknown-source",
                    Some(&link.source),
                )));
            }
        }

        // Load every declared provider snapshot through the confined
        // read-only capability.
        let fs = crate::project_fs::Fs::open(&root).map_err(|_| {
            DomainResult::invalid(diagnostic::provider_invalid("root-unreadable", None))
        })?;
        let referenced: std::collections::BTreeSet<&str> = self
            .references
            .iter()
            .map(|link| link.source.as_str())
            .collect();
        let mut snapshots = Vec::new();
        for provider in &self.providers {
            match provider::load_snapshot(&fs, provider) {
                provider::Loaded::Resolved(snapshot) => snapshots.push(snapshot),
                provider::Loaded::Absent => {
                    if referenced.contains(provider.source.as_str()) {
                        return Err(DomainResult::unavailable(diagnostic::provider_unavailable(
                            "root-absent",
                            Some(&provider.source),
                        )));
                    }
                    snapshots.push(provider::Snapshot::absent(provider));
                }
                provider::Loaded::Invalid(detail) => {
                    return Err(DomainResult::invalid(diagnostic::provider_invalid(
                        detail,
                        Some(&provider.source),
                    )));
                }
            }
        }

        let requirements: usize = snapshots.iter().map(|s| s.entries.len()).sum();
        let conflicts: usize = snapshots.iter().map(|s| s.conflicts.len()).sum();
        if requirements > version::MAX_REQUIREMENTS || conflicts > version::MAX_REQUIREMENTS {
            return Err(DomainResult::invalid(diagnostic::export_limit(
                "aggregate-rows",
                "max=10000",
            )));
        }
        let resolution = report::build(self, &compilation, snapshots);
        resolution
            .report
            .canonical_bytes()
            .map_err(DomainResult::invalid)?;
        Ok(resolution)
    }
}

/// The canonical wire view of the attachment: borrowed strings in the
/// closed member set, serialized with byte-sorted object keys by the
/// canonical writer.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentWire<'a> {
    schema_version: &'static str,
    identity: &'static str,
    project_id: &'a str,
    model_ref: ModelRefWire<'a>,
    providers: Vec<ProviderWire<'a>>,
    references: Vec<LinkWire<'a>>,
}

/// The canonical wire view of the Model pin.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelRefWire<'a> {
    model_version: &'a str,
    digest: &'a str,
}

/// The canonical wire view of one provider declaration.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderWire<'a> {
    source: &'a str,
    kind: &'static str,
    root: &'a str,
}

/// The canonical wire view of one requirement reference.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkWire<'a> {
    symbol: &'a str,
    relation: &'static str,
    source: &'a str,
    requirement: &'a str,
    revision: &'a str,
}

impl RequirementsAttachment {
    /// The canonical wire view of this attachment.
    fn wire(&self) -> AttachmentWire<'_> {
        AttachmentWire {
            schema_version: version::SCHEMA_VERSION,
            identity: version::IDENTITY,
            project_id: self.project_id.as_str(),
            model_ref: ModelRefWire {
                model_version: &self.model_ref.model_version,
                digest: self.model_ref.digest.as_str(),
            },
            providers: self
                .providers
                .iter()
                .map(|provider| ProviderWire {
                    source: &provider.source,
                    kind: provider.kind.as_str(),
                    root: &provider.root,
                })
                .collect(),
            references: self
                .references
                .iter()
                .map(|link| LinkWire {
                    symbol: &link.symbol,
                    relation: link.relation.as_str(),
                    source: &link.source,
                    requirement: &link.requirement,
                    revision: link.revision.as_str(),
                })
                .collect(),
        }
    }
}

/// Compute the canonical attachment bytes: compact JSON with byte-sorted
/// object keys over the canonical wire view, or the export-limit refusal.
pub(crate) fn canonical_attachment_bytes(
    attachment: &RequirementsAttachment,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let bytes = canonical_value_bytes(&attachment.wire());
    if bytes.len() > version::MAX_EXPORT_BYTES {
        return Err(diagnostic::export_limit(
            "canonical-bytes",
            &format!("bytes={}", bytes.len()),
        ));
    }
    Ok(bytes)
}

/// Serialize any value into compact JSON with byte-sorted object keys
/// (the round trip through `serde_json::Value`'s ordered map), which is
/// the attachment and report canonical form.
pub(crate) fn canonical_value_bytes<T: serde::Serialize>(value: &T) -> String {
    let dynamic = serde_json::to_value(value).expect("requirements wire values serialize");
    serde_json::to_string(&dynamic).expect("canonical JSON bytes fit in memory")
}

/// The exact `sha256:<64 lowercase hex>` digest of some bytes.
pub(crate) fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// The bare 64 lowercase hex SHA-256 of some bytes.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Whether `text` is a legal provider namespace id (`^[a-z][a-z0-9-]{0,31}$`).
pub(crate) fn is_source_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest.len() <= 31
        && rest
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// Whether `text` is a legal requirement id:
/// `<capability>.REQ-<slug>` with kebab-case capability and slug parts
/// (`planner.REQ-focus-task`).
pub(crate) fn is_requirement_id(text: &str) -> bool {
    let Some((capability, slug)) = text.split_once(".REQ-") else {
        return false;
    };
    let ok_part = |bytes: &[u8]| {
        !bytes.is_empty()
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    };
    text.len() <= 128
        && capability.len() <= 63
        && slug.len() <= 64
        && capability
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && slug
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && ok_part(capability.as_bytes())
        && ok_part(slug.as_bytes())
}

/// The stable slug of one requirement title: lowercased ASCII, every
/// non-alphanumeric run collapsed to one `-`, trimmed; fails closed when
/// the title carries no sluggable characters or exceeds the slug bound.
pub(crate) fn title_slug(title: &str) -> Option<String> {
    if title.chars().count() > version::MAX_TITLE_CHARS {
        return None;
    }
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in title.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character);
        } else if character.is_ascii_uppercase() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() || slug.len() > 64 {
        return None;
    }
    Some(slug)
}

/// Whether `text` is a legal change-directory id.
pub(crate) fn is_change_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && rest.len() <= 127
        && rest.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

/// The canonical requirement id of one capability/title pair.
pub(crate) fn requirement_id(capability: &str, slug: &str) -> String {
    format!("{capability}.REQ-{slug}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relations_round_trip() {
        for key in Relation::KEYS {
            assert_eq!(Relation::parse(key.as_str()), Some(key));
        }
        assert_eq!(Relation::parse("derives"), None);
    }

    #[test]
    fn provider_kinds_round_trip() {
        for key in ProviderKind::KEYS {
            assert_eq!(ProviderKind::parse(key.as_str()), Some(key));
        }
        assert_eq!(ProviderKind::parse("jira"), None);
    }

    #[test]
    fn source_ids_are_namespaced() {
        assert!(is_source_id("openspec"));
        assert!(is_source_id("openspec-legacy"));
        assert!(!is_source_id("OpenSpec"));
        assert!(!is_source_id(""));
        assert!(!is_source_id("-lead"));
        assert!(!is_source_id("a..b"));
    }

    #[test]
    fn requirement_ids_are_capability_qualified() {
        assert!(is_requirement_id("planner.REQ-focus-task"));
        assert!(is_requirement_id("planner-focus.REQ-001"));
        assert!(!is_requirement_id("PLANNER.REQ-001"));
        assert!(!is_requirement_id("planner.REQ-"));
        assert!(!is_requirement_id("planner"));
        assert!(!is_requirement_id(".REQ-a"));
        assert!(!is_requirement_id("planner.REQ-focus_task"));
    }

    #[test]
    fn title_slugs_are_deterministic_kebab() {
        assert_eq!(title_slug("Focus task").as_deref(), Some("focus-task"));
        assert_eq!(
            title_slug("  Archive  &  restore -- tasks! ").as_deref(),
            Some("archive-restore-tasks")
        );
        assert_eq!(title_slug("!!!"), None);
        assert_eq!(title_slug(&"x".repeat(200)), None);
    }

    #[test]
    fn change_ids_are_bound() {
        assert!(is_change_id("2026-09-01-add-focus"));
        assert!(is_change_id("archive"));
        assert!(!is_change_id("/absolute"));
        assert!(!is_change_id(""));
        assert!(!is_change_id("UPPER"));
    }
}
