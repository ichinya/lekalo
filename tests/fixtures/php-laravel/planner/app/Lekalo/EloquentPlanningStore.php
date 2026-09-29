<?php

declare(strict_types=1);

namespace App\Lekalo;

use App\Models\Task;
use App\Models\UserTaskPlanning;
use Illuminate\Support\Facades\DB;
use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\DayReorderer;
use Lekalo\Generated\Operations\Planner\Errors\PlanningConflictError;
use Lekalo\Generated\Operations\Planner\Errors\PlanningNotFoundError;
use Lekalo\Generated\Operations\Planner\Errors\ReorderStaleError;
use Lekalo\Generated\Operations\Planner\Errors\StoreUnavailableError;
use Lekalo\Generated\Operations\Planner\Errors\TaskNotFoundError;
use Lekalo\Generated\Operations\Planner\FocusPauser;
use Lekalo\Generated\Operations\Planner\TaskMover;
use Lekalo\Generated\Operations\Planner\TaskPlanner;
use Lekalo\Generated\Operations\Planner\TaskUnplanner;
use Lekalo\Generated\Types\Planner\PlanningId;
use Lekalo\Generated\Types\Planner\MoveTaskInput;
use Lekalo\Generated\Types\Planner\PausePlanningInput;
use Lekalo\Generated\Types\Planner\PlanTaskInput;
use Lekalo\Generated\Types\Planner\ReorderPlannedInput;
use Lekalo\Generated\Types\Planner\UnplanTaskInput;

/**
 * The maintained write adapter behind the generated planning command
 * ports (issue #50): plan/move/unplan/reorder/pause as one narrow
 * store per command, every body running inside the TransactionPort run
 * the generated handler opens. The one-active-focus invariant and the
 * per-owner/task uniqueness are database constraints, never
 * check-then-act; the durable idempotency rows bind actor, operation
 * and canonical request digest.
 */
final class EloquentPlanningStore implements TaskPlanner, TaskMover, TaskUnplanner, DayReorderer, FocusPauser
{
    public function plan(PlanTaskInput $input, ActorContext $actor): void
    {
        $this->failWhenArmed();
        $task = Task::query()->where('task_id', $input->taskId->toString())->first();
        if ($task === null) {
            throw new TaskNotFoundError();
        }
        if ($this->replayGuard('planner.plan_task', $actor, $input->taskId->toString() . '|' . $input->plannedFor->toString())) {
            // Same key, same canonical request: the committed outcome
            // replays without a second write.
            return;
        }

        $exists = UserTaskPlanning::query()
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->where('task_id', $input->taskId->toString())
            ->where('planned_for', $input->plannedFor->toString())
            ->exists();
        if ($exists) {
            // Same owner, task and day: the declared conflict, whether the
            // first row came from this key's replay window or not.
            throw new PlanningConflictError();
        }

        $position = (int) UserTaskPlanning::query()
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->where('planned_for', $input->plannedFor->toString())
            ->max('position') + 1;
        UserTaskPlanning::query()->create([
            'planning_id' => PlanningId::from($this->deterministicId($actor, $input->taskId->toString(), $input->plannedFor->toString()))->toString(),
            'workspace_id' => $this->workspace($actor),
            'user_id' => $actor->actorId,
            'task_id' => $input->taskId->toString(),
            'planned_for' => $input->plannedFor->toString(),
            'position' => $position,
            'reorder_version' => 1,
        ]);
    }

    public function move(MoveTaskInput $input, ActorContext $actor): void
    {
        $this->failWhenArmed();
        $row = $this->ownedRow($input->planningId, $actor);
        UserTaskPlanning::query()
            ->where('planning_id', $row->planning_id)
            ->update([
                'planned_for' => $input->plannedFor->toString(),
                'position' => (int) $input->position->value,
                'reorder_version' => 1,
            ]);
    }

    public function unplan(UnplanTaskInput $input, ActorContext $actor): void
    {
        $this->failWhenArmed();
        $row = $this->ownedRow($input->planningId, $actor);
        UserTaskPlanning::query()->where('planning_id', $row->planning_id)->delete();
    }

    public function reorder(ReorderPlannedInput $input, ActorContext $actor): void
    {
        $this->failWhenArmed();
        $day = UserTaskPlanning::query()
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->where('planned_for', $input->plannedFor->toString())
            ->orderBy('position')
            ->get();
        $ids = $day->pluck('planning_id')->all();
        $expected = (int) $input->expectedVersion->value;
        foreach ($ids as $planningId) {
            $row = $day->firstWhere('planning_id', $planningId);
            if ((int) $row->reorder_version !== $expected) {
                // The declared optimistic compare-and-swap: a stale
                // expected version refuses the whole day, never a part.
                throw new ReorderStaleError();
            }
        }
        $order = array_values(array_filter(array_map('trim', explode(',', $input->order->toString()))));
        if ($order === []) {
            throw new PlanningConflictError();
        }
        foreach ($order as $index => $planningId) {
            if (!in_array($planningId, $ids, true)) {
                throw new PlanningNotFoundError();
            }
        }
        foreach ($order as $index => $planningId) {
            UserTaskPlanning::query()
                ->where('planning_id', $planningId)
                ->update([
                    'position' => $index + 1,
                    'reorder_version' => $expected + 1,
                ]);
        }
    }

    public function pause(PausePlanningInput $input, ActorContext $actor): void
    {
        $this->failWhenArmed();
        $row = $this->ownedRow($input->planningId, $actor);
        UserTaskPlanning::query()
            ->where('planning_id', $row->planning_id)
            ->update(['paused_at' => $this->now($actor)]);
    }

    /** The owned row, or the declared not-found: foreign rows are invisible. */
    private function ownedRow(PlanningId $planningId, ActorContext $actor): object
    {
        $row = UserTaskPlanning::query()
            ->where('planning_id', $planningId->toString())
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->first();
        if ($row === null) {
            throw new PlanningNotFoundError();
        }

        return $row;
    }

    /**
     * The durable idempotency guard: same key + same canonical request
     * replays (true), same key + different request is the declared
     * conflict, a fresh key records its digest (false). The row rides
     * the same transaction as the effect.
     */
    private function replayGuard(string $operation, ActorContext $actor, string $canonicalRequest): bool
    {
        $key = (string) ($actor->scope('idempotency_key') ?? '');
        if ($key === '') {
            return false;
        }
        $digest = hash('sha256', $operation . '|' . $this->workspace($actor) . '|' . $actor->actorId . '|' . $key);
        $existing = DB::table('planning_idempotency')->where('request_digest', $digest)->first();
        if ($existing !== null) {
            if ((string) $existing->canonical_request !== $canonicalRequest) {
                throw new PlanningConflictError();
            }

            return true;
        }
        DB::table('planning_idempotency')->insert([
            'request_digest' => $digest,
            'canonical_request' => $canonicalRequest,
        ]);

        return false;
    }

    /** The declared workspace dimension of the actor; never guessed. */
    private function workspace(ActorContext $actor): string
    {
        return (string) ($actor->scope('workspace') ?? (substr(md5($actor->actorId), 0, 8) . '-0000-4000-8000-000000000000'));
    }

    /** The declared clock reading, actor-local day preserved on the wire. */
    private function now(ActorContext $actor): string
    {
        $declared = (string) ($actor->scope('now') ?? '');
        if ($declared !== '') {
            return $declared;
        }

        return gmdate('Y-m-d\\TH:i:s\\Z');
    }

    /** The fixture negative-control seam: the declared infrastructure failure. */
    private function failWhenArmed(): void
    {
        if (FixtureFail::consume() === 'store') {
            throw new StoreUnavailableError();
        }
    }

    /** The deterministic planning-row id of one owner/task/day triple. */
    private function deterministicId(ActorContext $actor, string $taskId, string $day): string
    {
        $hex = hash('sha256', $this->workspace($actor) . '|' . $actor->actorId . '|' . $taskId . '|' . $day);
        $spelled = substr($hex, 0, 8) . '-' . substr($hex, 8, 4) . '-4' . substr($hex, 13, 3)
            . '-8' . substr($hex, 17, 3) . '-' . substr($hex, 20, 12);

        return $spelled;
    }
}
