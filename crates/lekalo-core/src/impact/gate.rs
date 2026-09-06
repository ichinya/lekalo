//! Neutral targeted-gate selection for impact results (issue #16).
//!
//! Gates are selection facts for the owners that must re-run after a
//! change: semantic validation always, plus one gate per fired risk
//! dimension. The analyzer never runs a gate, never writes a plan, and
//! never changes task state. In the `strict` profile a required gate
//! resting on unknown or stale evidence is `blocked` and the whole result
//! denies (exit 3); in the `default` profile the same fact stays a visible
//! `unknown` warning.

use super::{
    Confidence, EvidenceState, GateItem, GateState, ImpactProfile, RiskDimension, RiskItem,
};

/// One gate selection computed from the risk vector.
pub(crate) fn select(
    risks: &[RiskItem],
    profile: ImpactProfile,
    unresolved_changed: bool,
) -> (Vec<GateItem>, bool) {
    let fired: Vec<(RiskDimension, Option<&RiskItem>)> = RiskDimension::ALL
        .iter()
        .map(|dimension| {
            (
                *dimension,
                risks.iter().find(|risk| risk.dimension == *dimension),
            )
        })
        .collect();

    let mut items = Vec::new();

    // The semantic validation gate is always required: every change
    // re-runs it.
    let validation_evidence = if unresolved_changed {
        EvidenceState::Unknown
    } else {
        EvidenceState::Canonical
    };
    items.push(GateItem {
        gate_id: "impact.gate.semantic-validate".to_owned(),
        owner: "validation",
        reason_refs: vec![super::risk::reason::CHANGED_INPUT.to_owned()],
        required: true,
        state: gate_state(true, validation_evidence, profile),
        evidence_state: validation_evidence,
        confidence: confidence_of(validation_evidence),
    });

    for (dimension, risk) in fired {
        let Some(risk) = risk else {
            items.push(GateItem {
                gate_id: gate_id(dimension),
                owner: owner(dimension),
                reason_refs: vec![dimension_reason(dimension).to_owned()],
                required: false,
                state: GateState::NotRequired,
                evidence_state: EvidenceState::Canonical,
                confidence: Confidence::Canonical,
            });
            continue;
        };
        let evidence_state = evidence_of(risk.state);
        let required = risk.required || dimension == RiskDimension::PublicContract;
        items.push(GateItem {
            gate_id: gate_id(dimension),
            owner: owner(dimension),
            reason_refs: risk.reason_refs.clone(),
            required,
            state: gate_state(required, evidence_state, profile),
            evidence_state,
            confidence: risk.confidence,
        });
    }

    items.sort_by(|left, right| left.gate_id.cmp(&right.gate_id));
    let blocked = items.iter().any(|gate| gate.state == GateState::Blocked);
    (items, blocked)
}

/// The gate ids every affected item must carry: semantic validation
/// always, plus the required gates of the item's fired dimensions.
pub(crate) fn required_refs(gates: &[GateItem], dimensions: &[RiskDimension]) -> Vec<String> {
    let mut refs: Vec<String> = gates
        .iter()
        .filter(|gate| {
            gate.gate_id == "impact.gate.semantic-validate"
                || dimensions
                    .iter()
                    .any(|dimension| gate.gate_id == gate_id_for(*dimension))
        })
        .filter(|gate| gate.required)
        .map(|gate| gate.gate_id.clone())
        .collect();
    refs.sort();
    refs.dedup();
    refs
}

/// The gate id of one risk dimension (the analyzer backfill needs it
/// without rebuilding the whole selection).
pub(crate) fn gate_id_for(dimension: RiskDimension) -> String {
    gate_id(dimension)
}

/// Whether any required gate rests on unknown or stale evidence.
pub(crate) fn has_required_unknown_evidence(gates: &[GateItem]) -> bool {
    gates.iter().any(|gate| {
        gate.required
            && matches!(
                gate.evidence_state,
                EvidenceState::Unknown | EvidenceState::Stale
            )
    })
}

fn gate_state(required: bool, evidence: EvidenceState, profile: ImpactProfile) -> GateState {
    if !required {
        return GateState::NotRequired;
    }
    match evidence {
        EvidenceState::Unknown | EvidenceState::Stale => match profile {
            ImpactProfile::Strict => GateState::Blocked,
            ImpactProfile::Default => GateState::Unknown,
        },
        _ => GateState::Selected,
    }
}

fn evidence_of(state: super::SectionState) -> EvidenceState {
    match state {
        super::SectionState::Complete => EvidenceState::Canonical,
        super::SectionState::Stale => EvidenceState::Stale,
        super::SectionState::Unknown
        | super::SectionState::Incomplete
        | super::SectionState::Conflicting
        | super::SectionState::Unsupported => EvidenceState::Unknown,
    }
}

fn confidence_of(evidence: EvidenceState) -> Confidence {
    match evidence {
        EvidenceState::Canonical => Confidence::Canonical,
        EvidenceState::Verified => Confidence::Verified,
        EvidenceState::Extracted => Confidence::Extracted,
        EvidenceState::Inferred => Confidence::Inferred,
        EvidenceState::Stale | EvidenceState::Unknown => Confidence::Unknown,
    }
}

fn gate_id(dimension: RiskDimension) -> String {
    format!(
        "impact.gate.{}",
        match dimension {
            RiskDimension::PublicContract => "public-contract",
            RiskDimension::MigrationData => "migration",
            RiskDimension::Transaction => "transaction",
            RiskDimension::Authorization => "authorization",
            RiskDimension::Portability => "portability",
            RiskDimension::Effects => "effect-review",
            RiskDimension::BindingsArtifacts => "binding-artifact",
            RiskDimension::ScenariosTests => "scenario",
        }
    )
}

fn owner(dimension: RiskDimension) -> &'static str {
    match dimension {
        RiskDimension::PublicContract => "public",
        RiskDimension::MigrationData => "migration",
        RiskDimension::Transaction => "transaction",
        RiskDimension::Authorization => "authorization",
        RiskDimension::Portability => "portability",
        RiskDimension::Effects => "effects",
        RiskDimension::BindingsArtifacts => "bindings",
        RiskDimension::ScenariosTests => "scenarios",
    }
}

fn dimension_reason(dimension: RiskDimension) -> &'static str {
    match dimension {
        RiskDimension::PublicContract => super::risk::reason::PUBLIC_VISIBILITY,
        RiskDimension::MigrationData => super::risk::reason::RENAME_HISTORY,
        RiskDimension::Transaction => super::risk::reason::TRANSACTION_GROUP,
        RiskDimension::Authorization => super::risk::reason::AUTHORIZATION_POLICY,
        RiskDimension::Portability => super::risk::reason::TARGET_SPECIFIC,
        RiskDimension::Effects => super::risk::reason::EFFECT_WRITE,
        RiskDimension::BindingsArtifacts => super::risk::reason::BINDING_REFERENCED,
        RiskDimension::ScenariosTests => super::risk::reason::SCENARIO_COVERS,
    }
}
