<?php

/**
 * Deterministic PHP emitter for the scenario-test compiler (issue #56,
 * S2). A structural port of the Node emitter
 * (`adapters/node-typescript/src/scenario-emit.mjs`): input is the pure
 * test model of `scenario-map.php`; output is one strict PHP test file
 * per scenario plus the shared `ScenarioTestKit.php`, the run-record
 * reporter `ScenarioReporter.php`, and the port shim `Port.php`, each
 * test paired with one canonical `.test.map.json` sidecar.
 *
 * Byte stability is the contract: a fixed header comment (adapter id /
 * version, contract identities, the `sha256:` digest of the exact
 * scenario document bytes — no timestamps, no host paths, no host
 * data), deterministic scenario/step ordering, 4-space indent, LF
 * endings, no trailing whitespace, exactly one final newline. String
 * literals are emitted via `php_emit_value`, whose escaping is
 * `var_export`-free single-quote encoding, so no interpolation or code
 * injection survives into a generated test.
 *
 * Evidence honesty mirrors the Node emitter: every assertion block
 * records exactly one outcome row (`pass | fail | unsupported |
 * infrastructure | degraded`), unsupported rows can never become
 * passes, and the reporter persists the rows into the durable run
 * record.
 */

const PHP_EMITTER_ADAPTER_ID = 'lekalo-target-php-laravel';

/** The sidecar micro-contract token of the scenario test maps. */
const PHP_MAP_CONTRACT = 'lekalo/scenario-test-map/v0.4.0';

/** The runner version reported when a scenario declares no explicit pin. */
const PHP_RUNNER_VERSION = 'bundled-toolchain';

/** The run-record contract family the reporter writes. */
const PHP_RUN_RECORD_SCHEMA_VERSION = 'lekalo/scenario-run/v0.4.0';
const PHP_RUN_RECORD_IDENTITY = 'dev.lekalo.scenario-run@0.4.0';

/** The run-record ingest home (an adjudicated `.lekalo/import` home). */
const PHP_RUN_RECORD_DIR = '.lekalo/import/scenario-runs';

/**
 * The toolchain custody record (issue #56, plan S1): one durable
 * document per suite run recording the OBSERVED runtime facts — PHP
 * version, the resolved Laratesto/Testo/Laravel package versions, and
 * the exact `composer.lock` digest — separate from the closed
 * run-record shape so custody never overloads the single `runner`
 * field.
 */
const PHP_TOOLCHAIN_DIR = '.lekalo/import/toolchain';
const PHP_TOOLCHAIN_SCHEMA_VERSION = 'lekalo/scenario-toolchain/v0.1.0';
const PHP_TOOLCHAIN_IDENTITY = 'dev.lekalo.scenario-toolchain@0.1.0';

/**
 * The conformance-relevant Composer packages the custody record
 * probes. The list is fixed emission data — never derived from the
 * project's composer.json at compile time, and 'not-installed' is an
 * honest row, never an omission.
 */
const PHP_TOOLCHAIN_PACKAGES = ['ichinya/laratesto', 'laravel/framework', 'testo/testo'];

/**
 * The user-owned scaffold home (issue #56, plan S3): a `scaffolded`
 * binding emits its test here exactly once — every later generation
 * keeps the user's bytes — so it must sit outside the managed
 * generated home. The segments stay lowercase because the write plan
 * travels the logical-path grammar (uppercase is wire-illegal).
 */
const PHP_SCAFFOLD_TESTS_DIR = 'tests/lekalo/scenario-tests';

/** Reserved emitted module names; a scenario module may never collide. */
const PHP_RESERVED_MODULES = ['testkit', 'port', 'reporter', 'ScenarioTestKit', 'Port', 'ScenarioReporter'];

/** The generated support files shared by every scenario test. */
const PHP_SUPPORT_FILES = [
    'scenario-test-kit.php',
    'scenario-reporter.php',
    'port.php',
];

/**
 * The semantic native-test id of one scenario on one target: the target
 * token is part of the identity, so a Node test and a Laravel test for
 * the same scenario never collide into one trace node.
 */
function php_native_test_id(string $scenarioId): string
{
    return 'php-laravel:' . $scenarioId;
}

/** The stable PHP FQN of one scenario's generated test class. */
function php_test_class_fqn(string $scenarioId): string
{
    return 'Lekalo\\Generated\\ScenarioTests\\' . php_module_of($scenarioId) . '\\'
        . php_class_of($scenarioId);
}

/**
 * The emission module of one scenario: the id prefix before the first
 * dot (the module namespace segment).
 */
function php_module_of(string $scenarioId): string
{
    $cut = strpos($scenarioId, '.');
    return $cut === false || $cut === 0 ? $scenarioId : substr($scenarioId, 0, $cut);
}

/** The PSR-4-safe class identifier of one scenario id: always ends
 * with `Test` (the case-suffix the naming convention locates) and the
 * emitted file spelling `<id>.test.php` ends with the file suffix
 * `Test.php` is checked against — the class name is what matters, so
 * the generated class carries the suffix.
 */
function php_class_of(string $scenarioId): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9]/', '_', $scenarioId);
    $parts = array_map(static fn (string $part): string => ucfirst($part), explode('_', $sanitized));
    // The Laratesto naming convention discovers `*Test.php` files whose
    // class name also ends in `Test`, so the suffix is part of the
    // stable class mapping.
    return implode('', $parts) . 'Test';
}

/** The safe PHP identifier of one scenario or step id (snake_case use). */
function php_identifier_of(string $id): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9_]/', '_', $id);
    // Extension-free leading-digit probe: strspn ships with ext/standard,
    // so the check survives `php -n` runtimes where ctype is absent.
    return strspn($sanitized, '0123456789') > 0 ? '_' . $sanitized : $sanitized;
}

/**
 * One comment-safe single-line projection of free wire text: every line
 * terminator and control character collapses, so a core-valid `summary`
 * can never close a generated comment and inject live code into the
 * emitted test.
 */
function php_comment_safe(mixed $text): string
{
    $value = (string) ($text ?? '');
    $value = preg_replace('/\r\n|[\r\n\x{0085}\x{2028}\x{2029}]|\p{Cc}/u', ' ', $value) ?? '';
    $value = preg_replace('/\s+/', ' ', $value) ?? '';
    $value = trim($value);
    // Byte-exact truncation: the scenario byte-range maps account bytes,
    // so the cap must count bytes (mb_* would count characters and
    // desynchronize the map); it also keeps php -n runtimes safe.
    return substr($value, 0, 200);
}

/**
 * Emit every generated file of one mapped scenario document.
 *
 * `input` is `{models, inputDigest, adapterVersion, portModulePath,
 * startedBy}`. Returns sorted `{path, text, map}` records; `map` is
 * non-null only on sidecars. A checked binding emits nothing for its
 * scenario (review F-4: the checked identity belongs exclusively to the
 * existing native test).
 */
function php_emit_scenario_tests(array $input): array
{
    $context = [
        'inputDigest' => $input['inputDigest'],
        'adapterVersion' => $input['adapterVersion'],
        'portModulePath' => $input['portModulePath'],
        'portClass' => $input['portClass'] ?? '',
        'startedBy' => $input['startedBy'] ?? 'lekalo-scenario-harness',
    ];
    if ($context['portClass'] === '' && $context['portModulePath'] !== '') {
        // A declared port always carries both its logical path and its
        // class; only the declaration-absent compile (every feature an
        // explicit unsupported row, the shim never invoked) emits with
        // an empty class binding.
        throw new LogicException('scenario emit without a validated port class');
    }
    $files = [
        php_file(PHP_SCENARIO_TESTS_DIR . '/scenario-test-kit.php', php_testkit_text($context), null),
        php_file(PHP_SCENARIO_TESTS_DIR . '/scenario-reporter.php', php_reporter_text($context), null),
        php_file(PHP_SCENARIO_TESTS_DIR . '/port.php', php_port_text($context), null),
    ];
    $models = $input['models'];
    usort($models, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));
    foreach ($models as $model) {
        $mode = $model['binding']['mode'];
        if ($mode === 'checked') {
            // A checked binding declares that an EXISTING native test
            // owns the scenario identity: nothing is generated for it.
            continue;
        }
        $module = php_module_of($model['id']);
        if (in_array($module, PHP_RESERVED_MODULES, true)) {
            throw new LogicException('scenario module collides with a reserved emitted file: ' . $module);
        }
        $scaffolded = $mode === 'scaffolded';
        // A scaffolded test is user-owned: it is emitted once into the
        // scaffold home (`frozen` — the kernel plans its write only
        // when absent) and its map sidecar travels beside it as the
        // scaffold marker the custody rules key on.
        $dir = ($scaffolded ? PHP_SCAFFOLD_TESTS_DIR : PHP_SCENARIO_TESTS_DIR) . '/' . $module;
        $testFile = php_emit_test($model, $context, $scaffolded);
        $mapPath = $dir . '/' . $model['id'] . '.test.map.json';
        $test = php_file($dir . '/' . $model['id'] . '.test.php', $testFile['text'], null);
        if ($scaffolded) {
            $test['frozen'] = true;
            $test['marker'] = $mapPath;
        }
        $files[] = $test;
        $files[] = php_file($mapPath, php_canonical_json($testFile['map']) . "\n", $testFile['map']);
    }
    usort($files, static fn (array $left, array $right): int => strcmp($left['path'], $right['path']));
    return $files;
}

function php_file(string $path, string $text, ?array $map): array
{
    return ['path' => $path, 'text' => $text, 'map' => $map];
}

// ---------------------------------------------------------------------------
// Shared emitted support files.
// ---------------------------------------------------------------------------

function php_doc_header(array $context): string
{
    // The open tag leads every emitted file: without it PHP would parse
    // the whole file as inline HTML and the class would never exist.
    return "<?php\n\n// Generated by " . PHP_EMITTER_ADAPTER_ID . '@' . $context['adapterVersion']
        . ' (scenario-test-compiler, issue #56).' . "\n"
        . '// From ' . PHP_SCENARIO_IDENTITY . ' input ' . $context['inputDigest'] . '.'
        . ' Do not edit: regenerate with `lekalo generate`.';
}

function php_testkit_text(array $context): string
{
    $header = php_doc_header($context);
    return <<<PHP
$header
// The shared runner-neutral helpers; content depends only on the
// adapter version, so this file is itself a determinism probe.
// Generated file — do not edit.

declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests;

use Testo\\Assert;

/**
 * Canonical typed equality over the closed Scenario IR value domain:
 * dates, datetimes, uuids, uris, and decimals compare exactly as their
 * canonical strings; integers compare numerically; objects compare
 * field-by-field in any key order.
 */
final class ScenarioTestKit
{
    /**
     * One bounded, control-cleaned failure detail: the run record
     * carries no absolute paths, no host data, and never more than one
     * short line.
     */
    public static function boundedDetail(mixed \$value): string
    {
        \$text = \$value === null ? 'unknown' : (string) \$value;
        \$text = preg_replace('/[^a-zA-Z0-9._:\\/() -]+/', '?', \$text) ?? '?';
        \$text = trim(\$text);
        return substr(\$text, 0, 200);
    }

    /** Canonical typed equality (see class docblock). */
    public static function typedEqual(mixed \$actual, mixed \$expected): bool
    {
        if (\$actual === \$expected) {
            return true;
        }
        if (is_object(\$actual) || is_object(\$expected)) {
            if (!is_object(\$actual) || !is_object(\$expected)) {
                return false;
            }
            if (get_class(\$actual) !== get_class(\$expected)) {
                return false;
            }
            return self::typedEqual((array) \$actual, (array) \$expected);
        }
        if (is_array(\$actual) || is_array(\$expected)) {
            if (!is_array(\$actual) || !is_array(\$expected)) {
                return false;
            }
            if (array_is_list(\$actual) !== array_is_list(\$expected)) {
                return false;
            }
            if (array_is_list(\$actual)) {
                if (count(\$actual) !== count(\$expected)) {
                    return false;
                }
                foreach (\$actual as \$index => \$item) {
                    if (!self::typedEqual(\$item, \$expected[\$index])) {
                        return false;
                    }
                }
                return true;
            }
            \$leftKeys = array_keys(\$actual);
            sort(\$leftKeys, SORT_STRING);
            \$rightKeys = array_keys(\$expected);
            sort(\$rightKeys, SORT_STRING);
            if (\$leftKeys !== \$rightKeys) {
                return false;
            }
            foreach (\$leftKeys as \$key) {
                if (!self::typedEqual(\$actual[\$key], \$expected[\$key])) {
                    return false;
                }
            }
            return true;
        }
        return false;
    }

    /**
     * The public subset of one error object: the id plus the declared
     * payload fields only — a generated test never asserts private
     * error internals.
     */
    public static function errorFieldsMatch(mixed \$error, array \$fields): bool
    {
        if (!is_array(\$error)) {
            return false;
        }
        foreach (\$fields as \$key => \$expected) {
            \$carried = \$error['fields'][\$key] ?? null;
            if (!self::typedEqual(\$carried, \$expected)) {
                return false;
            }
        }
        return true;
    }

    // -----------------------------------------------------------------
    // Canonical-form matchers: exact ports of the core grammar
    // functions in scenario/value.rs. A stored state field that
    // violates the canonical contract must fail the generated matcher —
    // over-accepting approximations are false passes.
    // -----------------------------------------------------------------

    /** Real Gregorian month lengths, leap years included (value.rs). */
    private static function daysInMonth(int \$year, int \$month): int
    {
        if (in_array(\$month, [1, 3, 5, 7, 8, 10, 12], true)) {
            return 31;
        }
        if (in_array(\$month, [4, 6, 9, 11], true)) {
            return 30;
        }
        return ((\$year % 4 === 0 && \$year % 100 !== 0) || \$year % 400 === 0) ? 29 : 28;
    }

    /** The canonical calendar date with real month and day values. */
    private static function canonicalDate(string \$text): bool
    {
        if (preg_match('/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/', \$text) !== 1) {
            return false;
        }
        \$year = (int) substr(\$text, 0, 4);
        \$month = (int) substr(\$text, 5, 2);
        \$day = (int) substr(\$text, 8, 2);
        if (\$year < 1 || \$year > 9999 || \$month < 1 || \$month > 12) {
            return false;
        }
        return \$day >= 1 && \$day <= self::daysInMonth(\$year, \$month);
    }

    /** The canonical decimal spelling (value.rs canonical_decimal). */
    public static function canonicalDecimal(mixed \$text): bool
    {
        \$value = (string) \$text;
        if (\$value === '-0') {
            return false;
        }
        \$negative = str_starts_with(\$value, '-');
        \$rest = \$negative ? substr(\$value, 1) : \$value;
        \$dot = strpos(\$rest, '.');
        \$integral = \$dot === false ? \$rest : substr(\$rest, 0, \$dot);
        \$fractional = \$dot === false ? null : substr(\$rest, \$dot + 1);
        if (preg_match('/^[0-9]+$/', \$integral) !== 1) {
            return false;
        }
        if (strlen(\$integral) > 1 && str_starts_with(\$integral, '0')) {
            return false;
        }
        if (\$integral === '0' && \$negative) {
            return false;
        }
        if (\$fractional === null) {
            return true;
        }
        return preg_match('/^[0-9]+$/', \$fractional) === 1 && !str_ends_with(\$fractional, '0');
    }

    /** The canonical UTC datetime (value.rs canonical_datetime). */
    public static function canonicalDatetime(mixed \$text): bool
    {
        \$value = (string) \$text;
        if (strlen(\$value) < 20 || !str_ends_with(\$value, 'Z')) {
            return false;
        }
        if (!self::canonicalDate(substr(\$value, 0, 10))) {
            return false;
        }
        if (\$value[10] !== 'T') {
            return false;
        }
        \$time = substr(\$value, 11, strlen(\$value) - 12);
        \$dot = strpos(\$time, '.');
        \$clock = \$dot === false ? \$time : substr(\$time, 0, \$dot);
        \$fraction = \$dot === false ? null : substr(\$time, \$dot + 1);
        \$parts = explode(':', \$clock);
        if (count(\$parts) !== 3) {
            return false;
        }
        foreach (\$parts as \$part) {
            if (strlen(\$part) !== 2 || preg_match('/^[0-9]+$/', \$part) !== 1) {
                return false;
            }
        }
        \$hour = (int) \$parts[0];
        \$minute = (int) \$parts[1];
        \$second = (int) \$parts[2];
        if (\$hour > 23 || \$minute > 59 || \$second > 59) {
            return false;
        }
        if (\$fraction === null) {
            return true;
        }
        \$length = strlen(\$fraction);
        return \$length >= 1 && \$length <= 9 && preg_match('/^[0-9]+$/', \$fraction) === 1;
    }

    /** The canonical URI (value.rs canonical_uri). */
    public static function canonicalUri(mixed \$text): bool
    {
        \$value = (string) \$text;
        \$bytes = strlen(\$value);
        if (\$bytes < 8 || \$bytes > 2048) {
            return false;
        }
        \$marker = strpos(\$value, '://');
        if (\$marker === false) {
            return false;
        }
        \$scheme = substr(\$value, 0, \$marker);
        \$rest = substr(\$value, \$marker + 3);
        if (\$scheme === '' || !preg_match('/^[a-z]/', \$scheme)) {
            return false;
        }
        if (preg_match('/^[a-z0-9+.-]*$/', substr(\$scheme, 1)) !== 1) {
            return false;
        }
        if (\$rest === '') {
            return false;
        }
        if (preg_match('/[<>"{}|\\\\^` ]|[\\x00-\\x1f\\x7f-\\x9f]/', \$value) === 1) {
            return false;
        }
        \$authorityMatch = strpbrk(\$rest, '/?#');
        \$authorityEnd = \$authorityMatch === false ? strlen(\$rest) : strpos(\$rest, \$authorityMatch[0]);
        return strpos(substr(\$rest, 0, \$authorityEnd), '@') === false;
    }
}

PHP;
}

function php_reporter_text(array $context): string
{
    $header = php_doc_header($context);
    $schemaVersion = PHP_RUN_RECORD_SCHEMA_VERSION;
    $identity = PHP_RUN_RECORD_IDENTITY;
    $ingestDir = PHP_RUN_RECORD_DIR;
    $toolchainDir = PHP_TOOLCHAIN_DIR;
    $toolchainSchema = PHP_TOOLCHAIN_SCHEMA_VERSION;
    $toolchainIdentity = PHP_TOOLCHAIN_IDENTITY;
    $adapterId = PHP_EMITTER_ADAPTER_ID;
    $adapterVersion = ADAPTER_VERSION;
    $probedPackages = implode(', ', array_map(
        static fn (string $package): string => "'" . $package . "'",
        PHP_TOOLCHAIN_PACKAGES,
    ));
    return <<<PHP
$header
// The durable run-record writer: one $schemaVersion document per
// scenario run, written into the adjudicated ingest home $ingestDir/,
// plus one toolchain custody record in $toolchainDir/ recording the
// observed runtime facts (PHP, the resolved Laratesto/Testo/Laravel
// versions, the composer.lock digest) — the closed run-record shape
// never carries toolchain data.
// Generated file — do not edit.

declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests;

/**
 * One run recorder for one scenario test. Records exactly one bounded
 * outcome row per executed assertion and flushes the closed
// run-record document into the ingest home. The test fingerprint is
 * computed over the exact bytes of the importing test file at flush
 * time; the project root is derived from this file's own fixed
 * location under the generated root, never from the cwd.
 */
final class ScenarioReporter
{
    private const GENERATED_ROOT_DEPTH = 4;

    private array \$assertions = [];

    /** @param array<string, mixed> \$spec */
    public function __construct(
        private readonly array \$spec,
        private readonly string \$testFile,
    ) {}

    /**
     * Record exactly one assertion outcome row. Outcomes are closed:
     * pass | fail | unsupported | infrastructure | degraded. An
     * unsupported row can never become a pass.
     */
    public function record(array \$row): void
    {
        \$outcome = (string) \$row['outcome'];
        if (!in_array(\$outcome, ['pass', 'fail', 'unsupported', 'infrastructure', 'degraded'], true)) {
            throw new LogicException('closed outcome vocabulary violation: ' . \$outcome);
        }
        \$entry = [
            'step_id' => \$row['step_id'] === null ? null : (string) \$row['step_id'],
            'observes' => (\$row['observes'] ?? null) === null ? null : (string) \$row['observes'],
            'kind' => (string) \$row['kind'],
            'outcome' => \$outcome,
        ];
        if (isset(\$row['detail']) && \$row['detail'] !== null) {
            \$entry['detail'] = substr((string) \$row['detail'], 0, 200);
        }
        \$this->assertions[] = \$entry;
    }

    /** Whether any row is unsupported (such a run is never a pass). */
    public function hasUnsupported(): bool
    {
        foreach (\$this->assertions as \$row) {
            if (\$row['outcome'] === 'unsupported') {
                return true;
            }
        }
        return false;
    }

    /** Whether any row failed on an assertion or on infrastructure. */
    public function hasBlockingFailure(): bool
    {
        foreach (\$this->assertions as \$row) {
            if (\$row['outcome'] === 'fail' || \$row['outcome'] === 'infrastructure') {
                return true;
            }
        }
        return false;
    }

    /**
     * Persist the canonical run record into the ingest home; returns
     * its project-relative path.
     */
    public function flush(): string
    {
        \$scenario = \$this->spec['scenario'];
        \$runner = \$this->spec['runner'];
        \$test = \$this->spec['test'];
        \$document = [
            'schema_version' => '$schemaVersion',
            'identity' => '$identity',
            'scenario' => [
                'id' => (string) \$scenario['id'],
                'version' => (string) \$scenario['version'],
                'ir_digest' => (string) \$scenario['irDigest'],
                'symbols' => is_array(\$scenario['symbols'] ?? null) ? \$scenario['symbols'] : [],
                'operations' => is_array(\$scenario['operations'] ?? null) ? \$scenario['operations'] : [],
            ],
            'runner' => [
                'id' => (string) \$runner['id'],
                'version' => (string) \$runner['version'],
            ],
            'profile' => null,
            'test' => [
                'id' => (string) \$test['id'],
                'path' => \$this->relativeTestPath(),
                'fingerprint' => 'sha256:' . hash_file('sha256', \$this->testFile),
            ],
            'binding_mode' => (string) \$this->spec['bindingMode'],
            'started_by' => (string) \$this->spec['startedBy'],
            'assertions' => \$this->assertions,
        ];
        \$root = \$this->projectRoot();
        \$target = \$root . '/' . '$ingestDir' . '/' . \$scenario['id'] . '.json';
        \$dir = dirname(\$target);
        if (!is_dir(\$dir)) {
            mkdir(\$dir, 0777, true);
        }
        file_put_contents(\$target, self::canonicalJson(\$document) . "\\n");
        self::writeToolchainCustody(\$root, \$runner);
        return '$ingestDir' . '/' . \$scenario['id'] . '.json';
    }

    /**
     * Persist the toolchain custody record: the OBSERVED runtime facts
     * of this suite run — the PHP version, the resolved package
     * versions of the conformance stack, and the exact digest of the
     * project's composer.lock. One deterministic document per suite
     * run; an absent package records 'not-installed', an absent lock a
     * null digest — honest absence, never an omission.
     */
    private static function writeToolchainCustody(string \$root, array \$runner): void
    {
        \$packages = [];
        foreach ([$probedPackages] as \$package) {
            \$version = null;
            if (class_exists(\\Composer\\InstalledVersions::class)
                && \\Composer\\InstalledVersions::isInstalled(\$package)) {
                \$version = \\Composer\\InstalledVersions::getPrettyVersion(\$package);
            }
            \$packages[\$package] = is_string(\$version) ? \$version : 'not-installed';
        }
        \$lockPath = \$root . '/composer.lock';
        \$document = [
            'schema_version' => '$toolchainSchema',
            'identity' => '$toolchainIdentity',
            'adapter' => ['id' => '$adapterId', 'version' => '$adapterVersion'],
            'runner' => [
                'id' => (string) \$runner['id'],
                'version' => (string) \$runner['version'],
            ],
            'toolchain' => [
                'php' => PHP_VERSION,
                'composer_lock' => is_file(\$lockPath) ? 'sha256:' . hash_file('sha256', \$lockPath) : null,
                'packages' => \$packages,
            ],
        ];
        \$dir = \$root . '/' . '$toolchainDir';
        if (!is_dir(\$dir)) {
            mkdir(\$dir, 0777, true);
        }
        file_put_contents(\$dir . '/php-laravel.json', self::canonicalJson(\$document) . "\\n");
    }

    /**
     * Canonical JSON over the closed run-record domain: sorted object
     * keys, no whitespace, no escaped slashes, non-ASCII kept literal.
     * The self-contained mirror of the kernel encoder — the emitted
     * reporter never runs inside the adapter process.
     */
    private static function canonicalJson(mixed \$value): string
    {
        if (\$value === null) {
            return 'null';
        }
        if (is_bool(\$value)) {
            return \$value ? 'true' : 'false';
        }
        if (is_int(\$value)) {
            return (string) \$value;
        }
        if (is_string(\$value)) {
            return self::canonicalString(\$value);
        }
        if (!is_array(\$value)) {
            throw new LogicException('run-record value outside the closed canonical domain');
        }
        if (array_is_list(\$value)) {
            return '[' . implode(',', array_map([self::class, 'canonicalJson'], \$value)) . ']';
        }
        \$keys = array_keys(\$value);
        usort(\$keys, 'strcmp');
        \$body = [];
        foreach (\$keys as \$key) {
            \$body[] = self::canonicalString((string) \$key)
                . ':' . self::canonicalJson(\$value[\$key]);
        }
        return '{' . implode(',', \$body) . '}';
    }

    /** The canonical JSON string spelling of one bounded UTF-8 value. */
    private static function canonicalString(string \$value): string
    {
        \$sentinel = str_replace("\\x7F", "\\x00", \$value);
        \$encoded = json_encode(
            \$sentinel,
            JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
        );
        return str_replace('\\u0000', "\\x7F", \$encoded);
    }

    /** The project-relative logical path of the importing test file. */
    private function relativeTestPath(): string
    {
        \$root = \$this->projectRoot();
        \$relative = str_replace('\\\\', '/', substr(\$this->testFile, strlen(\$root) + 1));
        return \$relative;
    }

    /**
     * The project root derived from this file's fixed emitted location —
     * never from the current working directory, never from host config.
     */
    private function projectRoot(): string
    {
        \$here = dirname(__FILE__);
        \$root = \$here;
        for (\$index = 0; \$index < self::GENERATED_ROOT_DEPTH; \$index += 1) {
            \$root = dirname(\$root);
        }
        return \$root;
    }
}

PHP;
}

function php_port_text(array $context): string
{
    $header = php_doc_header($context);
    // The FQN travels through the closed string escaper: the validated
    // grammar admits only identifiers and namespace separators, and the
    // escaping keeps even a hostile value from breaking the constant.
    $portClass = php_emit_value($context['portClass']);
    return <<<PHP
$header
// The project test-port binding shim; content depends only on the
// adapter version and the declared port document, so this file is
// itself a determinism probe. Generated file — do not edit.

declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests;

/**
 * Forwarding shim to the project-declared ScenarioPort implementation.
 * The emitted `PORT_CLASS` constant is completed at generation time
 * with the exact project port class name from the validated
 * `lekalo/php-test-port.json` declaration, so the shim itself stays
 * deterministic given the same inputs.
 */
final class Port
{
    private const GENERATED_ROOT_DEPTH = 4;
    private const PORT_CLASS = $portClass;

    /** @var array<string, object> resolved instances, keyed by class */
    private static array \$instances = [];

    /** The resolved project port instance (one per test process). */
    public static function instance(): object
    {
        \$class = self::PORT_CLASS;
        if (isset(self::\$instances[\$class])) {
            return self::\$instances[\$class];
        }
        if (!class_exists(\$class)) {
            throw new LogicException('the declared ScenarioPort class does not exist: ' . \$class);
        }
        return self::\$instances[\$class] = new \$class();
    }

    public static function reset(): void
    {
        self::\$instances = [];
    }
}

PHP;
}

// ---------------------------------------------------------------------------
// Per-scenario test rendering.
// ---------------------------------------------------------------------------

function php_emit_test(array $model, array $context, bool $scaffolded = false): array
{
    $segments = [];
    $cursor = 0;
    $push = static function (string $text, ?string $stepId = null) use (&$segments, &$cursor): void {
        $segments[] = ['text' => $text, 'start' => $cursor, 'stepId' => $stepId];
        $cursor += strlen($text);
    };
    $runnerVersion = $model['runner']['declaredVersion'] ?? null;
    $classFqn = php_test_class_fqn($model['id']);
    $scenarioId = $model['id'];
    $header = php_doc_header($context);
    // The sibling support files travel with every emission (the PHP
    // mirror of the Node emitter's relative import block): the emitted
    // test is self-contained and never depends on project autoload
    // configuration for the generated namespace. A scaffolded test sits
    // in the user-owned scaffold home, four segments below the project
    // root, so its requires walk back to the managed support home.
    $requires = ($scaffolded
        ? "// The support files live in the managed generated home; this file is user-owned.\n"
            . "require_once dirname(__DIR__, 4) . '/src/generated/php-laravel/scenario-tests/scenario-test-kit.php';\n"
            . "require_once dirname(__DIR__, 4) . '/src/generated/php-laravel/scenario-tests/scenario-reporter.php';\n"
            . "require_once dirname(__DIR__, 4) . '/src/generated/php-laravel/scenario-tests/port.php';"
        : "// The sibling support files travel with every generation (the PHP\n"
            . "// mirror of the Node emitter's relative import block): the emitted\n"
            . "// test is self-contained and never depends on project autoload\n"
            . "// configuration for the generated namespace.\n"
            . "require_once __DIR__ . '/../scenario-test-kit.php';\n"
            . "require_once __DIR__ . '/../scenario-reporter.php';\n"
            . "require_once __DIR__ . '/../port.php';");
    // The scaffolded file is user-owned after its one emission: it
    // carries the `lekalo:<id>` claim marker the observed index scans
    // for, plus an explicit edit-freedom note, so the scaffold never
    // masquerades as managed content.
    $marker = $scaffolded
        ? "// lekalo:{$scenarioId} — scaffolded once; edit freely, regeneration never overwrites this file.\n"
        : '';
    $heredoc = <<<PHP
$header
//
// Scenario {$scenarioId} @{$model['version']}: {$model['summary']}
// Runner {$model['runner']['id']}; binding {$model['binding']['mode']}; native test id
// php-laravel:{$scenarioId}; generated by the scenario-test compiler (issue #56).
{$marker}
declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests\\{$model['projectId']};

use Lekalo\\Generated\\ScenarioTests\\Port;
use Lekalo\\Generated\\ScenarioTests\\ScenarioReporter;
use Lekalo\\Generated\\ScenarioTests\\ScenarioTestKit;
use Laratesto\\Attribute\\DatabaseMigrations;
use Testo\\Assert;
use Testo\Test;

{$requires}

PHP;
    $push($heredoc, null);
    $blockStart = $cursor;
    // The class carries the `#[Test]` attribute: the canonical
    // attribute-driven discovery of the pinned Testo version. The file
    // spelling stays lowercase (the logical-path grammar forbids
    // uppercase segments) and never relies on the case-suffix
    // convention.
    $push("#[DatabaseMigrations]
#[Test]
" . 'final class ' . php_class_of($scenarioId) . "
{
", null);
    $body = php_render_body($model);
    foreach ($body['segments'] as $segment) {
        // One segment per then-step (the Node emitter's layout): each
        // keeps its step id so the sidecar can declare exact byte
        // ranges per assertion step, never only the whole-class span.
        $push($segment['text'], $segment['stepId']);
    }
    $push("}\n", null);
    $text = implode('', array_map(static fn (array $segment): string => $segment['text'], $segments));
    $leaf = php_scenario_leaf($model['id']);
    $map = [
        'contract' => PHP_MAP_CONTRACT,
        'adapter' => ['id' => PHP_EMITTER_ADAPTER_ID, 'version' => $context['adapterVersion']],
        'owner' => $model['id'],
        'fields' => ['' => $model['id']],
        'declarations' => [
            [
                'id' => $model['id'],
                'kind' => 'scenario',
                'export' => $classFqn,
                'start' => $blockStart,
                'end' => strlen($text),
            ],
            // The scenario leaf scoped under each then-step id is a
            // grammar-valid two-segment Model symbol, unique within the
            // sidecar; the kind and the full step spelling ride beside
            // it, exactly like the Node emitter's map records.
            ...array_map(
                static fn (array $segment): array => [
                    'id' => $leaf . '.' . $segment['stepId'],
                    'kind' => 'then',
                    'step' => $segment['stepId'],
                    'export' => $classFqn,
                    'start' => $segment['start'],
                    'end' => $segment['start'] + strlen($segment['text']),
                ],
                array_values(array_filter(
                    $segments,
                    static fn (array $segment): bool => $segment['stepId'] !== null,
                )),
            ),
        ],
    ];
    return ['text' => $text, 'map' => $map];
}

/** The leaf of one scenario id: the last dot-separated segment. */
function php_scenario_leaf(string $scenarioId): string
{
    $cut = strrpos($scenarioId, '.');
    return $cut === false ? $scenarioId : substr($scenarioId, $cut + 1);
}

/**
 * The per-scenario test body: ONE public `test*` method per scenario
 * (the Laratesto naming convention discovers `*Test.php` classes with
 * `test*` methods). The recorder wraps every recorded row group,
 * unsupported rows short-circuit into a `Skipped` throw (never a
 * pass), and any non-assertion throwable rethrows after flushing
 * (Testo reports it as `Error`, which normalizes to infrastructure
 * evidence).
 *
 * Returns the method as tagged `{text, stepId}` segments: one segment
 * per then-step so the caller's byte ranges can declare per-step
 * ownership, exactly like the Node emitter's segment list.
 */
function php_render_body(array $model): array
{
    $groups = php_render_groups($model);
    $head = '    public function test' . ucfirst(php_identifier_of($model['id'])) . "(): void\n    {\n";
    $head .= "        \$recorder = new ScenarioReporter([\n";
    $head .= "            'scenario' => ['id' => " . php_emit_value($model['id']) . ", 'version' => "
        . php_emit_value($model['version']) . ", 'irDigest' => " . php_emit_value($model['irDigest'] ?? null)
        . ", 'symbols' => [], 'operations' => " . php_emit_value(php_operations_of($model)) . "],\n";
    $head .= "            'runner' => ['id' => " . php_emit_value($model['runner']['id']) . ", 'version' => "
        . php_emit_value($model['runner']['declaredVersion'] ?? PHP_RUNNER_VERSION) . "],\n";
    $head .= "            'test' => ['id' => " . php_emit_value(php_native_test_id($model['id'])) . "],\n";
    $head .= "            'bindingMode' => " . php_emit_value($model['binding']['mode']) . ",\n";
    $head .= "            'startedBy' => 'lekalo-scenario-harness',\n";
    $head .= "        ], __FILE__);\n";
    $head .= "        try {\n";
    // The PHP mirror of the Node emitter's `await resetPort()`: the shim
    // drops its memoized instances, so every test constructs a fresh
    // project port and inherits no in-memory state from a previous test
    // in the same process (the DB lifecycle is the migrations
    // attribute's contract).
    $head .= "            Port::reset();\n";
    $segments = [['text' => $head, 'stepId' => null]];
    foreach ($groups as $group) {
        $text = '';
        foreach ($group['lines'] as $line) {
            $text .= $line . "\n";
        }
        $segments[] = ['text' => $text, 'stepId' => $group['stepId']];
    }
    $tail = "            if (\$recorder->hasUnsupported()) {\n";
    $tail .= "                // Unsupported rows never become passes: skip the test and\n";
    $tail .= "                // let the flushed record carry the exact rows.\n";
    $tail .= "                \$recorder->flush();\n";
    $tail .= "                throw new \\Testo\\Core\\Exception\\SkipTest('scenario.unsupported-capability');\n";
    $tail .= "            }\n";
    $tail .= "            \$recorder->flush();\n";
    $tail .= "        } catch (\\Throwable \$_error) {\n";
    $tail .= "            \$recorder->flush();\n";
    $tail .= "            throw \$_error;\n";
    $tail .= "        }\n";
    $tail .= "    }\n";
    $segments[] = ['text' => $tail, 'stepId' => null];
    return ['segments' => $segments];
}

/** The sorted distinct operation ids of one model's when steps. */
function php_operations_of(array $model): array
{
    $operations = [];
    foreach ($model['when'] as $step) {
        $id = $step['operation']['id'] ?? null;
        if (is_string($id) && !in_array($id, $operations, true)) {
            $operations[] = $id;
        }
    }
    sort($operations, SORT_STRING);
    return $operations;
}

/** The body row groups: given, when, then, in scenario order. */
function php_render_groups(array $model): array
{
    $groups = [];
    $wholeScenarioUnsupported = $model['unsupported'] !== [];
    $stepVars = [];
    $clockIsos = [];
    if ($wholeScenarioUnsupported) {
        // Concurrency race cases and binding-capability gaps: record the
        // declared rows and execute nothing (a serial run would lie).
        $lines = [];
        foreach ($model['unsupported'] as $entry) {
            $lines[] = php_unsupported_row(null, null, 'scenario', $entry['capability'] . ': ' . $entry['reason']);
        }
        foreach ($model['then'] as $step) {
            $lines[] = php_unsupported_row($step['stepId'], $step['observes'], $step['kind'], 'scenario-unsupported');
        }
        $groups[] = ['lines' => $lines, 'stepId' => null];
        return $groups;
    }
    foreach ($model['given'] as $step) {
        if ($step['unsupported'] !== null) {
            $groups[] = [
                'lines' => [php_unsupported_row($step['stepId'], null, 'given:' . $step['kind'],
                    $step['unsupported']['capability'] . ': ' . $step['unsupported']['reason'])],
                'stepId' => null,
            ];
            continue;
        }
        $groups[] = [
            'lines' => array_merge(
                ['    // given ' . $step['stepId'] . ' (' . $step['kind'] . ')'],
                php_render_given($step, $stepVars, $clockIsos),
            ),
            'stepId' => null,
        ];
    }
    foreach ($model['when'] as $step) {
        if ($step['unsupported'] !== null) {
            $groups[] = [
                'lines' => [php_unsupported_row($step['stepId'], null, 'when',
                    $step['unsupported']['capability'] . ': ' . $step['unsupported']['reason'])],
                'stepId' => null,
            ];
            continue;
        }
        $groups[] = [
            'lines' => array_merge(
                ['    // when ' . $step['stepId'] . ' (' . $step['operation']['kind'] . ' ' . $step['operation']['id'] . ')'],
                php_render_when($step, $stepVars),
            ),
            'stepId' => null,
        ];
    }
    foreach ($model['then'] as $step) {
        $groups[] = ['lines' => php_render_then($step, $model, $stepVars, $clockIsos), 'stepId' => $step['stepId']];
    }
    return $groups;
}

function php_unsupported_row(?string $stepId, ?string $observes, string $kind, string $detail): string
{
    return '    $recorder->record(['
        . "'step_id' => " . php_emit_value($stepId) . ', '
        . "'observes' => " . php_emit_value($observes) . ', '
        . "'kind' => " . php_emit_value($kind) . ', '
        . "'outcome' => 'unsupported', "
        . "'detail' => ScenarioTestKit::boundedDetail(" . php_emit_value($detail) . ')]);';
}

function php_render_given(array $step, array &$stepVars, array &$clockIsos): array
{
    $variable = 'given_' . php_identifier_of($step['stepId']);
    $stepVars[$step['stepId']] = $variable;
    $payload = $step['payload'] ?? [];
    switch ($step['kind']) {
        case 'state':
            return [
        '    $' . $variable . ' = Port::instance()->state->seed('
                . php_emit_value($payload['entity'] ?? null) . ', ',
        '        ' . php_emit_value(php_selector_object($payload['selector'] ?? [], $stepVars)) . ', ',
        '        ' . php_emit_value(php_fields_object($payload['fields'] ?? [])) . ');',
            ];
        case 'fixture':
            return [
        '    Port::instance()->fixtures->load('
                . php_emit_value($payload['fixture'] ?? null) . ', ['
                . "'version' => " . php_emit_value($payload['version'] ?? null) . ', '
                . "'capabilities' => " . php_emit_value($payload['capabilities'] ?? []) . ']);',
            ];
        case 'actor':
            return $payload['scope'] === null
                ? ["    \$" . $variable . ' = Port::instance()->actor(' . php_emit_value($payload['actor'] ?? null) . ');']
                : ["    \$" . $variable . ' = Port::instance()->actor('
                    . php_emit_value($payload['actor'] ?? null) . ', '
                    . php_emit_value($payload['scope']) . ');'];
        case 'clock':
            $clockIsos[$step['stepId']] = $payload['at'] ?? null;
            return ['    Port::instance()->clock->freeze(' . php_emit_value($payload['at'] ?? null) . ');'];
        case 'id_source':
            return [
        '    Port::instance()->ids->seed(['
                . "'algorithm' => " . php_emit_value($payload['algorithm'] ?? null) . ', '
                . "'seed' => " . php_emit_value($payload['seed'] ?? null) . ']);',
            ];
        default:
            return ['    // unknown precondition kind ' . $step['kind'] . '; nothing to establish'];
    }
}

/** The emitted selector object of one state precondition. */
function php_selector_object(array $selector, array $stepVars): array
{
    $object = [];
    foreach ($selector as $term) {
        $object[$term['field']] = php_literal_of($term['equals'] ?? null, $stepVars);
    }
    return $object;
}

/** The emitted fields object of one state precondition. */
function php_fields_object(array $fields): array
{
    $object = [];
    $emptyVars = [];
    foreach ($fields as $entry) {
        $object[$entry[0]] = php_literal_of($entry[1] ?? null, $emptyVars);
    }
    return $object;
}

function php_render_when(array $step, array &$stepVars): array
{
    $variable = 'step_' . php_identifier_of($step['stepId']);
    $stepVars[$step['stepId']] = $variable;
    $input = [];
    foreach ($step['input'] as $entry) {
        $leaf = $entry['leaf'];
        $emptyVars = [];
        $input[$entry['field']] = php_literal_of($leaf, $emptyVars);
    }
    $ctx = [];
    if (array_key_exists('actor', $step['ctx'])) {
        $actor = $step['ctx']['actor'];
        $actorId = is_array($actor) ? ($actor['id'] ?? null) : $actor;
        $ctx['actor'] = isset($stepVars[$actorId]) ? ['__stepVar' => $stepVars[$actorId]] : $actorId;
    }
    if (array_key_exists('clock', $step['ctx'])) {
        // The clock ctx references a given clock step's frozen instant;
        // an unestablished reference resolves to null (the mapper has
        // already validated reachability before emission).
        $clockRef = is_array($step['ctx']['clock']) ? ($step['ctx']['clock']['id'] ?? null) : $step['ctx']['clock'];
        $ctx['clock'] = $clockRef;
    }
    if (array_key_exists('idempotencyKey', $step['ctx'])) {
        $emptyVars = [];
        $ctx['idempotencyKey'] = php_literal_of($step['ctx']['idempotencyKey'], $emptyVars);
    }
    return [
        "    \$" . $variable . ' = null;',
        '    try {',
        '        $' . $variable . ' = Port::instance()->invoke('
            . php_emit_value($step['operation']['id']) . ', ',
        '            ' . php_emit_value($input) . ', ',
        '            ' . php_emit_value($ctx) . ');',
        '    } catch (\Throwable $when_error) {',
        '        $recorder->record([' . "'step_id' => " . php_emit_value($step['stepId'])
            . ", 'observes' => null, 'kind' => 'when', 'outcome' => 'infrastructure', "
            . "'detail' => ScenarioTestKit::boundedDetail(\$when_error->getMessage())]);",
        '        throw $when_error;',
        '    }',
    ];
}

function php_render_then(array $step, array $model, array $stepVars, array $clockIsos): array
{
    $observed = $stepVars[$step['observes'] ?? ''] ?? ('step_' . php_identifier_of((string) ($step['observes'] ?? 'run')));
    $meta = "'step_id' => " . php_emit_value($step['stepId'])
        . ", 'observes' => " . php_emit_value($step['observes'])
        . ", 'kind' => " . php_emit_value($step['kind']);
    if ($step['unsupported'] !== null) {
        return [php_unsupported_row($step['stepId'], $step['observes'], $step['kind'],
            $step['unsupported']['capability'] . ': ' . $step['unsupported']['reason'])];
    }
    $checks = php_render_checks($step, $model, $stepVars, $clockIsos, $observed);
    $lines = [
        '    // then ' . $step['stepId'] . ': ' . $step['kind'] . ' over ' . $step['observes'],
        '    try {',
    ];
    foreach ($checks as $check) {
        $lines[] = '        ' . $check;
    }
    $lines[] = '        $recorder->record([' . $meta . ", 'outcome' => 'pass']);";
    $lines[] = '    } catch (\\Testo\\Assert\\State\\Assertion\\AssertionException $then_failure) {';
    $lines[] = '        $recorder->record([' . $meta . ", 'outcome' => 'fail', "
            . "'detail' => ScenarioTestKit::boundedDetail(\$then_failure->getMessage())]);";
    $lines[] = '        throw $then_failure;';
    $lines[] = '    } catch (\Throwable $then_error) {';
    $lines[] = '        $recorder->record([' . $meta . ", 'outcome' => 'infrastructure', "
            . "'detail' => ScenarioTestKit::boundedDetail(\$then_error->getMessage())]);";
    $lines[] = '        throw $then_error;';
    $lines[] = '    }';
    return $lines;
}

/** The check statements of one mapped then step. */
function php_render_checks(array $step, array $model, array $stepVars, array $clockIsos, string $observed): array
{
    $payload = $step['payload'] ?? [];
    switch ($step['kind']) {
        case 'result':
            $checks = ["Assert::same(true, \$" . $observed . "['ok'], ScenarioTestKit::boundedDetail(\$"
                . $observed . "['error']['id'] ?? 'invoke-failed'));"];
            if (array_key_exists('value', $payload)) {
                $emptyVars = [];
                $checks[] = 'Assert::true(ScenarioTestKit::typedEqual($' . $observed
                    . "['value'] ?? null, " . php_emit_value(php_literal_of($payload['value'], $emptyVars)) . '), '
                    . php_emit_value('result-value') . ');';
            }
            return $checks;
        case 'error':
            $checks = [
                'Assert::same(false, $' . $observed . "['ok'], " . php_emit_value('expected a typed error') . ');',
                'Assert::same(' . php_emit_value($payload['error'] ?? null) . ', $' . $observed
                    . "['error']['id'] ?? null, " . php_emit_value('error-id') . ');',
            ];
            foreach ($payload['payload'] ?? [] as $entry) {
                if (($entry['leafProblem'] ?? null) !== null) {
                    continue;
                }
                $emptyVars = [];
                $checks[] = 'Assert::true(ScenarioTestKit::errorFieldsMatch($' . $observed . "['error'] ?? null, ["
                    . php_emit_value($entry['field']) . ' => '
                    . php_emit_value(php_literal_of($entry['leaf'], $emptyVars)) . ']), '
                    . php_emit_value('error-fields') . ');';
            }
            if (!empty($payload['contract'])) {
                $projection = array_map(static fn (array $entry): string => (string) $entry['field'], $payload['payload'] ?? []);
                $checks[] = 'Assert::same(true, Port::instance()->contractCheck('
                    . php_emit_value($payload['contract']) . ', '
                    . php_emit_value($projection) . ', '
                    . '$' . $observed . "['error'] ?? null), " . php_emit_value('error-contract') . ');';
            }
            return $checks;
        case 'entity_state':
            $emptyVars = [];
            $selector = php_emit_value(php_selector_object($payload['where'] ?? [], $emptyVars));
            $exactFields = [];
            $matchFields = [];
            foreach ($payload['fields'] ?? [] as $field => $expectation) {
                if (is_array($expectation) && array_key_exists('match', $expectation)) {
                    $matchFields[] = [$field, $expectation['match']];
                    continue;
                }
                $exactFields[$field] = php_literal_of(
                    is_array($expectation) && array_key_exists('value', $expectation)
                        ? $expectation['value'] : $expectation,
                    $emptyVars,
                );
            }
            $checks = [
                '$stateRows = Port::instance()->state->query('
                    . php_emit_value($payload['entity'] ?? null) . ', ' . $selector . ');',
            ];
            $expect = $payload['expect'] ?? null;
            $count = php_expect_count($expect);
            if ($count === null) {
                $checks[] = "Assert::true(count(\$stateRows) >= 1, " . php_emit_value('entity-exists') . ');';
            } else {
                $checks[] = 'Assert::same(' . var_export($count, true) . ', count($stateRows), '
                    . php_emit_value('entity-count') . ');';
            }
            if ($exactFields !== []) {
                // Per-row projection equals the expected fields object:
                // the PHP mirror of the Node emitter's every-row check.
                $checks[] = 'Assert::true(count(array_filter($stateRows, static fn (array $row): bool => ScenarioTestKit::typedEqual('
                    . "array_intersect_key(\$row, "
                    . php_emit_value(array_fill_keys(array_keys($exactFields), true)) . '), '
                    . php_emit_value($exactFields) . '))) === count($stateRows), '
                    . php_emit_value('entity-fields') . ');';
            }
            foreach ($matchFields as [$field, $matcher]) {
                $check = php_match_check($matcher);
                if ($check === null) {
                    $checks[] = 'Assert::fail(' . php_emit_value('unrenderable match kind ' . $matcher) . ');';
                    continue;
                }
                // The row value is bound into a single-expression closure
                // so the matcher text stays a pure function of one value
                // with no interpolation surface.
                $checks[] = 'foreach ($stateRows as $match_row) { $value = $match_row['
                    . php_emit_value($field) . ']; Assert::true('
                    . str_replace('\\$value', '$value', $check) . ', '
                    . php_emit_value('entity-match:' . $field . ':' . $matcher) . '); }';
            }
            return array_map(
                static fn (string $line): string => ltrim($line),
                $checks,
            );
        case 'emitted':
            $target = $payload['target'] ?? [];
            $countExpr = php_emit_count($payload['count'] ?? null);
            return [
                '$emissions = array_values(array_filter('
                . 'Port::instance()->emissions(), '
                . 'static fn (array $entry): bool => '
                . '($entry[' . "'id'" . '] ?? null) === ' . php_emit_value($target['id'] ?? null)
                . ' && ($entry[' . "'kind'" . '] ?? null) === ' . php_emit_value($target['kind'] ?? null) . '));',
                $countExpr === null
                    ? "Assert::true(count(\$emissions) >= 1, " . php_emit_value('emitted-at-least-one') . ');'
                    : 'Assert::true(count($emissions) ' . $countExpr . ', '
                        . php_emit_value('emitted-count') . ');',
            ];
        case 'forbidden_effect':
            $scopeFilters = [];
            if (($payload['scope'] ?? null) === 'field') {
                $scopeFilters[] = '($entry[' . "'field'" . '] ?? null) === ' . php_emit_value($payload['field'] ?? null);
            }
            return [
                '$matching = array_values(array_filter('
                . 'Port::instance()->effects(), '
                . 'static fn (array $entry): bool => '
                . '($entry[' . "'effect'" . '] ?? null) === ' . php_emit_value($payload['effect'] ?? null)
                . ($scopeFilters !== [] ? ' && ' . implode(' && ', $scopeFilters) : '') . '));',
                'Assert::same(0, count($matching), ' . php_emit_value('forbidden-effect') . ');',
            ];
        case 'authorization':
            return [
                '$decision = Port::instance()->authorize('
                . php_emit_value($payload['actor']['id'] ?? null) . ', '
                . php_emit_value($payload['policy'] ?? null) . ', '
                . php_emit_value(php_observes_operation($model, $step)) . ');',
                'Assert::same(' . php_emit_value($payload['outcome'] ?? null) . ', $decision, '
                    . php_emit_value('authorization-outcome') . ');',
            ];
        case 'idempotency':
            $original = 'step_' . php_identifier_of((string) ($payload['replay'] ?? ''));
            $checks = [
                'Assert::true(ScenarioTestKit::typedEqual($' . $observed . ', $' . $original . '), '
                    . php_emit_value('replay-equivalence') . ');',
            ];
            if (($payload['duplicates'] ?? null) === 'none') {
                $originalStep = null;
                foreach ($model['when'] as $candidate) {
                    if ($candidate['stepId'] === ($payload['replay'] ?? null)) {
                        $originalStep = $candidate;
                        break;
                    }
                }
                if (($originalStep['operation']['id'] ?? null) !== null) {
                    $checks[] = '$original_emissions = array_filter('
                    . 'Port::instance()->emissions(), '
                    . 'static fn (array $entry): bool => '
                    . '($entry[' . "'operation'" . '] ?? null) === '
                    . php_emit_value($originalStep['operation']['id']) . ');';
                    $checks[] = 'Assert::true(count($original_emissions) <= 1, '
                        . php_emit_value('duplicates-none') . ');';
                }
            }
            return array_map(
                static fn (string $line): string => ltrim($line),
                $checks,
            );
        case 'contract_match':
            return [
                '$projection = ' . php_emit_value($payload['projection'] ?? []) . ';',
                '$actual = $' . $observed . "['value'] ?? null;",
                'Assert::same(true, Port::instance()->contractCheck('
                    . php_emit_value($payload['contract'] ?? null) . ', $projection, $actual), '
                    . php_emit_value('contract-match') . ');',
            ];
        case 'deterministic_fixture':
            $fixtureStep = null;
            foreach ($model['given'] as $candidate) {
                if ($candidate['kind'] === 'fixture') {
                    $fixtureStep = $candidate;
                    break;
                }
            }
            if ($fixtureStep === null) {
                return ['Assert::fail(' . php_emit_value('deterministic_fixture without a fixture precondition') . ');'];
            }
            // The digest covers the fixture, the seeded id source, and the
            // frozen clock, so the equality transitively asserts the
            // declared clock/idSource control refs.
            return [
                'Assert::same(' . php_emit_value($payload['digest'] ?? null) . ', '
                . 'Port::instance()->fixtureDigest('
                . php_emit_value($fixtureStep['payload']['fixture'] ?? null) . '), '
                . php_emit_value('fixture-digest') . ');',
            ];
        default:
            return ['Assert::fail(' . php_emit_value('unrenderable assertion kind ' . $step['kind']) . ');'];
    }
}

/** The emitted value check for one closed matcher kind. */
function php_match_check(string $matcher): ?string
{
    return match ($matcher) {
        'uuid' => "preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\\$/', (string) \\$value) === 1",
        'datetime' => 'ScenarioTestKit::canonicalDatetime((string) \\$value)',
        'uri' => 'ScenarioTestKit::canonicalUri((string) \\$value)',
        'decimal' => 'ScenarioTestKit::canonicalDecimal((string) \\$value)',
        'non-null' => '\\$value !== null',
        default => null,
    };
}

/** Wrap one matcher expression over `$value` into a full assert line. */
function php_match_assert(string $check, string $label): string
{
    return 'Assert::true((static function (mixed $value): bool { return '
        . $check . '; })(' . '$' . "row['" . str_replace('entity-match:', '', $label) . "'] ?? null" . '), '
        . php_emit_value($label) . ');';
}

/** The observed when step of one then step. */
function php_observes_operation(array $model, array $step): ?string
{
    foreach ($model['when'] as $candidate) {
        if ($candidate['stepId'] === ($step['observes'] ?? null)) {
            return $candidate['operation']['id'] ?? null;
        }
    }
    return null;
}

/** The exact row count expectation, or null for at-least-one. */
function php_expect_count(mixed $expect): int|string|null
{
    if (is_array($expect)) {
        if (array_key_exists('count', $expect)) {
            return $expect['count'];
        }
        if (($expect['presence'] ?? null) === 'missing') {
            return 0;
        }
        if (($expect['presence'] ?? null) === 'exists') {
            return null;
        }
    }
    if (is_int($expect)) {
        return $expect;
    }
    return $expect;
}

/** The emitted comparison operator of one closed count shape. */
function php_emit_count(mixed $count): ?string
{
    if (is_array($count)) {
        if (array_key_exists('exactly', $count)) {
            return '=== ' . var_export($count['exactly'], true);
        }
        if (array_key_exists('atLeast', $count)) {
            return '>= ' . var_export($count['atLeast'], true);
        }
    }
    return null;
}

/**
 * Render one typed leaf (value or reference) into its emitted argument.
 * `step-output` and `given-value` references become the emitted
 * variable bindings of their steps; every other reference kind compiles
 * to its identity string (a port-call argument, never guessed code).
 */
function php_literal_of(mixed $leaf, array &$stepVars = []): mixed
{
    if ($leaf === null || !is_array($leaf) || array_is_list($leaf)) {
        return null;
    }
    if (is_string($leaf['$ref'] ?? null)) {
        if (($leaf['$ref'] === 'step-output' || $leaf['$ref'] === 'given-value')
            && is_string($leaf['id'] ?? null)
            && isset($stepVars[$leaf['id']])) {
            return ['__stepVar' => $stepVars[$leaf['id']]];
        }
        return '$ref:' . $leaf['$ref'] . ':' . ($leaf['id'] ?? 'null');
    }
    switch ($leaf['type'] ?? null) {
        case 'null':
            return null;
        case 'boolean':
        case 'string':
        case 'decimal':
        case 'date':
        case 'datetime':
        case 'uuid':
        case 'uri':
            return $leaf['value'] ?? null;
        case 'integer':
            return is_string($leaf['value'] ?? null) ? (int) $leaf['value'] : $leaf['value'];
        case 'list':
            $items = [];
            foreach ($leaf['value'] ?? [] as $item) {
                $items[] = php_literal_of($item, $stepVars);
            }
            return $items;
        case 'object':
            $object = [];
            foreach ($leaf['value'] ?? [] as $key => $value) {
                $object[$key] = php_literal_of($value, $stepVars);
            }
            return $object;
        default:
            throw new LogicException('unrenderable leaf kind ' . (string) ($leaf['type'] ?? 'null'));
    }
}

/**
 * Render one literal_of output into its exact emitted PHP text: step
 * variables pass through raw, everything else is single-quote encoded
 * with no interpolation (no escaping drift, no code injection).
 */
function php_emit_value(mixed $value): string
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
    if (is_float($value)) {
        // Closed-wire decimals never ride as floats; a float here is an
        // emitter bug, and emitting a raw float literal would silently
        // weaken the typed contract.
        throw new LogicException('float value in a closed typed position');
    }
    if (is_string($value)) {
        return "'" . str_replace(['\\', "'"], ['\\\\', "\\'"], $value) . "'";
    }
    if (is_array($value) && isset($value['__stepVar'])) {
        return '$' . $value['__stepVar'];
    }
    if (is_array($value) && array_is_list($value)) {
        $items = array_map(__FUNCTION__, $value);
        return '[' . implode(', ', $items) . ']';
    }
    if (is_array($value)) {
        $members = [];
        foreach ($value as $key => $member) {
            $members[] = php_emit_value((string) $key) . ' => ' . php_emit_value($member);
        }
        return '[' . implode(', ', $members) . ']';
    }
    throw new LogicException('unrenderable emitted value');
}
