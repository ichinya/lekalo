<?php

declare(strict_types=1);

use App\Http\Controllers\TaskFocusController;
use Illuminate\Contracts\Console\Kernel as ConsoleKernel;
use Illuminate\Contracts\Http\Kernel as HttpKernel;
use Illuminate\Foundation\Application;
use Tests\Support\PlannerConsoleKernel;
use Tests\Support\PlannerHttpKernel;

// The minimal Laravel application of the planner fixture (issue #56
// S4): HTTP/console kernels bound, the typed exception handler bound,
// API routes registered. Kept as plain construction so the fixture
// shows every binding it depends on.

$app = new Application(dirname(__DIR__));

// The framework bootstrap chain (env, config, facades, providers) —
// the same chain the console kernel would run, executed here so a
// `require` of this file yields a fully-bootstrapped application with
// every binding the fixture's port and HTTP kernel depend on.
$app->bootstrapWith([
    \Illuminate\Foundation\Bootstrap\LoadEnvironmentVariables::class,
    \Illuminate\Foundation\Bootstrap\LoadConfiguration::class,
    \Illuminate\Foundation\Bootstrap\HandleExceptions::class,
    \Illuminate\Foundation\Bootstrap\RegisterFacades::class,
    \Illuminate\Foundation\Bootstrap\SetRequestForConsole::class,
    \Illuminate\Foundation\Bootstrap\RegisterProviders::class,
    \Illuminate\Foundation\Bootstrap\BootProviders::class,
]);

$app->singleton(HttpKernel::class, PlannerHttpKernel::class);
$app->singleton(ConsoleKernel::class, PlannerConsoleKernel::class);

// The exception handler binding the standard ApplicationBuilder installs
// via withExceptions(): the fixture registers it explicitly because it
// constructs the application by hand.
$app->singleton(
    \Illuminate\Contracts\Debug\ExceptionHandler::class,
    \Illuminate\Foundation\Exceptions\Handler::class,
);

// The fixture's transport routes load here — at boot time, through the
// same router the HTTP kernel dispatches through.
$app->booted(static function (Application $app): void {
    $app->make('router')->group(['prefix' => 'api'], static function ($router): void {
        $router->post('/tasks/{task_id}/focus', [TaskFocusController::class, 'focus'])
            ->name('tasks.focus');
    });
});

return $app;
