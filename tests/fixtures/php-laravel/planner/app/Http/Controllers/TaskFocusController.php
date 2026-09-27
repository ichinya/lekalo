<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Models\Task;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;

// The focus endpoint of the fixture: binds `planner.focus_task` to the
// real HTTP kernel. The typed domain error (unknown task) and the
// persisted effect (focused + focused_at + the domain event) mirror
// the semantics the generated scenario tests assert through the port.

final class TaskFocusController
{
    public function focus(Request $request, string $taskId): JsonResponse
    {
        $task = Task::query()->where('task_id', $taskId)->first();
        if ($task === null) {
            return response()->json([
                'error' => [
                    'id' => 'planner.error.task_missing',
                    'fields' => ['task_id' => $taskId],
                ],
            ], 404);
        }

        $task->focused = true;
        // The port owns the canonical `...Z` spelling; the endpoint
        // stores and returns it verbatim, no Carbon re-serialization.
        $task->focused_at = $request->input('focused_at', '2026-01-01T00:00:00Z');
        $task->save();

        event(new \App\Events\TaskFocused($taskId));

        return response()->json([
            'ok' => true,
            'value' => [
                'task_id' => $task->task_id,
                'user_id' => $task->user_id,
                'focused' => true,
                'focused_at' => (string) $task->focused_at,
            ],
        ]);
    }
}
