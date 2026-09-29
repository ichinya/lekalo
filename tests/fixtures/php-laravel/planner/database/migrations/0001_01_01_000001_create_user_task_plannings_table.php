<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

// The planner fixture planning schema (issue #50): the
// `user_task_plannings` table mirroring the `planner.user_task_planning`
// entity — the per-workspace/user/task unique identity, the planned
// calendar day, the day position, the focus/pause/completion stamps,
// and the
// optimistic reorder version.

return new class() extends Migration {
    public function up(): void
    {
        Schema::create('user_task_plannings', static function (Blueprint $table): void {
            $table->string('planning_id')->primary();
            $table->string('workspace_id');
            $table->string('user_id');
            $table->string('task_id');
            $table->date('planned_for');
            $table->integer('position');
            $table->dateTime('focused_at')->nullable();
            $table->dateTime('paused_at')->nullable();
            $table->dateTime('completed_at')->nullable();
            $table->integer('reorder_version')->default(1);
            $table->timestamps();
            // The declared per-workspace/user/task unique identity: two
            // users planning one task are two rows, one user planning the
            // same task twice is refused by the database.
            $table->unique(['workspace_id', 'user_id', 'task_id'], 'planning_unique_owner_task');
            $table->index(['user_id', 'planned_for'], 'planning_user_day_index');
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('user_task_plannings');
    }
};
