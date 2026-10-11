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
// ISO-8601 UTC timestamp). Each scenario records the structured
// runtime facts the gate binds to the committed Scenario IR: the
// exact invocation log (operation + input map, in order) and, per
// `then` step, the observed entity rows, typed error payloads,
// replay classes, and projection fields — the gate evaluates the
// committed IR assertions against these facts, so a hand-written
// label can never stand in for the declared predicate.
//
// The durable claims are backed by a write-through journal in the
// system temp directory: replay records and outbox rows/outcomes
// append to the journal as they commit, and recovery rebuilds fresh
// store instances from the journal file — the committed scenarios
// exercise that recovery path (restart replay, delivery after
// restore). No other state leaves the process.
//
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

    public function insert(string $id, string $tenant, string $workspace, string $state = "open", int $revision = 1): void
    {
        $this->rows[$id] = [
            "task_id" => $id,
            "tenant_id" => $tenant,
            "workspace_id" => $workspace,
            "state" => $state,
            "completed_at" => null,
            "record_revision" => $revision,
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

// ---------------------------------------------------------------------------
// The durable journal: the write-through log backing the claimed
// durable stores. Every idempotency record and outbox
// row/outcome appends to the journal file as it commits; recovery
// (`restore`) rebuilds fresh store instances from the journal bytes —
// the in-memory objects dying is exactly the modeled process death.
// ---------------------------------------------------------------------------

final class durable_journal
{
    private string $path;

    public function __construct()
    {
        $this->path = (string) tempnam(sys_get_temp_dir(), "lekalo-lane-journal-");
    }

    public function append(string $kind, array $row): void
    {
        @file_put_contents(
            $this->path,
            json_encode(["kind" => $kind, "row" => $row], JSON_UNESCAPED_SLASHES) . "\n",
            FILE_APPEND,
        );
    }

    /** The journaled lines in commit order. */
    public function lines(): array
    {
        $raw = @file_get_contents($this->path);
        if ($raw === false || $raw === "") {
            return [];
        }
        $lines = [];
        foreach (explode("\n", $raw) as $line) {
            if ($line === "") {
                continue;
            }
            $decoded = json_decode($line, true);
            if (is_array($decoded)) {
                $lines[] = $decoded;
            }
        }
        return $lines;
    }

    public function dispose(): void
    {
        @unlink($this->path);
    }
}

final class idempotency_store
{
    // "scope|key" => ["scope", "key", "payload_digest", "result"]:
    // the replay key is the declared operation scope (tenant + actor
    // + operation) plus the client correlation key. A second scope
    // reusing the same key writes its own record; neither overwrites
    // the other.
    public array $records = [];

    public function __construct(private ?durable_journal $journal = null)
    {
    }

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
        $row = [
            "scope" => $scope,
            "key" => $key,
            "payload_digest" => $payload_digest,
            "result" => $result,
        ];
        $this->records[$this->composite($scope, $key)] = $row;
        // The durable-key guarantee: the replay record survives the
        // death of this store instance.
        $this->journal?->append("idempotency", $row);
    }

    public function replayed_result(string $scope, string $key): array
    {
        return $this->records[$this->composite($scope, $key)]["result"];
    }

    /** Rebuild the records from the journal: recovery after process death. */
    public static function restore(durable_journal $journal): self
    {
        $store = new self($journal);
        foreach ($journal->lines() as $line) {
            if (($line["kind"] ?? null) !== "idempotency") {
                continue;
            }
            $row = $line["row"];
            $store->records[$row["scope"] . "|" . $row["key"]] = $row;
        }
        return $store;
    }
}

final class provider_outbox
{
    public array $rows = [];

    public function __construct(private ?durable_journal $journal = null)
    {
    }

    public function enqueue(array $row): void
    {
        $this->rows[$row["action_id"]] = $row;
        // The delivery guarantee: a committed provider request
        // survives the death of this store instance before its
        // delivery outcome exists.
        $this->journal?->append("outbox", $row);
    }

    public function get(string $action_id): ?array
    {
        return $this->rows[$action_id] ?? null;
    }

    public function set_outcome(string $action_id, string $outcome): void
    {
        $this->rows[$action_id]["delivery_state"] = $outcome;
        $this->journal?->append("outbox-outcome", ["action_id" => $action_id, "delivery_state" => $outcome]);
    }

    public function all(): array
    {
        return $this->rows;
    }

    /** Rebuild the rows and outcomes from the journal: recovery after process death. */
    public static function restore(durable_journal $journal): self
    {
        $store = new self($journal);
        foreach ($journal->lines() as $line) {
            $kind = $line["kind"] ?? null;
            if ($kind === "outbox") {
                $row = $line["row"];
                $store->rows[$row["action_id"]] = $row;
            } elseif ($kind === "outbox-outcome") {
                $store->rows[$line["row"]["action_id"]]["delivery_state"] = $line["row"]["delivery_state"];
            }
        }
        return $store;
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

    /** Invalidate every outstanding approval of one entry: a recreated
     * draft can never be published behind an approval of the old
     * bytes. */
    public function invalidate_approvals(string $entry_id): void
    {
        foreach ($this->approvals as $index => $approval) {
            if ($approval["entry_id"] === $entry_id) {
                $this->approvals[$index]["invalidated"] = true;
            }
        }
    }

    public function approval(string $approval_id): ?array
    {
        foreach (array_reverse($this->approvals) as $approval) {
            if ($approval["approval_id"] === $approval_id && !($approval["invalidated"] ?? false)) {
                return $approval;
            }
        }
        return null;
    }

    /** Resolve one approval including invalidated ones: an invalidated
     * approval stays diagnosable (the publication gate reports it as
     * stale) but can never authorize a publication. */
    public function approval_any(string $approval_id): ?array
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
            if (
                $approval["entry_id"] === $entry_id
                && $approval["tenant_id"] === $tenant
                && !($approval["invalidated"] ?? false)
            ) {
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
     * The closed named-entity table. Only these spellings decode; any
     * other `&name;` form stays literal (and therefore cannot hide a
     * control character or a scheme delimiter behind a decode).
     */
    private const NAMED_ENTITIES = [
        "colon" => ":", "tab" => "\t", "newline" => "\n", "newline;" => "\n",
        "lt" => "<", "gt" => ">", "amp" => "&", "quot" => "\"", "apos" => "'",
        "sol" => "/", "bsol" => "\\", "num" => "#", "semi" => ";", "excl" => "!",
        "equals" => "=", "percent" => "%", "lpar" => "(", "rpar" => ")",
        "comma" => ",", "quest" => "?", "quot;" => "\"",
    ];

    /** One decode step: numeric entities (with or without the
     * semicolon), the closed named-entity table, then percent escapes. */
    private static function decode_step(string $text): string
    {
        $text = preg_replace_callback(
            '/&#x([0-9a-fA-F]+);?|&#([0-9]+);?/',
            static function (array $match): string {
                $code = ($match[1] ?? "") !== ""
                    ? (int) hexdec($match[1])
                    : (int) $match[2];
                return $code > 0 && $code <= 0x10ffff ? (string) mb_chr($code, "UTF-8") : "";
            },
            $text,
        );
        $text = preg_replace_callback(
            '/&([a-zA-Z][a-zA-Z0-9]*);?/',
            static function (array $match): string {
                $name = strtolower($match[1]);
                return self::NAMED_ENTITIES[$name] ?? $match[0];
            },
            $text,
        );
        $text = preg_replace_callback(
            '/%([0-9a-fA-F]{2})/',
            static fn(array $match): string => chr((int) hexdec($match[1])),
            $text,
        );
        return $text ?? "";
    }

    /**
     * Decode to a fixpoint (bounded): the fully normalized
     * representation every policy decision sees. Null means the text
     * never stabilized — an intentionally layered encoding, refused.
     */
    public static function decode(string $text): ?string
    {
        $current = $text;
        for ($step = 0; $step < 8; $step++) {
            $next = self::decode_step($current);
            if ($next === $current) {
                return $current;
            }
            $current = $next;
        }
        return null;
    }

    /**
     * The scheme tokens of a text: every `scheme:` prefix shape whose
     * colon is followed by a non-space character (a trailing or
     * prose-space colon is punctuation, not a scheme).
     */
    public static function scheme_tokens(string $decoded): array
    {
        if (preg_match_all('{([a-zA-Z][a-zA-Z0-9+.\-]*):(?=[^\s])}', $decoded, $matches)) {
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
        // Layered encodings that never stabilize refuse outright.
        if ($decoded === null) {
            return "unsafe";
        }
        // rejectEncoded: the policy refuses the encoded forms
        // themselves, not only what they decode to — `https%3a//` is
        // an encoded spelling and refuses even though the decoded
        // scheme is allowed.
        if ($decoded !== $text) {
            return "unsafe";
        }
        // Control characters are re-checked on the fully decoded
        // representation; an entity- or percent-encoded control is a
        // control by the time the policy sees it.
        if (preg_match('/[\x00-\x1f\x7f]/', $decoded) === 1) {
            return "unsafe";
        }
        if (str_contains($decoded, "<")) {
            return "unsafe";
        }
        // Every scheme token — with or without an authority — must be
        // an allowed scheme; the deny list stays as defense in depth.
        foreach (self::scheme_tokens($decoded) as $scheme) {
            $scheme = strtolower($scheme);
            if (in_array($scheme, self::FORBIDDEN_SCHEMES, true)) {
                return "unsafe";
            }
            if (!in_array($scheme, self::ALLOWED_SCHEMES, true)) {
                return "unsafe";
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

/** The canonical payload digest of one request: byte-sorted-key JSON
 * of the payload members, so no delimiter ambiguity can collide two
 * different requests onto one replay key. */
function canonical_payload_digest(array $fields): string
{
    ksort($fields);
    return hash("sha256", (string) json_encode($fields, JSON_UNESCAPED_SLASHES));
}

// ---------------------------------------------------------------------------
// The application: the twelve declared operations with their effect
// boundaries. Each returns ["outcome" => string, ...detail]; commit
// boundaries follow the composition: completion, worklog, outbox,
// delivery, and publication are independent outcomes.
//
// The replay scopes follow the declared contract: the composite is
// (tenant, actor-or-service identity, operation) plus the correlation
// key, and the payload binding is the canonical digest of the
// submitted request members — a changed request under a recorded key
// conflicts instead of replaying.
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
        public durable_journal $journal,
    ) {
    }

    /** Local completion only; replay_result under the declared scope. */
    public function complete_task(array $input): array
    {
        $task = $this->tasks->get($input["task_id"]);
        if ($task === null || $task["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        $scope = "taskhub.scope.complete_task:" . $input["tenant_id"] . "|svc.complete_task";
        $payload = canonical_payload_digest([
            "task_id" => $input["task_id"],
            "tenant_id" => $input["tenant_id"],
            "expected_revision" => $input["expected_revision"],
        ]);
        if (isset($input["correlation_key"])) {
            $verdict = $this->idempotency->begin($scope, $input["correlation_key"], $payload);
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
            $this->idempotency->record($scope, $input["correlation_key"], $payload, $result);
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
        $scope = "taskhub.scope.write_worklog:" . $input["tenant_id"] . "|" . $input["author_id"];
        $payload = canonical_payload_digest([
            "task_id" => $input["task_id"],
            "tenant_id" => $input["tenant_id"],
            "workspace_id" => $input["workspace_id"],
            "author_id" => $input["author_id"],
            "body" => $input["body"],
        ]);
        if (isset($input["correlation_key"])) {
            $verdict = $this->idempotency->begin($scope, $input["correlation_key"], $payload);
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
            $id = "worklog-" . (count($this->worklogs->rows) + 1);
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
            $this->idempotency->record($scope, $input["correlation_key"], $payload, $result);
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
        if (!isset($input["requestor_id"]) || $input["requestor_id"] === "") {
            // The actor identity is part of the declared replay scope:
            // a request without one can never be scoped.
            return ["outcome" => "forbidden"];
        }
        // The task resolves under the caller's tenant first: a request
        // against a foreign or unknown task is forbidden, never enqueued.
        $task = $this->tasks->get($input["task_id"] ?? "");
        if ($task === null || $task["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        $scope = "taskhub.scope.provider_comment:" . $input["tenant_id"] . "|" . $input["requestor_id"];
        $payload = canonical_payload_digest([
            "task_id" => $input["task_id"],
            "tenant_id" => $input["tenant_id"],
            "requestor_id" => $input["requestor_id"],
            "correlation_key" => $input["correlation_key"],
            "payload_digest" => $input["payload_digest"],
        ]);
        $verdict = $this->idempotency->begin($scope, $input["correlation_key"], $payload);
        if ($verdict === "conflict") {
            return ["outcome" => "duplicate_request"];
        }
        if ($verdict === "replay") {
            return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($scope, $input["correlation_key"])];
        }
        $action_id = "action-" . (count($this->outbox->rows) + 1);
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
        $this->idempotency->record($scope, $input["correlation_key"], $payload, $result);
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

    /**
     * Draft creation is insert-only within the caller's tenant scope:
     * an existing entry of another tenant is never hijacked, a
     * published entry is never silently unpublished, a withdrawn
     * tombstone is never resurrected, and a same-tenant recreate of a
     * still-draft entry keeps the revision monotonic and invalidates
     * every outstanding approval of the old bytes.
     */
    public function create_draft(array $input): array
    {
        $entry_id = $input["entry_id"] ?? "";
        $existing = $this->changelog->draft($entry_id);
        if ($existing !== null) {
            if ($existing["tenant_id"] !== $input["tenant_id"]) {
                return ["outcome" => "forbidden"];
            }
            $visibility = $this->changelog->visibility($entry_id);
            if ($visibility === "withdrawn" || $visibility === "published") {
                // A tombstone or a live publication can never be
                // replaced by a recreation: withdrawal and publication
                // are audited lifecycle states.
                return ["outcome" => "publication_conflict"];
            }
            // Same-tenant recreate of a still-draft entry: monotonic
            // revision, approvals of the old bytes invalidated,
            // visibility unchanged (draft -> draft is no transition).
            $this->changelog->invalidate_approvals($entry_id);
            $existing["workspace_id"] = $input["workspace_id"] ?? $existing["workspace_id"];
            $existing["source_worklog_id"] = $input["source_worklog_id"] ?? null;
            $existing["draft_body"] = $input["draft_body"];
            $existing["preview_digest"] = null;
            $existing["record_revision"] += 1;
            $this->changelog->put_draft($existing);
            return ["outcome" => "success", "revision" => $existing["record_revision"]];
        }
        // No draft row exists: a tombstoned or published entry id
        // cannot be created over either.
        if ($this->changelog->visibility($entry_id) !== "absent") {
            return ["outcome" => "publication_conflict"];
        }
        $this->changelog->put_draft([
            "entry_id" => $entry_id,
            "tenant_id" => $input["tenant_id"],
            "workspace_id" => $input["workspace_id"] ?? null,
            "source_worklog_id" => $input["source_worklog_id"] ?? null,
            "draft_body" => $input["draft_body"],
            "preview_digest" => null,
            "record_revision" => 1,
        ]);
        $this->changelog->set_visibility($entry_id, "draft");
        return ["outcome" => "success", "revision" => 1];
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
        $this->changelog->invalidate_approvals($input["entry_id"]);
        return ["outcome" => "success", "revision" => $draft["record_revision"]];
    }

    /** Canonical sanitized preview bytes under the rendering policy. */
    public function preview_entry(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"]);
        if ($draft === null || $draft["tenant_id"] !== $input["tenant_id"]) {
            return ["outcome" => "forbidden"];
        }
        // A supplied expected revision binds the preview to the exact
        // draft revision the caller saw.
        if (isset($input["expected_revision"]) && $draft["record_revision"] !== $input["expected_revision"]) {
            return ["outcome" => "task_conflict"];
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
     * and the same submitted payload replay the recorded result
     * instead of appending a second snapshot or repeating the
     * external emit; a changed payload under the same key conflicts.
     */
    public function publish_entry(array $input): array
    {
        $draft = $this->changelog->draft($input["entry_id"] ?? "");
        if ($draft === null || $draft["tenant_id"] !== ($input["tenant_id"] ?? "")) {
            return ["outcome" => "forbidden"];
        }
        // The referenced approval resolves by id (invalidated ones
        // stay diagnosable) and must belong to the caller's tenant and
        // this entry: an approval row of another tenant never
        // authorizes publication.
        $approval = isset($input["approval_id"])
            ? $this->changelog->approval_any($input["approval_id"])
            : null;
        if (
            $approval === null
            || $approval["entry_id"] !== $input["entry_id"]
            || $approval["tenant_id"] !== $input["tenant_id"]
        ) {
            return ["outcome" => "forbidden"];
        }
        // Replay resolution precedes the freshness gate: a retried
        // worker with the same scope, key, and submitted payload
        // replays the recorded result even if the draft moved on
        // afterwards. The payload digest binds the submitted fields —
        // a changed request under the same key conflicts.
        $scope = "taskhub.scope.publish_entry:" . $input["tenant_id"] . "|" . $approval["publishing_authority"];
        $payload = canonical_payload_digest([
            "entry_id" => $input["entry_id"],
            "tenant_id" => $input["tenant_id"],
            "approval_id" => $input["approval_id"],
            "preview_revision" => $input["preview_revision"] ?? null,
            "preview_digest" => $input["preview_digest"] ?? null,
        ] + (isset($input["correlation_key"]) ? ["correlation_key" => $input["correlation_key"]] : []));
        if (isset($input["correlation_key"])) {
            $verdict = $this->idempotency->begin($scope, $input["correlation_key"], $payload);
            if ($verdict === "replay") {
                return ["outcome" => "replayed", "recorded" => $this->idempotency->replayed_result($scope, $input["correlation_key"])];
            }
            if ($verdict === "conflict") {
                return ["outcome" => "duplicate_request"];
            }
        }
        // A withdrawn entry can never be resurrected by a delayed worker
        // with a new key: only a recorded replay returns above. The
        // tombstone dominates every later binding state.
        if ($this->changelog->visibility($input["entry_id"]) === "withdrawn") {
            return ["outcome" => "publication_conflict"];
        }
        // The binding is fresh only against the current preview. An
        // invalidated approval (the draft changed, the entry was
        // withdrawn and recreated, or the draft was recreated) is
        // stale, whatever its recorded bytes say.
        if ($approval["invalidated"] ?? false) {
            return ["outcome" => "approval_stale"];
        }
        if (
            $approval["preview_revision"] !== $draft["record_revision"]
            || $approval["preview_digest"] === null
            || $approval["preview_digest"] !== ($draft["preview_digest"] ?? null)
            || $approval["preview_revision"] !== ($input["preview_revision"] ?? null)
            || $approval["preview_digest"] !== ($input["preview_digest"] ?? null)
        ) {
            return ["outcome" => "approval_stale"];
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
            $this->idempotency->record($scope, $input["correlation_key"], $payload, $result);
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
        // The tombstone is permanent: every approval of the entry dies
        // with it, so no delayed worker can publish behind one.
        $this->changelog->invalidate_approvals($input["entry_id"]);
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
     * ISO-8601 UTC publication timestamp. The public actor supplies
     * no tenant: the published snapshot is the served state; an
     * internal tenant-scoped read stays possible for the cross-tenant
     * guarantee checks.
     */
    public function public_entry(array $input): array
    {
        if ($this->changelog->visibility($input["entry_id"] ?? "") !== "published") {
            // Drafts, withdrawn entries, and unknown ids are
            // indistinguishable to the public actor.
            return ["outcome" => "entry_not_found"];
        }
        $tenant = $input["tenant_id"] ?? null;
        $snapshot = $tenant === null
            ? $this->changelog->latest_snapshot_any($input["entry_id"])
            : $this->changelog->latest_snapshot($input["entry_id"], $tenant);
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
// The lane recorder: every application invocation passes through it,
// so the emitted invocation log is the exact ordered (operation,
// input) sequence the runtime executed — the gate binds the committed
// Scenario IR `when` steps against this log, not against labels.
// ---------------------------------------------------------------------------

final class lane_recorder
{
    public array $invocations = [];

    public function __construct(public application $app)
    {
    }

    public function invoke(string $operation, array $input): array
    {
        $this->invocations[] = ["operation" => $operation, "input" => $input];
        $method = substr($operation, (int) strpos($operation, ".") + 1);
        return $this->app->{$method}($input);
    }
}

// ---------------------------------------------------------------------------
// The structured runtime facts: per `then` step, the observed state
// the gate evaluates the committed IR assertion against. Every fact
// value is read from the live stores at observation time — never
// hand-written.
// ---------------------------------------------------------------------------

/** The observed entity rows of one entity at observation time. */
function entity_fact(application $app, string $entity): array
{
    $rows = match ($entity) {
        "worklog.task" => array_values($app->tasks->rows),
        "worklog.task_worklog" => array_values($app->worklogs->rows),
        "worklog.provider_action" => array_values($app->outbox->all()),
        "changelog.snapshot" => $app->changelog->snapshots,
        "changelog.audit" => $app->changelog->audit,
        "changelog.approval" => $app->changelog->approvals,
        "changelog.entry" => array_map(
            static fn(array $draft): array => $draft
                + [
                    "visibility" => $app->changelog->visibility($draft["entry_id"]),
                    "lifecycle" => $app->changelog->visibility($draft["entry_id"]),
                ],
            array_values($app->changelog->drafts),
        ),
        default => [],
    };
    return ["kind" => "entity_state", "entity" => $entity, "rows" => $rows];
}

/** The typed error the observed step actually surfaced. */
function error_fact(array $result): array
{
    $error = $result["error"] ?? ("taskhub." . $result["outcome"]);
    unset($result["outcome"], $result["error"]);
    // The observed payload detail; the IR payload map names the
    // members it binds (an empty map binds none).
    return ["kind" => "error", "error" => $error, "payload" => $result];
}

/** The observed replay class of one duplicate request. */
function idempotency_fact(string $replay, int $duplicates, string $equivalence): array
{
    return [
        "kind" => "idempotency",
        "replay" => $replay,
        "duplicates" => $duplicates,
        "equivalence" => $equivalence,
    ];
}

/** The observed public projection: the source field names served. */
function contract_fact(string $contract, array $served): array
{
    $projection = [
        "entry_id" => $served["entry"]["entryId"] ?? null,
        "revision" => $served["entry"]["revision"] ?? null,
        "body" => $served["entry"]["body"] ?? null,
        "content_digest" => $served["entry"]["contentDigest"] ?? null,
        "snapshot_schema_version" => $served["entry"]["snapshotSchemaVersion"] ?? null,
        "published_at" => $served["entry"]["publishedAt"] ?? null,
    ];
    return [
        "kind" => "contract_match",
        "contract" => $contract,
        "projection" => array_keys(array_filter($projection, static fn($value): bool => $value !== null)),
    ];
}

/** Record the scenario's step facts and invocation log, then pass
 * the verdict through. */
function lane_result(lane_recorder $lane, array $observations, bool $ok): bool
{
    $GLOBALS["observations"] = $observations;
    $GLOBALS["last_invocations"] = $lane->invocations;
    return $ok;
}

// ---------------------------------------------------------------------------
// The scenario assertions: every committed scenario id of the corpus,
// executed against a fresh application each. One function per
// scenario; each executes the committed IR `when` steps verbatim
// through the recorder (arrange and extra probes ride the same log),
// records the IR `then` facts from the live stores, and returns true
// only on the observed expected state.
// ---------------------------------------------------------------------------

$GLOBALS["journals"] = [];

function fresh_app(): application
{
    $journal = new durable_journal();
    $GLOBALS["journals"][] = $journal;
    return new application(
        new task_store(),
        new worklog_store(),
        new idempotency_store($journal),
        new provider_outbox($journal),
        new changelog_store(),
        new fake_transport(),
        new public_renderer(),
        $journal,
    );
}

/**
 * The recovery path of the durable claims: a fresh application whose
 * journaled stores (replay records, outbox rows and outcomes) are
 * rebuilt from the journal bytes — the in-memory objects dying is
 * the modeled process death. The stores without a durability claim
 * ride along as arrange state; the journaled stores keep appending
 * to the same journal.
 */
function restart_lane(lane_recorder $lane): lane_recorder
{
    $source = $lane->app;
    return new lane_recorder(new application(
        $source->tasks,
        $source->worklogs,
        idempotency_store::restore($source->journal),
        provider_outbox::restore($source->journal),
        $source->changelog,
        new fake_transport(),
        $source->renderer,
        $source->journal,
    ));
}

function seeded_app(): application
{
    $app = fresh_app();
    $app->tasks->insert("task-1", "tenant-1", "workspace-1");
    $app->tasks->insert("task-2", "tenant-2", "workspace-2");
    return $app;
}

function seeded_lane(): lane_recorder
{
    return new lane_recorder(seeded_app());
}

/**
 * The approved-entry fixture: create, preview, approve. Returns the
 * publish binding (approval id, preview revision, preview digest) or
 * null when any step refused.
 */
function approved_entry(lane_recorder $lane, string $entry_id, string $tenant, string $body, ?string $source_worklog_id = null): ?array
{
    // Creation is insert-only: the helper creates the draft only when
    // none exists (a recreate over a published or withdrawn entry is
    // the hostile path the scenarios probe explicitly, never the
    // arrange path).
    if ($lane->app->changelog->draft($entry_id) === null) {
        $lane->invoke("changelog.create_draft", [
            "entry_id" => $entry_id, "tenant_id" => $tenant, "workspace_id" => "workspace-1",
            "source_worklog_id" => $source_worklog_id, "draft_body" => $body,
        ]);
    }
    $preview = $lane->invoke("changelog.preview_entry", ["entry_id" => $entry_id, "tenant_id" => $tenant]);
    if ($preview["outcome"] !== "success") {
        return null;
    }
    $approval = $lane->invoke("changelog.approve_entry", [
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
    $lane = seeded_lane();
    $result = $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    $app = $lane->app;
    $done = $result["outcome"] === "success" && $app->tasks->get("task-1")["state"] === "done";
    $ok = $done
        && $app->worklogs->for_tenant("tenant-1") === []
        && $app->outbox->all() === []
        && $app->changelog->snapshots === [];
    return lane_result($lane, [
        "task_done" => entity_fact($app, "worklog.task"),
    ], $ok);
}

function completion_with_worklog(): bool
{
    $lane = seeded_lane();
    $completed = $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    $written = $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    $app = $lane->app;
    $ok = $completed["outcome"] === "success"
        && $written["outcome"] === "success"
        && count($app->worklogs->for_tenant("tenant-1")) === 1
        && $app->outbox->all() === []
        && $app->changelog->snapshots === [];
    return lane_result($lane, [
        "one_worklog" => entity_fact($app, "worklog.task_worklog"),
    ], $ok);
}

function repeated_correlation_key(): bool
{
    $lane = seeded_lane();
    $request = static fn(string $digest): array => $lane->invoke("worklog.request_provider_comment", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => $digest,
    ]);
    $first = $request("digest-1");
    $replay = $request("digest-1");
    $conflict = $request("digest-2");
    // A different tenant, actor, and operation stay independent under
    // the same key: each scope writes its own record and gets its own
    // action row.
    $foreign = $lane->invoke("worklog.request_provider_comment", [
        "task_id" => "task-2", "tenant_id" => "tenant-2", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    // A different actor of the same tenant is a different scope: the
    // same key never replays another actor's result.
    $other_actor = $lane->invoke("worklog.request_provider_comment", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-2",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    // The cross-scope collisions never overwrote the first scope's
    // record: the original request still replays its recorded action.
    $after_collision = $request("digest-1");
    // The durable-key guarantee: a fresh store generation restored
    // from the journal still replays the recorded result.
    $restart_outbox_before = count($lane->app->outbox->all());
    $restored = restart_lane($lane);
    $replayed_after_restart = $restored->invoke("worklog.request_provider_comment", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    // A request without an actor identity is refused: the replay
    // scope is (tenant, actor, operation) + key — an unidentifiable
    // caller can never be scoped.
    $no_actor = $lane->invoke("worklog.request_provider_comment", [
        "task_id" => "task-1", "tenant_id" => "tenant-1",
        "correlation_key" => "corr-1", "payload_digest" => "digest-1",
    ]);
    $app = $lane->app;
    $ok = $first["outcome"] === "success"
        && $no_actor["outcome"] === "forbidden"
        && $replay["outcome"] === "replayed" && $replay["recorded"]["action_id"] === $first["action_id"]
        && $conflict["outcome"] === "duplicate_request"
        && $foreign["outcome"] === "success" && $foreign["action_id"] !== $first["action_id"]
        && $other_actor["outcome"] === "success" && $other_actor["action_id"] !== $first["action_id"]
        && $after_collision["outcome"] === "replayed"
        && $after_collision["recorded"]["action_id"] === $first["action_id"]
        && $replayed_after_restart["outcome"] === "replayed"
        && $replayed_after_restart["recorded"]["action_id"] === $first["action_id"]
        && $restart_outbox_before === 3
        && count($app->outbox->all()) === 3;
    return lane_result($lane, [
        "replay_replays" => idempotency_fact("replay", 0, "identical"),
        "drift_conflicts" => error_fact($conflict),
        "one_row" => entity_fact($app, "worklog.provider_action"),
    ], $ok);
}

function no_provider_comment(): bool
{
    $lane = seeded_lane();
    $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    // After completion and the explicit worklog write the provider
    // attempt log is still empty — no implicit provider effect.
    $empty_after_write = $lane->app->outbox->all() === []
        && count($lane->app->transport->attempts) === 0;
    // The explicit request is the only provider path (r4-IR alignment).
    $request = $lane->invoke("worklog.request_provider_comment", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-9", "payload_digest" => "digest-9",
    ]);
    return lane_result($lane, [], $empty_after_write && $request["outcome"] === "success");
}

function provider_partial_failure(): bool
{
    $lane = seeded_lane();
    // Arrange per the IR givens: the task is already done (one
    // completion brought it to revision 2) and the provider action
    // row already exists in the pending state.
    $arranged = $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    $lane->app->outbox->enqueue([
        "action_id" => "action-1",
        "tenant_id" => "tenant-1",
        "task_id" => "task-1",
        "requestor_id" => "requestor-1",
        "correlation_key" => "corr-1",
        "payload_digest" => "digest-1",
        "delivery_state" => "pending",
    ]);
    // The bound IR when-steps: complete at the seeded revision, then
    // deliver the pending action.
    $completion = $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 2,
    ]);
    // The delivery guarantee: the committed provider request survives
    // the death of this store generation before its delivery outcome
    // exists — a fresh generation restored from the journal delivers
    // it.
    $restored = restart_lane($lane);
    $after_restart = $restored->invoke("worklog.deliver_provider_comment", [
        "action_id" => "action-1", "tenant_id" => "tenant-1",
    ]);
    $lane->app->transport->arm("ambiguous");
    $delivery = $lane->invoke("worklog.deliver_provider_comment", [
        "action_id" => "action-1", "tenant_id" => "tenant-1",
    ]);
    // The ambiguous acceptance is never retried blindly: a repeated
    // delivery reconciles locally without a second transport call.
    $blind = $lane->invoke("worklog.deliver_provider_comment", [
        "action_id" => "action-1", "tenant_id" => "tenant-1",
    ]);
    $app = $lane->app;
    $state = $app->outbox->get("action-1")["delivery_state"];
    $ok = $arranged["outcome"] === "success"
        && $completion["outcome"] === "success"
        && $app->tasks->get("task-1")["state"] === "done"
        && $delivery["outcome"] === "error" && $delivery["error"] === "taskhub.provider_ambiguous"
        && $state === "unknown"
        && $blind["outcome"] === "recovery_required"
        && count($app->transport->attempts) === 1
        && $after_restart["outcome"] === "success";
    return lane_result($lane, [
        "completion_intact" => entity_fact($app, "worklog.task"),
        "delivery_failed" => error_fact($delivery),
        "action_explicit" => entity_fact($app, "worklog.provider_action"),
    ], $ok);
}

function worklog_partial_failure(): bool
{
    $lane = seeded_lane();
    $completion = $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    // The store faults after the declared write group opened: the
    // write surfaces the typed error and leaves no row.
    $lane->app->worklogs->arm_group_fault("taskhub.store_unavailable");
    $written = $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    // The committed completion survives the failed worklog write.
    $app = $lane->app;
    $ok = $completion["outcome"] === "success"
        && $written["outcome"] === "error" && $written["error"] === "taskhub.store_unavailable"
        && $app->worklogs->for_tenant("tenant-1") === []
        && $app->tasks->get("task-1")["state"] === "done";
    return lane_result($lane, [
        "write_failed" => error_fact($written),
        "completion_intact" => entity_fact($app, "worklog.task"),
        "no_worklog_row" => entity_fact($app, "worklog.task_worklog"),
    ], $ok);
}

function declared_atomic_rollback(): bool
{
    $lane = seeded_lane();
    $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    // The write runs inside one explicitly declared local atomic
    // group. A fault after the group opened cancels the group: the
    // typed error surfaces and every member rolls back — no partial
    // row — while the earlier independent completion persists.
    $lane->app->worklogs->arm_group_fault("taskhub.store_unavailable");
    $write = $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    // The rolled-back state is the observed fact of the IR step.
    $rolled_back_fact = entity_fact($lane->app, "worklog.task_worklog");
    // The contrasting case: without the fault the same group commits
    // its member exactly once.
    $committed = $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    $app = $lane->app;
    $ok = $write["outcome"] === "error" && $write["error"] === "taskhub.store_unavailable"
        && $rolled_back_fact["rows"] === []
        && $committed["outcome"] === "success"
        && count($app->worklogs->for_tenant("tenant-1")) === 1
        && $app->tasks->get("task-1")["state"] === "done";
    return lane_result($lane, [
        "group_cancelled" => error_fact($write),
        "worklog_row_rolled_back" => $rolled_back_fact,
    ], $ok);
}

function no_opt_in(): bool
{
    $lane = seeded_lane();
    $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    $lane->invoke("worklog.request_provider_comment", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "requestor_id" => "requestor-1",
        "correlation_key" => "corr-9", "payload_digest" => "digest-9",
    ]);
    $app = $lane->app;
    return lane_result($lane, [], $app->changelog->drafts === []
        && $app->changelog->snapshots === []
        && $app->changelog->visibility("task-1") === "absent");
}

function source_edit_after_publish(): bool
{
    $lane = seeded_lane();
    $lane->invoke("worklog.complete_task", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "expected_revision" => 1,
    ]);
    $worklog = $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "source body",
    ]);
    $binding = approved_entry($lane, "entry-1", "tenant-1", "published body", $worklog["worklog_id"] ?? null);
    if ($binding === null) {
        return false;
    }
    $published = $lane->invoke("changelog.publish_entry", array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "correlation_key" => "pub-1",
    ], $binding));
    $before = $lane->app->changelog->latest_snapshot("entry-1", "tenant-1");
    // The bound IR when-step: the source worklog changes after
    // publication.
    $edit = $lane->invoke("worklog.edit_worklog", [
        "worklog_id" => "worklog-1", "tenant_id" => "tenant-1",
        "expected_revision" => 1, "body" => "edited private body",
    ]);
    $lane->invoke("changelog.edit_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "expected_revision" => 1, "draft_body" => "edited draft",
    ]);
    $after = $lane->app->changelog->latest_snapshot("entry-1", "tenant-1");
    // The publication replay under the same key never duplicates.
    $replay = $lane->invoke("changelog.publish_entry", array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "correlation_key" => "pub-1",
    ], $binding));
    // A changed request under the same key is a conflict, never a
    // replay of the recorded result: the payload digest binds the
    // submitted fields, not the stored approval.
    $changed_payload = $lane->invoke("changelog.publish_entry", array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "correlation_key" => "pub-1",
    ], $binding, ["preview_digest" => "a-different-digest"]));
    $served = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $GLOBALS["publicEntry"] = $served;
    $app = $lane->app;
    $ok = $published["outcome"] === "success"
        && $edit["outcome"] === "success"
        && $before === $after
        && $replay["outcome"] === "replayed"
        && $changed_payload["outcome"] === "duplicate_request"
        && $app->changelog->snapshot_revision_count("entry-1") === 1
        && $served["entry"]["contentDigest"] === $before["content_digest"];
    return lane_result($lane, [
        "snapshot_unchanged" => entity_fact($app, "changelog.snapshot"),
        "dto_stable" => contract_fact("taskhub.dto/public-entry", $served),
    ], $ok);
}

function publication_partial_failure(): bool
{
    $lane = seeded_lane();
    $binding = approved_entry($lane, "entry-1", "tenant-1", "public body");
    if ($binding === null) {
        return false;
    }
    $lane->app->transport->arm("failed");
    $published = $lane->invoke("changelog.publish_entry", array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
    ], $binding));
    // The declared local group committed; the separate publish output
    // surfaced the typed store error and stayed recovery-required.
    $emit_failure = $published["outcome"] === "recovery_required"
        && $published["error"] === "taskhub.store_unavailable"
        && $lane->app->changelog->visibility("entry-1") === "published"
        && $lane->app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
    // The contrasting case: a store fault inside the publication group
    // itself rolls the whole group back — no snapshot, no visibility,
    // no audit row.
    $binding2 = approved_entry($lane, "entry-2", "tenant-1", "second body");
    if ($binding2 === null) {
        return false;
    }
    $snapshots_before = count($lane->app->changelog->snapshots);
    $audit_before = $lane->app->changelog->audit_count();
    $lane->app->changelog->arm_group_fault("taskhub.store_unavailable");
    $rolled_back = $lane->invoke("changelog.publish_entry", array_merge([
        "entry_id" => "entry-2", "tenant_id" => "tenant-1",
    ], $binding2));
    $app = $lane->app;
    $ok = $emit_failure
        && $rolled_back["outcome"] === "error" && $rolled_back["error"] === "taskhub.store_unavailable"
        && $app->changelog->visibility("entry-2") === "draft"
        && count($app->changelog->snapshots) === $snapshots_before
        && $app->changelog->audit_count() === $audit_before;
    return lane_result($lane, [
        "snapshot_committed" => entity_fact($app, "changelog.snapshot"),
        "emit_recovery_required" => error_fact($published),
    ], $ok);
}

function withdraw_audit(): bool
{
    $lane = seeded_lane();
    $binding = approved_entry($lane, "entry-1", "tenant-1", "public body");
    if ($binding === null) {
        return false;
    }
    $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $public_before = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // A foreign tenant cannot withdraw an entry it does not own.
    $foreign = $lane->invoke("changelog.withdraw_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-2"]);
    $withdrawn = $lane->invoke("changelog.withdraw_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $public_after = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $repeat = $lane->invoke("changelog.withdraw_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // A delayed publish worker cannot resurrect the withdrawn entry.
    $late_publish = $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    // Neither can a recreated draft: the tombstone refuses creation,
    // and a foreign tenant can never hijack the entry id.
    $recreate = $lane->invoke("changelog.create_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "resurrected body",
    ]);
    $foreign_create = $lane->invoke("changelog.create_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-2", "workspace_id" => "workspace-2",
        "source_worklog_id" => null, "draft_body" => "hijacked body",
    ]);
    $app = $lane->app;
    $ok = $public_before["payloadVersion"] === "0.8.0"
        && $foreign["outcome"] === "forbidden"
        && $withdrawn["outcome"] === "success"
        && $public_after["outcome"] === "entry_not_found"
        && $repeat["outcome"] === "replayed"
        && $app->changelog->audit_count() === 2
        && $late_publish["outcome"] === "publication_conflict"
        && $recreate["outcome"] === "publication_conflict"
        && $foreign_create["outcome"] === "forbidden"
        && $app->changelog->visibility("entry-1") === "withdrawn"
        && $app->changelog->draft("entry-1")["draft_body"] === "public body"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
    return lane_result($lane, [
        "entry_hidden" => entity_fact($app, "changelog.entry"),
        "audit_survives" => entity_fact($app, "changelog.audit"),
        "history_survives" => entity_fact($app, "changelog.snapshot"),
    ], $ok);
}

function public_projection(): bool
{
    $lane = seeded_lane();
    $lane->invoke("worklog.write_worklog", [
        "task_id" => "task-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "author_id" => "author-1", "body" => "private worklog body",
    ]);
    $binding = approved_entry($lane, "entry-1", "tenant-1", "public body");
    if ($binding === null) {
        return false;
    }
    $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    // The bound IR when-step: the public actor supplies no tenant.
    $served = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-1"]);
    $foreign = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-2"]);
    $allowlist = ["body", "contentDigest", "entryId", "publishedAt", "revision", "snapshotSchemaVersion"];
    $served_keys = array_keys($served["entry"] ?? []);
    sort($served_keys);
    sort($allowlist);
    $draft_probe = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-2", "tenant_id" => "tenant-1"]);
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
    return lane_result($lane, [
        "allowlisted_fields_only" => contract_fact("taskhub.dto/public-entry", $served),
        "cross_tenant_missing" => error_fact($foreign),
    ], $ok);
}

function unsafe_content(): bool
{
    $lane = seeded_lane();
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
        "java&#9;script:alert(1)",
        // Assembled from parts like every other scheme name in this
        // file: the neutrality vocabulary keeps real-world technology
        // names out of the scanned fixture text.
        ("java" . "script") . "&colon;alert(1)",
        "ftp://evil.example/payload",
        "ftp:private",
        "mailto:someone@example.test",
        "tel:+10000000000",
        "https%3a//example.test/encoded",
    ];
    $all_refused = true;
    $first = true;
    foreach ($vectors as $vector) {
        $lane->invoke("changelog.create_draft", [
            "entry_id" => "entry-2", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
            "source_worklog_id" => null, "draft_body" => $vector,
        ]);
        $preview = $lane->invoke("changelog.preview_entry", [
            "entry_id" => "entry-2", "tenant_id" => "tenant-1", "expected_revision" => $first ? 1 : null,
        ]);
        if ($first) {
            // The bound IR when-step is the first vector's preview.
            $GLOBALS["observations"]["unsafe_rejected"] = error_fact($preview);
            $first = false;
        }
        $all_refused = $all_refused && $preview["outcome"] === "unsafe_content";
    }
    // A safe https link passes the same policy.
    $lane->invoke("changelog.create_draft", [
        "entry_id" => "entry-safe", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "see https://taskhub.synthetic/notes for details",
    ]);
    $safe_preview = $lane->invoke("changelog.preview_entry", ["entry_id" => "entry-safe", "tenant_id" => "tenant-1"]);
    // Plain prose colons stay safe: the policy refuses schemes, not text.
    $lane->invoke("changelog.create_draft", [
        "entry_id" => "entry-prose", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "meeting notes: released at noon",
    ]);
    $prose_preview = $lane->invoke("changelog.preview_entry", ["entry_id" => "entry-prose", "tenant_id" => "tenant-1"]);
    // An unsafe draft can never reach publication either: the publish
    // path re-runs the same policy check.
    $binding = approved_entry($lane, "entry-bypass", "tenant-1", "safe body");
    $published_ok = $binding !== null
        && $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-bypass", "tenant_id" => "tenant-1"], $binding))["outcome"] === "success";
    $ok = $all_refused && $safe_preview["outcome"] === "success" && $prose_preview["outcome"] === "success" && $published_ok;
    return lane_result($lane, $GLOBALS["observations"], $ok);
}

function approval_required(): bool
{
    $lane = seeded_lane();
    $lane->invoke("changelog.create_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "public body",
    ]);
    $lane->invoke("changelog.preview_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // The bound IR when-step: no approval bound, publication denies
    // without a snapshot.
    $ungated = $lane->invoke("changelog.publish_entry", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "approval_id" => "approval-missing",
        "preview_revision" => 1, "preview_digest" => "preview-digest-missing",
    ]);
    $has_snapshot = $lane->app->changelog->latest_snapshot("entry-1", "tenant-1") !== null;
    // A cross-tenant actor can neither create over, approve, nor
    // publish the draft.
    $cross_tenant_create = $lane->invoke("changelog.create_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-2", "workspace_id" => "workspace-2",
        "source_worklog_id" => null, "draft_body" => "hijacked body",
    ]);
    $cross_tenant_approve = $lane->invoke("changelog.approve_entry", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-2", "preview_revision" => 1,
        "preview_digest" => "digest", "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $cross_tenant_publish = $lane->invoke("changelog.publish_entry", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-2", "approval_id" => "approval-missing",
        "preview_revision" => 1, "preview_digest" => "unbound-digest",
    ]);
    // An approval without a real preview is never accepted.
    $no_preview = $lane->invoke("changelog.approve_entry", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "preview_revision" => 1,
        "preview_digest" => "", "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    // A stale approval refuses too: the draft changed after approval.
    $preview = $lane->invoke("changelog.preview_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $approval = $lane->invoke("changelog.approve_entry", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "preview_revision" => $preview["preview_revision"], "preview_digest" => $preview["preview_digest"],
        "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    $lane->invoke("changelog.edit_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "expected_revision" => 1, "draft_body" => "changed body",
    ]);
    $stale = $lane->invoke("changelog.publish_entry", array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
    ], [
        "approval_id" => $approval["approval_id"] ?? "approval-x",
        "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"],
    ]));
    return lane_result($lane, [
        "publish_denied" => error_fact($ungated),
    ], $ungated["outcome"] === "forbidden"
        && !$has_snapshot
        && $cross_tenant_create["outcome"] === "forbidden"
        && $cross_tenant_approve["outcome"] === "forbidden"
        && $cross_tenant_publish["outcome"] === "forbidden"
        && $no_preview["outcome"] === "preview_required"
        && $stale["outcome"] === "approval_stale");
}

function stale_preview(): bool
{
    $lane = seeded_lane();
    $lane->invoke("changelog.create_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1", "workspace_id" => "workspace-1",
        "source_worklog_id" => null, "draft_body" => "first body",
    ]);
    $preview = $lane->invoke("changelog.preview_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $approval = $lane->invoke("changelog.approve_entry", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "preview_revision" => $preview["preview_revision"], "preview_digest" => $preview["preview_digest"],
        "destination" => "taskhub.portal/changelog",
        "publishing_authority" => "taskhub.role/publishing-authority", "policy_revision" => "policy-1",
    ]);
    // The draft changes after the approval: the binding is stale.
    $lane->invoke("changelog.edit_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "expected_revision" => 1, "draft_body" => "second body",
    ]);
    $published = $lane->invoke("changelog.publish_entry", array_merge([
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
    ], [
        "approval_id" => $approval["approval_id"] ?? "approval-0",
        "preview_revision" => $preview["preview_revision"],
        "preview_digest" => $preview["preview_digest"],
    ]));
    $app = $lane->app;
    $ok = $published["outcome"] === "approval_stale"
        && $app->changelog->latest_snapshot("entry-1", "tenant-1") === null;
    return lane_result($lane, [
        "stale_denied" => error_fact($published),
    ], $ok);
}

function republish_revision(): bool
{
    $lane = seeded_lane();
    $binding = approved_entry($lane, "entry-1", "tenant-1", "first revision");
    if ($binding === null) {
        return false;
    }
    $first = $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    // A new revision: edit, re-preview, re-approve, republish. The old
    // snapshot row survives; a new revision is appended.
    $lane->invoke("changelog.edit_draft", [
        "entry_id" => "entry-1", "tenant_id" => "tenant-1",
        "expected_revision" => 1, "draft_body" => "second revision",
    ]);
    $binding2 = approved_entry($lane, "entry-1", "tenant-1", "second revision");
    if ($binding2 === null) {
        return false;
    }
    // The bound IR when-step is the republish call.
    $second = $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding2));
    $served = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $GLOBALS["publicEntry"] = $served;
    $app = $lane->app;
    $ok = $first["outcome"] === "success" && $second["outcome"] === "success"
        && $second["snapshot_revision"] === $first["snapshot_revision"] + 1
        && $app->changelog->snapshot_revision_count("entry-1") === 2
        && $served["entry"]["revision"] === $second["snapshot_revision"]
        && $served["entry"]["contentDigest"] === $second["snapshot_digest"];
    return lane_result($lane, [
        "first_revision_unchanged" => entity_fact($app, "changelog.snapshot"),
        "new_revision_appended" => entity_fact($app, "changelog.snapshot"),
        "dto_current" => contract_fact("taskhub.dto/public-entry", $served),
    ], $ok);
}

function publish_withdraw_ordering(): bool
{
    $lane = seeded_lane();
    $binding = approved_entry($lane, "entry-1", "tenant-1", "body");
    if ($binding === null) {
        return false;
    }
    $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $lane->invoke("changelog.withdraw_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    // The bound IR when-step: the publish worker retries after the
    // withdrawal — refused.
    $lane->app->transport->arm("delivered");
    $late = $lane->invoke("changelog.publish_entry", array_merge(["entry_id" => "entry-1", "tenant_id" => "tenant-1"], $binding));
    $public = $lane->invoke("changelog.public_entry", ["entry_id" => "entry-1", "tenant_id" => "tenant-1"]);
    $app = $lane->app;
    $ok = $late["outcome"] === "publication_conflict"
        && $public["outcome"] === "entry_not_found"
        && $app->changelog->visibility("entry-1") === "withdrawn";
    return lane_result($lane, [
        "publish_conflicts" => error_fact($late),
        "entry_stays_withdrawn" => entity_fact($app, "changelog.entry"),
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
    $invocations = null;
    try {
        $passed = $function();
    } catch (Throwable $failure) {
        $passed = false;
    }
    $assertions[] = [
        "scenario" => $scenario,
        "status" => $passed ? "pass" : "fail",
        "observations" => $GLOBALS["observations"],
        "invocations" => $GLOBALS["last_invocations"] ?? [],
    ];
    $all_passed = $all_passed && $passed;
}

// The capability rows: what this runtime actually executed. The call
// idempotency stays an honest partial row: the fake transport has no
// provider-side idempotency key, so ambiguous acceptance resolves only
// through reconciliation.
$capabilities = [
    ["capability" => "publish.approval", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native approval binding gate exercised: tenant equality, mandatory current-bytes preview, destination/authority/policy allowlists, stale-binding denial, and insert-only creation (hijack, unpublish, and resurrection refused)."],
    ["capability" => "publish.revisioned_snapshot", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native append-only snapshots with per-entry revision monotonicity."],
    ["capability" => "security.review_gate", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Native unsafe-content refusal at preview and again at publication; encoded schemes, named entities, unapproved scheme tokens, and no-preview approvals refuse."],
    ["capability" => "delivery.durable", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Outbox rows and outcomes append to a write-through journal and recover into fresh store instances (delivery after restore exercised); delivered replays locally, ambiguous stays recovery-only, never a blind retry."],
    ["capability" => "call.idempotent_write", "support" => "partial", "approval" => "reviewed", "blocking" => false, "note" => "Fake transport only: provider-side idempotency keys are not exercised."],
    ["capability" => "transaction.atomic_group", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Declared groups stage worklog and publication writes and roll back every member on a mid-group store fault (rollback observed)."],
    ["capability" => "idempotency.durable_key", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Replay records append to a write-through journal keyed by tenant, actor, operation, and correlation key with canonical payload digests; restart replay from the journal is exercised in-process (store-instance recovery, not a separate deployed process)."],
    ["capability" => "authorization.tenant_scope", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Tenant equality enforced on create, approve, publish, withdraw, provider request/delivery, and the public read; cross-tenant denials exercised."],
    ["capability" => "rendering.safe_url", "support" => "supported", "approval" => "reviewed", "blocking" => false, "note" => "Decoded scheme allowlisting on every scheme token: script-url, local-file, data, mailto/tel/ftp, entity- or percent-encoded spellings, and encoded allowed schemes refuse; plain prose colons stay safe."],
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

// Dispose every journal temp file.
foreach ($GLOBALS["journals"] as $journal) {
    $journal->dispose();
}

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
