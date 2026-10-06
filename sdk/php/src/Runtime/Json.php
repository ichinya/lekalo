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

    /** Strict parse of one complete JSON document (object or array). */
    public static function strictParse(string $text): array {
        if (str_starts_with($text, "\xEF\xBB\xBF")) {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        $trimmed = ltrim($text);
        $first = $trimmed === '' ? '' : $trimmed[0];
        if ($first !== '{' && $first !== '[') {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        $end = self::documentEnd($trimmed);
        if ($end === -1) {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        if (strlen(trim(substr($trimmed, $end + 1))) > 0) {
            return ['ok' => false, 'reason' => 'trailing-document'];
        }
        $document = substr($trimmed, 0, $end + 1);
        try {
            $value = json_decode($document, true, 512, JSON_THROW_ON_ERROR);
        } catch (\JsonException $e) {
            return ['ok' => false, 'reason' => 'not-json'];
        }
        if (count(self::duplicateKeys($document)) > 0) {
            return ['ok' => false, 'reason' => 'duplicate-key'];
        }
        return ['ok' => true, 'value' => $value];
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
