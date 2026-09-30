<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\ReorderPlannedPolicy;

/**
 * The maintained authorization adapter behind the generated
 * ReorderPlannedPolicy port (issue #50): the declared
 * `planner.deny_foreign_planning` gate of the planning family.
 */
final class ForeignReorderPolicy implements ReorderPlannedPolicy
{
    use DeniesForeignPlanning;

    public function authorize(\Lekalo\Generated\Types\Planner\ReorderPlannedInput $input, ActorContext $actor): void
    {
        $this->denyForeign($actor);
    }
}
