<?php
/**
 * The protocol unit suite of the `lekalo-php` language kernel (issue
 * #54): the strict request decoder, the closed validation rules,
 * canonical output, and the deterministic generation seam.
 * Dependency-free; run with `php tests/protocol.php` or through the
 * monorepo's `scripts/test-php-laravel-adapter.mjs` (the vendored
 * copy). The Laravel framework behavior is proven by the
 * `lekalo-php-laravel` suite, not here.
 */

declare(strict_types=1);

require __DIR__ . '/../src/analyzer.php';
require __DIR__ . '/../src/strict-profile.php';
require __DIR__ . '/../src/kernel.php';
// The provider registry rides like production (issue #241 stage 4):
// the framework families resolve through the provider seam, and this
// language suite exercises the seam with the synthetic fixture below
// and with no provider at all (the child leg).
require __DIR__ . '/../src/provider.php';

// The synthetic provider fixture (lekalo-php#1 acceptance): a second
// provider connects to the language adapter without a single change
// to it — exactly what a framework package does. The loader hooks are
// inert here (the suite never dispatches a real framework family);
// the runner table and lint facts carry fixture facts the checks
// below negotiate against.
lekalo_provider_register([
    'id' => 'lekalo-synthetic-fixture',
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
        'migrations.emit' => static function (string $bytes, string $target, string $irPath): array {
            throw new RequestRefusal('provider-descriptor');
        },
        'routes.emit' => static function (): void {},
        'scenario.runners' => static function (): array {
            return [
                'runners' => [
                    'testo-fixture' => [
                        'capabilities' => ['testing.clock', 'testing.fixtures'],
                        'concurrency' => true,
                        'eventCapture' => 'none',
                        'syntax' => 'testo-fixture',
                    ],
                ],
                'default' => 'testo-fixture',
            ];
        },
        'scenario.emit' => static function (): void {},
        'native.gates' => static function (string $method = 'load'): mixed {
            if ($method === 'policy_document') {
                // A minimal parsable policy: the dispatch smoke only
                // proves the digest-mismatch refusal path, so the
                // fixture document never needs real confirmations.
                return ['kind' => 'native-gate-policy', 'confirmations' => []];
            }
            return null;
        },
        'lint.framework-semantics' => static function (): array {
            return ['model_base' => 'Synthetic\\Framework\\Model'];
        },
    ],
]);

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
$writes = deterministic_writes($gen);
check(plan_id($writes) === 'plan-' . sha256_hex(canonical_json($writes)), 'plan id deterministic over canonical bytes');
$genAgain = deterministic_generation(['target' => 'php-laravel', 'profile' => 'default', 'ir_path' => '.lekalo/ir/minimal.json']);
check($genAgain['bytes'] === $gen['bytes'] && $genAgain['digest'] === $gen['digest'], 'generation is a pure function');

// --- describe payload closure ---------------------------------------------

$caps = describe_capabilities();
check($caps['adapter']['id'] === ADAPTER_ID, 'identity id');
check($caps['protocol_versions'] === ['0.3.2', '0.6.4'], 'declared protocol versions');
check($caps['ir_versions'] === [IR_VERSION], 'declared IR versions');
check(count($caps['operations']) === 10, 'complete v1 operation surface');
foreach ($caps['operations'] as $operation) {
    check(in_array($operation, OPERATIONS, true), "operation {$operation} in the closed set");
}
foreach ($caps['capabilities'] as $id => $state) {
    check(is_capability_id($id) && in_array($state, SUPPORT_STATES, true), "capability {$id} grammar");
}

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

// --- the adapter-owned PHP port declaration (issue #56) ---------------------

load_scenario_modules();

/** One valid declaration with one overridden or removed member. */
function port_doc(array $overrides = [], array $remove = []): array
{
    $doc = [
        'schema_version' => PHP_PORT_DOC_SCHEMA_VERSION,
        'identity' => PHP_PORT_DOC_IDENTITY,
        'port' => [
            'path' => 'tests/Support/PlannerPort.php',
            'class' => 'Tests\\Support\\PlannerPort',
            'exports' => [
                'invoke' => true,
                'state' => true,
                'actor' => true,
                'clock' => true,
                'ids' => true,
                'emissions' => true,
                'effects' => true,
                'authorize' => true,
                'reset' => true,
            ],
        ],
    ];
    foreach ($overrides as $key => $value) {
        $doc[$key] = $value;
    }
    foreach ($remove as $key) {
        unset($doc[$key]);
    }
    return $doc;
}

$validPort = php_validate_port_doc(port_doc());
check(is_array($validPort), 'valid port declaration accepted');
check($validPort['class'] === 'Tests\\Support\\PlannerPort', 'port class survives validation');
check(str_ends_with($validPort['path'], '.php'), 'port path is a php logical path');

$badPortCases = [
    'wrong-schema-version' => port_doc(['schema_version' => 'lekalo/php-test-port/v0.2.0']),
    'wrong-identity' => port_doc(['identity' => 'dev.lekalo.php-test-port@0.0.9']),
    'extra-top-member' => array_merge(port_doc(), ['extra' => true]),
    'extra-port-member' => port_doc(['port' => array_merge(port_doc()['port'], ['code' => '<?php'])]),
    'missing-port' => port_doc([], ['port']),
    'traversal-path' => port_doc(['port' => array_merge(port_doc()['port'], ['path' => '../escape/Port.php'])]),
    'absolute-path' => port_doc(['port' => array_merge(port_doc()['port'], ['path' => '/etc/Passport.php'])]),
    'wrong-suffix' => port_doc(['port' => array_merge(port_doc()['port'], ['path' => 'tests/Support/port.mjs'])]),
    'oversized-path' => port_doc(['port' => array_merge(port_doc()['port'], ['path' => str_repeat('a', 300) . '.php'])]),
    'missing-class' => port_doc(['port' => array_merge(port_doc()['port'], ['class' => ''])]),
    'code-in-class' => port_doc(['port' => array_merge(port_doc()['port'], ['class' => 'Tests\\Support\\Port; evil'])]),
    'leading-backslash-class' => port_doc(['port' => array_merge(port_doc()['port'], ['class' => '\\Tests\\Port'])]),
    'digit-lead-class' => port_doc(['port' => array_merge(port_doc()['port'], ['class' => '9Port'])]),
    'missing-invoke' => port_doc(['port' => array_merge(port_doc()['port'], ['exports' => ['state' => true]])]),
    'false-invoke' => port_doc(['port' => array_merge(port_doc()['port'], ['exports' => ['invoke' => false]])]),
    'unknown-export' => port_doc(['port' => array_merge(port_doc()['port'], ['exports' => ['invoke' => true, 'gorilla' => true]])]),
    'non-bool-export' => port_doc(['port' => array_merge(port_doc()['port'], ['exports' => ['invoke' => true, 'state' => 'yes']])]),
];
foreach ($badPortCases as $name => $doc) {
    check(php_validate_port_doc($doc) === null, "hostile port declaration refused: {$name}");
}

// The scenario-IR recognition admits the planner fixture spellings
// (`<project>.scenario.<case>.json`) without widening to arbitrary JSON.
check(is_scenario_ir_path('lekalo/scenarios/planner.scenario.focus_happy.json'), 'planner scenario spelling recognized');
check(is_scenario_ir_path('lekalo/scenarios/thing.scenario.json'), 'plain scenario spelling recognized');
check(!is_scenario_ir_path('lekalo/scenarios/notes.json'), 'arbitrary json is not a scenario');
check(!is_scenario_ir_path('lekalo/php-test-port.json'), 'the port declaration is not a scenario document');

// --- comment-safe summary projection ----------------------------------------

// A one-line comment ends at the close-tag pair a hostile summary can
// carry: the emitted file lints clean but dumps its tail as output, and
// a follow-up open tag would re-enter PHP mode. The projection must
// break the pair itself, so the emitted comment stays inert text.
$leak = php_comment_safe('Focusing x?>LEAKED');
check(strpos($leak, '?>') === false, 'comment projection never carries a close tag');
check(strpos($leak, "\n") === false && strpos($leak, "\r") === false, 'comment projection stays single-line');
$reopen = php_comment_safe('x?><?php exit(1);');
check(strpos($reopen, '?>') === false, 'close-tag-plus-reopen collapses to inert text');
check(strpos($reopen, '<?php') !== false, 'the reopened php token stays inside the neutralized comment');
check(php_comment_safe('Seed a task, focus it, and observe.') === 'Seed a task, focus it, and observe.', 'clean summaries project unchanged');

// --- provider registry (lekalo-php#1, issue #241 stage 4) -------------------

// The synthetic fixture provider resolves every framework family the
// kernel dispatches, with its negotiated state.
$capability = lekalo_provider_require('migrations.emit');
check($capability['provider'] === 'lekalo-synthetic-fixture', 'migration family resolves through the provider');
check($capability['state'] === 'full', 'migration family negotiates full support');
check(lekalo_provider_require('lint.framework-semantics')['state'] === 'partial', 'lint family negotiates partial support');
$providers = lekalo_providers();
check(count($providers) === 1 && $providers[0]['id'] === 'lekalo-synthetic-fixture', 'exactly one provider is registered');
check($providers[0]['api'] === PROVIDER_API_SCHEMA_VERSION, 'the provider speaks the adapter provider API version');

// The scenario runner table is provider-supplied and complete.
$runners = lekalo_provider_scenario_runners();
check(isset($runners['testo-fixture']) && $runners['testo-fixture']['concurrency'] === true, 'the fixture runner arrives from the provider');
check(lekalo_provider_default_scenario_runner() === 'testo-fixture', 'the default runner arrives from the provider');

// The lint facts classify the provider's model base and nothing else.
check(lekalo_provider_lint_classifies_model('Synthetic\\Framework\\Model'), 'the fixture model base classifies through the provider');
check(!lekalo_provider_lint_classifies_model('App\\Model'), 'an unrelated base class does not classify');
check(lekalo_provider_lint_observer_semantics(), 'observer lint semantics have a provider');

// Descriptor refusals are stable codes, never silent downgrades.
try {
    lekalo_provider_register([
        'id' => 'lekalo-synthetic-fixture', 'version' => '0.0.1',
        'api' => PROVIDER_API_SCHEMA_VERSION, 'capabilities' => [], 'hooks' => [],
    ]);
    check(false, 'duplicate provider registration refused');
} catch (RequestRefusal $r) {
    check($r->getMessage() === 'provider-duplicate', 'duplicate provider registration refused');
}
try {
    lekalo_provider_register([
        'id' => 'synthetic-second', 'version' => '0.1.0',
        'api' => 'lekalo/provider/v9.9.9', 'capabilities' => [], 'hooks' => [],
    ]);
    check(false, 'foreign provider API version refused');
} catch (RequestRefusal $r) {
    check($r->getMessage() === 'provider-version-conflict', 'foreign provider API version refused');
}
try {
    lekalo_provider_register([
        'id' => 'synthetic-second', 'version' => '0.1.0',
        'api' => PROVIDER_API_SCHEMA_VERSION,
        'capabilities' => ['not-a-family' => 'full'], 'hooks' => [],
    ]);
    check(false, 'unknown capability family refused');
} catch (RequestRefusal $r) {
    check($r->getMessage() === 'provider-descriptor', 'unknown capability family refused');
}
try {
    lekalo_provider_register([
        'id' => 'synthetic-second', 'version' => '0.1.0',
        'api' => PROVIDER_API_SCHEMA_VERSION,
        'capabilities' => ['routes.emit' => 'full'], 'hooks' => [],
    ]);
    check(false, 'capability without hook refused');
} catch (RequestRefusal $r) {
    check($r->getMessage() === 'provider-descriptor', 'capability without hook refused');
}

// With no provider registered the registry degrades honestly: probes
// return null, negotiation is the typed provider-missing refusal, and
// the language-only lint walk carries no framework classification. The
// registry is process-global, so the empty-registry leg runs in a
// child interpreter with only the provider contract loaded.
$providerChild = <<<'CHILD'
<?php
// The provider contract's refusals ride the kernel's RequestRefusal;
// the child declares the transport stub so the contract loads alone.
final class RequestRefusal extends RuntimeException {}
require $argv[1] . '/src/provider.php';
$missed = false;
try {
    lekalo_provider_require('routes.emit');
} catch (RequestRefusal $r) {
    $missed = $r->getMessage() === 'provider-missing';
}
echo json_encode([
    'probe' => lekalo_provider_capability('routes.emit') === null,
    'missed' => $missed,
    'runners' => lekalo_provider_scenario_runners() === null,
    'model' => lekalo_provider_lint_classifies_model('Illuminate\\Database\\Eloquent\\Model') === false,
]) . "\n";
CHILD;
$childPath = tempnam(sys_get_temp_dir(), 'lekalo-provider-');
file_put_contents($childPath, $providerChild);
// The child spawns through proc_open with piped stdio (the process
// suite's pattern): no shell redirection, so the leg is identical on
// Windows, macOS, and Linux.
$childCommand = escapeshellarg(PHP_BINARY) . ' -n ' . escapeshellarg($childPath)
    . ' ' . escapeshellarg(dirname(__DIR__));
$process = proc_open($childCommand, [
    0 => ['pipe', 'r'],
    1 => ['pipe', 'w'],
    2 => ['pipe', 'w'],
], $pipes);
$childOutput = false;
if (is_resource($process)) {
    fclose($pipes[0]);
    $childOutput = stream_get_contents($pipes[1]);
    fclose($pipes[1]);
    fclose($pipes[2]);
    proc_close($process);
}
unlink($childPath);
$degraded = json_decode((string) $childOutput, true);
check(is_array($degraded), 'the empty-registry child reports');
check($degraded['probe'] ?? false, 'with no provider the family probe is null');
check($degraded['missed'] ?? false, 'with no provider the negotiation refuses provider-missing');
check($degraded['runners'] ?? false, 'with no provider the runner table is null');
check($degraded['model'] ?? false, 'with no provider no base class classifies as a model');

// --- summary ----------------------------------------------------------------

fwrite(STDOUT, json_encode([
    'suite' => 'php-laravel-protocol',
    'checks' => $GLOBALS['__lekalo_checks'],
    'failures' => count($GLOBALS['__lekalo_failures']),
]) . "\n");
exit($GLOBALS['__lekalo_failures'] === [] ? 0 : 1);
