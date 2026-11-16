<?php
/**
 * The framework suite of `lekalo-php-laravel` (lekalo-php#1, issue
 * #241 stage 4): the Laravel provider registers into the `lekalo-php`
 * language kernel through the versioned provider API, the capability
 * matrix negotiates, the migration emitter unit leg runs verbatim
 * (moved from the monorepo protocol suite), the Laratesto runner
 * table and the Eloquent lint facts arrive from this package, and the
 * Composer execution policy document loads from the package root.
 *
 * The language kernel is resolved beside this package (the lekalo-adds
 * checkout layout and the monorepo vendor layout agree on
 * `../lekalo-php/src`) or through LEKALO_PHP_SRC.
 */

declare(strict_types=1);

$lekaloPhpSrc = getenv('LEKALO_PHP_SRC') ?: dirname(__DIR__, 2) . '/lekalo-php/src';
foreach (['provider.php', 'analyzer.php', 'strict-profile.php', 'kernel.php'] as $module) {
    if (!is_file($lekaloPhpSrc . '/' . $module)) {
        fwrite(STDERR, "lekalo-php source not found at {$lekaloPhpSrc} (set LEKALO_PHP_SRC)\n");
        exit(1);
    }
    require $lekaloPhpSrc . '/' . $module;
}
require __DIR__ . '/../src/laravel-provider.php';
// The migration emitter unit leg calls the package module directly
// (the family itself still resolves through the hook).
require __DIR__ . '/../src/laravel-migrations.php';

$GLOBALS['__lekalo_failures'] = [];
$GLOBALS['__lekalo_checks'] = 0;

function check(bool $condition, string $name): void
{
    $GLOBALS['__lekalo_checks']++;
    if (!$condition) {
        $GLOBALS['__lekalo_failures'][] = $name;
        fwrite(STDERR, "FAIL {$name}\n");
    }
}

// The emitter legs read the published ledger relative to the process
// cwd: run from a fresh temp root so the suite is hermetic.
$laravelSuiteRoot = sys_get_temp_dir() . '/lekalo-laravel-suite-' . getmypid();
if (!is_dir($laravelSuiteRoot)) {
    mkdir($laravelSuiteRoot, 0777, true);
}
chdir($laravelSuiteRoot);

// --- provider registration (lekalo-php#1) ----------------------------------

$capability = lekalo_provider_require('migrations.emit');
check($capability['provider'] === 'lekalo-php-laravel', 'migration family resolves through this package');
check($capability['state'] === 'full', 'migration family negotiates full support');
$providers = lekalo_providers();
check(count($providers) === 1 && $providers[0]['id'] === 'lekalo-php-laravel', 'exactly this package is registered');
check($providers[0]['api'] === PROVIDER_API_SCHEMA_VERSION, 'the package speaks the adapter provider API version');

$runners = lekalo_provider_scenario_runners();
check(isset($runners['laratesto']) && $runners['laratesto']['concurrency'] === false, 'the laratesto runner arrives from this package');
check(lekalo_provider_default_scenario_runner() === 'laratesto', 'the default runner arrives from this package');

check(lekalo_provider_lint_classifies_model('Illuminate\\Database\\Eloquent\\Model'), 'the eloquent model base classifies through this package');
check(!lekalo_provider_lint_classifies_model('App\\Model'), 'an unrelated base class does not classify');

$policy = lekalo_laravel_native_policy_document();
check(($policy['schema_version'] ?? null) === 'lekalo/native-gate-policy/v0.4.0', 'the composer execution policy loads from the package root');
check($policy['allowed_gate_kinds'] === ['build', 'typecheck', 'lint', 'test'], 'the policy declares the closed gate-kind vocabulary');

// --- Laravel migration emitter (issue #57, moved from the monorepo protocol suite) ---

$validInput = [
    'schemaVersion' => 'lekalo/laravel-migration-input/v0.4.0',
    'identity' => 'dev.lekalo.laravel-migration-input@0.4.0',
    'engine' => 'postgres',
    'engineVersion' => '16.4.0',
    'projectId' => 'planner',
    'baseDigest' => 'sha256:' . str_repeat('a', 64),
    'candidateDigest' => 'sha256:' . str_repeat('b', 64),
    'diffDigest' => 'sha256:' . str_repeat('c', 64),
    'planId' => 'sha256:' . str_repeat('d', 64),
    'gated' => false,
    'backfillGated' => false,
    'effectiveStatus' => 'ready',
    'operations' => [
        [
            'ordinal' => 1,
            'kind' => 'create_extension',
            'statement' => 'CREATE EXTENSION IF NOT EXISTS "pgcrypto";',
            'risk' => 'none',
            'rollback' => 'data-loss-on-rollback',
        ],
        [
            'ordinal' => 2,
            'kind' => 'add_column',
            'statement' => 'ALTER TABLE "task" ADD COLUMN "last_seen_at" timestamptz;',
            'risk' => 'none',
            'rollback' => 'data-loss-on-rollback',
        ],
        [
            'ordinal' => 3,
            'kind' => "add_index",
            'statement' => 'CREATE INDEX "idx_task_due" ON "task" ("due_date");',
            'risk' => 'none',
            'rollback' => 'reversible',
            'inverse' => 'DROP INDEX "idx_task_due";',
        ],
    ],
];
$emitted = emit_laravel_migrations($validInput, 'php-laravel', '.lekalo/ir/test.migration-input.json');
check(str_starts_with($emitted['bytes'], "<?php\n"), 'migration is PHP');
check(str_contains($emitted['bytes'], 'declare(strict_types=1);'), 'migration declares strict types');
check(str_contains($emitted['bytes'], 'extends Migration'), 'migration extends the Laravel Migration');
check(str_contains($emitted['bytes'], 'DB::statement('), 'migration executes planned SQL');
check(str_contains($emitted['bytes'], 'CREATE INDEX "idx_task_due"'), 'SQL bytes survive the PHP literal');
check(str_contains($emitted['bytes'], 'public function up(): void'), 'up() signature');
check(str_contains($emitted['bytes'], 'public function down(): void'), 'down() signature');
check(str_contains($emitted['bytes'], 'RuntimeException'), 'unsafe rollback refuses');
check($emitted['digest'] === sha256_digest($emitted['bytes']), 'artifact digest covers exact bytes');
check(scope_covers('.lekalo/generated/php-laravel/**', $emitted['path']), 'artifact inside write scope');
check(str_contains($emitted['path'], '/migrations/'), 'artifact lands in the migrations directory');
check(str_starts_with($emitted['filename'], MIGRATION_TIMESTAMP_BASE . '_'), 'timestamp comes from the declared policy base');
$again = emit_laravel_migrations($validInput, 'php-laravel', '.lekalo/ir/test.migration-input.json');
check($again['bytes'] === $emitted['bytes'] && $again['path'] === $emitted['path'], 'two runs are byte-identical (deterministic filenames and content)');
$changedInput = $validInput;
$changedInput['operations'][0]['statement'] = 'CREATE EXTENSION IF NOT EXISTS "pgcrypto2";';
$changed = emit_laravel_migrations($changedInput, 'php-laravel', '.lekalo/ir/test.migration-input.json');
check($changed['path'] !== $emitted['path'], 'a changed plan produces a new migration file, never overwrites');

// Reversible plan renders a real down().
$reversibleInput = $validInput;
$reversibleInput['operations'] = [$validInput['operations'][2]];
$reversible = emit_laravel_migrations($reversibleInput, 'php-laravel', '.lekalo/ir/test.migration-input.json');
check(!str_contains($reversible['bytes'], 'RuntimeException'), 'fully reversible plan renders a real down()');
check(str_contains($reversible['bytes'], 'DROP INDEX "idx_task_due"'), 'down() carries the typed inverse');

// The gated refusal: a blocked input generates zero bytes.
$blocked = $validInput;
$blocked['effectiveStatus'] = 'blocked';
$refused = false;
try {
    emit_laravel_migrations($blocked, 'php-laravel', '.lekalo/ir/test.migration-input.json');
} catch (RequestRefusal $refusal) {
    $refused = $refusal->getMessage() === 'input-gated';
}
check($refused, 'a blocked input refuses before any artifact');

// Closed-shape refusals.
$unknownMember = $validInput;
$unknownMember['surprise'] = 1;
$refusedUnknown = false;
try {
    decode_storage_input(canonical_json($unknownMember));
} catch (RequestRefusal $refusal) {
    $refusedUnknown = $refusal->getMessage() === 'input-member';
}
check($refusedUnknown, 'unknown input member refused');
$wrongKind = $validInput;
$wrongKind['operations'][0]['kind'] = 'nuke_everything';
$refusedKind = false;
try {
    decode_storage_input(canonical_json($wrongKind));
} catch (RequestRefusal $refusal) {
    $refusedKind = $refusal->getMessage() === 'input-kind';
}
check($refusedKind, 'unknown operation kind refused');
$badDigest = $validInput;
$badDigest['planId'] = 'not-a-digest';
$refusedDigest = false;
try {
    decode_storage_input(canonical_json($badDigest));
} catch (RequestRefusal $refusal) {
    $refusedDigest = $refusal->getMessage() === 'input-digest';
}
check($refusedDigest, 'malformed plan digest refused');
$reordered = $validInput;
$reordered['operations'] = [$validInput['operations'][2], $validInput['operations'][0]];
$refusedOrder = false;
try {
    decode_storage_input(canonical_json($reordered));
} catch (RequestRefusal $refusal) {
    $refusedOrder = $refusal->getMessage() === 'input-order';
}
check($refusedOrder, 'non-monotonic ordinals refused');
$noInverse = $validInput;
$noInverse['operations'] = [['ordinal' => 1, 'kind' => 'add_index', 'statement' => 'CREATE INDEX "i" ON "t" ("c");', 'risk' => 'none', 'rollback' => 'reversible']];
$refusedInverse = false;
try {
    decode_storage_input(canonical_json($noInverse));
} catch (RequestRefusal $refusal) {
    $refusedInverse = $refusal->getMessage() === 'input-inverse';
}
check($refusedInverse, 'reversible operation without its inverse refused');

// SQL literal escaping keeps bytes.
$tricky = "UPDATE \"t\" SET \"a\" = 'it\'s fine' WHERE \"b\" = E'\\d\'";
check(php_single_quote($tricky) === str_replace(["\\", "'"], ['\\\\', "\\'"], $tricky), 'single-quote escaping escapes only backslash and quote');

// The append-only ledger custody.
$ledger = ['migrations' => [
    ['ordinal' => 1, 'filename' => 'a.php', 'digest' => 'sha256:' . str_repeat('1', 64)],
]];
$append = ['migrations' => [
    ['ordinal' => 1, 'filename' => 'a.php', 'digest' => 'sha256:' . str_repeat('1', 64)],
    ['ordinal' => 2, 'filename' => 'b.php', 'digest' => 'sha256:' . str_repeat('2', 64)],
]];
$shrunk = ['migrations' => []];
$rewritten = ['migrations' => [
    ['ordinal' => 1, 'filename' => 'a.php', 'digest' => 'sha256:' . str_repeat('9', 64)],
]];
$rewrittenOk = true;
try {
    assert_append_only($ledger, $append);
    assert_append_only($ledger, $ledger);
} catch (RequestRefusal) {
    $rewrittenOk = false;
}
check($rewrittenOk, 'append and byte-identical regeneration accepted');
$refusedShrunk = $refusedRewrite = false;
try {
    assert_append_only($ledger, $shrunk);
} catch (RequestRefusal $refusal) {
    $refusedShrunk = $refusal->getMessage() === 'ledger-shrunk';
}
try {
    assert_append_only($ledger, $rewritten);
} catch (RequestRefusal $refusal) {
    $refusedRewrite = $refusal->getMessage() === 'ledger-rewritten';
}
check($refusedShrunk, 'a shrinking ledger refused');
check($refusedRewrite, 'a rewritten published entry refused');

fwrite(STDOUT, json_encode([
    'suite' => 'php-laravel-framework',
    'checks' => $GLOBALS['__lekalo_checks'],
    'failures' => count($GLOBALS['__lekalo_failures']),
]) . "\n");
exit($GLOBALS['__lekalo_failures'] === [] ? 0 : 1);
