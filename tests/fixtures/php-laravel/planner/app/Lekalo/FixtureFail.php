<?php

declare(strict_types=1);

namespace App\Lekalo;

/**
 * The deterministic failure-injection seam of the fixture runtime lane
 * (issue #60): the runtime driver arms the next request with a declared
 * infrastructure failure, and the maintained read adapter honors it.
 * A test seam of the fixture itself — never part of the generated
 * surface.
 */
final class FixtureFail
{
    private static ?string $next = null;

    /** Arm the next consumed failure. */
    public static function arm(string $kind): void
    {
        self::$next = $kind;
    }

    /** Consume one armed failure kind, if any. */
    public static function consume(): ?string
    {
        $kind = self::$next;
        self::$next = null;

        return $kind;
    }
}
