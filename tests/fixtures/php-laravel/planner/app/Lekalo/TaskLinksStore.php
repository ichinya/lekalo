<?php

declare(strict_types=1);

namespace App\Lekalo;

use Illuminate\Database\Capsule\Manager;
use Illuminate\Database\Connection;

/** Maintained Laravel port of the synthetic TaskExternalLink storage boundary.
 * Network acceptance is a separate committed receipt; it is NEVER enclosed in
 * the task/decision/outbox/audit transaction. Full worker authority context is
 * persisted and rechecked on each claim, including after a process restart.
 */
final class TaskLinksStore
{
    private Connection $db;

    public function __construct(string $database)
    {
        $capsule = new Manager();
        $capsule->addConnection(['driver' => 'sqlite', 'database' => $database, 'prefix' => '']);
        $this->db = $capsule->getConnection();
        $this->db->statement('PRAGMA journal_mode=WAL');
        $this->db->statement('PRAGMA synchronous=FULL');
        $this->db->statement('PRAGMA busy_timeout=10000');
        $this->db->statement('PRAGMA foreign_keys=ON');
    }

    public function initialize(): void
    {
        $this->db->unprepared(<<<'SQL'
CREATE TABLE task (task_id TEXT PRIMARY KEY, title TEXT NOT NULL, version INTEGER NOT NULL);
CREATE TABLE link (link_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, workspace_id TEXT NOT NULL,
 installation_id TEXT NOT NULL, provider_id TEXT NOT NULL, object_type TEXT NOT NULL, external_id TEXT NOT NULL,
 generation INTEGER NOT NULL DEFAULT 1, state TEXT NOT NULL DEFAULT 'active', version INTEGER NOT NULL DEFAULT 1,
 UNIQUE(workspace_id COLLATE BINARY,installation_id COLLATE BINARY,provider_id COLLATE BINARY,object_type COLLATE BINARY,external_id COLLATE BINARY));
CREATE TABLE decision (decision_id TEXT PRIMARY KEY, link_id TEXT NOT NULL, actor TEXT NOT NULL, fields TEXT NOT NULL,
 policy_revision TEXT NOT NULL, capability_revision TEXT NOT NULL, outcome TEXT NOT NULL);
CREATE TABLE outbox (intent_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, command TEXT NOT NULL, action TEXT NOT NULL,
 workspace_id TEXT NOT NULL, task_id TEXT NOT NULL, link_id TEXT NOT NULL REFERENCES link(link_id), generation INTEGER NOT NULL,
 link_version INTEGER NOT NULL, actor TEXT NOT NULL, fields TEXT NOT NULL, policy_revision TEXT NOT NULL,
 capability_revision TEXT NOT NULL, payload_digest TEXT NOT NULL, causation_id TEXT NOT NULL, idempotency_key TEXT NOT NULL,
 state TEXT NOT NULL, attempts INTEGER NOT NULL, max_attempts INTEGER NOT NULL, lease_token TEXT, receipt_digest TEXT,
 UNIQUE(link_id,generation,action,idempotency_key));
CREATE TABLE audit (audit_id TEXT PRIMARY KEY, decision_id TEXT NOT NULL, causation_id TEXT NOT NULL);
CREATE TABLE remote_receipt (idempotency_key TEXT PRIMARY KEY, receipt_digest TEXT NOT NULL);
SQL);
        $this->db->table('task')->insert(['task_id' => 'task-one', 'title' => 'before', 'version' => 1]);
    }

    public function attach(string $task, string $external = 'object-one', string $link = 'link-one'): string
    {
        return $this->db->transaction(function () use ($task, $external, $link): string {
            $inserted = $this->db->table('link')->insertOrIgnore(['link_id' => $link, 'task_id' => $task,
                'workspace_id' => 'workspace-one', 'installation_id' => 'installation-one', 'provider_id' => 'github-issues',
                'object_type' => 'issue', 'external_id' => $external]);
            $row = $this->db->table('link')->where('workspace_id', 'workspace-one')->where('installation_id', 'installation-one')
                ->where('provider_id', 'github-issues')->where('object_type', 'issue')->where('external_id', $external)->first();
            if ($row === null) { throw new \RuntimeException('identity-reservation-missing'); }
            return $row->task_id !== $task ? 'identity_already_bound' : ($inserted === 1 ? 'attached' : 'replayed');
        });
    }

    public function request(string $intent, string $run, string $policy, string $capability, int $maxAttempts, ?\Closure $beforeCommit = null): void
    {
        $this->db->transaction(function () use ($intent, $run, $policy, $capability, $maxAttempts, $beforeCommit): void {
            $link = $this->db->table('link')->where('link_id', 'link-one')->first();
            if ($link === null || $link->state !== 'active' || $link->task_id !== 'task-one') { throw new \RuntimeException('scope-denied'); }
            $fields = '["state","title"]';
            $this->db->table('task')->where('task_id', 'task-one')->update(['title' => $intent]);
            $this->db->table('decision')->insert(['decision_id' => $intent, 'link_id' => 'link-one', 'actor' => 'identity.user',
                'fields' => $fields, 'policy_revision' => $policy, 'capability_revision' => $capability, 'outcome' => 'allowed']);
            $this->db->table('outbox')->insert(['intent_id' => $intent, 'run_id' => $run, 'command' => 'tasklinks.request_action',
                'action' => 'tasklinks.request_action', 'workspace_id' => 'workspace-one', 'task_id' => 'task-one',
                'link_id' => 'link-one', 'generation' => 1, 'link_version' => 1, 'actor' => 'identity.user', 'fields' => $fields,
                'policy_revision' => $policy, 'capability_revision' => $capability, 'payload_digest' => 'sha256:'.hash('sha256', $fields),
                'causation_id' => $intent, 'idempotency_key' => $intent, 'state' => 'pending', 'attempts' => 0, 'max_attempts' => $maxAttempts]);
            $this->db->table('audit')->insert(['audit_id' => $intent, 'decision_id' => $intent, 'causation_id' => $intent]);
            if ($beforeCommit !== null) { $beforeCommit(); }
        });
    }

    public function claim(string $intent, string $token, string $policy, string $capability): string
    {
        // One guarded SQL transition: a concurrent worker cannot borrow the
        // lease or the pre-restart authorization. Every persisted context
        // member participates in admission, not merely state/attempt/token.
        $changed = $this->db->update(<<<'SQL'
UPDATE outbox SET state='leased', lease_token=?, attempts=attempts+1
WHERE intent_id=? AND state IN ('pending','retry_wait') AND attempts<max_attempts
 AND command='tasklinks.request_action' AND action='tasklinks.request_action'
 AND actor='identity.user' AND workspace_id='workspace-one' AND task_id='task-one'
 AND fields='["state","title"]' AND policy_revision=? AND capability_revision=?
 AND EXISTS(SELECT 1 FROM link WHERE link.link_id=outbox.link_id AND link.task_id=outbox.task_id
  AND link.workspace_id=outbox.workspace_id AND link.generation=outbox.generation AND link.version=outbox.link_version AND link.state='active')
 AND EXISTS(SELECT 1 FROM decision WHERE decision.decision_id=outbox.intent_id AND decision.outcome='allowed'
  AND decision.actor=outbox.actor AND decision.fields=outbox.fields AND decision.policy_revision=outbox.policy_revision
  AND decision.capability_revision=outbox.capability_revision)
SQL, [$token, $intent, $policy, $capability]);
        return $changed === 1 ? 'claimed' : 'refused';
    }

    public function accept(string $intent, string $token): void
    {
        // Remote provider deduplication is a synthetic PDO-backed receipt
        // service in this battery, committed OUTSIDE the local transaction.
        $row = $this->db->table('outbox')->where('intent_id', $intent)->where('state', 'leased')->where('lease_token', $token)->first();
        if ($row === null) { throw new \RuntimeException('lease-stale'); }
        $receipt = 'sha256:'.hash('sha256', $row->idempotency_key);
        $this->db->table('remote_receipt')->insertOrIgnore(['idempotency_key' => $row->idempotency_key, 'receipt_digest' => $receipt]);
        $this->db->table('outbox')->where('intent_id', $intent)->where('lease_token', $token)->update(['state' => 'accepted_pending_ack']);
    }

    public function recover(string $intent): string
    {
        $this->db->table('outbox')->where('intent_id', $intent)->where('state', 'accepted_pending_ack')
            ->update(['state' => 'unknown_delivery', 'lease_token' => null]);
        return (string) $this->db->table('outbox')->where('intent_id', $intent)->value('state');
    }

    public function reconcile(string $intent): string
    {
        return $this->db->transaction(function () use ($intent): string {
            $row = $this->db->table('outbox')->where('intent_id', $intent)->where('state', 'unknown_delivery')->first();
            if ($row === null) { return 'refused'; }
            $receipt = $this->db->table('remote_receipt')->where('idempotency_key', $row->idempotency_key)->value('receipt_digest');
            if ($receipt === null) { return 'unknown_delivery'; }
            $this->db->table('outbox')->where('intent_id', $intent)->update(['state' => 'delivered', 'receipt_digest' => $receipt]);
            return 'delivered';
        });
    }

    public function fail(string $intent, string $token): string
    {
        $this->db->update("UPDATE outbox SET state=CASE WHEN attempts>=max_attempts THEN 'dead_letter' ELSE 'retry_wait' END, lease_token=NULL WHERE intent_id=? AND state='leased' AND lease_token=?", [$intent, $token]);
        return (string) $this->db->table('outbox')->where('intent_id', $intent)->value('state');
    }

    public function compareAndSwap(int $expected, string $title): string
    {
        return $this->db->update('UPDATE task SET title=?,version=version+1 WHERE task_id=? AND version=?', [$title, 'task-one', $expected]) === 1 ? 'updated' : 'stale';
    }
}
