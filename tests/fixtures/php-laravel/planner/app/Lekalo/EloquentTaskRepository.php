<?php

declare(strict_types=1);

namespace App\Lekalo;

use App\Models\Task;
use Lekalo\Generated\Operations\Planner\TaskRepository;
use Lekalo\Generated\Types\Planner\TaskDto;
use Lekalo\Generated\Types\Planner\TaskId;
use Lekalo\Generated\Types\Planner\TaskState;
use Lekalo\Generated\Types\Planner\Text;

/**
 * The maintained Eloquent adapter behind the generated narrow
 * TaskRepository port (issue #60 fixture integration). The fixture
 * schema tracks the observable focused state; the title column the
 * generated DTO carries is a fixture constant, documented instead of
 * modeled.
 */
final class EloquentTaskRepository implements TaskRepository
{
    /** The fixture's constant title: the schema does not model titles. */
    private const FIXTURE_TITLE = 'Fixture task';

    public function find(TaskId $key): ?TaskDto
    {
        $row = Task::query()->where('task_id', $key->toString())->first();
        if ($row === null) {
            return null;
        }

        return new TaskDto(
            $key,
            Text::from(self::FIXTURE_TITLE),
            ((bool) $row->focused) ? TaskState::Focused : TaskState::Backlog,
        );
    }

    public function save(TaskDto $entity): void
    {
        $focused = $entity->state === TaskState::Focused;
        // The canonical `...Z` spelling: the port owns timestamps end to
        // end, no Carbon re-serialization.
        $focusedAt = $focused ? gmdate('Y-m-d\TH:i:s\Z') : null;
        Task::query()
            ->where('task_id', $entity->taskId->toString())
            ->update(['focused' => $focused, 'focused_at' => $focusedAt]);
    }
}
