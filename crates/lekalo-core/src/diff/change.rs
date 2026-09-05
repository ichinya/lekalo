//! The closed change taxonomy and typed change records (issue #18).
//!
//! One record per independently observable semantic change. The change
//! identity is a `sha256` digest over the side, the stable subject
//! identity, the taxonomy kind, and the canonical before/after summaries —
//! never an array position or rendered text. Members and references are
//! order-normalized in the projection, so no record depends on source
//! order.

use super::identity::Subject;
use super::version::MAX_HISTORY_HOPS;

/// The closed change taxonomy: exactly the kinds the accepted Model
/// (0.1.0 / 1.0.0) can produce. Families without a declared source in the
/// accepted contracts (standalone invariants, error unions, storage
/// projections, artifact manifests) have no kinds here; activating them
/// requires a Diff contract successor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ChangeKind {
    SymbolAdded,
    SymbolRemoved,
    SymbolRenamed,
    SymbolReplaced,
    SymbolTombstoned,
    SymbolKindChanged,
    SymbolVisibilityChanged,
    SymbolPortabilityChanged,
    SymbolVersionChanged,
    SymbolDerivedFromChanged,
    ModuleImportsChanged,
    FieldAdded,
    FieldRemoved,
    FieldTypeChanged,
    FieldNullabilityChanged,
    FieldPresenceChanged,
    TypeShapeChanged,
    TypeMemberAdded,
    TypeMemberRemoved,
    SignatureInputAdded,
    SignatureInputRemoved,
    SignatureInputTypeChanged,
    SignatureInputPresenceChanged,
    SignatureOutputChanged,
    SignatureReadsChanged,
    SignatureEffectsChanged,
    InvariantIdentityChanged,
    PolicyDecisionChanged,
    PolicyScopeChanged,
    EffectOperationChanged,
    EffectEntityChanged,
    EffectEmitsChanged,
    EndpointMethodChanged,
    EndpointPathChanged,
    EndpointInvokesChanged,
    ScenarioSummaryChanged,
    ScenarioCoversChanged,
    BindingTargetChanged,
}

impl ChangeKind {
    /// The exact wire spelling (`family.kind`).
    pub const fn key(self) -> &'static str {
        match self {
            Self::SymbolAdded => "symbol.added",
            Self::SymbolRemoved => "symbol.removed",
            Self::SymbolRenamed => "symbol.renamed",
            Self::SymbolReplaced => "symbol.replaced",
            Self::SymbolTombstoned => "symbol.tombstoned",
            Self::SymbolKindChanged => "symbol.kind-changed",
            Self::SymbolVisibilityChanged => "symbol.visibility-changed",
            Self::SymbolPortabilityChanged => "symbol.portability-changed",
            Self::SymbolVersionChanged => "symbol.version-changed",
            Self::SymbolDerivedFromChanged => "symbol.derived-from-changed",
            Self::ModuleImportsChanged => "module.imports-changed",
            Self::FieldAdded => "field.added",
            Self::FieldRemoved => "field.removed",
            Self::FieldTypeChanged => "field.type-changed",
            Self::FieldNullabilityChanged => "field.nullability-changed",
            Self::FieldPresenceChanged => "field.presence-changed",
            Self::TypeShapeChanged => "type.shape-changed",
            Self::TypeMemberAdded => "type.member-added",
            Self::TypeMemberRemoved => "type.member-removed",
            Self::SignatureInputAdded => "signature.input-added",
            Self::SignatureInputRemoved => "signature.input-removed",
            Self::SignatureInputTypeChanged => "signature.input-type-changed",
            Self::SignatureInputPresenceChanged => "signature.input-presence-changed",
            Self::SignatureOutputChanged => "signature.output-changed",
            Self::SignatureReadsChanged => "signature.reads-changed",
            Self::SignatureEffectsChanged => "signature.effects-changed",
            Self::InvariantIdentityChanged => "invariant.identity-changed",
            Self::PolicyDecisionChanged => "policy.decision-changed",
            Self::PolicyScopeChanged => "policy.scope-changed",
            Self::EffectOperationChanged => "effect.operation-changed",
            Self::EffectEntityChanged => "effect.entity-changed",
            Self::EffectEmitsChanged => "effect.emits-changed",
            Self::EndpointMethodChanged => "endpoint.method-changed",
            Self::EndpointPathChanged => "endpoint.path-changed",
            Self::EndpointInvokesChanged => "endpoint.invokes-changed",
            Self::ScenarioSummaryChanged => "scenario.summary-changed",
            Self::ScenarioCoversChanged => "scenario.covers-changed",
            Self::BindingTargetChanged => "binding.target-changed",
        }
    }

    /// The kind whose wire spelling equals `key`, or `None`.
    pub fn from_key(key: &str) -> Option<Self> {
        ALL.iter().copied().find(|kind| kind.key() == key)
    }
}

/// Every kind in canonical (wire-spelling byte) order.
pub const ALL: [ChangeKind; 38] = [
    ChangeKind::SymbolAdded,
    ChangeKind::SymbolRemoved,
    ChangeKind::SymbolRenamed,
    ChangeKind::SymbolReplaced,
    ChangeKind::SymbolTombstoned,
    ChangeKind::SymbolKindChanged,
    ChangeKind::SymbolVisibilityChanged,
    ChangeKind::SymbolPortabilityChanged,
    ChangeKind::SymbolVersionChanged,
    ChangeKind::SymbolDerivedFromChanged,
    ChangeKind::ModuleImportsChanged,
    ChangeKind::FieldAdded,
    ChangeKind::FieldRemoved,
    ChangeKind::FieldTypeChanged,
    ChangeKind::FieldNullabilityChanged,
    ChangeKind::FieldPresenceChanged,
    ChangeKind::TypeShapeChanged,
    ChangeKind::TypeMemberAdded,
    ChangeKind::TypeMemberRemoved,
    ChangeKind::SignatureInputAdded,
    ChangeKind::SignatureInputRemoved,
    ChangeKind::SignatureInputTypeChanged,
    ChangeKind::SignatureInputPresenceChanged,
    ChangeKind::SignatureOutputChanged,
    ChangeKind::SignatureReadsChanged,
    ChangeKind::SignatureEffectsChanged,
    ChangeKind::InvariantIdentityChanged,
    ChangeKind::PolicyDecisionChanged,
    ChangeKind::PolicyScopeChanged,
    ChangeKind::EffectOperationChanged,
    ChangeKind::EffectEntityChanged,
    ChangeKind::EffectEmitsChanged,
    ChangeKind::EndpointMethodChanged,
    ChangeKind::EndpointPathChanged,
    ChangeKind::EndpointInvokesChanged,
    ChangeKind::ScenarioSummaryChanged,
    ChangeKind::ScenarioCoversChanged,
    ChangeKind::BindingTargetChanged,
];

/// The comparison side a record belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Side {
    /// Present only in the base projection.
    Base,
    /// Present only in the candidate projection.
    Candidate,
    /// Present on both sides with different content.
    Both,
}

impl Side {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Candidate => "candidate",
            Self::Both => "both",
        }
    }
}

/// One typed before/after summary value: a bounded string, boolean,
/// integer, semantic-id array, or one of the closed rename/replacement
/// objects. `None` renders as JSON `null`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Summary {
    Text(String),
    Flag(bool),
    Count(u64),
    Ids(Vec<String>),
    Renamed { renamed_from: Vec<String> },
    Replaced { replaced_by: String },
    Tombstoned { since: u64 },
    None,
}

/// One typed change record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeRecord {
    pub(crate) change_id: String,
    pub(crate) kind: ChangeKind,
    pub(crate) side: Side,
    pub(crate) subject: Subject,
    pub(crate) before: Summary,
    pub(crate) after: Summary,
    pub(crate) reasons: Vec<String>,
}

impl ChangeRecord {
    /// The stable `sha256:` change identity.
    pub fn change_id(&self) -> &str {
        &self.change_id
    }

    /// The closed taxonomy kind.
    pub const fn kind(&self) -> ChangeKind {
        self.kind
    }

    /// The comparison side.
    pub const fn side(&self) -> Side {
        self.side
    }

    /// The stable semantic subject.
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The typed before summary.
    pub const fn before(&self) -> &Summary {
        &self.before
    }

    /// The typed after summary.
    pub const fn after(&self) -> &Summary {
        &self.after
    }

    /// The direct, stable reason identifiers in canonical order.
    pub fn reasons(&self) -> &[String] {
        &self.reasons
    }

    /// Assemble one record with its digest derived from the canonical
    /// identity parts; reasons are sorted and deduplicated.
    pub(crate) fn assemble(
        kind: ChangeKind,
        side: Side,
        subject: Subject,
        before: Summary,
        after: Summary,
        mut reasons: Vec<String>,
    ) -> Self {
        // Every record names its own taxonomy kind first: the most direct
        // fact, present even when richer history or profile reasons join.
        reasons.push(super::reason::kind_reason(kind.key()));
        reasons.sort();
        reasons.dedup();
        let change_id = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            side.key(),
            subject.identity(),
            kind.key(),
            summary_identity(&before),
            summary_identity(&after)
        );
        Self {
            change_id: format!("sha256:{}", super::digest_hex(change_id.as_bytes())),
            kind,
            side,
            subject,
            before,
            after,
            reasons,
        }
    }
}

/// The digest preimage of one summary: type tag plus bounded content.
pub(crate) fn summary_identity(summary: &Summary) -> String {
    match summary {
        Summary::Text(value) => format!("t\u{1f}{value}"),
        Summary::Flag(value) => format!("b\u{1f}{value}"),
        Summary::Count(value) => format!("n\u{1f}{value}"),
        Summary::Ids(values) => {
            let mut identity = String::from("a\u{1f}");
            for value in values {
                identity.push_str(value);
                identity.push('\u{1e}');
            }
            identity
        }
        Summary::Renamed { renamed_from } => {
            let mut identity = String::from("r\u{1f}");
            for value in renamed_from {
                identity.push_str(value);
                identity.push('\u{1e}');
            }
            identity
        }
        Summary::Replaced { replaced_by } => format!("p\u{1f}{replaced_by}"),
        Summary::Tombstoned { since } => format!("d\u{1f}{since}"),
        Summary::None => "0".to_owned(),
    }
}

/// The hop bound every rename-history walk obeys.
pub(crate) const HISTORY_HOPS: usize = MAX_HISTORY_HOPS;
