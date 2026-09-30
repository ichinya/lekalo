<?php

declare(strict_types=1);

namespace App\Lekalo;

use App\Models\Task;
use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\CountFocusedInput;
use Lekalo\Generated\Operations\Planner\Errors\StoreUnavailableError;
use Lekalo\Generated\Operations\Planner\FocusedCounter;
use Lekalo\Generated\Types\Planner\TaskState;

/**
 * The maintained read adapter behind the generated FocusedCounter port
 * (issue #60 fixture integration): the today board answers the planner
 * focus state over the real tasks table. The declared infrastructure
 * failure is triggerable through the fixture's own negative-control
 * seam so the boundary mapping stays testable.
 */
final class EloquentFocusedCounter implements FocusedCounter
{
    public function count(CountFocusedInput $input, ActorContext $actor): TaskState
    {
        if (FixtureFail::consume() === 'store') {
            throw new StoreUnavailableError();
        }

        return Task::query()->where('focused', true)->exists()
            ? TaskState::Focused
            : TaskState::Backlog;
    }
}
