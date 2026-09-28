<?php

declare(strict_types=1);

use App\Models\Task;
use Laratesto\Attribute\DatabaseMigrations;
use Laratesto\Testing\LaravelTestCase;
use Tests\Support\PlannerPort;

// The port self-test of the planner fixture (issue #56, S4): the
// closed port surface is proven against the real Laravel kernel and
// the real database. The generated scenario tests depend on every rule
// below; a violation here is infrastructure, never a silent pass.

final class PortSelftestTest extends LaravelTestCase
{
    #[DatabaseMigrations]
    public function testPortSurfaceAnswers(): void
    {
        $port = $this->app()->make(PlannerPort::class);
        $port->clock()->freeze('2026-05-06T07:08:09Z');
        $port->ids()->seed(['algorithm' => 'sequence', 'seed' => 'planner-1']);

        // state seed + typed invoke over the real kernel and database.
        $port->state()->seed('planner.task', ['task_id' => 'task-1'], ['user_id' => 'user-1']);
        $outcome = $port->invoke('planner.focus_task', ['task_id' => 'task-1', 'user_id' => 'user-1'], []);
        \Testo\Assert::same(true, $outcome['ok']);

        // The persisted effect is observable through a real query.
        $rows = $port->state()->query('planner.task', ['task_id' => 'task-1']);
        \Testo\Assert::same(1, count($rows));
        \Testo\Assert::same(true, $rows[0]['focused']);

        // The typed missing-task error is a domain answer, not a throw.
        $missing = $port->invoke('planner.focus_task', ['task_id' => 'task-404'], []);
        \Testo\Assert::same(false, $missing['ok']);
        \Testo\Assert::same('planner.error.task_missing', $missing['error']['id']);

        // The idempotency key deduplicates: one emission for the keyed
        // pair, identical outcomes for the replay.
        $before = count($port->emissions());
        $first = $port->invoke('planner.focus_task', ['task_id' => 'task-1'], ['idempotencyKey' => 'user-1']);
        $replay = $port->invoke('planner.focus_task', ['task_id' => 'task-1'], ['idempotencyKey' => 'user-1']);
        \Testo\Assert::same($first, $replay);
        \Testo\Assert::same(1, count($port->emissions()) - $before);

        // Control surfaces answer: actor, authorize, contractCheck, fixtureDigest.
        \Testo\Assert::same(['ref' => 'planner/member', 'scope' => 'planner'], $port->actor('planner/member', 'planner'));
        \Testo\Assert::same('allowed', $port->authorize('agent-1', 'planner.deny_bulk_focus', 'planner.focus_task'));
        \Testo\Assert::same('denied', $port->authorize('bulk-agent', 'planner.deny_bulk_focus', 'planner.focus_task'));
        \Testo\Assert::same(true, $port->contractCheck('core.contracts/focus-state', ['focused'], ['focused' => true]));
        \Testo\Assert::same(false, $port->contractCheck('core.contracts/focus-state', ['missing'], ['focused' => true]));
        $digest = $port->fixtureDigest('core/planner-seed');
        \Testo\Assert::same($digest, $port->fixtureDigest('core/planner-seed'));

        // The property-fluent spellings the generated tests compile to:
        // the declared surface answers identically through both shapes.
        $port->state->seed('planner.task', ['task_id' => 'task-9'], ['user_id' => 'user-2']);
        \Testo\Assert::same(1, count($port->state->query('planner.task', ['task_id' => 'task-9'])));
        $port->clock->freeze('2026-05-06T07:08:09Z');
        $port->ids->seed(['algorithm' => 'sequence', 'seed' => 'planner-1']);
        $rows = $port->fixtures->load('core/planner-seed');
        \Testo\Assert::same(1, count($rows));
        \Testo\Assert::same(true, isset($rows[0]['task_id']));

        // Reset wipes every surface: reruns start clean.
        $port->reset();
        \Testo\Assert::same(0, count($port->emissions()));
        \Testo\Assert::same(0, count($port->state()->query('planner.task', [])));
    }
}
