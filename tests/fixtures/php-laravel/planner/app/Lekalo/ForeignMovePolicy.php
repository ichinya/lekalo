<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\MoveTaskPolicy;

/**
 * The maintained authorization adapter behind the generated
 * MoveTaskPolicy port (issue #50): the declared
 * `planner.deny_foreign_planning` gate of the planning family.
 */
final class ForeignMovePolicy implements MoveTaskPolicy
{
    use DeniesForeignPlanning;

    public function authorize(\Lekalo\Generated\Types\Planner\MoveTaskInput $input, ActorContext $actor): void
    {
        $this->denyForeign($actor);
    }
}
