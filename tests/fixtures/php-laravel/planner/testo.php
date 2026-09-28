<?php

declare(strict_types=1);

namespace Tests\Support;

// The Laravel suite configuration of the planner fixture (issue #56
// S4/S6): the Laratesto plugin boots a fresh application around every
// test; the recording plugin captures per-test Testo results so the
// supervisor can reconcile them against the recorder's rows (boot and
// cleanup failures happen outside the test body).
//
// The suite is strictly sequential (Laravel global state): no fiber
// plugin is registered.

use Internal\Container\Container;
use Laratesto\Config\LaravelConfig;
use Laratesto\LaravelPlugin;
use Testo\Application\Config\ApplicationConfig;
use Testo\Application\Config\SuiteConfig;
use Testo\Common\EventListenerCollector;
use Testo\Common\PluginConfigurator;
use Testo\Convention\NamingConventionPlugin;
use Testo\Event\Test\TestFinished;

/**
 * The result-recording plugin: captures every terminal TestResult
 * through the public event API of the pinned Testo version. Raw
 * failures and host paths stay inside the process — only the bounded
 * terminal status names travel into the evidence home.
 */
final class ResultRecorderPlugin implements PluginConfigurator
{
    /** @var array<array{name: string, status: string}> */
    public static array $results = [];

    public function configure(Container $container): void
    {
        $container->get(EventListenerCollector::class)->addListener(
            TestFinished::class,
            static function (TestFinished $event): void {
                self::$results[] = [
                    'name' => $event->testResult->info->identity->fqn(),
                    'status' => $event->testResult->status->name,
                ];
            },
        );
    }
}

return new ApplicationConfig(
    suites: [
        new SuiteConfig(
            name: 'Laravel',
            location: [
                'tests/Feature',
                'src/generated/php-laravel/scenario-tests',
                // The user-owned scaffold home (issue #56 S3): scaffolded
                // scenario tests live beside the managed suite so they
                // stay discoverable while regeneration keeps them frozen.
                'tests/lekalo/scenario-tests',
            ],
            plugins: [
                new NamingConventionPlugin(),
                new LaravelPlugin(new LaravelConfig(basePath: __DIR__)),
                new ResultRecorderPlugin(),
            ],
        ),
    ],
);
