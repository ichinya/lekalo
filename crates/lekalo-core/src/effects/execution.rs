//! Authoritative transaction/case joins for an execution-owner receipt. Raw
//! envelope declarations never enter this channel; the capability is sealed by
//! the deterministic scenario owner after engine transitions and assertions.

use crate::diagnostics::DiagnosticSet;
use crate::extended_effects::{ExtendedEffectsAttachment, Fault, Outcome};
use crate::task_links::execution::Execution;
use crate::transaction_concurrency::{TransactionConcurrencyAttachment, TransactionMode};

use super::{diagnostic, EffectEdge, EffectGraph, EffectKind};

pub(crate) fn attach_execution(
    graph: &EffectGraph,
    execution: &Execution,
    transactions: &TransactionConcurrencyAttachment,
    extended: &ExtendedEffectsAttachment,
) -> Result<EffectGraph, DiagnosticSet> {
    let refuse = diagnostic::input_invalid_detail;
    let envelope = execution.envelope();
    // Prevent a declaration-only graph from inheriting another run's trust.
    if graph.envelope_count() != 0 || graph.execution_digest.is_some() {
        return Err(refuse("execution-mixed-custody"));
    }
    if envelope.source_revision != graph.ir_digest()
        || transactions.project_id().as_str() != graph.project_id().unwrap_or_default()
        || extended.project_id().as_str() != graph.project_id().unwrap_or_default()
        || transactions.ir_digest().as_str() != graph.ir_digest()
        || extended.ir_digest().as_str() != graph.ir_digest()
        || transactions.model_ref().digest().as_str() != graph.ir_digest()
        || extended.model_ref().digest().as_str() != graph.ir_digest()
        || transactions.model_ref().version().as_str() != graph.model_version().as_str()
        || extended.model_ref().version().as_str() != graph.model_version().as_str()
        || transactions.effect_graph_digest().map(|pin| pin.as_str()) != Some(graph.ir_digest())
        || extended.effect_graph_digest().map(|pin| pin.as_str()) != Some(graph.ir_digest())
    {
        return Err(refuse("execution-owner-pin-mismatch"));
    }
    transactions.validate_against_graph(graph)?;
    // Every executed local group must join EXACT owner membership, in both
    // directions. Same-looking names, omitted decision/audit writes, optional
    // transactions and network membership confer no atomic semantics.
    let mut declared = graph.declared().to_vec();
    for observed in &envelope.transactions {
        let groups: Vec<_> = transactions
            .operations()
            .iter()
            .flat_map(|operation| {
                operation
                    .groups()
                    .iter()
                    .map(move |group| (operation, group))
            })
            .filter(|(_, group)| group.group_id() == &observed.group)
            .collect();
        if groups.len() != 1 {
            return Err(refuse("execution-group-unresolved"));
        }
        let (operation, group) = groups[0];
        if operation.transaction() != TransactionMode::Required
            || group.effect_refs().len() != observed.members.len()
        {
            return Err(refuse("execution-group-members"));
        }
        for reference in group.effect_refs() {
            let index = declared
                .iter()
                .position(|edge| edge.key().to_canonical_string() == reference.as_str())
                .ok_or_else(|| refuse("execution-group-members"))?;
            let edge = &declared[index];
            if edge.key().kind() == EffectKind::ExternalCall
                || !observed.members.iter().any(|member| {
                    member.operation == edge.key().operation().semantic_id()
                        && member.kind == edge.key().kind()
                        && member.subject
                            == format!(
                                "{}:{}",
                                edge.key().subject().resource().kind().key(),
                                edge.key().subject().resource().as_str()
                            )
                })
            {
                return Err(refuse("execution-group-members"));
            }
            declared[index] = EffectEdge::new(
                edge.key().clone(),
                edge.provenance().clone(),
                Some(observed.group.clone()),
                edge.sensitivity().cloned(),
            );
        }
    }
    if transactions
        .operations()
        .iter()
        .flat_map(|operation| operation.groups())
        .count()
        != envelope.transactions.len()
    {
        return Err(refuse("execution-group-unobserved"));
    }
    let mut joined = super::build::attach_attested(graph, envelope)?;
    extended.validate_against_graph(&joined)?;
    for call in extended.calls() {
        let resource = call.common().effect.subject();
        let Some((request, response)) = execution.provider_schemas(resource) else {
            return Err(refuse("execution-provider-schema-unresolved"));
        };
        if call.request_digest.as_str() != request || call.response_digest.as_str() != response {
            return Err(refuse("execution-provider-schema-pin"));
        }
    }
    if extended
        .jobs()
        .iter()
        .any(|job| job.payload_digest.as_str() != execution.job_payload_digest())
    {
        return Err(refuse("execution-job-schema-pin"));
    }
    // Exact extended owner contracts must cover every observed non-local edge.
    // The binding's effect identity includes operation/kind/subject/occurrence.
    let bindings = extended.effect_bindings();
    for edge in joined.detected().iter().filter(|edge| {
        matches!(
            edge.key().kind(),
            EffectKind::ExternalCall | EffectKind::EnqueueJob | EffectKind::EmitEvent
        )
    }) {
        if !bindings.iter().any(|binding| {
            binding.graph && binding.scenarios && binding.effect == edge.key().to_canonical_string()
        }) {
            return Err(refuse("execution-effect-unresolved"));
        }
    }
    for observed in &envelope.entries {
        let Some(trace) = &observed.partial_failure else {
            continue;
        };
        let case = extended
            .cases()
            .iter()
            .find(|case| case.case_id() == trace.case_ref)
            .ok_or_else(|| refuse("execution-case-unresolved"))?;
        if !execution
            .report()
            .scenarios
            .iter()
            .any(|id| id == case.scenario().scenario_id())
            || case.scenario().ir_digest()
                != crate::task_links::execution::scenario_digest_from_run(
                    execution,
                    case.scenario().scenario_id(),
                )
                .unwrap_or_default()
            || case.steps().len() != trace.recipients.len()
            || case.outcomes().len() != trace.recipients.len()
        {
            return Err(refuse("execution-case-pin-mismatch"));
        }
        for recipient in &trace.recipients {
            let call = extended
                .calls()
                .iter()
                .find(|call| {
                    bindings.iter().any(|binding| {
                        binding.contract_ref == call.contract_ref()
                            && binding
                                .effect
                                .contains(&format!("|external-call|{}|", recipient.provider))
                    })
                })
                .ok_or_else(|| refuse("execution-case-contract"))?;
            let step = case
                .steps()
                .iter()
                .find(|step| step.contract_ref() == call.contract_ref())
                .ok_or_else(|| refuse("execution-case-contract"))?;
            let outcome = case
                .outcomes()
                .iter()
                .find(|row| row.contract_ref() == call.contract_ref())
                .ok_or_else(|| refuse("execution-case-contract"))?;
            if recipient.error_class.as_deref().is_some_and(|class| {
                !call
                    .error_refs()
                    .any(|reference| reference.ends_with(&format!("/{class}")))
            }) {
                return Err(refuse("execution-case-error-contract"));
            }
            let coherent = match recipient.result {
                super::RecipientOutcome::Delivered => {
                    step.fault() == Fault::None && outcome.outcome() == Outcome::Success
                }
                super::RecipientOutcome::Failed => {
                    recipient.error_class.as_deref() == Some("timeout")
                        && step.fault() == Fault::Timeout
                        && outcome.outcome() == Outcome::Error
                }
            };
            if !coherent {
                return Err(refuse("execution-case-outcome"));
            }
        }
    }
    joined.declared = declared;
    joined.execution_digest = Some(execution.report().result_digest.clone());
    joined.index = super::index::EffectIndex::build(&joined.declared, &joined.detected);
    Ok(joined)
}
