<?php

/**
 * Pure Scenario IR → test-AST mapping for the scenario-test compiler
 * (issue #56, S2). A structural port of the Node mapper
 * (`adapters/node-typescript/src/scenario-map.mjs`): the same closed
 * scenario shapes resolve to the same test model, with the PHP runner
 * registry in place of the Node one.
 *
 * Inputs are one decoded Scenario IR document, one decoded compiled
 * project IR document, one decoded project test-port declaration, and
 * the profile's negotiated capability snapshot. The output is a closed
 * test model plus typed findings. No filesystem, clock, environment,
 * process, or network access happens here: the same inputs always map
 * to the same model — the determinism contract the byte-stable emitter
 * depends on.
 *
 * Honesty rules (mirrored from the Node mapper):
 * - Every scenario feature without a port surface, runner capability,
 *   or resolvable target lands in `unsupported[]` with its diagnostic
 *   — never silently dropped, never approximated, never a pass.
 * - Operation references resolve to `command` or `query` from the
 *   compiled IR, never from the name; anything else is a
 *   `scenario.operation-unresolved` compile-time finding.
 * - Concurrency race scenarios (metadata `testing.concurrency`) map to
 *   an explicit whole-scenario unsupported outcome: a serial execution
 *   never satisfies a race fixture.
 * - `unsupported` assertion kinds compile to recorded unsupported rows
 *   and can never report pass.
 */

const PHP_SCENARIO_IDENTITY = 'dev.lekalo.scenario-ir@0.2.16';
const PHP_IR_IDENTITY = 'dev.lekalo.ir@0.2.16';

/** The generated scenario-test home under the generated root. */
const PHP_SCENARIO_TESTS_DIR = 'src/generated/php-laravel/scenario-tests';

/**
 * The adapter-owned PHP port declaration path (issue #56).
 *
 * The core `lekalo/test-port` contract v0.4.0 restricts port paths to
 * `.mjs`/`.ts` modules, and its version custody is pinned to the
 * workspace product version, so a `.php` port cannot ride that family
 * without the coordinated contract successor (docs/m5/issue-56-research.md
 * S1, deliberately deferred). Until that successor lands, the PHP
 * adapter reads its own bounded sibling document `lekalo/php-test-port.json`
 * with the exact closed export-flag vocabulary of the core contract, so
 * the eventual lift is mechanical.
 */
const PHP_PORT_DOC_PATH = 'lekalo/php-test-port.json';

/** The closed identity of the adapter-owned PHP port declaration. */
const PHP_PORT_DOC_SCHEMA_VERSION = 'lekalo/php-test-port/v0.1.0';
const PHP_PORT_DOC_IDENTITY = 'dev.lekalo.php-test-port@0.1.0';

/** The declared-port class FQN grammar: PSR-4 style, bounded, no code. */
const PHP_PORT_CLASS_PATTERN = '/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*)*$/';

/**
 * The closed PHP runner registry. The Laratesto entry mirrors the
 * bridge's declared facts on the pinned toolchain: the Laravel suite is
 * sequential (fresh application per test), so `testing.concurrency`
 * stays deliberately absent — a race case compiles to an explicit
 * unsupported outcome, never to a serial run that would lie.
 */
const PHP_RUNNER_REGISTRY = [
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
];

/** The default runner when a scenario declares no native binding. */
const PHP_DEFAULT_RUNNER = 'laratesto';

/**
 * Binding capabilities the port's `invoke` surface itself provides when
 * the dispatch enforces idempotency dedup (mirrors the Node mapper).
 */
const PHP_PORT_PROVIDED_CAPABILITIES = [
    'idempotency.durable_key',
    'idempotency.replay',
];

/** The scenario metadata key whose presence marks a concurrency race case. */
const PHP_CONCURRENCY_METADATA_KEY = 'testing.concurrency';

/** The closed given-step precondition kinds. */
const PHP_PRECONDITION_KINDS = ['state', 'fixture', 'actor', 'clock', 'id_source'];

/** The closed assertion kinds. */
const PHP_ASSERTION_KINDS = [
    'result', 'error', 'entity_state', 'emitted', 'forbidden_effect',
    'authorization', 'idempotency', 'contract_match', 'deterministic_fixture',
    'unsupported',
];

/** The closed typed-value wire kinds (scenario/value.rs). */
const PHP_VALUE_KINDS = [
    'null', 'boolean', 'integer', 'string', 'decimal', 'date', 'datetime',
    'uuid', 'uri', 'list', 'object',
];

/** The closed reference kinds (scenario/reference.rs). */
const PHP_REF_KINDS = [
    'symbol', 'operation', 'entity', 'field', 'event', 'job', 'effect',
    'error', 'requirement', 'fixture', 'actor', 'clock', 'id-source',
    'step-output', 'given-value',
];

/** IR bounds mirrored from the core (scenario/version.rs). */
const PHP_LIMITS = [
    'maxGivenSteps' => 256,
    'maxWhenSteps' => 256,
    'maxThenSteps' => 512,
    'maxTotalSteps' => 1024,
    'maxBindings' => 32,
    'maxTypedDepth' => 32,
    'maxTypedItems' => 4096,
    'maxScalarCodepoints' => 4096,
];

/** The closed port surface flags (beyond the mandatory `invoke`). */
const PHP_PORT_FLAGS = [
    'invoke', 'state', 'fixtures', 'actor', 'clock', 'ids',
    'emissions', 'effects', 'authorize', 'contractCheck', 'fixtureDigest',
    'reset',
];

// ---------------------------------------------------------------------------
// Closed-shape grammar checks (mirrors of the Node mapper predicates).
// ---------------------------------------------------------------------------

/**
 * The closed SemanticId grammar mirrored from the core
 * (scenario/id.rs): two or three dot-separated lowercase segments
 * (`[a-z][a-z0-9_]*`, ≤63 each), total ≤191 bytes, and the first
 * segment never the reserved `lekalo`/`dev`.
 */
function is_php_semantic_id(mixed $text): bool
{
    if (!is_string($text) || $text === '' || strlen($text) > 191) {
        return false;
    }
    $segments = explode('.', $text);
    $count = count($segments);
    if ($count < 2 || $count > 3) {
        return false;
    }
    foreach ($segments as $index => $segment) {
        if ($segment === '' || strlen($segment) > 63) {
            return false;
        }
        if (!preg_match('/^[a-z][a-z0-9_]*$/', $segment)) {
            return false;
        }
        if ($index === 0 && ($segment === 'lekalo' || $segment === 'dev')) {
            return false;
        }
    }
    return true;
}

/** The closed single-segment step-id grammar (scenario/id.rs::StepId). */
function is_php_step_id(mixed $text): bool
{
    return is_string($text)
        && $text !== ''
        && strlen($text) <= 64
        && preg_match('/^[a-z][a-z0-9_]*$/', $text) === 1;
}

/** The closed canonical JSON writer (byte-identical to the Node rule). */
function php_canonical_json(mixed $value): string
{
    if ($value === null) {
        return 'null';
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if (is_int($value)) {
        return (string) $value;
    }
    if (is_string($value)) {
        return json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    }
    if (is_array($value) && array_is_list($value)) {
        $items = array_map(__FUNCTION__, $value);
        return '[' . implode(',', $items) . ']';
    }
    if (is_array($value)) {
        $keys = array_keys($value);
        usort($keys, 'strcmp');
        $body = [];
        foreach ($keys as $key) {
            $body[] = php_canonical_json((string) $key) . ':' . php_canonical_json($value[$key]);
        }
        return '{' . implode(',', $body) . '}';
    }
    // Floats never appear in a closed scenario document: the wire keeps
    // integers as integers (or decimal spellings) and strings as strings.
    throw new LogicException('unrenderable canonical JSON value');
}

// ---------------------------------------------------------------------------
// Mapping entry point.
// ---------------------------------------------------------------------------

/**
 * Map one scenario document plus its joined context into the test model.
 *
 * `input` is `{scenario, ir, port, portPresent, profileCapabilities}`:
 * `scenario` is the decoded Scenario IR document, `ir` the decoded
 * compiled project IR evidence (null when absent), `port` the decoded
 * test-port declaration (null when `portPresent` is false), and
 * `profileCapabilities` the negotiated `[{id, support}]` snapshot (null
 * when the launch carries no resolution). Returns the closed mapper
 * outcome: `{state: "refused", refusal}` or
 * `{state: "mapped", scenarios, findings}`.
 */
function php_map_scenario(array $input): array
{
    $scenario = $input['scenario'] ?? null;
    $findings = [];
    $context = [
        'findings' => &$findings,
        'scenarioId' => is_string($scenario['scenarioId'] ?? null) ? $scenario['scenarioId'] : null,
        'operationIndex' => php_operation_index($input['ir'] ?? null),
    ];
    $shapeRefusal = php_check_scenario_shape($scenario);
    if ($shapeRefusal !== null) {
        return ['state' => 'refused', 'refusal' => $shapeRefusal, 'scenarios' => [], 'findings' => []];
    }
    $ir = $input['ir'] ?? null;
    $irDigest = $input['irDigest'] ?? null;
    $irRefOk = is_array($ir)
        && ($ir['contract'] ?? null) === PHP_IR_IDENTITY
        && isset($scenario['irRef']['digest'])
        && $irDigest !== null
        && $scenario['irRef']['digest'] === $irDigest;
    if (!$irRefOk) {
        $findings[] = [
            'code' => 'scenario.ir-ref-mismatch',
            'symbol' => $scenario['scenarioId'],
            'detail' => 'ir-ref-digest',
        ];
    }
    $runner = php_resolve_runner($scenario, $findings);
    $portSurface = php_resolve_port_surface($input, $findings);
    $unsupported = [];
    php_collect_concurrency_unsupported($scenario, $runner, $unsupported);
    php_collect_capability_gaps($scenario, $input, $runner, $unsupported);
    $model = [
        'id' => $scenario['scenarioId'],
        'version' => $scenario['scenarioVersion'],
        'summary' => $scenario['summary'],
        'projectId' => $scenario['projectId'],
        'irDigest' => $scenario['irRef']['digest'] ?? null,
        'runner' => $runner,
        'binding' => php_binding_model($scenario),
        'tags' => $scenario['tags'] ?? [],
        'unsupported' => $unsupported,
        'given' => php_map_given($scenario['given'] ?? [], $portSurface),
        'when' => php_map_when($scenario['when'] ?? [], $context, $portSurface),
        'then' => php_map_then($scenario['then'] ?? [], $context, $portSurface),
    ];
    return ['state' => 'mapped', 'scenarios' => [$model], 'findings' => $findings];
}

/** The closed-shape decoder (defense in depth over the core's custody). */
function php_check_scenario_shape(mixed $scenario): ?string
{
    if (!is_array($scenario) || array_is_list($scenario)) {
        return 'scenario-shape';
    }
    if (($scenario['schemaVersion'] ?? null) !== 'lekalo/scenario-ir/v0.2.16'
        || ($scenario['identity'] ?? null) !== PHP_SCENARIO_IDENTITY) {
        return 'scenario-identity';
    }
    foreach ([
        'projectId', 'scenarioId', 'scenarioVersion', 'summary',
        'irRef', 'modelRef', 'given', 'when', 'then', 'bindings', 'tags', 'metadata',
    ] as $key) {
        if (!array_key_exists($key, $scenario)) {
            return 'scenario-missing-field';
        }
    }
    // The comment block of every emitted test interpolates the version
    // and the summary: both must be strings before the comment-safe
    // projection runs, so a non-string wire shape can never reach the
    // emitter at all (defense in depth over the core's custody).
    foreach (['scenarioVersion', 'summary'] as $textField) {
        if (!is_string($scenario[$textField])) {
            return 'scenario-text-field';
        }
    }
    if (!is_php_semantic_id($scenario['scenarioId'])) {
        return 'scenario-id';
    }
    if (!is_array($scenario['given']) || !is_array($scenario['when']) || !is_array($scenario['then'])) {
        return 'scenario-steps-shape';
    }
    if (count($scenario['when']) === 0 || count($scenario['then']) === 0) {
        return 'scenario-empty';
    }
    if (count($scenario['given']) > PHP_LIMITS['maxGivenSteps']
        || count($scenario['when']) > PHP_LIMITS['maxWhenSteps']
        || count($scenario['then']) > PHP_LIMITS['maxThenSteps']
        || count($scenario['given']) + count($scenario['when']) + count($scenario['then'])
            > PHP_LIMITS['maxTotalSteps']) {
        return 'scenario-steps-bound';
    }
    if (!is_array($scenario['bindings']) || count($scenario['bindings']) > PHP_LIMITS['maxBindings']) {
        return 'scenario-bindings-bound';
    }
    $stepIds = [];
    foreach ([$scenario['given'], $scenario['when'], $scenario['then']] as $role) {
        foreach ($role as $step) {
            if (!is_array($step) || array_is_list($step)
                || !is_php_step_id($step['stepId'] ?? null)) {
                return 'scenario-step-id';
            }
            if (in_array($step['stepId'], $stepIds, true)) {
                return 'scenario-duplicate-step-id';
            }
            $stepIds[] = $step['stepId'];
        }
    }
    return null;
}

/** The wire-shape check for one typed value or reference leaf. */
function php_check_leaf(mixed $leaf, int $depth): ?string
{
    if (!is_array($leaf) || array_is_list($leaf)) {
        return 'leaf-shape';
    }
    if ($depth > PHP_LIMITS['maxTypedDepth']) {
        return 'leaf-depth';
    }
    if (is_string($leaf['$ref'] ?? null)) {
        return in_array($leaf['$ref'], PHP_REF_KINDS, true) ? null : 'leaf-ref-kind';
    }
    if (!is_string($leaf['type'] ?? null) || !in_array($leaf['type'], PHP_VALUE_KINDS, true)) {
        return 'leaf-value-kind';
    }
    if ($leaf['type'] === 'list') {
        if (!is_array($leaf['value'] ?? null) || count($leaf['value']) > PHP_LIMITS['maxTypedItems']) {
            return 'leaf-items';
        }
        foreach ($leaf['value'] as $item) {
            $problem = php_check_leaf($item, $depth + 1);
            if ($problem !== null) {
                return $problem;
            }
        }
        return null;
    }
    if ($leaf['type'] === 'object') {
        $map = $leaf['value'] ?? null;
        if (!is_array($map) || array_is_list($map)) {
            return 'leaf-object';
        }
        if (count($map) > PHP_LIMITS['maxTypedItems']) {
            return 'leaf-items';
        }
        foreach ($map as $child) {
            $problem = php_check_leaf($child, $depth + 1);
            if ($problem !== null) {
                return $problem;
            }
        }
        return null;
    }
    if (!array_key_exists('value', $leaf)) {
        return 'leaf-value';
    }
    if (is_string($leaf['value'])
        && mb_strlen($leaf['value']) > PHP_LIMITS['maxScalarCodepoints']) {
        return 'leaf-scalar';
    }
    return null;
}

// ---------------------------------------------------------------------------
// Resolution: operations, runners, port surfaces, capabilities.
// ---------------------------------------------------------------------------

/** The command/query index of the compiled IR evidence. */
function php_operation_index(mixed $ir): array
{
    $index = [];
    if (!is_array($ir) || !is_array($ir['definitions'] ?? null)) {
        return $index;
    }
    foreach ($ir['definitions'] as $definition) {
        if (!is_array($definition)) {
            continue;
        }
        if (($definition['kind'] ?? null) === 'command' || ($definition['kind'] ?? null) === 'query') {
            $index[$definition['id']] = $definition['kind'];
        }
    }
    return $index;
}

/**
 * Resolve one scenario operation reference against the compiled IR. The
 * exact definition id wins; otherwise the kind-qualified wire spelling
 * (`<module>.command.<name>` / `<module>.query.<name>`) resolves when
 * the base id is declared with exactly that kind. Anything else is
 * unresolved — never a guessed call kind.
 */
function php_resolve_operation(array $index, string $id): ?array
{
    if (isset($index[$id])) {
        return ['id' => $id, 'kind' => $index[$id]];
    }
    $segments = explode('.', $id);
    $count = count($segments);
    if ($count >= 3) {
        $kind = $segments[$count - 2];
        if ($kind === 'command' || $kind === 'query') {
            $base = implode('.', array_merge(array_slice($segments, 0, -2), [$segments[$count - 1]]));
            if (($index[$base] ?? null) === $kind) {
                return ['id' => $base, 'kind' => $kind, 'ref' => $id];
            }
        }
    }
    return null;
}

/** The resolved runner entry, or an unknown-runner finding with the default. */
function php_resolve_runner(array $scenario, array &$findings): array
{
    $nativeBinding = null;
    foreach ($scenario['bindings'] ?? [] as $binding) {
        if (is_array($binding) && ($binding['backend'] ?? null) === 'native') {
            $nativeBinding = $binding;
            break;
        }
    }
    $runnerId = is_string($nativeBinding['runner'] ?? null)
        ? $nativeBinding['runner']
        : PHP_DEFAULT_RUNNER;
    if (!isset(PHP_RUNNER_REGISTRY[$runnerId])) {
        $findings[] = [
            'code' => 'scenario.runner-unknown',
            'symbol' => $scenario['scenarioId'],
            'detail' => php_bound_token($runnerId),
        ];
        $runnerId = PHP_DEFAULT_RUNNER;
    }
    $entry = PHP_RUNNER_REGISTRY[$runnerId];
    $runner = array_merge(['id' => $runnerId], $entry);
    if (is_string($nativeBinding['runnerVersion'] ?? null)) {
        $runner['declaredVersion'] = $nativeBinding['runnerVersion'];
    }
    return $runner;
}

/**
 * Validate the adapter-owned PHP port declaration against its closed
 * shape (issue #56): bounded document, closed identity, one logical
 * `.php` path, one PSR-4 class FQN, and the exact closed export-flag
 * vocabulary of the core test-port contract (absent/other-than-true
 * means the feature compiles to an explicit unsupported diagnostic).
 * Returns the normalized `{path, class, exports}` document, or null
 * when any bound is violated. No code, no expressions, no traversal:
 * the grammar itself keeps the declaration data-only.
 */
function php_validate_port_doc(array $doc): ?array
{
    if (($doc['schema_version'] ?? null) !== PHP_PORT_DOC_SCHEMA_VERSION
        || ($doc['identity'] ?? null) !== PHP_PORT_DOC_IDENTITY
        || count($doc) !== 3
        || !is_array($doc['port'] ?? null)
        || count($doc['port']) !== 3) {
        return null;
    }
    $port = $doc['port'];
    $path = $port['path'] ?? null;
    if (!is_string($path) || $path === '' || strlen($path) > 256
        || preg_match('/^[a-zA-Z0-9][a-zA-Z0-9._\/-]*\\.php$/', $path) !== 1
        || str_contains($path, '..')) {
        return null;
    }
    $class = $port['class'] ?? null;
    if (!is_string($class) || $class === '' || strlen($class) > 256
        || preg_match(PHP_PORT_CLASS_PATTERN, $class) !== 1) {
        return null;
    }
    $exports = $port['exports'] ?? null;
    if (!is_array($exports) || ($exports['invoke'] ?? null) !== true) {
        return null;
    }
    foreach ($exports as $flag => $value) {
        if (!in_array($flag, PHP_PORT_FLAGS, true) || !is_bool($value)) {
            return null;
        }
    }
    return ['path' => $path, 'class' => $class, 'exports' => $exports];
}

/** The port surface join: every closed port flag the project declares.
 * `port` is the kernel-validated normalized document (`path`, `class`,
 * `exports`); a declaration-absent project keeps the port-missing
 * finding and an all-false surface, so every port-backed feature maps
 * to an explicit unsupported diagnostic.
 */
function php_resolve_port_surface(array $input, array &$findings): array
{
    $port = $input['port'] ?? null;
    if (($input['portPresent'] ?? false) !== true || !is_array($port)) {
        $findings[] = ['code' => 'scenario.port-missing', 'detail' => 'declaration-absent'];
        return php_empty_surface();
    }
    $exports = is_array($port['exports'] ?? null) ? $port['exports'] : null;
    if (!is_array($exports) || ($exports['invoke'] ?? null) !== true) {
        $findings[] = ['code' => 'scenario.port-shape', 'detail' => 'exports-shape'];
        return php_empty_surface();
    }
    $surface = [];
    foreach (PHP_PORT_FLAGS as $flag) {
        $surface[$flag] = ($exports[$flag] ?? null) === true;
    }
    return $surface;
}

function php_empty_surface(): array
{
    $surface = [];
    foreach (PHP_PORT_FLAGS as $flag) {
        $surface[$flag] = false;
    }
    return $surface;
}

/**
 * Concurrency race cases: a scenario that declares the concurrency
 * metadata compiles to one whole-scenario unsupported row. A serial
 * execution never satisfies a race fixture, and the Laratesto suite is
 * sequential by contract.
 */
function php_collect_concurrency_unsupported(array $scenario, array $runner, array &$unsupported): void
{
    $metadata = $scenario['metadata'] ?? null;
    if (!is_array($metadata) || !array_key_exists(PHP_CONCURRENCY_METADATA_KEY, $metadata)) {
        return;
    }
    if (in_array('testing.concurrency', $runner['capabilities'], true)) {
        return;
    }
    $unsupported[] = [
        'step' => null,
        'capability' => 'testing.concurrency',
        'reason' => 'scenario-requires-concurrency',
        'detail' => php_bound_token((string) $metadata[PHP_CONCURRENCY_METADATA_KEY]),
    ];
}

/**
 * The binding capability join: every declared binding capability must be
 * resolvable from the runner registry entry, the port dispatch surface,
 * or the negotiated profile snapshot; each gap is one unsupported row.
 */
function php_collect_capability_gaps(array $scenario, array $input, array $runner, array &$unsupported): void
{
    $profile = [];
    foreach ($input['profileCapabilities'] ?? [] as $entry) {
        if (is_array($entry) && isset($entry['id']) && isset($entry['support'])
            && $entry['support'] !== 'unsupported') {
            $profile[$entry['id']] = true;
        }
    }
    foreach ($scenario['bindings'] ?? [] as $binding) {
        if (!is_array($binding) || ($binding['backend'] ?? null) !== 'native') {
            continue; // fake-reference stays with #107
        }
        foreach (is_array($binding['capabilities'] ?? null) ? $binding['capabilities'] : [] as $capability) {
            if (!is_string($capability)) {
                continue;
            }
            if (in_array($capability, $runner['capabilities'], true)) {
                continue;
            }
            if (in_array($capability, PHP_PORT_PROVIDED_CAPABILITIES, true)) {
                continue;
            }
            if ($profile !== [] && !isset($profile[$capability])) {
                $unsupported[] = [
                    'step' => null,
                    'capability' => $capability,
                    'reason' => 'binding-capability-gap',
                    'detail' => 'profile-snapshot',
                ];
                continue;
            }
            if ($profile === []) {
                $unsupported[] = [
                    'step' => null,
                    'capability' => $capability,
                    'reason' => 'binding-capability-gap',
                    'detail' => 'runner-registry',
                ];
            }
        }
    }
}

/** The binding metadata of the generated test (native binding only). */
function php_binding_model(array $scenario): array
{
    $nativeBinding = null;
    foreach ($scenario['bindings'] ?? [] as $binding) {
        if (is_array($binding) && ($binding['backend'] ?? null) === 'native') {
            $nativeBinding = $binding;
            break;
        }
    }
    if ($nativeBinding === null) {
        return ['mode' => 'generated', 'backend' => 'none', 'test' => null, 'capabilityDigest' => null];
    }
    return [
        'mode' => is_string($nativeBinding['mode'] ?? null) ? $nativeBinding['mode'] : 'generated',
        'backend' => 'native',
        'test' => is_string($nativeBinding['test'] ?? null) ? $nativeBinding['test'] : null,
        'capabilityDigest' => is_string($nativeBinding['capabilityDigest'] ?? null)
            ? $nativeBinding['capabilityDigest'] : null,
    ];
}

// ---------------------------------------------------------------------------
// Step mapping: given / when / then, each port-joined.
// ---------------------------------------------------------------------------

function php_map_given(array $given, array $portSurface): array
{
    $surfaceOf = [
        'state' => 'state',
        'fixture' => 'fixtures',
        'actor' => 'actor',
        'clock' => 'clock',
        'id_source' => 'ids',
    ];
    return array_map(static function (array $step) use ($surfaceOf, $portSurface): array {
        $precondition = is_array($step['precondition'] ?? null) ? $step['precondition'] : [];
        $kind = $precondition['kind'] ?? null;
        $mapped = [
            'stepId' => $step['stepId'],
            'kind' => in_array($kind, PHP_PRECONDITION_KINDS, true) ? $kind : 'unknown',
            'port' => null,
            'unsupported' => null,
        ];
        $surface = $surfaceOf[$kind] ?? null;
        if ($surface === null) {
            $mapped['unsupported'] = [
                'capability' => 'scenario.precondition.' . ($kind ?? 'unknown'),
                'reason' => 'precondition-kind-unknown',
            ];
            return $mapped;
        }
        if (!$portSurface[$surface]) {
            $mapped['unsupported'] = [
                'capability' => 'testing.' . ($surface === 'ids' ? 'ids' : $surface),
                'reason' => 'port-surface-absent',
                'detail' => $surface,
            ];
            return $mapped;
        }
        $mapped['port'] = $surface;
        $mapped['payload'] = php_precondition_payload($precondition);
        // Typed leaves propagate as unsupported rows (review R-3), never
        // as mid-render crashes.
        if ($kind === 'state') {
            $problem = php_state_leaf_problem($mapped['payload']);
            if ($problem !== null) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.value',
                    'reason' => $problem['reason'],
                    'detail' => php_bound_token($problem['field']),
                ];
            }
        }
        return $mapped;
    }, $given);
}

/** The first unrenderable leaf of one mapped state precondition. */
function php_state_leaf_problem(array $payload): ?array
{
    foreach ($payload['selector'] ?? [] as $term) {
        $problem = php_check_leaf($term['equals'] ?? null, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $term['field'] ?? null];
        }
    }
    foreach ($payload['fields'] ?? [] as $entry) {
        $problem = php_check_leaf($entry[1] ?? null, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $entry[0] ?? null];
        }
    }
    return null;
}

/** The first unrenderable leaf of one mapped entity_state assertion. */
function php_entity_state_leaf_problem(array $payload): ?array
{
    foreach ($payload['where'] ?? [] as $term) {
        $problem = php_check_leaf($term['equals'] ?? null, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $term['field'] ?? null];
        }
    }
    foreach ($payload['fields'] ?? [] as $field => $expectation) {
        if (is_array($expectation) && array_key_exists('match', $expectation)) {
            continue; // A match-kind expectation is a kind token, never a leaf.
        }
        $problem = php_check_leaf(is_array($expectation) && array_key_exists('value', $expectation)
            ? $expectation['value'] : $expectation, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $field];
        }
    }
    return null;
}

function php_precondition_payload(array $precondition): array
{
    switch ($precondition['kind'] ?? null) {
        case 'state':
            $fields = [];
            foreach ($precondition['fields'] ?? [] as $field => $leaf) {
                $fields[] = [$field, $leaf];
            }
            return [
                'entity' => $precondition['entity'] ?? null,
                'selector' => array_map(static fn (array $term): array => [
                    'field' => $term['field'] ?? null,
                    'equals' => $term['equals'] ?? null,
                ], $precondition['selector'] ?? []),
                'fields' => $fields,
            ];
        case 'fixture':
            return [
                'fixture' => $precondition['fixture'] ?? null,
                'version' => $precondition['version'] ?? null,
                'capabilities' => $precondition['capabilities'] ?? [],
            ];
        case 'actor':
            return [
                'actor' => $precondition['actor'] ?? null,
                'scope' => $precondition['scope'] ?? null,
            ];
        case 'clock':
            return ['at' => $precondition['at']['value'] ?? null];
        case 'id_source':
            return [
                'seed' => $precondition['seed'] ?? null,
                'algorithm' => $precondition['algorithm'] ?? null,
            ];
        default:
            return [];
    }
}

function php_map_when(array $when, array &$context, array $portSurface): array
{
    return array_map(static function (array $step) use (&$context, $portSurface): array {
        $action = is_array($step['action'] ?? null) ? $step['action'] : [];
        $mapped = [
            'stepId' => $step['stepId'],
            'kind' => 'invoke',
            'operation' => null,
            'input' => [],
            'ctx' => [],
            'replay' => null,
            'unsupported' => null,
        ];
        if (($action['kind'] ?? null) !== 'invoke') {
            $mapped['unsupported'] = ['capability' => 'scenario.action', 'reason' => 'action-kind-unknown'];
            return $mapped;
        }
        $operationId = $action['operation'] ?? null;
        if (!is_string($operationId)) {
            $context['findings'][] = [
                'code' => 'scenario.operation-unresolved',
                'symbol' => $context['scenarioId'],
                'detail' => php_bound_token((string) $operationId),
            ];
            $mapped['unsupported'] = [
                'capability' => 'scenario.operation',
                'reason' => 'operation-unresolved',
                'detail' => php_bound_token((string) $operationId),
            ];
            return $mapped;
        }
        $operation = php_resolve_operation($context['operationIndex'], $operationId);
        if ($operation === null) {
            $context['findings'][] = [
                'code' => 'scenario.operation-unresolved',
                'symbol' => $context['scenarioId'],
                'detail' => php_bound_token($operationId),
            ];
            $mapped['unsupported'] = [
                'capability' => 'scenario.operation',
                'reason' => 'operation-unresolved',
                'detail' => php_bound_token($operationId),
            ];
            return $mapped;
        }
        $mapped['operation'] = $operation;
        $mapped['input'] = [];
        foreach ($action['input'] ?? [] as $field => $leaf) {
            $mapped['input'][] = [
                'field' => $field,
                'leaf' => $leaf,
                'leafProblem' => php_check_leaf($leaf, 0),
            ];
        }
        // Review F-9: a `when`-input leaf outside the closed typed set is
        // unsupported, never a crash.
        foreach ($mapped['input'] as $entry) {
            if ($entry['leafProblem'] !== null) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.value',
                    'reason' => $entry['leafProblem'],
                    'detail' => php_bound_token($entry['field']),
                ];
                break;
            }
        }
        if (array_key_exists('actor', $action)) {
            $mapped['ctx']['actor'] = $action['actor'];
        }
        if (array_key_exists('clock', $action)) {
            $mapped['ctx']['clock'] = $action['clock'];
        }
        if (array_key_exists('idempotencyKey', $action)) {
            $mapped['ctx']['idempotencyKey'] = $action['idempotencyKey'];
            $problem = php_check_leaf($action['idempotencyKey'], 0);
            if ($problem !== null) {
                $mapped['unsupported'] = ['capability' => 'scenario.value', 'reason' => $problem];
            }
        }
        if (!$portSurface['invoke']) {
            $mapped['unsupported'] = [
                'capability' => 'testing.fixtures',
                'reason' => 'port-surface-absent',
                'detail' => 'invoke',
            ];
        }
        if (is_array($step['replay'] ?? null)) {
            $mapped['replay'] = [
                'of' => $step['replay']['of'] ?? null,
                'expect' => $step['replay']['expect'] ?? null,
            ];
        }
        return $mapped;
    }, $when);
}

function php_map_then(array $then, array &$context, array $portSurface): array
{
    $surfaceOf = [
        'entity_state' => 'state',
        'emitted' => 'emissions',
        'forbidden_effect' => 'effects',
        'authorization' => 'authorize',
        'contract_match' => 'contractCheck',
        'deterministic_fixture' => 'fixtureDigest',
    ];
    return array_map(static function (array $step) use (&$context, $surfaceOf, $portSurface): array {
        $assertion = is_array($step['assertion'] ?? null) ? $step['assertion'] : [];
        $kind = $assertion['kind'] ?? null;
        $mapped = [
            'stepId' => $step['stepId'],
            'observes' => $step['observes'] ?? null,
            'kind' => in_array($kind, PHP_ASSERTION_KINDS, true) ? $kind : 'unknown',
            'port' => null,
            'unsupported' => null,
            'payload' => [],
        ];
        if (!in_array($kind, PHP_ASSERTION_KINDS, true)) {
            $mapped['unsupported'] = ['capability' => 'scenario.assertion', 'reason' => 'assertion-kind-unknown'];
            return $mapped;
        }
        if ($kind === 'unsupported') {
            // The explicit unsupported expectation: always a recorded
            // unsupported row carrying the capability ref — never a pass.
            $mapped['unsupported'] = [
                'capability' => $assertion['capability'] ?? 'scenario.capability',
                'reason' => 'declared-unsupported',
                'detail' => array_key_exists('note', $assertion)
                    ? php_bound_token((string) $assertion['note']) : null,
            ];
            return $mapped;
        }
        if ($kind === 'result') {
            $mapped['payload']['valueType'] = $assertion['valueType'] ?? null;
            if (array_key_exists('value', $assertion)) {
                $mapped['payload']['value'] = $assertion['value'];
                $problem = php_check_leaf($assertion['value'], 0);
                if ($problem !== null) {
                    $mapped['unsupported'] = ['capability' => 'scenario.value', 'reason' => $problem];
                }
            }
            return $mapped;
        }
        if ($kind === 'error') {
            $mapped['payload']['error'] = $assertion['error'] ?? null;
            $mapped['payload']['payload'] = [];
            foreach ($assertion['payload'] ?? [] as $field => $leaf) {
                $mapped['payload']['payload'][] = [
                    'field' => $field,
                    'leaf' => $leaf,
                    'leafProblem' => php_check_leaf($leaf, 0),
                ];
            }
            $mapped['payload']['contract'] = $assertion['contract'] ?? null;
            if ($mapped['payload']['contract'] !== null && !$portSurface['contractCheck']) {
                // The contract half needs the port contract check; the
                // typed error identity and public-field subset stay supported.
                $mapped['unsupported'] = [
                    'capability' => 'scenario.contract-check',
                    'reason' => 'port-surface-absent',
                    'detail' => 'contractCheck',
                ];
            }
            return $mapped;
        }
        if ($kind === 'idempotency') {
            // The weaker semantic-equivalence relation has no evaluator in
            // v1; an explicit unsupported row, never a proxy.
            if (($assertion['equivalence'] ?? null) === 'equivalent') {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.equivalence-equivalent',
                    'reason' => 'equivalence-unimplemented',
                    'detail' => 'equivalent',
                ];
                return $mapped;
            }
            $mapped['payload']['replay'] = $assertion['replay'] ?? null;
            $mapped['payload']['equivalence'] = $assertion['equivalence'] ?? null;
            $mapped['payload']['duplicates'] = $assertion['duplicates'] ?? null;
            return $mapped;
        }
        $surface = $surfaceOf[$kind] ?? null;
        if ($surface !== null && !$portSurface[$surface]) {
            $mapped['unsupported'] = [
                'capability' => 'testing.' . $surface,
                'reason' => 'port-surface-absent',
                'detail' => $surface,
            ];
            return $mapped;
        }
        if ($kind === 'forbidden_effect' && ($assertion['scope'] ?? null) === 'resource') {
            // No resource ledger surface exists on the closed port contract.
            $mapped['unsupported'] = [
                'capability' => 'scenario.forbidden-scope-resource',
                'reason' => 'scope-unimplemented',
                'detail' => 'resource',
            ];
            return $mapped;
        }
        if ($kind === 'entity_state') {
            // A matcher outside the closed vocabulary is unsupported,
            // never silently weakened.
            $known = ['datetime', 'uuid', 'uri', 'decimal', 'non-null'];
            $unknown = [];
            foreach ($assertion['fields'] ?? [] as $field => $expectation) {
                if (is_array($expectation) && array_key_exists('match', $expectation)
                    && !in_array($expectation['match'], $known, true)) {
                    $unknown[] = $field . ':' . $expectation['match'];
                }
            }
            if ($unknown !== []) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.match-kind',
                    'reason' => 'match-kind-unimplemented',
                    'detail' => php_bound_token(implode(',', $unknown)),
                ];
                return $mapped;
            }
        }
        $mapped['port'] = $surface ?? null;
        $mapped['payload'] = $assertion;
        unset($mapped['payload']['kind']);
        if ($kind === 'entity_state') {
            $problem = php_entity_state_leaf_problem($mapped['payload']);
            if ($problem !== null) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.value',
                    'reason' => $problem['reason'],
                    'detail' => php_bound_token($problem['field']),
                ];
            }
        }
        foreach ($mapped['payload'] as $value) {
            if (is_array($value) && !array_is_list($value)
                && (isset($value['$ref']) || isset($value['type']))) {
                $problem = php_check_leaf($value, 0);
                if ($problem !== null) {
                    $mapped['unsupported'] = ['capability' => 'scenario.value', 'reason' => $problem];
                }
            }
        }
        return $mapped;
    }, $then);
}

/** Bounded, control-cleaned detail token (no raw attacker text). */
function php_bound_token(mixed $text): string
{
    $value = (string) ($text ?? 'unknown');
    $value = preg_replace('/[^a-zA-Z0-9._:\\/-]+/', '?', $value) ?? '?';
    return substr($value, 0, 128);
}

/** The canonical AST digest input: the mapper model in canonical JSON. */
function php_ast_digest_input(array $model): string
{
    return php_canonical_json($model);
}

// ---------------------------------------------------------------------------
// The checked-binding join (issue #56, plan S3) — a structural port of
// `joinCheckedBindings` in the Node scenario compiler.
// ---------------------------------------------------------------------------

const PHP_BINDING_MISSING = 'scenario.binding-missing';
const PHP_BINDING_AMBIGUOUS = 'scenario.binding-ambiguous';
const PHP_BINDING_MISMATCH = 'scenario.binding-mismatch';

/**
 * Join every native `checked` binding against the observed index's
 * `test_bindings` records — the scan pipeline's view of which native
 * tests claim which `lekalo:<id>` scenario identities. The join is
 * pure and read-only: a missing, ambiguous, or stale binding is a
 * typed finding, never a silent pass and never a rewrite.
 *
 * `indexDocument` is the parsed observed index (`null` when absent —
 * legal absence: the join simply has nothing to say). Returns one
 * finding per violated binding, ordered by the document's binding
 * order.
 */
function php_join_checked_bindings(mixed $scenarioDocument, mixed $indexDocument): array
{
    $findings = [];
    if (!is_array($indexDocument) || !is_array($indexDocument['test_bindings'] ?? null)) {
        return $findings;
    }
    $claims = [];
    foreach ($indexDocument['test_bindings'] as $record) {
        if (!is_array($record)) {
            continue;
        }
        $ids = php_claimed_ids($record['id'] ?? null);
        if ($ids === []) {
            continue;
        }
        $claims[] = [
            'ids' => $ids,
            'symbol' => is_string($record['symbol'] ?? null) ? $record['symbol'] : null,
            'fingerprint' => is_string($record['fingerprint'] ?? null) ? $record['fingerprint'] : null,
        ];
    }
    foreach (is_array($scenarioDocument) ? ($scenarioDocument['bindings'] ?? []) : [] as $binding) {
        if (!is_array($binding)) {
            continue;
        }
        if (($binding['backend'] ?? null) !== 'native' || ($binding['mode'] ?? null) !== 'checked') {
            continue;
        }
        if (!is_string($binding['test'] ?? null)) {
            continue;
        }
        $testId = $binding['test'];
        $claiming = [];
        foreach ($claims as $claim) {
            if (in_array($testId, $claim['ids'], true)) {
                $claiming[] = $claim;
            }
        }
        if (count($claiming) === 0) {
            $findings[] = ['code' => PHP_BINDING_MISSING, 'symbol' => $testId, 'detail' => 'no-scanned-test'];
            continue;
        }
        if (count($claiming) > 1) {
            $findings[] = [
                'code' => PHP_BINDING_AMBIGUOUS,
                'symbol' => $testId,
                'detail' => 'claimed-by-' . count($claiming) . '-tests',
            ];
            continue;
        }
        // One native test file may legitimately cover several scenarios
        // (one shared fixture setup, one harness), so a record whose
        // claimed set CONTAINS the bound id joins cleanly; a declared
        // evidence digest that disagrees with the scanned fingerprint
        // means the test changed under the binding — stale evidence.
        $record = $claiming[0];
        if (is_string($binding['evidenceDigest'] ?? null) && $binding['evidenceDigest'] !== ''
            && $record['fingerprint'] !== null
            && $binding['evidenceDigest'] !== $record['fingerprint']) {
            $findings[] = [
                'code' => PHP_BINDING_MISMATCH,
                'symbol' => $testId,
                'detail' => 'stale-evidence-digest',
            ];
        }
    }
    return $findings;
}

/**
 * The claimed scenario ids of one observed test-binding id: the core
 * spells them `<test-path>#lekalo:<id>[,lekalo:<id>…]`; a bare
 * `lekalo:<id>` (no path half) still joins.
 */
function php_claimed_ids(mixed $id): array
{
    if (!is_string($id)) {
        return [];
    }
    $hash = strrpos($id, '#');
    $name = $hash === false ? $id : substr($id, $hash + 1);
    $claimed = [];
    foreach (explode(',', $name) as $part) {
        if (str_starts_with($part, 'lekalo:')) {
            $value = substr($part, strlen('lekalo:'));
            if ($value !== '') {
                $claimed[] = $value;
            }
        }
    }
    return $claimed;
}
