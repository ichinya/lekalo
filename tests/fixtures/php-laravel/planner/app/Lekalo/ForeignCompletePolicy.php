<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\CompletePlanningPolicy;
use Lekalo\Generated\Types\Planner\CompletePlanningInput;

/**
 * The maintained authorization adapter behind the generated
 * CompletePlanningPolicy port (issue #53): the shared
 * `planner.deny_foreign_planning` gate of the planning family.
 */
final class ForeignCompletePolicy implements CompletePlanningPolicy
{
    use DeniesForeignPlanning;

    public function authorize(CompletePlanningInput $input, ActorContext $actor): void
    {
        $this->denyForeign($actor);
    }
}
