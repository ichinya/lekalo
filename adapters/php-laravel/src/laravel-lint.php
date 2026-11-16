<?php
/**
 * The Laravel lint facts, provider-owned since the stage-4 separation
 * (lekalo-php#1, issue #241 M1): the persisted-model base class of the
 * framework, served through the `lint.framework-semantics` capability.
 * The language walk in `ai-lint.php` resolves class base names
 * generically and asks the provider whether one is its model base —
 * with no provider registered the eloquent classification is uniformly
 * false and observer-effect records are uniformly absent (the honest
 * language-only outcome), never a silent Laravel assumption.
 */

/** The Laravel framework lint facts consumed by the language walk. */
function lekalo_laravel_lint_facts(): array
{
    return [
        'model_base' => 'Illuminate\\Database\\Eloquent\\Model',
    ];
}
