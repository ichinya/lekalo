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
    foreach (($lock['probe']['artifacts'] ?? []) as $artifact) {
        if (isset($artifact['binarySha256'])) {
            $toolDigest = (string) $artifact['binarySha256'];
            break;
        }
    }
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

// Unavailable: the real analyzer finds no receipt in this suite's view
// (the suite cwd has no staged .lekalo/import/mago/receipt.json).
$missingOutcome = (new MagoEvidenceAnalyzer())->analyze();
check($missingOutcome->state === 'unavailable', 'an absent receipt is exactly unavailable');

// Incompatible: tool digest differs from the pinned toolchain.
$staleBody = receipt_body();
$staleBody['tool']['digest'] = 'sha256:' . str_repeat('9', 64);
$staged = mago_decode_receipt(receipt_json($staleBody));
$reason = mago_check_compatibility($staged);
check(is_string($reason) && str_contains($reason, 'digest'), 'a foreign tool digest is incompatible');

// Failed: the recorded run did not complete. The FakeAnalyzer decode
// path is exercised directly: a receipt whose completion says `failed`
// must never surface as an ok outcome.
$failedBody = receipt_body();
$failedBody['completion']['status'] = 'failed';
$failedAnalyzer = new FakeAnalyzer($failedBody, 'ok');
$failedOutcome = $failedAnalyzer->analyze();
check($failedOutcome->state === 'failed', 'an incomplete run reports failed, never ok');
check(is_string($failedOutcome->reason) && str_contains((string) $failedOutcome->reason, 'did not complete'), 'the failed completion reason is explicit');

// Ok: compatible + completed decodes to findings.
$compatible = mago_decode_receipt(receipt_json(receipt_body()));
check(mago_check_compatibility($compatible) === null, 'a pin-compatible receipt passes the compatibility gate');

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
// original code leading the detail; the native evidence path converts
// to a legal lowercase logical path for the wire (the native spelling
// stays visible in the detail text).
$okBody = receipt_body();
$okAnalyzer = new FakeAnalyzer($okBody, 'ok');
$okResponse = validate_response($request, $okAnalyzer);
check($okResponse['status'] === 'ok', 'a current successful analysis answers ok');
check($okResponse['result']['findings'][0]['code'] === 'target.analysis.strict-types', 'the registered rule id is the finding code');
check(str_starts_with((string) $okResponse['result']['findings'][0]['detail'], 'app/Models/User.php — strict-types: '), 'the exact native path precedes the original code in the bounded detail');
check($okResponse['result']['findings'][0]['path'] === 'app/models/user.php', 'a native path is converted to the lowercase logical-path wire grammar');
check(is_logical_path((string) $okResponse['result']['findings'][0]['path']), 'the emitted finding path satisfies the wire logical-path grammar');
check(mb_strlen((string) $okResponse['result']['findings'][0]['detail']) <= 128, 'the detail stays within the wire 128-codepoint bound');
check((bool) preg_match('//u', (string) $okResponse['result']['findings'][0]['detail']), 'the clamped detail is valid UTF-8 (no mid-codepoint cut)');

// Multibyte safety: a message full of multibyte characters clamps on a
// codepoint boundary — valid UTF-8, ≤128 codepoints, envelope lives.
$multibyteBody = receipt_body();
$multibyteBody['diagnostics'][0]['message'] = str_repeat('—–‘’', 20) . str_repeat('“, ”', 10); // ~220 bytes of valid UTF-8: under the 512-byte decoder bound, over the 128-codepoint detail bound
$multibyteResponse = validate_response($request, new FakeAnalyzer($multibyteBody, 'ok'));
check($multibyteResponse['status'] === 'ok', 'a multibyte message does not kill the envelope');
$multibyteDetail = (string) $multibyteResponse['result']['findings'][0]['detail'];
check(mb_strlen($multibyteDetail) <= 128, 'the multibyte detail clamps to the codepoint bound');
check((bool) preg_match('//u', $multibyteDetail) && json_encode($multibyteDetail, JSON_THROW_ON_ERROR) !== false, 'the clamped multibyte detail is valid UTF-8, not a mid-codepoint cut');
check(str_starts_with($multibyteDetail, 'app/Models/User.php — '), 'the native path survives the multibyte clamp');

// Undeclared profile tokens are an in-envelope invalid refusal, never a
// silent downgrade to the default gate.
$bogusRequest = $request;
$bogusRequest['profile'] = 'bogus-profile';
$bogusResponse = validate_response($bogusRequest, $okAnalyzer);
check($bogusResponse['status'] === 'error' && $bogusResponse['error']['code'] === 'profile-undeclared', 'an undeclared profile is refused explicitly');

// Strict + unavailable: per-rule evidence-unsupported rows, never a
// bare clean pass indistinguishable from a real strict run.
$strictUnavailableRequest = $request;
$strictUnavailableRequest['profile'] = STRICT_PROFILE_TOKEN;
$strictUnavailable = validate_response($strictUnavailableRequest, new FakeAnalyzer([], 'unavailable'));
check($strictUnavailable['status'] === 'ok', 'strict+unavailable still answers ok (advice channel)');
check(count($strictUnavailable['result']['findings']) === count(STRICT_RULES), 'every strict rule reports a row when evidence is absent');
$unsupportedDetails = array_map(
    static fn (array $finding): string => (string) $finding['detail'],
    $strictUnavailable['result']['findings'],
);
check(count(array_unique($unsupportedDetails)) === count($unsupportedDetails), 'each strict row names its own unsupported rule id in the detail');
check(count(array_filter($unsupportedDetails, static fn (string $d): bool => str_contains($d, 'lekalo.unsupported:'))) === count(STRICT_RULES), 'every strict row carries its evidence-unsupported marker');
$defaultUnavailable = validate_response($request, new FakeAnalyzer([], 'unavailable'));
check($defaultUnavailable['result']['findings'] === [], 'the default profile keeps the #54 empty no-claims success');

// Default profile: recorded diagnostics ride verbatim; strict-profile
// predicates and unsupported rows ride only the strict profile.
$defaultRequest = $request;
$defaultRequest['profile'] = PROFILE_TOKEN;
$defaultResponse = validate_response($defaultRequest, $okAnalyzer);
$strictRequest = $request;
$strictRequest['profile'] = STRICT_PROFILE_TOKEN;
$strictResponse = validate_response($strictRequest, $okAnalyzer);
check(count($defaultResponse['result']['findings']) === 1, 'the default profile reports recorded diagnostics without strict-predicate rows');
check(count($strictResponse['result']['findings']) > count($defaultResponse['result']['findings']), 'the strict profile adds its predicate/unsupported rows on top');

// Verify mirrors validate; Mago success never satisfies scenarios.
$verifyRequest = $request;
$verifyRequest['operation'] = 'verify';
$verifyResponse = verify_response($verifyRequest, $okAnalyzer);
check($verifyResponse['status'] === 'ok', 'verify answers through the same seam');
check(DECLARED_CAPABILITIES['verify.scenarios'] === 'unsupported', 'verify.scenarios stays unsupported: Mago is not scenario evidence');

// Scan evidence projection: the bounded evidence record attaches to
// the receipt document entry (the join domain is the receipt's own
// governance row, not the disjoint native source paths). The suite
// stages a real receipt file inside a temp read view exactly like the
// runner would, then cleans it up.
$scanRequest = $request;
$scanRequest['operation'] = 'scan';
unset($scanRequest['ir_path']);
$stagedView = sys_get_temp_dir() . '/lekalo-analyzer-suite-' . getmypid();
if (!is_dir($stagedView . '/.lekalo/import/mago')) {
    mkdir($stagedView . '/.lekalo/import/mago', 0777, true);
}
$cwd = getcwd();
chdir($stagedView);
file_put_contents(MAGO_RECEIPT_PATH, receipt_json($okBody));
$scanResponse = scan_response($scanRequest, $okAnalyzer);
unlink(MAGO_RECEIPT_PATH);
chdir($cwd);
check($scanResponse['status'] === 'ok', 'scan answers with evidence when available');
$receiptEntryEvidence = null;
foreach ($scanResponse['result']['entries'] as $entry) {
    if (($entry['path'] ?? '') === MAGO_RECEIPT_PATH && isset($entry['evidence'])) {
        $receiptEntryEvidence = $entry['evidence'];
    }
}
check($receiptEntryEvidence !== null, 'the receipt entry carries the bounded evidence when evidence projects');

// Nine relations cannot fit the eight-reference wire bound: the whole
// claim is refused (null), never truncated silently.
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
$projection = mago_receipt_wire_evidence(new AnalysisOutcome(
    'ok',
    diagnostics: [],
    symbols: $nine['symbols'],
    relations: $nine['relations'],
));
check($projection === null, 'an over-bound graph is refused from the wire projection, not truncated');

$projectionOk = mago_receipt_wire_evidence(new AnalysisOutcome(
    'ok',
    diagnostics: [],
    symbols: $nine['symbols'],
    relations: array_slice($nine['relations'], 0, 8),
));
check($projectionOk !== null && count($projectionOk['references']) === 8, 'eight references project in full');
check(isset($projectionOk['signature']) && is_sha256_digest((string) $projectionOk['signature']), 'the projected signature is a digest over the receipt evidence');

// A relation whose source symbol is unknown is unjoined remainder: the
// whole projection refuses rather than publishing a partial claim.
$unjoined = $nine;
$unjoined['relations'] = [array_merge($nine['relations'][0], ['from' => 'php.fixture.ghost'])];
check(mago_receipt_wire_evidence(new AnalysisOutcome('ok', diagnostics: [], symbols: $unjoined['symbols'], relations: $unjoined['relations'])) === null, 'an unjoinable relation refuses the whole projection');

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
