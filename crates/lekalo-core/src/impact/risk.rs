//! The closed impact risk vector (issue #16).
//!
//! Nine independent dimensions — public contract, migration and data,
//! transaction, authorization, portability, effects, bindings and
//! artifacts, scenarios and tests — computed only from accepted canonical
//! facts (IR visibility and history, graph relations, effect edges).
//! Absent, stale, or unknown evidence marks a dimension unknown; it never
//! downgrades to safe. Each item carries stable reason references from the
//! closed `impact.reason.*` vocabulary and bounded evidence references.

use std::collections::BTreeMap;

use crate::graph::NodeId;
use crate::ir::{Definition, Portability, Visibility};

use super::input::MemberChange;
use super::{Confidence, RiskDimension, SectionState};

/// The closed reason-token vocabulary (stable `impact.reason.*` refs).
pub mod reason {
    /// Discovered through the typed changed-input handoff.
    pub const CHANGED_INPUT: &str = "impact.reason.changed-input";
    /// Discovered along a dependency edge.
    pub const DEPENDENCY_EDGE: &str = "impact.reason.dependency-edge";
    /// Public through its declared visibility.
    pub const PUBLIC_VISIBILITY: &str = "impact.reason.public-visibility";
    /// Public through its definition kind (endpoint, event, query).
    pub const KIND_PUBLIC_SURFACE: &str = "impact.reason.kind-public-surface";
    /// Named in the project rename history.
    pub const RENAME_HISTORY: &str = "impact.reason.rename-history";
    /// Recorded tombstone in the project registry.
    pub const TOMBSTONE: &str = "impact.reason.tombstone";
    /// A typed member seed marked the field removed.
    pub const FIELD_REMOVAL: &str = "impact.reason.field-removal";
    /// A typed member seed narrowed the field type.
    pub const FIELD_TYPE_NARROWED: &str = "impact.reason.field-type-narrowed";
    /// An affected operation writes the changed subject.
    pub const EFFECT_WRITE: &str = "impact.reason.effect-write";
    /// An affected operation reads the changed subject.
    pub const EFFECT_READ: &str = "impact.reason.effect-read";
    /// An affected operation emits an event.
    pub const EVENT_EMISSION: &str = "impact.reason.event-emission";
    /// Affected effects share a transaction group.
    pub const TRANSACTION_GROUP: &str = "impact.reason.transaction-group";
    /// No transaction evidence exists for an affected operation.
    pub const TRANSACTION_UNKNOWN: &str = "impact.reason.transaction-unknown";
    /// A policy applies to the affected operation.
    pub const AUTHORIZATION_POLICY: &str = "impact.reason.authorization-policy";
    /// No policy mapping exists for the affected operation.
    pub const AUTHORIZATION_UNMAPPED: &str = "impact.reason.authorization-unmapped";
    /// The affected definition is target-specific.
    pub const TARGET_SPECIFIC: &str = "impact.reason.target-specific";
    /// A target binding covers the affected module scope.
    pub const BINDING_REFERENCED: &str = "impact.reason.binding-referenced";
    /// A scenario covers the affected operation.
    pub const SCENARIO_COVERS: &str = "impact.reason.scenario-covers";
    /// The affected surface belongs to one module scope.
    pub const MODULE_SCOPE: &str = "impact.reason.module-scope";
}

/// Everything the risk vector needs to know about one semantic symbol.
#[derive(Clone, Debug)]
pub struct SymbolFact {
    /// The definition kind wire key (empty for structural nodes).
    pub kind: &'static str,
    /// The typed subkind (`command`/`query`), when the kind carries one.
    pub subkind: Option<&'static str>,
    /// The declared visibility (structural nodes are public).
    pub visibility: Visibility,
    /// The declared portability, when declared.
    pub portability: Option<Portability>,
    /// The declared historical ids of this symbol.
    pub renamed_from: Vec<String>,
    /// Whether the project registry tombstoned this id.
    pub tombstoned: bool,
}

/// Compute the risk vector from the collected radius facts.
///
/// `radius` maps every affected subject to its facts; `operations` carries
/// the effect observations of the affected operations (writes, reads,
/// emissions, transaction groups, degraded evidence); `bindings` carries
/// the affected-module target bindings; `seeds` are the typed member
/// seeds; `unresolved_changed` counts changed entries that resolved to no
/// symbols.
pub(crate) fn compute(
    radius: &BTreeMap<NodeId, SymbolFact>,
    operations: &[(NodeId, OperationObservation)],
    bindings: &[(NodeId, String)],
    seeds: &[super::MemberSeed],
    unresolved_changed: usize,
) -> Vec<super::RiskItem> {
    let mut items: Vec<super::RiskItem> = Vec::new();

    // public_contract: any public affected symbol is a contract surface.
    let public_subjects: Vec<NodeId> = radius
        .iter()
        .filter(|(_, fact)| is_public(fact) || is_kind_public(fact))
        .map(|(node, _)| node.clone())
        .collect();
    if !public_subjects.is_empty() {
        items.push(super::RiskItem {
            dimension: RiskDimension::PublicContract,
            state: SectionState::Complete,
            reason_refs: vec![
                reason::PUBLIC_VISIBILITY.to_owned(),
                reason::KIND_PUBLIC_SURFACE.to_owned(),
            ],
            subject_refs: public_subjects,
            evidence_refs: Vec::new(),
            required: true,
            confidence: Confidence::Canonical,
        });
    }

    // migration_data: history, tombstones, and typed member seeds.
    let mut migration_subjects: Vec<NodeId> = Vec::new();
    let mut migration_reasons: Vec<String> = Vec::new();
    let mut migration_state = SectionState::Complete;
    for (node, fact) in radius {
        if !fact.renamed_from.is_empty() {
            migration_subjects.push(node.clone());
            if !migration_reasons.contains(&reason::RENAME_HISTORY.to_owned()) {
                migration_reasons.push(reason::RENAME_HISTORY.to_owned());
            }
        }
        if fact.tombstoned {
            migration_subjects.push(node.clone());
            if !migration_reasons.contains(&reason::TOMBSTONE.to_owned()) {
                migration_reasons.push(reason::TOMBSTONE.to_owned());
            }
        }
    }
    if !seeds.is_empty() {
        for seed in seeds {
            let reason = match seed.change() {
                MemberChange::Removed => reason::FIELD_REMOVAL,
                MemberChange::TypeNarrowed => reason::FIELD_TYPE_NARROWED,
            };
            if !migration_reasons.contains(&reason.to_owned()) {
                migration_reasons.push(reason.to_owned());
            }
        }
        if let Some(entity) = seeds.first().map(super::MemberSeed::entity) {
            if let Some(node) = radius.keys().find(|node| node.semantic_id() == entity) {
                if !migration_subjects.contains(node) {
                    migration_subjects.push(node.clone());
                }
            }
        }
    }
    if unresolved_changed > 0 {
        migration_state = SectionState::Unknown;
    }
    if !migration_subjects.is_empty() || migration_state == SectionState::Unknown {
        migration_reasons.sort();
        items.push(super::RiskItem {
            dimension: RiskDimension::MigrationData,
            state: migration_state,
            subject_refs: migration_subjects,
            reason_refs: migration_reasons,
            evidence_refs: Vec::new(),
            required: true,
            confidence: if migration_state == SectionState::Complete {
                Confidence::Canonical
            } else {
                Confidence::Unknown
            },
        });
    }

    // transaction: known groups are facts; absent evidence is unknown.
    let mut transaction_subjects: Vec<NodeId> = Vec::new();
    let mut transaction_reasons: Vec<String> = Vec::new();
    let mut transaction_confidence = Confidence::Canonical;
    let mut transaction_state = SectionState::Complete;
    for (node, observation) in operations {
        if observation.transaction_group.is_some() {
            transaction_subjects.push(node.clone());
            if !transaction_reasons
                .iter()
                .any(|entry| entry == reason::TRANSACTION_GROUP)
            {
                transaction_reasons.push(reason::TRANSACTION_GROUP.to_owned());
            }
            if observation.degraded_evidence {
                transaction_state = SectionState::Stale;
                transaction_confidence = Confidence::Unknown;
            }
        } else if observation.mutates && !observation.has_transaction_evidence {
            transaction_subjects.push(node.clone());
            if !transaction_reasons
                .iter()
                .any(|entry| entry == reason::TRANSACTION_UNKNOWN)
            {
                transaction_reasons.push(reason::TRANSACTION_UNKNOWN.to_owned());
            }
            transaction_state = SectionState::Unknown;
            transaction_confidence = Confidence::Unknown;
        }
    }
    transaction_reasons.sort();
    if !transaction_subjects.is_empty() {
        items.push(super::RiskItem {
            dimension: RiskDimension::Transaction,
            state: transaction_state,
            subject_refs: transaction_subjects,
            reason_refs: transaction_reasons,
            evidence_refs: Vec::new(),
            required: transaction_state != SectionState::Complete,
            confidence: transaction_confidence,
        });
    }

    // authorization: policy mappings are facts; unmapped commands are
    // unknown and never optimistic.
    let mut authorization_subjects: Vec<NodeId> = Vec::new();
    let mut authorization_reasons: Vec<String> = Vec::new();
    let mut authorization_state = SectionState::Complete;
    for (node, observation) in operations {
        if observation.has_policy {
            authorization_subjects.push(node.clone());
            if !authorization_reasons
                .iter()
                .any(|entry| entry == reason::AUTHORIZATION_POLICY)
            {
                authorization_reasons.push(reason::AUTHORIZATION_POLICY.to_owned());
            }
        } else if observation.is_command {
            authorization_subjects.push(node.clone());
            if !authorization_reasons
                .iter()
                .any(|entry| entry == reason::AUTHORIZATION_UNMAPPED)
            {
                authorization_reasons.push(reason::AUTHORIZATION_UNMAPPED.to_owned());
            }
            authorization_state = SectionState::Unknown;
        }
    }
    authorization_reasons.sort();
    if !authorization_subjects.is_empty() {
        items.push(super::RiskItem {
            dimension: RiskDimension::Authorization,
            state: authorization_state,
            subject_refs: authorization_subjects,
            reason_refs: authorization_reasons,
            evidence_refs: Vec::new(),
            required: authorization_state == SectionState::Unknown,
            confidence: if authorization_state == SectionState::Complete {
                Confidence::Canonical
            } else {
                Confidence::Unknown
            },
        });
    }

    // portability: target-specific affected definitions.
    let portability_subjects: Vec<NodeId> = radius
        .iter()
        .filter(|(_, fact)| fact.portability == Some(Portability::TargetSpecific))
        .map(|(node, _)| node.clone())
        .collect();
    if !portability_subjects.is_empty() {
        items.push(super::RiskItem {
            dimension: RiskDimension::Portability,
            state: SectionState::Complete,
            reason_refs: vec![reason::TARGET_SPECIFIC.to_owned()],
            subject_refs: portability_subjects,
            evidence_refs: Vec::new(),
            required: false,
            confidence: Confidence::Canonical,
        });
    }

    // effects: observed effect surface of the affected operations.
    let mut effect_subjects: Vec<NodeId> = Vec::new();
    let mut effect_reasons: Vec<String> = Vec::new();
    let mut effect_state = SectionState::Complete;
    for (node, observation) in operations {
        if observation.writes || observation.reads || observation.emits {
            effect_subjects.push(node.clone());
            for (flag, reason) in [
                (observation.writes, reason::EFFECT_WRITE),
                (observation.reads, reason::EFFECT_READ),
                (observation.emits, reason::EVENT_EMISSION),
            ] {
                if flag && !effect_reasons.iter().any(|entry| entry == reason) {
                    effect_reasons.push(reason.to_owned());
                }
            }
            if observation.degraded_evidence {
                effect_state = SectionState::Stale;
            }
        }
    }
    effect_reasons.sort();
    if !effect_subjects.is_empty() {
        items.push(super::RiskItem {
            dimension: RiskDimension::Effects,
            state: effect_state,
            subject_refs: effect_subjects,
            reason_refs: effect_reasons,
            evidence_refs: Vec::new(),
            required: effect_state == SectionState::Stale,
            confidence: if effect_state == SectionState::Complete {
                Confidence::Canonical
            } else {
                Confidence::Unknown
            },
        });
    }

    // bindings_artifacts: affected-module target bindings; generated
    // artifacts have no accepted manifest evidence in v1.
    if !bindings.is_empty() {
        items.push(super::RiskItem {
            dimension: RiskDimension::BindingsArtifacts,
            state: SectionState::Unknown,
            reason_refs: vec![reason::BINDING_REFERENCED.to_owned()],
            subject_refs: bindings.iter().map(|(node, _)| node.clone()).collect(),
            evidence_refs: Vec::new(),
            required: false,
            confidence: Confidence::Unknown,
        });
    }

    // scenarios_tests: affected scenarios are facts; tests have no
    // accepted evidence surface in v1.
    let scenario_subjects: Vec<NodeId> = radius
        .iter()
        .filter(|(_, fact)| fact.kind == "scenario")
        .map(|(node, _)| node.clone())
        .collect();
    if !scenario_subjects.is_empty() {
        items.push(super::RiskItem {
            dimension: RiskDimension::ScenariosTests,
            state: SectionState::Unknown,
            reason_refs: vec![reason::SCENARIO_COVERS.to_owned()],
            subject_refs: scenario_subjects,
            evidence_refs: Vec::new(),
            required: false,
            confidence: Confidence::Unknown,
        });
    }

    items.sort_by(|left, right| left.dimension.key().cmp(right.dimension.key()));
    items
}

/// Whether one symbol fact is public (anything not explicitly
/// module-scoped).
pub(crate) fn is_public(fact: &SymbolFact) -> bool {
    fact.visibility != Visibility::Module
}

/// Whether one subject is on the always-public wire surface by kind.
pub(crate) fn is_kind_public(fact: &SymbolFact) -> bool {
    matches!(
        fact.kind,
        "endpoint" | "event" | "query" | "command" | "target-binding"
    )
}

/// The effect observations of one affected operation.
#[derive(Clone, Debug, Default)]
pub struct OperationObservation {
    pub writes: bool,
    pub reads: bool,
    pub emits: bool,
    pub mutates: bool,
    pub is_command: bool,
    pub has_policy: bool,
    pub has_transaction_evidence: bool,
    pub transaction_group: Option<String>,
    pub degraded_evidence: bool,
}

/// Convert one IR definition into its symbol fact.
pub(crate) fn fact_of(definition: &Definition) -> SymbolFact {
    let common = definition.common();
    SymbolFact {
        kind: definition.kind().as_str(),
        subkind: None,
        visibility: common.visibility.unwrap_or(Visibility::Project),
        portability: common.portability,
        renamed_from: common
            .renamed_from
            .iter()
            .map(|symbol| symbol.as_str().to_owned())
            .collect(),
        tombstoned: false,
    }
}
