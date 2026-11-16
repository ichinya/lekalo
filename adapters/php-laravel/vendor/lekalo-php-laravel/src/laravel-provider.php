<?php
/**
 * The Laravel framework provider, registered into the language
 * adapter's provider registry (lekalo-php#1, issue #241 stage 4 M1).
 *
 * This descriptor owns every Laravel-specific decision of the adapter
 * process: the storage-transition migration emitter, the Illuminate
 * route emitter, the Laratesto scenario runner registry and test
 * emitter, the Composer/artisan native gate modules, and the Eloquent
 * lint facts. The language kernel resolves each family by capability
 * id through `lekalo_provider_require` and never names Laravel.
 *
 * The loader hooks stay inert inside the unified artifact: build.php
 * appends the owned modules, so their functions already exist when the
 * hooks run and each `function_exists` guard skips the source require
 * — the same 1:1 source/artifact relation every kernel loader keeps.
 * In M2 this descriptor moves to the `lekalo-php-laravel` package and
 * registers from there; the compiled-in registration is transitional.
 */

/**
 * The Laravel package's Composer execution policy document (issue
 * #61): the bundled bytes inside the unified artifact, or the
 * checked-in `composer-gates-policy.json` at this package's root in
 * source mode. Confirmation data only — never execution authority.
 */
function lekalo_laravel_native_policy_document(): array
{
    if (defined('NATIVE_GATES_POLICY_BUNDLED')) {
        $bytes = NATIVE_GATES_POLICY_BUNDLED;
    } else {
        $path = dirname(__DIR__) . '/composer-gates-policy.json';
        $bytes = is_file($path) ? (string) file_get_contents($path) : '';
    }
    if ($bytes === '') {
        throw new RequestRefusal('native-policy-absent');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('native-policy-unparsable');
    }
    if (!is_array($document)) {
        throw new RequestRefusal('native-policy-unparsable');
    }
    return $document;
}

lekalo_provider_register([
    'id' => 'lekalo-php-laravel',
    'version' => '0.1.0',
    'api' => PROVIDER_API_SCHEMA_VERSION,
    'capabilities' => [
        'migrations.emit' => 'full',
        'routes.emit' => 'full',
        'scenario.runners' => 'full',
        'scenario.emit' => 'full',
        'native.gates' => 'full',
        'lint.framework-semantics' => 'partial',
    ],
    'hooks' => [
        // Storage transitions: decode the closed migration input
        // contract and emit the migration class plus its ledger.
        'migrations.emit' => static function (string $bytes, string $target, string $irPath): array {
            if (!function_exists('emit_laravel_migrations')) {
                require_once __DIR__ . '/laravel-migrations.php';
            }
            return emit_laravel_migrations(decode_storage_input($bytes), $target, $irPath);
        },
        // Routes: the framework HTTP vocabulary emitter (Illuminate
        // request/response/route facades) behind the generic map.
        'routes.emit' => static function (): void {
            if (!function_exists('php_emit_routes')) {
                require_once __DIR__ . '/route-emit.php';
            }
        },
        // Scenario tests: the Laratesto emitter behind the generic
        // scenario compiler map.
        'scenario.emit' => static function (): void {
            if (!function_exists('php_emit_scenario_tests')) {
                require_once __DIR__ . '/scenario-emit.php';
            }
        },
        // Native gates: the Composer/Laravel execution-policy modules
        // and the policy document itself (framework custody since the
        // stage-4 separation; the kernel asks by method name).
        'native.gates' => static function (string $method = 'load'): mixed {
            if ($method === 'policy_document') {
                return lekalo_laravel_native_policy_document();
            }
            if ($method !== 'load') {
                throw new RequestRefusal('provider-descriptor');
            }
            if (!function_exists('php_build_native_plan')) {
                require_once __DIR__ . '/native-policy.php';
                require_once __DIR__ . '/native-plan.php';
            }
            return null;
        },
        // The runner registry the scenario mapper resolves against.
        // The Laratesto entry mirrors the bridge's declared facts on
        // the pinned toolchain: the Laravel suite is sequential (fresh
        // application per test), so `testing.concurrency` stays
        // deliberately absent — a race case compiles to an explicit
        // unsupported outcome, never to a serial run that would lie.
        'scenario.runners' => static function (): array {
            return [
                'runners' => [
                    'laratesto' => [
                        'capabilities' => [
                            'testing.clock',
                            'testing.db-refresh',
                            'testing.event-capture',
                            'testing.fixtures',
                            'testing.http',
                            'testing.session',
                        ],
                        'concurrency' => false,
                        'eventCapture' => 'plugin',
                        'syntax' => 'laratesto',
                    ],
                ],
                'default' => 'laratesto',
            ];
        },
        // The framework lint facts of the persisted-model base class.
        'lint.framework-semantics' => static function (): array {
            if (!function_exists('lekalo_laravel_lint_facts')) {
                require_once __DIR__ . '/laravel-lint.php';
            }
            return lekalo_laravel_lint_facts();
        },
    ],
]);
