//! Semantic admission: the requirement ledger and capability qualification
//! (issue #41).
//!
//! Only qualified full support discharges a generation obligation; the
//! absence of coverage is unknown rather than zero obligations. Extension
//! selections must be explicit, unique, and adapter-validated.

use super::diagnostic;
use super::wire::{
    Coverage, Extension, ExtensionCheck, Mode, ModuleAssignment, Requirement, RequirementKind,
    Sample, Scalar, State, Support, TargetAssignment,
};
use super::{
    detail, pin, DomainResult, CAPABILITY_GAP, EXTENSION_UNRESOLVED, INPUT_INVALID,
    VERSION_UNSUPPORTED,
};

/// The capability-qualified admission of one module/target assignment.
#[derive(Clone, Debug, PartialEq)]
pub struct Admission {
    pub module: String,
    pub target: String,
    pub recipe: super::wire::Reference,
    pub generated_root: String,
    pub rendered_requirements: usize,
    pub ledger_requirements: usize,
    pub extensions: Vec<Extension>,
}

fn capability_gap(requirement: &str, reason: &str) -> DomainResult {
    diagnostic::failure(
        CAPABILITY_GAP,
        &[("requirement", requirement), ("reason", reason)],
    )
}

fn extension_unresolved(symbol: &str, reason: &str) -> DomainResult {
    diagnostic::failure(
        EXTENSION_UNRESOLVED,
        &[("requirement", symbol), ("reason", reason)],
    )
}

fn invalid(reason: &str) -> DomainResult {
    diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// Sample/projection coherence: the sample must witness exactly the declared
/// projection (the rendered test invokes the handler with it), and the
/// sample's expected output must equal the projection applied to the
/// sample input (r6, devin F5 / codex F6): the rendered behavioral test
/// asserts exactly that expectation against the deterministic handler,
/// so a self-contradictory sample would publish an artifact whose own
/// generated test fails on arrival — it never admits, at planning time,
/// before any byte exists.
fn sample_matches(requirement: &Requirement) -> Result<(), DomainResult> {
    let projected_from: Vec<&String> = requirement.projection.iter().map(|p| &p.from).collect();
    let projected_to: Vec<&String> = requirement.projection.iter().map(|p| &p.to).collect();
    match requirement.sample.value() {
        Some(sample) => {
            let inputs: Vec<&String> = sample.input.keys().collect();
            let outputs: Vec<&String> = sample.output.keys().collect();
            if inputs != projected_from || outputs != projected_to {
                return Err(invalid("sample-projection"));
            }
            // Behavioral consistency (r6): observed = projection(input)
            // must equal the pinned expectation — the emitted test's own
            // assertion. A contradiction is a self-failing artifact.
            let observed: std::collections::BTreeMap<&String, &Scalar> = requirement
                .projection
                .iter()
                .filter_map(|projection| {
                    sample
                        .input
                        .get(&projection.from)
                        .map(|value| (&projection.to, value))
                })
                .collect();
            if observed.len() != requirement.projection.len()
                || sample
                    .output
                    .iter()
                    .any(|(field, value)| observed.get(field) != Some(&value))
            {
                return Err(capability_gap(&requirement.id, "scenario-assertion"));
            }
            Ok(())
        }
        None if requirement.projection.is_empty() => Ok(()),
        None => Err(invalid("sample-projection")),
    }
}

/// Admit one coverage document for one exact ownership assignment. Any
/// capability, extension, or protocol refusal happens here, before any
/// plan or apply exists.
pub fn admit(
    assignment: &ModuleAssignment,
    target: &TargetAssignment,
    coverage: &Coverage,
) -> Result<Admission, DomainResult> {
    if coverage.module != assignment.module || coverage.target != target.target {
        return Err(invalid("subject-mismatch"));
    }
    if assignment.mode != Mode::Managed {
        return Err(diagnostic::failure(
            INPUT_INVALID,
            &[("module", &assignment.module), ("reason", "not-managed")],
        ));
    }
    if coverage.adapter.protocol != super::PROTOCOL {
        return Err(diagnostic::failure(
            VERSION_UNSUPPORTED,
            &detail("protocol"),
        ));
    }
    if !coverage.complete {
        return Err(capability_gap("coverage", "incomplete"));
    }
    let mut rendered = 0usize;
    for requirement in &coverage.requirements {
        if requirement.support != Support::Full {
            return Err(capability_gap(&requirement.id, "support"));
        }
        if requirement
            .witness
            .value()
            .is_none_or(|witness| !pin(witness))
        {
            return Err(capability_gap(&requirement.id, "witness"));
        }
        if requirement.kind.renders() {
            let missing_projection = requirement.projection.is_empty();
            if missing_projection {
                return Err(capability_gap(&requirement.id, "projection"));
            }
            if requirement.sample.value().is_none() {
                return Err(capability_gap(&requirement.id, "sample"));
            }
            rendered += 1;
        } else if matches!(
            requirement.kind,
            RequirementKind::Expression | RequirementKind::Policy | RequirementKind::Transaction
        ) && requirement.projection.is_empty()
        {
            // Ledger-only kinds stay ledger-only; nothing is inferred.
        }
        sample_matches(requirement)?;
    }
    // Extension resolution: the explicit ownership selection and the
    // adapter-validated coverage must name the same set, once each.
    let selected: Vec<&Extension> = target.extensions.iter().collect();
    let validated: Vec<&ExtensionCheck> = coverage.extensions.iter().collect();
    for selection in &selected {
        let matches: Vec<&&ExtensionCheck> = validated
            .iter()
            .filter(|check| check.symbol == selection.symbol)
            .collect();
        if matches.len() != 1 {
            return Err(extension_unresolved(&selection.symbol, "ambiguous"));
        }
        let check: &ExtensionCheck = matches[0];
        if !check.validated {
            return Err(extension_unresolved(&selection.symbol, "unvalidated"));
        }
        if check.binding != selection.binding || check.contract_digest != selection.contract_digest
        {
            return Err(extension_unresolved(&selection.symbol, "stale"));
        }
    }
    for check in &validated {
        if !selected
            .iter()
            .any(|selection| selection.symbol == check.symbol)
        {
            return Err(extension_unresolved(&check.symbol, "unselected"));
        }
    }
    if !sorted_unique_requirements(coverage) {
        return Err(invalid("requirement-order"));
    }
    Ok(Admission {
        module: assignment.module.clone(),
        target: target.target.clone(),
        recipe: coverage.recipe.clone(),
        generated_root: target.generated_root.clone(),
        rendered_requirements: rendered,
        ledger_requirements: coverage.requirements.len(),
        extensions: target.extensions.clone(),
    })
}

fn sorted_unique_requirements(coverage: &Coverage) -> bool {
    coverage
        .requirements
        .windows(2)
        .all(|window| window[0].id < window[1].id)
}

/// The exact extension list a plan carries: the explicit ownership
/// selections, in the fixed symbol byte order.
pub fn plan_extensions(assignment: &ModuleAssignment, target: &TargetAssignment) -> Vec<Extension> {
    let _ = assignment;
    target.extensions.clone()
}

/// Sample accessor used by the equivalence fixture checks.
pub fn sample_of(requirement: &Requirement) -> Option<&Sample> {
    match &requirement.sample {
        State::Known { value } => Some(value),
        State::Absent => None,
    }
}
