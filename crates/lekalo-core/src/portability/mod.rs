//! Capability-based target portability over typed IR, exact profile
//! pins, and qualified evidence (issue #19).
//!
//! [`assess`] is pure: it receives the validated compilation, the
//! verified target pins, and recorded evidence, and performs no
//! filesystem, process, or network operation. The CLI owns classified
//! reads, duplicate-key-safe parsing, and rendering; the existing
//! profile, storage, and implementation projections are consumed as
//! inputs, never reimplemented.
//!
//! A capability declaration establishes a declared guarantee within its
//! exact scope; it never establishes that two native implementations
//! behave equivalently. Unsupported, untested, and unknown stay distinct
//! everywhere in the report.

pub mod assess;
pub mod canonical;
pub mod diagnostic;
pub mod planning;
pub mod request;
pub mod requirements;
pub mod version;
pub mod wire;

pub use assess::{assess, Assessment, TargetInput};
pub use canonical::{bytes_equal, canonical_bytes, report_digest};
pub use request::{embedded_recipe, parse_evidence, parse_targets, Request, Scope, SelectorError};
pub use requirements::{extract, Subjects};
use wire::PinState;
pub use wire::{
    Action, AdapterSnapshot, Binding, Classification, Destination, Dimension, DimensionSummary,
    EvidenceDocument, JoinState, ObligationRow, PlanningRow, Policy, ProfilePin, Ratio, Report,
    ScopeKind, SourceSelection, SubjectRow, Summary, TargetsDocument, Verification,
};

use crate::diagnostics::DiagnosticSet;
use crate::target_profile::resolution::ResolvedProfile;
use diagnostic as diagnostic_module;

/// Build the exact profile pin of one resolved profile document.
pub fn profile_pin(resolved: &ResolvedProfile) -> ProfilePin {
    ProfilePin {
        id: resolved.id.clone(),
        version: resolved.version.clone(),
        source_digest: resolved.source_digest.clone(),
        resolved_digest: resolved.digest.clone(),
        component_versions: resolved
            .components
            .iter()
            .map(|c| wire::ProfileComponent {
                axis: c.axis.as_str().to_owned(),
                component: c.id.clone(),
                definition_version: c.definition_version.to_owned(),
            })
            .collect(),
        capabilities: resolved
            .capabilities
            .iter()
            .map(|c| wire::ProfileCapability {
                id: c.id.clone(),
                support: c.support.as_str().to_owned(),
            })
            .collect(),
    }
}

/// Verify one configured pin against one resolved profile document:
/// identity, version, declaration digest, and effective digest must all
/// match exactly, or the configuration is invalid.
pub fn verify_profile_pin(
    binding: &wire::ProfileRef,
    resolved: &ResolvedProfile,
) -> Result<(), DiagnosticSet> {
    if binding.id != resolved.id
        || binding.version != resolved.version
        || binding.source_digest != resolved.source_digest
        || binding.resolved_digest != resolved.digest
    {
        return Err(diagnostic_module::invalid("profile-pin"));
    }
    Ok(())
}

/// The resolved target selection of one run: exact pins when the
/// configuration supplies them, derived declarations otherwise.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTargets {
    pub source: TargetInput,
    pub destinations: Vec<TargetInput>,
    pub document: Option<wire::TargetsDocument>,
}

/// Resolve the source/destination selection from the request, the
/// optional configuration, and the declared Model targets.
///
/// Source precedence: `--from`, then `defaultSource`, then the sole
/// declared Model target; multiple candidates refuse with an actionable
/// ambiguity diagnostic. Destination precedence: `--to`, then
/// `defaultDestinations`, then every other configured target sorted. An
/// explicitly selected unknown target id is invalid, and a resolved
/// empty destination set is invalid: a zero-destination assessment is
/// never a valid green report.
pub fn resolve_targets(
    request: &Request,
    document: Option<&wire::TargetsDocument>,
    subjects: &Subjects,
    pins: &std::collections::BTreeMap<String, Option<ProfilePin>>,
    adapters: &std::collections::BTreeMap<String, Option<wire::AdapterSnapshot>>,
    storage_refs: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<ResolvedTargets, DiagnosticSet> {
    let declared = &subjects.declared_targets;
    let configured: Vec<&str> = document
        .map(|d| d.targets.iter().map(|t| t.target_id.as_str()).collect())
        .unwrap_or_default();
    let configured: std::collections::BTreeSet<&str> = configured.into_iter().collect();
    let universe: Vec<&str> = if document.is_some() {
        configured.iter().copied().collect()
    } else {
        declared.iter().map(String::as_str).collect()
    };
    let source_id = match &request.source {
        Some(explicit) => {
            if !universe.contains(&explicit.as_str()) {
                return Err(diagnostic_module::invalid("target-unknown"));
            }
            explicit.clone()
        }
        None => {
            let from_config = document.and_then(|d| d.default_source.as_deref());
            if let Some(source) = from_config {
                source.to_owned()
            } else if declared.len() == 1 {
                declared[0].clone()
            } else if declared.is_empty() {
                return Err(diagnostic_module::invalid("source-missing"));
            } else {
                return Err(diagnostic_module::invalid("source-ambiguous"));
            }
        }
    };
    let mut destinations: Vec<String> = match request.destinations.is_empty() {
        false => {
            for destination in &request.destinations {
                if !universe.contains(&destination.as_str()) {
                    return Err(diagnostic_module::invalid("target-unknown"));
                }
            }
            request.destinations.clone()
        }
        true => match document.and_then(|d| d.default_destinations.as_deref()) {
            Some([first, rest @ ..]) => {
                let mut resolved = Vec::with_capacity(1 + rest.len());
                resolved.push((*first).to_owned());
                resolved.extend(rest.iter().map(|d| (*d).to_owned()));
                resolved
            }
            _ => universe
                .iter()
                .filter(|t| **t != source_id)
                .map(|t| (*t).to_owned())
                .collect(),
        },
    };
    destinations.sort();
    destinations.dedup();
    if destinations.is_empty() {
        // An assessment with zero destinations is the vacuous green
        // report; the selection refuses instead of emitting one.
        return Err(diagnostic_module::invalid("destination-empty"));
    }
    let pin_state = |target: &str| {
        let _ = target;
        if document.is_some() {
            PinState::Pinned
        } else {
            PinState::Derived
        }
    };
    let target_input = |target: &str| TargetInput {
        target_id: target.to_owned(),
        state: pin_state(target),
        profile: pins.get(target).cloned().flatten(),
        adapter: adapters.get(target).cloned().flatten(),
        storage_profile_refs: storage_refs.get(target).cloned().unwrap_or_default(),
    };
    Ok(ResolvedTargets {
        source: target_input(&source_id),
        destinations: destinations.iter().map(|t| target_input(t)).collect(),
        document: document.cloned(),
    })
}
