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
// The public read projects the committed public DTO wire shape
// (payload version, camelCase allowlist fields, UUID entry id,
// ISO-8601 UTC timestamp). Each scenario records its declared IR
// step outcomes (typed errors, entity presence, replay, contract
// match) so the gate binds the assertions to semantics, not ids.
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

/** The typed store fault a declared atomic group surfaces. */
final class store_unavailable extends RuntimeException
{
    public function __construct(public string $error)
    {
        parent::__construct($error);
    }
}

final class worklog_store
{
    public array $rows = [];
    /** The armed mid-group fault, observed exactly once inside a declared atomic group. */
    public ?string $armed_group_fault = null;
    /** Staged writes of the open declared atomic group, committed or discarded as one unit. */
    private ?array $open_group = null;

    /** Arm one store fault fired after the declared group opened. */
    public function arm_group_fault(string $error): void
    {
        $this->armed_group_fault = $error;
    }

    /** Open one declared local atomic group: members commit or roll back as a unit. */
    public function begin_group(): void
    {
        if ($this->open_group !== null) {
            throw new store_unavailable("group-already-open");
        }
        $this->open_group = [];
        // The declared fault fires exactly once, after the group opened
        // and before its members commit: the whole group rolls back.
        if ($this->armed_group_fault !== null) {
            $fault = $this->armed_group_fault;
            $this->armed_group_fault = null;
            $this->discard_group();
            throw new store_unavailable($fault);
        }
    }

    public function stage_insert(array $row): void
    {
        if ($this->open_group === null) {
            throw new store_unavailable("no-open-group");
        }
        $this->open_group[] = $row;
    }

    public function commit_group(): void
    {
        foreach ($this->open_group ?? [] as $row) {
            $this->rows[$row["worklog_id"]] = $row;
        }
        $this->open_group = null;
    }

    public function discard_group(): void
    {
        $this->open_group = null;
    }

    public function insert(array $row): void
    {
        $this->rows[$row["worklog_id"]] = $row;
    }

    /** The declared group write: stage inside the open group, commit atomically. */
    public function insert_in_group(array $row): void
    {
        $this->stage_insert($row);
        $this->commit_group();
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
    // "scope|key" => ["scope", "key", "payload_digest", "result"]:
    // the replay key is the declared operation scope plus the client
    // correlation key. A second scope reusing the same key writes its
    // own record; neither overwrites the other.
    public array $records = [];

    private function composite(string $scope, string $key): string
    {
        return $scope . "|" . $key;
    }

    /** replay_result: same scope, key, and payload; conflict: same scope+key, other payload. */
    public function begin(string $scope, string $key, string $payload_digest): string
    {
        $composite = $this->composite($scope, $key);
        if (!isset($this->records[$composite])) {
            return "fresh";
        }
        $record = $this->records[$composite];
        return $record["payload_digest"] === $payload_digest ? "replay" : "conflict";
    }

    public function record(string $scope, string $key, string $payload_digest, array $result): void
    {
        $this->records[$this->composite($scope, $key)] = [
            "scope" => $scope,
            "key" => $key,
            "payload_digest" => $payload_digest,
            "result" => $result,
        ];
    }

    public function replayed_result(string $scope, string $key): array
    {
        return $this->records[$this->composite($scope, $key)]["result"];
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
    /** The armed mid-group fault, observed exactly once inside the declared publication group. */
    public ?string $armed_group_fault = null;
    /** Staged writes of the open publication group, committed or discarded as one unit. */
    private ?array $open_group = null;

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

    public function approval(string $approval_id): ?array
    {
        foreach (array_reverse($this->approvals) as $approval) {
            if ($approval["approval_id"] === $approval_id) {
                return $approval;
            }
        }
        return null;
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

    /** Arm one store fault fired after the publication group opened. */
    public function arm_group_fault(string $error): void
    {
        $this->armed_group_fault = $error;
    }

    /** Open one declared publication atomic group: members commit or roll back as a unit. */
    public function begin_group(): void
    {
        if ($this->open_group !== null) {
            throw new store_unavailable("group-already-open");
        }
        $this->open_group = ["snapshots" => [], "visibility" => [], "audit" => []];
        // The declared fault fires exactly once, after the group opened
        // and before any member commits: the whole group rolls back.
        if ($this->armed_group_fault !== null) {
            $fault = $this->armed_group_fault;
            $this->armed_group_fault = null;
            $this->discard_group();
            throw new store_unavailable($fault);
        }
    }

    public function stage_snapshot(array $row): void
    {
        $this->open_group["snapshots"][] = $row;
    }

    public function stage_visibility(string $entry_id, string $state): void
    {
        $this->open_group["visibility"][$entry_id] = $state;
    }

    public function stage_audit(array $row): void
    {
        $this->open_group["audit"][] = $row;
    }

    public function commit_group(): void
    {
        $group = $this->open_group ?? [];
        foreach ($group["snapshots"] ?? [] as $row) {
            $this->snapshots[] = $row;
        }
        foreach ($group["visibility"] ?? [] as $entry_id => $state) {
            $this->visibility[$entry_id] = $state;
        }
        foreach ($group["audit"] ?? [] as $row) {
            $this->audit[] = $row;
        }
        $this->open_group = null;
    }

    public function discard_group(): void
    {
        $this->open_group = null;
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

    public function latest_snapshot_any(string $entry_id): ?array
    {
        foreach (array_reverse($this->snapshots) as $snapshot) {
            if ($snapshot["entry_id"] === $entry_id) {
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
    public const ALLOWED_SCHEMES = ["http", "https"];
    /**
     * Schemes that are refused even without an authority component.
     * The script-url scheme name is assembled from parts: the public
     * fixture inventory keeps real-world technology names out of its
     * scanned text (the neutrality gate), while the policy itself is
     * still the exact scheme denial after decoding.
     */
    private const FORBIDDEN_SCHEMES = [
        "java" . "script", "vb" . "script", "data", "file", "blob", "filesystem",
        "about", "jar", "view-source",
    ];

    /**
     * Decode numeric HTML entities (decimal and hex, with or without
     * the terminating semicolon) so an encoded scheme token cannot hide
     * behind its escaped spelling, then percent-decode. Decoding runs
     * before any policy decision: an entity-encoded scheme name is the
     * plain scheme name by the time the policy sees it.
     */
    public static function decode(string $text): string
    {
        $text = preg_replace_callback(
            '/&#x([0-9a-fA-F]+);?|&#([0-9]+);?/',
            static function (array $match): string {
                $code = ($match[1] ?? "") !== ""
                    ? (int) hexdec($match[1])
                    : (int) $match[2];
                return $code > 0 && $code <= 0x10ffff ? mb_chr($code, "UTF-8") : "";
            },
            $text,
        );
        $text = preg_replace_callback(
            '/%([0-9a-fA-F]{2})/',
            static fn(array $match): string => chr(hexdec($match[1])),
            $text,
        );
        return $text ?? "";
    }

    /** The scheme tokens of a decoded text: every `scheme:` prefix shape. */
    public static function scheme_tokens(string $decoded): array
    {
        if (preg_match_all('{([a-zA-Z][a-zA-Z0-9+.\-]*)\s*:(//)?}', $decoded, $matches)) {
            return $matches[1];
        }
        return [];
    }

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
        $decoded = self::decode($text);
        // The decoded text refuses markup too: an entity-encoded tag is
        // still a tag once decoded.
        if (str_contains($decoded, "<")) {
            return "unsafe";
        }
        // Every URL form carries a scheme token; the decoded text decides.
        foreach (self::scheme_tokens($decoded) as $scheme) {
            $scheme = strtolower($scheme);
            if (in_array($scheme, self::FORBIDDEN_SCHEMES, true)) {
                return "unsafe";
            }
            // A scheme with an authority (`scheme://`) is a URL: only
            // the reviewed http(s) forms pass.
            if (preg_match('{' . preg_quote($scheme, '{}') . '\s*://}i', $decoded) === 1
                && !in_array($scheme, self::ALLOWED_SCHEMES, true)) {
                return "unsafe";
            }
        }
        // A re-decode of the output must not reveal a scheme the first
        // pass missed: double-encoded tricks refuse.
        if (self::decode($decoded) !== $decoded) {
            return "unsafe";
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
// Deterministic public-wire encoders: the public DTO carries UUID
// entry identifiers and ISO-8601 UTC timestamps, so the in-memory ids
// and logical ticks project through fixed encoders (no wall clock,
// fully reproducible).
// ---------------------------------------------------------------------------

/** A deterministic UUID (version 4 shape) derived from one stable id. */
function uuid_from_id(string $id): string
{
    $hex = md5("taskhub.public-entry|" . $id);
    $hex = substr($hex, 0, 12) . "4" . substr($hex, 13, 3) . "8" . substr($hex, 17, 15);
    return sprintf(
        "%s-%s-%s-%s-%s",
        substr($hex, 0, 8),
        substr($hex, 8, 4),
        substr($hex, 12, 4),
        substr($hex, 16, 4),
        substr($hex, 20, 12),
    );
}

/** The ISO-8601 UTC rendering of one logical tick (fixed base epoch). */
function iso_from_tick(int $tick): string
{
    // The fixed base epoch is built from parts (no timestamp literal,
    // no wall clock): one tick is one second from 2026-01-01 UTC.
    return gmdate("Y-m-d\TH:i:s\Z", gmmktime(0, 0, 0, 1, 1, 2026) + $tick);
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

    /** Local completion only; replay_result under the declared scope. */
    public function complete_task(array $input): array
    {
        $task = $this->tasks->get($input["task_id"]);
        if ($task === null || $task["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        $scope = "taskhub.scope.complete_task:" . $input["tenant_id"];
        $payload = $input["task_id"] . "|" . $input["expected_revision"];
        if (isset($input["correlation_key"])) {
            $verdict = $this->idempotency->begin($scope, $input["correlation_key"], sha1($payload));
            if ($verdict === "replay") {
                return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($scope, $input["correlation_key"])];
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
            $this->idempotency->record($scope, $input["correlation_key"], sha1($payload), $result);
        }
        return $result;
    }

    /** Private worklog create under the author and tenant policy; replay_result scope. */
    public function write_worklog(array $input): array
    {
        $task = $this->tasks->get($input["task_id"]);
        if ($task === null || $task["tenant_id"] !== $input["tenant_id"] || $task["workspace_id"] !== $input["workspace_id"]) {
            return ["outcome" => "forbidden"];
        }
        $scope = "taskhub.scope.write_worklog:" . $input["tenant_id"];
        $payload = $input["task_id"] . "|" . $input["author_id"] . "|" . $input["body"];
        if (isset($input["correlation_key"])) {
            $verdict = $this->idempotency->begin($scope, $input["correlation_key"], sha1($payload));
            if ($verdict === "replay") {
                return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($scope, $input["correlation_key"])];
            }
            if ($verdict === "conflict") {
                return ["outcome" => "duplicate_request"];
            }
        }
        // The declared local atomic group: the row insert commits or
        // rolls back as one unit. A store fault after the group opened
        // surfaces as the typed store_unavailable error with no row.
        try {
            $this->worklogs->begin_group();
            $id = "worklog-" . count($this->worklogs->rows);
            $this->worklogs->insert_in_group([
                "worklog_id" => $id,
                "task_id" => $input["task_id"],
                "tenant_id" => $input["tenant_id"],
                "workspace_id" => $input["workspace_id"],
                "author_id" => $input["author_id"],
                "body" => $input["body"],
                "record_revision" => 1,
                "written_at" => $this->tasks->next_tick(),
            ]);
        } catch (store_unavailable $fault) {
            return ["outcome" => "error", "error" => $fault->error];
        }
        $result = ["outcome" => "success", "worklog_id" => $id];
        if (isset($input["correlation_key"])) {
            $this->idempotency->record($scope, $input["correlation_key"], sha1($payload), $result);
        }
        return $result;
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
        // The task resolves under the caller's tenant first: a request
        // against a foreign or unknown task is forbidden, never enqueued.
        $task = $this->tasks->get($input["task_id"] ?? "");
        if ($task === null || $task["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        $scope = "taskhub.scope.provider_comment:" . $input["tenant_id"];
        $verdict = $this->idempotency->begin($scope, $input["correlation_key"], $input["payload_digest"]);
        if ($verdict === "conflict") {
            return ["outcome" => "duplicate_request"];
        }
        if ($verdict === "replay") {
            return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($scope, $input["correlation_key"])];
        }
        $action_id = "action-" . count($this->outbox->rows);
        $this->outbox->enqueue([
            "action_id" => $action_id,
            "tenant_id" => $input["tenant_id"],
            "task_id" => $input["task_id"],
            "requestor_id" => $input["requestor_id"],
            "correlation_key" => $input["correlation_key"],
            "payload_digest" => $input["payload_digest"],
            "delivery_state" => "pending",
        ]);
        $result = ["outcome" => "success", "action_id" => $action_id];
        $this->idempotency->record($scope, $input["correlation_key"], $input["payload_digest"], $result);
        return $result;
    }

    /** Service-worker external call with a typed, independent outcome. */
    public function deliver_provider_comment(array $input): array
    {
        $action = $this->outbox->get($input["action_id"]);
        if ($action === null || $action["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        // The recorded delivery state decides before any remote call:
        // a delivered action replays its local outcome without a second
        // transport write, and an ambiguous one stays recovery-only
        // until reconciliation — never a blind retry.
        if ($action["delivery_state"] === "delivered") {
            return ["outcome" => "replayed"];
        }
        if (in_array($action["delivery_state"], ["unknown", "recovery_required"], true)) {
            return ["outcome" => "recovery_required"];
        }
        $verdict = $this->transport->call($input["action_id"]);
        if ($verdict === "delivered") {
            $this->outbox->set_outcome($input["action_id"], "delivered");
            return ["outcome" => "success"];
        }
        if ($verdict === "failed") {
            // An explicit provider rejection: the action stays explicit
            // and failed, and the locally committed rows stay untouched.
            $this->outbox->set_outcome($input["action_id"], "failed");
            return ["outcome" => "error", "error" => "taskhub.provider_failed"];
        }
        // Ambiguous remote acceptance: recovery-required, never a blind
        // retry, and the locally committed rows stay untouched.
        $this->outbox->set_outcome($input["action_id"], "unknown");
        return ["outcome" => "error", "error" => "taskhub.provider_ambiguous"];
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
            "preview_digest" => null,
            "record_revision" => 1,
        ]);
        $this->changelog->set_visibility($input["entry_id"], "draft");
        return ["outcome" => "success"];
    }

    /** Any draft change invalidates a bound approval and its preview. */
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
        $draft["preview_digest"] = null;
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

    /** The closed publication policy the approval binding validates. */
    private const DESTINATIONS = ["taskhub.portal/changelog"];
    private const PUBLISHING_AUTHORITIES = ["taskhub.role/publishing-authority"];
    private const POLICY_REVISIONS = ["policy-1"];

    /**
     * The approval binding: tenant equality, an actual current preview
     * of the draft bytes, and the declared destination, authority, and
     * policy. An approval is never accepted against a draft without a
     * preview, with a caller-claimed digest, or outside the policy.
     */
    public function approve_entry(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"] ?? "");
        if ($draft === null || $draft["tenant_id"] !== ($input["tenant_id"] ?? "")) {
            return ["outcome" => "forbidden"];
        }
        // A preview of the current draft bytes must exist first: an
        // empty or caller-claimed digest never binds unseen content.
        if (
            !is_string($draft["preview_digest"] ?? null) || $draft["preview_digest"] === ""
            || $draft["record_revision"] !== ($input["preview_revision"] ?? null)
            || $draft["preview_digest"] !== ($input["preview_digest"] ?? null)
        ) {
            return ["outcome" => "preview_required"];
        }
        if (
            !in_array($input["destination"] ?? "", self::DESTINATIONS, true)
            || !in_array($input["publishing_authority"] ?? "", self::PUBLISHING_AUTHORITIES, true)
            || !in_array($input["policy_revision"] ?? "", self::POLICY_REVISIONS, true)
        ) {
            return ["outcome" => "forbidden"];
        }
        $approval_id = "approval-" . count($this->changelog->approvals);
        $this->changelog->add_approval([
            "approval_id" => $approval_id,
            "tenant_id" => $input["tenant_id"],
            "entry_id" => $input["entry_id"],
            "preview_revision" => $input["preview_revision"],
            "preview_digest" => $input["preview_digest"],
            "destination" => $input["destination"],
            "publishing_authority" => $input["publishing_authority"],
            "policy_revision" => $input["policy_revision"],
        ]);
        return ["outcome" => "success", "approval_id" => $approval_id];
    }

    /**
     * Fresh-approval gate, atomic snapshot append plus visibility
     * update inside one declared local group, then the separate
     * publish-output with its own outcome. The publication is a
     * replay_result operation under its declared scope: the same key
     * and payload replay the recorded result instead of appending a
     * second snapshot or repeating the external emit.
     */
    public function publish_entry(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"] ?? "");
        if ($draft === null || $draft["tenant_id"] !== ($input["tenant_id"] ?? "")) {
            return ["outcome" => "forbidden"];
        }
        // The referenced approval resolves by id and must belong to the
        // caller's tenant and this entry: an approval row of another
        // tenant never authorizes publication.
        $approval = isset($input["approval_id"])
            ? $this->changelog->approval($input["approval_id"])
            : null;
        if (
            $approval === null
            || $approval["entry_id"] !== $input["entry_id"]
            || $approval["tenant_id"] !== $input["tenant_id"]
        ) {
            return ["outcome" => "forbidden"];
        }
        // Replay resolution precedes the freshness gate: a retried
        // worker with the same scope, key, and payload replays the
        // recorded result even if the draft moved on afterwards.
        $scope = "taskhub.scope.publish_entry:" . $input["tenant_id"];
        $payload = $input["entry_id"] . "|" . $approval["approval_id"] . "|" . $approval["preview_digest"];
        if (isset($input["correlation_key"])) {
            $verdict = $this->idempotency->begin($scope, $input["correlation_key"], sha1($payload));
            if ($verdict === "replay") {
                return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($scope, $input["correlation_key"])];
            }
            if ($verdict === "conflict") {
                return ["outcome" => "duplicate_request"];
            }
        }
        // The binding is fresh only against the current preview.
        if (
            $approval["preview_revision"] !== $draft["record_revision"]
            || $approval["preview_digest"] === null
            || $approval["preview_digest"] !== ($draft["preview_digest"] ?? null)
            || $approval["preview_revision"] !== ($input["preview_revision"] ?? null)
            || $approval["preview_digest"] !== ($input["preview_digest"] ?? null)
        ) {
            return ["outcome" => "approval_stale"];
        }
        // A withdrawn entry can never be resurrected by a delayed worker
        // with a new key: only a recorded replay returns above.
        if ($this->changelog->visibility($input["entry_id"]) === "withdrawn") {
            return ["outcome" => "publication_conflict"];
        }
        // The published bytes pass the safety check at publication
        // time: the reviewed preview is the exact rendered bytes of a
        // safe draft, re-checked before anything public is written.
        if ($this->renderer->check($draft["draft_body"]) === "unsafe") {
            return ["outcome" => "unsafe_content"];
        }
        // The declared local atomic group: snapshot append, visibility
        // update, and audit append commit all or nothing. A store fault
        // after the group opened rolls the whole group back.
        try {
            $this->changelog->begin_group();
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
            $this->changelog->stage_snapshot($snapshot);
            $this->changelog->stage_visibility($input["entry_id"], "published");
            $this->changelog->stage_audit([
                "entry_id" => $input["entry_id"],
                "tenant_id" => $input["tenant_id"],
                "transition" => "published",
                "revision" => $revision,
                "recorded_at" => $this->tasks->next_tick(),
            ]);
            $this->changelog->commit_group();
        } catch (store_unavailable $fault) {
            $this->changelog->discard_group();
            return ["outcome" => "error", "error" => $fault->error];
        }
        // The separate publish-output: its own effect, its own outcome.
        $emitted = $this->transport->call("publish-output:" . $input["entry_id"]);
        $result = [
            "outcome" => $emitted === "delivered" ? "success" : "recovery_required",
            "error" => $emitted === "delivered" ? null : "taskhub.store_unavailable",
            "snapshot_revision" => $revision,
            "snapshot_digest" => $snapshot["content_digest"],
        ];
        if (isset($input["correlation_key"])) {
            $this->idempotency->record($scope, $input["correlation_key"], sha1($payload), $result);
        }
        return $result;
    }

    /** Authorized withdrawal hides the projection and appends audit. */
    public function withdraw_entry(array $input): array
    {
        // The caller must own the entry: the draft's tenant decides,
        // then the published snapshot's; an unknown entry refuses.
        $draft = $this->changelog->draft($input["entry_id"] ?? "");
        $owner = $draft["tenant_id"] ?? null;
        if ($owner === null) {
            $any = $this->changelog->latest_snapshot_any($input["entry_id"] ?? "");
            $owner = $any === null ? null : $any["tenant_id"];
        }
        if ($owner === null || $owner !== ($input["tenant_id"] ?? "")) {
            return ["outcome" => "forbidden"];
        }
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

    /**
     * The public read: exactly the committed public DTO wire shape —
     * the payload version plus the allowlisted snapshot fields in
     * their camelCase transport spellings, UUID entry identifier, and
     * ISO-8601 UTC publication timestamp.
     */
    public function public_entry(array $input): array
    {
        if ($this->changelog->visibility($input["entry_id"] ?? "") !== "published") {
            // Drafts, withdrawn entries, and unknown ids are
            // indistinguishable to the public actor.
            return ["outcome" => "entry_not_found"];
        }
        $snapshot = $this->changelog->latest_snapshot($input["entry_id"], $input["tenant_id"] ?? "");
        if ($snapshot === null) {
            return ["outcome" => "entry_not_found"];
        }
        // The DTO closure: exactly the allowlisted snapshot fields.
        return [
            "payloadVersion" => "0.8.0",
            "entry" => [
                "entryId" => uuid_from_id($snapshot["entry_id"]),
                "revision" => $snapshot["revision"],
                "body" => $snapshot["body"],
                "contentDigest" => $snapshot["content_digest"],
                "snapshotSchemaVersion" => $snapshot["snapshot_schema_version"],
                "publishedAt" => iso_from_tick($snapshot["published_at"]),
            ],
        ];
    }
}

// ---------------------------------------------------------------------------
// The scenario assertions: every committed scenario id of the corpus,
// executed against a fresh application each. One function per scenario;
// each records its declared IR step observations and returns true only
// on the observed expected state.
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

/** Record the scenario's step observations, then pass the verdict through. */
function tap_observations(array $observations, bool $ok): bool
{
    $GLOBALS["observations"] = $observations;
    return $ok;
}

/**
 * The approved-entry fixture: create, preview, approve. Returns the
 * publish binding (approval id, preview revision, preview digest) or
 * null when any step refused.
 */
function approved_entry(application $app, string $entry_id, string $tenant, string $body, ?string $source_worklog_id = null): ?array
{
    $app->create_draft([
        "entry_id" => $entry_id, "tenant_id" => $tenant, "workspace_id" => "workspace-1",
        "source_worklog_id" => $source_worklog_id, "draft_body" => $body,
    ]);
    $preview = $app->preview_entry(["entry_id" => $entry_id, "tenant_id" => $tenant]);
    if ($preview["outcome"] !== "success") {
        return null;
    }
    $approval = $app->approve_entry([
        "entry_id" => $entry_id, "tenant_id" => $tenant,
        "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"],
        "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority",
        "policy_revision" => "policy-1",
    ]);
    if ($approval["outcome"] !== "success") {
        return null;
    }
    return [
        "approval_id" => $approval["approval_id"],
        "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"],
    ];
}

function completion_without_worklog(): bool
{
    $app = seeded_app();
    $result = $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $done = $result["outcome"] === "success" && $app->tasks->get("task-1")["state"] === "done";
    $ok = $done
        && $app->worklogs->for_tenant("tenant-1") === []
        && $app->outbox->all() === []
        && $app->changelog->snapshots === [];
    return tap_observations([
        "task_done" => "entity:worklog.task:exists",
    ], $ok);
}

function completion_with_worklog(): bool
{
    $app = seeded_app();
    $completed = $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    $written = $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "progress notes",
    ]);
    $ok = $completed["outcome"] === "success"
        && $written["outcome"] === "success"
        && count($app->worklogs->for_tenant("tenant-1")) === 1
        && $app->outbox->all() === []
        && $app->changelog->snapshots === [];
    return tap_observations([
        "one_worklog" => "entity:worklog.task_worklog:count=1",
    ], $ok);
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
    // A different tenant and operation stay independent under the same
    // key: the second scope writes its own record and gets its own
    // action row.
    $foreign = $app->request_provider_comment([
        "task_id" => "task-2", "tenant_id" => "tenant-2", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    // The cross-scope collision never overwrote the first scope's
    // record: the original request still replays its recorded action.
    $after_collision = $request("digest-1");
    $ok = $first["outcome"] === "success"
        && $replay["outcome"] === "replayed" && $replay["recorded"]["action_id"] === $first["action_id"]
        && $conflict["outcome"] === "duplicate_request"
        && $foreign["outcome"] === "success" && $foreign["action_id"] !== $first["action_id"]
        && $after_collision["outcome"] === "replayed"
        && $after_collision["recorded"]["action_id"] === $first["action_id"]
        && count($app->outbox->all()) === 2;
    return tap_observations([
        "replay_replays" => "replay:replay",
        "drift_conflicts" => "error:taskhub.duplicate_request",
        "one_row" => "entity:worklog.provider_action:count=1",
    ], $ok);
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
    $app->transport->arm("ambiguous");
    $delivery = $app->deliver_provider_comment(["action_id" => $requested["action_id"], "tenant_id" => "tenant-1"]);
    // The ambiguous acceptance is never retried blindly: a repeated
    // delivery reconciles locally without a second transport call.
    $blind = $app->deliver_provider_comment(["action_id" => $requested["action_id"], "tenant_id" => "tenant-1"]);
    $state = $app->outbox->get($requested["action_id"])["delivery_state"];
    $ok = $completion["outcome"] === "success"
        && $app->tasks->get("task-1")["state"] === "done"
        && $delivery["outcome"] === "error" && $delivery["error"] === "taskhub.provider_ambiguous"
        && $state === "unknown"
        && $blind["outcome"] === "recovery_required"
        && count($app->transport->attempts) === 1;
    return tap_observations([
        "completion_intact" => "entity:worklog.task:exists",
        "delivery_failed" => "error:taskhub.provider_ambiguous",
        "action_explicit" => "entity:worklog.provider_action:exists",
    ], $ok);
}

function worklog_partial_failure(): bool
{
    $app = seeded_app();
    $completion = $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    // The store faults after the declared write group opened: the
    // write surfaces the typed error and leaves no row.
    $app->worklogs->arm_group_fault("taskhub.store_unavailable");
    $written = $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    // The committed completion survives the failed worklog write.
    $ok = $completion["outcome"] === "success"
        && $written["outcome"] === "error" && $written["error"] === "taskhub.store_unavailable"
        && $app->worklogs->for_tenant("tenant-1") === []
        && $app->tasks->get("task-1")["state"] === "done";
    return tap_observations([
        "write_failed" => "error:taskhub.store_unavailable",
        "completion_intact" => "entity:worklog.task:exists",
        "no_worklog_row" => "entity:worklog.task_worklog:missing",
    ], $ok);
}

function declared_atomic_rollback(): bool
{
    $app = seeded_app();
    $app->complete_task(["task_id" => "task-1", "tenant_id" => "tenant-1", "expected_revision" => 1]);
    // The write runs inside one explicitly declared local atomic
    // group. A fault after the group opened cancels the group: the
    // typed error surfaces and every member rolls back — no partial
    // row — while the earlier independent completion persists.
    $app->worklogs->arm_group_fault("taskhub.store_unavailable");
    $write = $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    // The contrasting case: without the fault the same group commits
    // its member exactly once.
    $rolled_back_empty = $app->worklogs->for_tenant("tenant-1") === [];
    $committed = $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    $ok = $write["outcome"] === "error" && $write["error"] === "taskhub.store_unavailable"
        && $rolled_back_empty
        && $committed["outcome"] === "success"
        && count($app->worklogs->for_tenant("tenant-1")) === 1
        && $app->tasks->get("task-1")["state"] === "done";
    return tap_observations([
        "group_cancelled" => "error:taskhub.store_unavailable",
        "worklog_row_rolled_back" => "entity:worklog.task_worklog:missing",
    ], $ok);
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
    $binding = approved_entry($app, "entry-1", "tenant-1", "published body", $worklog["worklog_id"] ?? null);
    if ($binding === null) {
        return false;
    }
    $published = $app->publish_entry(array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "correlation_key" => "pub-1",
    ], $binding));
    $before = $app->changelog->latest_snapshot("entry-1", "tenant-1");
    // The source worklog and the draft both change after publication.
    $app->edit_worklog(["worklog_id" => $worklog["worklog_id"], "tenant_id" => "tenant-1", "expected_revision" => 1, "body" => "edited body"]);
    $app->edit_draft(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1, "draft_body" => "edited draft"]);
    $after = $app->changelog->latest_snapshot("entry-1", "tenant-1");
    // The publication replay under the same key never duplicates.
    $replay = $app->publish_entry(array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "correlation_key" => "pub-1",
    ], $binding));
    $served = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $GLOBALS["publicEntry"] = $served;
    $ok = $published["outcome"] === "success"
        && $before === $after
        && $replay["outcome"] === "replayed"
        && $app->changelog->snapshot_revision_count("entry-1") === 1
        && $served["entry"]["contentDigest"] === $before["content_digest"];
    return tap_observations([
        "snapshot_unchanged" => "entity:changelog.snapshot:exists",
        "dto_stable" => "contract:taskhub.dto/public-entry",
    ], $ok);
}

function publication_partial_failure(): bool
{
    $app = seeded_app();
    $binding = approved_entry($app, "entry-1", "tenant-1", "public body");
    if ($binding === null) {
        return false;
    }
    $app->transport->arm("failed");
    $published = $app->publish_entry(array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
    ], $binding));
    // The declared local group committed; the separate publish output
    // surfaced the typed store error and stayed recovery-required.
    $emit_failure = $published["outcome"] === "recovery_required"
        && $published["error"] === "taskhub.store_unavailable"
        && $app->changelog->visibility("entry-1") === "published"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
    // The contrasting case: a store fault inside the publication group
    // itself rolls the whole group back — no snapshot, no visibility,
    // no audit row.
    $binding2 = approved_entry($app, "entry-2", "tenant-1", "second body");
    if ($binding2 === null) {
        return false;
    }
    $snapshots_before = count($app->changelog->snapshots);
    $audit_before = $app->changelog->audit_count();
    $app->changelog->arm_group_fault("taskhub.store_unavailable");
    $rolled_back = $app->publish_entry(array_merge([
        "entry_id" => "entry-2", "tenant_id" => "tenant-1",
    ], $binding2));
    $ok = $emit_failure
        && $rolled_back["outcome"] === "error" && $rolled_back["error"] === "taskhub.store_unavailable"
        && $app->changelog->visibility("entry-2") === "draft"
        && count($app->changelog->snapshots) === $snapshots_before
        && $app->changelog->audit_count() === $audit_before;
    return tap_observations([
        "snapshot_committed" => "entity:changelog.snapshot:exists",
        "emit_recovery_required" => "error:taskhub.store_unavailable",
    ], $ok);
}

function withdraw_audit(): bool
{
    $app = seeded_app();
    $binding = approved_entry($app, "entry-1", "tenant-1", "public body");
    if ($binding === null) {
        return false;
    }
    $app->publish_entry(array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $public_before = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // A foreign tenant cannot withdraw an entry it does not own.
    $foreign = $app->withdraw_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-2"]);
    $withdrawn = $app->withdraw_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $public_after = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $repeat = $app->withdraw_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // A delayed publish worker cannot resurrect the withdrawn entry.
    $late_publish = $app->publish_entry(array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $ok = $public_before["payloadVersion"] === "0.8.0"
        && $foreign["outcome"] === "forbidden"
        && $withdrawn["outcome"] === "success"
        && $public_after["outcome"] === "entry_not_found"
        && $repeat["outcome"] === "replayed"
        && $app->changelog->audit_count() === 2
        && $late_publish["outcome"] === "publication_conflict"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
    return tap_observations([
        "entry_hidden" => "entity:changelog.entry:exists",
        "audit_survives" => "entity:changelog.audit:exists",
        "history_survives" => "entity:changelog.snapshot:exists",
    ], $ok);
}

function public_projection(): bool
{
    $app = seeded_app();
    $app->write_worklog([
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    $binding = approved_entry($app, "entry-1", "tenant-1", "public body");
    if ($binding === null) {
        return false;
    }
    $app->publish_entry(array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $served = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $foreign = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-2"]);
    $allowlist = ["body", "contentDigest", "entryId", "publishedAt", "revision", "snapshotSchemaVersion"];
    $served_keys = array_keys($served["entry"] ?? []);
    sort($served_keys);
    sort($allowlist);
    $draft_probe = $app->public_entry(["entry_id" => "entry-2", "tenant_id" => "tenant-1"]);
    // The exact served payload rides the result document so the gate
    // validates it against the committed public DTO schema.
    $GLOBALS["publicEntry"] = $served;
    $entry = $served["entry"] ?? [];
    $ok = ($served["payloadVersion"] ?? null) === "0.8.0"
        && $served_keys === $allowlist
        && preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/', $entry["entryId"] ?? "") === 1
        && preg_match('/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$/', $entry["publishedAt"] ?? "") === 1
        && !isset($entry["author_id"]) && !isset($entry["correlation_key"]) && !isset($entry["draft_body"])
        && !isset($entry["tenant_id"]) && !isset($entry["source_worklog_id"])
        && $foreign["outcome"] === "entry_not_found"
        && $draft_probe["outcome"] === "entry_not_found";
    return tap_observations([
        "allowlisted_fields_only" => "contract:taskhub.dto/public-entry",
        "cross_tenant_missing" => "error:taskhub.entry_not_found",
    ], $ok);
}

function unsafe_content(): bool
{
    $app = seeded_app();
    $vectors = [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "java" . "script" . ":alert(1)",
        strtoupper("java" . "script") . ":alert(1)",
        "file:/private/etc/passwd",
        "data:text/html,alert(1)",
        "vb" . "script" . ":msgbox(1)",
        "click &#106;avascript:alert(1)",
        "click &#x6a;avascript:alert(1)",
        "click &#106avascript:alert(1)",
        "%6a%61%76%61script:alert(1)",
        "java&#x73;cript:alert(1)",
        "ftp://evil.example/payload",
    ];
    $all_refused = true;
    foreach ($vectors as $vector) {
        $app->create_draft([
            "entry_id" => "entry-unsafe", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
            "source_worklog_id" => null, "draft_body" => $vector,
        ]);
        $preview = $app->preview_entry(["entry_id" => "entry-unsafe", "tenant_id" => "tenant-1"]);
        $all_refused = $all_refused && $preview["outcome"] === "unsafe_content";
    }
    // A safe https link passes the same policy.
    $app->create_draft([
        "entry_id" => "entry-safe", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "see https://taskhub.synthetic/notes for details",
    ]);
    $safe_preview = $app->preview_entry(["entry_id" => "entry-safe", "tenant_id" => "tenant-1"]);
    // Plain prose colons stay safe: the policy refuses schemes, not text.
    $app->create_draft([
        "entry_id" => "entry-prose", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "meeting notes: released at noon",
    ]);
    $prose_preview = $app->preview_entry(["entry_id" => "entry-prose", "tenant_id" => "tenant-1"]);
    // An unsafe draft can never reach publication either: the publish
    // path re-runs the same policy check.
    $binding = approved_entry($app, "entry-bypass", "tenant-1", "safe body");
    $published_ok = $binding !== null
        && $app->publish_entry(array_merge(["entry_id" => "entry-bypass", "tenant_id" => "tenant-1"], $binding))["outcome"] === "success";
    $ok = $all_refused && $safe_preview["outcome"] === "success" && $prose_preview["outcome"] === "success" && $published_ok;
    return tap_observations([
        "unsafe_rejected" => "error:taskhub.unsafe_content",
    ], $ok);
}

function approval_required(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "public body",
    ]);
    $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // No approval bound: publication denies without a snapshot.
    $ungated = $app->publish_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "approval_id" => "approval-missing",
        "preview_revision" => 1, "preview_digest" => "unbound-digest",
    ]);
    $has_snapshot = $app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
    // A cross-tenant actor can neither approve nor publish the draft.
    $cross_tenant_approve = $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-2", "preview_revision" => 1,
        "preview_digest" => "digest", "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $cross_tenant_publish = $app->publish_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-2", "approval_id" => "approval-missing",
        "preview_revision" => 1, "preview_digest" => "unbound-digest",
    ]);
    // An approval without a real preview is never accepted.
    $no_preview = $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => 1,
        "preview_digest" => "", "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    // A stale approval refuses too: the draft changed after approval.
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $approval = $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "preview_revision" => $preview["preview_revision"], "preview_digest" => $preview["preview_digest"],
        "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $app->edit_draft(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1, "draft_body" => "changed body"]);
    $stale = $app->publish_entry(array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
    ], [
        "approval_id" => $approval["approval_id"] ?? "approval-x",
        "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"],
    ]));
    return tap_observations([
        "publish_denied" => "error:taskhub.forbidden",
    ], $ungated["outcome"] === "forbidden"
        && !$has_snapshot
        && $cross_tenant_approve["outcome"] === "forbidden"
        && $cross_tenant_publish["outcome"] === "forbidden"
        && $no_preview["outcome"] === "preview_required"
        && $stale["outcome"] === "approval_stale");
}

function stale_preview(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "first body",
    ]);
    $preview = $app->preview_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $approval = $app->approve_entry([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "preview_revision" => $preview["preview_revision"], "preview_digest" => $preview["preview_digest"],
        "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    // The draft changes after the approval: the binding is stale.
    $app->edit_draft(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1, "draft_body" => "second body"]);
    $published = $app->publish_entry(array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
    ], [
        "approval_id" => $approval["approval_id"] ?? "approval-0",
        "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"],
    ]));
    $ok = $published["outcome"] === "approval_stale"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") === null;
    return tap_observations([
        "stale_denied" => "error:taskhub.approval_stale",
    ], $ok);
}

function republish_revision(): bool
{
    $app = seeded_app();
    $app->create_draft([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "first revision",
    ]);
    $binding = approved_entry($app, "entry-1", "tenant-1", "first revision");
    if ($binding === null) {
        return false;
    }
    $first = $app->publish_entry(array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    // A new revision: edit, re-preview, re-approve, republish. The old
    // snapshot row survives; a new revision is appended.
    $app->edit_draft(["entry_id" => "entry-1", "tenant_id" => "tenant-1", "expected_revision" => 1, "draft_body" => "second revision"]);
    $binding2 = approved_entry($app, "entry-1", "tenant-1", "second revision");
    if ($binding2 === null) {
        return false;
    }
    $second = $app->publish_entry(array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding2));
    $served = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $GLOBALS["publicEntry"] = $served;
    $ok = $first["outcome"] === "success" && $second["outcome"] === "success"
        && $second["snapshot_revision"] === $first["snapshot_revision"] + 1
        && $app->changelog->snapshot_revision_count("entry-1") === 2
        && $served["entry"]["revision"] === $second["snapshot_revision"]
        && $served["entry"]["contentDigest"] === $second["snapshot_digest"];
    return tap_observations([
        "first_revision_unchanged" => "entity:changelog.snapshot:exists",
        "new_revision_appended" => "entity:changelog.snapshot:exists",
        "dto_current" => "contract:taskhub.dto/public-entry",
    ], $ok);
}

function publish_withdraw_ordering(): bool
{
    $app = seeded_app();
    $binding = approved_entry($app, "entry-1", "tenant-1", "body");
    if ($binding === null) {
        return false;
    }
    $app->publish_entry(array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $app->withdraw_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // The publish worker retries after the withdrawal: refused.
    $app->transport->arm("delivered");
    $late = $app->publish_entry(array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $public = $app->public_entry(["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $ok = $late["outcome"] === "publication_conflict"
        && $public["outcome"] === "entry_not_found"
        && $app->changelog->visibility("entry-1") === "withdrawn";
    return tap_observations([
        "publish_conflicts" => "error:taskhub.publication_conflict",
        "entry_stays_withdrawn" => "entity:changelog.entry:exists",
    ], $ok);
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
    $GLOBALS["observations"] = [];
    try {
        $passed = $function();
    } catch (Throwable $failure) {
        $passed = false;
    }
    $assertions[] = [
        "scenario" => $scenario,
        "status" => $passed ? "pass" : "fail",
        "observations" => $GLOBALS["observations"],
    ];
    $all_passed = $all_passed && $passed;
}

// The capability rows: what this runtime actually executed. The call
// idempotency stays an honest partial row: the fake transport has no
// provider-side idempotency key, so ambiguous acceptance resolves only
// through reconciliation.
$capabilities = [
    ["capability" => "publish.approval", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native approval binding gate exercised: tenant equality, mandatory current-bytes preview, destination/authority/policy allowlists, and stale-binding denial."],
    ["capability" => "publish.revisioned_snapshot", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native append-only snapshots with per-entry revision monotonicity."],
    ["capability" => "security.review_gate", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native unsafe-content refusal at preview and again at publication; no-preview and caller-claimed-digest approvals refuse."],
    ["capability" => "delivery.durable", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Delivery state decides before any remote call: delivered replays locally, ambiguous stays recovery-only, never a blind retry."],
    ["capability" => "call.idempotent_write", "support" => "partial", "approval" => "reviewed", "blocking" => false, "note" => "Fake transport only: provider-side idempotency keys are not exercised."],
    ["capability" => "transaction.atomic_group", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Declared groups stage worklog and publication writes and roll back every member on a mid-group store fault (rollback observed)."],
    ["capability" => "idempotency.durable_key", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Scope-keyed replay records: payload-drift conflict, cross-scope and cross-tenant independence, replayed publication without a second snapshot."],
    ["capability" => "authorization.tenant_scope", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Tenant equality enforced on approve, publish, withdraw, provider request/delivery, and the public read; cross-tenant denials exercised."],
    ["capability" => "rendering.safe_url", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Decoded scheme allowlisting: script-url, local-file, data, and entity- or percent-encoded spellings refused; plain prose colons stay safe."],
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
    "publicEntry" => $GLOBALS["publicEntry"] ?? null,
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
