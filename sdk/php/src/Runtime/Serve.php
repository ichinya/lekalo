<?php
// The per-exchange process lifetime (issue #106): read exactly one
// bounded request from stdin or the host-owned --lekalo-request-file
// coordinate, decode and validate it, dispatch one operation callback,
// and emit exactly one compact canonical response on stdout. The
// host-owned file coordinate wins whenever present; a piped stdin next
// to it is the ordinary host shape, never an ambiguity.

namespace Lekalo\AdapterSdk\Runtime;

use Lekalo\AdapterSdk\Generated\Protocol;

/**
 * The one input-source contract (issue #106), implemented identically in
 * all four runtimes: the host-owned `--lekalo-request-file` coordinate
 * wins whenever present. A piped stdin alongside it is the ordinary host
 * shape (the host always spawns adapters with piped stdio), so pipe-ness
 * is never ambiguous. Empty stdin without a file flag is the only
 * missing-input refusal. Emission is bounded: the completed envelope is
 * serialized and measured before a single stdout byte is written, and a
 * response over the accepted output ceiling refuses instead of writing a
 * truncated ok.
 */
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
            return self::emit($response);
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
        return self::emit($response);
    }

    /**
     * Bounded emission (issue #106): the completed envelope is serialized
     * into a bounded buffer and measured before a single stdout byte is
     * written; a response over the accepted output ceiling refuses with a
     * nonzero exit instead of writing a truncated ok.
     */
    private static function emit(array $response): int {
        $bytes = Build::responseBytes($response);
        if (strlen($bytes) > Protocol::BOUNDS['DEFAULT_MAX_OUTPUT_BYTES']) {
            return 1;
        }
        fwrite(STDOUT, $bytes);
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

    /**
     * Diagnostics-only stdin probe retained for host-side tooling: since
     * issue #106 pipe-ness never refuses an exchange (the host always
     * spawns adapters with piped stdin, including on file transport), so
     * the serve path no longer consults it.
     */
    private static function stdinPiped(): bool {
        $spec = stat(STDIN);
        if ($spec === false) {
            return false;
        }
        return ($spec['mode'] & 0x02000) === 0; // S_IFCHR: character device.
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
