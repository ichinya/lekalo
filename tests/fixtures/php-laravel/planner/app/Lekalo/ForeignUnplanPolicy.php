<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Operations\Planner\UnplanTaskPolicy;

/**
 * The maintained authorization adapter behind the generated
 * UnplanTaskPolicy port (issue #50): the declared
 * `planner.deny_foreign_planning` gate of the planning family.
 */
final class ForeignUnplanPolicy implements UnplanTaskPolicy
{
    use DeniesForeignPlanning;

    public function authorize(\Lekalo\Generated\Types\Planner\UnplanTaskInput $input, ActorContext $actor): void
    {
        $this->denyForeign($actor);
    }
}
