<?php

/**
 * The routes runtime driver of the routes corpus (issue #60, extended
 * by the planning battery of issue #50): boots the materialized
 * planner fixture through its real bootstrap, creates the schema, and
 * drives real HTTP requests through the kernel — the generated routes,
 * thin controllers, and operations handlers against the maintained
 * adapters. Prints one JSON line per request outcome.
 *
 * Usage: php driver.php <request-spec-json>
 * The spec is a list of objects:
 *   {method, uri, body?, headers?}                     — one HTTP request
 *   {probe: "plannings"|"tasks"|"idempotency"}         — one DB snapshot
 *   {probe: "second-focus", task_id, user_id}          — the one-active-focus
 *       constraint proof through a SECOND database connection.
 * String values may carry {{planning:USER|TASK_ID|DAY}} placeholders,
 * resolved against the live planning rows.
 */

declare(strict_types=1);

error_reporting(E_ALL);
ini_set('display_errors', 'stderr');

$_SERVER['APP_ENV'] = 'testing';
$_ENV['APP_ENV'] = 'testing';
$_ENV['DB_CONNECTION'] = 'sqlite';
// The file database: the one-active-focus proof opens a second
// connection against the same bytes.
$database = sys_get_temp_dir() . '/lekalo-planner-runtime-' . getmypid() . '.sqlite3';
@unlink($database);
file_put_contents($database, '');
$_ENV['DB_DATABASE'] = $database;

require getcwd() . '/vendor/autoload.php';
$app = require getcwd() . '/bootstrap/app.php';

// The fixture schema: the same tables the committed migrations create,
// created in the driver so the runtime lane stays self-contained.
Illuminate\Support\Facades\Schema::create('tasks', static function ($table): void {
    $table->string('task_id')->primary();
    $table->string('user_id');
    $table->boolean('focused')->default(false);
    $table->dateTime('focused_at')->nullable();
    $table->string('provider_synced_at')->nullable();
    $table->timestamps();
});
// The declared one-active-focus invariant (issue #50): at most one
// focused task per user at any instant, enforced by the database.
Illuminate\Support\Facades\DB::statement(
    'CREATE UNIQUE INDEX tasks_one_active_focus_per_user ON tasks (user_id) WHERE focused = 1',
);
Illuminate\Support\Facades\Schema::create('user_task_plannings', static function ($table): void {
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
    $table->unique(['workspace_id', 'user_id', 'task_id'], 'planning_unique_owner_task');
    $table->index(['user_id', 'planned_for'], 'planning_user_day_index');
});
Illuminate\Support\Facades\Schema::create('planning_idempotency', static function ($table): void {
    $table->string('request_digest')->primary();
    $table->string('canonical_request');
});

// The deterministic seed rows of the runtime lane.
Illuminate\Support\Facades\DB::table('tasks')->insert([
    ['task_id' => '3f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b45', 'user_id' => 'user-1'],
    ['task_id' => '8f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b88', 'user_id' => 'user-2'],
    ['task_id' => '4f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b33', 'user_id' => 'user-3'],
    ['task_id' => '5f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b44', 'user_id' => 'user-3'],
    ['task_id' => '6f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b55', 'user_id' => '11111111-1111-4111-8111-111111111111'],
]);
// The carry-over seed of user-3: planned for 2026-01-01, one day
// before the pinned runtime instant, never rewritten by any read.
Illuminate\Support\Facades\DB::table('user_task_plannings')->insert([
    [
        'planning_id' => 'a1b2c3d4-0000-4000-8000-000000000001',
        'workspace_id' => '0a0a0a0a-0a0a-4a0a-8a0a-0a0a0a0a0a01',
        'user_id' => '33333333-3333-4333-8333-333333333333',
        'task_id' => '4f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b33',
        'planned_for' => '2026-01-01',
        'position' => 1,
        'reorder_version' => 1,
    ],
]);

/** Resolve {{planning:USER|TASK|DAY}} placeholders against live rows. */
$expand = static function (mixed $value) use ($app): mixed {
    if (!is_string($value) || !str_contains($value, '{{planning:')) {
        return $value;
    }

    return preg_replace_callback(
        '/\{\{planning:([^}|]+)\|([^}|]+)\|([^}]+)\}\}/',
        static function (array $m) use ($app): string {
            $row = $app['db']->table('user_task_plannings')
                ->where('user_id', $m[1])
                ->where('task_id', $m[2])
                ->where('planned_for', $m[3])
                ->first();

            return $row === null ? '{{missing:' . $m[1] . '}}' : (string) $row->planning_id;
        },
        $value,
    );
};

$kernel = $app->make(Illuminate\Contracts\Http\Kernel::class);
$specs = json_decode($argv[1] ?? '[]', true, 512, JSON_THROW_ON_ERROR);
$outcomes = [];
foreach ($specs as $spec) {
    if (isset($spec['probe'])) {
        $outcomes[] = match ((string) $spec['probe']) {
            'plannings' => [
                'probe' => 'plannings',
                'rows' => $app['db']->table('user_task_plannings')
                    ->orderBy('user_id')->orderBy('planned_for')->orderBy('position')
                    ->get()->map(static fn ($row): array => (array) $row)->all(),
            ],
            'tasks' => [
                'probe' => 'tasks',
                'rows' => $app['db']->table('tasks')
                    ->orderBy('task_id')
                    ->get()->map(static fn ($row): array => (array) $row)->all(),
            ],
            'idempotency' => [
                'probe' => 'idempotency',
                'rows' => $app['db']->table('planning_idempotency')
                    ->orderBy('request_digest')
                    ->get()->map(static fn ($row): array => (array) $row)->all(),
            ],
            'second-focus' => [
                'probe' => 'second-focus',
                // The real transaction guarantee: a second connection
                // attempts the second active focus for the user; the
                // partial unique index refuses it, whatever the
                // application layer believes.
                'violated' => (static function () use ($database, $spec): bool {
                    try {
                        $second = new PDO('sqlite:' . $database);
                        $second->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
                        $statement = $second->prepare(
                            'UPDATE tasks SET focused = 1, focused_at = ? WHERE task_id = ?',
                        );
                        $statement->execute([gmdate('Y-m-d\TH:i:s\Z'), (string) $spec['task_id']]);

                        return false;
                    } catch (Throwable) {
                        return true;
                    }
                })(),
            ],
            default => ['probe' => 'unknown', 'spec' => $spec],
        };

        continue;
    }
    $uri = $expand((string) $spec['uri']);
    $body = isset($spec['body']) ? array_map($expand, $spec['body']) : null;
    $request = Illuminate\Http\Request::create(
        $uri,
        (string) $spec['method'],
        [],
        [],
        [],
        ['HTTP_ACCEPT' => 'application/json'],
        $body !== null ? json_encode($body, JSON_THROW_ON_ERROR) : null,
    );
    foreach (($spec['headers'] ?? []) as $name => $value) {
        $request->headers->set((string) $name, (string) $expand($value));
    }
    // The deterministic failure-injection seam: arm before handling.
    $fail = (string) ($spec['headers']['X-Fixture-Fail'] ?? '');
    if ($fail !== '') {
        \App\Lekalo\FixtureFail::arm($fail);
    }
    try {
        $response = $kernel->handle($request);
        $outcomes[] = [
            'spec' => $spec,
            'status' => $response->getStatusCode(),
            'body' => json_decode($response->getContent() ?: 'null', true, 512, JSON_THROW_ON_ERROR),
        ];
    } catch (Throwable $failure) {
        $outcomes[] = [
            'spec' => $spec,
            'status' => 'throwable',
            'class' => $failure::class,
            'message' => $failure->getMessage(),
        ];
    }
}

@unlink($database);
echo json_encode($outcomes, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES), "\n";
