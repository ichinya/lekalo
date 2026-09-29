<?php

/**
 * The routes runtime driver of the routes corpus (issue #60): boots the
 * materialized planner fixture through its real bootstrap, creates the
 * schema, and drives real HTTP requests through the kernel — the
 * generated routes, thin controllers, and operations handlers against
 * the maintained adapters. Prints one JSON line per request outcome.
 *
 * Usage: php driver.php <request-spec-json>
 * The spec is a list of {method, uri, body?, headers?} objects.
 */

declare(strict_types=1);

error_reporting(E_ALL);
ini_set('display_errors', 'stderr');

$_SERVER['APP_ENV'] = 'testing';
$_ENV['APP_ENV'] = 'testing';
$_ENV['DB_CONNECTION'] = 'sqlite';
$_ENV['DB_DATABASE'] = ':memory:';

require getcwd() . '/vendor/autoload.php';
$app = require getcwd() . '/bootstrap/app.php';

// The fixture schema: the same tasks table the committed migration
// creates, created in the driver so the runtime lane stays
// self-contained.
Illuminate\Support\Facades\Schema::create('tasks', static function ($table): void {
    $table->string('task_id')->primary();
    $table->string('user_id');
    $table->boolean('focused')->default(false);
    $table->dateTime('focused_at')->nullable();
    $table->timestamps();
});

// The deterministic seed rows of the runtime lane: one task per owner
// the request specs address.
Illuminate\Support\Facades\DB::table('tasks')->insert([
    ['task_id' => '3f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b45', 'user_id' => 'user-1'],
    ['task_id' => '8f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b88', 'user_id' => 'user-2'],
]);

$kernel = $app->make(Illuminate\Contracts\Http\Kernel::class);
$specs = json_decode($argv[1] ?? '[]', true, 512, JSON_THROW_ON_ERROR);
$outcomes = [];
foreach ($specs as $spec) {
    $request = Illuminate\Http\Request::create(
        (string) $spec['uri'],
        (string) $spec['method'],
        [],
        [],
        [],
        ['HTTP_ACCEPT' => 'application/json'],
        isset($spec['body']) ? json_encode($spec['body'], JSON_THROW_ON_ERROR) : null,
    );
    foreach (($spec['headers'] ?? []) as $name => $value) {
        $request->headers->set((string) $name, (string) $value);
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

echo json_encode($outcomes, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES), "\n";
