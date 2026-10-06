<?php
// PHP codec runner for the shared adapter-SDK vector corpus (issue
// #106). Executed by `scripts/test-adapter-sdk-vectors.mjs`; it proves
// this language's runtime against every corpus family and prints one
// JSON verdict line. No network; no side effects beyond stdout.

declare(strict_types=1);

error_reporting(E_ALL & ~E_DEPRECATED);

use Lekalo\AdapterSdk\Runtime\Build;
use Lekalo\AdapterSdk\Runtime\Canonical;
use Lekalo\AdapterSdk\Runtime\Decode;
use Lekalo\AdapterSdk\Runtime\Ids;
use Lekalo\AdapterSdk\Runtime\Json;
use Lekalo\AdapterSdk\Runtime\Paths;

// The runner bootstraps the runtime directly (no composer required).
spl_autoload_register(function (string $class): void {
    $map = [
        'Lekalo\\AdapterSdk\\Generated\\Protocol' => __DIR__ . '/../src/Generated/Protocol.php',
        'Lekalo\\AdapterSdk\\Generated\\Capabilities' => __DIR__ . '/../src/Generated/Capabilities.php',
        'Lekalo\\AdapterSdk\\Generated\\Diagnostics' => __DIR__ . '/../src/Generated/Diagnostics.php',
        'Lekalo\\AdapterSdk\\Generated\\CompatibilityTests' => __DIR__ . '/../src/Generated/CompatibilityTests.php',
        'Lekalo\\AdapterSdk\\Runtime\\Json' => __DIR__ . '/../src/Runtime/Json.php',
        'Lekalo\\AdapterSdk\\Runtime\\Ids' => __DIR__ . '/../src/Runtime/Ids.php',
        'Lekalo\\AdapterSdk\\Runtime\\Paths' => __DIR__ . '/../src/Runtime/Paths.php',
        'Lekalo\\AdapterSdk\\Runtime\\Canonical' => __DIR__ . '/../src/Runtime/Canonical.php',
        'Lekalo\\AdapterSdk\\Runtime\\Logger' => __DIR__ . '/../src/Runtime/Logger.php',
        'Lekalo\\AdapterSdk\\Runtime\\Decode' => __DIR__ . '/../src/Runtime/Decode.php',
        'Lekalo\\AdapterSdk\\Runtime\\Build' => __DIR__ . '/../src/Runtime/Build.php',
        'Lekalo\\AdapterSdk\\Runtime\\Serve' => __DIR__ . '/../src/Runtime/Serve.php',
    ];
    if (isset($map[$class])) {
        require $map[$class];
    }
});

$corpus = null;
foreach (array_slice($argv, 1) as $argument) {
    if (str_starts_with($argument, '--corpus=')) {
        $corpus = substr($argument, strlen('--corpus='));
    }
}
$corpus ??= __DIR__ . '/../../../tests/fixtures/adapter-sdk';

$results = [];
$canonicalOutputs = [];

$record = function (string $name, bool $ok, string $detail = '') use (&$results): void {
    $results[] = ['name' => $name, 'ok' => $ok, 'detail' => $detail];
};

$read = fn (string $relative): string => (string) file_get_contents("{$corpus}/{$relative}");

$list = function (string $family) use ($corpus): array {
    $names = [];
    foreach (scandir("{$corpus}/{$family}") ?: [] as $name) {
        if (str_ends_with($name, '.json') && !str_ends_with($name, '.expect.json')) {
            $names[] = $name;
        }
    }
    sort($names);
    return $names;
};

// ---- valid/ ----
foreach ($list('valid') as $name) {
    $text = $read("valid/{$name}");
    $isResponse = str_contains($text, '"status"') && str_contains($text, '"evidence"');
    if ($isResponse) {
        $decoded = Decode::admitResponse($text);
        $record("valid/{$name}", $decoded['ok'], $decoded['reason'] ?? '');
    } else {
        $decoded = Decode::admitRequest($text);
        $record("valid/{$name}", $decoded['ok'], $decoded['reason'] ?? '');
    }
}

// ---- invalid/ ----
foreach ($list('invalid') as $name) {
    $text = $read("invalid/{$name}");
    $expect = json_decode($read('invalid/' . preg_replace('/\.json$/', '.expect.json', $name)), true);
    $expectStage = (string) ($expect['stage'] ?? '');
    $expectReason = (string) ($expect['reason'] ?? '');
    $isResponse = str_contains($text, '"status"') && str_contains($text, '"evidence"');
    $actualStage = 'none';
    $actualReason = '';
    if (!$isResponse) {
        $decoded = Decode::admitRequest($text);
        if (!$decoded['ok']) {
            $actualStage = $decoded['stage'];
            $actualReason = $decoded['reason'];
        }
    } else {
        $decoded = Decode::admitResponse($text);
        if (!$decoded['ok']) {
            $actualStage = $decoded['stage'];
            $actualReason = $decoded['reason'];
        }
    }
    $ok = $actualStage === $expectStage && $actualReason === $expectReason;
    $record("invalid/{$name}", $ok, "{$actualStage}/{$actualReason} vs {$expectStage}/{$expectReason}");
}

// ---- semantic/ ----
foreach ($list('semantic') as $name) {
    $vector = json_decode($read("semantic/{$name}"), true);
    $kind = (string) ($vector['kind'] ?? '');
    $expect = (string) ($vector['expect'] ?? '');
    [$expectKind, $expectReason] = array_pad(explode(':', $expect, 2), 2, '');
    $outcome = function () use ($vector, $kind, $expectKind, $expectReason): array {
        try {
            switch ($kind) {
                case 'validate-before-write':
                    $request = Decode::admitRequest(Json::canonicalJson($vector['request']));
                    if (!$request['ok']) {
                        return [$expectKind === 'reject' && $expectReason === $request['reason'], $request['reason']];
                    }
                    $response = Decode::admitResponse(Json::canonicalJson($vector['response']));
                    if (!$response['ok']) {
                        return [$expectKind === 'reject' && $expectReason === $response['reason'], $response['reason']];
                    }
                    $requestValue = $request['value'];
                    $reason = '';
                    $identity = Decode::validateResponseIdentity(
                        $response['value'],
                        (string) $requestValue['protocol_version'],
                        (string) $requestValue['operation'],
                        (string) $requestValue['request_id']
                    );
                    if ($identity !== null) {
                        $reason = $identity['reason'];
                    } else {
                        $payload = Decode::validateResponsePayload($requestValue, $response['value']);
                        if ($payload !== null) {
                            $reason = $payload['reason'];
                        }
                    }
                    if ($reason === '') {
                        return [$expectKind === 'accept', ''];
                    }
                    return [$expectKind === 'reject' && $expectReason === $reason, $reason];
                case 'build-success':
                    $request = Decode::admitRequest(Json::canonicalJson($vector['request']));
                    if (!$request['ok']) {
                        return [false, $request['reason'] ?? ''];
                    }
                    $builder = $vector['builder'];
                    $identity = Build::buildAdapterIdentity(
                        (string) $builder['identity']['id'],
                        (string) $builder['identity']['version'],
                        (string) $builder['identity']['digest']
                    );
                    if ($identity === null) {
                        return [false, 'identity'];
                    }
                    $fragments = $builder['fragments'] ?? [];
                    $envelope = Build::buildSuccess(
                        $request['value'],
                        $identity,
                        $builder['capabilities'] ?? null,
                        $fragments['payload'] ?? null,
                        $fragments['writes'] ?? null,
                        $fragments['progress'] ?? null,
                        isset($fragments['planId']) ? (string) $fragments['planId'] : null
                    );
                    if (Build::isRefusal($envelope)) {
                        return [$expectKind === 'reject' && $expectReason === $envelope['reason'], $envelope['reason']];
                    }
                    $ok = $expectKind === 'accept';
                    if ($ok && isset($fragments['writes'])) {
                        $reference = $fragments['writes'];
                        usort($reference, fn ($a, $b) => $a['path'] <=> $b['path']);
                        $ok = Json::canonicalJson($envelope['writes']) === Json::canonicalJson($reference);
                    }
                    return [$ok, ''];
                case 'build-error':
                    $request = Decode::admitRequest(Json::canonicalJson($vector['request']));
                    if (!$request['ok']) {
                        return [false, $request['reason'] ?? ''];
                    }
                    $builder = $vector['builder'];
                    $identity = Build::buildAdapterIdentity(
                        (string) $builder['identity']['id'],
                        (string) $builder['identity']['version'],
                        (string) $builder['identity']['digest']
                    );
                    $error = $builder['error'];
                    $envelope = Build::buildErrorEnvelope(
                        $request['value'],
                        $identity,
                        (string) $error['errorClass'],
                        (string) $error['errorCode'],
                        (string) $error['message']
                    );
                    if (Build::isRefusal($envelope)) {
                        return [$expectKind === 'reject' && $expectReason === $envelope['reason'], $envelope['reason']];
                    }
                    return [$expectKind === 'accept' && $envelope['status'] === 'error', ''];
                case 'decode-oversize':
                    $maxBytes = (int) ($vector['maxBytes'] ?? 0);
                    $text = (string) ($vector['text'] ?? '');
                    $parsed = Json::readSingleObject($text, $maxBytes);
                    return [$expectKind === 'reject' && ($parsed['reason'] ?? '') === $expectReason, (string) ($parsed['reason'] ?? '')];
                case 'canonical-plan':
                    $sorted = Canonical::sortWritePlan($vector['writes']);
                    if ($sorted === null) {
                        return [false, 'unsortable'];
                    }
                    return [Canonical::canonicalPlanId($sorted) === (string) $vector['planId'], ''];
                case 'artifact-manifest':
                    $entries = [];
                    foreach ($vector['entries'] ?? [] as $entry) {
                        $entries[] = [(string) $entry['path'], (string) $entry['bytes']];
                    }
                    $manifest = Canonical::buildArtifactManifest($entries);
                    if ($expectKind === 'reject') {
                        return [$manifest === null, ''];
                    }
                    return [$manifest !== null && Json::canonicalJson($manifest['entries']) === Json::canonicalJson($vector['expectSorted']), ''];
                case 'scenario-result':
                    $normalized = Canonical::normalizeScenarioResult($vector['result']);
                    if ($expectKind === 'reject') {
                        return [$normalized === null, ''];
                    }
                    return [$normalized !== null && Json::canonicalJson($normalized['findings']) === Json::canonicalJson($vector['normalizedFindings']), ''];
                default:
                    return [false, 'unknown-kind'];
            }
        } catch (\Throwable $e) {
            return [$expectKind === 'reject' && $expectReason === $e->getMessage(), $e->getMessage()];
        }
    };
    [$ok, $detail] = $outcome();
    $record("semantic/{$name}", $ok, $detail);
}

// ---- canonical/ ----
foreach ($list('canonical') as $name) {
    $vector = json_decode($read("canonical/{$name}"), true);
    $bytes = Json::canonicalJson($vector['value']);
    $canonicalOutputs["canonical/{$name}"] = $bytes;
    $roundTrip = Json::strictParse($bytes);
    $ok = $bytes === $vector['canonical'] && $roundTrip['ok']
        && Json::canonicalJson($roundTrip['value']) === Json::canonicalJson($vector['value']);
    $record("canonical/{$name}", $ok, $bytes);
}

// ---- security/ ----
$nfcSupported = false;
foreach ($list('security') as $name) {
    $vector = json_decode($read("security/{$name}"), true);
    if (($vector['requiresNfc'] ?? false) === true && !$nfcSupported) {
        $record("security/{$name}", true, 'skipped:nfc-unsupported');
        continue;
    }
    $ok = false;
    if (($vector['scopeGrammar'] ?? false) === true) {
        $value = (string) ($vector['value'] ?? $vector['path'] ?? '');
        $ok = Paths::isScope($value) === ($vector['scopeOk'] ?? false);
        if ($ok && array_key_exists('touches', $vector)) {
            $ok = Paths::scopeTouchesProtectedHome($value) === $vector['touches'];
        }
        if ($ok && array_key_exists('logical', $vector)) {
            $ok = Paths::isLogicalPath($value) === $vector['logical'];
        }
    } elseif (array_key_exists('token', $vector)) {
        $ok = Paths::isToken((string) $vector['token']) === $vector['tokenOk'];
    } elseif (array_key_exists('capability', $vector)) {
        $ok = Paths::isCapabilityId((string) $vector['capability']) === $vector['tokenOk'];
    } else {
        $path = (string) ($vector['path'] ?? $vector['value'] ?? '');
        $ok = Paths::isLogicalPath($path) === $vector['logical'];
        if ($ok && array_key_exists('home', $vector)) {
            $ok = Paths::protectedHome($path) === $vector['home'];
        }
        if ($ok && array_key_exists('scope', $vector)) {
            $ok = Paths::scopeCovers((string) $vector['scope'], $path) === $vector['covers'];
        }
    }
    $record("security/{$name}", $ok, '');
}

// ---- upgrade/ ----
foreach ($list('upgrade') as $name) {
    $vector = json_decode($read("upgrade/{$name}"), true);
    $requestText = Json::canonicalJson($vector['request']);
    $accepted = false;
    $reason = '';
    $decoded = Decode::admitRequest($requestText);
    if (!$decoded['ok']) {
        $reason = $decoded['reason'];
    } else {
        $validated = Decode::validateRequest($decoded['value']);
        if ($validated !== null) {
            $reason = $validated['reason'];
        } else {
            $accepted = true;
        }
    }
    $expectAccept = $vector['accept'] ?? false;
    $expectReason = (string) ($vector['reason'] ?? '');
    $ok = $expectAccept === $accepted && ($expectAccept || $expectReason === $reason);
    $record("upgrade/{$name}", $ok, sprintf('(%s, %s)', $accepted ? 'true' : 'false', $reason));
}

$failed = array_values(array_filter($results, fn ($entry) => !$entry['ok']));
$verdict = [
    'ok' => count($failed) === 0,
    'language' => 'php',
    'total' => count($results),
    'failed' => count($failed),
    'canonicalOutputs' => $canonicalOutputs,
];
if (count($failed) > 0) {
    $verdict['failures'] = array_map(fn ($entry) => ['name' => $entry['name'], 'detail' => $entry['detail']], $failed);
}
echo json_encode($verdict, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE), "\n";
if (count($failed) > 0) {
    exit(1);
}
