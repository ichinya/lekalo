<?php

declare(strict_types=1);

// Laravel framework configuration of the planner fixture: exactly the
// keys the fixture's runtime touches, nothing else (issue #56 S4).

return [
    'debug' => (bool) env('APP_DEBUG', false),
    'url' => env('APP_URL', 'http://localhost'),
    'timezone' => 'UTC',
    'locale' => 'en',
    'fallback_locale' => 'en',
    'faker_locale' => 'en_US',
    'key' => env('APP_KEY'),
    'cipher' => 'AES-256-CBC',
    'providers' => [
        Illuminate\Filesystem\FilesystemServiceProvider::class,
        Illuminate\Database\DatabaseServiceProvider::class,
        // The response factory needs the view factory; the console
        // support provider carries the migration commands (migrate:fresh
        // etc.) the migrations attribute drives through Artisan.
        Illuminate\View\ViewServiceProvider::class,
        Illuminate\Foundation\Providers\ConsoleSupportServiceProvider::class,
        Illuminate\Routing\RoutingServiceProvider::class,
    ],
    'aliases' => [],
];
