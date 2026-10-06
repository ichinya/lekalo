<?php
// The per-exchange process lifetime (issue #106): read exactly one
// bounded request from stdin or the host-owned --lekalo-request-file
// coordinate, decode and validate it, dispatch one operation callback,
// and emit exactly one compact canonical response on stdout.

namespace Lekalo\AdapterSdk\Runtime;

use Lekalo\AdapterSdk\Generated\Protocol;

final class Serve {
    /**
     * Serve exactly one exchange; returns the process exit code.
     *
     * The adapter provides an `identity` array and a `handlers` map of
     * callables. The `describe` handler returns the capability map; every
     * other handler returns builder fragments `{payload, planId,
     * progress, writes}` or an `error` array `{errorClass, errorCode,
     * message}` for a closed in-envelope refusal.
     */
    public static function serve(array $adapter): int {
        $identity = $adapter['identity'];
        $tag = $identity['id'] ?? 'adapter';
        $fileRequested = self::hasRequestFileFlag();
        if ($fileRequested && self::stdinPiped()) {
            Logger::error($tag, 'transport: ambiguous input sources');
            return 1;
        }
        $raw = self::readRequestInput($fileRequested);
        if ($raw === null) {
            Logger::error($tag, 'transport: request input unreadable');
            return 1;
        }
        $decoded = Decode::admitRequest($raw);
        if (!$decoded['ok']) {
            Logger::error($tag, 'request refused: ' . $decoded['reason']);
            return 1;
        }
        $request = $decoded['value'];
        $validated = Decode::validateRequest($request);
        if ($validated !== null) {
            Logger::error($tag, 'request refused: ' . $validated['reason']);
            return 1;
        }
        $operation = $request['operation'];
        $handlers = $adapter['handlers'] ?? [];
        $handler = $handlers[$operation] ?? null;
        if (!is_callable($handler)) {
            $response = Build::buildErrorEnvelope($request, $identity, 'unsupported', 'operation-unsupported', 'the operation is not implemented by this adapter');
            if ($response === null) {
                return 1;
            }
            fwrite(STDOUT, Build::responseBytes($response));
            return 0;
        }
        $output = $handler($request);
        $response = self::buildResponse($request, $identity, $operation, is_array($output) ? $output : []);
        if ($response === null) {
            Logger::error($tag, 'response refused: builder');
            $response = Build::buildErrorEnvelope($request, $identity, 'infrastructure', 'sdk-build-refused', 'the adapter refused to emit an invalid response');
            if ($response === null) {
                return 1;
            }
        }
        fwrite(STDOUT, Build::responseBytes($response));
        return 0;
    }

    private static function buildResponse(array $request, array $identity, string $operation, array $output): ?array {
        if (isset($output['error'])) {
            $error = $output['error'];
            return Build::buildErrorEnvelope($request, $identity, $error['errorClass'], $error['errorCode'], $error['message']);
        }
        if ($operation === 'describe') {
            return Build::buildSuccess($request, $identity, $output, null, null, null, null);
        }
        return Build::buildSuccess(
            $request,
            $identity,
            null,
            $output['payload'] ?? null,
            $output['writes'] ?? null,
            $output['progress'] ?? null,
            isset($output['planId']) ? (string) $output['planId'] : null
        );
    }

    private static function hasRequestFileFlag(): bool {
        foreach ($_SERVER['argv'] ?? [] as $argument) {
            if ($argument === '--lekalo-request-file') {
                return true;
            }
        }
        return false;
    }

    private static function stdinPiped(): bool {
        return true;
    }

    private static function readRequestInput(bool $fileRequested): ?string {
        if ($fileRequested) {
            $argv = $_SERVER['argv'] ?? [];
            foreach ($argv as $index => $argument) {
                if ($argument === '--lekalo-request-file') {
                    $path = $argv[$index + 1] ?? null;
                    if ($path === null) {
                        return null;
                    }
                    $bytes = @file_get_contents($path);
                    return $bytes === false ? null : $bytes;
                }
            }
            return null;
        }
        $chunks = [];
        $total = 0;
        $stream = fopen('php://stdin', 'rb');
        if ($stream === false) {
            return null;
        }
        while (!feof($stream) && $total <= Protocol::BOUNDS['MAX_REQUEST_BYTES']) {
            $chunk = fread($stream, 65536);
            if ($chunk === false || $chunk === '') {
                break;
            }
            $chunks[] = $chunk;
            $total += strlen($chunk);
        }
        fclose($stream);
        return implode('', $chunks);
    }
}
