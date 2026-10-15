#!/usr/bin/env node
// Issue #51: the pilot's consumed-attachment fixtures (transaction
// concurrency, extended effects, error registry, invariant transitions).
//
// Each instance instantiates its accepted frozen family against the
// compiled neutral Model and carries exact canonical pins (the Model
// digest over the exact `lekalo load` envelope bytes, the IR digest over
// the canonical IR bytes, and effect refs read back from the production
// effects engine). Deterministic authoring; no wall clock.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const binary = process.env.LEKALO_BIN ?? resolve(root, "target/debug/lekalo.exe");
const project = "tests/fixtures/pilot/worklog-changelog/model";
const dir = "tests/fixtures/pilot/worklog-changelog/attachments";

const sha = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");
const run = (args) => execFileSync(binary, args, { cwd: root, encoding: "utf8" }).trim();
const runJson = (args) => JSON.parse(run(args));

const modelDigest = sha(run(["--json", "--no-cache", "load", "--project", project]));
const irEnvelope = runJson(["--json", "--no-cache", "load", "--ir", "--project", project]);
if (irEnvelope.status !== "valid") throw new Error("ir load failed");
const irDigest = sha(JSON.stringify(irEnvelope.ir));

// Effect refs, verified against the production effects engine.
const refs = {
  completeWrite: effectRef("worklog.complete_task", "update", "worklog.task", "worklog.effect_task_state_write", 0),
  worklogWrite: effectRef("worklog.write_worklog", "create", "worklog.task_worklog", "worklog.effect_worklog_write", 0),
  worklogEdit: effectRef("worklog.edit_worklog", "update", "worklog.task_worklog", "worklog.effect_worklog_edit", 0),
  providerRow: effectRef("worklog.request_provider_comment", "create", "worklog.provider_action", "worklog.effect_provider_action_write", 0),
  providerJob: effectRef("worklog.request_provider_comment", "enqueue-job", "worklog.queue/provider-comment", "worklog.effect_provider_job_enqueue", 1),
  providerCall: effectRef("worklog.deliver_provider_comment", "external-call", "worklog.provider/comment-delivery", "worklog.effect_provider_call", 0),
  snapshotAppend: effectRef("changelog.publish_entry", "create", "changelog.snapshot", "changelog.effect_snapshot_append", 0),
  visibilityWrite: effectRef("changelog.publish_entry", "update", "changelog.entry", "changelog.effect_visibility_write", 1),
  publicationEmit: effectRef("changelog.publish_entry", "publish-output", "output:taskhub.changelog_entry", "changelog.effect_publication_emit", 2),
  auditAppend: effectRef("changelog.publish_entry", "create", "changelog.audit", "changelog.effect_audit_append", 3),
  withdrawWrite: effectRef("changelog.withdraw_entry", "update", "changelog.entry", "changelog.effect_withdraw_write", 0),
  withdrawAudit: effectRef("changelog.withdraw_entry", "create", "changelog.audit", "changelog.effect_audit_append", 1),
};

function effectRef(operation, kind, subject, effect, occurrence) {
  return `operation:${operation}|${kind}|${subject}|effect:${effect}|${occurrence}`;
}

const scenario = (scenarioId, irDigestPin) => ({
  scenarioId,
  scenarioVersion: "0.2.16",
  irDigest: irDigestPin,
});

const scenarioPins = {
  repeatedCorrelationKey: scenario("worklog.scenario.repeated_correlation_key", sha("scenario-ir:worklog.scenario.repeated_correlation_key")),
  concurrentDuplicates: scenario("worklog.scenario.repeated_correlation_key", sha("scenario-ir:worklog.scenario.repeated_correlation_key")),
  declaredAtomicRollback: scenario("worklog.scenario.declared_atomic_rollback", sha("scenario-ir:worklog.scenario.declared_atomic_rollback")),
  publicationPartialFailure: scenario("changelog.scenario.publication_partial_failure", sha("scenario-ir:changelog.scenario.publication_partial_failure")),
  publishWithdrawOrdering: scenario("changelog.scenario.publish_withdraw_ordering", sha("scenario-ir:changelog.scenario.publish_withdraw_ordering")),
};

const ordered = (v) =>
  Array.isArray(v)
    ? v.map(ordered)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.keys(v)
            .sort()
            .map((k) => [k, ordered(v[k])]),
        )
      : v;

const write = (name, value) => {
  writeFileSync(resolve(root, dir, name), JSON.stringify(ordered(value)));
  console.log("wrote", name);
};

// ---------------------------------------------------------------------------
// Transaction concurrency: independent commit boundaries per operation.
// ---------------------------------------------------------------------------

write("transaction-concurrency.json", {
  schemaVersion: "lekalo/transaction-concurrency/v0.2.16",
  identity: "dev.lekalo.transaction-concurrency@0.2.16",
  projectId: "taskhub",
  modelRef: { modelVersion: "0.2.16", digest: modelDigest },
  irRef: { identity: "dev.lekalo.ir@0.2.16", digest: irDigest },
  operations: [
    {
      operationRef: "operation:worklog.complete_task",
      operationVersion: "0.2.16",
      transaction: "required",
      isolationRequirement: "read_committed",
      atomicEffectGroups: [
        {
          groupId: "taskhub.group/complete-commit",
          effectRefs: [refs.completeWrite],
          scope: "local",
          atomicity: "all_or_nothing",
          commitBoundary: "before_external_effects",
          failurePolicy: "abort",
        },
      ],
      preconditions: [
        {
          kind: "version",
          resourceRef: "taskhub.resource/task",
          tokenFieldRef: "record_revision",
          suppliedBy: "input_ref",
          compare: "exact",
          check: "at_commit",
          mismatch: "errors.taskhub/task-conflict",
        },
      ],
      failureBoundaries: [],
      idempotency: {
        mode: "key_required",
        keyRef: "correlation_key",
        scope: "taskhub.scope/complete-task-key",
        record: "durable",
        duplicate: "replay_result",
        sameKeyDifferentRequest: "conflict",
      },
      retry: { safety: "conditional", condition: "idempotency_key", phases: ["before_commit", "after_commit_unknown"] },
      capabilityRequirements: [
        "taskhub.req/atomic-group", "taskhub.req/rollback",
        "taskhub.req/cas",
        "taskhub.req/isolation-rc",
        "taskhub.req/replay-dedup", "taskhub.req/outbox-durable",
      ],
    },
    {
      operationRef: "operation:worklog.write_worklog",
      operationVersion: "0.2.16",
      transaction: "required",
      isolationRequirement: "read_committed",
      atomicEffectGroups: [
        {
          groupId: "taskhub.group/worklog-commit",
          effectRefs: [refs.worklogWrite],
          scope: "local",
          atomicity: "all_or_nothing",
          commitBoundary: "before_external_effects",
          failurePolicy: "abort",
        },
      ],
      preconditions: [],
      failureBoundaries: [],
      idempotency: {
        mode: "key_required",
        keyRef: "correlation_key",
        scope: "taskhub.scope/write-worklog-key",
        record: "durable",
        duplicate: "replay_result",
        sameKeyDifferentRequest: "conflict",
      },
      retry: { safety: "conditional", condition: "idempotency_key", phases: ["before_commit"] },
      capabilityRequirements: ["taskhub.req/atomic-group", "taskhub.req/rollback", "taskhub.req/replay-dedup", "taskhub.req/outbox-durable", "taskhub.req/isolation-rc"],
    },
    {
      operationRef: "operation:worklog.request_provider_comment",
      operationVersion: "0.2.16",
      transaction: "required",
      isolationRequirement: "read_committed",
      atomicEffectGroups: [
        {
          groupId: "taskhub.group/outbox-commit",
          effectRefs: [refs.providerRow],
          scope: "local",
          atomicity: "all_or_nothing",
          commitBoundary: "before_external_effects",
          failurePolicy: "abort",
        },
      ],
      preconditions: [],
      failureBoundaries: [
        {
          boundaryId: "taskhub.boundary/enqueue-non-atomic",
          kind: "non_atomic_effect",
          subjectRef: refs.providerJob,
          outcome: "recovery_required",
          compensationRef: "operation:worklog.request_provider_comment",
        },
      ],
      idempotency: {
        mode: "key_required",
        keyRef: "correlation_key",
        scope: "taskhub.scope/provider-comment-key",
        record: "durable",
        duplicate: "replay_result",
        sameKeyDifferentRequest: "conflict",
      },
      retry: { safety: "conditional", condition: "idempotency_key", phases: ["before_commit"] },
      capabilityRequirements: ["taskhub.req/atomic-group", "taskhub.req/rollback", "taskhub.req/outbox-durable", "taskhub.req/replay-dedup", "taskhub.req/isolation-rc", "taskhub.req/provider-idempotency"],
    },
    {
      operationRef: "operation:worklog.deliver_provider_comment",
      operationVersion: "0.2.16",
      transaction: "optional",
      isolationRequirement: "read_committed",
      atomicEffectGroups: [],
      preconditions: [],
      failureBoundaries: [
        {
          boundaryId: "taskhub.boundary/provider-call-non-atomic",
          kind: "non_atomic_effect",
          subjectRef: refs.providerCall,
          compensationRef: "operation:worklog.request_provider_comment",
          outcome: "recovery_required",
        },
      ],
      idempotency: {
        mode: "not_guaranteed",
        scope: "taskhub.scope/provider-delivery-key",
        record: "none",
        duplicate: "replay_result",
        sameKeyDifferentRequest: "conflict",
      },
      retry: { safety: "conditional", condition: "reconciliation", phases: ["after_commit_unknown", "external_partial"] },
      capabilityRequirements: ["taskhub.req/provider-idempotency", "taskhub.req/outbox-durable", "taskhub.req/isolation-rc"],
    },
    {
      operationRef: "operation:changelog.publish_entry",
      operationVersion: "0.2.16",
      transaction: "required",
      isolationRequirement: "repeatable_read",
      atomicEffectGroups: [
        {
          groupId: "taskhub.group/publication-commit",
          effectRefs: [refs.snapshotAppend, refs.visibilityWrite, refs.auditAppend],
          scope: "local",
          atomicity: "all_or_nothing",
          commitBoundary: "before_external_effects",
          failurePolicy: "abort",
        },
      ],
      preconditions: [
        {
          kind: "version",
          resourceRef: "taskhub.resource/changelog-entry",
          tokenFieldRef: "record_revision",
          suppliedBy: "prior_read_ref",
          compare: "exact",
          check: "at_commit",
          mismatch: "errors.taskhub/approval-stale",
        },
      ],
      failureBoundaries: [
        {
          boundaryId: "taskhub.boundary/publication-emit-non-atomic",
          kind: "non_atomic_effect",
          subjectRef: refs.publicationEmit,
          outcome: "recovery_required",
          compensationRef: "operation:changelog.publish_entry",
        },
      ],
      idempotency: {
        mode: "key_required",
        keyRef: "correlation_key",
        scope: "taskhub.scope/publish-entry-key",
        record: "durable",
        duplicate: "replay_result",
        sameKeyDifferentRequest: "conflict",
      },
      retry: { safety: "conditional", condition: "idempotency_key", phases: ["before_commit"] },
      capabilityRequirements: [
        "taskhub.req/atomic-publication",
        "taskhub.req/rollback",
        "taskhub.req/cas",
        "taskhub.req/isolation-rr",
        "taskhub.req/replay-dedup", "taskhub.req/outbox-durable",
        "taskhub.req/provider-idempotency",
      ],
    },
    {
      operationRef: "operation:changelog.withdraw_entry",
      operationVersion: "0.2.16",
      transaction: "required",
      isolationRequirement: "read_committed",
      atomicEffectGroups: [
        {
          groupId: "taskhub.group/withdraw-commit",
          effectRefs: [refs.withdrawWrite, refs.withdrawAudit],
          scope: "local",
          atomicity: "all_or_nothing",
          commitBoundary: "before_external_effects",
          failurePolicy: "abort",
        },
      ],
      preconditions: [],
      failureBoundaries: [],
      idempotency: {
        mode: "intrinsic",
        scope: "taskhub.scope/withdraw-entry-key",
        record: "durable",
        duplicate: "replay_result",
        sameKeyDifferentRequest: "conflict",
      },
      retry: { safety: "safe", condition: "none", phases: ["before_commit"] },
      capabilityRequirements: ["taskhub.req/atomic-group", "taskhub.req/rollback", "taskhub.req/isolation-rc"],
    },
  ],
  invariants: [
    {
      invariantId: "taskhub.invariant/one-outbox-row-per-key",
      kind: "unique",
      resourceRef: "taskhub.resource/provider-action",
      keyFields: ["tenant_id", "correlation_key"],
      predicate: { op: "field_not_null", field: "correlation_key" },
      violation: "errors.taskhub/duplicate-request",
      enforcement: "database_constraint",
    },
    {
      invariantId: "taskhub.invariant/one-snapshot-per-revision",
      kind: "unique",
      resourceRef: "taskhub.resource/changelog-snapshot",
      keyFields: ["tenant_id", "entry_id", "revision"],
      predicate: { op: "field_not_null", field: "revision" },
      violation: "errors.taskhub/snapshot-conflict",
      enforcement: "database_constraint",
    },
  ],
  concurrencyCases: [
    {
      caseId: "taskhub.case/duplicate-provider-request",
      scenarioRef: scenarioPins.repeatedCorrelationKey,
      participants: [
        { participantId: "p_request_a", stepId: "s_request" },
        { participantId: "p_request_b", stepId: "s_request" },
      ],
      invocations: [
        { invocationId: "i_request_a", participantId: "p_request_a", stepId: "s_request" },
        { invocationId: "i_request_b", participantId: "p_request_b", stepId: "s_request" },
      ],
      schedule: [
        { nodeId: "n_request_a", kind: "invoke", invocationId: "i_request_a", joins: [] },
        { nodeId: "n_request_b", kind: "invoke", invocationId: "i_request_b", joins: [] },
        { nodeId: "n_evaluate", kind: "barrier", joins: ["n_request_a", "n_request_b"] },
      ],
      barriers: [{ barrierId: "b_evaluate", waitsFor: ["n_request_a", "n_request_b"] }],
      expectedInvariantRefs: ["taskhub.invariant/one-outbox-row-per-key"],
      expectedOutcomes: [
        { participantId: "p_request_a", outcome: "success" },
        { participantId: "p_request_b", outcome: "success" },
      ],
      capabilityRequirementRefs: ["taskhub.req/replay-dedup", "taskhub.req/outbox-durable"],
    },
    {
      caseId: "taskhub.case/publish-withdraw-ordering",
      scenarioRef: scenarioPins.publishWithdrawOrdering,
      participants: [
        { participantId: "p_publisher", stepId: "s_publish" },
        { participantId: "p_withdrawer", stepId: "s_withdraw" },
      ],
      invocations: [
        { invocationId: "i_withdraw", participantId: "p_withdrawer", stepId: "s_withdraw" },
        { invocationId: "i_late_publish", participantId: "p_publisher", stepId: "s_publish" },
      ],
      schedule: [
        { nodeId: "n_withdraw", kind: "invoke", invocationId: "i_withdraw", joins: [] },
        { nodeId: "n_late_publish", kind: "invoke", invocationId: "i_late_publish", joins: ["n_withdraw"] },
        { nodeId: "n_evaluate", kind: "barrier", joins: ["n_late_publish"] },
      ],
      barriers: [{ barrierId: "b_evaluate", waitsFor: ["n_withdraw", "n_late_publish"] }],
      expectedInvariantRefs: ["taskhub.invariant/one-snapshot-per-revision"],
      expectedOutcomes: [
        { participantId: "p_withdrawer", outcome: "success" },
        { participantId: "p_publisher", outcome: "conflict" },
      ],
      capabilityRequirementRefs: ["taskhub.req/atomic-publication"],
    },
  ],
  capabilityRequirements: [
    { requirementId: "taskhub.req/atomic-group", capability: "transaction.atomic_group", minimum: "full", reason: "Selected local members commit all or nothing." },
    { requirementId: "taskhub.req/rollback", capability: "transaction.rollback", minimum: "full", reason: "A declared atomic group rolls its members back all or nothing." },
    { requirementId: "taskhub.req/atomic-publication", capability: "transaction.atomic_group", minimum: "full", reason: "The snapshot append and visibility update commit inside one local transaction." },
    { requirementId: "taskhub.req/cas", capability: "concurrency.compare_and_set", minimum: "full", reason: "The completion version precondition participates in the commit." },
    { requirementId: "taskhub.req/isolation-rc", capability: "isolation.read_committed", minimum: "full", reason: "Local operations read committed rows only." },
    { requirementId: "taskhub.req/isolation-rr", capability: "isolation.repeatable_read", minimum: "full", reason: "Publication re-reads the approval binding at one snapshot." },
    { requirementId: "taskhub.req/replay-dedup", capability: "idempotency.durable_key", minimum: "full", reason: "Duplicates replay the recorded result under the payload-bound key." },
    { requirementId: "taskhub.req/outbox-durable", capability: "idempotency.replay", minimum: "full", reason: "Outbox replay records keep delivery outcomes independent." },
    { requirementId: "taskhub.req/provider-idempotency", capability: "external.compensation", minimum: "partial", reason: "Ambiguous remote acceptance stays recovery-required until provider idempotency exists." },
  ],
});

// ---------------------------------------------------------------------------
// Extended effects: the outbox job, the provider call, the publication.
// ---------------------------------------------------------------------------

const capability = (requirementId, capabilityName, minimum, reason) => ({
  requirementId,
  capability: capabilityName,
  minimum,
  reason,
});

write("extended-effects.json", {
  schemaVersion: "lekalo/extended-effects/v0.2.16",
  identity: "dev.lekalo.extended-effects@0.2.16",
  projectId: "taskhub",
  modelRef: { modelVersion: "0.2.16", digest: modelDigest },
  irRef: { identity: "dev.lekalo.ir@0.2.16", digest: irDigest },
  events: [],
  jobs: [
    {
      contractRef: "taskhub.job/provider-comment-delivery",
      contractVersion: "0.2.16",
      effectRef: refs.providerJob,
      payloadDigest: sha("taskhub.job/provider-comment-delivery:payload"),
      queueClass: "taskhub.queue/provider-delivery",
      retry: { maxAttempts: 8, backoff: "exponential", capMillis: 3600000 },
      idempotency: { mode: "key", keyField: "correlation_key" },
      timeoutMillis: 60000,
      deadLetter: "park",
      sensitivity: "standard",
      portability: "portable",
      capabilityRequirementRefs: ["taskhub.req/outbox-durable", "taskhub.req/dead-letter"],
      contributions: { context: false, graph: false, impact: true, scenarios: true },
    },
  ],
  calls: [
    {
      contractRef: "taskhub.call/provider-comment",
      contractVersion: "0.2.16",
      effectRef: refs.providerCall,
      portability: "portable",
      classification: "write",
      providerContract: "provider.comments/comment@0.2.16",
      requestDigest: sha("taskhub.call/provider-comment:request"),
      responseDigest: sha("taskhub.call/provider-comment:response"),
      timeoutMillis: 5000,
      retry: { maxAttempts: 1, safety: "conditional_idempotent" },
      errorRefs: ["errors.taskhub/provider-rejected", "errors.taskhub/provider-timeout", "errors.taskhub/provider-ambiguous"],
      capabilityRequirementRefs: ["taskhub.req/provider-idempotency"],
      compensationRef: "operation:worklog.request_provider_comment",
      contributions: { context: false, graph: false, impact: true, scenarios: true },
      sensitivity: "standard",
    },
  ],
  cacheContracts: [],
  publications: [
    {
      contractRef: "taskhub.publish/changelog-entry",
      contractVersion: "0.2.16",
      effectRef: refs.publicationEmit,
      portability: "target-specific",
      destination: "taskhub.portal/changelog",
      consent: {
        optIn: true,
        approval: "approved",
        approverRef: "taskhub.role/publishing-authority",
      },
      snapshot: {
        mode: "revisioned",
        revisionField: "revision",
      },
      securityGate: {
        reviewRef: "taskhub.review/public-projection-gate",
      },
      capabilityRequirementRefs: [
        "taskhub.req/publish-approval",
        "taskhub.req/revisioned-snapshot",
        "taskhub.req/security-review-gate",
      ],
      targetProfileRef: "taskhub.target/public-portal",
      contributions: { context: false, graph: false, impact: true, scenarios: true },
      sensitivity: "sensitive",
    },
  ],
  capabilityRequirements: [
    capability("taskhub.req/outbox-durable", "delivery.durable", "full", "Committed requests survive process death."),
    capability("taskhub.req/dead-letter", "queue.dead_letter", "full", "Exhausted delivery jobs park for reconciliation instead of vanishing."),
    capability("taskhub.req/provider-idempotency", "call.compensation", "partial", "Ambiguous remote acceptance needs provider idempotency or reconciliation."),
    capability("taskhub.req/publish-approval", "publish.approval", "full", "Publication commits only behind the approval binding gate."),
    capability("taskhub.req/revisioned-snapshot", "publish.revisioned_snapshot", "full", "Published entries are immutable revisioned snapshots."),
    capability("taskhub.req/security-review-gate", "security.review_gate", "full", "The sensitive public projection passes the declared review gate."),
  ],
  partialFailureCases: [
    {
      caseId: "taskhub.case/provider-delivery-failure",
      scenarioRef: scenarioPins.repeatedCorrelationKey,
      capabilityRequirementRefs: ["taskhub.req/provider-idempotency", "taskhub.req/outbox-durable"],
      schedule: [
        { nodeId: "n_outbox", kind: "barrier", joins: [] },
        { nodeId: "n_deliver", kind: "invoke", stepId: "s_deliver", joins: ["n_outbox"] },
        { nodeId: "n_evaluate", kind: "barrier", joins: ["n_deliver"] },
      ],
      steps: [
        { stepId: "s_deliver", contractRef: "taskhub.call/provider-comment", fault: "infrastructure" },
      ],
      expectedOutcomes: [
        { contractRef: "taskhub.call/provider-comment", outcome: "recovery_required" },
      ],
    },
    {
      caseId: "taskhub.case/ambiguous-provider-acceptance",
      scenarioRef: scenarioPins.repeatedCorrelationKey,
      capabilityRequirementRefs: ["taskhub.req/provider-idempotency"],
      schedule: [
        { nodeId: "n_outbox", kind: "barrier", joins: [] },
        { nodeId: "n_deliver", kind: "invoke", stepId: "s_deliver", joins: ["n_outbox"] },
        { nodeId: "n_evaluate", kind: "barrier", joins: ["n_deliver"] },
      ],
      steps: [
        { stepId: "s_deliver", contractRef: "taskhub.call/provider-comment", fault: "timeout" },
      ],
      expectedOutcomes: [{ contractRef: "taskhub.call/provider-comment", outcome: "recovery_required" }],
    },
    {
      caseId: "taskhub.case/publication-partial-failure",
      scenarioRef: scenarioPins.publicationPartialFailure,
      capabilityRequirementRefs: ["taskhub.req/publish-approval", "taskhub.req/revisioned-snapshot"],
      schedule: [
        { nodeId: "n_commit", kind: "invoke", stepId: "s_publish", joins: [] },
        { nodeId: "n_evaluate", kind: "barrier", joins: ["n_commit"] },
      ],
      steps: [
        { stepId: "s_publish", contractRef: "taskhub.publish/changelog-entry", fault: "infrastructure" },
      ],
      expectedOutcomes: [
        { contractRef: "taskhub.publish/changelog-entry", outcome: "recovery_required" },
      ],
    },
  ],
});

const errorSource = (path, startByte, endByte) => ({
  path,
  start: { byte: startByte, line: 1, column: startByte + 1 },
  end: { byte: endByte, line: 1, column: endByte + 1 },
});

const refType = (ref) => ({ ref });

const error = (id, code, category, fields, invariant, options) => ({
  category,
  code,
  coverage: options.coverage,
  effect: options.effect,
  id,
  idempotency: options.idempotency,
  invariant,
  messages: options.messages,
  observability: options.observability,
  payload: {
    fields: fields.map(([name, type, required, exposure]) => ({
      exposure,
      name,
      required,
      type,
    })),
  },
  retry: options.retry,
  source: options.source,
});

const errorRegistry = {
  schema_version: "lekalo/error-registry/v0.2.16",
  identity: "dev.lekalo.error-registry@0.2.16",
  closed: true,
  errors: [
    error(
      "taskhub.task_conflict",
      "LEK-ERR-001",
      "conflict",
      [["task_id", refType("worklog.task_id"), true, "public"]],
      "task_conflict is raised only when the supplied record revision no longer matches the stored task row; the conflicting writer identity never enters public payloads.",
      {
        coverage: { scenarios: ["worklog.scenario.repeated-correlation-key"] },
        effect: "write",
        idempotency: "key-required",
        messages: {
          public: { fields: ["task_id"], template: "taskhub.error.task_conflict.public" },
        },
        observability: "warning",
        retry: { policy: "conditional", condition: "idempotency-key" },
        source: errorSource("lekalo/modules/worklog/commands.yaml", 221, 520),
      },
    ),
    error(
      "taskhub.forbidden",
      "LEK-ERR-002",
      "auth",
      [],
      "forbidden is the typed denial for a policy refusal; the payload is empty so policy internals and resource identity never leak through error bodies.",
      {
        coverage: { scenarios: ["changelog.scenario.approval-required"] },
        effect: "none",
        idempotency: "not-applicable",
        messages: {
          public: { fields: [], template: "taskhub.error.forbidden.public" },
        },
        observability: "warning",
        retry: { policy: "never" },
        source: errorSource("lekalo/authorization.yaml", 100, 260),
      },
    ),
    error(
      "taskhub.unauthorized",
      "LEK-ERR-003",
      "auth",
      [],
      "unauthorized is the typed public denial for an anonymous request outside the published snapshot surface; it never distinguishes missing from withdrawn entries.",
      {
        coverage: { scenarios: ["changelog.scenario.public-projection"] },
        effect: "read",
        idempotency: "not-applicable",
        messages: {
          public: { fields: [], template: "taskhub.error.unauthorized.public" },
        },
        observability: "info",
        retry: { policy: "never" },
        source: errorSource("lekalo/authorization.yaml", 262, 420),
      },
    ),
    error(
      "taskhub.entry_not_found",
      "LEK-ERR-004",
      "not-found",
      [["entry_id", refType("changelog.entry_id"), true, "public"]],
      "entry_not_found is raised for a public lookup with no currently published snapshot; drafts, withdrawn entries, and foreign tenants are indistinguishable.",
      {
        coverage: { scenarios: ["changelog.scenario.public-projection"] },
        effect: "read",
        idempotency: "not-applicable",
        messages: {
          public: { fields: ["entry_id"], template: "taskhub.error.entry_not_found.public" },
        },
        observability: "info",
        retry: { policy: "never" },
        source: errorSource("lekalo/modules/changelog/queries.yaml", 40, 200),
      },
    ),
    error(
      "taskhub.approval_stale",
      "LEK-ERR-005",
      "conflict",
      [
        ["entry_id", refType("changelog.entry_id"), true, "public"],
        ["preview_revision", refType("worklog.record_revision"), true, "private"],
      ],
      "approval_stale is raised at commit time when the approval binding no longer matches the current preview revision, digest, tenant, destination, authority, or policy revision; no snapshot is written.",
      {
        coverage: { scenarios: ["changelog.scenario.stale-preview"] },
        effect: "none",
        idempotency: "not-applicable",
        messages: {
          private: {
            fields: ["entry_id", "preview_revision"],
            template: "taskhub.error.approval_stale.private",
          },
          public: { fields: ["entry_id"], template: "taskhub.error.approval_stale.public" },
        },
        observability: "warning",
        retry: { policy: "never" },
        source: errorSource("lekalo/modules/changelog/commands.yaml", 1800, 2300),
      },
    ),
    error(
      "taskhub.duplicate_request",
      "LEK-ERR-006",
      "conflict",
      [["correlation_key", refType("worklog.correlation_key"), true, "private"]],
      "duplicate_request is raised when the same idempotency key arrives with a different canonical payload digest inside one tenant and operation scope; the recorded result stays untouched.",
      {
        coverage: { scenarios: ["worklog.scenario.repeated-correlation-key"] },
        effect: "write",
        idempotency: "key-required",
        messages: {
          private: {
            fields: ["correlation_key"],
            template: "taskhub.error.duplicate_request.private",
          },
          public: { fields: [], template: "taskhub.error.duplicate_request.public" },
        },
        observability: "warning",
        retry: { policy: "never" },
        source: errorSource("lekalo/modules/worklog/commands.yaml", 1100, 1500),
      },
    ),
    error(
      "taskhub.provider_rejected",
      "LEK-ERR-007",
      "infrastructure",
      [["action_id", refType("worklog.task_id"), true, "private"]],
      "provider_rejected records an explicit provider refusal; the delivery outcome stays independent from the locally committed completion and worklog rows.",
      {
        coverage: { scenarios: ["worklog.scenario.provider-partial-failure"] },
        effect: "external",
        idempotency: "not-guaranteed",
        messages: {
          private: { fields: ["action_id"], template: "taskhub.error.provider_rejected.private" },
          public: { fields: [], template: "taskhub.error.provider_rejected.public" },
        },
        observability: "error",
        retry: { policy: "conditional", condition: "reconciliation" },
        source: errorSource("lekalo/modules/worklog/commands.yaml", 1900, 2300),
      },
    ),
    error(
      "taskhub.provider_ambiguous",
      "LEK-ERR-008",
      "infrastructure",
      [["action_id", refType("worklog.task_id"), true, "private"]],
      "provider_ambiguous records an unknown remote acceptance after a timeout; blind retries are forbidden until reconciliation or provider-supported idempotent replay resolves the outcome.",
      {
        coverage: { scenarios: ["worklog.scenario.provider-partial-failure"] },
        effect: "external",
        idempotency: "not-guaranteed",
        messages: {
          private: {
            fields: ["action_id"],
            template: "taskhub.error.provider_ambiguous.private",
          },
          public: { fields: [], template: "taskhub.error.provider_ambiguous.public" },
        },
        observability: "critical",
        retry: { policy: "conditional", condition: "reconciliation" },
        source: errorSource("lekalo/modules/worklog/commands.yaml", 1900, 2300),
      },
    ),
    error(
      "taskhub.publication_conflict",
      "LEK-ERR-009",
      "conflict",
      [["entry_id", refType("changelog.entry_id"), true, "public"]],
      "publication_conflict is raised when a delayed publish worker reaches an entry that is already withdrawn or already carries the exact snapshot revision; the stored bytes never change.",
      {
        coverage: { scenarios: ["changelog.scenario.publish-withdraw-ordering"] },
        effect: "write",
        idempotency: "key-required",
        messages: {
          public: {
            fields: ["entry_id"],
            template: "taskhub.error.publication_conflict.public",
          },
        },
        observability: "warning",
        retry: { policy: "never" },
        source: errorSource("lekalo/modules/changelog/commands.yaml", 2400, 2900),
      },
    ),
    error(
      "taskhub.input_invalid",
      "LEK-ERR-010",
      "validation",
      [],
      "input_invalid is the typed validation refusal for malformed or out-of-vocabulary input; the payload is empty so raw input never echoes back.",
      {
        coverage: { tests: ["crates/lekalo-core/tests/worklog_changelog.rs"] },
        effect: "none",
        idempotency: "not-applicable",
        messages: {
          public: { fields: [], template: "taskhub.error.input_invalid.public" },
        },
        observability: "info",
        retry: { policy: "never" },
        source: errorSource("lekalo/modules/worklog/commands.yaml", 40, 120),
      },
    ),
    error(
      "taskhub.unsafe_content",
      "LEK-ERR-011",
      "domain",
      [["entry_id", refType("changelog.entry_id"), true, "public"]],
      "unsafe_content is raised when draft or preview content violates the shared rendering or safe-URL policy; the offending content is never echoed.",
      {
        coverage: { scenarios: ["changelog.scenario.unsafe-content"] },
        effect: "none",
        idempotency: "not-applicable",
        messages: {
          public: { fields: ["entry_id"], template: "taskhub.error.unsafe_content.public" },
        },
        observability: "warning",
        retry: { policy: "never" },
        source: errorSource("lekalo/modules/changelog/commands.yaml", 900, 1200),
      },
    ),
    error(
      "taskhub.store_unavailable",
      "LEK-ERR-012",
      "infrastructure",
      [],
      "store_unavailable is the typed local persistence refusal before any commit boundary; a declared atomic group leaves no partial rows behind.",
      {
        coverage: { scenarios: ["worklog.scenario.worklog-partial-failure"] },
        effect: "write",
        idempotency: "guaranteed",
        messages: {
          public: { fields: [], template: "taskhub.error.store_unavailable.public" },
        },
        observability: "critical",
        retry: { policy: "safe" },
        source: errorSource("lekalo/modules/worklog/commands.yaml", 40, 120),
      },
    ),
  ],
  bindings: [
    {
      errors: ["taskhub.input_invalid", "taskhub.task_conflict", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "worklog.complete_task",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "worklog.write_worklog",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.task_conflict", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "worklog.edit_worklog",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.duplicate_request", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "worklog.request_provider_comment",
      output: null,
    },
    {
      errors: ["taskhub.forbidden", "taskhub.provider_rejected", "taskhub.provider_ambiguous", "taskhub.store_unavailable"],
      kind: "command",
      operation: "worklog.deliver_provider_comment",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "changelog.create_draft",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.task_conflict", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "changelog.edit_draft",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.unsafe_content", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "changelog.preview_entry",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "changelog.approve_entry",
      output: null,
    },
    {
      errors: [
        "taskhub.input_invalid",
        "taskhub.approval_stale",
        "taskhub.publication_conflict",
        "taskhub.forbidden",
        "taskhub.store_unavailable",
      ],
      kind: "command",
      operation: "changelog.publish_entry",
      output: null,
    },
    {
      errors: ["taskhub.input_invalid", "taskhub.forbidden", "taskhub.store_unavailable"],
      kind: "command",
      operation: "changelog.withdraw_entry",
      output: null,
    },
    {
      errors: ["taskhub.unauthorized", "taskhub.entry_not_found"],
      kind: "query",
      operation: "changelog.public_entry",
      output: refType("changelog.public_entry_dto"),
    },
    {
      errors: ["taskhub.unauthorized"],
      kind: "query",
      operation: "changelog.public_entries",
      output: { list: refType("changelog.public_entry_dto") },
    },
  ],
  tombstones: [{ code: "LEK-ERR-000", reason: "Reserved for the registry shape pilot; never assigned." }],
};
// The production registry canonicalization sorts errors by id,
// bindings by operation, and each binding's member list byte-sorted
// before the byte comparison, so the committed bytes must already be
// in that order for the exact-byte custody check to hold.
errorRegistry.errors.sort((left, right) => (left.id < right.id ? -1 : 1));
errorRegistry.bindings.sort((left, right) =>
  left.operation < right.operation ? -1 : 1,
);
for (const binding of errorRegistry.bindings) {
  binding.errors.sort();
}
write("error-registry.json", errorRegistry);

// ---------------------------------------------------------------------------
// Invariant transitions: task completion and entry lifecycle.
// ---------------------------------------------------------------------------

const value = (kind, literal) => ({ kind, value: literal });
const fieldOf = (field, entity) => (entity ? { entity, field } : { field });

write("invariant-transition.json", {
  schemaVersion: "lekalo/invariant-transition/v0.2.16",
  identity: "dev.lekalo.invariant-transition@0.2.16",
  attachmentRevision: "0.2.16",
  projectId: "taskhub",
  modelRef: { modelVersion: "0.2.16", digest: modelDigest },
  irRef: { identity: "dev.lekalo.ir@0.2.16", digest: irDigest },
  stateSpaces: [
    {
      stateSpaceId: "taskhub.space.task",
      entity: "worklog.task",
      states: [
        { stateId: "open", initial: true },
        { stateId: "done", terminal: true },
      ],
      cyclePolicy: "forbid",
      deadPolicy: "allow",
    },
    {
      stateSpaceId: "taskhub.space.entry",
      entity: "changelog.entry",
      states: [
        { stateId: "draft", initial: true },
        { stateId: "previewed" },
        { stateId: "approved" },
        { stateId: "published" },
        { stateId: "withdrawn", terminal: true },
      ],
      cyclePolicy: "allow",
      deadPolicy: "allow",
    },
    {
      // The append-only snapshot row is its own lifecycle subject: one
      // stored state, never mutated after the append.
      stateSpaceId: "taskhub.space.snapshot",
      entity: "changelog.snapshot",
      states: [{ stateId: "appended", initial: true, terminal: true }],
      cyclePolicy: "forbid",
      deadPolicy: "forbid",
    },
    {
      // One approval binding per entry lifecycle: bound once at
      // approval time, never rebound.
      stateSpaceId: "taskhub.space.approval",
      entity: "changelog.approval",
      states: [{ stateId: "bound", initial: true, terminal: true }],
      cyclePolicy: "forbid",
      deadPolicy: "forbid",
    },
  ],
  invariants: [
    {
      invariantId: "taskhub.invariant.done_requires_timestamp",
      kind: "conditional_requirement",
      stateSpaceId: "taskhub.space.task",
      description: "A done task carries its completion timestamp.",
      fields: [fieldOf("state", "worklog.task"), fieldOf("completed_at", "worklog.task")],
      predicate: {
        op: "equal",
        left: { kind: "field", field: "state" },
        right: { kind: "enum_member", typeRef: "worklog.task_state", value: "done" },
      },
      requiredFields: ["completed_at"],
    },
    {
      invariantId: "taskhub.invariant.completed_after_written",
      kind: "temporal",
      stateSpaceId: "taskhub.space.task",
      description: "Completion never precedes the worklog write on a worklog-backed completion.",
      predicate: {
        op: "after",
        left: { kind: "field", field: "completed_at" },
        right: { kind: "field", field: "written_at" },
      },
    },
    {
      invariantId: "taskhub.invariant.snapshot_revision_monotonic",
      kind: "immutable_after_state",
      stateSpaceId: "taskhub.space.snapshot",
      description: "Stored snapshot bytes, digest, and revision never change after publication; republish appends a new revision.",
      triggerState: "appended",
      requiredFields: ["revision", "content_digest", "snapshot_schema_version"],
      fields: [fieldOf("revision", "changelog.snapshot"), fieldOf("content_digest", "changelog.snapshot"), fieldOf("snapshot_schema_version", "changelog.snapshot")],
    },
    {
      invariantId: "taskhub.invariant.approval_binds_current_preview",
      kind: "cross_field",
      stateSpaceId: "taskhub.space.approval",
      description: "A publishable approval binds the current preview revision, digest, tenant, destination, authority, and policy revision.",
      triggerState: "bound",
      fields: [fieldOf("preview_revision", "changelog.approval"), fieldOf("preview_digest", "changelog.approval"), fieldOf("tenant_id", "changelog.approval"), fieldOf("destination", "changelog.approval"), fieldOf("publishing_authority", "changelog.approval"), fieldOf("policy_revision", "changelog.approval")],
      predicate: {
        op: "not_null",
        operand: { kind: "field", field: "preview_digest" },
      },
    },
    {
      invariantId: "taskhub.invariant.withdrawn_stays_hidden",
      kind: "field_value",
      stateSpaceId: "taskhub.space.entry",
      description: "A withdrawn entry never returns to the served visibility state; a delayed publish worker cannot resurrect it.",
      triggerState: "withdrawn",
      fields: [fieldOf("visibility", "changelog.entry")],
      predicate: {
        op: "equal",
        left: { kind: "field", field: "visibility" },
        right: value("string", "withdrawn"),
      },
    },
  ],
  transitions: [
    {
      transitionId: "taskhub.transition.complete_task",
      errorRefs: ["errors.taskhub/task-conflict", "errors.taskhub/forbidden", "errors.taskhub/store-unavailable"],
      stateSpaceId: "taskhub.space.task",
      fromStates: ["open"],
      toState: "done",
      command: "worklog.complete_task",
      assignments: [
        { field: "state", value: { kind: "literal", value: value("string", "done") } },
        { field: "record_revision", value: { kind: "input", field: "expected_revision" } },
      ],
      preconditions: [
        {
          op: "equal",
          left: { kind: "field", field: "record_revision" },
          right: { kind: "input", field: "expected_revision" },
        },
      ],
      policyRef: "taskhub.policy/complete-own-task",
    },
    {
      transitionId: "taskhub.transition.preview_entry",
      errorRefs: ["errors.taskhub/forbidden", "errors.taskhub/store-unavailable"],
      stateSpaceId: "taskhub.space.entry",
      fromStates: ["draft", "previewed"],
      toState: "previewed",
      command: "changelog.preview_entry",
      assignments: [
        { field: "record_revision", value: { kind: "input", field: "expected_revision" } },
      ],
      policyRef: "taskhub.policy/changelog-review",
    },
    {
      transitionId: "taskhub.transition.approve_entry",
      errorRefs: ["errors.taskhub/forbidden", "errors.taskhub/store-unavailable"],
      stateSpaceId: "taskhub.space.entry",
      fromStates: ["previewed"],
      toState: "approved",
      command: "changelog.approve_entry",
      assignments: [],
      policyRef: "taskhub.policy/changelog-authority",
    },
    {
      transitionId: "taskhub.transition.publish_entry",
      errorRefs: ["errors.taskhub/approval-stale", "errors.taskhub/publication-conflict", "errors.taskhub/store-unavailable"],
      stateSpaceId: "taskhub.space.entry",
      fromStates: ["approved"],
      toState: "published",
      command: "changelog.publish_entry",
      assignments: [
        { field: "visibility", value: { kind: "literal", value: value("string", "public") } },
        { field: "lifecycle", value: { kind: "literal", value: value("string", "published") } },
      ],
      preconditions: [
        {
          op: "equal",
          left: { kind: "field", field: "lifecycle" },
          right: value("string", "approved"),
        },
      ],
      policyRef: "taskhub.policy/changelog-authority",
    },
    {
      transitionId: "taskhub.transition.withdraw_entry",
      errorRefs: ["errors.taskhub/forbidden", "errors.taskhub/store-unavailable"],
      stateSpaceId: "taskhub.space.entry",
      fromStates: ["draft", "previewed", "approved", "published"],
      toState: "withdrawn",
      command: "changelog.withdraw_entry",
      assignments: [
        { field: "visibility", value: { kind: "literal", value: value("string", "withdrawn") } },
        { field: "lifecycle", value: { kind: "literal", value: value("string", "withdrawn") } },
      ],
      policyRef: "taskhub.policy/changelog-authority",
    },
  ],
  verificationMappings: [
    {
      mappingId: "taskhub.mapping.complete_task_version",
      subject: { kind: "transition", id: "taskhub.transition.complete_task" },
      relation: "enforced_by",
      targetRef: { targetKind: "validator", targetId: "taskhub.precondition/version-at-commit" },
      enforcementLayer: "validator",
      status: "partial",
      confidence: "high",
    },
    {
      mappingId: "taskhub.mapping.snapshot_immutable",
      subject: { kind: "invariant", id: "taskhub.invariant.snapshot_revision_monotonic" },
      relation: "tested_by",
      targetRef: { targetKind: "native_test", targetId: "taskhub.test/source-edit-after-publish" },
      enforcementLayer: "native_test",
      status: "partial",
      confidence: "high",
    },
    {
      mappingId: "taskhub.mapping.approval_gate",
      subject: { kind: "invariant", id: "taskhub.invariant.approval_binds_current_preview" },
      relation: "enforced_by",
      targetRef: { targetKind: "runtime_method", targetId: "taskhub.method/publish-entry-gate" },
      enforcementLayer: "runtime_method",
      status: "partial",
      confidence: "high",
    },
    {
      mappingId: "taskhub.mapping.withdraw_hidden",
      subject: { kind: "invariant", id: "taskhub.invariant.withdrawn_stays_hidden" },
      relation: "requires_capability",
      targetRef: { targetKind: "unknown", targetId: "taskhub.capability/delayed-worker-ordering" },
      enforcementLayer: "unknown",
      status: "unknown",
      confidence: "low",
    },
  ],
  propertyHints: [
    {
      hintId: "taskhub.hint.entry_lifecycle",
      subject: { kind: "state_space", id: "taskhub.space.entry" },
      property: "transition_consistency",
      description: "Every lifecycle transition follows the declared from/to closure; withdrawal is terminal.",
    },
    {
      hintId: "taskhub.hint.snapshot_uniqueness",
      subject: { kind: "invariant", id: "taskhub.invariant.snapshot_revision_monotonic" },
      property: "uniqueness",
      description: "One snapshot row per tenant, entry, and revision.",
    },
  ],
});

console.log(JSON.stringify({ ok: true, modelDigest, irDigest }));
