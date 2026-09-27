<?php

declare(strict_types=1);

use App\Http\Controllers\TaskFocusController;
use Illuminate\Support\Facades\Route;

// The fixture transport surface the target test port binds operations
// to (issue #56 S4): the shape mirrors the planner model's endpoint
// (`planner.api_focus`, POST /tasks/{task_id}/focus).

Route::post('/tasks/{task_id}/focus', [TaskFocusController::class, 'focus'])
    ->name('tasks.focus');
