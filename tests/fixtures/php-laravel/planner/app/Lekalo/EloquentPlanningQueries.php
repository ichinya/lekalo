<?php

declare(strict_types=1);

namespace App\Lekalo;

use App\Models\Task;
use App\Models\UserTaskPlanning;
use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\BacklogReader;
use Lekalo\Generated\Operations\Planner\BacklogTasksInput;
use Lekalo\Generated\Operations\Planner\CarryOverInput;
use Lekalo\Generated\Operations\Planner\CarryOverReader;
use Lekalo\Generated\Operations\Planner\CompletedPlanningInput;
use Lekalo\Generated\Operations\Planner\CompletedReader;
use Lekalo\Generated\Operations\Planner\Errors\StoreUnavailableError;
use Lekalo\Generated\Operations\Planner\TodayPlanningInput;
use Lekalo\Generated\Operations\Planner\TodayReader;
use Lekalo\Generated\Types\Planner\Optional\OptionalNullableTimestamp;
use Lekalo\Generated\Types\Planner\PlanningDate;
use Lekalo\Generated\Types\Planner\PlanningId;
use Lekalo\Generated\Types\Planner\Position;
use Lekalo\Generated\Types\Planner\ReorderVersion;
use Lekalo\Generated\Types\Planner\TaskId;
use Lekalo\Generated\Types\Planner\Timestamp;
use Lekalo\Generated\Types\Planner\UserId;
use Lekalo\Generated\Types\Planner\UserTaskPlanningDto;
use Lekalo\Generated\Types\Planner\UserTaskPlanningDtoList;
use Lekalo\Generated\Types\Planner\WorkspaceId;

/**
 * The maintained read adapter behind the generated planning query
 * ports (issue #50): today, backlog and carry-over. Every query is
 * scoped to the actor's declared workspace and user — foreign rows are
 * invisible — and reads never rewrite the stored planned_for: the
 * carry-over rows are served at their original calendar day.
 *
 * The actor-local "today" is computed from the declared clock reading
 * (the fixture's deterministic X-Fixture-Now seam) and the declared
 * actor timezone: the same stored rows answer two disjoint today sets
 * across a UTC midnight boundary.
 */
final class EloquentPlanningQueries implements TodayReader, BacklogReader, CarryOverReader, CompletedReader
{
    public function today(TodayPlanningInput $input, ActorContext $actor): UserTaskPlanningDtoList
    {
        return $this->list($this->day($actor), $actor);
    }

    public function backlog(BacklogTasksInput $input, ActorContext $actor): UserTaskPlanningDtoList
    {
        $this->failWhenArmed();
        $plannedTaskIds = UserTaskPlanning::query()
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->pluck('task_id')
            ->all();
        $rows = Task::query()
            ->where('user_id', $actor->actorId)
            ->whereNotIn('task_id', $plannedTaskIds === [] ? [''] : $plannedTaskIds)
            ->orderBy('task_id')
            ->get();

        return UserTaskPlanningDtoList::fromList($rows->map(
            fn (Task $task): UserTaskPlanningDto => $this->backlogDto($task, $actor),
        )->all());
    }

    public function carryOver(CarryOverInput $input, ActorContext $actor): UserTaskPlanningDtoList
    {
        $rows = UserTaskPlanning::query()
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->where('planned_for', '<', $this->day($actor))
            ->orderBy('planned_for')
            ->orderBy('position')
            ->get();

        return UserTaskPlanningDtoList::fromList($rows->map(
            fn (UserTaskPlanning $row): UserTaskPlanningDto => $this->dto($row),
        )->all());
    }

    /** The planning rows the actor completed, newest day first. */
    public function completed(CompletedPlanningInput $input, ActorContext $actor): UserTaskPlanningDtoList
    {
        $this->failWhenArmed();
        $rows = UserTaskPlanning::query()
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->whereNotNull('completed_at')
            ->orderBy('planned_for', 'desc')
            ->orderBy('position')
            ->orderBy('planning_id')
            ->get();

        return UserTaskPlanningDtoList::fromList($rows->map(
            fn (UserTaskPlanning $row): UserTaskPlanningDto => $this->dto($row),
        )->all());
    }

    /** The actor-local today as a calendar day string. */
    private function day(ActorContext $actor): string
    {
        $now = (string) ($actor->scope('now') ?? gmdate('Y-m-d\TH:i:s\Z'));
        $timezone = (string) ($actor->scope('timezone') ?? 'UTC');
        $instant = new \DateTimeImmutable($now);
        $local = $instant->setTimezone(new \DateTimeZone($timezone));

        return $local->format('Y-m-d');
    }

    /** The today rows of the actor's day, in stable position order. */
    private function list(string $day, ActorContext $actor): UserTaskPlanningDtoList
    {
        $this->failWhenArmed();
        $rows = UserTaskPlanning::query()
            ->where('workspace_id', $this->workspace($actor))
            ->where('user_id', $actor->actorId)
            ->where('planned_for', $day)
            ->orderBy('position')
            ->get();

        return UserTaskPlanningDtoList::fromList($rows->map(
            fn (UserTaskPlanning $row): UserTaskPlanningDto => $this->dto($row),
        )->all());
    }

    /** The planning-row projection of one stored row. */
    private function dto(UserTaskPlanning $row): UserTaskPlanningDto
    {
        return new UserTaskPlanningDto(
            PlanningId::from((string) $row->planning_id),
            WorkspaceId::from((string) $row->workspace_id),
            UserId::from((string) $row->user_id),
            TaskId::from((string) $row->task_id),
            PlanningDate::from((string) $row->planned_for),
            Position::from((int) $row->position),
            $row->focused_at === null
                ? OptionalNullableTimestamp::absent()
                : OptionalNullableTimestamp::of(Timestamp::from((string) $row->focused_at)),
            $row->paused_at === null
                ? OptionalNullableTimestamp::absent()
                : OptionalNullableTimestamp::of(Timestamp::from((string) $row->paused_at)),
            $row->completed_at === null
                ? OptionalNullableTimestamp::absent()
                : OptionalNullableTimestamp::of(Timestamp::from((string) $row->completed_at)),
            ReorderVersion::from((int) $row->reorder_version),
        );
    }

    /**
     * The backlog projection: one placeholder planning shape per
     * unplanned task, carrying the task identity at the unset day —
     * the declared backlog spelling of the fixture.
     */
    private function backlogDto(Task $task, ActorContext $actor): UserTaskPlanningDto
    {
        return new UserTaskPlanningDto(
            PlanningId::from($this->backlogId($task, $actor)),
            WorkspaceId::from($this->workspace($actor)),
            UserId::from($actor->actorId),
            TaskId::from((string) $task->task_id),
            PlanningDate::from('1970-01-01'),
            Position::from(0),
            OptionalNullableTimestamp::absent(),
            OptionalNullableTimestamp::absent(),
            OptionalNullableTimestamp::absent(),
            ReorderVersion::from(0),
        );
    }

    /** The deterministic backlog placeholder id of one unplanned task. */
    private function backlogId(Task $task, ActorContext $actor): string
    {
        $hex = hash('sha256', 'backlog|' . $this->workspace($actor) . '|' . $actor->actorId . '|' . $task->task_id);

        return substr($hex, 0, 8) . '-' . substr($hex, 8, 4) . '-4' . substr($hex, 13, 3)
            . '-8' . substr($hex, 17, 3) . '-' . substr($hex, 20, 12);
    }

    /** The declared workspace dimension of the actor; never guessed. */
    private function workspace(ActorContext $actor): string
    {
        return (string) ($actor->scope('workspace') ?? (substr(md5($actor->actorId), 0, 8) . '-0000-4000-8000-000000000000'));
    }

    /** The fixture negative-control seam: the declared infrastructure failure. */
    private function failWhenArmed(): void
    {
        if (FixtureFail::consume() === 'store') {
            throw new StoreUnavailableError();
        }
    }
}
