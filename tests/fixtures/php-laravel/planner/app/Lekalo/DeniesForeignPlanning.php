<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\Errors\PlanningDeniedError;

/**
 * The shared deny of the declared `planner.deny_foreign_planning`
 * gate (issue #50): a `foreign` actor mode denies; every other actor
 * passes the gate and meets the enforced workspace/user scope inside
 * the store and query ports. Absence of the dimension is never allow.
 */
trait DeniesForeignPlanning
{
    private function denyForeign(ActorContext $actor): void
    {
        if ((string) ($actor->scope('mode') ?? '') === 'foreign') {
            throw new PlanningDeniedError();
        }
    }
}
