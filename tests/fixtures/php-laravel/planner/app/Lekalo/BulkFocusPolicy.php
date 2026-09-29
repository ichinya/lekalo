<?php

declare(strict_types=1);

namespace App\Lekalo;

use Lekalo\Generated\Operations\Planner\Errors\FocusDeniedError;
use Lekalo\Generated\Operations\Planner\FocusTaskPolicy;
use Lekalo\Generated\Operations\ActorContext;
use Lekalo\Generated\Types\Planner\FocusTaskInput;

/**
 * The maintained authorization adapter behind the generated
 * FocusTaskPolicy port (issue #60 fixture integration): the
 * `planner.deny_bulk_focus` policy denies bulk actors with the typed
 * domain error the boundary maps to the declared auth status.
 */
final class BulkFocusPolicy implements FocusTaskPolicy
{
    public function authorize(FocusTaskInput $input, ActorContext $actor): void
    {
        if ($actor->scope('mode') === 'bulk') {
            throw new FocusDeniedError();
        }
    }
}
