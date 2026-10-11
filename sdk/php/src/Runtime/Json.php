<?php
// Strict JSON admission for the adapter SDK (issue #106): closed
// parsing with duplicate-key, trailing-document, explicit-null, and
// non-finite refusals, plus the canonical byte serializer the wire
// requires (byte-sorted keys, compact, no trailing LF).

namespace Lekalo\AdapterSdk\Runtime;

final class Json {
    /** Raw lexer pass: duplicated key spellings, if any. */
    public static function duplicateKeys(string $text): array {
        $offenders = [];
        $stack = [null];
        $index = 0;
        $length = strlen($text);
        $skip = function () use (&$index, $text, $length): void {
            while ($index < $length && in_array($text[$index], [' ', "\t", "\r", "\n"], true)) {
                $index++;
            }
        };
        while ($index < $length) {
            $ch = $text[$index];
            if ($ch === '"') {
                $start = $index;
                $index++;
                while ($index < $length) {
                    $c = $text[$index];
                    if ($c === '\\') {
                        $index += 2;
                        continue;
                    }
                    if ($c === '"') {
                        $index++;
                        break;
                    }
                    $index++;
                }
                $raw = substr($text, $start, $index - $start);
                $key = json_decode($raw, true, 512, JSON_THROW_ON_ERROR);
                $skip();
                if ($index < $length && $text[$index] === ':') {
                    $index++;
                    $frame = $stack[count($stack) - 1];
                    if ($frame !== null) {
                        if (isset($frame[$key])) {
                            $offenders[] = $key;
                        }
                        $stack[count($stack) - 1][$key] = true;
                    }
                }
                continue;
            }
            if ($ch === '{') {
                $stack[] = [];
                $index++;
                continue;
            }
            if ($ch === '}') {
                if (count($stack) > 1) {
                    array_pop($stack);
                }
                $index++;
                continue;
            }
            $index++;
        }
        return $offenders;
    }

    /** The index of the closing bracket of the first document, or -1. */
    private static function documentEnd(string $text): int {
        $depth = 0;
        $inString = false;
        $escaped = false;
        $length = strlen($text);
        for ($index = 0; $index < $length; $index++) {
            $ch = $text[$index];
            if ($inString) {
                if ($escaped) {
                    $escaped = false;
                } elseif ($ch === '\\') {
                    $escaped = true;
                } elseif ($ch === '"') {
                    $inString = false;
                }
                continue;
            }
            if ($ch === '"') {
                $inString = true;
            } elseif ($ch === '{' || $ch === '[') {
                $depth++;
            } elseif ($ch === '}' || $ch === ']') {
                $depth--;
                if ($depth === 0) {
                    return $index;
                }
            }
        }
        return -1;
    }

    private const JSON_WHITESPACE = " \t\r\n";

    /** Strict parse of one complete JSON document (object or array).
     * Only JSON whitespace (space, tab, CR, LF) may surround the
     * document (issue #106 review r4 #4): bare ltrim/trim also strip
     * NUL and vertical tab, admitting control bytes the host's parser
     * refuses. Nesting depth is bounded like the host's parser (issue
     * #106 review r4 #5) through json_decode's depth argument. The
     * parse result also carries the raw-lexeme audit (issue #106 review
     * r4 #3): the member paths whose raw value is a JSON object and
     * whose raw number lexeme is `-0` — the two distinctions
     * json_decode's associative mapping collapses and production's
     * typed decoder refuses. */
    public static function strictParse(string $text): array {
        if (str_starts_with($text, "\xEF\xBB\xBF")) {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        $trimmed = ltrim($text, self::JSON_WHITESPACE);
        $first = $trimmed === '' ? '' : $trimmed[0];
        if ($first !== '{' && $first !== '[') {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        $end = self::documentEnd($trimmed);
        if ($end === -1) {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        if (strlen(rtrim(substr($trimmed, $end + 1), self::JSON_WHITESPACE)) > 0) {
            return ['ok' => false, 'reason' => 'trailing-document'];
        }
        $document = substr($trimmed, 0, $end + 1);
        try {
            $value = json_decode($document, true, 128, JSON_THROW_ON_ERROR);
        } catch (\JsonException $e) {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        if (count(self::duplicateKeys($document)) > 0) {
            return ['ok' => false, 'reason' => 'duplicate-key'];
        }
        $objectPaths = [];
        $zeroPaths = [];
        $auditIndex = 0;
        self::auditValue($document, $auditIndex, '', $objectPaths, $zeroPaths);
        return ['ok' => true, 'value' => $value, 'objectPaths' => array_keys($objectPaths), 'zeroPaths' => array_keys($zeroPaths)];
    }

    private static function skipJsonWhitespace(string $text, int &$index): void {
        $length = strlen($text);
        while ($index < $length && strpos(self::JSON_WHITESPACE, $text[$index]) !== false) {
            $index++;
        }
    }

    /** Consume one quoted JSON string starting at $index; returns the
     * decoded text or null when json_decode refuses the token. */
    private static function auditString(string $text, int &$index): ?string {
        $start = $index;
        $index++;
        $length = strlen($text);
        while ($index < $length) {
            $c = $text[$index];
            if ($c === '\\') {
                $index += 2;
                continue;
            }
            if ($c === '"') {
                $index++;
                break;
            }
            $index++;
        }
        try {
            return json_decode(substr($text, $start, $index - $start), true, 2, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return null;
        }
    }

    /** Structural walk over one ALREADY-VALID raw document recording the
     * member paths whose raw value is a JSON object (list-position
     * members must refuse them: `{}` and `{"0":…}` decode to list
     * values) and whose raw number lexeme is `-0` (json_decode maps it
     * to int 0; production's serde u64 visitor refuses the signed
     * spelling). Paths join member names and array indices with dots. */
    private static function auditValue(string $text, int &$index, string $path, array &$objectPaths, array &$zeroPaths): void {
        self::skipJsonWhitespace($text, $index);
        if ($index >= strlen($text)) {
            return;
        }
        $c = $text[$index];
        if ($c === '{') {
            $objectPaths[$path] = true;
            $index++;
            self::skipJsonWhitespace($text, $index);
            if (substr($text, $index, 1) === '}') {
                $index++;
                return;
            }
            for (;;) {
                self::skipJsonWhitespace($text, $index);
                if (substr($text, $index, 1) !== '"') {
                    return;
                }
                $key = self::auditString($text, $index);
                if ($key === null) {
                    return;
                }
                self::skipJsonWhitespace($text, $index);
                if (substr($text, $index, 1) !== ':') {
                    return;
                }
                $index++;
                self::auditValue($text, $index, $path === '' ? $key : $path . '.' . $key, $objectPaths, $zeroPaths);
                self::skipJsonWhitespace($text, $index);
                $next = substr($text, $index, 1);
                if ($next === ',') {
                    $index++;
                    continue;
                }
                if ($next === '}') {
                    $index++;
                }
                return;
            }
        }
        if ($c === '[') {
            $index++;
            self::skipJsonWhitespace($text, $index);
            if (substr($text, $index, 1) === ']') {
                $index++;
                return;
            }
            for ($element = 0; ; $element++) {
                self::auditValue($text, $index, $path === '' ? (string) $element : $path . '.' . $element, $objectPaths, $zeroPaths);
                self::skipJsonWhitespace($text, $index);
                $next = substr($text, $index, 1);
                if ($next === ',') {
                    $index++;
                    continue;
                }
                if ($next === ']') {
                    $index++;
                }
                return;
            }
        }
        if ($c === '"') {
            self::auditString($text, $index);
            return;
        }
        foreach (['true', 'false', 'null'] as $literal) {
            if (substr($text, $index, strlen($literal)) === $literal) {
                $index += strlen($literal);
                return;
            }
        }
        // A number lexeme: `-0` is the negative-zero integer spelling
        // json_decode collapses to an accepted int 0.
        $start = $index;
        $length = strlen($text);
        while ($index < $length && strpos('0123456789+-.eE', $text[$index]) !== false) {
            $index++;
        }
        if (substr($text, $start, $index - $start) === '-0') {
            $zeroPaths[$path] = true;
        }
    }

    /** Read exactly one bounded JSON object document. */
    public static function readSingleObject(string $text, int $maxBytes): array {
        if (strlen($text) > $maxBytes) {
            return ['ok' => false, 'reason' => 'oversize'];
        }
        $parsed = self::strictParse($text);
        if (!$parsed['ok']) {
            return $parsed;
        }
        if (!is_array($parsed['value']) || array_is_list($parsed['value'])) {
            return ['ok' => false, 'reason' => 'shape'];
        }
        return $parsed;
    }

    /** Canonical compact JSON: sorted keys, no whitespace, no trailing LF. */
    public static function canonicalJson(mixed $value): string {
        if (is_array($value)) {
            if (array_is_list($value)) {
                $items = array_map([self::class, 'canonicalJson'], $value);
                return '[' . implode(',', $items) . ']';
            }
            $keys = array_keys($value);
            sort($keys, SORT_STRING);
            $members = [];
            foreach ($keys as $key) {
                $members[] = json_encode((string) $key, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE) . ':' . self::canonicalJson($value[$key]);
            }
            return '{' . implode(',', $members) . '}';
        }
        return json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    }
}
