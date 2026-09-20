//! Issue #87: data classification as a closed, versioned project
//! attachment.
//!
//! One independent, immutable contract family binding every
//! classification subject of one project — definition, field, and
//! payload paths over the bound IR — to its closed data kind, with
//! per-profile secure defaults and subject-bound declassification
//! grants. The attachment is metadata-only declaration data: it never
//! carries, quotes, or embeds field values, source text, physical
//! paths, runtime principals, tokens, secrets, provider transcripts,
//! or Model/IR bytes. Subjects address Model symbols and bounded
//! field paths resolved against the pinned compilation; unknown
//! subjects reject.
//!
//! Resolution is total and deterministic (plan §2.3): for any concrete
//! field the resolved classification is the first match of the exact
//! field-path entry, the field entry, the containing definition
//! default, the operation default, and the profile defaults. No entry
//! plus no covering default is the `unclassified` state — distinct
//! from every kind — which the strict profile rejects on sensitive
//! sinks. Propagation widens to the highest contributing kind and
//! never silently lowers; lowering happens only through an explicit
//! reviewed grant, and `credential` never lowers.
//!
//! Authority and boundaries: classification is Lekalo-owned semantic
//! data. It binds by `modelRef`/`irRef` custody plus symbol
//! references; the closed Model/IR is never extended. Reader, writer,
//! destination, sink-ceiling, masking, retention, export, and
//! encryption policy live in the sibling classification-policy
//! family (`dev.lekalo.classification-policy@0.4.0`), not here.

pub mod diagnostic;
pub mod policy;
pub mod resolve;
pub mod types;
pub mod version;

mod canonical;
mod json;
pub mod wire;

pub use policy::{
    CrossTenantMode, DestinationKind, KindRule, MaskingStrategy, PolicyAttachment, SinkCeiling,
    SinkName,
};
pub use resolve::{Resolution, ResolvedKind, SensitivityMark};
pub use types::{
    BoundedText, Condition, ContractRef, DataKind, IsoTimestamp, Label, PolicyRef, Profile,
    QuestionId, ReviewRef, RetentionClass, SubjectPath, VocabularyError,
};
pub use version::{
    FAMILY, IDENTITY, IR_IDENTITY, MAX_CANONICAL_BYTES, MAX_CLASSIFICATIONS, MAX_CONDITIONS,
    MAX_DECLASSIFICATIONS, MAX_DOC_BYTES, MAX_EXPORT_BYTES, MAX_LABELS, MAX_OPEN_QUESTIONS,
    MAX_SUBJECT_SEGMENTS, MODEL_VERSION, SCHEMA_VERSION, VERSION,
};

pub use wire::{
    Attachment, Classification, Declassification, Defaults, OpenQuestion,
};

use crate::diagnostics::DiagnosticSet;

/// The canonical export of one classification attachment: compact
/// JSON with byte-sorted keys, canonical collections, and no trailing
/// LF, or the export-limit refusal.
pub fn attachment_canonical_bytes(attachment: &Attachment) -> Result<String, DiagnosticSet> {
    let bytes = canonical::canonical_value_bytes(&attachment.wire());
    canonical::check_export_bound(&bytes)?;
    Ok(bytes)
}

/// The canonical export of one classification-policy attachment.
pub fn policy_canonical_bytes(policy: &PolicyAttachment) -> Result<String, DiagnosticSet> {
    let bytes = canonical::canonical_value_bytes(&policy.wire());
    canonical::check_export_bound(&bytes)?;
    Ok(bytes)
}

/// The exact `sha256:<64 lowercase hex>` digest of the canonical
/// attachment bytes.
pub fn attachment_digest(attachment: &Attachment) -> Result<String, DiagnosticSet> {
    Ok(format!(
        "sha256:{}",
        canonical::sha256_hex(attachment_canonical_bytes(attachment)?.as_bytes())
    ))
}

/// The exact `sha256:<64 lowercase hex>` digest of the canonical
/// policy bytes.
pub fn policy_digest(policy: &PolicyAttachment) -> Result<String, DiagnosticSet> {
    Ok(format!(
        "sha256:{}",
        canonical::sha256_hex(policy_canonical_bytes(policy)?.as_bytes())
    ))
}
