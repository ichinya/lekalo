<?php

declare(strict_types=1);

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

// The planner fixture schema: one tasks table mirroring the
// `planner.task` entity (identity `task_id`, the focused flag, the
// deterministic focused_at timestamp, and the owning user).

return new class() extends Migration {
    public function up(): void
    {
        Schema::create('tasks', static function (Blueprint $table): void {
            $table->string('task_id')->primary();
            $table->string('user_id');
            $table->boolean('focused')->default(false);
            $table->dateTime('focused_at')->nullable();
            $table->string('provider_synced_at')->nullable();
            $table->timestamps();
        });
        // The declared one-active-focus invariant (issue #50): at most one
        // focused task per user at any instant, enforced by the database
        // inside the planning transaction — never by a check-then-act.
        Illuminate\Support\Facades\DB::statement(
            'CREATE UNIQUE INDEX tasks_one_active_focus_per_user ON tasks (user_id) WHERE focused = 1',
        );
    }

    public function down(): void
    {
        Schema::dropIfExists('tasks');
    }
};
