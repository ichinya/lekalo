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
function deterministic_generation(array $request): array
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

function deterministic_writes(array $request): array
{
    $artifact = deterministic_generation($request);
    $writes = [[
        'path' => $artifact['path'],
        'action' => 'create',
        'sha256' => $artifact['digest'],
    ]];
    // The migration emitter ships its append-only ledger beside the
    // migration file; both must be planned and written atomically.
    if (isset($artifact['ledger'])) {
        $writes[] = [
            'path' => $artifact['ledger']['path'],
            'action' => 'create',
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
    $ledger = [
        'schemaVersion' => 'lekalo/laravel-migration-ledger/v0.4.0',
        'identity' => 'dev.lekalo.laravel-migration-ledger@0.4.0',
        'projectId' => $input['projectId'],
        'engine' => $input['engine'],
        'engineVersion' => $input['engineVersion'],
        'migrations' => [[
            'ordinal' => 1,
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
        ]],
    ];
    $ledgerBytes = canonical_json($ledger) . "\n";
    return [
        'path' => $path,
        'bytes' => $bytes,
        'digest' => sha256_digest($bytes),
        'ledger' => [
            'path' => $ledgerPath,
            'bytes' => $ledgerBytes,
            'digest' => sha256_digest($ledgerBytes),
        ],
        'filename' => $filename,
    ];
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
        $lines[] = '        throw new \'RuntimeException\'(\'lekalo: rollback is not safe for this migration\');';
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
        'write_scopes' => ['.lekalo/generated/php-laravel/**'],
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
 * Mago success never satisfies scenario verification — the named
 * `verify.scenarios` capability stays `unsupported`, so a green Mago
 * run cannot masquerade as a verified scenario (#56 owns those).
 */
function verify_response(array $request, ?Analyzer $analyzer = null): array
{
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
    // Published migration artifacts are append-only custody (issue
    // #57): the ledger records them, and a generic clean confirmation
    // never retires them. The plan skips them — the deletion plan
    // covers only non-retained owned artifacts — so an orphan sweep
    // can never rewrite migration history.
    $plan = [];
    foreach ($writes as $entry) {
        if (retained_artifact($entry['path'])) {
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
    $writes = deterministic_writes($request);
    // Retained custody mirrors the plan: a migration or ledger path
    // refuses the apply outright instead of silently surviving.
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
    // The migration emitter ships a second artifact (the ledger); the
    // byte set is keyed by path so every planned entry writes its own
    // exact bytes.
    $bytesByPath = [$artifact['path'] => $artifact['bytes']];
    if (isset($artifact['ledger'])) {
        $bytesByPath[$artifact['ledger']['path']] = $artifact['ledger']['bytes'];
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
        if (!scope_covers('.lekalo/generated/php-laravel/**', $path)) {
            throw new RequestRefusal('write-denied');
        }
        if (is_file($path)) {
            throw new RequestRefusal('write-denied');
        }
        if (strlen($bytes) > MAX_FILE_BYTES) {
            throw new RequestRefusal('write-denied');
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

exit(main());
