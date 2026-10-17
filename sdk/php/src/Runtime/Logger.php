<?php
declare(strict_types=1);
// The bounded, redacted stderr logger (issue #106): logs never touch
// stdout and never carry raw request, source, or environment content.

namespace Lekalo\AdapterSdk\Runtime;

final class Logger {
    private const MAX_MESSAGE_CHARS = 256;
    private const MAX_STDERR_BYTES = 65536;

    private static int $emittedBytes = 0;

    private static function emit(string $level, string $tag, string $message): void {
        if ($message === '' || str_contains($message, "\n") || str_contains($message, "\r")) {
            return;
        }
        // UTF-8 code-point bound without ext-mbstring (issue #106 review
        // r6 #2): the shared Decode helpers count characters byte-exactly.
        $text = Decode::utf8Length($message) > self::MAX_MESSAGE_CHARS ? Decode::utf8Prefix($message, self::MAX_MESSAGE_CHARS) : $message;
        // The fixed-width UTC stamp keeps logs deterministic in shape and
        // free of host timezone data.
        $line = sprintf("%s %s[%s]: %s\n", gmdate('Ymd\THis\Z'), $level, $tag, $text);
        $bytes = strlen($line);
        if (self::$emittedBytes + $bytes > self::MAX_STDERR_BYTES) {
            return;
        }
        self::$emittedBytes += $bytes;
        fwrite(STDERR, $line);
    }

    public static function debug(string $tag, string $message): void {
        self::emit('debug', $tag, $message);
    }

    public static function error(string $tag, string $message): void {
        self::emit('error', $tag, $message);
    }

    public static function info(string $tag, string $message): void {
        self::emit('info', $tag, $message);
    }

    public static function warn(string $tag, string $message): void {
        self::emit('warn', $tag, $message);
    }

    public static function isLoggableToken(mixed $value): bool {
        return is_string($value) && $value !== '' && Decode::utf8Length($value) <= 128
            && !str_contains($value, "\n") && !str_contains($value, "\r");
    }
}
