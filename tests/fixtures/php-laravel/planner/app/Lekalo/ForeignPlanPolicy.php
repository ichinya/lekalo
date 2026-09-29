<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\PlanTaskPolicy;

/**
 * The maintained authorization adapter behind the generated
 * PlanTaskPolicy port (issue #50): the declared
 * `planner.deny_foreign_planning` gate of the planning family.
 */
final class ForeignPlanPolicy implements PlanTaskPolicy
{
    use DeniesForeignPlanning;

    public function authorize(\Lekalo\Generated\Types\Planner\PlanTaskInput $input, ActorContext $actor): void
    {
        $this->denyForeign($actor);
    }
}
