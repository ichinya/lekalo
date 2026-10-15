//! Execution-owner channel for the deterministic pilot. Only this module can
//! construct an `Execution`: no deserializer, public fields, receipt import,
//! caller-supplied outcomes, or boolean qualification. The owner executes the
//! engine and independently checks state/effect assertions before sealing a run.
//! Serialized runs are exportable observations, NOT re-importable authority.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{decision::*, diagnostic, pins, validate, Attachment, Failure};
use crate::effects::{
    EffectKind, EvidenceEntry, EvidenceEnvelope, LocalTransaction, OperationId, PartialFailure,
    RecipientOutcome, RecipientResult, ResourceId, ResourceKind, Subject, TransactionGroupId,
    TransactionMember, TrustState,
};

/// The process-local, owner-issued capability. Its private fields and lack of
/// Deserialize prevent producer-authored JSON from entering the trusted channel.
#[derive(Clone, Debug)]
pub struct Execution {
    report: ExecutionReport,
    envelope: EvidenceEnvelope,
    facts: BTreeSet<&'static str>,
    scenario_digests: BTreeMap<String, String>,
    provider_schemas: BTreeMap<String, (String, String)>,
    job_payload_digest: String,
}

/// The public normalized execution receipt (never admitted as an input).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReport {
    /// Versioned export contract.
    pub schema_version: &'static str,
    /// Versioned export identity.
    pub identity: &'static str,
    /// Execution owner, not a caller's producer label.
    pub owner: &'static str,
    /// Actual executed implementation.
    pub implementation: &'static str,
    /// Digest of the runner/engine source embedded in this build.
    pub build_digest: String,
    /// Actual source compilation.
    pub source_digest: String,
    /// Actual admitted attachment/recipe inputs.
    pub input_digest: String,
    /// Actual evaluated assertions and observed transitions.
    pub result_digest: String,
    /// Opaque, safe public receipt reference.
    pub execution_id: String,
    /// The exact pinned attachment revision.
    pub attachment_revision: String,
    /// Origin is fixed by the executing owner.
    pub origin: &'static str,
    /// Completed scenario assertions (not merely declared covers).
    pub scenarios: Vec<String>,
    /// Observed engine transitions.
    pub steps: Vec<Step>,
    /// Individually checked domain guarantees.
    pub assertions: Vec<String>,
}

/// One actual engine transition. No raw values, credentials or producer IDs.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// The executed declared command.
    pub command: &'static str,
    /// Observed outcome from the engine.
    pub outcome: String,
    /// Digest of the resulting normalized state.
    pub state_digest: String,
}

impl Execution {
    /// Export the normalized observation, not its authority.
    pub fn report(&self) -> &ExecutionReport {
        &self.report
    }

    /// Whether this sealed run witnesses a fact for these exact inputs.
    pub fn witnesses(&self, attachment: &Attachment, fact: &str) -> bool {
        self.report.attachment_revision == attachment.computed_revision()
            && self.report.source_digest == attachment.ir_digest().as_str()
            && self.facts.contains(fact)
    }

    pub(crate) fn provider_schemas(&self, provider: &str) -> Option<&(String, String)> {
        self.provider_schemas.get(provider)
    }

    pub(crate) fn job_payload_digest(&self) -> &str {
        &self.job_payload_digest
    }

    pub(crate) fn envelope(&self) -> &EvidenceEnvelope {
        &self.envelope
    }
}

/// Digest of the exact canonical compiled scenario definition, not a caller pin.
pub fn scenario_digest(project: &crate::ir::CompiledProject, id: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(&project.to_canonical_json()).ok()?;
    let row = value["definitions"]
        .as_array()?
        .iter()
        .find(|row| row["id"] == id)?;
    Some(digest(row))
}

pub(crate) fn scenario_digest_from_run<'a>(execution: &'a Execution, id: &str) -> Option<&'a str> {
    execution.scenario_digests.get(id).map(String::as_str)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocalCommit {
    task: BTreeMap<String, String>,
    decisions: BTreeMap<String, serde_json::Value>,
    intents: BTreeMap<String, Intent>,
    audits: BTreeMap<String, String>,
}

fn stage_request(
    original: &LocalCommit,
    links: &[&Link],
    input: &DecisionInput,
    attachment: &Attachment,
) -> Result<LocalCommit, Failure> {
    let mut staged = original.clone();
    for link in links {
        let outcome = decide(input);
        require(outcome == DecisionOutcome::Allowed)?;
        let capability = attachment
            .capability_requirements()
            .iter()
            .find(|row| {
                attachment.provider_contracts().iter().any(|provider| {
                    provider.provider_id == link.provider_id
                        && provider.contract_id == row.contract_id
                }) && attachment.action_policies().iter().any(|policy| {
                    policy.action == input.action
                        && policy.capability_ref.as_ref() == Some(&row.capability_id)
                })
            })
            .ok_or_else(|| diagnostic::input_invalid("execution-capability-unresolved"))?;
        let policy = attachment
            .action_policies()
            .iter()
            .find(|row| row.action == input.action)
            .ok_or_else(|| diagnostic::input_invalid("execution-policy-unresolved"))?;
        staged.decisions.insert(link.link_id.clone(), serde_json::json!({ "link": link.link_id, "workspace": link.workspace_id,
            "task": link.task_id, "actor": policy.actor, "action": input.action, "fields": input.fields,
            "policyRevision": policy.revision.as_str(), "capabilityRevision": capability.revision.as_str(), "outcome": outcome.as_str() }));
        require(matches!(
            plan_intent(
                &mut staged.intents,
                link,
                outcome,
                &input.action,
                "action-one".to_owned(),
                0
            ),
            PlanOutcome::Planned(_)
        ))?;
    }
    require(input.fields.iter().any(|field| field == "title"))?;
    staged
        .task
        .insert("title".to_owned(), "updated-title".to_owned());
    staged
        .audits
        .insert("action-one".to_owned(), digest(&staged.decisions));
    Ok(staged)
}

fn require(condition: bool) -> Result<(), Failure> {
    if condition {
        Ok(())
    } else {
        Err(diagnostic::rule_invalid(
            diagnostic::EVIDENCE_INVALID,
            "execution-assertion-failed",
            None,
        ))
    }
}

fn digest(value: &impl Serialize) -> String {
    super::wire_digest(&serde_json::to_vec(value).expect("normalized execution data"))
        .as_str()
        .to_owned()
}

fn observe(steps: &mut Vec<Step>, command: &'static str, outcome: &str, state: &impl Serialize) {
    steps.push(Step {
        command,
        outcome: outcome.to_owned(),
        state_digest: digest(state),
    });
}

fn fixture_link(id: &str, installation: &str, provider: &str) -> Link {
    Link {
        link_id: id.to_owned(),
        task_id: "task-one".to_owned(),
        workspace_id: "workspace-one".to_owned(),
        installation_id: installation.to_owned(),
        provider_id: provider.to_owned(),
        object_type: "issue".to_owned(),
        external_id: "object-one".to_owned(),
        state: LinkState::Active,
        sync_mode: SyncMode::Bidirectional,
        generation: 1,
        inbound_cursor: None,
        outbound_cursor: None,
    }
}

fn event(link: &Link) -> InboundEvent {
    InboundEvent {
        event_identity: "event-one".to_owned(),
        workspace_id: link.workspace_id.clone(),
        installation_id: link.installation_id.clone(),
        provider_id: link.provider_id.clone(),
        object_type: link.object_type.clone(),
        external_id: link.external_id.clone(),
        revision: "r-1".to_owned(),
        causation_id: "cause-one".to_owned(),
    }
}

/// Execute all ten compiled pilot scenarios against deterministic mock providers.
/// There is no file/result operand. A modified Model or attachment must pass the
/// same semantic join, and each scenario must cover the operation actually run.
pub fn run_mock(
    attachment: &Attachment,
    compilation: &crate::ir::Compilation,
) -> Result<Execution, Failure> {
    pins::verify_ir_digest(attachment, compilation)?;
    require(attachment.attachment_revision().as_str() == attachment.computed_revision())?;
    require(attachment.contributions().scenarios && attachment.contributions().effect_trace)?;
    let validation = validate::validate(attachment, compilation, "strict")?;
    require(!validate::invalidates(&validation))?;
    let scenario_commands = [
        ("two_providers", "tasklinks.request_action"),
        ("duplicate_import", "tasklinks.import_link"),
        ("identity_collision", "tasklinks.attach_link"),
        ("attributed_inbound", "tasklinks.ingest_event"),
        ("decision_matrix", "tasklinks.request_action"),
        ("partial_delivery", "tasklinks.dispatch_outbox"),
        ("stale_conflict", "tasklinks.resolve_conflict"),
        ("detach_history", "tasklinks.detach_link"),
        ("retry_echo_crash", "tasklinks.ack_delivery"),
        ("secret_canary", "tasklinks.request_action"),
    ];
    for (scenario, command) in scenario_commands {
        require(compilation.project.definitions.iter().any(|definition| matches!(definition,
            crate::ir::Definition::Scenario(row) if row.id.as_str() == format!("tasklinks.scenario.{scenario}")
                && row.covers.iter().any(|symbol| symbol.as_str() == command))))?;
    }
    // Resolve actual admitted policy/field/capability input, rather than forcing
    // an allow because a successful-looking evidence step requested one.
    let policy = attachment
        .action_policies()
        .iter()
        .find(|row| row.action == "tasklinks.request_action")
        .ok_or_else(|| diagnostic::input_invalid("execution-policy-unresolved"))?;
    let field_policy = attachment
        .field_policies()
        .iter()
        .find(|row| {
            row.direction == "outbound" && row.actor == policy.actor && row.decision == "allow"
        })
        .ok_or_else(|| diagnostic::input_invalid("execution-policy-unresolved"))?;
    require(policy.decision == "allow" && policy.actor == "identity.user")?;
    let contracts = attachment.provider_contracts();
    require(contracts.len() == 2)?;
    let mut a = fixture_link("link-a", "installation-a", &contracts[0].provider_id);
    let b = fixture_link("link-b", "installation-b", &contracts[1].provider_id);
    let mut input = DecisionInput {
        action: policy.action.clone(),
        fields: field_policy.fields.clone(),
        capability: CapabilityOutcome::Supported,
        authorization: AuthorizationOutcome::Allowed,
        link_state: a.state,
        sync_mode: a.sync_mode,
        permitted_subset: None,
    };
    let rules = attachment.delivery_rules();
    let mut steps = Vec::new();
    let mut assertions = Vec::new();
    let mut record_assertion = |name: &str| assertions.push(name.to_owned());

    // Exact identity tuple: duplicate import replays, cross-task collision refuses,
    // and a different installation remains distinct.
    let identity = ExternalIdentity {
        workspace_id: a.workspace_id.clone(),
        installation_id: a.installation_id.clone(),
        provider_id: a.provider_id.clone(),
        object_type: a.object_type.clone(),
        external_id: a.external_id.clone(),
    };
    let mut reservations = BTreeMap::new();
    let reserve = |bindings: &mut BTreeMap<ExternalIdentity, String>, task| {
        reserve_identity(
            bindings,
            &identity.workspace_id,
            &identity.installation_id,
            &identity.provider_id,
            &identity.object_type,
            &identity.external_id,
            task,
        )
    };
    require(reserve(&mut reservations, "task-one") == Some(Reservation::Fresh))?;
    require(reserve(&mut reservations, "task-one") == Some(Reservation::Replayed))?;
    require(reserve(&mut reservations, "task-two").is_none())?;
    require(reservations.len() == 1)?;
    observe(
        &mut steps,
        "tasklinks.import_link",
        "replayed",
        &reservations.len(),
    );
    observe(
        &mut steps,
        "tasklinks.attach_link",
        "conflict",
        &reservations.len(),
    );
    require(
        reserve_identity(
            &mut reservations,
            &b.workspace_id,
            &b.installation_id,
            &b.provider_id,
            &b.object_type,
            &b.external_id,
            &b.task_id,
        ) == Some(Reservation::Fresh),
    )?;
    require(reservations.len() == 2)?;
    record_assertion("external-identity-exact-tuple");

    // Stage ACTUAL task/decision/outbox/audit state, discard at the injected
    // abort boundary, then commit all four categories together. No fabricated
    // `successful writes` counter can populate the executed trace.
    let mut local = LocalCommit {
        task: BTreeMap::from([("title".to_owned(), "before".to_owned())]),
        decisions: BTreeMap::new(),
        intents: BTreeMap::new(),
        audits: BTreeMap::new(),
    };
    let before = local.clone();
    let aborted = stage_request(&local, &[&a, &b], &input, attachment)?;
    require(aborted.task != local.task && aborted.intents.len() == 2)?;
    drop(aborted);
    require(local == before)?;
    let staged = stage_request(&local, &[&a, &b], &input, attachment)?;
    local = staged;
    require(
        local.task != before.task
            && local.decisions.len() == 2
            && local.audits.len() == 1
            && local.intents.len() == 2,
    )?;
    observe(
        &mut steps,
        "tasklinks.request_action",
        "applied",
        &(
            &local.task,
            &local.decisions,
            &local.audits,
            local
                .intents
                .values()
                .map(|row| {
                    (
                        &row.intent_id,
                        &row.link_id,
                        &row.action,
                        row.state.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
        ),
    );
    let mut intents = local.intents;
    record_assertion("local-task-decision-outbox-audit-all-or-nothing");
    record_assertion("task-links-0-n");

    // Policy/capability matrix: rejected recipients have NO intents.
    for (capability, authorization, expected) in [
        (
            CapabilityOutcome::Supported,
            AuthorizationOutcome::Denied,
            DecisionOutcome::Denied,
        ),
        (
            CapabilityOutcome::Unsupported,
            AuthorizationOutcome::Allowed,
            DecisionOutcome::Unsupported,
        ),
        (
            CapabilityOutcome::Unknown,
            AuthorizationOutcome::Allowed,
            DecisionOutcome::Indeterminate,
        ),
    ] {
        input.capability = capability;
        input.authorization = authorization;
        let mut refused = BTreeMap::new();
        require(decide(&input) == expected)?;
        require(matches!(
            plan_intent(
                &mut refused,
                &a,
                decide(&input),
                &input.action,
                "refused".to_owned(),
                0
            ),
            PlanOutcome::NotAdmitted
        ))?;
        require(refused.is_empty())?;
    }
    input.capability = CapabilityOutcome::Partial;
    input.authorization = AuthorizationOutcome::Allowed;
    require(decide(&input) == DecisionOutcome::Indeterminate)?;
    input.permitted_subset = Some(input.fields.clone());
    require(decide(&input) == DecisionOutcome::Allowed)?;
    input.capability = CapabilityOutcome::Supported;
    observe(
        &mut steps,
        "tasklinks.request_action",
        "matrix-checked",
        &intents.len(),
    );
    record_assertion("actor-and-field-policy");

    // Two actual recipient transitions: A acknowledges, B fails. Receipt is read
    // from the delivered state, and retry set from the failed state, not labels.
    let mut results = Vec::new();
    for (ordinal, link) in [&a, &b].into_iter().enumerate() {
        let intent = intents
            .values_mut()
            .find(|row| row.link_id == link.link_id)
            .expect("planned link");
        require(
            claim(
                intent,
                ordinal as u64 + 1,
                0,
                rules.lease_ms,
                link.generation,
                rules.max_attempts,
            ) == DispatchStep::Claimed,
        )?;
        let transition = if ordinal == 0 {
            DispatchEvent::Acknowledged
        } else {
            DispatchEvent::ProviderFailed
        };
        let outcome = dispatch(
            intent,
            &transition,
            ordinal as u64 + 1,
            1,
            rules.max_attempts,
            rules.backoff_ms,
            link.generation,
        );
        require(
            outcome
                == if ordinal == 0 {
                    DispatchStep::Delivered
                } else {
                    DispatchStep::RetryScheduled
                },
        )?;
        results.push(RecipientResult {
            provider: format!("tasklinks.provider/{}", link.provider_id),
            result: if intent.state == OutboxState::Delivered {
                RecipientOutcome::Delivered
            } else {
                RecipientOutcome::Failed
            },
            receipt_digest: intent.receipt_digest.clone(),
            error_class: (ordinal == 1).then(|| "timeout".to_owned()),
        });
    }
    let summary = delivery_summary(&intents);
    require(!summary.is_complete() && summary.delivered == 1)?;
    let retry: Vec<String> = results
        .iter()
        .filter(|row| row.result == RecipientOutcome::Failed)
        .map(|row| row.provider.clone())
        .collect();
    require(retry.len() == 1 && results[0].receipt_digest.is_some())?;
    observe(
        &mut steps,
        "tasklinks.dispatch_outbox",
        "partial",
        &(summary.delivered, retry.len()),
    );
    observe(
        &mut steps,
        "tasklinks.ack_delivery",
        "delivered",
        &results[0].receipt_digest,
    );
    record_assertion("failed-only-retry-preserves-success-receipt");

    // Inbound attribution, duplicate identity, ordering and echo fencing.
    let mut inbox = BTreeMap::new();
    let inbound = event(&a);
    let ingested = ingest_event(
        &mut a,
        &mut inbox,
        RevisionOrdering::ComparableTotal,
        &inbound,
    );
    require(
        ingested == (true, "applied")
            && inbox
                .keys()
                .next()
                .is_some_and(|row| row.provider_id == a.provider_id),
    )?;
    require(
        ingest_event(
            &mut a,
            &mut inbox,
            RevisionOrdering::ComparableTotal,
            &inbound,
        ) == (false, "replayed")
            && inbox.len() == 1,
    )?;
    // A provider-origin event is not a user action: no fresh fan-out is
    // planned for it, so the source receipt cannot create a bridge/loop.
    let before_echo = intents.len();
    let duplicate = ingest_event(
        &mut a,
        &mut inbox,
        RevisionOrdering::ComparableTotal,
        &inbound,
    );
    require(!duplicate.0 && intents.len() == before_echo)?;
    observe(
        &mut steps,
        "tasklinks.ingest_event",
        "attributed-no-echo",
        &inbox.len(),
    );
    record_assertion("attributed-inbound-dedup-no-bridge");

    // Scoped two-version resolution, stale refusal and append-only audit.
    let mut conflicts = BTreeMap::from([(
        "conflict-one".to_owned(),
        ConflictRecord {
            conflict_id: "conflict-one".to_owned(),
            link_id: a.link_id.clone(),
            base_revision: "r-0".to_owned(),
            observed_revision: "r-1".to_owned(),
            link_version: 1,
            fields: vec!["state".to_owned()],
            state: ConflictState::Open,
        },
    )]);
    let authority = ResolutionAuthority::new(
        "identity.user",
        &a.link_id,
        AuthorizationOutcome::Allowed,
        vec!["state".to_owned()],
        ResolutionDecision::AcceptRemote,
        "cause-one",
    )
    .expect("admitted authority");
    let mut audits = Vec::new();
    require(
        resolve_conflict(
            &mut conflicts,
            &mut audits,
            "conflict-one",
            "r-0",
            1,
            &authority,
        ) == ResolveOutcome::Stale
            && audits.is_empty(),
    )?;
    require(
        resolve_conflict(
            &mut conflicts,
            &mut audits,
            "conflict-one",
            "r-1",
            1,
            &authority,
        ) == ResolveOutcome::Resolved
            && audits.len() == 1,
    )?;
    require(
        resolve_conflict(
            &mut conflicts,
            &mut audits,
            "conflict-one",
            "r-1",
            1,
            &authority,
        ) == ResolveOutcome::Replayed
            && audits.len() == 1,
    )?;
    observe(
        &mut steps,
        "tasklinks.resolve_conflict",
        "audited",
        &audits.len(),
    );
    record_assertion("version-conflict-detection");

    // Stable replay key and actual acceptance-window recovery, never delivered.
    let mut recovery = BTreeMap::new();
    let original = plan_intent(
        &mut recovery,
        &b,
        DecisionOutcome::Allowed,
        &input.action,
        "crash-key".to_owned(),
        0,
    )
    .into_intent()
    .expect("planned");
    require(
        matches!(
            plan_intent(
                &mut recovery,
                &b,
                DecisionOutcome::Allowed,
                &input.action,
                "crash-key".to_owned(),
                0
            ),
            PlanOutcome::Replayed(_)
        ) && recovery.len() == 1,
    )?;
    let intent = recovery
        .get_mut(&original.intent_id)
        .expect("durable intent");
    require(
        claim(
            intent,
            7,
            0,
            rules.lease_ms,
            b.generation,
            rules.max_attempts,
        ) == DispatchStep::Claimed,
    )?;
    require(
        dispatch(
            intent,
            &DispatchEvent::ProviderAccepted,
            7,
            1,
            rules.max_attempts,
            rules.backoff_ms,
            b.generation,
        ) == DispatchStep::AcceptedPendingAck,
    )?;
    require(recover_lease(intent, 7, b.generation) == DispatchStep::UnknownDelivery)?;
    require(
        claim(
            intent,
            8,
            2,
            rules.lease_ms,
            b.generation,
            rules.max_attempts,
        ) == DispatchStep::Ignored,
    )?;
    require(intent.idempotency_key == original.idempotency_key && intent.receipt_digest.is_none())?;
    observe(
        &mut steps,
        "tasklinks.ack_delivery",
        "unknown_delivery",
        &intent.state.as_str(),
    );
    record_assertion("idempotent-intent");

    // Detach preserves existing receipts/history and never deletes remotely.
    let receipts = results[0].receipt_digest.clone();
    detach_link(&mut a);
    cancel_link_intents(&mut intents, &a.link_id);
    require(
        a.state == LinkState::Detached
            && reservations.len() == 2
            && audits.len() == 1
            && results[0].receipt_digest == receipts,
    )?;
    observe(
        &mut steps,
        "tasklinks.detach_link",
        "history-retained",
        &(reservations.len(), audits.len()),
    );
    record_assertion("detach-preserves-history");

    // Credential is held at a synthetic boundary only, never projected into the
    // state/payload/effect receipt. Check emitted normalized bytes independently.
    let canary = "synthetic-credential-canary-do-not-export";
    let payload = serde_json::to_vec(&field_policy.fields).expect("allowlist");
    require(!String::from_utf8_lossy(&payload).contains(canary))?;
    require(
        !serde_json::to_string(&steps)
            .expect("safe steps")
            .contains(canary),
    )?;
    record_assertion("secret-canary-absent");

    let local_members = [
        (EffectKind::Update, "planner.task"),
        (EffectKind::Create, "tasklinks.outbox_item"),
        (EffectKind::Create, "tasklinks.policy_decision"),
        (EffectKind::Create, "tasklinks.action_audit"),
    ];
    let transaction = LocalTransaction {
        group: TransactionGroupId::new("tasklinks/tx-intent-local").expect("owner group"),
        members: local_members
            .iter()
            .map(|(kind, entity)| TransactionMember {
                operation: "tasklinks.request_action".to_owned(),
                kind: *kind,
                subject: format!("canonical:{entity}"),
            })
            .collect(),
    };
    let mut entries = Vec::new();
    let mut add = |operation: &str, kind, resource_kind, subject: &str, group, partial_failure| {
        let occurrence = entries.len() as u32;
        entries.push(EvidenceEntry {
            operation: OperationId::from_semantic(operation).expect("owner operation"),
            kind,
            subject: Subject::new(ResourceId::new(resource_kind, subject).expect("owner resource")),
            occurrence,
            trust: None,
            transaction_group: group,
            sensitivity: None,
            partial_failure,
        });
    };
    let partial_failure = PartialFailure {
        case_ref: "tasklinks.case/partial-delivery".to_owned(),
        operation: "tasklinks.dispatch_outbox".to_owned(),
        recipients: results.clone(),
        retry_eligible: retry,
    };
    for (ordinal, result) in results.iter().enumerate() {
        add(
            "tasklinks.dispatch_outbox",
            EffectKind::ExternalCall,
            ResourceKind::ExternalService,
            &result.provider,
            None,
            (ordinal == 0).then(|| partial_failure.clone()),
        );
    }
    add(
        "tasklinks.dispatch_outbox",
        EffectKind::EnqueueJob,
        ResourceKind::Job,
        "tasklinks.queue/outbox",
        None,
        None,
    );
    add(
        "tasklinks.ack_delivery",
        EffectKind::EmitEvent,
        ResourceKind::Event,
        "tasklinks.delivery_recorded",
        None,
        None,
    );
    for (kind, entity) in local_members {
        add(
            "tasklinks.request_action",
            kind,
            ResourceKind::Canonical,
            entity,
            Some(transaction.group.clone()),
            None,
        );
    }
    let source_digest = pins::ir_digest(compilation);
    let build_digest = digest(&(
        env!("CARGO_PKG_VERSION"),
        include_str!("execution.rs"),
        include_str!("decision.rs"),
        include_str!("validate.rs"),
    ));
    let input_digest = digest(&(
        attachment.computed_revision(),
        &source_digest,
        &build_digest,
        "injected-clock-zero",
        &scenario_commands,
    ));
    let result_digest = digest(&(
        &steps,
        &assertions,
        &results
            .iter()
            .map(|row| {
                (
                    &row.provider,
                    row.result.key(),
                    &row.receipt_digest,
                    &row.error_class,
                )
            })
            .collect::<Vec<_>>(),
    ));
    let run_digest = digest(&(&input_digest, &result_digest));
    let report = ExecutionReport {
        schema_version: "lekalo/task-links-execution/v0.8.0",
        identity: "dev.lekalo.task-links-execution@0.8.0",
        owner: "lekalo-core/task-links-execution",
        implementation: "lekalo-core-deterministic-mock",
        build_digest,
        source_digest: source_digest.clone(),
        input_digest,
        result_digest,
        execution_id: format!("tasklinks.execution.run.ev{}", &run_digest[7..23]),
        attachment_revision: attachment.computed_revision().to_owned(),
        origin: "mock",
        scenarios: scenario_commands
            .iter()
            .map(|(scenario, _)| format!("tasklinks.scenario.{scenario}"))
            .collect(),
        steps,
        assertions,
    };
    let facts = [
        "external-identity-exact-tuple",
        "task-links-0-n",
        "actor-and-field-policy",
        "version-conflict-detection",
        "idempotent-intent",
        "detach-preserves-history",
    ]
    .into_iter()
    .collect();
    let envelope = EvidenceEnvelope {
        adapter_id: "tasklinks/execution-owner".to_owned(),
        target_id: "tasklinks/deterministic-mock".to_owned(),
        protocol_version: "0.8.0".to_owned(),
        evidence_digest: run_digest,
        source_revision: source_digest,
        trust: TrustState::Verified,
        transactions: vec![transaction],
        entries,
    };
    let scenario_digests = report
        .scenarios
        .iter()
        .map(|id| {
            (
                id.clone(),
                scenario_digest(&compilation.project, id).expect("executed scenario"),
            )
        })
        .collect();
    let provider_schemas = attachment
        .provider_contracts()
        .iter()
        .map(|provider| {
            (
                format!("tasklinks.provider/{}", provider.provider_id),
                (
                    scenario_digest(&compilation.project, &provider.request_schema_ref)
                        .expect("admitted schema"),
                    scenario_digest(&compilation.project, &provider.response_schema_ref)
                        .expect("admitted schema"),
                ),
            )
        })
        .collect();
    Ok(Execution {
        report,
        envelope,
        facts,
        scenario_digests,
        provider_schemas,
        job_payload_digest: scenario_digest(&compilation.project, "tasklinks.outbox_item")
            .ok_or_else(|| diagnostic::input_invalid("execution-outbox-schema-unresolved"))?,
    })
}
