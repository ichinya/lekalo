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
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('tasks');
    }
};
