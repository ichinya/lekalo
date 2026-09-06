//! Evidence, confidence, and completeness projections (issue #16).
//!
//! Every result section reports where its facts came from and how much
//! they can be trusted. Closed vocabularies only: the #13 confidence
//! ranks, the evidence states, and the section states. Missing, stale, or
//! unsupported surfaces are explicit; an unavailable optional projection
//! is never an empty array pretending no facts exist.

use super::{Confidence, EvidenceSurface, SectionState, SurfaceKind};

/// One assembled evidence section.
pub(crate) struct EvidenceReport {
    pub surfaces: Vec<EvidenceSurface>,
    pub degraded: bool,
}

/// Assemble the evidence surfaces from the analysis facts.
///
/// `graph_canonical` marks the dependency-graph surface; `effect_degraded`
/// marks stale or unknown detected effect evidence; the manifests and
/// tests surfaces are structurally unavailable in v1 (recorded as
/// unsupported/unknown facts — they do not by themselves degrade the
/// top-level completeness; the owner decision is recorded in ADR-0017).
pub(crate) fn report(
    graph_canonical: bool,
    effect_degraded: bool,
    has_detected_envelopes: bool,
    changed_mode: bool,
    changed_resolved: bool,
) -> EvidenceReport {
    let mut degraded = !graph_canonical || effect_degraded || !has_detected_envelopes;
    let mut surfaces = vec![EvidenceSurface {
        surface: SurfaceKind::Graph,
        state: state(graph_canonical),
        reason_refs: vec![super::diagnostic::EVIDENCE_UNKNOWN.to_owned()],
        confidence: confidence(graph_canonical),
    }];

    surfaces.push(EvidenceSurface {
        surface: SurfaceKind::Effects,
        state: if effect_degraded {
            SectionState::Stale
        } else {
            SectionState::Complete
        },
        reason_refs: vec![if effect_degraded {
            super::diagnostic::EFFECT_STALE.to_owned()
        } else {
            super::risk::reason::EFFECT_READ.to_owned()
        }],
        confidence: if effect_degraded {
            Confidence::Unknown
        } else {
            Confidence::Canonical
        },
    });

    surfaces.push(EvidenceSurface {
        surface: SurfaceKind::DetectedEvidence,
        state: state(has_detected_envelopes),
        reason_refs: vec![super::diagnostic::EVIDENCE_UNKNOWN.to_owned()],
        confidence: if has_detected_envelopes {
            Confidence::Verified
        } else {
            Confidence::Unknown
        },
    });

    if changed_mode {
        surfaces.push(EvidenceSurface {
            surface: SurfaceKind::ChangedInputs,
            state: if changed_resolved {
                SectionState::Complete
            } else {
                SectionState::Incomplete
            },
            reason_refs: vec![super::diagnostic::CHANGED_INPUT_INCOMPLETE.to_owned()],
            confidence: if changed_resolved {
                Confidence::Canonical
            } else {
                Confidence::Unknown
            },
        });
        degraded = degraded || !changed_resolved;
    }

    surfaces.push(EvidenceSurface {
        surface: SurfaceKind::Manifests,
        state: SectionState::Unknown,
        reason_refs: vec![super::diagnostic::EVIDENCE_UNKNOWN.to_owned()],
        confidence: Confidence::Unknown,
    });

    surfaces.push(EvidenceSurface {
        surface: SurfaceKind::Scenarios,
        state: SectionState::Complete,
        reason_refs: vec![super::risk::reason::SCENARIO_COVERS.to_owned()],
        confidence: Confidence::Canonical,
    });

    surfaces.push(EvidenceSurface {
        surface: SurfaceKind::Tests,
        state: SectionState::Unsupported,
        reason_refs: vec![super::diagnostic::EVIDENCE_UNKNOWN.to_owned()],
        confidence: Confidence::Unknown,
    });

    surfaces.sort_by(|left, right| left.surface.key().cmp(right.surface.key()));
    EvidenceReport { surfaces, degraded }
}

fn state(available: bool) -> SectionState {
    if available {
        SectionState::Complete
    } else {
        SectionState::Unknown
    }
}

fn confidence(complete: bool) -> Confidence {
    if complete {
        Confidence::Canonical
    } else {
        Confidence::Unknown
    }
}

/// The confidence meet over a section, derived from its items.
pub(crate) fn meet(confidences: impl Iterator<Item = Confidence>) -> Confidence {
    confidences.fold(Confidence::Canonical, |meet, confidence| {
        meet.meet(confidence)
    })
}
