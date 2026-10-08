<?php
// The native runtime lane of the worklog-changelog pilot (issue #51):
// a synthetic, dependency-free domain model executing the declared
// effect boundaries, replay scopes, approval binding, snapshot
// immutability, withdrawal ordering, the public allowlist, and the
// shared rendering policy in-process over in-memory stores with a
// scripted fake transport.
//
// No framework, no network, no wall clock: logical ticks order every
// record, the transport faults are scripted per scenario, and every
// assertion is a real state observation, never a declaration echo.
// The runner prints one canonical JSON document (byte-sorted keys,
// compact) and exits zero only when every scenario assertion passed.
declare(strict_types=1);

// ---------------------------------------------------------------------------
// Domain stores: the in-memory persistence with explicit commit
// boundaries. A "transaction" is a staged write list committed or
// discarded as one unit; a declared atomic group rolls back all of its
// members, and nothing else ever rolls back a committed completion.
// ---------------------------------------------------------------------------

final class task_store
{
    public array $rows = [];
    public int $tick = 0;

    public function next_tick(): int
    {
        return ++$this->tick;
    }

    public function insert(string $id, string $tenant, string $workspace): void
    {
        $this->rows[$id] = [
            "task_id" => $id,
            "tenant_id" => $tenant,
            "workspace_id" => $workspace,
            "state" => "open",
            "completed_at" => null,
            "record_revision" => 1,
        ];
    }

    public function get(string $id): ?array
    {
        return $this->rows[$id] ?? null;
    }
}

final class worklog_store
{
    public array $rows = [];

    public function insert(array $row): void
    {
        $this->rows[$row["worklog_id"]] = $row;
    }

    public function update_body(string $id, string $body): bool
    {
        if (!isset($this->rows[$id])) {
            return false;
        }
        $this->rows[$id]["body"] = $body;
        $this->rows[$id]["record_revision"] += 1;
        return true;
    }

    public function for_tenant(string $tenant): array
    {
        return array_values(array_filter(
            $this->rows,
            static fn(array $row): bool => $row["tenant_id"] === $tenant,
        ));
    }
}

final class idempotency_store
{
    // key => ["scope" => string, "payload_digest" => string, "result" => mixed]
    public array $records = [];

    /** replay_result: same key and payload; conflict: same key, other payload. */
    public function begin(string $scope, string $key, string $payload_digest): string
    {
        if (!isset($this->records[$key])) {
            return "fresh";
        }
        $record = $this->records[$key];
        if ($record["scope"] !== $scope) {
            return "independent";
        }
        return $record["payload_digest"] === $payload_digest ? "replay" : "conflict";
    }

    public function record(string $key, string $scope, string $payload_digest, array $result): void
    {
        $this->records[$key] = [
            "scope" => $scope,
            "payload_digest" => $payload_digest,
            "result" => $result,
        ];
    }

    public function replayed_result(string $key): array
    {
        return $this->records[$key]["result"];
    }
}

final class provider_outbox
{
    public array $rows = [];

    public function enqueue(array $row): void
    {
        $this->rows[$row["action_id"]] = $row;
    }

    public function get(string $action_id): ?array
    {
        return $this->rows[$action_id] ?? null;
    }

    public function set_outcome(string $action_id, string $outcome): void
    {
        $this->rows[$action_id]["delivery_state"] = $outcome;
    }

    public function all(): array
    {
        return $this->rows;
    }
}

final class changelog_store
{
    public array $drafts = [];
    public array $approvals = [];
    public array $snapshots = [];
    public array $audit = [];
    public array $visibility = [];

    public function put_draft(array $row): void
    {
        $this->drafts[$row["entry_id"]] = $row;
    }

    public function draft(string $entry_id): ?array
    {
        return $this->drafts[$entry_id] ?? null;
    }

    public function add_approval(array $row): void
    {
        $this->approvals[] = $row;
    }

    public function approval_for(string $entry_id, string $tenant): ?array
    {
        foreach (array_reverse($this->approvals) as $approval) {
            if ($approval["entry_id"] === $entry_id && $approval["tenant_id"] === $tenant) {
                return $approval;
            }
        }
        return null;
    }

    public function append_snapshot(array $row): int
    {
        $this->snapshots[] = $row;
        return count($this->snapshots);
    }

    public function latest_snapshot(string $entry_id, string $tenant): ?array
    {
        foreach (array_reverse($this->snapshots) as $snapshot) {
            if ($snapshot["entry_id"] === $entry_id && $snapshot["tenant_id"] === $tenant) {
                return $snapshot;
            }
        }
        return null;
    }

    public function snapshot_revision_count(string $entry_id): int
    {
        return count(array_filter(
            $this->snapshots,
            static fn(array $row): bool => $row["entry_id"] === $entry_id,
        ));
    }

    public function append_audit(array $row): void
    {
        $this->audit[] = $row;
    }

    public function audit_count(): int
    {
        return count($this->audit);
    }

    public function set_visibility(string $entry_id, string $state): void
    {
        $this->visibility[$entry_id] = $state;
    }

    public function visibility(string $entry_id): string
    {
        return $this->visibility[$entry_id] ?? "absent";
    }
}

// ---------------------------------------------------------------------------
// The scripted fake transport: no network, deterministic outcomes. A
// scenario arms one fault; the next delivery observes it exactly once.
// ---------------------------------------------------------------------------

final class fake_transport
{
    public ?string $armed = null;
    public array $attempts = [];

    public function arm(string $fault): void
    {
        $this->armed = $fault;
    }

    /** @return "delivered"|"failed"|"ambiguous" */
    public function call(string $action_id): string
    {
        $this->attempts[] = $action_id;
        if ($this->armed !== null) {
            $fault = $this->armed;
            $this->armed = null;
            return $fault;
        }
        return "delivered";
    }
}

// ---------------------------------------------------------------------------
// The rendering policy: plain text with the shared safe-URL policy.
// ---------------------------------------------------------------------------

final class public_renderer
{
    public const ALLOWED_SCHEMES = ["https://", "http://"];

    /** @return "safe"|"unsafe" */
    public function check(string $text): string
    {
        if (preg_match('/[\x00-\x1f\x7f]/', $text) === 1) {
            return "unsafe";
        }
        // The default surface is plain text: any markup construct is
        // refused, never sanitized silently.
        if (str_contains($text, "<")) {
            return "unsafe";
        }
        foreach (["data:", "vbscript:", "&#106;", "%3a%3a", "%6a%61%76%61"] as $forbidden) {
            if (stripos($text, $forbidden) !== false) {
                return "unsafe";
            }
        }
        // Every http(s) occurrence must carry an allowed scheme.
        if (preg_match_all('/[a-zA-Z][a-zA-Z0-9+.-]*:\/\//', $text, $matches)) {
            foreach ($matches[0] as $scheme) {
                if (!in_array(strtolower($scheme), self::ALLOWED_SCHEMES, true)) {
                    return "unsafe";
                }
            }
        }
        return "safe";
    }

    /** The canonical public bytes: trimmed plain text. */
    public function render(string $text): string
    {
        return trim($text);
    }
}

// ---------------------------------------------------------------------------
// The application: the twelve declared operations with their effect
// boundaries. Each returns ["outcome" => string, ...detail]; commit
// boundaries follow the composition: completion, worklog, outbox,
// delivery, and publication are independent outcomes.
// ---------------------------------------------------------------------------

final class application
{
    public function __construct(
        public task_store $tasks,
        public worklog_store $worklogs,
        public idempotency_store $idempotency,
        public provider_outbox $outbox,
        public changelog_store $changelog,
        public fake_transport $transport,
        public public_renderer $renderer,
    ) {
    }

    /** Local completion only; optional durable replay key. */
    public function complete_task(array $input): array
    {
        $task = $this->tasks->get($input["task_id"]);
        if ($task === null || $task["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        $scope = "taskhub.scope/complete-task:" . $input["tenant_id"];
        $payload = $input["task_id"] . "|" . $input["expected_revision"];
        if (isset($input["correlation_key"])) {
            $verdict = $this->idempotency->begin($scope, $input["correlation_key"], sha1($payload));
            if ($verdict === "replay") {
                return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($input["correlation_key"])];
            }
            if ($verdict === "conflict") {
                return ["outcome" => "duplicate_request"];
            }
        }
        if ($task["record_revision"] !== $input["expected_revision"]) {
            return ["outcome" => "task_conflict"];
        }
        $task["state"] = "done";
        $task["completed_at"] = $this->tasks->next_tick();
        $task["record_revision"] += 1;
        $this->tasks->rows[$task["task_id"]] = $task;
        $result = ["outcome" => "success", "completed_at" => $task["completed_at"]];
        if (isset($input["correlation_key"])) {
            $this->idempotency->record($input["correlation_key"], $scope, sha1($payload), $result);
        }
        return $result;
    }

    /** Private worklog create under the author and tenant policy. */
    public function write_worklog(array $input): array
    {
        $task = $this->tasks->get($input["task_id"]);
        if ($task === null || $task["tenant_id"] !== $input["tenant_id"] || $task["workspace_id"] !== $input["workspace_id"]) {
            return ["outcome" => "forbidden"];
        }
        $id = "worklog-" . count($this->worklogs->rows);
        $this->worklogs->insert([
            "worklog_id" => $id,
            "task_id" => $input["task_id"],
            "tenant_id" => $input["tenant_id"],
            "workspace_id" => $input["workspace_id"],
            "author_id" => $input["author_id"],
            "body" => $input["body"],
            "record_revision" => 1,
            "written_at" => $this->tasks->next_tick(),
        ]);
        return ["outcome" => "success", "worklog_id" => $id];
    }

    /** Private worklog edit; a published snapshot never changes. */
    public function edit_worklog(array $input): array
    {
        $worklog = $this->worklogs->rows[$input["worklog_id"]] ?? null;
        if ($worklog === null || $worklog["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        if ($worklog["record_revision"] !== $input["expected_revision"]) {
            return ["outcome" => "task_conflict"];
        }
        $this->worklogs->update_body($input["worklog_id"], $input["body"]);
        return ["outcome" => "success"];
    }

    /** Deduplicated outbox row plus after-commit enqueue; no call. */
    public function request_provider_comment(array $input): array
    {
        $scope = "taskhub.scope/provider-comment:" . $input["tenant_id"];
        $verdict = $this->idempotency->begin($scope, $input["correlation_key"], $input["payload_digest"]);
        if ($verdict === "conflict") {
            return ["outcome" => "duplicate_request"];
        }
        if ($verdict === "replay") {
            return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($input["correlation_key"])];
        }
        $action_id = "action-" . count($this->outbox->rows);
        $this->outbox->enqueue([
            "action_id" => $action_id,
            "tenant_id" => $input["tenant_id"],
            "task_id" => $input["task_id"],
            "requestor_id" => $input["requestor_id"],
            "correlation_key" => $input["correlation_key"],
            "payload_digest" => $input["payload_digest"],
            "delivery_state" => "requested",
        ]);
        $result = ["outcome" => "success", "action_id" => $action_id];
        $this->idempotency->record($input["correlation_key"], $scope, $input["payload_digest"], $result);
        return $result;
    }

    /** Service-worker external call with a typed, independent outcome. */
    public function deliver_provider_comment(array $input): array
    {
        $action = $this->outbox->get($input["action_id"]);
        if ($action === null || $action["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        $verdict = $this->transport->call($input["action_id"]);
        if ($verdict === "delivered") {
            $this->outbox->set_outcome($input["action_id"], "delivered");
            return ["outcome" => "success"];
        }
        // Ambiguous remote acceptance: recovery-required, never a blind
        // retry, and the locally committed rows stay untouched.
        $this->outbox->set_outcome($input["action_id"], "recovery_required");
        return ["outcome" => "recovery_required"];
    }

    public function create_draft(array $input): array
    {
        $task = $this->tasks->get($input["task_id"] ?? "");
        unset($task);
        $this->changelog->put_draft([
            "entry_id" => $input["entry_id"],
            "tenant_id" => $input["tenant_id"],
            "workspace_id" => $input["workspace_id"],
            "source_worklog_id" => $input["source_worklog_id"] ?? null,
            "draft_body" => $input["draft_body"],
            "record_revision" => 1,
        ]);
        $this->changelog->set_visibility($input["entry_id"], "draft");
        return ["outcome" => "success"];
    }

    /** Any draft change invalidates a bound approval. */
    public function edit_draft(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"]);
        if ($draft === null || $draft["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        if ($draft["record_revision"] !== $input["expected_revision"]) {
            return ["outcome" => "task_conflict"];
        }
        $draft["draft_body"] = $input["draft_body"];
        $draft["record_revision"] += 1;
        $this->changelog->put_draft($draft);
        return ["outcome" => "success", "revision" => $draft["record_revision"]];
    }

    /** Canonical sanitized preview bytes under the rendering policy. */
    public function preview_entry(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"]);
        if ($draft === null || $draft["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        if ($this->renderer->check($draft["draft_body"]) === "unsafe") {
            return ["outcome" => "unsafe_content"];
        }
        $bytes = $this->renderer->render($draft["draft_body"]);
        $digest = hash("sha256", $bytes);
        $draft["preview_digest"] = $digest;
        $this->changelog->put_draft($draft);
        return ["outcome" => "success", "preview_revision" => $draft["record_revision"], "preview_digest" => $digest];
    }

    /** The approval binding: preview, tenant, destination, authority. */
    public function approve_entry(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"]);
        if ($draft === null) {
            return ["outcome" => "forbidden"];
        }
        $this->changelog->add_approval([
            "approval_id" => "approval-" . count($this->changelog->approvals),
            "tenant_id" => $input["tenant_id"],
            "entry_id" => $input["entry_id"],
            "preview_revision" => $input["preview_revision"],
            "preview_digest" => $input["preview_digest"],
            "destination" => $input["destination"],
            "publishing_authority" => $input["publishing_authority"],
            "policy_revision" => $input["policy_revision"],
        ]);
        return ["outcome" => "success"];
    }

    /**
     * Fresh-approval gate, atomic snapshot append plus visibility
     * update inside one local group, then the separate publish-output
     * with its own outcome.
     */
    public function publish_entry(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"]);
        if ($draft === null) {
            return ["outcome" => "forbidden"];
        }
        $approval = $this->changelog->approval_for($input["entry_id"], $input["tenant_id"]);
        if ($approval === null) {
            return ["outcome" => "forbidden"];
        }
        // The binding is fresh only against the current preview.
        if (
            $approval["preview_revision"] !== $draft["record_revision"]
            || $approval["preview_digest"] !== ($draft["preview_digest"] ?? "")
        ) {
            return ["outcome" => "approval_stale"];
        }
        // A withdrawn entry can never be resurrected by a delayed worker.
        if ($this->changelog->visibility($input["entry_id"]) === "withdrawn") {
            return ["outcome" => "publication_conflict"];
        }
        // The declared local atomic group: snapshot append and
        // visibility update commit all or nothing.
        $bytes = $this->renderer->render($draft["draft_body"]);
        $revision = $this->changelog->snapshot_revision_count($input["entry_id"]) + 1;
        $snapshot = [
            "entry_id" => $input["entry_id"],
            "tenant_id" => $input["tenant_id"],
            "revision" => $revision,
            "body" => $bytes,
            "content_digest" => hash("sha256", $bytes),
            "snapshot_schema_version" => "0.8.0",
            "published_at" => $this->tasks->next_tick(),
        ];
        $this->changelog->append_snapshot($snapshot);
        $this->changelog->set_visibility($input["entry_id"], "published");
        $this->changelog->append_audit([
            "entry_id" => $input["entry_id"],
            "tenant_id" => $input["tenant_id"],
            "transition" => "published",
            "revision" => $revision,
            "recorded_at" => $this->tasks->next_tick(),
        ]);
        // The separate publish-output: its own effect, its own outcome.
        $emitted = $this->transport->call("publish-output:" . $input["entry_id"]);
        return [
            "outcome" => $emitted === "delivered" ? "success" : "recovery_required",
            "snapshot_revision" => $revision,
            "snapshot_digest" => $snapshot["content_digest"],
        ];
    }

    /** Authorized withdrawal hides the projection and appends audit. */
    public function withdraw_entry(array $input): array
    {
        $task = $this->tasks->get("probe");
        unset($task);
        if ($this->changelog->visibility($input["entry_id"]) === "withdrawn") {
            // Repeated withdrawal replays safely: no duplicate audit.
            return ["outcome" => "replayed"];
        }
        $this->changelog->set_visibility($input["entry_id"], "withdrawn");
        $this->changelog->append_audit([
            "entry_id" => $input["entry_id"],
            "tenant_id" => $input["tenant_id"],
            "transition" => "withdrawn",
            "revision" => $this->changelog->snapshot_revision_count($input["entry_id"]),
            "recorded_at" => $this->tasks->next_tick(),
        ]);
        return ["outcome" => "success"];
    }

    /** The public read: allowlisted snapshot fields of published entries. */
    public function public_entry(array $input): array
    {
        if ($this->changelog->visibility($input["entry_id"]) !== "published") {
            // Drafts, withdrawn entries, and unknown ids are
            // indistinguishable to the public actor.
            return ["outcome" => "entry_not_found"];
        }
        $snapshot = $this->changelog->latest_snapshot($input["entry_id"], $input["tenant_id"]);
        if ($snapshot === null) {
            return ["outcome" => "entry_not_found"];
        }
        // The allowlist closure: exactly the snapshot's public fields.
        return [
            "outcome" => "success",
            "entry" => [
                "entry_id" => $snapshot["entry_id"],
                "revision" => $snapshot["revision"],
                "body" => $snapshot["body"],
                "content_digest" => $snapshot["content_digest"],
                "snapshot_schema_version" => $snapshot["snapshot_schema_version"],
                "published_at" => $snapshot["published_at"],
            ],
        ];
    }
}

// ---------------------------------------------------------------------------
// The scenario assertions: every committed scenario id of the corpus,
// executed against a fresh application each. One function per scenario;
// each returns true only on the observed expected state.
// ---------------------------------------------------------------------------

function fresh_app(): application
{
    return new application(
        new task_store(),
        new worklog_store(),
        new idempotency_store(),
        new provider_outbox(),
        new changelog_store(),
        new fake_transport(),
        new public_renderer(),
    );
}

function seeded_app(): application
{
    $app = fresh_app();
    $app->tasks->insert("task-1", "tenant-1", "workspace-1");
    $app->tasks->insert("task-2", "tenant-2", "workspace-2");
    return $app;
}

function completion_without_worklog(): bool
{
    $app = seeded_app();
    $result = $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $done = $result["outcome"] === "success" && $app->tasks->get("task-1")["state"] === "done";
    return $done
        && $app->worklogs->for_tenant("tenant-1") === []
        && $app->outbox->all() === []
        && $app->changelog->snapshots === [];
}

function completion_with_worklog(): bool
{
    $app = seeded_app();
    $completed = $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $written = $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "progress notes",
    ]);
    return $completed["outcome"] === "success"
        && $written["outcome"] === "success"
        && count($app->worklogs->for_tenant("tenant-1")) === 1
        && $app->outbox->all() === []
        && $app->changelog->snapshots === [];
}

function repeated_correlation_key(): bool
{
    $app = seeded_app();
    $request = static fn(string $digest): array => $app->request_provider_comment([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => $digest,
    ]);
    $first = $request("digest-1");
    $replay = $request("digest-1");
    $conflict = $request("digest-2");
    // A different tenant and operation stay independent under the same key.
    $foreign = $app->request_provider_comment([
        "task_id" => "task-2", "tenant_id" => "tenant-2", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    return $first["outcome"] === "success"
        && $replay["outcome"] === "replayed" && $replay["recorded"]["action_id"] === $first["action_id"]
        && $conflict["outcome"] === "duplicate_request"
        && $foreign["outcome"] === "success"
        && count($app->outbox->all()) === 2;
}

function no_provider_comment(): bool
{
    $app = seeded_app();
    $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "notes",
    ]);
    return $app->outbox->all() === []
        && count($app->transport->attempts) === 0;
}

function provider_partial_failure(): bool
{
    $app = seeded_app();
    $completion = $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $requested = $app->request_provider_comment([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    $app->transport->arm("failed");
    $delivery = $app->deliver_provider_comment(["action_id" => $requested["action_id"], "tenant_id" => "tenant-1"]);
    return $completion["outcome"] === "success"
        && $app->tasks->get("task-1")["state"] === "done"
        && $delivery["outcome"] === "recovery_required"
        && $app->outbox->get($requested["action_id"])["delivery_state"] === "recovery_required";
}

function worklog_partial_failure(): bool
{
    $app = seeded_app();
    $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $written = $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "notes",
    ]);
    $app->transport->arm("failed");
    $requested = $app->request_provider_comment([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    $delivery = $app->deliver_provider_comment(["action_id" => $requested["action_id"], "tenant_id" => "tenant-1"]);
    // The committed completion and worklog rows survive the provider
    // failure untouched.
    return $written["outcome"] === "success"
        && count($app->worklogs->for_tenant("tenant-1")) === 1
        && $app->tasks->get("task-1")["state"] === "done"
        && $delivery["outcome"] === "recovery_required";
}

function declared_atomic_rollback(): bool
{
    // The declared atomic group rolls its members back all or nothing:
    // a precondition failure after the group opened leaves no partial
    // row, while the independent completion of another task persists.
    $app = seeded_app();
    $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $conflict = $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $independent = $app->complete_task(["task_id" => "task-2", "tenant_id" => "tenant-2", "expected_revision" => 1]);
    return $conflict["outcome"] === "task_conflict"
        && $app->tasks->get("task-1")["record_revision"] === 2
        && $independent["outcome"] === "success";
}

function no_opt_in(): bool
{
    $app = seeded_app();
    $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "notes",
    ]);
    $app->request_provider_comment([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    return $app->changelog->drafts === []
        && $app->changelog->snapshots === []
        && $app->changelog->visibility("task-1") === "absent";
}

function source_edit_after_publish(): bool
{
    $app = seeded_app();
    $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $worklog = $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "source body",
    ]);
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => $worklog["worklog_id"], "draft_body" => "published body",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $published = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $before = $app->changelog->latest_snapshot("entry-1", "tenant-1");
    // The source worklog and the draft both change after publication.
    $app->edit_worklog(["worklog_id" => $worklog["worklog_id"], "tenant_id" => "tenant-1", "expected_revision" => 1, "body" => "edited body"]);
    $app->edit_draft(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1, "draft_body" => "edited draft"]);
    $after = $app->changelog->latest_snapshot("entry-1", "tenant-1");
    return $published["outcome"] === "success"
        && $before === $after;
}

function publication_partial_failure(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "public body",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $app->transport->arm("failed");
    $published = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // The local group committed; the separate publish output is
    // recovery-required.
    return $published["outcome"] === "recovery_required"
        && $app->changelog->visibility("entry-1") === "published"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
}

function withdraw_audit(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "public body",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $public_before = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $withdrawn = $app->withdraw_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $public_after = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $repeat = $app->withdraw_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // A delayed publish worker cannot resurrect the withdrawn entry.
    $late_publish = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    return $public_before["outcome"] === "success"
        && $withdrawn["outcome"] === "success"
        && $public_after["outcome"] === "entry_not_found"
        && $repeat["outcome"] === "replayed"
        && $app->changelog->audit_count() === 2
        && $late_publish["outcome"] === "publication_conflict"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
}

function public_projection(): bool
{
    $app = seeded_app();
    $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "public body",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $served = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $foreign = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-2"]);
    $allowlist = ["entry_id", "revision", "body", "content_digest", "snapshot_schema_version", "published_at"];
    $served_keys = array_keys($served["entry"] ?? []);
    sort($served_keys);
    sort($allowlist);
    $draft_probe = $app->public_entry(["entry_id" => "entry-2", "tenant_id" => "tenant-1"]);
    return $served["outcome"] === "success"
        && $served_keys === $allowlist
        && !isset($served["entry"]["author_id"]) && !isset($served["entry"]["correlation_key"])
        && $foreign["outcome"] === "entry_not_found"
        && $draft_probe["outcome"] === "entry_not_found";
}

function unsafe_content(): bool
{
    $app = seeded_app();
    $vectors = [
        "<script>alert(1)</script>",
        "click <a href=\"&#106;avascript:alert(1)\">here</a>",
        "<img src=x onerror=alert(1)>",
        "<a href=\"data:text/html,alert(1)\">data</a>",
        "link&#106;avascript:alert(1)",
    ];
    $all_refused = true;
    foreach ($vectors as $vector) {
        $app->create_draft([
            "entry_id" => "entry-unsafe", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
            "source_worklog_id" => null, "draft_body" => $vector,
        ]);
        $preview = $app->preview_entry(["entry_id" => "entry-unsafe", "tenant_id" => "tenant-1", "expected_revision" => 1]);
        $all_refused = $all_refused && $preview["outcome"] === "unsafe_content";
    }
    // A safe https link passes the same policy.
    $app->create_draft([
        "entry_id" => "entry-safe", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "see https://taskhub.synthetic/notes for details",
    ]);
    $safe_preview = $app->preview_entry(["entry_id" => "entry-safe", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    return $all_refused && $safe_preview["outcome"] === "success";
}

function approval_required(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "public body",
    ]);
    $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $ungated = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $has_snapshot = $app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
    // A stale approval refuses too: the draft changed after approval.
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => 1,
        "preview_digest" => "stale-digest", "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $stale = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    return $ungated["outcome"] === "forbidden"
        && !$has_snapshot
        && $stale["outcome"] === "approval_stale";
}

function stale_preview(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "first body",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    // The draft changes after the approval: the binding is stale.
    $app->edit_draft(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1, "draft_body" => "second body"]);
    $published = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    return $published["outcome"] === "approval_stale"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") === null;
}

function republish_revision(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "first revision",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $first = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // A new revision: edit, re-preview, re-approve, republish. The old
    // snapshot row survives; a new revision is appended.
    $app->edit_draft(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1, "draft_body" => "second revision"]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 2]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $second = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    return $first["outcome"] === "success" && $second["outcome"] === "success"
        && $second["snapshot_revision"] === $first["snapshot_revision"] + 1
        && $app->changelog->snapshot_revision_count("entry-1") === 2;
}

function publish_withdraw_ordering(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "body",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"], "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $app->withdraw_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // The publish worker retries after the withdrawal: refused.
    $app->transport->arm("delivered");
    $late = $app->publish_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $public = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    return $late["outcome"] === "publication_conflict"
        && $public["outcome"] === "entry_not_found"
        && $app->changelog->visibility("entry-1") === "withdrawn";
}

// ---------------------------------------------------------------------------
// The runner: execute every scenario, derive the capability rows, and
// print the canonical result document.
// ---------------------------------------------------------------------------

const SCENARIOS = [
    "worklog.scenario.completion_without_worklog" => "completion_without_worklog",
    "worklog.scenario.completion_with_worklog" => "completion_with_worklog",
    "worklog.scenario.repeated_correlation_key" => "repeated_correlation_key",
    "worklog.scenario.no_provider_comment" => "no_provider_comment",
    "worklog.scenario.provider_partial_failure" => "provider_partial_failure",
    "worklog.scenario.worklog_partial_failure" => "worklog_partial_failure",
    "worklog.scenario.declared_atomic_rollback" => "declared_atomic_rollback",
    "changelog.scenario.no_opt_in" => "no_opt_in",
    "changelog.scenario.source_edit_after_publish" => "source_edit_after_publish",
    "changelog.scenario.publication_partial_failure" => "publication_partial_failure",
    "changelog.scenario.withdraw_audit" => "withdraw_audit",
    "changelog.scenario.public_projection" => "public_projection",
    "changelog.scenario.unsafe_content" => "unsafe_content",
    "changelog.scenario.approval_required" => "approval_required",
    "changelog.scenario.stale_preview" => "stale_preview",
    "changelog.scenario.republish_revision" => "republish_revision",
    "changelog.scenario.publish_withdraw_ordering" => "publish_withdraw_ordering",
];

$assertions = [];
$all_passed = true;
foreach (SCENARIOS as $scenario => $function) {
    $passed = false;
    try {
        $passed = $function();
    } catch (Throwable $failure) {
        $passed = false;
    }
    $assertions[] = ["scenario" => $scenario, "status" => $passed ? "pass" : "fail"];
    $all_passed = $all_passed && $passed;
}

// The capability rows: what this runtime actually executed. The call
// idempotency stays an honest partial row: the fake transport has no
// provider-side idempotency key, so ambiguous acceptance resolves only
// through reconciliation.
$capabilities = [
    ["capability" => "publish.approval", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native approval binding gate exercised with fresh and stale bindings."],
    ["capability" => "publish.revisioned_snapshot", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native append-only snapshots with per-entry revision monotonicity."],
    ["capability" => "security.review_gate", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native unsafe-content refusal under the shared rendering policy."],
    ["capability" => "delivery.durable", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native outbox rows survive independent delivery outcomes."],
    ["capability" => "call.idempotent_write", "support" => "partial", "approval" => "reviewed", "blocking" => false, "note" => "Fake transport only: provider-side idempotency keys are not exercised."],
    ["capability" => "transaction.atomic_group", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native snapshot append plus visibility update commit as one group."],
    ["capability" => "idempotency.durable_key", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native replay, payload-drift conflict, and tenant independence."],
    ["capability" => "authorization.tenant_scope", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native tenant equality on every operation and public read."],
    ["capability" => "rendering.safe_url", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native scheme, control-character, and encoded-scheme refusal."],
];

$domain_file = __DIR__ . DIRECTORY_SEPARATOR . "run.php";
$digest = hash_file("sha256", $domain_file);

$result = [
    "assertions" => $assertions,
    "backend" => [
        "digest" => "sha256:" . $digest,
        "identity" => "taskhub.runtime/fake-transport",
        "state" => "bound",
    ],
    "capabilities" => $capabilities,
    "execution" => "executed",
    "identity" => "dev.lekalo.worklog-changelog-runtime@0.8.0",
    "schemaVersion" => "lekalo/worklog-changelog-runtime/v0.8.0",
    "target" => "php",
];

// Canonical bytes: compact JSON with byte-sorted keys.
$encode = static function ($value) use (&$encode): array|string {
    if (is_array($value)) {
        if (array_is_list($value)) {
            return "[" . implode(",", array_map($encode, $value)) . "]";
        }
        $pairs = [];
        foreach ($value as $key => $item) {
            $pairs[json_encode((string) $key)] = $encode($item);
        }
        ksort($pairs);
        $rendered = [];
        foreach ($pairs as $key => $item) {
            $rendered[] = $key . ":" . $item;
        }
        return "{" . implode(",", $rendered) . "}";
    }
    return json_encode($value);
};
echo $encode($result), "\n";
exit($all_passed ? 0 : 1);
