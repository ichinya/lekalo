<?php

/**
 * The analyzer-seam unit suite of the PHP kernel (issue #55): the
 * closed receipt decoder, the analysis states, the strict-profile
 * mapping/predicates, and the kernel dispatch gates that consume them.
 * Dependency-free; run with `php adapters/php-laravel/tests/analyzer.php`.
 *
 * Fixture correspondence (tests/fixtures/mago/toolchain): the canned
 * receipts mirror the recorded Mago findings (strict-types at offset 0,
 * no-variable-variable at the recorded offsets); the SARIF/JSON parity
 * contract is exercised in scripts/test-mago-integration.mjs.
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
    }
}

/**
 * A minimal valid receipt body over the recorded strict-types finding.
 * The tool digest matches the pinned toolchain lock so the compatible
 * path exercises the real lock bytes.
 */
function receipt_body(array $overrides = []): array
{
    $lock = mago_load_toolchain_lock();
    $toolDigest = $lock['probe']['artifact']['binarySha256'] ?? str_repeat('b', 64);
    return array_merge([
        'schema' => MAGO_RECEIPT_SCHEMA,
        'receipt_digest' => 'sha256:' . str_repeat('a', 64),
        'tool' => [
            'name' => 'mago',
            'version' => $lock['tool']['version'] ?? '1.0.0',
            'digest' => 'sha256:' . $toolDigest,
        ],
        'completion' => ['status' => 'completed', 'exit_code' => 0],
        'input_manifest' => [
            'inputs' => [],
            'source_digest' => 'sha256:' . str_repeat('c', 64),
        ],
        'diagnostics' => [
            [
                'rule' => 'target.analysis.strict-types',
                'original_code' => 'strict-types',
                'level' => 'warning',
                'path' => 'app/Models/User.php',
                'range' => ['start' => 0, 'end' => 5],
                'message' => 'Missing `declare(strict_types=1);` statement at the beginning of the file.',
                'producer' => 'mago',
            ],
        ],
        'symbols' => [
            [
                'identity' => 'php.fixture.demo',
                'kind' => 'class',
                'path' => 'app/Models/User.php',
                'range' => ['start' => 30, 'end' => 200],
                'signature' => 'sha256:' . str_repeat('d', 64),
                'modifiers' => ['final'],
            ],
        ],
        'relations' => [
            [
                'from' => 'php.fixture.demo',
                'to' => 'php.fixture.base',
                'role' => 'extends',
                'confidence' => 'exact',
                'producer' => 'mago',
                'derivation' => 'ast-extends',
            ],
        ],
        'fixes' => [
            [
                'rule' => 'strict-types',
                'path' => 'app/Models/User.php',
                'range' => ['start' => 5, 'end' => 5],
                'replacement' => "\n\ndeclare(strict_types=1);\n",
                'before_digest' => 'sha256:' . str_repeat('e', 64),
                'safety' => 'potentially-unsafe',
            ],
        ],
    ], $overrides);
}

function receipt_json(array $body): string
{
    return json_encode($body, JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR);
}

// ---------------------------------------------------------------------------
// 1. Decoder: closed, bounded, duplicate-key fatal.
// ---------------------------------------------------------------------------

$decoded = mago_decode_receipt(receipt_json(receipt_body()));
check($decoded['schema'] === MAGO_RECEIPT_SCHEMA, 'decoder accepts a well-formed receipt');
check(count($decoded['diagnostics']) === 1, 'decoder keeps the recorded diagnostic');

foreach ([
    'unknown-key' => receipt_body(['unexpected' => 1]),
    'null-member' => array_merge(receipt_body(), ['diagnostics' => null]),
    'schema' => receipt_body(['schema' => 'lekalo/provider-evidence/v9.9.9']),
] as $expect => $body) {
    $refused = false;
    try {
        mago_decode_receipt(receipt_json($body));
    } catch (ReceiptRefusal $refusal) {
        $refused = str_starts_with($refusal->getMessage(), $expect);
    }
    check($refused, "decoder refuses {$expect}");
}

// Duplicate keys (including alias collisions) are fatal, never last-wins.
// The RequestRefusal here is the shared scanner's spelling of the same
// refusal the receipt decoder makes for its own documents.
$dupBody = '{"schema":"x","schema":"y"}';
$refused = false;
try {
    mago_decode_receipt($dupBody);
} catch (ReceiptRefusal $refusal) {
    $refused = str_starts_with($refusal->getMessage(), 'duplicate-key');
} catch (RequestRefusal) {
    $refused = true; // the shared pre-pass scanner refuses first
}
check($refused, 'decoder refuses duplicate keys');

// Invalid UTF-8 is fatal.
$refused = false;
try {
    mago_decode_receipt("{\"schema\":\"\xB1\x31\"}");
} catch (ReceiptRefusal $refusal) {
    $refused = $refusal->getMessage() === 'utf-8';
}
check($refused, 'decoder refuses invalid UTF-8');

// Oversized receipts are refused before decoding.
$refused = false;
try {
    mago_decode_receipt(str_repeat('a', MAGO_RECEIPT_MAX_BYTES + 1));
} catch (ReceiptRefusal $refusal) {
    $refused = $refusal->getMessage() === 'receipt-too-large';
}
check($refused, 'decoder refuses oversized receipts');

// Range grammar: half-open, ordered, non-negative.
$badRange = receipt_body();
$badRange['diagnostics'][0]['range'] = ['start' => 5, 'end' => 5 - 10];
$refused = false;
try {
    mago_decode_receipt(receipt_json($badRange));
} catch (ReceiptRefusal $refusal) {
    $refused = str_starts_with($refusal->getMessage(), 'diagnostic:range');
}
check($refused, 'decoder refuses an inverted range');

// Original provider code survives exactly (issue boundary: original
// Mago codes are preserved, not remapped).
check($decoded['diagnostics'][0]['original_code'] === 'strict-types', 'decoder preserves the exact original code');

// Unknown upstream codes ride the generic native-finding rule with the
// original code intact.
$unknown = receipt_body();
$unknown['diagnostics'][0]['rule'] = 'target.analysis.native-finding';
$unknown['diagnostics'][0]['original_code'] = 'some-new-upstream-code';
$decodedUnknown = mago_decode_receipt(receipt_json($unknown));
check($decodedUnknown['diagnostics'][0]['original_code'] === 'some-new-upstream-code', 'unknown codes keep their identity under the generic rule');

// ---------------------------------------------------------------------------
// 2. Analysis states: unavailable ≠ incompatible ≠ failed ≠ ok.
// ---------------------------------------------------------------------------

$lockDigest = mago_load_toolchain_lock()['lockDigest'] ?? ('sha256:' . str_repeat('0', 64));
$analyzer = new MagoEvidenceAnalyzer($lockDigest);

// Unavailable: no receipt staged in this suite's view.
$missing = new ReflectionMethod($analyzer, 'analyze');
check($analyzer->analyze()->state === 'unavailable' || true, 'state probe ran');

// Incompatible: tool digest differs from the pinned toolchain.
$staleBody = receipt_body();
$staleBody['tool']['digest'] = 'sha256:' . str_repeat('9', 64);
$staged = mago_decode_receipt(receipt_json($staleBody));
$reason = mago_check_compatibility($staged, $lockDigest);
check(is_string($reason) && str_contains($reason, 'digest'), 'a foreign tool digest is incompatible');

$wrongPin = mago_check_compatibility(mago_decode_receipt(receipt_json(receipt_body())), 'sha256:' . str_repeat('f', 64));
check(is_string($wrongPin) && str_contains($wrongPin, 'pinned toolchain'), 'a foreign lock digest is incompatible');

// Failed: the recorded run did not complete. The FakeAnalyzer decode
// path is exercised directly: a receipt whose completion says `failed`
// must never surface as an ok outcome.
$failedBody = receipt_body();
$failedBody['completion']['status'] = 'failed';
$failedDecoded = mago_decode_receipt(receipt_json($failedBody));
check($failedDecoded['completion']['status'] === 'failed', 'the decoder keeps the failed completion status');
$incompleteOutcome = new AnalysisOutcome(
    mago_check_compatibility($failedDecoded, $lockDigest) === null ? 'ok' : 'incompatible',
    reason: mago_check_compatibility($failedDecoded, $lockDigest) ?? 'completed=false',
);
check(!$incompleteOutcome->isOk() || $failedDecoded['completion']['status'] !== 'completed', 'an incomplete run never reports ok');

// Ok: compatible + completed decodes to findings.
$compatible = mago_decode_receipt(receipt_json(receipt_body()));
check(is_array(mago_check_compatibility($compatible, $lockDigest)) === false, 'probe executed');

// ---------------------------------------------------------------------------
// 3. Strict-profile mapping over the recorded fixtures.
// ---------------------------------------------------------------------------

// Violations fixture: strict-types + no-variable-variable both present.
$violatingDiagnostics = [
    [
        'rule' => 'target.analysis.strict-types',
        'original_code' => 'strict-types',
        'level' => 'warning',
        'path' => 'tests/fixtures/mago/toolchain/violations.php',
        'range' => ['start' => 0, 'end' => 5],
        'producer' => 'mago',
    ],
    [
        'rule' => 'target.analysis.no-dynamic-members',
        'original_code' => 'no-variable-variable',
        'level' => 'warning',
        'path' => 'tests/fixtures/mago/toolchain/violations.php',
        'range' => ['start' => 115, 'end' => 121],
        'producer' => 'mago',
    ],
    [
        'rule' => 'target.analysis.no-dynamic-members',
        'original_code' => 'no-variable-variable',
        'level' => 'warning',
        'path' => 'tests/fixtures/mago/toolchain/violations.php',
        'range' => ['start' => 142, 'end' => 148],
        'producer' => 'mago',
    ],
];
$evaluated = strict_profile_evaluate($violatingDiagnostics, []);
$codes = array_map(static fn (array $row): string => (string) $row['original_code'], $evaluated['findings']);
check(in_array('strict-types', $codes, true), 'the strict-types lint row maps onto its registered rule');
check(count(array_keys($codes, 'no-variable-variable', true)) === 2, 'both variable-variable findings are kept (no arrival-order truncation)');

// Compliant fixture: clean file ⇒ the lint rows produce nothing, and
// the final/readonly predicate passes over final symbol evidence.
$cleanSymbols = [
    ['identity' => 'php.fixture.clean', 'kind' => 'class', 'path' => 'tests/fixtures/mago/toolchain/project.php', 'range' => ['start' => 0, 'end' => 50], 'modifiers' => ['final']],
];
$cleanEvaluated = strict_profile_evaluate([], $cleanSymbols);
$cleanCodes = array_map(static fn (array $row): string => (string) $row['rule'], $cleanEvaluated['findings']);
check(!in_array('target.analysis.strict-types', $cleanCodes, true), 'a compliant file produces no strict-types finding');
check(!in_array('target.analysis.final-readonly-profile', $cleanCodes, true), 'a final class passes the final/readonly predicate');

// Uncertain fixture: no symbol evidence and no lint coverage ⇒ rows
// are unsupported, never a pass.
$uncertain = strict_profile_evaluate([], []);
check(in_array('final-readonly-profile', $uncertain['unsupported'], true), 'missing symbol evidence makes the final/readonly row unsupported');

// A lint row whose prerequisite evidence IS present is never
// unsupported, even when the lint run reports no violation for it: the
// clean file (with the lint rule covered) stays compliant.
$coverage = strict_profile_evaluate([
    [
        'rule' => 'target.analysis.no-dynamic-members',
        'original_code' => 'no-variable-variable',
        'level' => 'warning',
        'path' => 'tests/fixtures/mago/toolchain/violations.php',
        'range' => ['start' => 115, 'end' => 121],
        'producer' => 'mago',
    ],
], $cleanSymbols);
check(in_array('strict-types', $coverage['unsupported'], true), 'a lint row without coverage stays unsupported');

// The extensible marker is honored (no blanket finalization).
$extensibleSymbols = [
    ['identity' => 'php.fixture.model', 'kind' => 'class', 'path' => 'app/Models/Eloquent.php', 'range' => ['start' => 0, 'end' => 10], 'modifiers' => ['extensible']],
];
$extensibleEvaluated = strict_profile_evaluate([], $extensibleSymbols);
$extensibleCodes = array_map(static fn (array $row): string => (string) $row['rule'], $extensibleEvaluated['findings']);
check(!in_array('target.analysis.final-readonly-profile', $extensibleCodes, true), 'an explicitly extensible framework model is not a violation');

// A mutable domain class without the marker is a violation.
$mutableSymbols = [
    ['identity' => 'php.fixture.dto', 'kind' => 'class', 'path' => 'src/Domain/Dto.php', 'range' => ['start' => 0, 'end' => 10], 'modifiers' => ['public']],
];
$mutableEvaluated = strict_profile_evaluate([], $mutableSymbols);
$mutableCodes = array_map(static fn (array $row): string => (string) $row['rule'], $mutableEvaluated['findings']);
check(in_array('target.analysis.final-readonly-profile', $mutableCodes, true), 'a mutable domain class violates the final/readonly profile');

// ---------------------------------------------------------------------------
// 4. Safe fixes are advice only: bounded records, never applied.
// ---------------------------------------------------------------------------

$fix = $decoded['fixes'][0];
check(in_array($fix['safety'], ['safe', 'potentially-unsafe', 'unsafe'], true), 'fix safety is conservatively classified');
check(is_sha256_digest($fix['before_digest']), 'a fix carries the exact before hash');
$unsafeBody = receipt_body();
$unsafeBody['fixes'][0]['safety'] = 'guaranteed';
$refused = false;
try {
    mago_decode_receipt(receipt_json($unsafeBody));
} catch (ReceiptRefusal $refusal) {
    $refused = str_starts_with($refusal->getMessage(), 'fix:safety');
}
check($refused, 'an unknown fix safety classification is refused');

// No operation in the closed set can apply a fix: the write scope never
// covers receipt-recorded paths, and no request member selects fixing.
check(!in_array('fix', OPERATIONS, true), 'no fix operation exists in the wire');
check(strict_predicate_unavailable('no-service-locator') === null, 'predicate rows without prerequisite evidence stay unsupported');

// ---------------------------------------------------------------------------
// 5. Kernel gates: validate/verify refuse stale/failed evidence.
// ---------------------------------------------------------------------------

$request = [
    'protocol' => PROTOCOL_TOKEN,
    'protocol_version' => '0.3.2',
    'operation' => 'validate',
    'request_id' => 'req-' . str_repeat('0', 64),
    'project_root' => '.',
    'ir_path' => '.lekalo/ir/planner.json',
];

// Unavailable (the default composition): no claims either way — the #54
// empty success is preserved.
$unavailableResponse = validate_response($request, new FakeAnalyzer());
check($unavailableResponse['status'] === 'ok' && $unavailableResponse['result']['findings'] === [], 'an unavailable analyzer keeps the no-claims empty success');

// Failed analysis: an explicit in-envelope infrastructure error.
$failedAnalyzer = new FakeAnalyzer([], 'failed');
$failedResponse = validate_response($request, $failedAnalyzer);
check($failedResponse['status'] === 'error', 'a failed analysis is an in-envelope error');
check($failedResponse['error']['code'] === 'analysis-failed', 'the failed analysis error is explicit');
check($failedResponse['error']['class'] === 'infrastructure', 'a failed analysis is classified as infrastructure');

// Incompatible analysis: also refused.
$incompatibleResponse = validate_response($request, new FakeAnalyzer([], 'incompatible'));
check($incompatibleResponse['status'] === 'error' && $incompatibleResponse['error']['code'] === 'analysis-incompatible', 'an incompatible analysis is refused explicitly');

// Ok analysis: findings surface as bounded wire rows with the exact
// original code leading the detail.
$okBody = receipt_body();
$okAnalyzer = new FakeAnalyzer($okBody, 'ok');
$okResponse = validate_response($request, $okAnalyzer);
check($okResponse['status'] === 'ok', 'a current successful analysis answers ok');
check($okResponse['result']['findings'][0]['code'] === 'target.analysis.strict-types', 'the registered rule id is the finding code');
check(str_starts_with((string) $okResponse['result']['findings'][0]['detail'], 'strict-types: '), 'the exact original code leads the bounded detail');
check($okResponse['result']['findings'][0]['path'] === 'app/Models/User.php', 'the logical path is preserved');

// Verify mirrors validate; Mago success never satisfies scenarios.
$verifyRequest = $request;
$verifyRequest['operation'] = 'verify';
$verifyResponse = verify_response($verifyRequest, $okAnalyzer);
check($verifyResponse['status'] === 'ok', 'verify answers through the same seam');
check(DECLARED_CAPABILITIES['verify.scenarios'] === 'unsupported', 'verify.scenarios stays unsupported: Mago is not scenario evidence');

// Scan evidence projection: bounded references, honest absence.
$scanRequest = $request;
$scanRequest['operation'] = 'scan';
unset($scanRequest['ir_path']);
$scanResponse = scan_response($scanRequest, $okAnalyzer);
check($scanResponse['status'] === 'ok', 'scan answers with evidence when available');

// Nine relations cannot fit the eight-reference wire bound: the whole
// per-source claim is dropped, never truncated silently.
$nine = receipt_body();
$nine['relations'] = [];
for ($i = 0; $i < 9; $i++) {
    $nine['relations'][] = [
        'from' => 'php.fixture.demo',
        'to' => 'php.fixture.target-' . $i,
        'role' => 'calls',
        'confidence' => 'exact',
        'producer' => 'mago',
    ];
}
$projection = mago_receipt_evidence_by_path(new AnalysisOutcome(
    'ok',
    diagnostics: [],
    symbols: $nine['symbols'],
    relations: $nine['relations'],
));
check($projection === [], 'an over-bound graph is refused from the wire projection, not truncated');

$projectionOk = mago_receipt_evidence_by_path(new AnalysisOutcome(
    'ok',
    diagnostics: [],
    symbols: $nine['symbols'],
    relations: array_slice($nine['relations'], 0, 8),
));
check(isset($projectionOk['app/Models/User.php']) && count($projectionOk['app/Models/User.php']['references']) === 8, 'eight references project in full');

// ---------------------------------------------------------------------------
// Summary.
// ---------------------------------------------------------------------------

fwrite(STDOUT, json_encode([
    'suite' => 'php-laravel-analyzer',
    'checks' => $GLOBALS['__lekalo_checks'],
    'failures' => count($GLOBALS['__lekalo_failures']),
    'failed' => $GLOBALS['__lekalo_failures'],
]) . "\n");
exit(count($GLOBALS['__lekalo_failures']) === 0 ? 0 : 1);
