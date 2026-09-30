<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\PausePlanningPolicy;

/**
 * The maintained authorization adapter behind the generated
 * PausePlanningPolicy port (issue #50): the declared
 * `planner.deny_foreign_planning` gate of the planning family.
 */
final class ForeignPausePolicy implements PausePlanningPolicy
{
    use DeniesForeignPlanning;

    public function authorize(\Lekalo\Generated\Types\Planner\PausePlanningInput $input, ActorContext $actor): void
    {
        $this->denyForeign($actor);
    }
}
