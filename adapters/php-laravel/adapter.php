<?php
/**
 * The committed single-file adapter artifact of `lekalo-target-php-laravel`
 * (issues #54, #55).
 *
 * GENERATED FILE — regenerate with `php adapters/php-laravel/build.php`;
 * verify with `php adapters/php-laravel/build.php --check`. Never edit.
 *
 * Self-contained: PHP built-ins only (json, hash, SPL), no Composer
 * package, no extension beyond always-compiled basics, so the confined
 * core runtime can copy the interpreter plus exactly this script.
 */

declare(strict_types=1);

// ---- bundled toolchain lock (issue #55) -------------------------
// EMBEDDED BY build.php from mago-toolchain.lock.json. Never edit.
// The exact bytes are part of this artifact, so the artifact digest
// changes whenever the supported toolchain changes (custody binds).
const MAGO_TOOLCHAIN_LOCK_BUNDLED = '{
  "schemaVersion": "lekalo/mago-toolchain-lock/v0.1.0",
  "identity": "dev.lekalo.mago-toolchain@0.1.0",
  "tool": {
    "name": "mago",
    "vendor": "carthage-software",
    "upstream": "https://github.com/carthage-software/mago",
    "license": "MIT OR Apache-2.0",
    "version": "1.0.0"
  },
  "probe": {
    "version": {
      "argv": ["mago", "--version"],
      "stdout": "mago 1.0.0"
    },
    "artifacts": [
      {
        "platform": "x86_64-pc-windows-msvc",
        "archive": "mago-1.0.0-x86_64-pc-windows-msvc.zip",
        "archiveSha256": "db898a5eea4b13529f202fa5968d2a17643b9c7109a5705001f466fddc07156b",
        "binary": "mago.exe",
        "binarySha256": "4702d8c00acb73f23624fa17ba771f5d0e060bb31f299d5b5dae49ece6f845fa"
      },
      {
        "platform": "x86_64-unknown-linux-gnu",
        "archive": "mago-1.0.0-x86_64-unknown-linux-gnu.tar.gz",
        "archiveSha256": "ea012e30d3c9f7b899bf6e2d3feaf61091c931bc3af248519fb01f2ff2375be4",
        "binary": "mago",
        "binarySha256": "582646d691f3307caf47cf23d6009aba949078ecc899bafd22c04a1634c185db"
      }
    ]
  },
  "compatibility": {
    "decoderRevision": "v1",
    "phpSyntax": [
      "7.2",
      "7.3",
      "7.4",
      "8.0",
      "8.1",
      "8.2",
      "8.3",
      "8.4",
      "8.5"
    ],
    "capabilities": {
      "syntaxAst": "full",
      "lint": "full",
      "analyze": "full",
      "guard": "full",
      "typeInference": "partial",
      "references": "partial",
      "incremental": "unknown",
      "fixPreview": "text-only",
      "fixApply": "out-of-scope"
    },
    "exitCodes": {
      "ok": 0,
      "findingsOrInfrastructure": 1,
      "usageOrConfiguration": 2
    }
  },
  "commands": {
    "lintRules": {
      "argv": ["mago", "lint", "--list-rules", "--json"],
      "exitCodes": [0]
    },
    "lintJson": {
      "argv": [
        "mago", "--workspace", ".", "lint",
        "--reporting-format", "json", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1],
      "notes": "exit 0 = clean or warning-only (default minimum-fail-level error); exit 1 = error-level findings or infrastructure failure; exit 2 = usage/config error"
    },
    "lintSarif": {
      "argv": [
        "mago", "--workspace", ".", "lint",
        "--reporting-format", "sarif", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "analyzeJson": {
      "argv": [
        "mago", "--workspace", ".", "analyze",
        "--reporting-format", "json", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "analyzeSarif": {
      "argv": [
        "mago", "--workspace", ".", "analyze",
        "--reporting-format", "sarif", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "guardJson": {
      "argv": [
        "mago", "--workspace", ".", "guard",
        "--reporting-format", "json", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "guardSarif": {
      "argv": [
        "mago", "--workspace", ".", "guard",
        "--reporting-format", "sarif", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "astJson": {
      "argv": ["mago", "--workspace", ".", "ast", "--json", "<FILE>"],
      "exitCodes": [0],
      "notes": "creates .mago cache; not used by the adapter"
    },
    "fixPreview": {
      "argv": [
        "mago", "--workspace", ".", "lint",
        "--fix", "--dry-run", "--unsafe", "--only", "<RULE>"
      ],
      "exitCodes": [0, 1],
      "notes": "unified diff on stdout; requires --unsafe because the strict-types insertion fix is classified potentially-unsafe; never combined with --reporting-format (clap refuses that pair)"
    }
  }
}
';

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

/**
 * The Lekalo strict-profile rules over the analyzer receipt (issue #55).
 *
 * The issue's target table maps each required rule to its evidence
 * source. Two implementation shapes exist and are kept strictly apart:
 *
 * - `lint` rows: a pinned Mago rule supplies the finding verbatim; the
 *   kernel only maps the original code into a registered rule id and
 *   keeps the exact original code in namespaced metadata.
 * - `predicate` rows: no upstream rule exists for the requirement, so
 *   the kernel evaluates a deterministic predicate over the receipt's
 *   symbol/AST evidence. When the prerequisite evidence is absent the
 *   row is `unsupported` — never a silent pass.
 *
 * Every row: compliant inputs produce no finding; violating inputs
 * produce one; evidence that cannot decide produces an explicit
 * `target.analysis.evidence-unsupported` diagnostic instead of a
 * fabricated pass or fail.
 */

/** The closed strict-profile row set (ids are kernel-stable). */
const STRICT_RULES = [
    'strict-types' => [
        'rule' => 'target.analysis.strict-types',
        'source' => 'lint',
        'mago_code' => 'strict-types',
    ],
    'final-readonly-profile' => [
        'rule' => 'target.analysis.final-readonly-profile',
        'source' => 'predicate',
    ],
    'no-dynamic-members' => [
        'rule' => 'target.analysis.no-dynamic-members',
        'source' => 'lint',
        'mago_code' => 'no-variable-variable',
    ],
    'no-service-locator' => [
        'rule' => 'target.analysis.no-service-locator',
        'source' => 'predicate',
    ],
    'no-magic-domain-state' => [
        'rule' => 'target.analysis.no-magic-domain-state',
        'source' => 'predicate',
    ],
    // Explicit nullable/array-shape/type coverage has no pinned upstream
    // rule AND no public shape-inference evidence in the receipt, so the
    // honest state is `unsupported` — never a re-mapped copy of the
    // strict-types diagnostics (which would double-report one rule).
    'explicit-types' => [
        'rule' => 'target.analysis.explicit-types',
        'source' => 'predicate',
    ],
];

/**
 * Evaluate the whole strict profile over one `ok` outcome. Returns the
 * normalizer-ready diagnostic rows (Lekalo rule ids, logical paths,
 * half-open ranges, namespaced original codes) plus one `unsupported`
 * row per rule whose prerequisite evidence was missing.
 *
 * @param array<int, array<string, mixed>> $diagnostics
 * @param array<int, array<string, mixed>> $symbols
 * @return array{findings: array<int, array<string, mixed>>, unsupported: array<int, string>}
 */
function strict_profile_evaluate(array $diagnostics, array $symbols): array
{
    $findings = [];
    $unsupported = [];
    foreach (STRICT_RULES as $id => $row) {
        if ($row['source'] === 'lint') {
            $matched = strict_map_lint_row($diagnostics, (string) $row['mago_code'], (string) $row['rule']);
            if ($matched === false) {
                $unsupported[] = (string) $id;
                continue;
            }
            foreach ($matched as $finding) {
                $findings[] = $finding;
            }
            continue;
        }
        $predicate = $id === 'final-readonly-profile'
            ? strict_predicate_final_readonly($symbols)
            : strict_predicate_unavailable((string) $id);
        if ($predicate === null) {
            $unsupported[] = (string) $id;
            continue;
        }
        foreach ($predicate as $finding) {
            $findings[] = $finding;
        }
    }
    return ['findings' => $findings, 'unsupported' => $unsupported];
}

/**
 * Map one Mago lint code onto its registered Lekalo row. `false` means
 * the prerequisite lint evidence was not in the receipt (the row's rule
 * never ran) — that is `unsupported`, not `compliant`.
 *
 * @param array<int, array<string, mixed>> $diagnostics
 * @return array<int, array<string, mixed>>|false
 */
function strict_map_lint_row(array $diagnostics, string $magoCode, string $ruleId): array|false
{
    $coverage = false;
    $findings = [];
    foreach ($diagnostics as $item) {
        if (($item['producer'] ?? '') !== 'mago') {
            continue;
        }
        if (($item['original_code'] ?? '') === $magoCode) {
            $coverage = true;
            $findings[] = $item;
        }
    }
    return $coverage ? $findings : false;
}

/**
 * The final/readonly profile predicate over symbol evidence: mutable
 * (non-final, non-abstract) domain classes are violations unless they
 * carry the explicit `extensible` marker in their modifiers. Symbol
 * evidence absent ⇒ `null` (unsupported), never a pass.
 *
 * @param array<int, array<string, mixed>> $symbols
 * @return array<int, array<string, mixed>>|null
 */
function strict_predicate_final_readonly(array $symbols): ?array
{
    $classes = array_values(array_filter(
        $symbols,
        static fn (array $symbol): bool => in_array($symbol['kind'], ['class', 'interface', 'trait', 'enum'], true),
    ));
    if ($classes === []) {
        return null;
    }
    $findings = [];
    foreach ($classes as $symbol) {
        $modifiers = $symbol['modifiers'] ?? [];
        $extensible = in_array('extensible', array_map('strval', $modifiers), true);
        if ($extensible || in_array('final', $modifiers, true) || in_array('abstract', $modifiers, true)) {
            continue;
        }
        $findings[] = [
            'rule' => 'target.analysis.final-readonly-profile',
            'original_code' => 'lekalo.final-profile',
            'level' => 'warning',
            'path' => $symbol['path'],
            'range' => $symbol['range'],
            'message' => 'domain class ' . (string) $symbol['identity'] . ' is neither final nor explicitly extensible',
            'producer' => 'lekalo',
        ];
    }
    return $findings;
}

/**
 * Rows whose prerequisite evidence does not exist in this slice: the
 * service-locator and magic-state predicates need resolved-reference
 * evidence the pinned toolchain does not export as a public graph.
 * Unsupported is the honest state (issue boundary: no regex fallback).
 */
function strict_predicate_unavailable(string $id): ?array
{
    return null;
}

/**
 * The single explicit `evidence-unsupported` diagnostic for one rule.
 *
 * @return array<string, mixed>
 */
function strict_unsupported_diagnostic(string $id): array
{
    return [
        'rule' => 'target.analysis.evidence-unsupported',
        'original_code' => 'lekalo.unsupported:' . $id,
        'level' => 'note',
        'path' => '.lekalo/import/mago/receipt.json',
        'range' => ['start' => 0, 'end' => 0],
        'message' => 'strict-profile rule ' . $id . ' lacks prerequisite evidence and is unsupported, not passing',
        'producer' => 'lekalo',
    ];
}
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
 *
 * Issue #55 extends the kernel with the external-analyzer seam: the
 * kernel stays subprocess-free (it never launches Mago, Composer, or a
 * shell) and instead consumes a runner-produced analyzer receipt inside
 * its declared read view. Validate/verify keep their #54 empty-success
 * behavior when the analysis seam is absent; once a receipt exists, an
 * incompatible or failed analysis refuses semantic claims instead of
 * reporting a fake pass. Safe fixes are advice-only: no request can
 * ever apply one.
 */

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
/**
 * The adapter identity token.
 */
const ADAPTER_VERSION = '0.2.0';
/** The adapter target token (the wire `target` of generate/bind). */
const TARGET_TOKEN = 'php-laravel';
/** The default profile token; the strict profile rides the analysis seam. */
const PROFILE_TOKEN = 'default';
const STRICT_PROFILE_TOKEN = 'strict';
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
    'verify.scenarios' => 'full',
    'verify.transport-http' => 'unsupported',
    'generate.transport-http' => 'unsupported',
    'preserve.classification' => 'unsupported',
];

/**
 * The generated scenario-test write scope (issue #56).
 */
const SCENARIO_WRITE_SCOPES = ['src/generated/php-laravel/scenario-tests/**'];

/**
 * The user-owned scaffold scope (issue #56, plan S3): `scaffolded`
 * bindings emit once here and are never rewritten or deleted by the
 * kernel. Declared in `write_scopes` for apply honesty; the delete
 * path refuses it outright.
 */
const PHP_SCAFFOLD_SCOPE = 'tests/lekalo/scenario-tests/**';

/**
 * The observed scan index the checked-binding join reads.
 */
const PHP_OBSERVED_INDEX_PATH = '.lekalo/import/observed/index.json';

/**
 * The canonical evidence home of the compiled project IR (core-owned).
 */
const IR_EVIDENCE_HOME = '.lekalo/cache/ir';

/**
 * The scenario compiler modules, in fixed load order. `build.php`
 * concatenates the kernel plus these modules into the shipped
 * single-file artifact, so inside the shipped artifact the functions
 * are already defined and loading is a no-op; the source-tree kernel
 * loads them directly.
 */
function load_scenario_modules(): void
{
    static $loaded = false;
    if ($loaded) {
        return;
    }
    $loaded = true;
    if (function_exists('php_map_scenario') && function_exists('php_emit_scenario_tests')) {
        return;
    }
    foreach ([__DIR__ . '/scenario-map.php', __DIR__ . '/scenario-emit.php'] as $module) {
        if (!is_file($module)) {
            throw new RequestRefusal('compiler-module-missing');
        }
        require_once $module;
    }
}

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
    if (!is_json_object($value)) {
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
        return canonical_string($value);
    }
    $isList = array_is_list($value);
    if ($isList) {
        return '[' . implode(',', array_map(__FUNCTION__, $value)) . ']';
    }
    $keys = array_keys($value);
    usort($keys, 'strcmp');
    $body = [];
    foreach ($keys as $key) {
        $body[] = canonical_string((string) $key)
            . ':' . write_canonical($value[$key]);
    }
    return '{' . implode(',', $body) . '}';
}

/**
 * Canonical JSON string encoding, byte-compatible with the core's
 * `serde_json` serializer: raw UTF-8 non-ASCII (never `\uXXXX`-escaped),
 * unescaped `/`, the closed escape set (`"`, `\`, and control
 * characters), and every other byte — DEL (U+007F) included — carried
 * raw. json_encode would silently DROP a DEL byte, so it travels as a
 * raw NUL sentinel through the encoder and is restored after; raw NUL
 * can never reach this function because the strict decoder refuses
 * control characters in request strings.
 */
function canonical_string(string $value): string
{
    $sentinel = str_replace("\x7F", "\x00", $value);
    $encoded = json_encode(
        $sentinel,
        JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
    );
    return str_replace('\\u0000', "\x7F", $encoded);
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
        // Extension-free digit probe: strspn ships with ext/standard, so
        // the check survives `php -n` runtimes where ctype is absent.
        if ($part === '' || strspn($part, '0123456789') !== strlen($part)) {
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

/**
 * One diagnostic rule id: the closed `target.analysis.*` family with
 * dotted segment grammar (the wire rule id, not the provider code).
 */
function is_analysis_rule_id(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 128
        || !str_starts_with($value, 'target.analysis.')) {
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

/**
 * One symbol identity: a bounded lowercase dotted-segment grammar
 * (`php.fixture.demo` is the convention; segments may include `_` and
 * `-`). The `php.` prefix is convention, not grammar — the prefix is
 * enforced by the closed segment charset, not by a reserved token.
 */
function is_symbol_identity(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 191) {
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

/**
 * One native evidence path: case-preserving, traversal-free, rooted
 * (no leading separator), with bounded segments. This is the versioned
 * native-path domain for target-project sources (see the diagnostic
 * decoder note on Laravel casing); it is deliberately distinct from the
 * lowercase Model logical-path grammar, and protected homes are still
 * refused by the same rule the wire enforces.
 */
function is_native_evidence_path(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 512 || str_starts_with($value, '/')) {
        return false;
    }
    $parts = explode('/', $value);
    if (in_array('', $parts, true)) {
        return false;
    }
    foreach ($parts as $part) {
        if ($part === '.' || $part === '..' || strlen($part) > 255) {
            return false;
        }
        if ((bool) preg_match('/[\x00-\x1f]/', $part)) {
            return false;
        }
        if ((bool) preg_match('/^[A-Za-z]:/', $part) || str_contains($part, '\\')) {
            return false;
        }
    }
    return protected_home($value) === null;
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
        if (!is_array($limits) || !is_json_object($limits)) {
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
        // Member closure is order-insensitive (serde `deny_unknown_fields`
        // never depends on decoded member order), so exactly the closed
        // pair — in any key order — is legal.
        if (!is_json_object($capability)
            || count($capability) !== 2
            || !array_key_exists('id', $capability)
            || !array_key_exists('support', $capability)) {
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
/**
 * Whether one decoded array can only be a JSON object: a non-empty list
 * is definitely a JSON array, but an empty PHP array is ambiguous — JSON
 * `{}` and `[]` both decode to `[]`, so an empty array is accepted as
 * the (member-less) object shape.
 */
function is_json_object(mixed $value): bool
{
    return is_array($value) && (!array_is_list($value) || $value === []);
}

function validate_native_request(mixed $native): void
{
    static $keys = [
        'changes', 'scan_ref', 'observed_ref', 'execution_policy_ref',
        'input_manifest_digest', 'tool_catalog_digest',
        'capability_snapshot_digest',
    ];
    if (!is_json_object($native)) {
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
    if (!is_json_object($changes)) {
        throw new RequestRefusal('native-request');
    }
    $files = $changes['files'] ?? [];
    if (!is_array($files) || !array_is_list($files) || count($files) > 1024) {
        throw new RequestRefusal('native-request');
    }
    foreach ($files as $file) {
        if (!is_json_object($file)) {
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
    if (!is_json_object($reference)
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
/**
 * Whether the request's `ir_path` names a scenario document: either the
 * closed `.scenario.json` spelling or the conformance fixture's exact
 * scenario input path.
 */
function is_scenario_ir_path(string $path): bool
{
    return str_ends_with($path, '.scenario.json')
        || str_ends_with($path, 'scenario-txn-concurrency.json')
        || str_contains(basename($path), '.scenario.');
}

/**
 * The deterministic generation entry (issue #56): a scenario document
 * at `ir_path` maps to the full Laratesto test set (support files plus
 * one test and one canonical sidecar per scenario); anything else
 * falls back to the #54 kernel artifact. A generation over a scenario
 * document carries the mapper's typed findings; a compile-time finding
 * vetoes every write exactly like the Node pipeline.
 */
function deterministic_generation(array $request): array
{
    $irPath = $request['ir_path'] ?? '';
    if (is_string($irPath) && is_scenario_ir_path($irPath)) {
        $outcome = scenario_generation($request);
        if (isset($outcome['refusal'])) {
            throw new RequestRefusal($outcome['refusal']);
        }
        if ($outcome['findings'] !== []) {
            // Capability honesty: the mapper cannot express the document.
            // Nothing is emitted and nothing is written.
            return [
                'path' => null,
                'bytes' => '',
                'digest' => null,
                'writes' => [],
                'findings' => $outcome['findings'],
            ];
        }
        $writes = [];
        $skipWrites = $outcome['skip_writes'] ?? [];
        foreach ($outcome['files'] as $file) {
            if (isset($skipWrites[$file['path']])) {
                continue;
            }
            $writes[] = [
                'path' => $file['path'],
                'action' => 'create',
                'sha256' => $file['digest'],
            ];
        }
        return [
            'path' => null,
            'bytes' => '',
            'digest' => null,
            'writes' => $writes,
            'files' => $outcome['files'],
            'findings' => [],
        ];
    }
    $artifact = kernel_artifact($request);
    $writes = [[
        'path' => $artifact['path'],
        'action' => $artifact['action'] ?? 'create',
        'sha256' => $artifact['digest'],
    ]];
    $envelope = [
        'path' => $artifact['path'],
        'bytes' => $artifact['bytes'],
        'digest' => $artifact['digest'],
        'writes' => $writes,
        'findings' => [],
    ];
    if (isset($artifact['ledger'])) {
        // The migration emitter's append-only ledger rides beside the
        // migration file through the envelope: the write planner adds
        // its entry and the apply loop verifies its exact bytes.
        $envelope['ledger'] = $artifact['ledger'];
    }
    return $envelope;
}

/**
 * The scenario read-and-map path shared by generate/validate/verify:
 * reads the scenario document, the compiled project IR evidence from
 * the canonical cache home, and the project test-port declaration;
 * maps through the pure mapper; and returns the emitted files plus
 * typed findings. A read refusal or a closed-shape refusal is an
 * in-envelope `failed` outcome, never a guessed plan.
 */
function read_and_map_scenario(array $request): array
{
    $outcome = scenario_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the scenario document could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    return $outcome;
}

/**
 * The scenario compiler: map the scenario document and emit the
 * deterministic test files. Compilation only reads data: application,
 * vendor, and port code never execute inside the compiler process.
 */
function scenario_generation(array $request): array
{
    $irPath = $request['ir_path'] ?? '';
    if (!is_string($irPath) || !is_scenario_ir_path($irPath)) {
        // Not a scenario document: the #54 kernel artifact path owns it.
        return ['not-scenario' => true, 'files' => [], 'findings' => []];
    }
    $scenarioText = read_view_file($irPath);
    if ($scenarioText === null) {
        return ['refusal' => 'scenario-unreadable', 'files' => [], 'findings' => []];
    }
    try {
        $scenario = json_decode($scenarioText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'scenario-shape', 'files' => [], 'findings' => []];
    }
    if (($scenario['schemaVersion'] ?? null) !== 'lekalo/scenario-ir/v0.2.16') {
        return ['refusal' => 'ir-version-unsupported', 'files' => [], 'findings' => []];
    }
    $projectId = $scenario['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9-]*$/', $projectId) !== 1) {
        return ['refusal' => 'scenario-project-id', 'files' => [], 'findings' => []];
    }
    $irEvidenceText = read_view_file(IR_EVIDENCE_HOME . '/' . $projectId . '.json');
    if ($irEvidenceText === null) {
        return ['refusal' => 'ir-evidence-unreadable', 'files' => [], 'findings' => []];
    }
    try {
        $irEvidence = json_decode($irEvidenceText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'ir-evidence-shape', 'files' => [], 'findings' => []];
    }
    $port = null;
    $portPresent = false;
    load_scenario_modules();
    $portText = read_view_file(PHP_PORT_DOC_PATH);
    if ($portText !== null) {
        $portPresent = true;
        try {
            $port = json_decode($portText, true, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException) {
            return ['refusal' => 'port-shape', 'files' => [], 'findings' => []];
        }
        if (!is_array($port)) {
            return ['refusal' => 'port-shape', 'files' => [], 'findings' => []];
        }
        // The adapter-owned declaration must satisfy its closed shape
        // before any plan exists: a present-but-invalid document is an
        // authoring error, never an all-unsupported silent fallback.
        $port = php_validate_port_doc($port);
        if ($port === null) {
            return ['refusal' => 'port-shape', 'files' => [], 'findings' => []];
        }
    }
    $capabilities = null;
    if (isset($request['profile_capabilities']) && is_array($request['profile_capabilities'])) {
        $capabilities = $request['profile_capabilities'];
    }
    $mapped = php_map_scenario([
        'scenario' => $scenario,
        'ir' => $irEvidence,
        'irDigest' => 'sha256:' . hash('sha256', $irEvidenceText),
        'port' => $port,
        'portPresent' => $portPresent,
        'profileCapabilities' => $capabilities,
    ]);
    if ($mapped['state'] === 'refused') {
        return ['refusal' => 'scenario-' . $mapped['refusal'], 'files' => [], 'findings' => []];
    }
    $files = php_emit_scenario_tests([
        'models' => $mapped['scenarios'],
        'inputDigest' => 'sha256:' . hash('sha256', $scenarioText),
        'adapterVersion' => ADAPTER_VERSION,
        'portModulePath' => $portPresent ? $port['path'] : '',
        'portClass' => $portPresent ? $port['class'] : '',
    ]);
    $emitted = [];
    $skipWrites = [];
    foreach ($files as $file) {
        $entry = [
            'path' => $file['path'],
            'bytes' => $file['text'],
            'digest' => 'sha256:' . hash('sha256', $file['text']),
        ];
        if (($file['frozen'] ?? false) === true) {
            // Scaffold-once custody: the frozen write is planned only
            // while its managed marker is absent. Once the marker
            // exists the file is user-owned — regeneration never
            // rewrites it, and a deleted test is a verify finding
            // rather than a silent recreate.
            $entry['frozen'] = true;
            $entry['marker'] = $file['marker'];
            if (is_file($file['marker'])) {
                $skipWrites[$file['path']] = true;
            }
        }
        $emitted[] = $entry;
    }
    return [
        'files' => $emitted,
        'findings' => $mapped['findings'],
        'skip_writes' => $skipWrites,
        'scenario' => $scenario,
    ];
}

/** One bounded read inside the declared read roots (null when absent). */
function read_view_file(string $path): ?string
{
    if (!is_logical_path($path) || strlen($path) > 4096) {
        return null;
    }
    if (str_starts_with($path, '.lekalo/ir/') || str_starts_with($path, '.lekalo/cache/')
        || str_starts_with($path, '.lekalo/import/')
        || str_starts_with($path, 'lekalo/')) {
        $bytes = @file_get_contents($path);
        return $bytes === false ? null : $bytes;
    }
    return null;
}

/**
 * The generated-artifact entry the kernel owns when no scenario
 * document is present (the #54 MVP surface, kept for the conformance
 * battery). Generation never claims a construct it did not map.
 */
function kernel_artifact(array $request): array
{
    $target = $request['target'] ?? TARGET_TOKEN;
    $profile = $request['profile'] ?? PROFILE_TOKEN;
    $irPath = $request['ir_path'] ?? '.lekalo/ir/planner.json';
    // Issue #57: a bound Laravel migration input document drives the
    // migration emitter; everything else rides the kernel fixture
    // artifact. The input path must sit inside the declared read
    // scope and carry the closed contract identity.
    if (str_ends_with($irPath, '.migration-input.json')
        && scope_covers('.lekalo/ir/**', $irPath)
        && is_file($irPath)) {
        $bytes = file_get_contents($irPath);
        if ($bytes !== false) {
            $input = decode_storage_input($bytes);
            return emit_laravel_migrations($input, $target, $irPath);
        }
    }
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

/** The write entries of one generation outcome (already computed). */
function deterministic_writes(array $artifact): array
{
    $writes = $artifact['writes'];
    // The migration emitter ships its append-only ledger beside the
    // migration file; both must be planned and written atomically. A
    // first publish plans a create, an append plans a replace of the
    // merged ledger (the append itself is proven by
    // assert_append_only inside the emitter).
    if (isset($artifact['ledger'])) {
        $writes[] = [
            'path' => $artifact['ledger']['path'],
            'action' => $artifact['ledger']['action'] ?? 'create',
            'sha256' => $artifact['ledger']['digest'],
        ];
    }
    return $writes;
}

/** The canonical plan id: sha256 over the canonical ordered entries. */
function plan_id(array $writes): string
{
    return 'plan-' . sha256_hex(canonical_json($writes));
}

// ---------------------------------------------------------------------------
// 5a. The Laravel migration emitter (issue #57).
// ---------------------------------------------------------------------------

/** The closed step-kind vocabulary the emitter consumes (the plan v1 set). */
const MIGRATION_OPERATION_KINDS = [
    'create_table', 'drop_table', 'rename_table', 'add_column',
    'drop_column', 'alter_column_type', 'set_column_null',
    'set_column_default', 'add_primary_key', 'rename_constraint',
    'drop_constraint', 'add_unique', 'add_foreign_key', 'add_check',
    'drop_check', 'add_index', 'drop_index', 'create_enum_type',
    'alter_enum_type', 'enable_rls', 'disable_rls', 'create_policy',
    'drop_policy', 'create_sequence', 'alter_sequence',
    'rename_sequence', 'drop_sequence', 'create_extension',
    'backfill', 'rename_column',
];

/** The closed rollback classes (the contract spelling). */
const MIGRATION_ROLLBACK_CLASSES = [
    'reversible', 'data-loss-on-rollback', 'irreversible',
];

/** The closed effective-status vocabulary. */
const MIGRATION_STATUSES = ['ready', 'blocked', 'confirmed'];

/** The declared UTC timestamp base of the fixture pipeline (policy, never the wall clock). */
const MIGRATION_TIMESTAMP_BASE = '2026_09_27_000001';

/**
 * Decode and validate one bounded Laravel migration input document.
 * The closed shape mirrors contracts/laravel-migration-input.schema
 * exactly: unknown members refuse, every closed vocabulary is checked,
 * and every digest pin must be a canonical sha256 spelling.
 */
function decode_storage_input(string $bytes): array
{
    if (strlen($bytes) > MAX_REQUEST_BYTES) {
        throw new RequestRefusal('input-too-large');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('input-syntax');
    }
    if (!is_json_object($document)) {
        throw new RequestRefusal('input-shape');
    }
    $required = [
        'schemaVersion', 'identity', 'engine', 'engineVersion',
        'projectId', 'baseDigest', 'candidateDigest', 'diffDigest',
        'planId', 'gated', 'backfillGated', 'effectiveStatus',
        'operations',
    ];
    $known = array_merge($required, ['historyDigest']);
    foreach (array_keys($document) as $key) {
        if (!in_array($key, $known, true)) {
            throw new RequestRefusal('input-member');
        }
    }
    foreach ($required as $key) {
        if (!array_key_exists($key, $document)) {
            throw new RequestRefusal('input-member');
        }
    }
    if ($document['schemaVersion'] !== 'lekalo/laravel-migration-input/v0.4.0'
        || $document['identity'] !== 'dev.lekalo.laravel-migration-input@0.4.0') {
        throw new RequestRefusal('input-identity');
    }
    if ($document['engine'] !== 'postgres'
        || !is_contract_version($document['engineVersion'])) {
        throw new RequestRefusal('input-engine');
    }
    foreach (['baseDigest', 'candidateDigest', 'diffDigest', 'planId'] as $key) {
        if (!is_sha256_digest($document[$key])) {
            throw new RequestRefusal('input-digest');
        }
    }
    if (array_key_exists('historyDigest', $document)
        && !is_sha256_digest($document['historyDigest'])) {
        throw new RequestRefusal('input-digest');
    }
    if (!is_bool($document['gated']) || !is_bool($document['backfillGated'])) {
        throw new RequestRefusal('input-shape');
    }
    if (!in_array($document['effectiveStatus'], MIGRATION_STATUSES, true)) {
        throw new RequestRefusal('input-status');
    }
    // The effective gate closes: a blocked document generates zero
    // bytes. The refusal is the honest state, never a partial file.
    if ($document['effectiveStatus'] === 'blocked') {
        throw new RequestRefusal('input-gated');
    }
    if (!is_array($document['operations']) || !array_is_list($document['operations'])
        || count($document['operations']) > 1024) {
        throw new RequestRefusal('input-operations');
    }
    $previous = 0;
    foreach ($document['operations'] as $operation) {
        if (!is_json_object($operation)) {
            throw new RequestRefusal('input-operation');
        }
        $operationKnown = ['ordinal', 'kind', 'statement', 'risk', 'requires', 'rollback', 'inverse'];
        $operationRequired = ['ordinal', 'kind', 'statement', 'risk', 'rollback'];
        foreach (array_keys($operation) as $key) {
            if (!in_array($key, $operationKnown, true)) {
                throw new RequestRefusal('input-member');
            }
        }
        foreach ($operationRequired as $key) {
            if (!array_key_exists($key, $operation)) {
                throw new RequestRefusal('input-operation');
            }
        }
        if (!is_int($operation['ordinal']) || $operation['ordinal'] <= $previous) {
            throw new RequestRefusal('input-order');
        }
        $previous = $operation['ordinal'];
        if (!in_array($operation['kind'], MIGRATION_OPERATION_KINDS, true)) {
            throw new RequestRefusal('input-kind');
        }
        if (!is_string($operation['statement']) || $operation['statement'] === ''
            || strlen($operation['statement']) > 4096) {
            throw new RequestRefusal('input-statement');
        }
        if (!in_array($operation['risk'], ['none', 'backfill_required', 'destructive'], true)) {
            throw new RequestRefusal('input-risk');
        }
        if (array_key_exists('requires', $operation)) {
            if (!is_array($operation['requires']) || !array_is_list($operation['requires'])) {
                throw new RequestRefusal('input-operation');
            }
            foreach ($operation['requires'] as $requirement) {
                if (!is_int($requirement) || $requirement < 1
                    || $requirement >= $operation['ordinal']) {
                    throw new RequestRefusal('input-requires');
                }
            }
        }
        if (!in_array($operation['rollback'], MIGRATION_ROLLBACK_CLASSES, true)) {
            throw new RequestRefusal('input-rollback');
        }
        if (array_key_exists('inverse', $operation)
            && (!is_string($operation['inverse']) || $operation['inverse'] === ''
                || strlen($operation['inverse']) > 4096)) {
            throw new RequestRefusal('input-inverse');
        }
        // A reversible operation carries its inverse: the plan v1
        // metadata fully determines it, so the absence would be a
        // truncated document, and an irreversible or data-loss class
        // never carries one.
        $hasInverse = array_key_exists('inverse', $operation);
        if (($operation['rollback'] === 'reversible') !== $hasInverse) {
            throw new RequestRefusal('input-inverse');
        }
    }
    return $document;
}

/**
 * Emit the deterministic Laravel migration artifacts for one bounded
 * input: one PHP class whose up() executes exactly the planned SQL in
 * dependency order and whose down() either inverts a fully reversible
 * plan or refuses before any statement, plus the append-only ledger.
 * Two runs with identical inputs are byte-identical; the artifact set
 * never contains anything but the migration file and the ledger.
 */
function emit_laravel_migrations(array $input, string $target, string $irPath): array
{
    // The effective gate closes here too: emit never renders a blocked
    // transition, defense in depth against a bypassed decode.
    if (($input['effectiveStatus'] ?? '') === 'blocked') {
        throw new RequestRefusal('input-gated');
    }
    $shortDigest = substr(sha256_hex(canonical_json($input)), 0, 12);
    $class = 'Lekalo' . strtoupper(substr(sha256_hex($shortDigest), 0, 8)) . 'Migration';
    $directory = '.lekalo/generated/php-laravel/' . $target . '/migrations';
    $filename = MIGRATION_TIMESTAMP_BASE . '_lekalo_' . $shortDigest . '.php';
    $path = $directory . '/' . $filename;
    $bytes = render_migration_class($input, $class, $target, $irPath);
    $ledgerPath = $directory . '/ledger.json';
    $entry = [
        'filename' => $filename,
        'digest' => sha256_digest($bytes),
        'bytes' => strlen($bytes),
        'planId' => $input['planId'],
        'inputDigest' => sha256_digest(canonical_json($input)),
        'baseDigest' => $input['baseDigest'],
        'candidateDigest' => $input['candidateDigest'],
        'timestampBase' => MIGRATION_TIMESTAMP_BASE,
        'gated' => $input['gated'],
        'backfillGated' => $input['backfillGated'],
        'effectiveStatus' => $input['effectiveStatus'],
    ];
    // The ledger is append-only custody: a published ledger never
    // shrinks and never rewrites an entry. The emitter reads the
    // published document, proves the append with assert_append_only,
    // and writes the merged ledger — so a second generate --apply
    // appends instead of dying write-denied on the existing file, and
    // a byte-identical regeneration replans exactly the published
    // bytes (a true no-op at apply time).
    $published = read_published_ledger($ledgerPath);
    if ($published === null) {
        $ledger = [
            'schemaVersion' => 'lekalo/laravel-migration-ledger/v0.4.0',
            'identity' => 'dev.lekalo.laravel-migration-ledger@0.4.0',
            'projectId' => $input['projectId'],
            'engine' => $input['engine'],
            'engineVersion' => $input['engineVersion'],
            'migrations' => [[
                'ordinal' => 1,
            ] + $entry],
        ];
        $ledgerBytes = canonical_json($ledger) . "\n";
        $ledgerAction = 'create';
    } else {
        $publishedMigrations = $published['migrations'];
        $alreadyPublished = null;
        $highestOrdinal = 0;
        foreach ($publishedMigrations as $publishedEntry) {
            $highestOrdinal = max($highestOrdinal, (int) $publishedEntry['ordinal']);
            if (($publishedEntry['filename'] ?? null) === $filename) {
                $alreadyPublished = $publishedEntry;
            }
        }
        if ($alreadyPublished !== null && $alreadyPublished['digest'] !== $entry['digest']) {
            // The same published filename with different bytes is a
            // custody violation: a migration file is named by its
            // input digest, so the published class must be identical.
            throw new RequestRefusal('ledger-conflict');
        }
        if ($alreadyPublished !== null) {
            // Byte-identical regeneration: the entry is already
            // published, the ledger keeps its published bytes, and the
            // planned write replays them exactly (no-op on disk).
            $ledgerBytes = file_get_contents($ledgerPath);
            if ($ledgerBytes === false) {
                throw new RequestRefusal('ledger-shape');
            }
            $ledgerAction = 'create';
        } else {
            $entry = ['ordinal' => $highestOrdinal + 1] + $entry;
            $merged = $published;
            $merged['migrations'][] = $entry;
            assert_append_only($published, $merged);
            $ledgerBytes = canonical_json($merged) . "\n";
            $ledgerAction = 'replace';
        }
    }
    return [
        'path' => $path,
        'bytes' => $bytes,
        'digest' => sha256_digest($bytes),
        'action' => 'create',
        'ledger' => [
            'path' => $ledgerPath,
            'bytes' => $ledgerBytes,
            'digest' => sha256_digest($ledgerBytes),
            'action' => $ledgerAction,
        ],
        'filename' => $filename,
    ];
}

/**
 * The published ledger of one migrations directory, or null when this
 * adapter has not published one yet. A present-but-invalid ledger
 * refuses: append-only custody cannot be proven over a corrupt
 * document, and silently starting a fresh ledger would orphan every
 * published migration.
 */
function read_published_ledger(string $path): ?array
{
    if (!is_file($path)) {
        return null;
    }
    $bytes = file_get_contents($path);
    if ($bytes === false) {
        throw new RequestRefusal('ledger-shape');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('ledger-shape');
    }
    if (!is_json_object($document)
        || !isset($document['migrations'])
        || !is_array($document['migrations'])
        || !array_is_list($document['migrations'])) {
        throw new RequestRefusal('ledger-shape');
    }
    return $document;
}

/**
 * Render one Laravel migration class: strict types, an anonymous
 * class extending Illuminate\Database\Migrations\Migration, up() as
 * sequential DB::statement calls of the planned SQL, and down() from
 * the typed reverse plan or an explicit refusal. The SQL travels as
 * single-quoted PHP literals byte-for-byte — quotes, backslashes,
 * dollar signs, and Unicode survive exactly — never through eval or
 * string interpolation.
 */
function render_migration_class(array $input, string $class, string $target, string $irPath): string
{
    $lines = [];
    $lines[] = '<?php';
    $lines[] = '';
    $lines[] = '// generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (issue #57)';
    $lines[] = 'declare(strict_types=1);';
    $lines[] = '';
    $lines[] = '// target: ' . $target;
    $lines[] = '// engine: postgres ' . $input['engineVersion'];
    $lines[] = '// project: ' . $input['projectId'];
    $lines[] = '// plan: ' . $input['planId'];
    $lines[] = '// input: ' . $irPath;
    $lines[] = '';
    $lines[] = 'use Illuminate\Database\Migrations\Migration;';
    $lines[] = 'use Illuminate\Database\Schema\Blueprint;' ;
    $lines[] = 'use Illuminate\Support\Facades\DB;';
    $lines[] = '';
    $lines[] = '/**';
    $lines[] = ' * Lekalo storage transition: ' . count($input['operations']) . ' operation(s),';
    $lines[] = ' * effective gate ' . $input['effectiveStatus'] . '.';
    $lines[] = ' */';
    $lines[] = 'return new class extends Migration';
    $lines[] = '{';
    $lines[] = '    /** Execute the planned transition in dependency order. */';
    $lines[] = '    public function up(): void';
    $lines[] = '    {';
    foreach ($input['operations'] as $operation) {
        $literal = php_single_quote($operation['statement']);
        $lines[] = '        DB::statement(\'' . $literal . '\');';
    }
    $lines[] = '    }';
    $lines[] = '';
    $reverses = [];
    foreach ($input['operations'] as $operation) {
        $reverses[] = $operation['rollback'] === 'reversible'
            ? $operation['inverse']
            : null;
    }
    $fullyReversible = !in_array(null, $reverses, true);
    $lines[] = '    /** Reverse the transition; refuses before any statement when unsafe. */';
    $lines[] = '    public function down(): void';
    $lines[] = '    {';
    if ($fullyReversible) {
        foreach (array_reverse($reverses) as $inverse) {
            $literal = php_single_quote($inverse);
            $lines[] = '        DB::statement(\'' . $literal . '\');';
        }
    } else {
        $lines[] = '        // The plan carries irreversible or data-loss-on-rollback';
        $lines[] = '        // operations: the rollback refuses before the first statement.';
        $lines[] = '        throw new \RuntimeException(\'lekalo: rollback is not safe for this migration\');';
    }
    $lines[] = '    }';
    $lines[] = '};';
    return implode("\n", $lines) . "\n";
}

/**
 * One single-quoted PHP string literal preserving the exact SQL bytes:
 * backslash and single quote escape, everything else travels raw
 * (dollar signs, double quotes, Unicode). No eval, no interpolation.
 */
function php_single_quote(string $sql): string
{
    return str_replace(["\\", "'"], ['\\\\', "\\'"], $sql);
}

/**
 * The append-only custody check: a published migration ledger never
 * loses entries and never rewrites a published file. An existing
 * ledger accepts only a byte-identical regeneration (no-op) or an
 * append with a strictly higher ordinal; any other shape refuses.
 */
function assert_append_only(array $existing, array $next): void
{
    if (!is_json_object($existing)
        || !isset($existing['migrations'])
        || !is_array($existing['migrations'])
        || !array_is_list($existing['migrations'])) {
        throw new RequestRefusal('ledger-shape');
    }
    $published = $existing['migrations'];
    $nextMigrations = $next['migrations'];
    $count = count($published);
    if (count($nextMigrations) < $count) {
        throw new RequestRefusal('ledger-shrunk');
    }
    foreach ($published as $index => $entry) {
        if ($nextMigrations[$index] !== $entry) {
            throw new RequestRefusal('ledger-rewritten');
        }
    }
    $previous = 0;
    foreach ($nextMigrations as $entry) {
        if (!is_int($entry['ordinal']) || $entry['ordinal'] <= $previous) {
            throw new RequestRefusal('ledger-order');
        }
        $previous = $entry['ordinal'];
    }
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

/**
 * The capability map of this kernel (issue #28 fluent surface), extended
 * for #55 with the analysis seam identity. The seam reports the injected
 * analyzer's capabilities; the wire named-capability map stays unchanged
 * (`scan.symbols` remains unsupported: the bounded scan wire cannot
 * carry a full native graph — see scan_response).
 */
function describe_capabilities(?Analyzer $analyzer = null): array
{
    $analyzer ??= new FakeAnalyzer();
    return [
        'adapter' => adapter_identity(),
        'protocol_versions' => SUPPORTED_VERSIONS,
        'operations' => OPERATIONS,
        'transports' => ['stdin', 'file'],
        'targets' => [TARGET_TOKEN],
        'profiles' => [PROFILE_TOKEN, STRICT_PROFILE_TOKEN],
        'read_scopes' => ['.lekalo/cache/**', '.lekalo/ir/**', '.lekalo/import/**'],
        'write_scopes' => array_merge(
            ['.lekalo/generated/php-laravel/**'],
            SCENARIO_WRITE_SCOPES,
            [PHP_SCAFFOLD_SCOPE],
        ),
        'progress' => false,
        'ir_versions' => [IR_VERSION],
        'capabilities' => DECLARED_CAPABILITIES,
        // The advisory bound mirrors the kernel's real write-plan file
        // cap (MAX_WRITE_FILES): a declared constraint never exceeds an
        // internally enforced one.
        'constraints' => ['max_entries' => MAX_WRITE_FILES],
    ];
}

/**
 * The analyzer capability identity (#55). Deliberately NOT part of the
 * closed describe envelope: the v0.3.2 wire `Capabilities` struct is
 * closed (`deny_unknown_fields`), so the seam identity travels through
 * internal evidence instead of undeclared extra members. Exposed as a
 * kernel function for evidence rendering and tests.
 */
function analysis_seam_identity(?Analyzer $analyzer = null): array
{
    $analyzer ??= new FakeAnalyzer();
    return $analyzer->capabilities();
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

/**
 * The declared read scopes this kernel scans (and no others). The
 * import home rides the #55 read scope so the staged receipt document
 * is a real observable scan entry — the evidence join target.
 */
const SCAN_ROOTS = ['.lekalo/ir', '.lekalo/cache', '.lekalo/import'];
/** The maximum scan entries one result may carry (mirrors the wire bound). */
const MAX_SCAN_ENTRIES = 10000;

/**
 * The scan exchange: a real read-only enumeration of the staged view's
 * declared read roots, never a canned entry list. Every returned path
 * was observed on the filesystem under a declared scope; the roots are
 * pruned by the same lexical grammar the core enforces, and the result
 * stays inside the wire bounds (a fuller inventory is `truncated`,
 * never silently cut).
 *
 * Issue #55 evidence join: the receipt's native evidence lives in the
 * target-project path domain (`app/Models/User.php`), disjoint from the
 * enumerated IR/cache roots (`.lekalo/**`). The join therefore attaches
 * the bounded evidence projection to the one governance row the receipt
 * owns inside the scanned domain: the receipt document itself
 * (`.lekalo/import/mago/receipt.json`). Source rows stay in the receipt
 * where their native paths are legal.
 */
function scan_response(array $request, ?Analyzer $analyzer = null): array
{
    $evidenceForReceipt = null;
    if ($analyzer !== null) {
        $outcome = $analyzer->analyze();
        if ($outcome->isOk()) {
            $evidenceForReceipt = mago_receipt_wire_evidence($outcome);
        }
    }
    $entries = [];
    $truncated = false;
    foreach (SCAN_ROOTS as $root) {
        if (!is_dir($root)) {
            continue;
        }
        $iterator = new RecursiveIteratorIterator(
            new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS),
            RecursiveIteratorIterator::LEAVES_ONLY,
        );
        foreach ($iterator as $file) {
            if (!$file->isFile()) {
                continue;
            }
            $path = str_replace('\\', '/', $file->getPathname());
            if (!is_logical_path($path)) {
                continue;
            }
            if (count($entries) >= MAX_SCAN_ENTRIES) {
                $truncated = true;
                break 2;
            }
            // The wire `kind` is a free bounded token: IR/cache bytes are
            // 'ir'; the staged provider receipt is not IR, so it carries
            // its honest 'evidence' spelling.
            $kind = str_starts_with($path, '.lekalo/import/') ? 'evidence' : 'ir';
            $entry = [
                'path' => $path,
                'kind' => $kind,
            ];
            if ($evidenceForReceipt !== null && $path === MAGO_RECEIPT_PATH) {
                $entry['evidence'] = $evidenceForReceipt;
            }
            $entries[] = $entry;
        }
    }
    // Canonical entry order: sorted by path (the closed wire keeps the
    // core's plan validator byte-comparable; scan results sort the same).
    usort($entries, static fn (array $a, array $b): int => strcmp($a['path'], $b['path']));
    return build_response($request, [
        'result' => [
            'entries' => $entries,
            'truncated' => $truncated,
        ],
    ]);
}

/**
 * Project the receipt's symbols and relations into the ONE bounded wire
 * evidence record that scan attaches to the receipt document itself
 * (`.lekalo/import/mago/receipt.json`): a signature digest over the
 * canonical projection (covering every symbol signature the receipt
 * carries) plus up to eight outbound references sampled deterministically
 * from the receipt's relation rows. A relation whose source symbol is
 * unknown cannot be attributed, so it drops out of the bounded
 * projection while remaining in the receipt. Any over-bound or unjoined
 * remainder drops the WHOLE claim instead of publishing a partial graph
 * under an exact-looking signature.
 *
 * @return array<string, mixed>|null null when nothing is projectable
 */
function mago_receipt_wire_evidence(AnalysisOutcome $outcome): ?array
{
    $signatures = [];
    $identityKnown = [];
    foreach ($outcome->symbols as $symbol) {
        $identityKnown[(string) $symbol['identity']] = true;
        if (isset($symbol['signature'])) {
            $signatures[(string) $symbol['path']] = (string) $symbol['signature'];
        }
    }
    ksort($signatures);
    $references = [];
    $unjoined = 0;
    $overflow = false;
    foreach ($outcome->relations as $relation) {
        $source = (string) $relation['from'];
        if (!isset($identityKnown[$source])) {
            $unjoined++;
            continue;
        }
        if (count($references) >= 8) {
            $overflow = true;
            break;
        }
        $references[] = [
            'target' => (string) $relation['to'],
            'role' => mago_wire_role((string) $relation['role']),
            'confidence' => mago_wire_confidence((string) $relation['confidence']),
        ];
    }
    // Nothing projectable: honest absence, never an empty claim.
    if ($signatures === [] || $references === [] || $overflow || $unjoined > 0) {
        return null;
    }
    $signature = sha256_digest(canonical_json(array_values($signatures)));
    return [
        'signature' => $signature,
        'references' => $references,
    ];
}

/** Map a receipt role onto the closed wire role set. */
function mago_wire_role(string $role): string
{
    return match ($role) {
        'reads' => 'read',
        'writes' => 'update',
        'calls' => 'call',
        default => 'reference',
    };
}

/** Map a receipt confidence onto the closed wire confidence set. */
function mago_wire_confidence(string $confidence): string
{
    return in_array($confidence, ['exact', 'high', 'medium', 'low', 'unknown'], true)
        ? $confidence
        : 'unknown';
}

/**
 * The validate exchange (#55): the analysis seam's verdict on the
 * staged view. Without a receipt (unavailable) the #54 empty success is
 * preserved — no analyzer, no claims either way. An incompatible or
 * failed analysis refuses with an explicit in-envelope error so a stale
 * receipt can never dress up as a pass. Findings are advice; safe fixes
 * are metadata only and are never applied by any operation.
 *
 * Profile: the default profile reports the analysis seam's recorded
 * diagnostics verbatim; the strict profile additionally evaluates the
 * Lekalo strict-profile predicates over the receipt evidence. A strict
 * request with an UNAVAILABLE receipt is not a silent clean pass: every
 * strict rule without prerequisite evidence emits its explicit
 * evidence-unsupported row (the #54 empty success can only stand for
 * the default profile). Only the declared profiles reach this function.
 */
function validate_response(array $request, ?Analyzer $analyzer = null): array
{
    // A scenario document takes the scenario gate (issue #56); every
    // other input keeps the analysis-seam gate (#55).
    if (is_scenario_ir_path(is_string($request['ir_path'] ?? null) ? $request['ir_path'] : '')) {
        return scenario_validate_response($request);
    }
    $analyzer ??= new FakeAnalyzer();
    // Profile closure: only the two declared spellings are meaningful;
    // anything else is an in-envelope invalid refusal, never a silent
    // downgrade to default (a typo must not silently change the gate).
    $profile = $request['profile'] ?? PROFILE_TOKEN;
    if ($profile !== PROFILE_TOKEN && $profile !== STRICT_PROFILE_TOKEN) {
        return build_response($request, [
            'error' => [
                'class' => 'invalid',
                'code' => 'profile-undeclared',
                'message' => 'the requested profile is not declared by this adapter',
                'retryable' => false,
                'partial' => false,
            ],
        ]);
    }
    $outcome = $analyzer->analyze();
    if (!$outcome->isOk() && $outcome->state !== 'unavailable') {
        return build_response($request, [
            'error' => mago_analysis_error($outcome),
        ]);
    }
    $strict = ($request['profile'] ?? PROFILE_TOKEN) === STRICT_PROFILE_TOKEN;
    $findings = [];
    if ($outcome->isOk()) {
        foreach ($outcome->diagnostics as $row) {
            $findings[] = mago_wire_finding($row);
        }
        if ($strict) {
            $evaluated = strict_profile_evaluate($outcome->diagnostics, $outcome->symbols);
            foreach ($evaluated['findings'] as $row) {
                $findings[] = mago_wire_finding($row);
            }
            foreach ($evaluated['unsupported'] as $id) {
                $findings[] = mago_wire_finding(strict_unsupported_diagnostic($id));
            }
        }
    } elseif ($strict) {
        // Strict + unavailable: no evidence is never a clean strict
        // pass — every rule row without prerequisite evidence reports
        // its explicit unsupported diagnostic.
        foreach (array_keys(STRICT_RULES) as $id) {
            $findings[] = mago_wire_finding(strict_unsupported_diagnostic((string) $id));
        }
    }
    return build_response($request, [
        'result' => ['ok' => true, 'findings' => $findings],
    ]);
}

/**
 * The verify exchange (#55): the same analysis-seam gate as validate.
 * Mago success never satisfies scenario verification — a green Mago run
 * cannot masquerade as a verified scenario: scenario documents verify
 * through the dedicated scenario drift gate (#56), which is what the
 * `verify.scenarios` capability names.
 */
function verify_response(array $request, ?Analyzer $analyzer = null): array
{
    // Scenario documents verify through the scenario drift gate (issue
    // #56); everything else keeps the analysis-seam gate (#55).
    if (is_scenario_ir_path(is_string($request['ir_path'] ?? null) ? $request['ir_path'] : '')) {
        return scenario_verify_response($request);
    }
    return validate_response($request, $analyzer);
}

/**
 * Clamp UTF-8 text to at most `$limit` codepoints without ever cutting
 * mid-codepoint: PHP `substr` counts BYTES, so a byte clamp on multibyte
 * text (em-dashes, smart quotes — legal receipt message content) yields
 * invalid UTF-8 and `json_encode` would refuse the whole envelope. The
 * wire bound counts codepoints, so back off to the last complete
 * character boundary at or before the byte position that holds
 * `$limit` characters.
 */
function utf8_safe_clamp(string $text, int $limit): string
{
    if (strlen($text) <= $limit) {
        return $text;
    }
    // Walk codepoints up to the limit; O(n) in the clamped prefix.
    $offset = 0;
    $count = 0;
    $length = strlen($text);
    while ($offset < $length && $count < $limit) {
        $byte = ord($text[$offset]);
        $offset += $byte < 0x80 ? 1 : ($byte < 0xE0 ? 2 : ($byte < 0xF0 ? 3 : 4));
        $count++;
    }
    return substr($text, 0, $offset);
}

/**
 * One in-envelope error for a non-unavailable failed analysis:
 * infrastructure class, non-retryable as-is (the runner must refresh
 * the receipt; retrying the kernel request cannot fix staleness). The
 * message is clamped to the wire's 256-codepoint bound, codepoint-safe.
 *
 * @return array<string, mixed>
 */
function mago_analysis_error(AnalysisOutcome $outcome): array
{
    return [
        'class' => 'infrastructure',
        'code' => 'analysis-' . $outcome->state,
        'message' => utf8_safe_clamp('analyzer evidence is ' . $outcome->state . ': ' . (string) ($outcome->reason ?? 'unspecified'), 256),
        'retryable' => false,
        'partial' => false,
    ];
}

/**
 * One normalized strict-profile row rendered as the bounded wire
 * finding. Wire rules: `path` must be a lowercase logical path, so a
 * native evidence path (Laravel-cased, e.g. `app/Models/User.php`) is
 * lowercased for the wire grammar — and the EXACT native spelling is
 * preserved by leading the detail text, because on case-sensitive
 * filesystems the lowercased spelling points at a nonexistent file.
 * Two cased siblings (User.php / user.php in one directory — not legal
 * PSR-4, but possible) collide onto one lowercase path; the detail
 * still distinguishes them and the receipt keeps both rows exactly.
 * `detail` is clamped to the wire's 128-codepoint bound, codepoint-safe.
 *
 * @param array<string, mixed> $row
 * @return array<string, mixed>
 */
function mago_wire_finding(array $row): array
{
    $nativePath = (string) $row['path'];
    $path = $nativePath;
    if (!is_logical_path($path)) {
        // Native evidence path → logical path: lowercase segments,
        // traversal already refused by the receipt decoder.
        $path = implode('/', array_map('strtolower', explode('/', $path)));
        if (!is_logical_path($path)) {
            $path = '.lekalo/import/mago/unmappable-finding.json';
        }
    }
    $nativePrefix = $path === $nativePath ? '' : $nativePath . ' — ';
    $detail = utf8_safe_clamp(
        $nativePrefix . (string) $row['original_code'] . ': ' . (string) ($row['message'] ?? $row['rule']),
        128,
    );
    return [
        'path' => $path,
        'code' => (string) $row['rule'],
        'detail' => $detail,
    ];
}

function dispatch(array $request, ?Analyzer $analyzer = null): array
{
    $analyzer ??= new FakeAnalyzer();
    $operation = $request['operation'];
    if ($operation === 'describe') {
        return build_response($request, ['capabilities' => describe_capabilities($analyzer)]);
    }
    switch ($operation) {
        case 'scan':
            return scan_response($request, $analyzer);
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
        case 'validate':
            return validate_response($request, $analyzer);
        case 'verify':
            return verify_response($request, $analyzer);
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
 * The scenario validate/verify exchange over the compiled IR evidence
 * (issue #56): the same read-and-map path generate uses, but no writes
 * ever result. Verify recomputes the expected scenario files and
 * reports one `scenario.drift` finding per drifted, missing, or
 * unreadable file; validate reports the mapper's typed findings (an
 * empty set is an honest empty findings answer).
 */
function scenario_validate_response(array $request): array
{
    $mapped = read_and_map_scenario($request);
    if (isset($mapped['error'])) {
        return $mapped['error'];
    }
    return build_response($request, ['result' => [
        'ok' => true,
        'findings' => array_map(
            static fn (array $finding): array => [
                'path' => $finding['symbol'] ?? ($finding['detail'] ?? 'scenario'),
                'code' => $finding['code'],
                'detail' => $finding['detail'] ?? null,
            ],
            $mapped['findings'],
        ),
    ]]);
}

function scenario_verify_response(array $request): array
{
    $mapped = read_and_map_scenario($request);
    if (isset($mapped['error'])) {
        return $mapped['error'];
    }
    $findings = [];
    foreach ($mapped['findings'] as $finding) {
        $findings[] = [
            'path' => $finding['symbol'] ?? ($finding['detail'] ?? 'scenario'),
            'code' => $finding['code'],
            'detail' => $finding['detail'] ?? null,
        ];
    }
    // The checked-binding join (Node parity): every `mode: checked`
    // binding must join against the observed scan index. An absent
    // index is legal silence — the join has nothing to say.
    $indexText = read_view_file(PHP_OBSERVED_INDEX_PATH);
    $index = null;
    if ($indexText !== null) {
        try {
            $index = json_decode($indexText, true, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException) {
            $index = null;
        }
    }
    foreach (php_join_checked_bindings($mapped['scenario'] ?? null, $index) as $finding) {
        $findings[] = [
            'path' => $finding['symbol'] ?? 'binding',
            'code' => $finding['code'],
            'detail' => $finding['detail'] ?? null,
        ];
    }
    foreach ($mapped['files'] as $file) {
        if (($file['frozen'] ?? false) === true) {
            // Scaffold-once custody: the user-owned test is
            // existence-checked only — its bytes are the user's. A
            // missing test with a surviving marker is a removed
            // scaffold (actionable); with the marker also gone the
            // drift finding on the map already says enough.
            if (!is_file($file['path']) && is_file($file['marker'])) {
                $findings[] = [
                    'path' => $file['path'],
                    'code' => 'scenario.scaffold-missing',
                    'detail' => 'scaffolded-test-removed',
                ];
            }
            continue;
        }
        $expectedDigest = $file['digest'];
        $actual = is_file($file['path']) ? hash_file('sha256', $file['path']) : false;
        if ($actual === false) {
            $findings[] = ['path' => $file['path'], 'code' => 'scenario.drift', 'detail' => 'missing'];
        } elseif ($actual !== substr($expectedDigest, 7)) {
            $findings[] = ['path' => $file['path'], 'code' => 'scenario.drift', 'detail' => 'drifted'];
        }
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $findings]]);
}

/**
 * The generate exchange: a dry run plans the deterministic write set
 * (existence-probing create semantics); an apply echoes the pending
 * plan id and writes exactly those bytes. Applied and declared bytes
 * are one deterministic function of the request.
 */
function generate_response(array $request): array
{
    $artifact = deterministic_generation($request);
    $writes = deterministic_writes($artifact);
    if ($artifact['findings'] !== []) {
        // Capability honesty: a compile-time finding vetoes every write.
        return build_response($request, ['result' => ['writes' => [], 'findings' => $artifact['findings']]]);
    }
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
        $files = $artifact['files']
            ?? [['path' => $artifact['path'], 'bytes' => $artifact['bytes'], 'digest' => $artifact['digest']]];
        if (isset($artifact['ledger'])) {
            // The migration ledger's exact bytes ride beside the
            // migration file so the apply loop can verify both.
            $files[] = [
                'path' => $artifact['ledger']['path'],
                'bytes' => $artifact['ledger']['bytes'],
                'digest' => $artifact['ledger']['digest'],
            ];
        }
        apply_writes($writes, $files);
    }
    return build_response($request, [
        'writes' => $writes,
        'evidence_plan_id' => $request['plan_id'] ?? plan_id($writes),
    ]);
}

/**
 * The plan-clean exchange: deletions only, over the owned artifact.
 * The scaffold scope is user-owned and never enters a delete plan.
 */
function plan_clean_response(array $request): array
{
    $writes = deterministic_writes(deterministic_generation($request));
    // Published migration artifacts are append-only custody (issue
    // #57): the ledger records them, and a generic clean confirmation
    // never retires them. The plan skips them — the deletion plan
    // covers only non-retained owned artifacts — so an orphan sweep
    // can never rewrite migration history. The scaffold scope is
    // user-owned for the same reason (issue #56).
    $plan = [];
    foreach ($writes as $entry) {
        if (retained_artifact($entry['path'])
            || scope_covers(PHP_SCAFFOLD_SCOPE, $entry['path'])) {
            continue;
        }
        $plan[] = ['path' => $entry['path'], 'action' => 'delete'];
    }
    return build_response($request, [
        'writes' => $plan,
        'evidence_plan_id' => plan_id($plan),
    ]);
}

/**
 * Whether one owned artifact path is retained custody: anything under
 * a migrations directory (migration classes and the ledger) is
 * append-only history and never deletable through generic clean.
 */
function retained_artifact(string $path): bool
{
    return str_contains($path, '/migrations/');
}

/** The clean apply: delete exactly the planned paths, echo the plan id. */
function clean_response(array $request): array
{
    $writes = deterministic_writes(deterministic_generation($request));
    // Retained custody mirrors the plan: a migration or ledger path
    // refuses the apply outright instead of silently surviving. The
    // scaffold scope is excluded — never silently kept, never deleted.
    foreach ($writes as $entry) {
        if (retained_artifact($entry['path'])) {
            return build_response($request, [
                'error' => [
                    'class' => 'conflict',
                    'code' => 'retained-artifact',
                    'message' => 'published migrations and their ledger are append-only; clean never deletes them',
                    'retryable' => false,
                    'partial' => false,
                ],
            ]);
        }
    }
    $plan = [];
    foreach ($writes as $entry) {
        if (scope_covers(PHP_SCAFFOLD_SCOPE, $entry['path'])) {
            continue;
        }
        $plan[] = ['path' => $entry['path'], 'action' => 'delete'];
    }
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
 * Apply the declared creates inside the core's private staged view.
 * The kernel trusts the core's sandbox for scope authority; it still
 * refuses paths outside its own declared write scope, protected homes,
 * non-logical paths, and create-on-existing, mirroring the plan
 * semantics core verifies after the child exits. Each write's declared
 * digest must match the bytes it carries: a plan/byte divergence is a
 * kernel bug, never a silent publish.
 */
function apply_writes(array $writes, array $files): void
{
    // Every planned entry writes its own exact bytes: the scenario
    // emitter ships the file list directly; the fixture and migration
    // emitters ship artifact (and ledger) bytes normalized by the
    // caller into the same file-list shape.
    $bytesByPath = [];
    foreach ($files as $file) {
        $bytesByPath[$file['path']] = $file['bytes'];
    }
    foreach ($writes as $entry) {
        $path = $entry['path'];
        if (!isset($bytesByPath[$path])) {
            throw new RequestRefusal('write-denied');
        }
        $bytes = $bytesByPath[$path];
        if (!is_logical_path($path) || protected_home($path) !== null) {
            throw new RequestRefusal('write-denied');
        }
        // The scenario home writes under the project's src tree; the
        // scaffold scope admits the one-shot user-owned emission; every
        // other generated artifact stays inside the runtime-owned
        // `.lekalo/generated/php-laravel/**` home.
        $inScenarioScope = false;
        foreach (SCENARIO_WRITE_SCOPES as $scope) {
            if (scope_covers($scope, $path)) {
                $inScenarioScope = true;
                break;
            }
        }
        if (!scope_covers('.lekalo/generated/php-laravel/**', $path) && !$inScenarioScope
            && !scope_covers(PHP_SCAFFOLD_SCOPE, $path)) {
            throw new RequestRefusal('write-denied');
        }
        $bytes = $bytesByPath[$path] ?? null;
        if ($bytes === null || strlen($bytes) > MAX_FILE_BYTES) {
            throw new RequestRefusal('write-denied');
        }
        if (('sha256:' . hash('sha256', $bytes)) !== $entry['sha256']) {
            throw new RequestRefusal('write-denied');
        }
        $exists = is_file($path);
        if ($exists) {
            if (hash_equals($entry['sha256'], sha256_digest((string) file_get_contents($path)))) {
                // Byte-identical regeneration: the published bytes
                // already equal the plan, so this entry is a true
                // no-op — append-only history stays untouched.
                continue;
            }
            if (($entry['action'] ?? 'create') === 'replace') {
                // A replace is custody's append lane: only the retained
                // ledger may be overwritten, and only with the merged
                // bytes the emitter's assert_append_only proved.
                if (!retained_artifact($path)) {
                    throw new RequestRefusal('write-denied');
                }
            } elseif (retained_artifact($path)) {
                // Create-on-existing with different bytes refuses for
                // append-only custody: a published migration is never
                // silently rewritten. Every other owned path follows
                // the plan — the staged view overwrites with the
                // planned exact bytes (scenario support files are
                // per-scenario and legitimately rewritten).
                throw new RequestRefusal('write-denied');
            }
        }
        $directory = dirname($path);
        if (!is_dir($directory) && !mkdir($directory, 0777, true) && !is_dir($directory)) {
            throw new RequestRefusal('write-denied');
        }
        if (@file_put_contents($path, $bytes) === false) {
            throw new RequestRefusal('write-denied');
        }
    }
}

function delete_write(string $path): void
{
    if (!is_logical_path($path) || protected_home($path) !== null
        || scope_covers(PHP_SCAFFOLD_SCOPE, $path)) {
        // The scaffold scope is user-owned: no kernel path may delete
        // inside it, whatever plan claimed otherwise.
        throw new RequestRefusal('write-denied');
    }
    $inScenarioScope = false;
    foreach (SCENARIO_WRITE_SCOPES as $scope) {
        if (scope_covers($scope, $path)) {
            $inScenarioScope = true;
            break;
        }
    }
    if (!scope_covers('.lekalo/generated/php-laravel/**', $path) && !$inScenarioScope) {
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
        // The production composition: the subprocess-free evidence
        // consumer over the runner-staged receipt inside the declared
        // read view. An absent receipt reports `unavailable` — the exact
        // #54 no-claims posture — while a staged receipt flows through
        // the closed decoder and the pin/compatibility gates. The test
        // double exists for suites and gates only; no request field,
        // environment variable, or argv flag can select it here.
        $analyzer = new MagoEvidenceAnalyzer();
        fwrite(STDOUT, canonical_json(dispatch($request, $analyzer)));
        return 0;
    } catch (RequestRefusal $refusal) {
        fwrite(STDERR, stderr_diagnostic($refusal->getMessage()) . "\n");
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

// ----- scenario compiler module: scenario-map.php -----


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

// ----- scenario compiler module: scenario-emit.php -----


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
    return mb_substr($value, 0, 200);
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
        \$characters = mb_strlen(\$value);
        if (\$characters < 8 || \$characters > 2048) {
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

exit(main());
