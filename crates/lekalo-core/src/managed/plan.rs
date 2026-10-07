//! Deterministic generation planning and stale-plan adjudication (issue #41).
//!
//! A plan is the complete reviewed intent: the pinned input closure, the
//! selected module/target scope, the coverage pin, every output with its
//! before/after digests, and the self digest over that payload. Same
//! inputs produce the same plan bytes; changed pins invalidate it.

use super::canonical::{render_inventory, Rendered};
use super::coverage::Admission;
use super::diagnostic;
use super::wire::{
    ArtifactRole, Coverage, Extension, GenerationDigest, GenerationPlan, InputPin, OutputAction,
    PlanAction, PlanOutput, State,
};
use super::{
    detail, pin, plan_digest, rel_path, DomainResult, GENERATED_DRIFT, INPUT_INVALID,
    OWNERSHIP_DENIED, PLAN_STALE,
};
use std::collections::BTreeMap;

/// The recorded before-state of one generated path (from the current
/// custody record, never guessed from disk membership).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    pub digest: String,
    pub role: ArtifactRole,
    pub owner_module: String,
    pub owner_target: String,
    pub object_ref: Option<String>,
}

/// The live before-state of the project: digests of every generated path
/// recorded in custody, and the actual on-disk digests where present.
pub struct BeforeState {
    /// Recorded ownership: path -> recorded output.
    pub recorded: BTreeMap<String, Recorded>,
    /// Actual file digest by path (present files only).
    pub actual: BTreeMap<String, String>,
}

fn invalid(reason: &str) -> DomainResult {
    diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// The generated-root coordinate map of one ownership document.
pub fn generated_roots(ownership: &crate::managed::wire::Ownership) -> BTreeMap<(String, String), String> {
    ownership
        .modules
        .iter()
        .flat_map(|module| {
            module.targets.iter().map(move |target| {
                (
                    (module.module.clone(), target.target.clone()),
                    target.generated_root.clone(),
                )
            })
        })
        .collect()
}

/// Build the deterministic plan for one admitted module/target.
///
/// `inputs` must already be the caller's exact closure; ordering and pin
/// shape are validated here. The before-state comes from recorded custody.
#[allow(clippy::too_many_arguments)]
pub fn build(
    action: PlanAction,
    admission: &Admission,
    coverage: &Coverage,
    inputs: Vec<InputPin>,
    before: &BeforeState,
    previous_generation: Option<String>,
    _lock_pin: Option<String>,
    coverage_digest: String,
) -> Result<GenerationPlan, DomainResult> {
    if !inputs
        .iter()
        .all(|pin| rel_path(&pin.path) && super::pin(&pin.digest))
    {
        return Err(invalid("input-closure"));
    }
    let mut closure = inputs;
    closure.sort_by(|left, right| left.path.cmp(&right.path));
    if super::has_duplicates(closure.iter().map(|p| &p.path)) {
        return Err(invalid("input-closure"));
    }
    let rendered = render_inventory(coverage, &admission.generated_root)?;
    let mut outputs = Vec::with_capacity(rendered.len());
    for artifact in &rendered {
        let after = super::digest(&artifact.bytes);
        let recorded = before.recorded.get(&artifact.path);
        let actual = before.actual.get(&artifact.path);
        let action = classify(artifact, recorded, actual)?;
        outputs.push(PlanOutput {
            path: artifact.path.clone(),
            action: action.0,
            role: artifact.role,
            owner_module: admission.module.clone(),
            owner_target: admission.target.clone(),
            before_digest: action.1.map(State::known).unwrap_or(State::Absent),
            after_digest: State::known(after),
        });
    }
    // Recorded generated paths that the new inventory no longer produces
    // are deleted only within the recorded ownership of this module.
    for (path, recorded) in &before.recorded {
        if recorded.owner_module != admission.module || recorded.owner_target != admission.target {
            continue;
        }
        if rendered.iter().any(|artifact| &artifact.path == path) {
            continue;
        }
        outputs.push(PlanOutput {
            path: path.clone(),
            action: OutputAction::Delete,
            role: recorded.role,
            owner_module: recorded.owner_module.clone(),
            owner_target: recorded.owner_target.clone(),
            before_digest: State::known(recorded.digest.clone()),
            after_digest: State::Absent,
        });
    }
    outputs.sort_by(|left, right| left.path.cmp(&right.path));
    if super::has_duplicates(outputs.iter().map(|o| &o.path)) {
        return Err(invalid("output-order"));
    }
    let plan = GenerationPlan {
        schema_version: super::header("managed-generation-plan").0,
        identity: super::header("managed-generation-plan").1,
        action,
        modules: vec![admission.module.clone()],
        targets: vec![admission.target.clone()],
        input_closure: closure,
        before_manifest: previous_generation
            .map(|digest| {
                State::known(GenerationDigest {
                    generation_digest: digest,
                })
            })
            .unwrap_or(State::Absent),
        coverage_digest,
        extensions: admission.extensions.clone(),
        outputs,
        plan_digest: String::new(),
    };
    let mut plan = plan;
    plan.plan_digest = plan_digest(&plan);
    Ok(plan)
}

fn classify(
    artifact: &Rendered,
    recorded: Option<&Recorded>,
    actual: Option<&String>,
) -> Result<(OutputAction, Option<String>), DomainResult> {
    let after = super::digest(&artifact.bytes);
    match (recorded, actual) {
        (None, None) => Ok((OutputAction::Create, None)),
        (None, Some(_)) => Err(diagnostic::failure(
            OWNERSHIP_DENIED,
            &[
                ("path", artifact.path.as_str()),
                ("reason", "create-conflict"),
            ],
        )),
        (Some(recorded), actual) => {
            if let Some(digest) = actual {
                if digest != &recorded.digest {
                    return Err(diagnostic::failure(
                        GENERATED_DRIFT,
                        &[("path", artifact.path.as_str()), ("reason", "before-state")],
                    ));
                }
            } else {
                return Err(diagnostic::failure(
                    GENERATED_DRIFT,
                    &[("path", artifact.path.as_str()), ("reason", "before-state")],
                ));
            }
            if recorded.digest == after {
                Ok((OutputAction::Unchanged, Some(recorded.digest.clone())))
            } else {
                Ok((OutputAction::Replace, Some(recorded.digest.clone())))
            }
        }
    }
}

/// Adjudicate a reviewed plan against the live state. Every refusal names
/// the exact pin that moved; the plan is never silently refreshed.
pub fn preflight(
    plan: &GenerationPlan,
    generated_roots: &BTreeMap<(String, String), String>,
    inputs_actual: &BTreeMap<String, String>,
    before: &BeforeState,
) -> Result<(), DomainResult> {
    for output in &plan.outputs {
        let root = generated_roots
            .get(&(output.owner_module.clone(), output.owner_target.clone()))
            .ok_or_else(|| {
                diagnostic::failure(
                    OWNERSHIP_DENIED,
                    &[("path", output.path.as_str()), ("reason", "unowned-module")],
                )
            })?;
        if !output.path.starts_with(&format!("{root}/")) {
            return Err(diagnostic::failure(
                OWNERSHIP_DENIED,
                &[("path", output.path.as_str()), ("reason", "outside-root")],
            ));
        }
    }
    for input in &plan.input_closure {
        match inputs_actual.get(&input.path) {
            Some(digest) if digest == &input.digest => {}
            _ => {
                return Err(diagnostic::failure(
                    PLAN_STALE,
                    &[("path", input.path.as_str()), ("reason", "input")],
                ))
            }
        }
    }
    for output in &plan.outputs {
        let actual = before.actual.get(&output.path);
        match output.action {
            OutputAction::Create => {
                if let Some(actual) = actual {
                    // Re-applying the identical generation is stale, not a
                    // collision; a different unclaimed file is denied.
                    if output.after_digest.value() == Some(actual) {
                        return Err(diagnostic::failure(
                            PLAN_STALE,
                            &[
                                ("path", output.path.as_str()),
                                ("reason", "already-applied"),
                            ],
                        ));
                    }
                    return Err(diagnostic::failure(
                        OWNERSHIP_DENIED,
                        &[
                            ("path", output.path.as_str()),
                            ("reason", "create-conflict"),
                        ],
                    ));
                }
            }
            OutputAction::Replace | OutputAction::Delete | OutputAction::Unchanged => {
                let before_digest = output
                    .before_digest
                    .value()
                    .ok_or_else(|| invalid("output-action"))?;
                match actual {
                    Some(digest) if digest == before_digest => {}
                    Some(_) => {
                        return Err(diagnostic::failure(
                            GENERATED_DRIFT,
                            &[("path", output.path.as_str()), ("reason", "edited")],
                        ))
                    }
                    None => {
                        return Err(diagnostic::failure(
                            GENERATED_DRIFT,
                            &[("path", output.path.as_str()), ("reason", "missing")],
                        ))
                    }
                }
                if output.action == OutputAction::Replace {
                    // Re-applying an identical generation is stale, not drift.
                    if let (Some(actual), Some(after)) = (actual, output.after_digest.value()) {
                        if actual == after && before_digest != after {
                            return Err(diagnostic::failure(
                                PLAN_STALE,
                                &[
                                    ("path", output.path.as_str()),
                                    ("reason", "already-applied"),
                                ],
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// The confirmation spelling of a plan digest.
pub fn confirmation(plan_digest: &str) -> String {
    format!("sha256:{plan_digest}")
}

/// Check the exact user confirmation against the plan identity.
pub fn require_confirmation(confirm: Option<&str>, expected: &str) -> Result<(), DomainResult> {
    match confirm {
        Some(confirm) if pin(confirm) && confirm == expected => Ok(()),
        Some(_) => Err(invalid("confirmation")),
        None => Err(invalid("confirmation-missing")),
    }
}

/// Sort helper for extension lists carried by plans and records.
pub fn sort_extensions(extensions: &mut [Extension]) {
    extensions.sort_by(|left, right| left.symbol.cmp(&right.symbol));
}
