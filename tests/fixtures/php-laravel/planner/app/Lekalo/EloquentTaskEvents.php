<?php

declare(strict_types=1);

namespace App\Lekalo;

use App\Events\TaskFocused;
use Lekalo\Generated\Operations\Planner\FocusTaskEvents;
use Lekalo\Generated\Types\Planner\TaskFocusedPayload;

/**
 * The maintained event adapter behind the generated FocusTaskEvents
 * port (issue #60 fixture integration): dispatches the fixture's real
 * domain event with the declared payload identity.
 */
final class EloquentTaskEvents implements FocusTaskEvents
{
    public function taskFocused(TaskFocusedPayload $event): void
    {
        event(new TaskFocused($event->taskId->toString()));
    }
}
