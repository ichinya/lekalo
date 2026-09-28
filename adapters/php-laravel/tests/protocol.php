<?php
/**
 * The protocol unit suite of the PHP kernel (issue #54): the strict
 * request decoder, the closed validation rules, canonical output, and
 * the deterministic generation seam. Dependency-free; run with
 * `php adapters/php-laravel/tests/protocol.php` or through
 * `scripts/test-php-laravel-adapter.mjs`.
 */

declare(strict_types=1);

require __DIR__ . '/../src/analyzer.php';
require __DIR__ . '/../src/strict-profile.php';
require __DIR__ . '/../src/kernel.php';

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

/** Build a legal describe request with one overridden member. */
function describe_request(array $overrides = []): array
{
    return array_merge([
        'protocol' => PROTOCOL_TOKEN,
        'protocol_version' => VERSION,
        'operation' => 'describe',
        'request_id' => 'req-' . str_repeat('a', 64),
        'project_root' => '.',
    ], $overrides);
}

function accepts(array $request): bool
{
    try {
        validate_request_object($request);
        return true;
    } catch (RequestRefusal) {
        return false;
    }
}

function refuses_with(array $request, string $code): bool
{
    try {
        validate_request_object($request);
        return false;
    } catch (RequestRefusal $refusal) {
        return $refusal->getMessage() === $code;
    }
}

// --- strict JSON decoding -------------------------------------------------

check(decode_json_document('{"a":1}') === ['a' => 1], 'plain decode');
try {
    decode_json_document("{\"a\":1,\"a\":2}");
    check(false, 'duplicate keys refused');
} catch (RequestRefusal $r) {
    check($r->getMessage() === 'duplicate-key', 'duplicate keys refused');
}
try {
    decode_json_document("{\"a\":1,\"\u0061\":2}");
    check(false, 'escaped duplicate keys refused');
} catch (RequestRefusal) {
    check(true, 'escaped duplicate keys refused');
}
try {
    decode_json_document('{} {}');
    check(false, 'trailing content refused');
} catch (RequestRefusal $r) {
    check($r->getMessage() === 'trailing-content', 'trailing content refused');
}
try {
    decode_json_document(str_repeat('[', 70) . '1' . str_repeat(']', 70));
    check(false, 'depth refused');
} catch (RequestRefusal $r) {
    check(in_array($r->getMessage(), ['depth', 'syntax'], true), 'depth refused');
}
try {
    decode_json_document("{\"a\":\"\xB1\x31\"}");
    check(false, 'malformed utf-8 refused');
} catch (RequestRefusal $r) {
    check($r->getMessage() === 'utf-8', 'malformed utf-8 refused');
}
try {
    decode_json_document('{"a":01}');
    check(false, 'leading-zero number refused');
} catch (RequestRefusal) {
    check(true, 'leading-zero number refused');
}
try {
    decode_json_document("{\"a\":\"x\x07y\"}");
    check(false, 'unescaped control refused');
} catch (RequestRefusal) {
    check(true, 'unescaped control refused');
}

// --- canonical output ------------------------------------------------------

check(canonical_json(['b' => 1, 'a' => ['z' => 1, 'A' => 2]]) === '{"a":{"A":2,"z":1},"b":1}', 'keys sorted bytewise');
check(canonical_json(['/x' => 1]) === '{"/x":1}', 'slashes unescaped like serde_json');
check(bin2hex(canonical_json("é")) === bin2hex('"é"'), 'non-ascii carried raw like serde_json');
check(canonical_json([]) === '[]' && canonical_json(new stdClass() instanceof stdClass ? [] : []) === '[]', 'empty list');
check(sha256_hex('abc') === 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad', 'sha256 anchor');

// --- grammar predicates ----------------------------------------------------

check(is_logical_path('.lekalo/generated/php-laravel/x/kernel.php'), 'logical path');
check(!is_logical_path('/abs/x'), 'absolute refused');
check(!is_logical_path('a/../b'), 'traversal refused');
check(!is_logical_path('a//b'), 'empty segment refused');
check(is_scope('src/**') && !is_scope('src/**/x') && !is_scope('**'), 'scope grammar');
check(scope_covers('src/**', 'src/a/b.php') && !scope_covers('src/**', 'srcx/a'), 'scope coverage');
check(protected_home('.lekalo/ir/planner.json') === 'ir', 'protected home ir');
check(protected_home('lekalo.lock') === 'lekalo-lockfile', 'protected lockfile');
check(protected_home('src/x.php') === null, 'unprotected path');
check(is_token('php-laravel') && !is_token('PHP') && !is_token('a_b'), 'token grammar');
check(is_capability_id('generate.zod') && !is_capability_id('Generate.Zod'), 'capability id grammar');
check(is_sha256_digest('sha256:' . str_repeat('0', 64)) && !is_sha256_digest('sha256:xx'), 'digest spelling');
check(is_plan_id('plan-' . str_repeat('f', 64)) && !is_plan_id('plan-xx'), 'plan id spelling');

// --- closed request contract ----------------------------------------------

check(accepts(describe_request()), 'legal describe');
check(refuses_with(describe_request(['protocol' => 'lekalo.target/v2']), 'protocol-token'), 'foreign token refused');
check(refuses_with(describe_request(['protocol_version' => '0.9.0']), 'protocol-version'), 'unsupported version refused');
check(refuses_with(describe_request(['request_id' => 'req-short']), 'request-id'), 'malformed request id refused');
check(refuses_with(describe_request(['project_root' => '/tmp']), 'project-root'), 'foreign project root refused');
check(refuses_with(describe_request(['unknown' => 1]), 'unknown-key'), 'unknown member refused');
check(refuses_with(describe_request(['ir_path' => null]), 'null-member'), 'explicit null refused');
check(refuses_with(describe_request(['ir_path' => 'x.json']), 'ir-path'), 'describe refuses ir_path pairing');
check(refuses_with(describe_request(['target' => 'php-laravel']), 'member'), 'describe carries no target');
check(refuses_with(describe_request(['operation' => 'nope']), 'operation'), 'unknown operation refused');

// generate pairing
$generate = describe_request(['operation' => 'generate', 'target' => 'php-laravel', 'ir_path' => '.lekalo/ir/minimal.json', 'dry_run' => true]);
check(accepts($generate), 'legal dry-run generate');
check(refuses_with(array_diff_key($generate, ['dry_run' => true]), 'dry-run'), 'generate without dry_run refused');
$apply = describe_request(['operation' => 'generate', 'target' => 'php-laravel', 'ir_path' => '.lekalo/ir/minimal.json', 'dry_run' => false, 'plan_id' => 'plan-' . str_repeat('b', 64)]);
check(accepts($apply), 'legal apply generate');
check(refuses_with(array_merge($apply, ['plan_id' => 'plan-nope']), 'plan-id'), 'malformed plan id refused');
$cleanApply = describe_request(['operation' => 'clean', 'plan_id' => 'plan-' . str_repeat('c', 64)]);
check(accepts($cleanApply), 'legal clean apply');
check(refuses_with(describe_request(['operation' => 'clean']), 'plan-id'), 'clean without plan refused');

// plan-native pairing (issue #48)
$nativeBase = [
    'changes' => ['files' => [['path' => 'src/x.php', 'change' => 'modified']], 'symbols' => []],
    'scan_ref' => ['digest' => 'sha256:' . str_repeat('1', 64)],
    'execution_policy_ref' => ['digest' => 'sha256:' . str_repeat('2', 64)],
    'input_manifest_digest' => 'sha256:' . str_repeat('3', 64),
    'tool_catalog_digest' => 'sha256:' . str_repeat('4', 64),
    'capability_snapshot_digest' => 'sha256:' . str_repeat('5', 64),
];
$native = describe_request(['operation' => 'plan-native', 'protocol_version' => VERSION, 'native_request' => $nativeBase]);
check(accepts($native), 'legal plan-native');
check(refuses_with(array_merge($native, ['dry_run' => true]), 'plan-id'), 'plan-native with dry_run refused');
$brokenNative = $native;
$brokenNative['native_request']['input_manifest_digest'] = 'sha256:zz';
check(refuses_with($brokenNative, 'native-request'), 'malformed native digest refused');
$nativeExtra = $native;
$nativeExtra['native_request']['surprise'] = 1;
check(refuses_with($nativeExtra, 'native-request'), 'unknown native member refused');

// profile resolution pairing (issue #29); describe refuses every
// operation member, so the legal resolution rides `validate`.
$resolution = describe_request([
    'operation' => 'validate',
    'ir_path' => '.lekalo/ir/minimal.json',
    'profile' => 'default',
    'profile_digest' => 'sha256:' . str_repeat('d', 64),
    'profile_capabilities' => [['id' => 'runtime.async', 'support' => 'partial']],
]);
check(accepts($resolution), 'legal profile resolution');
check(refuses_with(array_diff_key($resolution, ['profile_capabilities' => []]), 'profile-capabilities'), 'unpaired resolution refused');
$unsorted = $resolution;
$unsorted['profile_capabilities'] = [
    ['id' => 'zeta.cap', 'support' => 'full'],
    ['id' => 'alpha.cap', 'support' => 'full'],
];
check(refuses_with($unsorted, 'profile-capabilities'), 'unsorted capability snapshot refused');
check(refuses_with(array_merge($resolution, ['profile_digest' => 'sha256:short']), 'profile-digest'), 'malformed profile digest refused');

// limits pairing
check(accepts(describe_request(['limits' => ['timeout_ms' => 30000, 'max_output_bytes' => 8388608]])), 'legal limits');
check(refuses_with(describe_request(['limits' => ['timeout_ms' => 30000]]), 'shape'), 'partial limits refused');
check(refuses_with(describe_request(['limits' => ['timeout_ms' => 0, 'max_output_bytes' => 8388608]]), 'shape'), 'out-of-bound timeout refused');
check(refuses_with(describe_request(['limits' => ['timeout_ms' => 30000, 'max_output_bytes' => 8388608, 'extra' => 1]]), 'shape'), 'closed limits members');

// --- deterministic generation seam ----------------------------------------

$gen = deterministic_generation(['target' => 'php-laravel', 'profile' => 'default', 'ir_path' => '.lekalo/ir/minimal.json']);
check(str_starts_with($gen['bytes'], "<?php\n"), 'generated file is PHP');
check(str_contains($gen['bytes'], 'declare(strict_types=1);'), 'strict types declared');
check(is_logical_path($gen['path']), 'generated path is logical');
check(scope_covers('.lekalo/generated/php-laravel/**', $gen['path']), 'generated path inside write scope');
check($gen['digest'] === sha256_digest($gen['bytes']), 'plan digest covers exact bytes');
$writes = deterministic_writes(['target' => 'php-laravel', 'profile' => 'default', 'ir_path' => '.lekalo/ir/minimal.json']);
check(plan_id($writes) === 'plan-' . sha256_hex(canonical_json($writes)), 'plan id deterministic over canonical bytes');
$genAgain = deterministic_generation(['target' => 'php-laravel', 'profile' => 'default', 'ir_path' => '.lekalo/ir/minimal.json']);
check($genAgain['bytes'] === $gen['bytes'] && $genAgain['digest'] === $gen['digest'], 'generation is a pure function');

// --- describe payload closure ---------------------------------------------

$caps = describe_capabilities();
check($caps['adapter']['id'] === ADAPTER_ID, 'identity id');
check($caps['protocol_versions'] === ['0.3.2'], 'declared protocol versions');
check($caps['ir_versions'] === [IR_VERSION], 'declared IR versions');
check(count($caps['operations']) === 9, 'complete v1 operation surface');
foreach ($caps['operations'] as $operation) {
    check(in_array($operation, OPERATIONS, true), "operation {$operation} in the closed set");
}
foreach ($caps['capabilities'] as $id => $state) {
    check(is_capability_id($id) && in_array($state, SUPPORT_STATES, true), "capability {$id} grammar");
}

// --- Laravel migration emitter (issue #57) -------------------------------

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

// --- dispatch smoke ---------------------------------------------------------

$response = dispatch(describe_request());
check($response['status'] === 'ok' && isset($response['capabilities']), 'describe dispatch');
$scanRequest = describe_request(['operation' => 'scan']);
$scanResponse = dispatch($scanRequest);
check(($scanResponse['result']['truncated'] ?? null) === false, 'scan dispatch');
check($scanResponse['result']['entries'] === [], 'scan of an empty staged view observes nothing');
// A real file inside a declared read root is enumerated, not fabricated.
$scanRoot = '.lekalo/ir';
if (!is_dir($scanRoot)) {
    mkdir($scanRoot, 0777, true);
}
file_put_contents($scanRoot . '/protocol-test.json', 'x');
$scanned = dispatch($scanRequest)['result']['entries'];
$observedPaths = array_column($scanned, 'path');
check(in_array('.lekalo/ir/protocol-test.json', $observedPaths, true), 'scan enumerates a real staged file');
check(!in_array('.lekalo/ir/minimal.json', $observedPaths, true), 'scan never fabricates an unobserved entry');
unlink($scanRoot . '/protocol-test.json');
$unsupported = dispatch(describe_request(['operation' => 'plan-native', 'protocol_version' => VERSION, 'native_request' => $nativeBase]));
check($unsupported['status'] === 'error' && $unsupported['error']['class'] === 'unsupported', 'plan-native honest unsupported');

// --- summary ----------------------------------------------------------------

fwrite(STDOUT, json_encode([
    'suite' => 'php-laravel-protocol',
    'checks' => $GLOBALS['__lekalo_checks'],
    'failures' => count($GLOBALS['__lekalo_failures']),
]) . "\n");
exit($GLOBALS['__lekalo_failures'] === [] ? 0 : 1);
