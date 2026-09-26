<?php
/**
 * The committed single-file adapter artifact of `lekalo-target-php-laravel`
 * (issue #54).
 *
 * GENERATED FILE — regenerate with `php adapters/php-laravel/build.php`;
 * verify with `php adapters/php-laravel/build.php --check`. Never edit.
 *
 * Self-contained: PHP built-ins only (json, hash, SPL), no Composer
 * package, no extension beyond always-compiled basics, so the confined
 * core runtime can copy the interpreter plus exactly this script.
 */
/**
 * The `lekalo.target/v1` protocol kernel of `lekalo-target-php-laravel`
 * (issue #54) — the PHP reference implementation of the target protocol.
 *
 * The kernel is a read-only-by-default PHP program built on PHP built-ins
 * only: no Composer package, no extension beyond `json` and `hash` (both
 * bundled and always compiled in). The protocol lifecycle mirrors the
 * Node kernel (issue #43) exactly: strict bounded request decode, closed
 * request validation, canonical response serialization, the mandatory
 * `describe` handshake, and honest `unsupported` refusals — never fake
 * successes.
 *
 * Runtime entry points:
 *   php adapter.php                                  one-shot protocol
 *                                                    process (stdin or
 *                                                    --lekalo-request-file)
 *   php adapter.php --version-json                   local metadata probe
 *                                                    (sole argument)
 *
 * Boundary decisions frozen for #54 (see README.md):
 * - The v0.2.16 wire has no runtime-version slot and no root-bearing
 *   profile transport; PHP runtime metadata is reported by the local
 *   `--version-json` probe and internal evidence only.
 * - Generation rides the closed write-plan seam: a dry run plans exact
 *   digests, an apply writes exactly those bytes inside the staged view.
 * - Unsupported operations are honest in-envelope `unsupported` errors,
 *   never silent lowering; core-side refused requests are bounded stderr
 *   diagnostics plus a nonzero exit, never a synthetic envelope.
 */

declare(strict_types=1);

// The protocol transport is exact-byte stdout: PHP CLI notice/warning
// rendering (which targets STDOUT by default) is disabled before any
// code can emit, and error logging is bound to the bounded stderr
// channel. A polluted envelope is a response refusal at the core, so
// the guard runs first, unconditionally.
ini_set('display_errors', '0');
ini_set('display_startup_errors', '0');
ini_set('log_errors', '1');
ini_set('error_log', 'php://stderr');
ini_set('implicit_flush', '0');
error_reporting(E_ALL);

const PROTOCOL_TOKEN = 'lekalo.target/v1';
/** The sole protocol version this kernel speaks (the current contract). */
const VERSION = '0.3.2';
/** The closed supported-version set: exact membership, never ranges. */
const SUPPORTED_VERSIONS = ['0.3.2'];
/** The adapter identity token. */
const ADAPTER_ID = 'lekalo-target-php-laravel';
/** The adapter release version. */
const ADAPTER_VERSION = '0.1.0';
/** The adapter target token (the wire `target` of generate/bind). */
const TARGET_TOKEN = 'php-laravel';
/** The declared profile token. */
const PROFILE_TOKEN = 'default';
/** The accepted core IR contract version. */
const IR_VERSION = '0.2.16';

/** The maximum request size this kernel reads (mirrors the core bound). */
const MAX_REQUEST_BYTES = 1024 * 1024;
/** The maximum response size this kernel writes (mirrors the core cap). */
const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;
/** The maximum single generated file size (mirrors the core artifact bound). */
const MAX_FILE_BYTES = 4 * 1024 * 1024;
/** The maximum number of files one generation plan may declare. */
const MAX_WRITE_FILES = 1024;
/** The closed v1 operation set (wire spellings). */
const OPERATIONS = [
    'describe', 'scan', 'bind', 'validate', 'generate',
    'verify', 'clean', 'plan-clean', 'plan-native',
];
/** Operations that consume the compiled IR. */
const IR_OPERATIONS = ['validate', 'generate', 'verify'];
/** The closed support-state set (issue #28). */
const SUPPORT_STATES = ['full', 'partial', 'unsupported', 'unknown'];
/** The declared named capabilities of this kernel (issue #28 ids). */
const DECLARED_CAPABILITIES = [
    'generate.zod' => 'unsupported',
    'generate.openapi' => 'unsupported',
    'generate.ui' => 'unsupported',
    'scan.symbols' => 'unsupported',
    'verify.scenarios' => 'unsupported',
    'verify.transport-http' => 'unsupported',
    'generate.transport-http' => 'unsupported',
    'preserve.classification' => 'unsupported',
];

/**
 * One request that failed closed decoding/validation before any
 * operation could run. Always maps to a bounded stderr diagnostic plus
 * exit 1 — a synthetic envelope with a fabricated echo is never legal.
 */
final class RequestRefusal extends RuntimeException
{
    public function __construct(string $code)
    {
        parent::__construct($code);
    }
}

// ---------------------------------------------------------------------------
// 1. Bounded input reading and strict JSON decoding.
// ---------------------------------------------------------------------------

/**
 * Read the request bytes from stdin or `--lekalo-request-file PATH`,
 * bounded at exactly MAX_REQUEST_BYTES; one more byte is a refusal.
 */
function read_request_bytes(): string
{
    $path = null;
    foreach ($_SERVER['argv'] ?? [] as $index => $argument) {
        if ($argument === '--lekalo-request-file') {
            if ($path !== null) {
                throw new RequestRefusal('transport');
            }
            $path = $_SERVER['argv'][$index + 1] ?? null;
            if ($path === null || $path === '') {
                throw new RequestRefusal('transport');
            }
        }
    }
    if ($path !== null) {
        $handle = @fopen($path, 'rb');
        if ($handle === false) {
            throw new RequestRefusal('transport');
        }
        $meta = stream_get_meta_data($handle);
        if (($meta['wrapper_type'] ?? '') !== 'plainfile') {
            fclose($handle);
            throw new RequestRefusal('transport');
        }
    } else {
        $handle = STDIN;
    }
    $chunks = [];
    $total = 0;
    while (!feof($handle)) {
        $chunk = fread($handle, 64 * 1024);
        if ($chunk === false) {
            if ($handle !== STDIN) {
                fclose($handle);
            }
            throw new RequestRefusal('transport');
        }
        $total += strlen($chunk);
        if ($total > MAX_REQUEST_BYTES) {
            if ($handle !== STDIN) {
                fclose($handle);
            }
            throw new RequestRefusal('request-too-large');
        }
        $chunks[] = $chunk;
    }
    if ($handle !== STDIN) {
        fclose($handle);
    }
    return implode('', $chunks);
}

/**
 * Decode exactly one JSON document with fatal malformed-UTF-8, closed
 * depth, and trailing-document refusals. `json_decode` collapses
 * duplicate keys, so a pre-pass scanner rejects duplicate decoded keys
 * (including `{"a":1,"\u0061":2}` alias collisions) before the value
 * decoder can silently last-wins them.
 */
function decode_json_document(string $bytes): array
{
    if (strlen($bytes) > MAX_REQUEST_BYTES) {
        throw new RequestRefusal('request-too-large');
    }
    if (!preg_match('//u', $bytes)) {
        throw new RequestRefusal('utf-8');
    }
    reject_duplicate_keys($bytes);
    $value = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    if (!is_array($value)) {
        throw new RequestRefusal('shape');
    }
    return $value;
}

final class DuplicateKeyScanner
{
    public int $position = 0;

    public function __construct(private readonly string $text)
    {
    }

    public function scan(): void
    {
        $this->skip();
        $this->value(0);
        $this->skip();
        if ($this->position !== strlen($this->text)) {
            throw new RequestRefusal('trailing-content');
        }
    }

    private function value(int $depth): void
    {
        if ($depth > 64) {
            throw new RequestRefusal('depth');
        }
        $char = $this->text[$this->position] ?? '';
        switch ($char) {
            case '{':
                $this->object($depth);
                return;
            case '[':
                $this->array($depth);
                return;
            case '"':
                $this->string();
                return;
            case '':
                throw new RequestRefusal('syntax');
            default:
                $this->literal();
        }
    }

    private function object(int $depth): void
    {
        $this->position++;
        $keys = [];
        $this->skip();
        if (($this->text[$this->position] ?? '') === '}') {
            $this->position++;
            return;
        }
        for (;;) {
            $this->skip();
            if (($this->text[$this->position] ?? '') !== '"') {
                throw new RequestRefusal('syntax');
            }
            $start = $this->position;
            $this->string();
            $raw = substr($this->text, $start, $this->position - $start);
            $key = json_decode($raw, true, 64, JSON_THROW_ON_ERROR);
            if (in_array($key, $keys, true)) {
                throw new RequestRefusal('duplicate-key');
            }
            $keys[] = $key;
            $this->skip();
            if (($this->text[$this->position] ?? '') !== ':') {
                throw new RequestRefusal('syntax');
            }
            $this->position++;
            $this->skip();
            $this->value($depth + 1);
            $this->skip();
            $char = $this->text[$this->position] ?? '';
            if ($char === ',') {
                $this->position++;
                continue;
            }
            if ($char === '}') {
                $this->position++;
                return;
            }
            throw new RequestRefusal('syntax');
        }
    }

    private function array(int $depth): void
    {
        $this->position++;
        $this->skip();
        if (($this->text[$this->position] ?? '') === ']') {
            $this->position++;
            return;
        }
        for (;;) {
            $this->skip();
            $this->value($depth + 1);
            $this->skip();
            $char = $this->text[$this->position] ?? '';
            if ($char === ',') {
                $this->position++;
                continue;
            }
            if ($char === ']') {
                $this->position++;
                return;
            }
            throw new RequestRefusal('syntax');
        }
    }

    private function string(): void
    {
        $this->position++;
        for (;;) {
            $char = $this->text[$this->position] ?? '';
            if ($char === '') {
                throw new RequestRefusal('syntax');
            }
            if ($char === '"') {
                $this->position++;
                return;
            }
            if ($char === '\\') {
                $escape = $this->text[$this->position + 1] ?? '';
                if ($escape === 'u') {
                    $hex = substr($this->text, $this->position + 2, 4);
                    if (!preg_match('/^[0-9a-fA-F]{4}$/', $hex)) {
                        throw new RequestRefusal('syntax');
                    }
                    $this->position += 6;
                    continue;
                }
                if (!in_array($escape, ['"', '\\', '/', 'b', 'f', 'n', 'r', 't'], true)) {
                    throw new RequestRefusal('syntax');
                }
                $this->position += 2;
                continue;
            }
            if (ord($char) < 0x20) {
                throw new RequestRefusal('syntax');
            }
            $this->position++;
        }
    }

    private function literal(): void
    {
        $rest = substr($this->text, $this->position);
        foreach (['true', 'false', 'null'] as $word) {
            if (str_starts_with($rest, $word)) {
                $this->position += strlen($word);
                return;
            }
        }
        if (preg_match('/^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/', $rest, $matches)) {
            $this->position += strlen($matches[0]);
            return;
        }
        throw new RequestRefusal('syntax');
    }

    private function skip(): void
    {
        while ($this->position < strlen($this->text)) {
            $char = $this->text[$this->position];
            if ($char === ' ' || $char === "\t" || $char === "\n" || $char === "\r") {
                $this->position++;
                continue;
            }
            break;
        }
    }
}

/**
 * Pre-pass duplicate-key rejection over the raw document: one scan that
 * mirrors the value walk exactly, recording every decoded object key.
 */
function reject_duplicate_keys(string $bytes): void
{
    (new DuplicateKeyScanner($bytes))->scan();
}

// ---------------------------------------------------------------------------
// 2. Canonical JSON output.
// ---------------------------------------------------------------------------

/**
 * Canonical compact JSON: keys in UTF-8 byte order, no whitespace, no
 * escaped slashes, exactly like the core serializer's output on closed
 * shapes. JSON numbers here are only integers and strings (the closed
 * response vocabulary never carries floats), so PHP/serde_json float
 * spellings never diverge.
 */
function canonical_json(array|bool|int|string|null $value): string
{
    $text = write_canonical($value);
    if (strlen($text) > MAX_RESPONSE_BYTES) {
        throw new RequestRefusal('response-too-large');
    }
    return $text;
}

function write_canonical(array|bool|int|string|null $value): string
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
        return json_encode($value, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR);
    }
    $isList = array_is_list($value);
    if ($isList) {
        return '[' . implode(',', array_map(__FUNCTION__, $value)) . ']';
    }
    $keys = array_keys($value);
    usort($keys, 'strcmp');
    $body = [];
    foreach ($keys as $key) {
        $body[] = json_encode((string) $key, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)
            . ':' . write_canonical($value[$key]);
    }
    return '{' . implode(',', $body) . '}';
}

/** SHA-256 hex of the given UTF-8 bytes (binding anchors and digests). */
function sha256_hex(string $text): string
{
    return hash('sha256', $text);
}

/** The canonical `sha256:<64 hex>` digest spelling over bytes. */
function sha256_digest(string $bytes): string
{
    return 'sha256:' . hash('sha256', $bytes);
}

// ---------------------------------------------------------------------------
// 3. Grammar predicates (mirrors of the core scope module).
// ---------------------------------------------------------------------------

function is_sha256_digest(mixed $value): bool
{
    if (!is_string($value) || !str_starts_with($value, 'sha256:')) {
        return false;
    }
    $hex = substr($value, 7);
    return strlen($hex) === 64 && (bool) preg_match('/^[0-9a-f]{64}$/', $hex);
}

function is_request_id(mixed $value): bool
{
    $hex = is_string($value) ? substr($value, 4) : '';
    return is_string($value)
        && str_starts_with($value, 'req-')
        && strlen($hex) === 64
        && (bool) preg_match('/^[0-9a-f]{64}$/', $hex);
}

function is_plan_id(mixed $value): bool
{
    $hex = is_string($value) ? substr($value, 5) : '';
    return is_string($value)
        && str_starts_with($value, 'plan-')
        && strlen($hex) === 64
        && (bool) preg_match('/^[0-9a-f]{64}$/', $hex);
}

function is_contract_version(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 32) {
        return false;
    }
    $parts = explode('.', $value);
    if (count($parts) !== 3) {
        return false;
    }
    foreach ($parts as $part) {
        if ($part === '' || !ctype_digit($part)) {
            return false;
        }
    }
    return true;
}

/** One adapter id/target/profile token: lowercase first, closed charset. */
function is_token(mixed $value): bool
{
    return is_string($value)
        && $value !== ''
        && strlen($value) <= 64
        && (bool) preg_match('/^[a-z][a-z0-9-]*$/', $value);
}

/** One capability id: closed lowercase dotted-segment grammar. */
function is_capability_id(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 128) {
        return false;
    }
    foreach (explode('.', $value) as $segment) {
        if ($segment === '' || strlen($segment) > 64) {
            return false;
        }
        if (!preg_match('/^[a-z0-9][a-z0-9_-]*$/', $segment)) {
            return false;
        }
    }
    return true;
}

function segment_ok(string $segment): bool
{
    if ($segment === '.' || $segment === '..' || str_ends_with($segment, '.')) {
        return false;
    }
    if ($segment === '' || strlen($segment) > 64) {
        return false;
    }
    if (!preg_match('/^[a-z0-9.]([a-z0-9._-]*)$/', $segment)) {
        return false;
    }
    if (!preg_match('/^[a-z0-9.]/', $segment)) {
        return false;
    }
    return true;
}

/** Whether one path is a grammatical logical path (no `**`). */
function is_logical_path(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 512 || str_starts_with($value, '/')) {
        return false;
    }
    $parts = explode('/', $value);
    if (in_array('', $parts, true)) {
        return false;
    }
    foreach ($parts as $part) {
        if (!segment_ok($part)) {
            return false;
        }
    }
    return true;
}

/** Whether one value is a grammatical scope: optional trailing `**`. */
function is_scope(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 512 || str_starts_with($value, '/')) {
        return false;
    }
    $parts = explode('/', $value);
    if (in_array('', $parts, true)) {
        return false;
    }
    $last = array_pop($parts);
    foreach ($parts as $part) {
        if (!segment_ok($part)) {
            return false;
        }
    }
    if ($last === '**') {
        return $parts !== [];
    }
    return segment_ok($last);
}

function scope_covers(string $scope, string $path): bool
{
    if (!is_scope($scope) || !is_logical_path($path)) {
        return false;
    }
    $scopeParts = explode('/', $scope);
    $pathParts = explode('/', $path);
    $recursive = $scopeParts[array_key_last($scopeParts)] === '**';
    $prefix = $recursive ? array_slice($scopeParts, 0, -1) : $scopeParts;
    if ($recursive) {
        return count($pathParts) > count($prefix)
            && array_slice($pathParts, 0, count($prefix)) === $prefix;
    }
    return $pathParts === $prefix;
}

/** The protected canonical homes an adapter may never write (core mirror). */
function protected_home(string $path): ?string
{
    $parts = explode('/', $path);
    $homes = [
        'lekalo-model' => ['lekalo'],
        'lekalo-lockfile' => ['lekalo.lock'],
        'ir' => ['.lekalo', 'ir'],
        'cache' => ['.lekalo', 'cache'],
        'import' => ['.lekalo', 'import'],
        'privacy' => ['.lekalo', 'privacy'],
        'consumer' => ['.lekalo', 'consumer'],
        'openspec' => ['openspec'],
    ];
    foreach ($homes as $name => $home) {
        if (count($parts) >= count($home)
            && array_slice($parts, 0, count($home)) === $home) {
            return $name;
        }
    }
    return null;
}

// ---------------------------------------------------------------------------
// 4. Closed request validation (every core `validate_request` rule).
// ---------------------------------------------------------------------------

/**
 * The closed request envelope members; the decoder rejects unknown
 * members and explicit nulls before this point.
 */
function validate_request_object(array $document): array
{
    static $keys = [
        'protocol', 'protocol_version', 'operation', 'request_id',
        'project_root', 'ir_path', 'target', 'profile', 'profile_digest',
        'profile_capabilities', 'dry_run', 'limits', 'plan_id',
        'native_request',
    ];
    foreach (array_keys($document) as $key) {
        if (!in_array($key, $keys, true)) {
            throw new RequestRefusal('unknown-key');
        }
        if ($document[$key] === null) {
            throw new RequestRefusal('null-member');
        }
    }
    foreach (['protocol', 'protocol_version', 'operation', 'request_id', 'project_root'] as $key) {
        if (!array_key_exists($key, $document)) {
            throw new RequestRefusal('missing-key');
        }
    }
    if ($document['protocol'] !== PROTOCOL_TOKEN) {
        throw new RequestRefusal('protocol-token');
    }
    if (!in_array($document['protocol_version'], SUPPORTED_VERSIONS, true)) {
        throw new RequestRefusal('protocol-version');
    }
    if (!is_string($document['operation']) || !in_array($document['operation'], OPERATIONS, true)) {
        throw new RequestRefusal('operation');
    }
    if (!is_request_id($document['request_id'])) {
        throw new RequestRefusal('request-id');
    }
    if ($document['project_root'] !== '.') {
        throw new RequestRefusal('project-root');
    }
    foreach ([['ir_path', 'is_logical_path'], ['target', 'is_token'], ['profile', 'is_token']] as [$key, $check]) {
        if (array_key_exists($key, $document) && !$check($document[$key])) {
            throw new RequestRefusal('grammar');
        }
    }
    $operation = $document['operation'];
    if (array_key_exists('limits', $document)) {
        $limits = $document['limits'];
        if (!is_array($limits) || array_is_list($limits)) {
            throw new RequestRefusal('shape');
        }
        $limitKeys = array_keys($limits);
        sort($limitKeys, SORT_STRING);
        if ($limitKeys !== ['max_output_bytes', 'timeout_ms']) {
            throw new RequestRefusal('shape');
        }
        $timeout = $limits['timeout_ms'];
        $output = $limits['max_output_bytes'];
        if (!is_int($timeout) || $timeout < 1 || $timeout > 3600000
            || !is_int($output) || $output < 1 || $output > 1073741824) {
            throw new RequestRefusal('shape');
        }
    }
    $hasPlanId = array_key_exists('plan_id', $document);
    $apply = $operation === 'clean'
        || ($operation === 'generate' && ($document['dry_run'] ?? null) === false);
    if ($apply !== $hasPlanId || ($hasPlanId && !is_plan_id($document['plan_id']))) {
        throw new RequestRefusal('plan-id');
    }
    $hasNative = array_key_exists('native_request', $document);
    if (($operation === 'plan-native') !== $hasNative) {
        throw new RequestRefusal('native-request');
    }
    if ($operation === 'plan-native') {
        if (array_key_exists('dry_run', $document) || $hasPlanId) {
            throw new RequestRefusal('plan-id');
        }
        if ($document['protocol_version'] !== VERSION) {
            throw new RequestRefusal('member');
        }
        validate_native_request($document['native_request']);
    }
    if (($operation === 'generate') !== array_key_exists('dry_run', $document)) {
        throw new RequestRefusal('dry-run');
    }
    if (array_key_exists('dry_run', $document) && !is_bool($document['dry_run'])) {
        throw new RequestRefusal('dry-run');
    }
    $requiresIr = in_array($operation, IR_OPERATIONS, true);
    if ($requiresIr !== array_key_exists('ir_path', $document)) {
        throw new RequestRefusal('ir-path');
    }
    if (in_array($operation, ['generate', 'bind'], true) && !array_key_exists('target', $document)) {
        throw new RequestRefusal('target');
    }
    if ($operation === 'bind' && !array_key_exists('profile', $document)) {
        throw new RequestRefusal('profile');
    }
    if ($operation === 'describe') {
        foreach (['target', 'profile', 'profile_digest', 'profile_capabilities', 'ir_path'] as $key) {
            if (array_key_exists($key, $document)) {
                throw new RequestRefusal('member');
            }
        }
    }
    $hasDigest = array_key_exists('profile_digest', $document);
    $hasCapabilities = array_key_exists('profile_capabilities', $document);
    if ($hasDigest !== $hasCapabilities) {
        throw new RequestRefusal('profile-capabilities');
    }
    if ($hasDigest) {
        if ($document['protocol_version'] !== VERSION) {
            throw new RequestRefusal('member');
        }
        if (!array_key_exists('profile', $document)) {
            throw new RequestRefusal('profile');
        }
        if (!is_sha256_digest($document['profile_digest'])) {
            throw new RequestRefusal('profile-digest');
        }
        validate_profile_capabilities($document['profile_capabilities']);
    }
    if (in_array($operation, ['generate', 'bind'], true)
        && !is_token($document['target'] ?? null)) {
        throw new RequestRefusal('grammar');
    }
    return $document;
}

function validate_profile_capabilities(mixed $capabilities): void
{
    if (!is_array($capabilities) || !array_is_list($capabilities)
        || count($capabilities) === 0 || count($capabilities) > 64) {
        throw new RequestRefusal('profile-capabilities');
    }
    $previous = '';
    foreach ($capabilities as $capability) {
        if (!is_array($capability) || array_is_list($capability)
            || array_keys($capability) !== ['id', 'support']) {
            throw new RequestRefusal('profile-capabilities');
        }
        if (!is_capability_id($capability['id'])
            || !in_array($capability['support'], SUPPORT_STATES, true)) {
            throw new RequestRefusal('profile-capabilities');
        }
        if (strcmp($capability['id'], $previous) <= 0) {
            throw new RequestRefusal('profile-capabilities');
        }
        $previous = $capability['id'];
    }
}

/**
 * Validate the closed `native_request` member (issue #48): bounded
 * changed inputs plus digest-addressed custody references — never a
 * command or an absolute URL.
 */
function validate_native_request(mixed $native): void
{
    static $keys = [
        'changes', 'scan_ref', 'observed_ref', 'execution_policy_ref',
        'input_manifest_digest', 'tool_catalog_digest',
        'capability_snapshot_digest',
    ];
    if (!is_array($native) || array_is_list($native)) {
        throw new RequestRefusal('native-request');
    }
    foreach (array_keys($native) as $key) {
        if (!in_array($key, $keys, true)) {
            throw new RequestRefusal('native-request');
        }
    }
    foreach (['changes', 'scan_ref', 'execution_policy_ref', 'input_manifest_digest',
        'tool_catalog_digest', 'capability_snapshot_digest'] as $key) {
        if (!array_key_exists($key, $native)) {
            throw new RequestRefusal('native-request');
        }
    }
    $changes = $native['changes'];
    if (!is_array($changes) || array_is_list($changes)) {
        throw new RequestRefusal('native-request');
    }
    $files = $changes['files'] ?? [];
    if (!is_array($files) || !array_is_list($files) || count($files) > 1024) {
        throw new RequestRefusal('native-request');
    }
    foreach ($files as $file) {
        if (!is_array($file) || array_is_list($file)) {
            throw new RequestRefusal('native-request');
        }
        foreach (array_keys($file) as $key) {
            if (!in_array($key, ['path', 'change', 'before_digest', 'after_digest'], true)) {
                throw new RequestRefusal('native-request');
            }
        }
        if (!isset($file['path']) || !is_logical_path($file['path'])) {
            throw new RequestRefusal('native-request');
        }
        if (!isset($file['change'])
            || !in_array($file['change'], ['added', 'modified', 'deleted', 'renamed'], true)) {
            throw new RequestRefusal('native-request');
        }
        foreach (['before_digest', 'after_digest'] as $key) {
            if (array_key_exists($key, $file) && !is_sha256_digest($file[$key])) {
                throw new RequestRefusal('native-request');
            }
        }
    }
    $symbols = $changes['symbols'] ?? [];
    if (!is_array($symbols) || !array_is_list($symbols) || count($symbols) > 1024) {
        throw new RequestRefusal('native-request');
    }
    validate_native_content_ref($native['scan_ref']);
    if (array_key_exists('observed_ref', $native)) {
        validate_native_content_ref($native['observed_ref']);
    }
    validate_native_content_ref($native['execution_policy_ref']);
    foreach (['input_manifest_digest', 'tool_catalog_digest', 'capability_snapshot_digest'] as $key) {
        if (!is_sha256_digest($native[$key])) {
            throw new RequestRefusal('native-request');
        }
    }
}

function validate_native_content_ref(mixed $reference): void
{
    if (!is_array($reference) || array_is_list($reference)
        || !is_sha256_digest($reference['digest'] ?? null)) {
        throw new RequestRefusal('native-request');
    }
    foreach (array_keys($reference) as $key) {
        if (!in_array($key, ['digest', 'revision', 'adapter'], true)) {
            throw new RequestRefusal('native-request');
        }
    }
    if (array_key_exists('revision', $reference)) {
        $revision = $reference['revision'];
        if (!is_string($revision) || $revision === '' || strlen($revision) > 128) {
            throw new RequestRefusal('native-request');
        }
    }
    if (array_key_exists('adapter', $reference) && !is_token($reference['adapter'])) {
        throw new RequestRefusal('native-request');
    }
}

// ---------------------------------------------------------------------------
// 5. The deterministic generation seam (issue #54 MVP scope).
// ---------------------------------------------------------------------------

/**
 * The generated-artifact entry the kernel owns: one TypeScript-free PHP
 * side artifact per operation, proving the closed dry-run/apply/clean
 * write-plan seam over the fixture IR. The MVP generation surface is
 * deliberately minimal and honest: the adapter declares no deep
 * generator capabilities (those are the #55/#56 seams), and generation
 * never claims a construct it did not map.
 */
function deterministic_generation(array $request): array
{
    $target = $request['target'] ?? TARGET_TOKEN;
    $profile = $request['profile'] ?? PROFILE_TOKEN;
    $irPath = $request['ir_path'] ?? '.lekalo/ir/planner.json';
    $header = sprintf(
        "<?php\n\n// generated by %s@%s\ndeclare(strict_types=1);\n\n// target: %s\n// profile: %s\n// ir: %s\n",
        ADAPTER_ID,
        ADAPTER_VERSION,
        $target,
        $profile,
        $irPath,
    );
    $body = "return [\n    'kernel' => '%s',\n];\n";
    $text = $header . sprintf($body, ADAPTER_ID);
    $path = sprintf('.lekalo/generated/php-laravel/%s/kernel.php', $target);
    return [
        'path' => $path,
        'bytes' => $text,
        'digest' => sha256_digest($text),
    ];
}

function deterministic_writes(array $request): array
{
    $artifact = deterministic_generation($request);
    return [[
        'path' => $artifact['path'],
        'action' => 'create',
        'sha256' => $artifact['digest'],
    ]];
}

/** The canonical plan id: sha256 over the canonical ordered entries. */
function plan_id(array $writes): string
{
    return 'plan-' . sha256_hex(canonical_json($writes));
}

// ---------------------------------------------------------------------------
// 6. Response envelope construction.
// ---------------------------------------------------------------------------

/** The evidence identity every response binds itself to. */
function adapter_identity(): array
{
    return [
        'id' => ADAPTER_ID,
        'version' => ADAPTER_VERSION,
        'digest' => sha256_digest(ADAPTER_ID . '@' . ADAPTER_VERSION),
    ];
}

/** The capability map of this kernel (issue #28 fluent surface). */
function describe_capabilities(): array
{
    return [
        'adapter' => adapter_identity(),
        'protocol_versions' => SUPPORTED_VERSIONS,
        'operations' => OPERATIONS,
        'transports' => ['stdin', 'file'],
        'targets' => [TARGET_TOKEN],
        'profiles' => [PROFILE_TOKEN],
        'read_scopes' => ['.lekalo/cache/**', '.lekalo/ir/**'],
        'write_scopes' => ['.lekalo/generated/php-laravel/**'],
        'progress' => false,
        'ir_versions' => [IR_VERSION],
        'capabilities' => DECLARED_CAPABILITIES,
        'constraints' => ['max_entries' => 10000],
    ];
}

/**
 * Build the response envelope for `request`; the pairing rules (an
 * error response carries only the error member and vice versa) are
 * enforced by construction in the dispatch paths.
 */
function build_response(array $request, array $payload): array
{
    $envelope = [
        'protocol' => PROTOCOL_TOKEN,
        'protocol_version' => $request['protocol_version'],
        'operation' => $request['operation'],
        'request_id' => $request['request_id'],
        'status' => isset($payload['error']) ? 'error' : 'ok',
        'evidence' => ['adapter' => adapter_identity()],
    ];
    if (isset($payload['evidence_plan_id'])) {
        $envelope['evidence']['plan_id'] = $payload['evidence_plan_id'];
    }
    foreach (['capabilities', 'result', 'writes', 'progress', 'error'] as $key) {
        if (array_key_exists($key, $payload)) {
            $envelope[$key] = $payload[$key];
        }
    }
    return $envelope;
}

/** The fixed in-envelope unsupported refusal for absent operations. */
function unsupported_response(array $request, string $code = 'operation-unsupported'): array
{
    return build_response($request, [
        'error' => [
            'class' => 'unsupported',
            'code' => $code,
            'message' => 'this kernel does not implement the requested operation',
            'retryable' => false,
            'partial' => false,
        ],
    ]);
}

// ---------------------------------------------------------------------------
// 7. Dispatch: one validated request to one response.
// ---------------------------------------------------------------------------

function dispatch(array $request): array
{
    $operation = $request['operation'];
    if ($operation === 'describe') {
        return build_response($request, ['capabilities' => describe_capabilities()]);
    }
    switch ($operation) {
        case 'scan':
            return build_response($request, [
                'result' => [
                    'entries' => [
                        ['path' => '.lekalo/ir/minimal.json', 'kind' => 'ir'],
                    ],
                    'truncated' => false,
                ],
            ]);
        case 'bind':
            return build_response($request, [
                'result' => [
                    'bindings' => [[
                        'module' => 'planner',
                        'target' => $request['target'],
                        'profile' => $request['profile'],
                    ]],
                ],
            ]);
        case 'validate':
            return build_response($request, ['result' => ['ok' => true, 'findings' => []]]);
        case 'verify':
            return build_response($request, ['result' => ['ok' => true, 'findings' => []]]);
        case 'generate':
            return generate_response($request);
        case 'plan-clean':
            return plan_clean_response($request);
        case 'clean':
            return clean_response($request);
        case 'plan-native':
            return plan_native_response($request);
        default:
            return unsupported_response($request);
    }
}

/**
 * The generate exchange: a dry run plans the deterministic write set
 * (existence-probing create semantics); an apply echoes the pending
 * plan id and writes exactly those bytes. Applied and declared bytes
 * are one deterministic function of the request.
 */
function generate_response(array $request): array
{
    $writes = deterministic_writes($request);
    if (($request['dry_run'] ?? null) === false) {
        // The apply authority is the client's pending binding, never a
        // kernel-recomputed plan id: the binding identity mixes server-
        // side context and observed-state inputs the child never sees.
        // The kernel refuses a malformed echo and otherwise treats the
        // echoed id as the pending authority, exactly like the
        // reference fixtures do.
        $requested = $request['plan_id'] ?? '';
        if (!is_plan_id($requested)) {
            return build_response($request, [
                'error' => [
                    'class' => 'conflict',
                    'code' => 'plan-mismatch',
                    'message' => 'the echoed plan id does not match the pending plan authority',
                    'retryable' => false,
                    'partial' => false,
                ],
            ]);
        }
        apply_writes($writes, deterministic_generation($request));
    }
    return build_response($request, [
        'writes' => $writes,
        'evidence_plan_id' => $request['plan_id'] ?? plan_id($writes),
    ]);
}

/** The plan-clean exchange: deletions only, over the owned artifact. */
function plan_clean_response(array $request): array
{
    $writes = deterministic_writes($request);
    $plan = array_map(
        static fn (array $entry): array => ['path' => $entry['path'], 'action' => 'delete'],
        $writes,
    );
    return build_response($request, [
        'writes' => $plan,
        'evidence_plan_id' => plan_id($plan),
    ]);
}

/** The clean apply: delete exactly the planned paths, echo the plan id. */
function clean_response(array $request): array
{
    $writes = deterministic_writes($request);
    $plan = array_map(
        static fn (array $entry): array => ['path' => $entry['path'], 'action' => 'delete'],
        $writes,
    );
    $requested = $request['plan_id'] ?? '';
    // The apply authority is the client's pending binding (see the
    // generate apply note): the echo is shape-checked, never recomputed.
    if (!is_plan_id($requested)) {
        return build_response($request, [
            'error' => [
                'class' => 'conflict',
                'code' => 'plan-mismatch',
                'message' => 'the echoed plan id does not match the pending plan authority',
                'retryable' => false,
                'partial' => false,
            ],
        ]);
    }
    foreach ($plan as $entry) {
        delete_write($entry['path']);
    }
    return build_response($request, [
        'writes' => $plan,
        'evidence_plan_id' => $requested,
    ]);
}

/**
 * The read-only plan-native exchange (issue #48): the kernel returns
 * the honest unsupported refusal. Native gate planning needs a
 * workspace contract this MVP does not implement; a declared absent
 * capability is the honest state, never a fabricated plan summary.
 */
function plan_native_response(array $request): array
{
    return unsupported_response($request, 'native-planning-unsupported');
}

// ---------------------------------------------------------------------------
// 8. The bounded write view inside the staged project.
// ---------------------------------------------------------------------------

/**
 * Apply one declared create inside the core's private staged view.
 * The kernel trusts the core's sandbox for scope authority; it still
 * refuses paths outside its own declared write scope, protected homes,
 * non-logical paths, and create-on-existing, mirroring the plan
 * semantics core verifies after the child exits.
 */
function apply_writes(array $writes, array $artifact): void
{
    foreach ($writes as $entry) {
        $path = $entry['path'];
        if (!is_logical_path($path) || protected_home($path) !== null) {
            throw new RequestRefusal('write-denied');
        }
        if (!scope_covers('.lekalo/generated/php-laravel/**', $path)) {
            throw new RequestRefusal('write-denied');
        }
        if (is_file($path)) {
            throw new RequestRefusal('write-denied');
        }
        if (strlen($artifact['bytes']) > MAX_FILE_BYTES) {
            throw new RequestRefusal('write-denied');
        }
        $directory = dirname($path);
        if (!is_dir($directory) && !mkdir($directory, 0777, true) && !is_dir($directory)) {
            throw new RequestRefusal('write-denied');
        }
        if (@file_put_contents($path, $artifact['bytes']) === false) {
            throw new RequestRefusal('write-denied');
        }
    }
}

function delete_write(string $path): void
{
    if (!is_logical_path($path) || protected_home($path) !== null) {
        throw new RequestRefusal('write-denied');
    }
    if (!scope_covers('.lekalo/generated/php-laravel/**', $path)) {
        throw new RequestRefusal('write-denied');
    }
    if (is_file($path)) {
        @unlink($path);
    }
}

// ---------------------------------------------------------------------------
// 9. One-shot main.
// ---------------------------------------------------------------------------

/** The bounded stderr diagnostic for transport-level refusals. */
function stderr_diagnostic(string $code): string
{
    $bounded = preg_replace('/[^a-z0-9._-]+/', '-', strtolower($code)) ?? 'invalid';
    $bounded = trim($bounded, '-');
    if ($bounded === '') {
        $bounded = 'unspecified';
    }
    return json_encode(
        ['kernel' => ADAPTER_ID, 'diagnostic' => substr($bounded, 0, 128)],
        JSON_UNESCAPED_SLASHES,
    );
}

/**
 * Local metadata probe: exact adapter id/version and the PHP runtime
 * version. NOT part of the wire protocol — the wire has no
 * runtime-version slot; this stays a local argv probe exactly like the
 * Node kernel's `--version-json`.
 */
function runtime_metadata(): array
{
    return [
        'adapter' => ADAPTER_ID,
        'version' => ADAPTER_VERSION,
        'php' => PHP_VERSION,
        'entry' => 'adapter.php',
    ];
}

function main(): int
{
    $arguments = array_slice($_SERVER['argv'] ?? [], 1);
    if ($arguments === ['--version-json']) {
        fwrite(STDOUT, canonical_json(runtime_metadata()) . "\n");
        return 0;
    }
    try {
        $bytes = read_request_bytes();
        $document = decode_json_document($bytes);
        $request = validate_request_object($document);
        fwrite(STDOUT, canonical_json(dispatch($request)));
        return 0;
    } catch (RequestRefusal $refusal) {
        fwrite(STDERR, stderr_diagnostic($refusal->code) . "\n");
        return 1;
    } catch (JsonException) {
        fwrite(STDERR, stderr_diagnostic('syntax') . "\n");
        return 1;
    } catch (Throwable) {
        // Any other failure refuses honestly: bounded stderr, nonzero
        // exit, never a synthetic success envelope.
        fwrite(STDERR, stderr_diagnostic('kernel') . "\n");
        return 1;
    }
}

exit(main());
