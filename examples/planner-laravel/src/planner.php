<?php

declare(strict_types=1);

use Illuminate\Database\Connection;
use Illuminate\Http\Request;

final class ReferenceRefusal extends RuntimeException
{
    public function __construct(public readonly int $status, string $code)
    {
        parent::__construct($code);
    }
}

final class ReferencePlanner
{
    public function __construct(private readonly Connection $db) {}

    private function refuse(int $status, string $code): never
    {
        throw new ReferenceRefusal($status, $code);
    }

    private function validDate(mixed $date): bool
    {
        if (!is_string($date) || !preg_match('/^\d{4}-\d{2}-\d{2}$/D', $date) || str_starts_with($date, '0000-')) {
            return false;
        }
        $parsed = DateTimeImmutable::createFromFormat('!Y-m-d', $date);
        return $parsed !== false && $parsed->format('Y-m-d') === $date;
    }

    public function handle(Request $request): array
    {
        $this->db->statement("SET application_name = 'reference-laravel'");
        $path = $request->getPathInfo();
        $method = $request->getMethod();
        if (strlen($request->getContent()) > 16384) {
            $this->refuse(413, 'body_too_large');
        }
        if ($path === '/health' && $method === 'GET') {
            $this->db->select('SELECT 1');
            return ['ok' => true];
        }
        $actor = $request->header('X-Actor');
        $tenant = $request->header('X-Tenant');
        if (!$actor || !$tenant) {
            $this->refuse(401, 'actor_required');
        }
        $scope = [$tenant, $actor];
        if (!$this->db->select('SELECT 1 FROM reference_actors WHERE tenant=? AND user_id=?', $scope)) {
            $this->refuse(403, 'actor_denied');
        }
        $now = getenv('REFERENCE_NOW') ?: gmdate('Y-m-d\TH:i:s\Z');
        try {
            $today = (new DateTimeImmutable($now))->setTimezone(new DateTimeZone($request->header('X-Timezone', 'UTC')))->format('Y-m-d');
        } catch (Throwable) {
            $this->refuse(422, 'invalid_timezone');
        }
        if ($method === 'GET') {
            if ($path === '/planning/backlog') {
                return ['items' => $this->db->select('SELECT task_id,title FROM reference_tasks t WHERE tenant=? AND user_id=?
                    AND NOT EXISTS (SELECT 1 FROM reference_planning p WHERE p.tenant=t.tenant AND p.user_id=t.user_id AND p.task_id=t.task_id) ORDER BY task_id', $scope)];
            }
            if (!in_array($path, ['/planning/today', '/planning/carry-over'], true)) {
                $this->refuse(404, 'not_found');
            }
            $comparison = $path === '/planning/today' ? '=' : '<';
            $rows = $this->db->select("SELECT planning_id,task_id,planned_for::text,position,version,focused,paused_at FROM reference_planning
                WHERE tenant=? AND user_id=? AND planned_for $comparison ?::date ORDER BY planned_for,position,planning_id", [...$scope, $today]);
            foreach ($rows as $row) {
                $row->position = (int) $row->position;
                $row->version = (int) $row->version;
                $row->focused = filter_var($row->focused, FILTER_VALIDATE_BOOLEAN);
            }
            return ['items' => $rows];
        }
        if (!in_array($method, ['POST', 'DELETE'], true)) {
            $this->refuse(404, 'not_found');
        }
        $key = $request->header('Idempotency-Key');
        if (!$key || !preg_match('/^[A-Za-z0-9._-]{1,80}$/D', $key)) {
            $this->refuse(422, 'idempotency_key_required');
        }
        try {
            $object = json_decode($request->getContent() ?: '{}', false, 32, JSON_THROW_ON_ERROR);
        } catch (JsonException) {
            $this->refuse(422, 'invalid_request');
        }
        if (!is_object($object)) {
            $this->refuse(422, 'invalid_request');
        }
        $body = get_object_vars($object);
        ksort($body);
        try {
            $canonical = json_encode((object) $body, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
        } catch (JsonException) {
            $this->refuse(422, 'invalid_request');
        }
        $digest = hash('sha256', $method.'|'.$path.'|'.$canonical);
        return $this->db->transaction(function () use ($request, $path, $method, $scope, $actor, $tenant, $key, $body, $digest, $now, $today): array {
            $this->db->select('SELECT 1 FROM reference_actors WHERE tenant=? AND user_id=? FOR UPDATE', $scope);
            $old = $this->db->select('SELECT digest,response FROM reference_receipts WHERE tenant=? AND user_id=? AND key=?', [...$scope, $key]);
            if ($old) {
                if ($old[0]->digest !== $digest) {
                    $this->refuse(409, 'idempotency_conflict');
                }
                return json_decode($old[0]->response, true, 32, JSON_THROW_ON_ERROR);
            }
            if ($path === '/planning' && $method === 'POST') {
                if (array_keys($body) !== ['planned_for', 'task_id'] || !is_string($body['task_id'])
                    || !preg_match('/^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/D', $body['task_id']) || !$this->validDate($body['planned_for'])) {
                    $this->refuse(422, 'invalid_request');
                }
                if (!$this->db->select('SELECT 1 FROM reference_tasks WHERE tenant=? AND user_id=? AND task_id=?', [...$scope, $body['task_id']])) {
                    $this->refuse(404, 'not_found');
                }
                if ($this->db->select('SELECT 1 FROM reference_planning WHERE tenant=? AND user_id=? AND task_id=? AND planned_for=?::date', [...$scope, $body['task_id'], $body['planned_for']])) {
                    $this->refuse(409, 'planning_conflict');
                }
                // Standard UUIDv5, identical to the Node reference.
                $hash = hash('sha1', hex2bin('6ba7b8109dad11d180b400c04fd430c8')."$tenant|$actor|{$body['task_id']}|{$body['planned_for']}");
                $id = substr($hash, 0, 8).'-'.substr($hash, 8, 4).'-5'.substr($hash, 13, 3).'-'.dechex((hexdec($hash[16]) & 3) | 8).substr($hash, 17, 3).'-'.substr($hash, 20, 12);
                // The actor lock also serializes this insert with day reorders.
                $this->db->insert('INSERT INTO reference_planning (tenant,user_id,planning_id,task_id,planned_for,position,version)
                    SELECT ?,?,?,?,?::date,COALESCE(MAX(position),0)+1,COALESCE(MAX(version),1) FROM reference_planning WHERE tenant=? AND user_id=? AND planned_for=?::date',
                    [...$scope, $id, $body['task_id'], $body['planned_for'], ...$scope, $body['planned_for']]);
            } elseif ($path === '/planning/reorder' && $method === 'POST') {
                if (array_keys($body) !== ['expected_version', 'order', 'planned_for'] || !$this->validDate($body['planned_for'])
                    || !(is_int($body['expected_version']) || (is_float($body['expected_version']) && is_finite($body['expected_version']) && floor($body['expected_version']) === $body['expected_version']))
                    || $body['expected_version'] < 1 || $body['expected_version'] >= 2147483647 || !is_string($body['order'])) {
                    $this->refuse(422, 'invalid_request');
                }
                $rows = $this->db->select('SELECT planning_id,version FROM reference_planning WHERE tenant=? AND user_id=? AND planned_for=?::date ORDER BY planning_id', [...$scope, $body['planned_for']]);
                $ids = array_map(fn ($row) => $row->planning_id, $rows);
                $order = explode(',', $body['order']);
                if (!$rows || count($order) !== count($ids) || count(array_unique($order)) !== count($order) || array_diff($order, $ids)) {
                    $this->refuse(422, 'invalid_order');
                }
                foreach ($rows as $row) {
                    if ((int) $row->version !== (int) $body['expected_version']) {
                        $this->refuse(409, 'reorder_stale');
                    }
                }
                foreach ($order as $index => $id) {
                    $this->db->update('UPDATE reference_planning SET position=?,version=version+1 WHERE tenant=? AND user_id=? AND planning_id=?', [$index + 1, ...$scope, $id]);
                }
            } elseif ($method === 'POST' && preg_match('#^/tasks/([^/]+)/focus$#D', $path, $match)) {
                if ($body) {
                    $this->refuse(422, 'invalid_request');
                }
                $target = $this->db->select('SELECT planning_id FROM reference_planning WHERE tenant=? AND user_id=? AND task_id=? AND planned_for=?::date', [...$scope, $match[1], $today]);
                if (!$target) {
                    $this->refuse(404, 'not_found');
                }
                $id = $target[0]->planning_id;
                if ($this->db->select('SELECT 1 FROM reference_planning WHERE tenant=? AND user_id=? AND focused AND planning_id<>?', [...$scope, $id])) {
                    $this->refuse(409, 'focus_conflict');
                }
                $this->db->update('UPDATE reference_planning SET focused=true,paused_at=NULL WHERE tenant=? AND user_id=? AND planning_id=?', [...$scope, $id]);
            } else {
                if (!preg_match('#^/planning/([a-f0-9-]{36})(/pause)?$#D', $path, $match)
                    || !(($method === 'DELETE' && !isset($match[2])) || ($method === 'POST' && isset($match[2]))) || $body) {
                    $this->refuse(404, 'not_found');
                }
                if (!$this->db->select('SELECT 1 FROM reference_planning WHERE tenant=? AND user_id=? AND planning_id=?', [...$scope, $match[1]])) {
                    $this->refuse(404, 'not_found');
                }
                if ($method === 'DELETE') {
                    $this->db->delete('DELETE FROM reference_planning WHERE tenant=? AND user_id=? AND planning_id=?', [...$scope, $match[1]]);
                } else {
                    $this->db->update('UPDATE reference_planning SET focused=false,paused_at=? WHERE tenant=? AND user_id=? AND planning_id=?', [$now, ...$scope, $match[1]]);
                }
            }
            if (getenv('REFERENCE_TEST_FAULTS') === '1' && $request->header('X-Reference-Fault') === 'rollback') {
                $this->refuse(503, 'store_unavailable');
            }
            $this->db->insert('INSERT INTO reference_receipts VALUES (?,?,?,?,?::jsonb)', [...$scope, $key, $digest, '{"ok":true}']);
            return ['ok' => true];
        });
    }
}
