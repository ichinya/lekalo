//! The Scenario Given/When/Then executor of the reference evaluator
//! (issue #107).
//!
//! One evaluation runs the whole scenario in memory: `given` steps
//! materialize typed entity rows, actors, clocks, and ID sources;
//! each `when` invoke runs as one staged all-or-nothing transaction
//! over the in-memory state (preconditions, ordered assignments,
//! post-write invariant enforcement, rollback on every failure
//! class); `then` assertions observe step outputs, the effect log,
//! and the final state. Every step outcome is closed typed data tied
//! to semantic identity, and semantics outside the declared reference
//! subset are explicit `unsupported` outcomes with fixed reason
//! tokens — never guesses.
//!
//! The executor performs no filesystem, network, process, wall-clock,
//! or randomness access of any kind; the only clock is the scenario's
//! declared clock (or the documented epoch fallback) and the only ID
//! source is the deterministic SHA-256 derivation.

use std::collections::BTreeMap;

use super::diagnostic;
use super::semantics::{self, Context};
use super::state::{Row, Store};
use super::trace::{
    AssertionRecord, EffectKind, EffectRecord, ErrorToken, GivenRecord, Outcome, ReferenceTrace,
    RowSnapshot, Status, Verdict, WhenRecord,
};
use super::version;
use crate::diagnostics::DiagnosticSet;
use crate::error_contract::ErrorRegistry;
use crate::invariant_transition::invariant::InvariantKind;
use crate::invariant_transition::transition::AssignmentValue;
use crate::invariant_transition::InvariantTransitionAttachment;
use crate::ir::{CompiledProject, Definition, EffectOperation, TypeRef};
use crate::scenario::action::InvokeAction;
use crate::scenario::assertion::{
    Assertion, EffectScope, EntityExpectation, FieldExpectation, MatchKind, OccurrenceCount,
};
use crate::scenario::precondition::{FieldPredicate, Precondition};
use crate::scenario::reference::{Ref, RefKind, RefTarget};
use crate::scenario::step::{ValueOrRef, WhenStep};
use crate::scenario::value::TypedValue;
use crate::scenario::ScenarioIr;

/// One bound evaluation: the pinned IR, the pinned
/// invariant-transition attachment, and the pinned #62 error
/// registry, indexed for deterministic lookup.
pub struct ReferenceEvaluation<'a> {
    project: &'a CompiledProject,
    attachment: &'a InvariantTransitionAttachment,
    registry: &'a ErrorRegistry,
    definitions: BTreeMap<String, &'a Definition>,
}

impl<'a> ReferenceEvaluation<'a> {
    /// Bind one evaluation to its exact pinned inputs. Pure: nothing
    /// is read, written, or executed.
    pub fn new(
        project: &'a CompiledProject,
        attachment: &'a InvariantTransitionAttachment,
        registry: &'a ErrorRegistry,
    ) -> Self {
        let definitions = project
            .definitions
            .iter()
            .map(|definition| (definition.id().as_str().to_owned(), definition))
            .collect();
        Self {
            project,
            attachment,
            registry,
            definitions,
        }
    }

    /// The pinned #62 error registry.
    pub const fn registry(&self) -> &ErrorRegistry {
        self.registry
    }

    /// Execute one validated scenario in memory and produce the
    /// deterministic reference trace. The only failures are the
    /// construction bounds (rows, effects, trace bytes); every
    /// behavioral outcome is typed trace data.
    pub fn execute(&self, scenario: &ScenarioIr) -> Result<ReferenceTrace, DiagnosticSet> {
        if scenario.given().len() + scenario.when().len() > version::MAX_STEP_RECORDS
            || scenario.then().len() > version::MAX_ASSERTIONS
        {
            return Err(diagnostic::limit_set("step-records"));
        }
        let scenario_digest = scenario_digest_of(scenario)?;
        let refusal = self.refusal_of(scenario);
        if let Some(reason) = refusal {
            return Ok(self.refused_trace(scenario, reason, scenario_digest));
        }
        let mut run = Run {
            evaluation: self,
            scenario,
            store: Store::default(),
            clocks: BTreeMap::new(),
            id_counters: BTreeMap::new(),
            idempotent: BTreeMap::new(),
            inputs: Vec::new(),
            outputs: BTreeMap::new(),
            given_values: BTreeMap::new(),
            given: Vec::new(),
            when: Vec::new(),
            effects: Vec::new(),
            derived: Vec::new(),
            last_transition: None,
            last_space: None,
            last_effects: Vec::new(),
        };
        run.run_given()?;
        run.run_when()?;
        let assertions = run.run_then();
        let state = run.snapshots();
        let state_digest = super::canonical::digest_of(&state_bytes(&state));
        let effect_digest = super::canonical::digest_of(&effects_bytes(&run.effects));
        let status = overall_status(&run.given, &run.when, &assertions);
        let trace = ReferenceTrace {
            scenario_id: scenario.scenario_id().as_str().to_owned(),
            scenario_version: scenario.scenario_version().as_str().to_owned(),
            scenario_digest,
            model_version: scenario.model_ref().version_text().to_owned(),
            ir_digest: scenario.ir_ref().digest.as_str().to_owned(),
            attachment_revision: self.attachment.attachment_revision().as_str().to_owned(),
            refusal: None,
            status,
            given: run.given,
            when: run.when,
            effects: run.effects,
            state,
            assertions,
            state_digest,
            effect_digest,
        };
        // Prove the trace fits the payload bound before handing it
        // out; `canonical_bytes` re-checks on every export.
        super::canonical::trace_bytes(&trace)?;
        Ok(trace)
    }

    /// The refusal token when the pinned inputs disagree, in the
    /// fixed check order.
    fn refusal_of(&self, scenario: &ScenarioIr) -> Option<&'static str> {
        let canonical = self.project.to_canonical_json();
        let actual = super::canonical::digest_of(&canonical);
        if actual != scenario.ir_ref().digest.as_str() {
            return Some("ir-pin-mismatch");
        }
        if scenario.ir_ref().digest.as_str() != self.attachment.ir_digest().as_str() {
            return Some("attachment-ir-mismatch");
        }
        if scenario.model_ref().version_text() != self.attachment.model_ref().version().as_str() {
            return Some("model-pin-mismatch");
        }
        None
    }

    /// The trace of a refused evaluation: the pins disagree, so no
    /// step may run and nothing is guessed.
    fn refused_trace(
        &self,
        scenario: &ScenarioIr,
        reason: &'static str,
        scenario_digest: String,
    ) -> ReferenceTrace {
        ReferenceTrace {
            scenario_id: scenario.scenario_id().as_str().to_owned(),
            scenario_version: scenario.scenario_version().as_str().to_owned(),
            scenario_digest,
            model_version: scenario.model_ref().version_text().to_owned(),
            ir_digest: scenario.ir_ref().digest.as_str().to_owned(),
            attachment_revision: self.attachment.attachment_revision().as_str().to_owned(),
            refusal: Some(reason),
            status: Status::Unsupported,
            given: Vec::new(),
            when: Vec::new(),
            effects: Vec::new(),
            state: Vec::new(),
            assertions: Vec::new(),
            state_digest: super::canonical::digest_of("[]"),
            effect_digest: super::canonical::digest_of("[]"),
        }
    }
}

/// The digest of the exact canonical scenario payload.
fn scenario_digest_of(scenario: &ScenarioIr) -> Result<String, DiagnosticSet> {
    let bytes = scenario.canonical_bytes()?;
    Ok(super::canonical::digest_of(&bytes))
}

/// The step identifier one scenario-local reference targets, when it
/// targets a step.
fn ref_step_text(reference: &Ref) -> Option<&str> {
    match &reference.id {
        RefTarget::Step(step) => Some(step.as_str()),
        _ => None,
    }
}

/// One running evaluation.
struct Run<'a> {
    evaluation: &'a ReferenceEvaluation<'a>,
    scenario: &'a ScenarioIr,
    store: Store,
    clocks: BTreeMap<String, String>,
    id_counters: BTreeMap<String, u64>,
    idempotent: BTreeMap<String, usize>,
    inputs: Vec<BTreeMap<String, TypedValue>>,
    outputs: BTreeMap<String, TypedValue>,
    given_values: BTreeMap<String, TypedValue>,
    given: Vec<GivenRecord>,
    when: Vec<WhenRecord>,
    effects: Vec<EffectRecord>,
    derived: Vec<TypedValue>,
    last_transition: Option<String>,
    last_space: Option<String>,
    last_effects: Vec<usize>,
}

impl Run<'_> {
    /// Establish every `given` step in scenario order.
    fn run_given(&mut self) -> Result<(), DiagnosticSet> {
        let scenario = self.scenario;
        let steps: Vec<&crate::scenario::step::GivenStep> = scenario.given().iter().collect();
        for step in steps {
            let record = self.establish(step.step_id.as_str(), &step.precondition)?;
            self.given.push(record);
        }
        Ok(())
    }

    /// Establish one precondition.
    fn establish(
        &mut self,
        step_id: &str,
        precondition: &Precondition,
    ) -> Result<GivenRecord, DiagnosticSet> {
        match precondition {
            Precondition::State {
                entity,
                selector,
                fields,
            } => {
                let entity_id = entity.as_str();
                let Some(entity_definition) = self.entity_fields(entity_id) else {
                    return Ok(GivenRecord {
                        step_id: step_id.to_owned(),
                        kind: "state",
                        status: Err("entity-unknown"),
                        entity: None,
                        row: None,
                    });
                };
                let identity: Vec<String> = entity_definition
                    .identity
                    .iter()
                    .map(|field| field.as_str().to_owned())
                    .collect();
                let mut row: Row = BTreeMap::new();
                for term in selector {
                    if let Ok(value) = self.resolve_leaf(&term.equals) {
                        row.insert(term.field.as_str().to_owned(), value);
                    }
                }
                for (field, leaf) in fields {
                    if let Ok(value) = self.resolve_leaf(leaf) {
                        row.insert(field.as_str().to_owned(), value);
                    }
                }
                let Some(key) = Store::row_key(&identity, &row) else {
                    return Ok(GivenRecord {
                        step_id: step_id.to_owned(),
                        kind: "state",
                        status: Err("identity-incomplete"),
                        entity: Some(entity_id.to_owned()),
                        row: None,
                    });
                };
                if self.store.row_count() >= version::MAX_ROWS {
                    return Err(diagnostic::limit_set("rows"));
                }
                self.store.merge_row(entity_id, key.clone(), row.clone());
                let value = row_object(&row);
                self.given_values.insert(step_id.to_owned(), value);
                Ok(GivenRecord {
                    step_id: step_id.to_owned(),
                    kind: "state",
                    status: Ok(()),
                    entity: Some(entity_id.to_owned()),
                    row: Some(key),
                })
            }
            Precondition::Fixture { .. } => Ok(GivenRecord {
                step_id: step_id.to_owned(),
                kind: "fixture",
                status: Err("fixture"),
                entity: None,
                row: None,
            }),
            Precondition::Actor { actor, .. } => {
                self.given_values.insert(
                    step_id.to_owned(),
                    TypedValue::String(actor.as_str().to_owned()),
                );
                Ok(GivenRecord {
                    step_id: step_id.to_owned(),
                    kind: "actor",
                    status: Ok(()),
                    entity: None,
                    row: None,
                })
            }
            Precondition::Clock { at } => {
                self.clocks.insert(step_id.to_owned(), at.clone());
                self.given_values
                    .insert(step_id.to_owned(), TypedValue::Datetime(at.clone()));
                Ok(GivenRecord {
                    step_id: step_id.to_owned(),
                    kind: "clock",
                    status: Ok(()),
                    entity: None,
                    row: None,
                })
            }
            Precondition::IdSource { .. } => Ok(GivenRecord {
                step_id: step_id.to_owned(),
                kind: "id-source",
                status: Ok(()),
                entity: None,
                row: None,
            }),
        }
    }

    /// The entity definition of one IR entity id.
    fn entity_fields(&self, entity: &str) -> Option<&crate::ir::EntityDef> {
        match self.evaluation.definitions.get(entity) {
            Some(Definition::Entity(definition)) => Some(definition),
            _ => None,
        }
    }

    /// Resolve one leaf fully, with the fixed unsupported token on
    /// failure.
    fn resolve_leaf(&mut self, leaf: &ValueOrRef) -> Result<TypedValue, &'static str> {
        match leaf {
            ValueOrRef::Value(value) => Ok(value.clone()),
            ValueOrRef::Reference(reference) => {
                let base = match reference.kind {
                    RefKind::StepOutput => self
                        .outputs
                        .get(ref_step_text(reference).ok_or("step-ref-shape")?)
                        .cloned()
                        .ok_or("step-output-unavailable")?,
                    RefKind::GivenValue => self
                        .given_values
                        .get(ref_step_text(reference).ok_or("step-ref-shape")?)
                        .cloned()
                        .ok_or("given-value-unavailable")?,
                    RefKind::Clock => {
                        let at = self
                            .clocks
                            .get(ref_step_text(reference).ok_or("step-ref-shape")?)
                            .ok_or("clock-unresolved")?;
                        TypedValue::Datetime(at.clone())
                    }
                    RefKind::IdSource => {
                        let step = ref_step_text(reference).ok_or("step-ref-shape")?;
                        let (seed, algorithm) = self.id_source_of(step)?;
                        let index = {
                            let counter = self.id_counters.entry(step.to_owned()).or_insert(0);
                            *counter += 1;
                            *counter
                        };
                        let derived = semantics::derive_id(&seed, algorithm, index);
                        self.derived.push(derived.clone());
                        derived
                    }
                    RefKind::Actor => match &reference.id {
                        RefTarget::Namespaced(actor) => {
                            TypedValue::String(actor.as_str().to_owned())
                        }
                        _ => return Err("actor-ref-unavailable"),
                    },
                    RefKind::Fixture => return Err("fixture"),
                    _ => return Err("semantic-ref-value"),
                };
                if let Some(path) = &reference.path {
                    let walked = semantics::member_path(&base, path).ok_or("member-path")?;
                    Ok(walked.clone())
                } else {
                    Ok(base)
                }
            }
        }
    }

    /// The seed and algorithm of one established ID source step.
    fn id_source_of(
        &self,
        step: &str,
    ) -> Result<(String, crate::scenario::precondition::IdAlgorithm), &'static str> {
        for given in self.scenario.given() {
            if given.step_id.as_str() == step {
                if let Precondition::IdSource { seed, algorithm } = &given.precondition {
                    return Ok((seed.clone(), *algorithm));
                }
            }
        }
        Err("id-source-unavailable")
    }

    /// The evaluation clock of one action: the explicit clock
    /// reference, else the first declared clock, else the documented
    /// epoch fallback.
    fn clock_of(&self, action: &InvokeAction) -> Result<String, &'static str> {
        if let Some(reference) = &action.clock {
            let step = ref_step_text(reference).ok_or("step-ref-shape")?;
            return self.clocks.get(step).cloned().ok_or("clock-unresolved");
        }
        Ok(self
            .clocks
            .values()
            .next()
            .cloned()
            .unwrap_or_else(|| version::EPOCH_CLOCK.to_owned()))
    }

    /// Execute every `when` step in scenario order.
    fn run_when(&mut self) -> Result<(), DiagnosticSet> {
        let scenario = self.scenario;
        let steps: Vec<&WhenStep> = scenario.when().iter().collect();
        for step in steps {
            let record = self.invoke(step)?;
            self.when.push(record);
        }
        Ok(())
    }

    /// Execute one invoke step (or its recorded replay).
    fn invoke(&mut self, step: &WhenStep) -> Result<WhenRecord, DiagnosticSet> {
        self.last_transition = None;
        self.last_space = None;
        self.last_effects = Vec::new();
        let action = &step.action;
        let operation = action.operation.as_str().to_owned();
        let mut input: BTreeMap<String, TypedValue> = BTreeMap::new();
        for (field, leaf) in &action.input {
            match self.resolve_leaf(leaf) {
                Ok(value) => {
                    input.insert(field.as_str().to_owned(), value);
                }
                Err(reason) => return Ok(self.unsupported_record(step, &operation, reason)),
            }
        }
        let clock = match self.clock_of(action) {
            Ok(clock) => clock,
            Err(reason) => return Ok(self.unsupported_record(step, &operation, reason)),
        };
        let idempotency_key = match &action.idempotency_key {
            Some(leaf) => match self.resolve_leaf(leaf) {
                Ok(value) => Some(value),
                Err(reason) => return Ok(self.unsupported_record(step, &operation, reason)),
            },
            None => None,
        };
        let key_text = idempotency_key
            .as_ref()
            .map(|value| format!("{operation}\u{0}{}", super::canonical::typed_value(value)));

        // Durable-key and explicit replays return the recorded result
        // and never duplicate effects.
        if let Some(key_text) = &key_text {
            if let Some(&prior_index) = self.idempotent.get(key_text) {
                let prior = &self.when[prior_index];
                if step.replay.is_some()
                    && (prior.operation != operation || self.inputs[prior_index] != input)
                {
                    return Ok(self.unsupported_record(step, &operation, "replay-mismatch"));
                }
                let replayed = WhenRecord {
                    step_id: step.step_id.as_str().to_owned(),
                    operation,
                    outcome: prior.outcome.clone(),
                    transition: None,
                    state_space: None,
                    clock: prior.clock.clone(),
                    replay_of: Some(prior.step_id.clone()),
                    effects: Vec::new(),
                    derived_ids: Vec::new(),
                };
                self.inputs.push(input);
                if let Outcome::Ok { output } = &replayed.outcome {
                    self.outputs
                        .insert(replayed.step_id.clone(), output.clone());
                }
                return Ok(replayed);
            }
        }
        // An explicit replay without any recorded key is unknown.
        if step.replay.is_some() {
            return Ok(self.unsupported_record(step, &operation, "replay-key-missing"));
        }

        let outcome = match self.evaluation.definitions.get(operation.as_str()) {
            Some(Definition::Command(_)) => self.execute_command(step, &operation, &input, &clock),
            Some(Definition::Query(_)) => self.execute_query(&operation, &input),
            Some(_) => Ok(Outcome::Unsupported {
                reason: "operation-not-invokable",
            }),
            None => Ok(Outcome::Unsupported {
                reason: "operation-unknown",
            }),
        }?;
        let record = WhenRecord {
            step_id: step.step_id.as_str().to_owned(),
            operation: operation.clone(),
            outcome: outcome.clone(),
            transition: self.last_transition.clone(),
            state_space: self.last_space.clone(),
            clock,
            replay_of: None,
            effects: self.last_effects.clone(),
            derived_ids: std::mem::take(&mut self.derived),
        };
        if let Some(key_text) = key_text {
            self.idempotent.insert(key_text, self.when.len());
        }
        self.inputs.push(input);
        if let Outcome::Ok { output } = &outcome {
            self.outputs.insert(record.step_id.clone(), output.clone());
        }
        Ok(record)
    }

    /// One assembled when record for unsupported early exits.
    fn unsupported_record(
        &mut self,
        step: &WhenStep,
        operation: &str,
        reason: &'static str,
    ) -> WhenRecord {
        WhenRecord {
            step_id: step.step_id.as_str().to_owned(),
            operation: operation.to_owned(),
            outcome: Outcome::Unsupported { reason },
            transition: None,
            state_space: None,
            clock: String::new(),
            replay_of: None,
            effects: Vec::new(),
            derived_ids: std::mem::take(&mut self.derived),
        }
    }

    /// Execute one command through its pinned transition.
    fn execute_command(
        &mut self,
        step: &WhenStep,
        operation: &str,
        input: &BTreeMap<String, TypedValue>,
        clock: &str,
    ) -> Result<Outcome, DiagnosticSet> {
        let action = &step.action;
        let transitions: Vec<_> = self
            .evaluation
            .attachment
            .transitions()
            .iter()
            .filter(|transition| transition.command().as_str() == operation)
            .collect();
        let transition = match transitions.as_slice() {
            [] => {
                return Ok(Outcome::Unsupported {
                    reason: "no-transition",
                })
            }
            [one] => one,
            _ => {
                return Ok(Outcome::Unsupported {
                    reason: "multiple-transitions",
                })
            }
        };
        // A declared authorization policy on an actor-carrying invoke
        // cannot be decided by the reference: policy execution is
        // owned by the authorization contract, so the command is
        // explicitly not executed rather than guessed as allowed.
        if action.actor.is_some() && transition.policy_ref().is_some() {
            return Ok(Outcome::Unsupported {
                reason: "authorization",
            });
        }
        let space = self
            .evaluation
            .attachment
            .state_spaces()
            .iter()
            .find(|space| space.state_space_id() == transition.state_space_id());
        let Some(space) = space else {
            return Ok(Outcome::Unsupported {
                reason: "state-space-unknown",
            });
        };
        let entity_id = space.entity().as_str().to_owned();
        let Some(entity) = self.entity_fields(&entity_id) else {
            return Ok(Outcome::Unsupported {
                reason: "entity-unknown",
            });
        };
        let identity: Vec<String> = entity
            .identity
            .iter()
            .map(|field| field.as_str().to_owned())
            .collect();
        let Some(key) = Store::row_key(&identity, input) else {
            return Ok(Outcome::Unsupported {
                reason: "identity-incomplete",
            });
        };
        let declared: Vec<String> = transition
            .error_refs()
            .iter()
            .map(|error| error.as_str().to_owned())
            .collect();
        let Some(prior) = self.store.row(&entity_id, &key).cloned() else {
            return Ok(Outcome::Error {
                token: ErrorToken::RowNotFound,
                declared,
                violations: Vec::new(),
            });
        };
        let context = Context {
            input,
            row: Some(&prior),
            entity: &entity_id,
            clock,
        };
        for precondition in transition.preconditions() {
            match semantics::evaluate(precondition, &context) {
                Ok(true) => {}
                Ok(false) => {
                    return Ok(Outcome::Error {
                        token: ErrorToken::PreconditionFailed,
                        declared,
                        violations: Vec::new(),
                    });
                }
                Err(reason) => {
                    return Ok(Outcome::Unsupported {
                        reason: reason.token(),
                    });
                }
            }
        }
        // Stage the assignments over the prior row.
        let mut staged = prior.clone();
        let mut written: Vec<String> = Vec::new();
        for assignment in transition.assignments() {
            let value = match assignment.value() {
                AssignmentValue::Literal(node) => match semantics::literal(node) {
                    Ok(value) => value,
                    Err(reason) => {
                        return Ok(Outcome::Unsupported {
                            reason: reason.token(),
                        })
                    }
                },
                AssignmentValue::Input { field } => match input.get(field.as_str()) {
                    Some(value) => value.clone(),
                    None => {
                        return Ok(Outcome::Unsupported {
                            reason: "input-unset",
                        })
                    }
                },
                AssignmentValue::Prior { field } => match prior.get(field.as_str()) {
                    Some(value) => value.clone(),
                    None => {
                        return Ok(Outcome::Unsupported {
                            reason: "field-unset",
                        })
                    }
                },
                AssignmentValue::Now => TypedValue::Datetime(clock.to_owned()),
                AssignmentValue::Expression { .. } => {
                    return Ok(Outcome::Unsupported {
                        reason: "expression-ref",
                    })
                }
            };
            let field = assignment.field().as_str().to_owned();
            staged.insert(field.clone(), value);
            if !written.contains(&field) {
                written.push(field);
            }
        }
        // Enforce the invariants of the state space over the staged
        // store; any violation rolls the whole transaction back.
        let mut staged_store = self.store.clone();
        staged_store.put_row(&entity_id, key.clone(), staged);
        match self.enforce_invariants(
            &staged_store,
            space.state_space_id().as_str(),
            &entity_id,
            clock,
        ) {
            Err(reason) => Ok(Outcome::Unsupported {
                reason: reason.token(),
            }),
            Ok(violations) if !violations.is_empty() => {
                let mut declared_errors = declared;
                for invariant in self.evaluation.attachment.invariants() {
                    if violations
                        .iter()
                        .any(|violation| violation == invariant.invariant_id().as_str())
                    {
                        if let Some(conflict) = invariant.conflict_error_ref() {
                            let text = conflict.as_str().to_owned();
                            if !declared_errors.contains(&text) {
                                declared_errors.push(text);
                            }
                        }
                    }
                }
                declared_errors.sort();
                declared_errors.dedup();
                Ok(Outcome::Error {
                    token: ErrorToken::InvariantViolated,
                    declared: declared_errors,
                    violations,
                })
            }
            Ok(_) => {
                self.store = staged_store;
                let index = self.push_effect(
                    step.step_id.as_str(),
                    EffectRecord {
                        index: 0,
                        step: String::new(),
                        kind: EffectKind::EntityWrite,
                        entity: Some(entity_id.clone()),
                        operation: Some(effect_operation_of(EffectOperation::Update)),
                        row: Some(key),
                        fields: {
                            let mut sorted = written;
                            sorted.sort();
                            sorted
                        },
                        target: None,
                    },
                )?;
                self.last_effects.push(index);
                // Record the declared event intents of the command's
                // effects, in declaration order.
                if let Some(Definition::Command(command)) =
                    self.evaluation.definitions.get(operation)
                {
                    for effect_id in &command.effects {
                        if let Some(Definition::Effect(effect)) =
                            self.evaluation.definitions.get(effect_id.as_str())
                        {
                            for emitted in &effect.emits {
                                let index = self.push_effect(
                                    step.step_id.as_str(),
                                    EffectRecord {
                                        index: 0,
                                        step: String::new(),
                                        kind: EffectKind::EventIntent,
                                        entity: None,
                                        operation: None,
                                        row: None,
                                        fields: Vec::new(),
                                        target: Some(emitted.as_str().to_owned()),
                                    },
                                )?;
                                self.last_effects.push(index);
                            }
                        }
                    }
                }
                self.last_transition = Some(transition.transition_id().as_str().to_owned());
                self.last_space = Some(space.state_space_id().as_str().to_owned());
                Ok(Outcome::Ok {
                    output: TypedValue::Null,
                })
            }
        }
    }

    /// Execute one query with the closed reference read semantics:
    /// exact-match filter over the single read entity, identity-key
    /// order, full-row projection.
    fn execute_query(
        &self,
        operation: &str,
        input: &BTreeMap<String, TypedValue>,
    ) -> Result<Outcome, DiagnosticSet> {
        let Definition::Query(query) = &self.evaluation.definitions[operation] else {
            return Ok(Outcome::Unsupported {
                reason: "operation-not-invokable",
            });
        };
        if query.reads.len() != 1 {
            return Ok(Outcome::Unsupported {
                reason: "reads-shape",
            });
        }
        let entity_id = query.reads[0].as_str();
        let Some(entity) = self.entity_fields(entity_id) else {
            return Ok(Outcome::Unsupported {
                reason: "entity-unknown",
            });
        };
        let field_names: Vec<&str> = entity.fields.iter().map(|f| f.name.as_str()).collect();
        for field in input.keys() {
            if !field_names.contains(&field.as_str()) {
                return Ok(Outcome::Unsupported {
                    reason: "query-input-unmapped",
                });
            }
        }
        let matched: Vec<(&String, &Row)> = self
            .store
            .rows_of(entity_id)
            .into_iter()
            .filter(|(_, row)| {
                input
                    .iter()
                    .all(|(field, value)| row.get(field) == Some(value))
            })
            .collect();
        match &query.returns {
            Some(TypeRef::List(inner)) => match &**inner {
                TypeRef::Ref(leaf) if leaf.as_str() == entity_id => {
                    let items: Vec<TypedValue> =
                        matched.iter().map(|(_, row)| row_object(row)).collect();
                    Ok(Outcome::Ok {
                        output: TypedValue::List(items),
                    })
                }
                _ => Ok(Outcome::Unsupported {
                    reason: "returns-shape",
                }),
            },
            Some(TypeRef::Ref(leaf)) if leaf.as_str() == entity_id => match matched.len() {
                0 => Ok(Outcome::Error {
                    token: ErrorToken::QueryEmpty,
                    declared: Vec::new(),
                    violations: Vec::new(),
                }),
                1 => Ok(Outcome::Ok {
                    output: row_object(matched[0].1),
                }),
                _ => Ok(Outcome::Error {
                    token: ErrorToken::QueryAmbiguous,
                    declared: Vec::new(),
                    violations: Vec::new(),
                }),
            },
            _ => Ok(Outcome::Unsupported {
                reason: "returns-shape",
            }),
        }
    }

    /// Enforce every invariant of one state space over the staged
    /// store. Ok(violations) is the byte-sorted set of violated
    /// invariant ids (empty when every invariant holds); Err is the
    /// fixed reason one invariant cannot be evaluated.
    fn enforce_invariants(
        &self,
        store: &Store,
        space_id: &str,
        entity_id: &str,
        clock: &str,
    ) -> Result<Vec<String>, semantics::Reason> {
        let empty_input = BTreeMap::new();
        let mut violations: Vec<String> = Vec::new();
        let rows: Vec<(String, Row)> = store
            .rows_of(entity_id)
            .into_iter()
            .map(|(key, row)| (key.clone(), row.clone()))
            .collect();
        for invariant in self.evaluation.attachment.invariants() {
            if invariant.state_space_id().as_str() != space_id {
                continue;
            }
            match invariant.kind {
                InvariantKind::FieldValue | InvariantKind::CrossField | InvariantKind::Temporal => {
                    if let Some(predicate) = invariant.predicate() {
                        // Row-level predicates bind to the staged row
                        // of the transition's entity.
                        for (_, row) in &rows {
                            let context = Context {
                                input: &empty_input,
                                row: Some(row),
                                entity: entity_id,
                                clock,
                            };
                            if !semantics::evaluate(predicate, &context)? {
                                violations.push(invariant.invariant_id().as_str().to_owned());
                                break;
                            }
                        }
                    }
                }
                InvariantKind::ConditionalRequirement => {
                    if let Some(predicate) = invariant.predicate() {
                        for (_, row) in &rows {
                            let context = Context {
                                input: &empty_input,
                                row: Some(row),
                                entity: entity_id,
                                clock,
                            };
                            if semantics::evaluate(predicate, &context)? {
                                let satisfied = invariant.required_fields().iter().all(|field| {
                                    !matches!(
                                        row.get(field.as_str()),
                                        None | Some(TypedValue::Null)
                                    )
                                });
                                if !satisfied {
                                    violations.push(invariant.invariant_id().as_str().to_owned());
                                }
                            }
                        }
                    }
                }
                InvariantKind::OneActive => {
                    if !invariant.max_active() {
                        return Err(semantics::Reason::MaxActiveMissing);
                    }
                    let Some(partition) = invariant.partition() else {
                        return Err(semantics::Reason::IncompatibleKind);
                    };
                    if let Some(owner) = partition.entity() {
                        if owner.as_str() != entity_id {
                            return Err(semantics::Reason::IncompatibleKind);
                        }
                    }
                    let field = partition.field().as_str();
                    let mut groups: BTreeMap<String, usize> = BTreeMap::new();
                    for (_, row) in &rows {
                        let partition_value =
                            row.get(field).ok_or(semantics::Reason::FieldUnset)?;
                        let key = super::canonical::typed_value(partition_value);
                        let holds = match invariant.predicate() {
                            Some(predicate) => {
                                let context = Context {
                                    input: &empty_input,
                                    row: Some(row),
                                    entity: entity_id,
                                    clock,
                                };
                                semantics::evaluate(predicate, &context)?
                            }
                            None => true,
                        };
                        if holds {
                            *groups.entry(key).or_insert(0) += 1;
                        }
                    }
                    if groups.values().any(|count| *count > 1) {
                        violations.push(invariant.invariant_id().as_str().to_owned());
                    }
                }
                InvariantKind::Uniqueness => {
                    let fields: Vec<&str> = invariant
                        .fields()
                        .iter()
                        .map(|field| field.field().as_str())
                        .collect();
                    let mut tuples: Vec<String> = Vec::new();
                    for (_, row) in &rows {
                        let mut pair = Vec::with_capacity(fields.len());
                        for field in &fields {
                            let value = row.get(*field).ok_or(semantics::Reason::FieldUnset)?;
                            pair.push(super::canonical::typed_value(value));
                        }
                        tuples.push(format!("[{}]", pair.join(",")));
                    }
                    tuples.sort();
                    let before = tuples.len();
                    tuples.dedup();
                    if tuples.len() != before {
                        violations.push(invariant.invariant_id().as_str().to_owned());
                    }
                }
                InvariantKind::Cardinality => {
                    if invariant.fields().len() != 1 {
                        return Err(semantics::Reason::IncompatibleKind);
                    }
                    let field = invariant.fields()[0].field().as_str().to_owned();
                    for (_, row) in &rows {
                        let count = match row.get(&field) {
                            Some(TypedValue::List(items)) => items.len() as i64,
                            Some(TypedValue::Null) | None => 0,
                            Some(_) => return Err(semantics::Reason::IncompatibleKind),
                        };
                        let min = invariant.min().unwrap_or(0);
                        let max = invariant.max().unwrap_or(i64::MAX);
                        if count < min || count > max {
                            violations.push(invariant.invariant_id().as_str().to_owned());
                        }
                    }
                }
                InvariantKind::AggregateConsistency => {
                    let Some(aggregate) = invariant.aggregate_ref() else {
                        return Err(semantics::Reason::AggregateUnknown);
                    };
                    if !self.evaluation.definitions.contains_key(aggregate.as_str()) {
                        return Err(semantics::Reason::AggregateUnknown);
                    }
                    if let Some(predicate) = invariant.predicate() {
                        let aggregate_rows: Vec<(String, Row)> = store
                            .rows_of(aggregate.as_str())
                            .into_iter()
                            .map(|(key, row)| (key.clone(), row.clone()))
                            .collect();
                        for (_, row) in &aggregate_rows {
                            let context = Context {
                                input: &empty_input,
                                row: Some(row),
                                entity: aggregate.as_str(),
                                clock,
                            };
                            if !semantics::evaluate(predicate, &context)? {
                                violations.push(invariant.invariant_id().as_str().to_owned());
                                break;
                            }
                        }
                    }
                }
                InvariantKind::MemberOfSet | InvariantKind::ImmutableAfterState => {
                    return Err(semantics::Reason::StateFieldUnbound);
                }
                InvariantKind::TargetCapability => {
                    // Declared-only: capability requirements address
                    // adapters, not row data; the reference evaluator
                    // records them and enforces nothing.
                }
            }
        }
        violations.dedup();
        Ok(violations)
    }
    /// Append one effect record under the next gap-free index; the
    /// record's step and index are assigned here so call sites stay
    /// within the argument bound.
    fn push_effect(
        &mut self,
        step: &str,
        mut record: EffectRecord,
    ) -> Result<usize, DiagnosticSet> {
        if self.effects.len() >= version::MAX_EFFECTS {
            return Err(diagnostic::limit_set("effects"));
        }
        record.step = step.to_owned();
        record.index = self.effects.len();
        let index = record.index;
        self.effects.push(record);
        Ok(index)
    }

    /// Evaluate every `then` assertion over the finished run.
    fn run_then(&mut self) -> Vec<AssertionRecord> {
        let scenario = self.scenario;
        let steps: Vec<&crate::scenario::step::ThenStep> = scenario.then().iter().collect();
        let mut records = Vec::new();
        for step in steps {
            let record = self.check_assertion(
                step.step_id.as_str(),
                step.observes.as_str(),
                &step.assertion,
            );
            records.push(record);
        }
        records
    }

    /// Evaluate one assertion.
    fn check_assertion(
        &mut self,
        step_id: &str,
        observes: &str,
        assertion: &Assertion,
    ) -> AssertionRecord {
        let verdict = self.verdict_of(observes, assertion);
        AssertionRecord {
            step_id: step_id.to_owned(),
            observes: observes.to_owned(),
            kind: assertion.kind_tag(),
            verdict,
        }
    }

    /// The verdict of one assertion against the finished run.
    fn verdict_of(&mut self, observes: &str, assertion: &Assertion) -> Verdict {
        match assertion {
            Assertion::Unsupported { capability, .. } => {
                let text = capability.as_str();
                if super::ABSENT_CAPABILITIES.contains(&text) {
                    Verdict::Pass
                } else if super::SUPPORTED_CAPABILITIES.contains(&text) {
                    Verdict::Fail("capability-available")
                } else {
                    Verdict::Unsupported("capability-unknown")
                }
            }
            Assertion::Authorization { .. } => Verdict::Unsupported("authorization"),
            Assertion::ContractMatch { .. } => Verdict::Unsupported("contract-match"),
            Assertion::DeterministicFixture { .. } => Verdict::Unsupported("fixture-digest"),
            Assertion::Result { value, .. } => {
                let Some(index) = self.when_index(observes) else {
                    return Verdict::Unsupported("observes-unknown");
                };
                let outcome = self.when[index].outcome.clone();
                match outcome {
                    Outcome::Ok { output } => match value {
                        None => Verdict::Pass,
                        Some(expected) => match self.resolve_leaf(expected) {
                            Ok(resolved) => {
                                if resolved == output {
                                    Verdict::Pass
                                } else {
                                    Verdict::Fail("value-mismatch")
                                }
                            }
                            Err(reason) => Verdict::Unsupported(reason),
                        },
                    },
                    Outcome::Error { .. } => Verdict::Fail("outcome-error"),
                    Outcome::Unsupported { reason } => Verdict::Unsupported(reason),
                }
            }
            Assertion::Error { error, payload, .. } => {
                if !payload.is_empty() {
                    return Verdict::Unsupported("payload-unmapped");
                }
                let Some(index) = self.when_index(observes) else {
                    return Verdict::Unsupported("observes-unknown");
                };
                let outcome = self.when[index].outcome.clone();
                match outcome {
                    Outcome::Error { .. } => {
                        // The typed error union of the operation is
                        // declared by the pinned #62 registry; the
                        // asserted error id must be a member of that
                        // union. Without a binding the verdict stays
                        // explicitly undecided.
                        let operation = self.when[index].operation.clone();
                        let binding = crate::error_contract::ErrorId::new(&operation)
                            .and_then(|id| self.evaluation.registry.binding(&id));
                        let Some(binding) = binding else {
                            return Verdict::Unsupported("error-union-unavailable");
                        };
                        let asserted = match crate::error_contract::ErrorId::new(error.as_str()) {
                            Some(id) => id,
                            None => return Verdict::Unsupported("error-id-invalid"),
                        };
                        if binding.errors().contains(&asserted) {
                            Verdict::Pass
                        } else {
                            Verdict::Fail("error-not-declared")
                        }
                    }
                    Outcome::Ok { .. } => Verdict::Fail("outcome-ok"),
                    Outcome::Unsupported { reason } => Verdict::Unsupported(reason),
                }
            }
            Assertion::EntityState {
                entity,
                selector,
                expect,
                fields,
            } => self.entity_state_verdict(entity.as_str(), selector, expect, fields),
            Assertion::Emitted { target, count } => {
                let actual = self
                    .effects
                    .iter()
                    .filter(|effect| {
                        effect.step == observes
                            && effect.kind == EffectKind::EventIntent
                            && effect.target.as_deref() == Some(target.id.as_str())
                    })
                    .count() as u32;
                let holds = match count {
                    None => actual >= 1,
                    Some(OccurrenceCount::AtLeast(minimum)) => actual >= *minimum,
                    Some(OccurrenceCount::Exactly(exact)) => actual == *exact,
                };
                if holds {
                    Verdict::Pass
                } else {
                    Verdict::Fail("emission-count")
                }
            }
            Assertion::ForbiddenEffect {
                effect,
                scope,
                field,
            } => {
                let Some(Definition::Effect(effect_def)) =
                    self.evaluation.definitions.get(effect.as_str())
                else {
                    return Verdict::Unsupported("effect-unknown");
                };
                let effect_entity = effect_def.entity.as_str();
                let writes: Vec<&EffectRecord> = self
                    .effects
                    .iter()
                    .filter(|entry| {
                        entry.step == observes
                            && entry.kind == EffectKind::EntityWrite
                            && entry.entity.as_deref() == Some(effect_entity)
                    })
                    .collect();
                let happened = match scope {
                    EffectScope::Entity => writes.iter().any(|write| {
                        write.operation == Some(effect_operation_of(effect_def.operation))
                    }),
                    EffectScope::Field => {
                        let Some(field) = field else {
                            return Verdict::Unsupported("scope-field-missing");
                        };
                        writes.iter().any(|write| {
                            write.operation == Some(effect_operation_of(effect_def.operation))
                                && write.fields.contains(&field.as_str().to_owned())
                        })
                    }
                    EffectScope::Resource => !writes.is_empty(),
                };
                if happened {
                    Verdict::Fail("effect-happened")
                } else {
                    Verdict::Pass
                }
            }
            Assertion::Idempotency { replay, .. } => {
                let Some(replayed) = self
                    .when
                    .iter()
                    .find(|record| record.step_id == replay.as_str())
                else {
                    return Verdict::Unsupported("replay-unknown");
                };
                match &replayed.outcome {
                    Outcome::Ok { .. } => {
                        if replayed.replay_of.is_some() && replayed.effects.is_empty() {
                            Verdict::Pass
                        } else {
                            Verdict::Fail("not-replayed")
                        }
                    }
                    Outcome::Error { .. } => Verdict::Fail("replay-error"),
                    Outcome::Unsupported { reason } => Verdict::Unsupported(reason),
                }
            }
        }
    }

    /// The record index of one observed `when` step.
    fn when_index(&self, observes: &str) -> Option<usize> {
        self.when
            .iter()
            .position(|record| record.step_id == observes)
    }

    /// The verdict of one entity-state assertion over the final
    /// state.
    fn entity_state_verdict(
        &mut self,
        entity: &str,
        selector: &[FieldPredicate],
        expect: &EntityExpectation,
        fields: &[(crate::scenario::id::FieldName, FieldExpectation)],
    ) -> Verdict {
        if self.entity_fields(entity).is_none() {
            return Verdict::Unsupported("entity-unknown");
        }
        let mut resolved: Vec<(String, TypedValue)> = Vec::new();
        for term in selector {
            match self.resolve_leaf(&term.equals) {
                Ok(value) => resolved.push((term.field.as_str().to_owned(), value)),
                Err(reason) => return Verdict::Unsupported(reason),
            }
        }
        let matched: Vec<(String, Row)> = self
            .store
            .rows_of(entity)
            .into_iter()
            .filter(|(_, row)| {
                resolved
                    .iter()
                    .all(|(field, value)| row.get(field) == Some(value))
            })
            .map(|(key, row)| (key.clone(), row.clone()))
            .collect();
        let presence_holds = match expect {
            EntityExpectation::Exists => !matched.is_empty(),
            EntityExpectation::Missing => matched.is_empty(),
            EntityExpectation::Count(exact) => matched.len() as u32 == *exact,
        };
        if !presence_holds {
            return Verdict::Fail(match expect {
                EntityExpectation::Count(_) => "count",
                _ => "presence",
            });
        }
        for (_, row) in &matched {
            for (field, expectation) in fields {
                let actual = row.get(field.as_str());
                match expectation {
                    FieldExpectation::Value(leaf) => {
                        let expected = match self.resolve_leaf(leaf) {
                            Ok(value) => value,
                            Err(reason) => return Verdict::Unsupported(reason),
                        };
                        if actual != Some(&expected) {
                            return Verdict::Fail("field-mismatch");
                        }
                    }
                    FieldExpectation::Match(kind) => {
                        let holds = match (kind, actual) {
                            (MatchKind::Datetime, Some(TypedValue::Datetime(_)))
                            | (MatchKind::Uuid, Some(TypedValue::Uuid(_)))
                            | (MatchKind::Uri, Some(TypedValue::Uri(_)))
                            | (MatchKind::Decimal, Some(TypedValue::Decimal(_))) => true,
                            (MatchKind::NonNull, Some(value)) => *value != TypedValue::Null,
                            _ => false,
                        };
                        if !holds {
                            return Verdict::Fail("matcher-mismatch");
                        }
                    }
                }
            }
        }
        Verdict::Pass
    }

    /// The final-state snapshots, entity-major and key-sorted.
    fn snapshots(&self) -> Vec<RowSnapshot> {
        let mut snapshots = Vec::new();
        for entity in self.store.entities() {
            for (key, row) in self.store.rows_of(entity) {
                snapshots.push(RowSnapshot {
                    entity: entity.clone(),
                    key: key.clone(),
                    fields: row
                        .iter()
                        .map(|(field, value)| (field.clone(), value.clone()))
                        .collect(),
                });
            }
        }
        snapshots
    }
}

/// The wire token of one IR effect operation.
const fn effect_operation_of(operation: EffectOperation) -> &'static str {
    match operation {
        EffectOperation::Create => "create",
        EffectOperation::Update => "update",
        EffectOperation::Delete => "delete",
    }
}

/// The canonical state bytes for the state digest.
fn state_bytes(state: &[RowSnapshot]) -> String {
    let members: Vec<String> = state
        .iter()
        .map(|row| {
            super::canonical::object(vec![
                ("entity", Some(super::canonical::string(&row.entity))),
                ("key", Some(super::canonical::string(&row.key))),
                (
                    "fields",
                    Some(super::canonical::array(
                        &row.fields
                            .iter()
                            .map(|(field, value)| {
                                super::canonical::object(vec![
                                    ("field", Some(super::canonical::string(field))),
                                    ("value", Some(super::canonical::typed_value(value))),
                                ])
                            })
                            .collect::<Vec<_>>(),
                    )),
                ),
            ])
        })
        .collect();
    super::canonical::array(&members)
}

/// The canonical effect bytes for the effect digest.
fn effects_bytes(effects: &[EffectRecord]) -> String {
    let members: Vec<String> = effects
        .iter()
        .map(|effect| {
            super::canonical::object(vec![
                (
                    "entity",
                    effect.entity.as_deref().map(super::canonical::string),
                ),
                (
                    "fields",
                    Some(super::canonical::array(
                        &effect
                            .fields
                            .iter()
                            .map(|field| super::canonical::string(field))
                            .collect::<Vec<_>>(),
                    )),
                ),
                ("index", Some(super::canonical::number(effect.index as i64))),
                ("kind", Some(super::canonical::string(effect.kind.as_str()))),
                ("operation", effect.operation.map(super::canonical::string)),
                ("row", effect.row.as_deref().map(super::canonical::string)),
                ("step", Some(super::canonical::string(&effect.step))),
                (
                    "target",
                    effect.target.as_deref().map(super::canonical::string),
                ),
            ])
        })
        .collect();
    super::canonical::array(&members)
}

/// The overall status: any failed assertion fails the evaluation;
/// otherwise any unsupported step or assertion makes it unsupported;
/// otherwise it passes.
fn overall_status(
    given: &[GivenRecord],
    when: &[WhenRecord],
    assertions: &[AssertionRecord],
) -> Status {
    if assertions
        .iter()
        .any(|record| matches!(record.verdict, Verdict::Fail(_)))
    {
        return Status::Fail;
    }
    let unsupported = given.iter().any(|record| record.status.is_err())
        || when
            .iter()
            .any(|record| matches!(record.outcome, Outcome::Unsupported { .. }))
        || assertions
            .iter()
            .any(|record| matches!(record.verdict, Verdict::Unsupported(_)));
    if unsupported {
        Status::Unsupported
    } else {
        Status::Pass
    }
}

/// The row projected as one typed object value.
fn row_object(row: &Row) -> TypedValue {
    TypedValue::Object(
        row.iter()
            .map(|(field, value)| {
                (
                    crate::scenario::id::FieldName::parse(field)
                        .expect("row fields carry valid scenario field names"),
                    value.clone(),
                )
            })
            .collect(),
    )
}
