<?php

declare(strict_types=1);

use App\Http\Controllers\ProviderWebhookController;
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
    // Issue #60: the generated families load through their
    // deterministic classmaps when a generate run has published them —
    // the loading authority, no Composer scan, no runtime magic.
    $classmaps = [];
    foreach (['types', 'operations', 'routes'] as $family) {
        $classmap = dirname(__DIR__) . '/.lekalo/generated/php-laravel/' . $family . '/classmap.php';
        if (is_file($classmap)) {
            $classmaps[$family] = [$family, require $classmap];
        }
    }
    if ($classmaps !== []) {
        $base = dirname(__DIR__);
        spl_autoload_register(static function (string $class) use ($base, $classmaps): void {
            foreach ($classmaps as [$family, $map]) {
                if (isset($map[$class])) {
                    // The types family's classmap spells `__DIR__`-based
                    // absolute paths; the operations/routes families spell
                    // root-relative ones. Both load through the same seam.
                    $path = (string) $map[$class];
                    if ($path !== '' && $path[0] !== '/' && !preg_match('/^[A-Za-z]:/', $path)) {
                        $path = $base . '/.lekalo/generated/php-laravel/' . $family . '/' . $path;
                    }
                    require $path;

                    return;
                }
            }
        });
    }

    // Issue #60: the maintained adapters behind the generated ports —
    // the only maintained integration the generated surface needs. The
    // bindings name strings so the fixture boots with or without the
    // generated tree.
    $app->bind(
        'Lekalo\Generated\Operations\Planner\TaskRepository',
        'App\Lekalo\EloquentTaskRepository',
    );
    $app->bind('Lekalo\Generated\Operations\TransactionPort', 'App\Lekalo\DbTransactionPort');
    $app->bind(
        'Lekalo\Generated\Operations\Planner\FocusTaskPolicy',
        'App\Lekalo\BulkFocusPolicy',
    );
    $app->bind(
        'Lekalo\Generated\Operations\Planner\FocusTaskEvents',
        'App\Lekalo\EloquentTaskEvents',
    );
    $app->bind(
        'Lekalo\Generated\Operations\Planner\FocusedCounter',
        'App\Lekalo\EloquentFocusedCounter',
    );

    // Issue #50: the maintained planning adapters behind the generated
    // planning ports — five command stores, three readers, five policy
    // gates; one maintained class per port, one binding each.
    $app->bind('Lekalo\Generated\Operations\Planner\TaskPlanner', 'App\Lekalo\EloquentPlanningStore');
    $app->bind('Lekalo\Generated\Operations\Planner\TaskMover', 'App\Lekalo\EloquentPlanningStore');
    $app->bind('Lekalo\Generated\Operations\Planner\TaskUnplanner', 'App\Lekalo\EloquentPlanningStore');
    $app->bind('Lekalo\Generated\Operations\Planner\DayReorderer', 'App\Lekalo\EloquentPlanningStore');
    $app->bind('Lekalo\Generated\Operations\Planner\FocusPauser', 'App\Lekalo\EloquentPlanningStore');
    $app->bind('Lekalo\Generated\Operations\Planner\TodayReader', 'App\Lekalo\EloquentPlanningQueries');
    $app->bind('Lekalo\Generated\Operations\Planner\BacklogReader', 'App\Lekalo\EloquentPlanningQueries');
    $app->bind('Lekalo\Generated\Operations\Planner\CarryOverReader', 'App\Lekalo\EloquentPlanningQueries');
    $app->bind('Lekalo\Generated\Operations\Planner\PlanTaskPolicy', 'App\Lekalo\ForeignPlanPolicy');
    $app->bind('Lekalo\Generated\Operations\Planner\MoveTaskPolicy', 'App\Lekalo\ForeignMovePolicy');
    $app->bind('Lekalo\Generated\Operations\Planner\UnplanTaskPolicy', 'App\Lekalo\ForeignUnplanPolicy');
    $app->bind('Lekalo\Generated\Operations\Planner\ReorderPlannedPolicy', 'App\Lekalo\ForeignReorderPolicy');
    $app->bind('Lekalo\Generated\Operations\Planner\PausePlanningPolicy', 'App\Lekalo\ForeignPausePolicy');

    // The fixture authentication seam the generated routes attach (the
    // input's middleware mapping names this alias).
    $app->make('router')->aliasMiddleware('fixture.auth', \App\Http\Middleware\RequireActor::class);

    $app->make('router')->group(['prefix' => 'api'], static function ($router): void {
        $router->post('/tasks/{task_id}/focus', [TaskFocusController::class, 'focus'])
            ->name('tasks.focus');
        // The provider webhook of the ownership-separation scenario
        // (issue #50): provider deliveries mutate provider-owned task
        // columns only — planning fields are never a provider surface.
        $router->post('/provider/webhook', [ProviderWebhookController::class, 'deliver'])
            ->name('provider.webhook');
    });

    // Issue #60: the managed routes of the routes family merge here —
    // ownership-aware: the file exists only after a generate run, and
    // the manual routes above are never touched by it.
    $generatedRoutes = dirname(__DIR__) . '/.lekalo/generated/php-laravel/routes/routes.php';
    if (is_file($generatedRoutes)) {
        require $generatedRoutes;
    }
});

return $app;
