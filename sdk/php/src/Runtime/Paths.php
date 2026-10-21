<?php
declare(strict_types=1);
// Logical path, scope, and protected-home helpers (issue #106):
// defense in depth mirroring the host's closed grammar. The NFC table
// check stays host-owned; vectors mark it requiresNfc and PHP runtimes
// skip those vectors explicitly.

namespace Lekalo\AdapterSdk\Runtime;

final class Paths {
    private const MAX_VALUE_BYTES = 512;
    private const MAX_SEGMENT_BYTES = 64;

    private const PROTECTED_HOMES = [
        'lekalo-model' => ['lekalo'],
        'lekalo-lockfile' => ['lekalo.lock'],
        'ir' => ['.lekalo', 'ir'],
        'cache' => ['.lekalo', 'cache'],
        'import' => ['.lekalo', 'import'],
        'privacy' => ['.lekalo', 'privacy'],
        'consumer' => ['.lekalo', 'consumer'],
        'openspec' => ['openspec'],
    ];

    private const DOS_DEVICES = [
        'CON', 'PRN', 'AUX', 'NUL', 'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8',
        'COM9', 'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
    ];

    /** The closed host path-violation reason, or null. */
    public static function pathViolation(string $path): ?string {
        if ($path === '') {
            return 'structure.path-empty';
        }
        if ($path[0] === '/' || str_starts_with($path, '\\\\') || str_starts_with($path, '//')) {
            return 'structure.path-absolute';
        }
        if ($path[0] === '~' || self::isDriveOrScheme($path)) {
            return 'structure.path-absolute';
        }
        if (str_contains($path, '\\') || self::containsPercentEscape($path) || str_contains($path, ':')) {
            return 'structure.path-escape';
        }
        foreach (explode('/', $path) as $segment) {
            if ($segment === '' || strlen($segment) > 64) {
                return 'structure.path-segment';
            }
            if ($segment === '.' || $segment === '..' || str_ends_with($segment, '.') || str_ends_with($segment, ' ')) {
                return 'structure.path-traversal';
            }
            if (in_array(self::dosStem($segment), self::DOS_DEVICES, true)) {
                return 'structure.path-device';
            }
            if (self::containsShortName($segment)) {
                return 'structure.path-short-name';
            }
            if (preg_match('/[A-Z]/', $segment) === 1) {
                return 'structure.path-case';
            }
        }
        return null;
    }

    private static function isDriveOrScheme(string $path): bool {
        // Wire grammar is ASCII and must work without optional ext-ctype
        // or locale-dependent character classes in a confined PHP runtime.
        if (strlen($path) >= 2 && preg_match('/\A[A-Za-z]\z/', $path[0]) === 1 && $path[1] === ':') {
            return true;
        }
        $colon = strpos($path, ':');
        if ($colon === false || $colon === 0) {
            return false;
        }
        $stem = substr($path, 0, $colon);
        if (preg_match('/\A[a-z]\z/', $stem[0]) !== 1) {
            return false;
        }
        return preg_match('/^[a-z0-9+.-]+\z/', $stem) === 1;
    }

    private static function dosStem(string $segment): string {
        $dot = strpos($segment, '.');
        $stem = $dot === false ? $segment : substr($segment, 0, $dot);
        return strtoupper($stem);
    }

    private static function containsShortName(string $segment): bool {
        $tilt = strpos($segment, '~');
        if ($tilt === false || $tilt < 1 || $tilt > 6 || $tilt + 1 >= strlen($segment)) {
            return false;
        }
        return preg_match('/\A[0-9]\z/', $segment[$tilt + 1]) === 1;
    }

    private static function containsPercentEscape(string $path): bool {
        return preg_match('/%[0-9a-fA-F]{2}/', $path) === 1;
    }

    private static function segmentOk(string $segment): bool {
        if ($segment === '.' || $segment === '..' || str_ends_with($segment, '.')) {
            return false;
        }
        $portable = str_starts_with($segment, '.') ? 'x' . substr($segment, 1) : $segment;
        if (self::pathViolation($portable) !== null) {
            return false;
        }
        if ($segment === '' || strlen($segment) > self::MAX_SEGMENT_BYTES) {
            return false;
        }
        $first = $segment[0];
        if (!(($first >= 'a' && $first <= 'z') || ($first >= '0' && $first <= '9') || $first === '.')) {
            return false;
        }
        return preg_match('/^[a-z0-9._-]*\z/', $segment) === 1;
    }

    /** @return list<string>|null */
    private static function segments(mixed $value): ?array {
        if (!is_string($value) || $value === '' || strlen($value) > self::MAX_VALUE_BYTES || $value[0] === '/') {
            return null;
        }
        $parts = explode('/', $value);
        foreach ($parts as $part) {
            if ($part === '') {
                return null;
            }
        }
        return $parts;
    }

    public static function isLogicalPath(mixed $value): bool {
        $parts = self::segments($value);
        if ($parts === null) {
            return false;
        }
        foreach ($parts as $part) {
            if (!self::segmentOk($part)) {
                return false;
            }
        }
        return true;
    }

    public static function isScope(mixed $value): bool {
        $parts = self::segments($value);
        if ($parts === null) {
            return false;
        }
        $last = $parts[count($parts) - 1];
        $head = array_slice($parts, 0, -1);
        if ($last === '**') {
            if (count($head) === 0) {
                return false;
            }
            foreach ($head as $part) {
                if (!self::segmentOk($part)) {
                    return false;
                }
            }
            return true;
        }
        foreach ($parts as $part) {
            if (!self::segmentOk($part)) {
                return false;
            }
        }
        return true;
    }

    public static function scopeCovers(string $scope, string $path): bool {
        if (!self::isScope($scope) || !self::isLogicalPath($path)) {
            return false;
        }
        $scopeParts = self::segments($scope);
        $pathParts = self::segments($path);
        $recursive = $scopeParts[count($scopeParts) - 1] === '**';
        $prefix = $recursive ? array_slice($scopeParts, 0, -1) : $scopeParts;
        if (count($pathParts) < count($prefix)) {
            return false;
        }
        foreach ($prefix as $index => $part) {
            if ($pathParts[$index] !== $part) {
                return false;
            }
        }
        if ($recursive) {
            return count($pathParts) > count($prefix);
        }
        return count($pathParts) === count($prefix);
    }

    public static function protectedHome(string $path): ?string {
        $parts = self::segments($path);
        if ($parts === null) {
            return null;
        }
        foreach (self::PROTECTED_HOMES as $name => $home) {
            if (count($parts) >= count($home) && self::equalPrefix($parts, $home)) {
                return $name;
            }
        }
        return null;
    }

    public static function scopeTouchesProtectedHome(string $scope): ?string {
        $parts = self::segments($scope);
        if ($parts === null) {
            return null;
        }
        $recursive = $parts[count($parts) - 1] === '**';
        $prefix = $recursive ? array_slice($parts, 0, -1) : $parts;
        foreach (self::PROTECTED_HOMES as $name => $home) {
            $inside = count($prefix) >= count($home) && self::equalPrefix($prefix, $home);
            $above = count($prefix) <= count($home) && self::equalPrefix($home, $prefix);
            if ($inside || $above) {
                return $name;
            }
        }
        return null;
    }

    /** @param list<string> $parts @param list<string> $prefix */
    private static function equalPrefix(array $parts, array $prefix): bool {
        if (count($parts) < count($prefix)) {
            return false;
        }
        foreach ($prefix as $index => $part) {
            if ($parts[$index] !== $part) {
                return false;
            }
        }
        return true;
    }

    public static function isToken(mixed $value): bool {
        if (!is_string($value) || $value === '' || strlen($value) > 64) {
            return false;
        }
        return preg_match('/^[a-z][a-z0-9-]*\z/', $value) === 1;
    }

    /** The manifest-local semantic-id grammar (core
     * trace::id::is_semantic_id, issue #106 review r3 #8): scan-entry
     * evidence reference targets — non-empty, at most 192 bytes, ASCII
     * alphanumeric or `_.:-`, starting alphanumeric or underscore. The
     * SDK used to enforce a length bound only, admitting slash-path
     * targets the production response decoder refuses. */
    public static function isSemanticId(mixed $value): bool {
        if (!is_string($value) || $value === '' || strlen($value) > 192) {
            return false;
        }
        return preg_match('/^[A-Za-z0-9_][A-Za-z0-9_.:-]*\z/', $value) === 1;
    }

    /** The production ai-lint identifier grammar (issue #106 review r3
     * #5): core ai_lint::input::token gates the lint scope items,
     * binding nativeId/symbol, and the revision pin's known value —
     * non-empty, at most 256 bytes, ASCII alphanumeric or
     * `_.:/#@\\-`, starting alphanumeric. The SDK used to enforce a
     * length bound only. */
    public static function isWireToken(mixed $value): bool {
        if (!is_string($value) || $value === '' || strlen($value) > 256) {
            return false;
        }
        return preg_match('/^[A-Za-z0-9][A-Za-z0-9_.:\/#@\\\\-]*\z/', $value) === 1;
    }

    public static function isCapabilityId(mixed $value): bool {
        if (!is_string($value) || $value === '' || strlen($value) > 128) {
            return false;
        }
        foreach (explode('.', $value) as $segment) {
            if ($segment === '' || strlen($segment) > 128) {
                return false;
            }
            $first = $segment[0];
            if (!(($first >= 'a' && $first <= 'z') || ($first >= '0' && $first <= '9'))) {
                return false;
            }
            if (preg_match('/^[a-z0-9_-]+\z/', $segment) !== 1) {
                return false;
            }
        }
        return true;
    }

    public static function isContractVersion(mixed $value): bool {
        if (!is_string($value) || $value === '' || strlen($value) > 32) {
            return false;
        }
        $parts = explode('.', $value);
        if (count($parts) !== 3) {
            return false;
        }
        foreach ($parts as $part) {
            if (preg_match('/\A[0-9]+\z/', $part) !== 1) {
                return false;
            }
        }
        return true;
    }

    /** @param list<string> $writeScopes */
    public static function planScopeViolation(string $path, array $writeScopes): ?string {
        $home = self::protectedHome($path);
        if ($home !== null) {
            return "protected-path:{$home}";
        }
        foreach ($writeScopes as $scope) {
            if (self::scopeCovers($scope, $path)) {
                return null;
            }
        }
        return 'scope:uncovered';
    }
}
