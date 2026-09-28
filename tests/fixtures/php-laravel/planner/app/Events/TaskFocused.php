<?php

declare(strict_types=1);

namespace App\Events;

// The focused event of the fixture: mirrors `planner.task_focused`.
// Captured through the port's event dispatcher instead of any real
// broadcast (the fixture has no production transport).

final class TaskFocused
{
    public function __construct(
        public readonly string $taskId,
    ) {}
}
