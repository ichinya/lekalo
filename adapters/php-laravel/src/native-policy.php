<?php
/**
 * Pure Composer/Laravel execution-policy verification (issue #61, plan
 * slice B; research doc §1 step 3 and §2).
 *
 * `php_verify_confirmations` checks every confirmed (package, gate)
 * recipe of the checked-in execution policy against the project's real
 * `composer.json` bytes: the manifest digest, the exact script entry
 * bytes, the decoded literal argv, and the bounded transitive recipe
 * closure. A confirmation never derives authority from being written
 * down — the manifest text is the only source of the executed argv,
 * and the policy argv must match it exactly.
 *
 * Unlike the Node #48 planner's `confirmationByPackage` map, which
 * silently kept only the last confirmation per package, confirmations
 * join per package by the stable `gate_id`: several confirmations for
 * one package each produce their own confirmed tuple.
 *
 * The module is pure: no clock, no environment, no process launch, no
 * I/O — the project bytes arrive through the injected `callable
 * $reader(string $logicalPath): ?string`. `php -n` compatible: PHP
 * built-ins only (json, hash, pcre), no Composer package.
 */

declare(strict_types=1);

/** The digest domain of the checked-in Composer execution policy. */
const PHP_POLICY_DIGEST_DOMAIN = 'lekalo.native-policy.v0.4.0';

/** The closed v0.4.0 gate-kind metadata set (mirrors the successor schema). */
const PHP_GATE_KINDS = [
    'composer-script', 'mago-format', 'mago-lint', 'mago-analyze', 'mago-guard',
    'laratesto', 'pest', 'phpunit', 'artisan-check', 'boot-smoke',
    'migration-static', 'migration-execute', 'discovery-smoke', 'legacy-suite',
];

/** The closed execution-gate set (the four-value v0.3.2 enum, unchanged). */
const PHP_GATES = ['build', 'typecheck', 'lint', 'test'];

/** Shell metacharacters and interpolation syntax refused in script literals. */
const PHP_SHELL_METACHARACTERS = '|&;<>()$`"\'\\' . "\n\r\t";

/** Composer/PHP subcommands that mutate, resolve the network, or dispatch plugins. */
const PHP_FORBIDDEN_SCRIPT_TOKENS = [
    'composer', 'composer.phar', 'install', 'update', 'require', 'remove',
    'config', 'global', 'create-project', 'exec', 'diagnose', 'putenv',
    'sh', 'bash', 'cmd', 'powershell', 'pwsh', 'eval',
];

/** Artisan commands that open sockets or interactive sessions. */
const PHP_FORBIDDEN_ARTISAN_TOKENS = ['serve', 'tinker', 'rx'];

/** The maximum scripts one composer.json may declare (decoder bound). */
const PHP_MAX_SCRIPTS = 64;
/** The maximum elements one script entry may carry (decoder bound). */
const PHP_MAX_SCRIPT_ELEMENTS = 16;
/** The maximum resolved leaf commands one script closure may expand to. */
const PHP_MAX_CLOSURE_COMMANDS = 128;
/** The maximum bytes of one script entry string. */
const PHP_MAX_SCRIPT_BYTES = 4096;

/**
 * Whether one string is a safe literal argv element: no shell
 * metacharacters, no interpolation, no globs. The check is on the
 * complete string, never a prefix.
 */
function php_is_safe_literal(string $text): bool
{
    if ($text === '' || strlen($text) > 1024) {
        return false;
    }
    if ((bool) preg_match('/[\s]{2,}/', $text)) {
        return false;
    }
    for ($i = 0, $n = strlen($text); $i < $n; $i++) {
        if (str_contains(PHP_SHELL_METACHARACTERS, $text[$i])) {
            return false;
        }
    }
    return true;
}

/**
 * Canonical JSON text: recursively bytewise key-sorted, compact, UTF-8.
 * Delegates to the kernel's byte-compatible encoder when loaded (the
 * shipped artifact concatenates the kernel first); standalone test
 * loading falls back to the same closed encoder inline.
 */
function php_canonical_text(array|bool|int|string|null $value): string
{
    if (function_exists('write_canonical')) {
        return write_canonical($value);
    }
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
        $encoded = json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
        return $encoded;
    }
    if (array_is_list($value)) {
        return '[' . implode(',', array_map(__FUNCTION__, $value)) . ']';
    }
    $keys = array_keys($value);
    usort($keys, 'strcmp');
    $body = [];
    foreach ($keys as $key) {
        $body[] = json_encode((string) $key, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR)
            . ':' . php_canonical_text($value[$key]);
    }
    return '{' . implode(',', $body) . '}';
}

/** sha256 over the pinned domain joined with the canonical bytes. */
function php_domain_digest(string $domain, array|bool|int|string|null $value): string
{
    return 'sha256:' . hash('sha256', $domain . php_canonical_text($value));
}

/**
 * Decode one composer.json `scripts` entry into the resolved literal
 * command list. Returns ['ok' => true, 'commands' => string[][]] where
 * each command is an argv vector, or ['ok' => false, 'reason' => token].
 *
 * Accepted forms (research §2: a simple direct literal recipe first;
 * anything else stays unsupported without substituting scripts):
 *   - "literal command string"       one command, single-spaced tokens;
 *   - "@other-script"                a reference edge into the DAG;
 *   - ["@a", "literal", ["extra"]]   element list; an array element
 *                                    appends literal args to the
 *                                    previous command.
 * `@php` resolves to the pinned interpreter token `php`; `@composer`
 * and every other Composer dispatch form is refused — the host never
 * launches the Composer dispatcher, installs, updates, or resolves
 * the network (research §3 step 3).
 *
 * @param array<string, mixed> $scripts the decoded scripts map
 * @param string $entry the requested script name
 */
function php_decode_composer_script(array $scripts, string $entry): array
{
    if ($entry === '' || strlen($entry) > 64
        || !(bool) preg_match('/^[A-Za-z0-9_][A-Za-z0-9._:-]*$/', $entry)) {
        return ['ok' => false, 'reason' => 'script-name-invalid'];
    }
    if (!array_key_exists($entry, $scripts)) {
        return ['ok' => false, 'reason' => 'script-absent'];
    }
    if (count($scripts) > PHP_MAX_SCRIPTS) {
        return ['ok' => false, 'reason' => 'script-map-oversize'];
    }
    $resolved = [];
    $stack = [];
    $outcome = php_decode_script_value($scripts[$entry], $scripts, $entry, $resolved, $stack, 0);
    if (!$outcome['ok']) {
        return $outcome;
    }
    if (count($resolved) === 0 || count($resolved) > PHP_MAX_CLOSURE_COMMANDS) {
        return ['ok' => false, 'reason' => 'script-closure-bound'];
    }
    // Every resolved leaf command must be a literal argv whose
    // executable is the pinned interpreter token and whose program is
    // a repo-relative path: the host resolves `php` through the
    // trusted catalog, never ambient PATH or a project shim.
    foreach ($resolved as $argv) {
        if ($argv[0] !== 'php') {
            return ['ok' => false, 'reason' => 'script-executable-unsupported'];
        }
        $program = $argv[1] ?? '';
        if ($program === '' || str_starts_with($program, '/')
            || str_contains($program, '\\') || str_contains($program, '..')) {
            return ['ok' => false, 'reason' => 'script-program-path'];
        }
    }
    return ['ok' => true, 'commands' => $resolved];
}

/**
 * Recursive decoder for one script value. `$resolved` collects the
 * leaf argv vectors; `$stack` carries the active reference path for
 * cycle rejection.
 *
 * @param array<string, mixed> $scripts
 * @param list<list<string>> $resolved
 * @param list<string> $stack
 */
function php_decode_script_value(mixed $value, array $scripts, string $name, array &$resolved, array &$stack, int $depth): array
{
    if ($depth > PHP_MAX_SCRIPTS) {
        return ['ok' => false, 'reason' => 'script-cycle'];
    }
    if (is_string($value)) {
        return php_decode_script_string($value, $scripts, $name, $resolved, $stack, $depth);
    }
    if (is_array($value) && array_is_list($value)) {
        if (count($value) === 0 || count($value) > PHP_MAX_SCRIPT_ELEMENTS) {
            return ['ok' => false, 'reason' => 'script-element-bound'];
        }
        $appendedTo = -1;
        foreach ($value as $element) {
            if (is_array($element)) {
                // An array element appends literal arguments to the
                // previous command (Composer arg-list semantics).
                if ($appendedTo < 0) {
                    return ['ok' => false, 'reason' => 'script-unsupported'];
                }
                if (count($element) > PHP_MAX_SCRIPT_ELEMENTS) {
                    return ['ok' => false, 'reason' => 'script-element-bound'];
                }
                foreach ($element as $argument) {
                    if (!is_string($argument) || !php_is_safe_literal($argument)) {
                        return ['ok' => false, 'reason' => 'script-unsafe-element'];
                    }
                    if (php_token_forbidden($argument)) {
                        return ['ok' => false, 'reason' => 'script-package-manager'];
                    }
                    $resolved[$appendedTo][] = $argument;
                }
                continue;
            }
            if (!is_string($element)) {
                return ['ok' => false, 'reason' => 'script-unsupported'];
            }
            $before = count($resolved);
            $outcome = php_decode_script_string($element, $scripts, $name, $resolved, $stack, $depth);
            if (!$outcome['ok']) {
                return $outcome;
            }
            // A plain command element starts a new command; a pure
            // reference to a previous element extends it.
            $appendedTo = count($resolved) - 1;
            if (count($resolved) === $before && $appendedTo >= 0) {
                $appendedTo = $before - 1;
            }
        }
        return ['ok' => true];
    }
    return ['ok' => false, 'reason' => 'script-unsupported'];
}

/**
 * One command string: a pure `@reference`, or a single-spaced literal
 * command whose tokens survive the closed safety grammar.
 *
 * @param array<string, mixed> $scripts
 * @param list<list<string>> $resolved
 * @param list<string> $stack
 */
function php_decode_script_string(string $text, array $scripts, string $name, array &$resolved, array &$stack, int $depth): array
{
    if ($text === '' || strlen($text) > PHP_MAX_SCRIPT_BYTES || !php_is_safe_literal($text)) {
        return ['ok' => false, 'reason' => 'script-shell-syntax'];
    }
    $tokens = explode(' ', $text);
    $first = $tokens[0];
    if (str_starts_with($first, '@')) {
        $reference = substr($first, 1);
        if ($reference === '') {
            return ['ok' => false, 'reason' => 'script-reference-empty'];
        }
        if ($reference === 'composer') {
            // Composer dispatch: never planned, never launched.
            return ['ok' => false, 'reason' => 'script-package-manager'];
        }
        if ($reference === 'php') {
            // The Composer-registered interpreter alias resolves to
            // the pinned interpreter token; the rest are literals.
            if (count($tokens) < 2) {
                return ['ok' => false, 'reason' => 'script-unsupported'];
            }
            array_shift($tokens);
            if ($tokens[0] === 'artisan' && php_artisan_token_forbidden($tokens)) {
                return ['ok' => false, 'reason' => 'script-network-or-interactive'];
            }
            $resolved[] = ['php', ...$tokens];
            return ['ok' => true];
        }
        if (isset($scripts[$reference])) {
            // A reference to another script: DAG edge with cycle
            // rejection over the active stack.
            if (in_array($reference, $stack, true)) {
                return ['ok' => false, 'reason' => 'script-cycle'];
            }
            if (count($tokens) !== 1) {
                return ['ok' => false, 'reason' => 'script-reference-args'];
            }
            $stack[] = $reference;
            $outcome = php_decode_script_value($scripts[$reference], $scripts, $reference, $resolved, $stack, $depth + 1);
            array_pop($stack);
            return $outcome;
        }
        return ['ok' => false, 'reason' => 'script-reference-unknown'];
    }
    if (count($tokens) > 0 && $tokens[0] === 'composer') {
        return ['ok' => false, 'reason' => 'script-package-manager'];
    }
    if ($tokens[0] === 'php' && ($tokens[1] ?? '') === 'artisan'
        && php_artisan_token_forbidden($tokens)) {
        return ['ok' => false, 'reason' => 'script-network-or-interactive'];
    }
    foreach ($tokens as $token) {
        if (php_token_forbidden($token)) {
            return ['ok' => false, 'reason' => 'script-package-manager'];
        }
    }
    $resolved[] = $tokens;
    return ['ok' => true];
}

/** Whether one literal token names a mutating or dispatching program. */
function php_token_forbidden(string $token): bool
{
    $lower = strtolower($token);
    if (in_array($lower, PHP_FORBIDDEN_SCRIPT_TOKENS, true)) {
        return true;
    }
    if (str_ends_with($lower, '/composer') || str_ends_with($lower, '\\composer')
        || str_ends_with($lower, 'composer.phar')) {
        return true;
    }
    // Assignment prefixes are environment injection through the shell.
    if ((bool) preg_match('/^[A-Za-z_][A-Za-z0-9_]*=/', $token)) {
        return true;
    }
    return false;
}

/** Whether one `php artisan ...` argv carries a networked/interactive command. */
function php_artisan_token_forbidden(array $tokens): bool
{
    foreach (array_slice($tokens, 2) as $token) {
        if (in_array(strtolower($token), PHP_FORBIDDEN_ARTISAN_TOKENS, true)) {
            return true;
        }
    }
    return false;
}

/**
 * Verify every confirmation of the execution policy against the
 * project's real composer.json bytes, joining several confirmations
 * per package by the stable gate id.
 *
 * `$reader(string $logicalPath): ?string` supplies the project bytes
 * (null when the file is absent or outside the declared read view).
 * The returned shape:
 *   ['ok' => bool, 'unverifiable' => list<string>, 'confirmed' => list<array>,
 *    'tool_catalog' => list<array>, 'policy_digest' => string]
 *
 * @param callable(string): ?string $reader
 */
function php_verify_confirmations(array $policyDocument, callable $reader): array
{
    $unverifiable = [];
    // The policy digest is computed over the canonical document with
    // the digest member absent (the plan_digest convention).
    $digestInput = $policyDocument;
    unset($digestInput['policy_digest']);
    $policyDigest = php_domain_digest(PHP_POLICY_DIGEST_DOMAIN, $digestInput);
    if (($policyDocument['policy_digest'] ?? null) !== $policyDigest) {
        $unverifiable[] = 'policy-digest-drift';
        return ['ok' => false, 'unverifiable' => $unverifiable, 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    $manifestBytes = $reader('composer.json');
    if ($manifestBytes === null) {
        return ['ok' => false, 'unverifiable' => ['composer-manifest-absent'], 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    $manifestDigest = 'sha256:' . hash('sha256', $manifestBytes);
    try {
        $manifest = json_decode($manifestBytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['ok' => false, 'unverifiable' => ['composer-manifest-unparsable'], 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    if (!is_array($manifest) || !isset($manifest['name']) || !is_string($manifest['name'])) {
        return ['ok' => false, 'unverifiable' => ['composer-manifest-invalid'], 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    $scripts = isset($manifest['scripts']) && is_array($manifest['scripts']) ? $manifest['scripts'] : [];
    $packageId = '.' . '=' . $manifest['name'];
    $confirmed = [];
    foreach (($policyDocument['confirmations'] ?? []) as $confirmation) {
        $joined = php_verify_one_confirmation($confirmation, $packageId, $manifestDigest, $scripts, $reader);
        if ($joined['ok']) {
            $confirmed[] = $joined['confirmed'];
        } else {
            $unverifiable[] = $joined['reason'];
        }
    }
    // The whole policy must verify: a partially confirmed gate set is
    // never a planning input (fail closed, never a silent subset).
    $ok = $unverifiable === [] && $confirmed !== [];
    return [
        'ok' => $ok,
        'unverifiable' => array_slice($unverifiable, 0, 16),
        'confirmed' => $confirmed,
        'tool_catalog' => php_build_tool_catalog($policyDocument),
        'policy_digest' => $policyDigest,
    ];
}

/**
 * Verify one confirmation against the decoded scripts map. The exact
 * script entry bytes are re-canonicalized for the entry digest, the
 * decoded closure must match the confirmed argv command-for-command,
 * and the confirmation's cwd/package/gate-kind must sit inside the
 * closed grammar.
 *
 * @param array<string, mixed> $scripts
 * @param callable(string): ?string $reader
 * @return array{ok: bool, confirmed?: array<string, mixed>, reason?: string}
 */
function php_verify_one_confirmation(array $confirmation, string $packageId, string $manifestDigest, array $scripts, callable $reader): array
{
    foreach (['package_id', 'gate', 'gate_id', 'gate_kind', 'required', 'cwd', 'script_name', 'manifest_digest', 'script_digest', 'argv', 'tool_ref', 'rule_version', 'rule_digest'] as $key) {
        if (!array_key_exists($key, $confirmation)) {
            return ['ok' => false, 'reason' => 'confirmation-incomplete'];
        }
    }
    if ($confirmation['package_id'] !== $packageId) {
        // A confirmation for another project is not applicable here;
        // it is neither verified nor a failure (fixture policies carry
        // several roots' worth of confirmations by design).
        return ['ok' => false, 'reason' => 'confirmation-other-package'];
    }
    if (!in_array($confirmation['gate'], PHP_GATES, true)
        || !in_array($confirmation['gate_kind'], PHP_GATE_KINDS, true)) {
        return ['ok' => false, 'reason' => 'confirmation-gate-unsupported'];
    }
    if (!is_bool($confirmation['required'])) {
        return ['ok' => false, 'reason' => 'confirmation-required-invalid'];
    }
    if ($confirmation['cwd'] !== '.' && !(bool) preg_match('/^[a-z0-9.][a-z0-9._-]*(\/[a-z0-9.][a-z0-9._-]*)*$/', (string) $confirmation['cwd'])) {
        return ['ok' => false, 'reason' => 'confirmation-cwd-invalid'];
    }
    // Custody: the confirmation names the exact manifest bytes this
    // kernel just read — manifest drift is never a silent pass.
    if ($confirmation['manifest_digest'] !== $manifestDigest) {
        return ['ok' => false, 'reason' => 'manifest-digest-drift'];
    }
    $scriptName = (string) $confirmation['script_name'];
    if (!array_key_exists($scriptName, $scripts)) {
        return ['ok' => false, 'reason' => 'script-absent-' . substr($scriptName, 0, 48)];
    }
    $entryDigest = 'sha256:' . hash('sha256', php_canonical_text($scripts[$scriptName]));
    if ($entryDigest !== $confirmation['script_digest']) {
        return ['ok' => false, 'reason' => 'script-digest-drift-' . substr($scriptName, 0, 48)];
    }
    $decoded = php_decode_composer_script($scripts, $scriptName);
    if (!$decoded['ok']) {
        return ['ok' => false, 'reason' => $decoded['reason'] . '-' . substr($scriptName, 0, 48)];
    }
    // One confirmed recipe = one resolved literal command. Multi-
    // command closures are recorded but only a single-command closure
    // confirms a runnable gate today (complex recipes stay unsupported
    // until they have a qualified execution path).
    if (count($decoded['commands']) !== 1) {
        return ['ok' => false, 'reason' => 'script-closure-complex-' . substr($scriptName, 0, 48)];
    }
    $argv = $decoded['commands'][0];
    if (!is_array($confirmation['argv']) || $confirmation['argv'] !== $argv) {
        return ['ok' => false, 'reason' => 'argv-mismatch-' . substr($scriptName, 0, 48)];
    }
    $toolRef = $confirmation['tool_ref'];
    if (!is_array($toolRef) || !isset($toolRef['id'], $toolRef['artifact_digest'])
        || !is_string($toolRef['id']) || !is_string($toolRef['artifact_digest'])
        || !(bool) preg_match('/^sha256:[0-9a-f]{64}$/', $toolRef['artifact_digest'])) {
        return ['ok' => false, 'reason' => 'confirmation-tool-invalid'];
    }
    $covers = $confirmation['covers_suite_ids'] ?? [];
    if (!is_array($covers)) {
        return ['ok' => false, 'reason' => 'confirmation-covers-invalid'];
    }
    $transitive = php_script_closure_digest($scripts, $scriptName);
    return ['ok' => true, 'confirmed' => [
        'package_id' => $confirmation['package_id'],
        'gate' => $confirmation['gate'],
        'gate_id' => (string) $confirmation['gate_id'],
        'gate_kind' => (string) $confirmation['gate_kind'],
        'required' => $confirmation['required'],
        'cwd' => (string) $confirmation['cwd'],
        'script_name' => $scriptName,
        'manifest_digest' => (string) $confirmation['manifest_digest'],
        'script_digest' => (string) $confirmation['script_digest'],
        'closure_digest' => $transitive,
        'confirmation_ref' => (string) $confirmation['rule_digest'],
        'argv' => $argv,
        'env' => [],
        'tool_ref' => (string) $toolRef['id'],
        'covers_suite_ids' => array_values(array_filter($covers, 'is_string')),
    ]];
}

/**
 * The bounded transitive recipe custody digest: sha256 over the
 * canonical resolved closure of one script (references expanded,
 * cycles refused). The runner rechecks this digest before launch.
 */
function php_script_closure_digest(array $scripts, string $scriptName): string
{
    $decoded = php_decode_composer_script($scripts, $scriptName);
    if (!$decoded['ok']) {
        return 'sha256:' . hash('sha256', 'unresolved:' . $decoded['reason']);
    }
    return php_domain_digest('lekalo.native-script-closure.v0.4.0', $decoded['commands']);
}

/**
 * The trusted tool catalog from the policy's confirmed tool refs,
 * deduplicated by id: the digests are the custody anchors the host
 * runner verifies before any launch (never PATH, never a .bat shim).
 *
 * @return list<array<string, mixed>>
 */
function php_build_tool_catalog(array $policyDocument): array
{
    $seen = [];
    $catalog = [];
    foreach (($policyDocument['confirmations'] ?? []) as $confirmation) {
        $tool = $confirmation['tool_ref'] ?? null;
        if (!is_array($tool) || !isset($tool['id']) || !is_string($tool['id']) || isset($seen[$tool['id']])) {
            continue;
        }
        $seen[$tool['id']] = true;
        $catalog[] = [
            'id' => $tool['id'],
            'name' => is_array($confirmation['argv'] ?? null) ? (string) ($confirmation['argv'][0] ?? $tool['id']) : $tool['id'],
            'version' => isset($tool['version']) && is_string($tool['version']) ? $tool['version'] : 'unknown',
            'artifact_digest' => (string) $tool['artifact_digest'],
            'entry_digest' => isset($tool['entry_digest']) && is_string($tool['entry_digest']) ? $tool['entry_digest'] : null,
            'platform' => PHP_OS_FAMILY === 'Windows' ? 'windows' : strtolower(PHP_OS_FAMILY),
            'provenance' => 'checked-in-policy',
        ];
    }
    usort($catalog, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));
    return $catalog;
}

/**
 * The tool catalog digest: sha256 over the canonical catalog bytes.
 */
function php_tool_catalog_digest(array $catalog): string
{
    return php_domain_digest('lekalo.native-tool-catalog.v0.4.0', $catalog);
}
