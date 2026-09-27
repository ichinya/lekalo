<?php

declare(strict_types=1);

namespace Tests\Support;

use App\Events\TaskFocused;
use App\Models\Task;
use Illuminate\Support\Facades\Event;
use Testo\Assert;

/**
 * The planner ScenarioPort of the fixture project (issue #56, S4).
 *
 * This class is the ONLY seam generated scenario tests may bind to: the
 * project's `lekalo/test-port.json` declares it, and the generated
 * `port.php` shim resolves exactly this class. The closed port surface
 * mirrors the Node fixture port (issue #47):
 *
 * - `invoke(operationId, input, ctx)` → `array{ok: bool, value?: array,
 *   error?: array{id: string, fields: array}}`; the dispatch binds
 *   operations to the real HTTP kernel (the planner model's endpoint),
 *   and `ctx.idempotencyKey` deduplicates; infrastructure faults THROW,
 *   typed domain errors RETURN.
 * - `state->seed` / `state->query` persist and read the real database.
 * - `emissions()` / `effects()` capture the event and effect ledger.
 * - `clock->freeze` / `ids->seed` / `actor()` / `authorize()` /
 *   `contractCheck()` / `fixtureDigest()` / `reset()` mirror the closed
 *   control surfaces; `reset()` gives the rerun-isolation guarantee.
 *
 * No network, no production secrets, no host state outside the
 * fixture's own database.
 */
final class PlannerPort
{
    /** @var array<string, array> the idempotency cache: key => outcome */
    private array $idempotencyCache = [];

    /** @var array<array{id: string, kind: string, operation?: string}> */
    private array $emissionLog = [];

    /** @var array<array{effect: string, entity?: string, operation?: string}> */
    private array $effectLedger = [];

    public ?string $frozenClock = null;

    public ?string $idSeed = null;

    public function __construct()
    {
        // Only the in-memory surfaces are cleared at construction; the
        // database lifecycle belongs to the migrations attribute (the
        // fresh migrations + transaction rollback guarantee isolation).
        $this->resetInMemory();
    }

    /** Clear every in-memory surface without touching the database. */
    public function resetInMemory(): void
    {
        $this->idempotencyCache = [];
        $this->emissionLog = [];
        $this->effectLedger = [];
        $this->frozenClock = null;
        $this->idSeed = null;
    }

    /** The closed dispatch surface. */
    public function invoke(string $operationId, array $input, array $ctx): array
    {
        $key = $ctx['idempotencyKey'] ?? null;
        $cacheKey = null;
        if ($key !== null && $key !== '') {
            $cacheKey = $operationId . "\0" . json_encode($key, JSON_THROW_ON_ERROR);
            if (isset($this->idempotencyCache[$cacheKey])) {
                // The replay returns the cached outcome and never re-emits.
                return $this->idempotencyCache[$cacheKey];
            }
        }

        $outcome = $this->dispatch($operationId, $input, $ctx);

        if ($cacheKey !== null && ($outcome['ok'] ?? false)) {
            $this->idempotencyCache[$cacheKey] = $outcome;
        }
        return $outcome;
    }

    private function dispatch(string $operationId, array $input, array $ctx): array
    {
        switch ($operationId) {
            case 'planner.focus_task':
                return $this->focusTask($input, $ctx);
            default:
                // Unresolved operations are infrastructure faults: the
                // generated test records an infrastructure row.
                throw new \LogicException('port.invoke: unresolved operation ' . $operationId);
        }
    }

    private function focusTask(array $input, array $ctx): array
    {
        $taskId = (string) ($input['task_id'] ?? '');
        $task = Task::query()->where('task_id', $taskId)->first();
        if ($task === null) {
            return [
                'ok' => false,
                'error' => ['id' => 'planner.error.task_missing', 'fields' => ['task_id' => $taskId]],
            ];
        }
        $focusedAt = $ctx['clock'] ?? $this->frozenClock ?? '2026-01-01T00:00:00Z';

        $response = app()->handle(\Illuminate\Http\Request::create(
            '/api/tasks/' . rawurlencode($taskId) . '/focus',
            'POST',
            ['focused_at' => $focusedAt],
        ));

        if ($response->getStatusCode() !== 200) {
            // The transport contract is violated by our own fixture:
            // that is infrastructure, never a domain error.
            throw new \RuntimeException('focus endpoint answered ' . $response->getStatusCode());
        }
        $body = json_decode((string) $response->getContent(), true, 64, JSON_THROW_ON_ERROR);
        if (!($body['ok'] ?? false)) {
            return ['ok' => false, 'error' => $body['error']];
        }
        $this->emissionLog[] = ['id' => 'planner.task_focused', 'kind' => 'event', 'operation' => 'planner.focus_task'];
        $this->effectLedger[] = ['effect' => 'planner.create_task', 'entity' => 'planner.task', 'operation' => 'planner.focus_task'];
        return ['ok' => true, 'value' => $body['value']];
    }

    /** The state surface: reads and seeds the real database. */
    public function state(): object
    {
        return new class($this) {
            public function __construct(private readonly PlannerPort $port) {}

            public function seed(string $entity, array $selector, array $fields): array
            {
                if ($entity !== 'planner.task') {
                    throw new \LogicException('unknown entity ' . $entity);
                }
                Task::query()->updateOrCreate(
                    ['task_id' => (string) $selector['task_id']],
                    ['user_id' => (string) ($fields['user_id'] ?? ''), 'focused' => false, 'focused_at' => null],
                );
                return $this->query($entity, $selector)[0];
            }

            public function query(string $entity, array $selector): array
            {
                if ($entity !== 'planner.task') {
                    throw new \LogicException('unknown entity ' . $entity);
                }
                $q = Task::query();
                foreach ($selector as $field => $value) {
                    $q->where($field, $value);
                }
                return $q->get()->map(static fn (Task $task): array => $task->only(
                    ['task_id', 'user_id', 'focused', 'focused_at'],
                ))->all();
            }
        };
    }

    /** The capture-log surface. */
    public function emissions(): array
    {
        return array_map(static fn (array $e): array => [...$e], $this->emissionLog);
    }

    /** The effect-ledger surface. */
    public function effects(): array
    {
        return array_map(static fn (array $e): array => [...$e], $this->effectLedger);
    }

    public function actor(string $ref, ?string $scope = null): array
    {
        return $scope === null ? ['ref' => $ref] : ['ref' => $ref, 'scope' => $scope];
    }

    /** The deterministic clock surface. */
    public function clock(): object
    {
        return new class($this) {
            public function __construct(private readonly PlannerPort $port) {}

            public function freeze(string $isoUtc): void
            {
                $this->port->frozenClock = $isoUtc;
            }
        };
    }

    /** The deterministic ID surface. */
    public function ids(): object
    {
        return new class($this) {
            public function __construct(private readonly PlannerPort $port) {}

            public function seed(array $spec): void
            {
                $this->port->idSeed = $spec['algorithm'] ?? null;
            }
        };
    }

    public function authorize(?string $actor, ?string $policy, ?string $operation): string
    {
        // The fixture policy: the deny-bulk-focus policy denies the bulk
        // actor; everything else is allowed. Authorization is data, not
        // a guessed side effect of the operation name.
        if ($policy === 'planner.deny_bulk_focus' && $actor !== null && str_contains($actor, 'bulk')) {
            return 'denied';
        }
        return 'allowed';
    }

    public function contractCheck(?string $contract, array $projection, mixed $actual): bool
    {
        if (!is_array($actual)) {
            return false;
        }
        $paths = $projection !== [] ? $projection : array_keys($actual);
        foreach ($paths as $path) {
            $cursor = $actual;
            foreach (explode('.', (string) $path) as $segment) {
                if (!is_array($cursor) || !array_key_exists($segment, $cursor)) {
                    return false;
                }
                $cursor = $cursor[$segment];
            }
        }
        return true;
    }

    public function fixtureDigest(?string $fixtureId): string
    {
        // The digest covers the fixture identity, the seeded id source,
        // and the frozen clock: a deterministic_fixture assertion
        // transitively asserts its declared control refs.
        return 'sha256:' . hash('sha256', json_encode([
            'clock' => $this->frozenClock,
            'fixture' => $fixtureId,
            'seed' => $this->idSeed,
        ], JSON_THROW_ON_ERROR));
    }

    /** The rerun-isolation guarantee: wipes every surface. */
    public function reset(): void
    {
        $this->resetInMemory();
        Task::query()->delete();
    }

    /** The named-fixture surface: loads a declared seed fixture. */
    public function fixtures(): object
    {
        return new class($this) {
            public function __construct(private readonly PlannerPort $port) {}

            /** Load one declared fixture: seeds the real DB and returns the rows. */
            public function load(string $fixtureId): array
            {
                return match ($fixtureId) {
                    'core/planner-seed' => [
                        $this->port->state()->seed('planner.task', ['task_id' => 'fixture-1'], ['user_id' => 'fixture-user']),
                    ],
                    default => throw new \LogicException('unknown fixture ' . $fixtureId),
                };
            }
        };
    }

    /** The property-fluent spellings the generated tests use. */
    public function __get(string $name): mixed
    {
        return match ($name) {
            'state' => $this->state(),
            'clock' => $this->clock(),
            'ids' => $this->ids(),
            'fixtures' => $this->fixtures(),
            default => throw new \LogicException('unknown port surface ' . $name),
        };
    }
}
