#!/usr/bin/env node
// Issue #51 pilot fixture builder: re-canonicalizes the worklog-changelog
// pilot composition document (`attachments/pilot.json`) against the
// compiled neutral Model. Pins are computed from the exact production
// bytes; consumed-attachment digests are the canonical digests of the
// fixture attachment instances. Deterministic; no wall clock, no paths.
//
// The composition roles mirror the neutral Model exactly: the effect
// declarations, commit boundaries, replay scopes, approval/snapshot field
// references, public allowlist, and capability requirements. The core
// module (crates/lekalo-core/src/worklog_changelog) re-validates every one
// of these facts against the compiled IR at gate time.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const binary = process.env.LEKALO_BIN ?? resolve(root, "target/debug/lekalo.exe");
const project = "tests/fixtures/pilot/worklog-changelog/model";
const outPath = "tests/fixtures/pilot/worklog-changelog/attachments/pilot.json";

const sha = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");
const runJson = (args) =>
  JSON.parse(execFileSync(binary, args, { cwd: root, encoding: "utf8" }).trim());

const attachmentBytes = (relative) =>
  readFileSync(resolve(root, relative), "utf8");

const modelEnvelope = runJson(["--json", "--no-cache", "load", "--project", project]);
if (modelEnvelope.status !== "valid") throw new Error("model load failed");
const irEnvelope = runJson(["--json", "--no-cache", "load", "--ir", "--project", project]);
if (irEnvelope.status !== "valid") throw new Error("ir load failed");
const modelDigest = sha(JSON.stringify(modelEnvelope));
const irDigest = sha(JSON.stringify(irEnvelope.ir));

const attachmentDigest = (relative) => sha(attachmentBytes(relative));

const attachment = {
  schemaVersion: "lekalo/worklog-changelog-pilot/v0.8.0",
  identity: "dev.lekalo.worklog-changelog-pilot@0.8.0",
  projectId: "taskhub",
  closed: true,
  modelRef: { modelVersion: "0.2.16", digest: modelDigest },
  irRef: { identity: "dev.lekalo.ir@0.2.16", digest: irDigest },
  consumed: [
    {
      family: "authorization",
      ref: {
        identity: "dev.lekalo.authorization@0.2.16",
        version: "0.2.16",
        digest: attachmentDigest("tests/fixtures/pilot/worklog-changelog/model/lekalo/authorization.yaml"),
      },
    },
    {
      family: "transaction-concurrency",
      ref: {
        identity: "dev.lekalo.transaction-concurrency@0.2.16",
        version: "0.2.16",
        digest: attachmentDigest("tests/fixtures/pilot/worklog-changelog/attachments/transaction-concurrency.json"),
      },
    },
    {
      family: "extended-effects",
      ref: {
        identity: "dev.lekalo.extended-effects@0.2.16",
        version: "0.2.16",
        digest: attachmentDigest("tests/fixtures/pilot/worklog-changelog/attachments/extended-effects.json"),
      },
    },
    {
      family: "error-contract",
      ref: {
        identity: "dev.lekalo.error-registry@0.2.16",
        version: "0.2.16",
        digest: attachmentDigest("tests/fixtures/pilot/worklog-changelog/attachments/error-registry.json"),
      },
    },
    {
      family: "invariant-transition",
      ref: {
        identity: "dev.lekalo.invariant-transition@0.2.16",
        version: "0.2.16",
        digest: attachmentDigest("tests/fixtures/pilot/worklog-changelog/attachments/invariant-transition.json"),
      },
    },
    {
      family: "requirements",
      ref: {
        identity: "dev.lekalo.requirements@0.2.16",
        version: "0.2.16",
        digest: attachmentDigest("tests/fixtures/pilot/worklog-changelog/attachments/requirements.json"),
      },
    },
  ],
  operations: [
    {
      operation: "worklog.complete_task",
      effects: [{ kind: "update", effectId: "worklog.effect_task_state_write" }],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "worklog.write_worklog",
      effects: [{ kind: "create", effectId: "worklog.effect_worklog_write" }],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "worklog.edit_worklog",
      effects: [{ kind: "update", effectId: "worklog.effect_worklog_edit" }],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "worklog.request_provider_comment",
      effects: [
        { kind: "create", effectId: "worklog.effect_provider_action_write" },
        { kind: "enqueue-job", effectId: "worklog.effect_provider_job_enqueue" },
      ],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "worklog.deliver_provider_comment",
      effects: [{ kind: "external-call", effectId: "worklog.effect_provider_call" }],
      commitBoundary: "local_then_external",
      defaultSelection: false,
    },
    {
      operation: "changelog.create_draft",
      effects: [{ kind: "create", effectId: "changelog.effect_draft_write" }],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "changelog.edit_draft",
      effects: [{ kind: "update", effectId: "changelog.effect_draft_edit" }],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "changelog.preview_entry",
      effects: [{ kind: "update", effectId: "changelog.effect_preview_write" }],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "changelog.approve_entry",
      effects: [{ kind: "create", effectId: "changelog.effect_approval_write" }],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "changelog.publish_entry",
      effects: [
        { kind: "create", effectId: "changelog.effect_snapshot_append" },
        { kind: "update", effectId: "changelog.effect_visibility_write" },
        { kind: "publish-output", effectId: "changelog.effect_publication_emit" },
        { kind: "create", effectId: "changelog.effect_audit_append" },
      ],
      commitBoundary: "local_then_external",
      defaultSelection: false,
    },
    {
      operation: "changelog.withdraw_entry",
      effects: [
        { kind: "update", effectId: "changelog.effect_withdraw_write" },
        { kind: "create", effectId: "changelog.effect_audit_append" },
      ],
      commitBoundary: "local",
      defaultSelection: false,
    },
    {
      operation: "changelog.public_entry",
      effects: [{ kind: "read", effectId: "changelog.effect_snapshot_read" }],
      commitBoundary: "read_only",
      defaultSelection: false,
    },
  ],
  independentEffects: [
    {
      operation: "worklog.complete_task",
      mustNotEnable: [
        "worklog.write_worklog",
        "worklog.edit_worklog",
        "worklog.request_provider_comment",
        "worklog.deliver_provider_comment",
        "changelog.create_draft",
        "changelog.publish_entry",
      ],
    },
    {
      operation: "worklog.write_worklog",
      mustNotEnable: [
        "worklog.complete_task",
        "worklog.request_provider_comment",
        "worklog.deliver_provider_comment",
        "changelog.create_draft",
        "changelog.publish_entry",
      ],
    },
    {
      operation: "worklog.request_provider_comment",
      mustNotEnable: [
        "worklog.complete_task",
        "worklog.write_worklog",
        "changelog.create_draft",
        "changelog.publish_entry",
      ],
    },
    {
      operation: "changelog.create_draft",
      mustNotEnable: ["worklog.complete_task", "changelog.publish_entry"],
    },
    {
      operation: "worklog.edit_worklog",
      mustNotEnable: ["worklog.complete_task", "changelog.publish_entry"],
    },
    {
      operation: "changelog.publish_entry",
      mustNotEnable: [
        "worklog.complete_task",
        "worklog.write_worklog",
        "worklog.deliver_provider_comment",
      ],
    },
  ],
  correlations: [
    {
      correlationId: "taskhub.correlation.completion_flow",
      keyRef: "correlation_key",
      memberOperations: ["worklog.complete_task", "worklog.write_worklog"],
    },
  ],
  approvals: [
    {
      approvalId: "taskhub.approval.entry_publication",
      operation: "changelog.publish_entry",
      previewRevisionField: "preview_revision",
      previewDigestField: "preview_digest",
      tenantField: "tenant_id",
      destinationField: "destination",
      authorityField: "publishing_authority",
      policyRevisionField: "policy_revision",
    },
  ],
  snapshots: [
    {
      snapshotId: "taskhub.snapshot.changelog_entry",
      entity: "changelog.snapshot",
      revisionField: "revision",
      digestField: "content_digest",
      schemaVersionField: "snapshot_schema_version",
      immutability: "append_only",
    },
  ],
  publicAllowlist: {
    dto: "changelog.public_entry_dto",
    fields: [
      { name: "entry_id", type: "changelog.entry_id", source: "snapshot" },
      { name: "revision", type: "worklog.record_revision", source: "snapshot" },
      { name: "body", type: "changelog.public_field_text", source: "snapshot" },
      { name: "content_digest", type: "worklog.payload_digest", source: "snapshot" },
      { name: "snapshot_schema_version", type: "worklog.text", source: "snapshot" },
      { name: "published_at", type: "worklog.timestamp", source: "snapshot" },
    ],
    excluded: [
      "action_id",
      "approval_id",
      "audit_id",
      "author_id",
      "body",
      "correlation_key",
      "delivery_state",
      "destination",
      "draft_body",
      "entry_id",
      "lifecycle",
      "payload_digest",
      "policy_revision",
      "preview_digest",
      "preview_revision",
      "publishing_authority",
      "record_revision",
      "recorded_at",
      "requestor_id",
      "source_worklog_id",
      "task_id",
      "tenant_id",
      "transition",
      "visibility",
      "worklog_id",
      "workspace_id",
      "written_at",
    ],
    rendering: {
      defaultMode: "plain_text",
      urlPolicy: { schemes: ["https", "http"], rejectEncoded: true, rejectControlChars: true },
    },
  },
  replayScopes: [
    {
      operation: "worklog.complete_task",
      scope: "taskhub.scope.complete_task",
      keyRef: "correlation_key",
      duplicate: "replay_result",
      sameKeyDifferentRequest: "conflict",
      payloadBinding: "canonical_digest",
    },
    {
      operation: "worklog.write_worklog",
      scope: "taskhub.scope.write_worklog",
      keyRef: "correlation_key",
      duplicate: "replay_result",
      sameKeyDifferentRequest: "conflict",
      payloadBinding: "canonical_digest",
    },
    {
      operation: "worklog.request_provider_comment",
      scope: "taskhub.scope.provider_comment",
      keyRef: "correlation_key",
      duplicate: "replay_result",
      sameKeyDifferentRequest: "conflict",
      payloadBinding: "canonical_digest",
    },
    {
      operation: "changelog.publish_entry",
      scope: "taskhub.scope.publish_entry",
      keyRef: "correlation_key",
      duplicate: "replay_result",
      sameKeyDifferentRequest: "conflict",
      payloadBinding: "canonical_digest",
    },
  ],
  capabilityRequirements: [
    {
      requirementId: "taskhub.req/outbox-durable",
      capability: "delivery.durable",
      minimum: "full",
      reason:
        "A committed provider request must survive process death before its delivery outcome exists.",
      operationRefs: ["worklog.request_provider_comment"],
    },
    {
      requirementId: "taskhub.req/provider-idempotency",
      capability: "call.idempotent_write",
      minimum: "partial",
      reason:
        "Only provider-supported idempotent replay resolves an ambiguous remote acceptance.",
      operationRefs: ["worklog.deliver_provider_comment"],
    },
    {
      requirementId: "taskhub.req/publish-approval",
      capability: "publish.approval",
      minimum: "full",
      reason: "Publication commits only behind the approval binding gate.",
      operationRefs: ["changelog.publish_entry"],
    },
    {
      requirementId: "taskhub.req/revisioned-snapshot",
      capability: "publish.revisioned_snapshot",
      minimum: "full",
      reason: "Published entries are immutable revisioned snapshots with stored bytes.",
      operationRefs: ["changelog.publish_entry"],
    },
    {
      requirementId: "taskhub.req/security-review-gate",
      capability: "security.review_gate",
      minimum: "full",
      reason: "The sensitive public projection passes the declared review gate.",
      operationRefs: ["changelog.publish_entry"],
    },
    {
      requirementId: "taskhub.req/atomic-publication",
      capability: "transaction.atomic_group",
      minimum: "full",
      reason: "The snapshot append and visibility update commit inside one local transaction.",
      operationRefs: ["changelog.publish_entry"],
    },
    {
      requirementId: "taskhub.req/replay-dedup",
      capability: "idempotency.durable_key",
      minimum: "full",
      reason: "Duplicate requests replay their recorded result under the payload-bound key.",
      operationRefs: ["worklog.complete_task", "worklog.request_provider_comment"],
    },
    {
      requirementId: "taskhub.req/tenant-isolation",
      capability: "authorization.tenant_scope",
      minimum: "full",
      reason: "Every operation enforces tenant equality before any effect.",
      operationRefs: [
        "worklog.complete_task",
        "worklog.write_worklog",
        "worklog.request_provider_comment",
        "changelog.publish_entry",
        "changelog.public_entry",
      ],
    },
  ],
};

mkdirSync(resolve(root, dirname(outPath)), { recursive: true });
writeFileSync(resolve(root, outPath), JSON.stringify(attachment));
console.log(
  JSON.stringify({
    ok: true,
    out: outPath,
    modelDigest,
    irDigest,
    operations: attachment.operations.length,
  }),
);
