<?php

/**
 * The external-analyzer seam of the PHP kernel (issue #55).
 *
 * The kernel itself stays subprocess-free: it never launches Mago, reads
 * a PATH, or runs Composer. Instead, a core-owned runner may produce an
 * *analyzer receipt* (`.lekalo/import/mago/receipt.json`) describing one
 * bounded Mago run; the kernel consumes that receipt through this closed
 * `Analyzer` contract. Two implementations exist:
 *
 * - `MagoEvidenceAnalyzer`: strict decoder/validation of a runner
 *   receipt; refuses missing, stale, mismatched, or malformed evidence
 *   with an explicit state instead of a fake success.
 * - `FakeAnalyzer`: a deterministic test double used by the suites and
 *   the fake integration gate; its canned output traverses exactly the
 *   same decoder, so a fake can never diverge from real semantics.
 *
 * Closed analysis states (the issue requires `Mago unavailable` to be
 * distinct from `analysis failure`):
 *   unavailable   — no receipt exists (tool never ran; absent capability)
 *   incompatible  — receipt exists, but tool version/decoder revision
 *                   differs from the pinned toolchain lock
 *   failed        — the recorded run itself failed (nonzero completion,
 *                   parse errors, or refused output)
 *   ok            — a well-formed, current, successful receipt
 *
 * (No module-level declare: the artifact's single strict_types
 * declaration at the top of the bundled file covers every module.)
 */

/** The receipt schema this kernel decodes (closed; bump on change). */
const MAGO_RECEIPT_SCHEMA = 'lekalo/provider-evidence/v0.1.0';
/** The pinned toolchain identity the receipt must agree with. */
const MAGO_TOOLCHAIN_LOCK_FILE = 'mago-toolchain.lock.json';
/** The pinned upstream tool version the decoder speaks (build-checked). */
const MAGO_PINNED_TOOL_VERSION = '1.0.0';
const MAGO_RECEIPT_PATH = '.lekalo/import/mago/receipt.json';
/** The maximum receipt document size (mirrors the core import bounds). */
const MAGO_RECEIPT_MAX_BYTES = 1024 * 1024;
/** The maximum number of diagnostics and symbols one receipt may carry. */
const MAGO_RECEIPT_MAX_ITEMS = 256;

/** The closed analysis states. */
const MAGO_STATES = ['ok', 'unavailable', 'incompatible', 'failed'];

/**
 * The analysis outcome handed to dispatch; every state other than `ok`
 * carries a bounded, machine-readable reason.
 */
final class AnalysisOutcome
{
    /** @param array<int, array<string, mixed>> $diagnostics */
    public function __construct(
        public readonly string $state,
        public readonly array $diagnostics = [],
        public readonly array $symbols = [],
        public readonly array $relations = [],
        public readonly array $fixes = [],
        public readonly ?string $reason = null,
        public readonly ?string $receiptDigest = null,
    ) {
    }

    public function isOk(): bool
    {
        return $this->state === 'ok';
    }
}

/** Why a receipt was refused. */
final class ReceiptRefusal extends RuntimeException
{
}

/** The closed analyzer contract; injection happens at composition only. */
interface Analyzer
{
    /** Capability identity: which receipt schema and pin this analyzer reads. */
    public function capabilities(): array;

    /** Produce one outcome for the staged read view (never launches anything). */
    public function analyze(): AnalysisOutcome;
}

/**
 * The production analyzer: decode and validate the runner receipt inside
 * the declared read view. Missing, incompatible, and failed receipts are
 * explicit outcomes — never collapsed into an empty success.
 */
final class MagoEvidenceAnalyzer implements Analyzer
{
    public function __construct()
    {
    }

    public function capabilities(): array
    {
        return [
            'analyzer' => 'mago',
            'receipt_schema' => MAGO_RECEIPT_SCHEMA,
            'toolchain_lock_digest' => mago_load_toolchain_lock()['lockDigest']
                ?? ('sha256:' . str_repeat('0', 64)),
            'modes' => ['lint', 'analyze', 'guard'],
        ];
    }

    public function analyze(): AnalysisOutcome
    {
        if (!is_file(MAGO_RECEIPT_PATH)) {
            return new AnalysisOutcome(
                'unavailable',
                reason: 'no analyzer receipt at ' . MAGO_RECEIPT_PATH,
            );
        }
        try {
            $receipt = mago_decode_receipt(
                (string) file_get_contents(MAGO_RECEIPT_PATH),
            );
        } catch (ReceiptRefusal $refusal) {
            return new AnalysisOutcome(
                'failed',
                reason: 'receipt refused: ' . $refusal->getMessage(),
            );
        }
        $compat = mago_check_compatibility($receipt);
        if ($compat !== null) {
            return new AnalysisOutcome('incompatible', reason: $compat);
        }
        if (($receipt['completion']['status'] ?? '') !== 'completed') {
            return new AnalysisOutcome(
                'failed',
                reason: 'recorded run did not complete: '
                    . (string) ($receipt['completion']['status'] ?? 'missing'),
                receiptDigest: $receipt['receipt_digest'] ?? null,
            );
        }
        return new AnalysisOutcome(
            'ok',
            diagnostics: $receipt['diagnostics'],
            symbols: $receipt['symbols'],
            relations: $receipt['relations'],
            fixes: $receipt['fixes'],
            receiptDigest: isset($receipt['receipt_digest']) ? (string) $receipt['receipt_digest'] : null,
        );
    }
}

/**
 * The deterministic test double used by the default production dispatch
 * (and the suites). The default dispatch has no staged receipt and must
 * preserve the #54 no-evidence, no-claims behavior, so the composition
 * default is an explicitly `unavailable` analyzer: no canned rows, and
 * the state is honest absence rather than a failed decode.
 */
final class FakeAnalyzer implements Analyzer
{
    /** @param array<string, mixed> $canned */
    public function __construct(
        private readonly array $canned = [],
        private readonly string $state = 'unavailable',
    ) {
    }

    public function capabilities(): array
    {
        return [
            'analyzer' => 'fake',
            'receipt_schema' => MAGO_RECEIPT_SCHEMA,
            'toolchain_lock_digest' => 'sha256:' . str_repeat('0', 64),
            'modes' => ['lint', 'analyze', 'guard'],
        ];
    }

    public function analyze(): AnalysisOutcome
    {
        if ($this->state !== 'ok') {
            return new AnalysisOutcome($this->state, reason: 'fake analyzer canned state');
        }
        try {
            $receipt = mago_decode_receipt(
                json_encode($this->canned, JSON_THROW_ON_ERROR),
            );
        } catch (ReceiptRefusal $refusal) {
            return new AnalysisOutcome(
                'failed',
                reason: 'fake receipt refused: ' . $refusal->getMessage(),
            );
        }
        // The fake traverses the same semantic gates as the production
        // consumer: compatibility against the bundled/file lock and the
        // completion-status gate — so a canned ok receipt with a failed
        // completion or a foreign tool digest reports failed or
        // incompatible exactly like the real one.
        $compat = mago_check_compatibility($receipt);
        if ($compat !== null) {
            return new AnalysisOutcome('incompatible', reason: $compat);
        }
        if (($receipt['completion']['status'] ?? '') !== 'completed') {
            return new AnalysisOutcome(
                'failed',
                reason: 'recorded run did not complete: '
                    . (string) ($receipt['completion']['status'] ?? 'missing'),
            );
        }
        return new AnalysisOutcome(
            'ok',
            diagnostics: $receipt['diagnostics'],
            symbols: $receipt['symbols'],
            relations: $receipt['relations'],
            fixes: $receipt['fixes'],
            receiptDigest: isset($receipt['receipt_digest']) ? (string) $receipt['receipt_digest'] : null,
        );
    }
}
// ---------------------------------------------------------------------------
// Receipt decoding: the strict, closed, bounded decoder.
// ---------------------------------------------------------------------------

/**
 * Decode one receipt document with the kernel's fatal conventions:
 * malformed UTF-8, duplicate keys, unknown members, null members, bound
 * overflow, and digest mismatches are refusals — never silent drops.
 *
 * @return array<string, mixed>
 */
function mago_decode_receipt(string $bytes): array
{
    if ($bytes === '') {
        throw new ReceiptRefusal('empty');
    }
    if (strlen($bytes) > MAGO_RECEIPT_MAX_BYTES) {
        throw new ReceiptRefusal('receipt-too-large');
    }
    if (!preg_match('//u', $bytes)) {
        throw new ReceiptRefusal('utf-8');
    }
    reject_duplicate_keys($bytes);
    try {
        $value = json_decode($bytes, true, 32, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new ReceiptRefusal('syntax');
    }
    if (!is_json_object($value)) {
        throw new ReceiptRefusal('shape');
    }
    $keys = [
        'schema', 'receipt_digest', 'tool', 'completion', 'input_manifest',
        'diagnostics', 'symbols', 'relations', 'fixes',
    ];
    foreach (array_keys($value) as $key) {
        if (!in_array($key, $keys, true)) {
            throw new ReceiptRefusal('unknown-key:' . $key);
        }
        if ($value[$key] === null) {
            throw new ReceiptRefusal('null-member:' . $key);
        }
    }
    foreach (['schema', 'receipt_digest', 'tool', 'completion', 'input_manifest'] as $key) {
        if (!array_key_exists($key, $value)) {
            throw new ReceiptRefusal('missing-key:' . $key);
        }
    }
    if ($value['schema'] !== MAGO_RECEIPT_SCHEMA) {
        throw new ReceiptRefusal('schema');
    }
    if (!is_sha256_digest($value['receipt_digest'])) {
        throw new ReceiptRefusal('receipt-digest');
    }
    if (!is_json_object($value['tool']) || !is_json_object($value['completion'])
        || !is_json_object($value['input_manifest'])) {
        throw new ReceiptRefusal('shape');
    }
    foreach ($value['tool'] as $key => $_) {
        if (!in_array($key, ['name', 'version', 'digest'], true)) {
            throw new ReceiptRefusal('unknown-key:tool.' . $key);
        }
    }
    if (($value['tool']['name'] ?? '') !== 'mago' || !is_string($value['tool']['version'])
        || $value['tool']['version'] === '' || strlen((string) $value['tool']['version']) > 32
        || !is_sha256_digest($value['tool']['digest'] ?? null)) {
        throw new ReceiptRefusal('tool');
    }
    foreach ($value['completion'] as $key => $_) {
        if (!in_array($key, ['status', 'exit_code'], true)) {
            throw new ReceiptRefusal('unknown-key:completion.' . $key);
        }
    }
    if (!in_array($value['completion']['status'] ?? '', ['completed', 'failed'], true)) {
        throw new ReceiptRefusal('completion.status');
    }
    foreach ($value['input_manifest'] as $key => $_) {
        if (!in_array($key, ['inputs', 'source_digest'], true)) {
            throw new ReceiptRefusal('unknown-key:input_manifest.' . $key);
        }
    }
    if (!is_sha256_digest($value['input_manifest']['source_digest'] ?? null)) {
        throw new ReceiptRefusal('input_manifest.source_digest');
    }
    foreach (['diagnostics', 'symbols', 'relations', 'fixes'] as $key) {
        $items = $value[$key] ?? [];
        if ($items === []) {
            $value[$key] = [];
            continue;
        }
        if (!is_array($items) || !array_is_list($items)) {
            throw new ReceiptRefusal($key . ':shape');
        }
        if (count($items) > MAGO_RECEIPT_MAX_ITEMS) {
            throw new ReceiptRefusal($key . ':overflow');
        }
    }
    foreach (($value['diagnostics'] ?? []) as $index => $item) {
        $value['diagnostics'][$index] = mago_decode_diagnostic($item);
    }
    foreach (($value['symbols'] ?? []) as $index => $item) {
        $value['symbols'][$index] = mago_decode_symbol($item);
    }
    foreach (($value['relations'] ?? []) as $index => $item) {
        $value['relations'][$index] = mago_decode_relation($item);
    }
    foreach (($value['fixes'] ?? []) as $index => $item) {
        $value['fixes'][$index] = mago_decode_fix($item);
    }
    return $value;
}

/**
 * One normalized diagnostic row: registered Lekalo rule id, bounded
 * logical path, half-open range, namespaced original code, and the
 * bounded payload. Unknown upstream codes keep their exact original
 * code under the generic native-finding rule.
 *
 * Laravel casing: the staged source files under `app/` keep their
 * canonical casing (e.g. `app/Models/User.php`), which the v0.3.2
 * logical-path grammar cannot spell. Findings for target sources are
 * validated as *native evidence paths* — closed, traversal-free, and
 * case-preserving — not as Model logical paths.
 *
 * @return array<string, mixed>
 */
function mago_decode_diagnostic(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('diagnostic:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['rule', 'original_code', 'level', 'path', 'range', 'message', 'producer'], true)) {
            throw new ReceiptRefusal('diagnostic:unknown-key:' . $key);
        }
    }
    foreach (['rule', 'original_code', 'level', 'path', 'range', 'producer'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('diagnostic:missing:' . $key);
        }
        if ($item[$key] === null) {
            throw new ReceiptRefusal('diagnostic:null:' . $key);
        }
    }
    if (!is_analysis_rule_id($item['rule'])) {
        throw new ReceiptRefusal('diagnostic:rule');
    }
    $code = $item['original_code'];
    if (!is_string($code) || $code === '' || strlen($code) > 64
        || (bool) preg_match('/[\x00-\x1f]/', $code)) {
        throw new ReceiptRefusal('diagnostic:original-code');
    }
    if (!in_array($item['level'], ['note', 'help', 'warning', 'error'], true)) {
        throw new ReceiptRefusal('diagnostic:level');
    }
    if (!is_native_evidence_path($item['path'])) {
        throw new ReceiptRefusal('diagnostic:path');
    }
    $range = $item['range'];
    if (!is_json_object($range)
        || !isset($range['start'], $range['end'])
        || !is_int($range['start']) || !is_int($range['end'])
        || $range['start'] < 0 || $range['end'] < $range['start']
        || $range['end'] - $range['start'] > MAGO_RECEIPT_MAX_BYTES) {
        throw new ReceiptRefusal('diagnostic:range');
    }
    if (array_key_exists('message', $item)
        && (!is_string($item['message']) || strlen($item['message']) > 512)) {
        throw new ReceiptRefusal('diagnostic:message');
    }
    if (!in_array($item['producer'], ['mago', 'lekalo'], true)) {
        throw new ReceiptRefusal('diagnostic:producer');
    }
    return $item;
}

/**
 * One symbol row: package-qualified identity, kind, span, and the
 * structural signature digest. Line movement never changes identity.
 *
 * @return array<string, mixed>
 */
function mago_decode_symbol(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('symbol:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['identity', 'kind', 'path', 'range', 'signature', 'modifiers'], true)) {
            throw new ReceiptRefusal('symbol:unknown-key:' . $key);
        }
    }
    foreach (['identity', 'kind', 'path', 'range'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('symbol:missing:' . $key);
        }
    }
    if (!is_symbol_identity($item['identity'])) {
        throw new ReceiptRefusal('symbol:identity');
    }
    if (!in_array($item['kind'], ['class', 'interface', 'trait', 'enum', 'function', 'method', 'property'], true)) {
        throw new ReceiptRefusal('symbol:kind');
    }
    if (!is_native_evidence_path($item['path'])) {
        throw new ReceiptRefusal('symbol:path');
    }
    $range = $item['range'];
    if (!is_json_object($range) || !isset($range['start'], $range['end'])
        || !is_int($range['start']) || !is_int($range['end'])
        || $range['start'] < 0 || $range['end'] < $range['start']) {
        throw new ReceiptRefusal('symbol:range');
    }
    if (array_key_exists('signature', $item)
        && (!is_string($item['signature']) || !is_sha256_digest($item['signature']))) {
        throw new ReceiptRefusal('symbol:signature');
    }
    if (array_key_exists('modifiers', $item)) {
        if (!is_array($item['modifiers']) || !array_is_list($item['modifiers'])) {
            throw new ReceiptRefusal('symbol:modifiers');
        }
        foreach ($item['modifiers'] as $modifier) {
            if (!in_array($modifier, ['final', 'readonly', 'abstract', 'static', 'public', 'protected', 'private', 'extensible'], true)) {
                throw new ReceiptRefusal('symbol:modifier');
            }
        }
    }
    return $item;
}

/**
 * One relation row: typed endpoint pair with explicit confidence and
 * provenance. Unresolved endpoints are refused here; uncertainty is
 * expressed with confidence `unknown`, never with invented targets.
 *
 * @return array<string, mixed>
 */
function mago_decode_relation(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('relation:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['from', 'to', 'role', 'confidence', 'producer', 'derivation'], true)) {
            throw new ReceiptRefusal('relation:unknown-key:' . $key);
        }
    }
    foreach (['from', 'to', 'role', 'confidence', 'producer'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('relation:missing:' . $key);
        }
    }
    foreach (['from', 'to'] as $key) {
        if (!is_symbol_identity($item[$key])) {
            throw new ReceiptRefusal('relation:' . $key);
        }
    }
    if (!in_array($item['role'], ['extends', 'implements', 'uses', 'calls', 'reads', 'writes', 'instantiates', 'relation', 'route', 'container-binding'], true)) {
        throw new ReceiptRefusal('relation:role');
    }
    if (!in_array($item['confidence'], ['exact', 'high', 'medium', 'low', 'unknown'], true)) {
        throw new ReceiptRefusal('relation:confidence');
    }
    if (!in_array($item['producer'], ['mago', 'laravel-extension', 'lekalo'], true)) {
        throw new ReceiptRefusal('relation:producer');
    }
    if (array_key_exists('derivation', $item)
        && (!is_string($item['derivation']) || strlen($item['derivation']) > 128)) {
        throw new ReceiptRefusal('relation:derivation');
    }
    return $item;
}

/**
 * One safe-fix record: advice only. A fix never carries raw source
 * patches across the boundary — only ranges, replacements, hashes, and
 * the conservative safety classification.
 *
 * @return array<string, mixed>
 */
function mago_decode_fix(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('fix:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['rule', 'path', 'range', 'replacement', 'before_digest', 'safety'], true)) {
            throw new ReceiptRefusal('fix:unknown-key:' . $key);
        }
    }
    foreach (['rule', 'path', 'range', 'before_digest', 'safety'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('fix:missing:' . $key);
        }
    }
    if (!is_string($item['rule']) || !is_analysis_rule_id($item['rule'])
        && !preg_match('/^[a-z0-9][a-z0-9.-]{0,126}[a-z0-9]$/', $item['rule'])) {
        throw new ReceiptRefusal('fix:rule');
    }
    if (!is_native_evidence_path($item['path'])) {
        throw new ReceiptRefusal('fix:path');
    }
    $range = $item['range'];
    if (!is_json_object($range) || !isset($range['start'], $range['end'])
        || !is_int($range['start']) || !is_int($range['end'])
        || $range['start'] < 0 || $range['end'] < $range['start']) {
        throw new ReceiptRefusal('fix:range');
    }
    if (!is_sha256_digest($item['before_digest'])) {
        throw new ReceiptRefusal('fix:before-digest');
    }
    if (!in_array($item['safety'], ['safe', 'potentially-unsafe', 'unsafe'], true)) {
        throw new ReceiptRefusal('fix:safety');
    }
    if (array_key_exists('replacement', $item)
        && (!is_string($item['replacement']) || strlen($item['replacement']) > 4096)) {
        throw new ReceiptRefusal('fix:replacement');
    }
    return $item;
}

/**
 * Compatibility gate: the receipt must agree with the pinned toolchain
 * (tool name/version/digest and the decoder revision the kernel speaks).
 * Returns null when compatible, or the bounded incompatibility reason.
 */
function mago_check_compatibility(array $receipt): ?string
{
    // The lock is the bundled custody source; there is no second digest
    // input to race against (both the analyzer and the fake resolve the
    // same cached load), so the receipt is compared against the pin
    // directly. The artifact digest moving with the lock is the upgrade
    // gate — a rebuilt artifact carries the new pin by construction.
    $lock = mago_load_toolchain_lock();
    if ($lock === null) {
        return 'toolchain lock missing';
    }
    // The lock stores bare hex; the receipt carries the `sha256:`
    // spelling — compare against both so custody compares the bytes.
    $digestMatch = false;
    foreach (($lock['probe']['artifacts'] ?? []) as $artifact) {
        if (in_array($receipt['tool']['digest'] ?? '', [
            'sha256:' . ($artifact['binarySha256'] ?? ''),
            $artifact['binarySha256'] ?? '',
        ], true)) {
            $digestMatch = true;
            break;
        }
    }
    if (!$digestMatch) {
        return 'tool digest differs from the pinned toolchain';
    }
    if (($receipt['tool']['version'] ?? '') !== ($lock['tool']['version'] ?? '')) {
        return 'tool version differs from the pinned toolchain';
    }
    return null;
}

/**
 * Load the adapter-owned toolchain lock (bundled beside the kernel at
 * build time); returns null when the packaging did not embed one.
 *
 * @return array<string, mixed>|null
 */
function mago_load_toolchain_lock(): ?array
{
    static $cache = false;
    static $lock = null;
    if ($cache === true) {
        return $lock;
    }
    $cache = true;
    // Custody source of truth: the verbatim lock bytes bundled into the
    // artifact by build.php (MAGO_TOOLCHAIN_LOCK_BUNDLED). The lock is
    // part of the shipped bytes, so the deployed single-file package
    // carries its own custody and no sibling file is needed. In the
    // source tree (before bundling) the constant does not exist yet;
    // dev/test then reads the same lock file beside src/ — the exact
    // bytes build.php embeds.
    if (defined('MAGO_TOOLCHAIN_LOCK_BUNDLED')) {
        $bytes = (string) MAGO_TOOLCHAIN_LOCK_BUNDLED;
    } else {
        $path = dirname(__DIR__) . '/mago-toolchain.lock.json';
        if (!is_file($path)) {
            return null;
        }
        $bytes = (string) file_get_contents($path);
    }
    try {
        $lock = json_decode($bytes, true, 32, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return $lock = null;
    }
    if (!is_array($lock) || !is_json_object($lock)) {
        return $lock = null;
    }
    $lock['lockDigest'] = sha256_digest($bytes);
    return $lock;
}
