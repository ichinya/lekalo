<?php
// Deterministic identifiers and digest grammar (issue #106).

namespace Lekalo\AdapterSdk\Runtime;

final class Ids {
    public const REQUEST_ID_PREFIX = 'req-';
    public const PLAN_ID_PREFIX = 'plan-';

    /** Lowercase hex SHA-256 of the UTF-8 bytes of the text. */
    public static function sha256Hex(string $text): string {
        return hash('sha256', $text);
    }

    /** The `sha256:<64 hex>` digest spelling. */
    public static function sha256(string $text): string {
        return 'sha256:' . self::sha256Hex($text);
    }

    private static function prefixedHex(string $value, string $prefix): bool {
        $hex = substr($value, 0, strlen($prefix)) === $prefix ? substr($value, strlen($prefix)) : null;
        if ($hex === null || strlen($hex) !== 64) {
            return false;
        }
        return preg_match('/^[0-9a-f]{64}$/', $hex) === 1;
    }

    public static function isSha256Digest(string $value): bool {
        return self::prefixedHex($value, 'sha256:');
    }

    public static function isRequestId(string $value): bool {
        return self::prefixedHex($value, self::REQUEST_ID_PREFIX);
    }

    public static function isPlanId(string $value): bool {
        return self::prefixedHex($value, self::PLAN_ID_PREFIX);
    }

    /** The deterministic request id over the envelope sans request_id. */
    public static function requestId(array $envelope): string {
        unset($envelope['request_id']);
        return self::REQUEST_ID_PREFIX . self::sha256Hex(Json::canonicalJson($envelope));
    }

    /** The deterministic plan id over the canonical bytes of one plan. */
    public static function planId(array $entries): string {
        return self::PLAN_ID_PREFIX . self::sha256Hex(Json::canonicalJson($entries));
    }
}
