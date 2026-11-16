<?php
/**
 * The versioned provider API between the language adapter and a
 * framework package (lekalo-php#1, issue #241 stage 4).
 *
 * A framework package registers exactly one provider descriptor: its
 * identity, the provider API version it speaks, a closed capability
 * matrix, and one hook callable per declared capability family. The
 * language adapter never names a framework: it resolves capability
 * families through this registry, so a missing provider is a typed
 * refusal with a stable code (`provider-missing`), a version mismatch
 * is `provider-version-conflict`, and a duplicate registration is
 * `provider-duplicate` — never a silent capability downgrade.
 *
 * Stage M1 (issue #241): the Laravel provider is compiled into the
 * unified artifact (build.php PROVIDER_MODULES), so the shipped
 * behavior is byte-compatible with the pre-seam adapter. The registry
 * is wire-invisible in M1 (no response envelope changes); the packages
 * become independently shippable in M2, and framework-pack discovery
 * (lekalo#197) replaces the compiled-in registration afterwards.
 */

const PROVIDER_API_SCHEMA_VERSION = 'lekalo/provider/v0.1.0';
const PROVIDER_API_IDENTITY = 'dev.lekalo.provider@0.1.0';

/** The closed capability families the language adapter resolves. */
const PROVIDER_FAMILIES = [
    'migrations.emit',
    'routes.emit',
    'scenario.runners',
    'scenario.emit',
    'native.gates',
    'lint.framework-semantics',
];

/** The closed per-family support states (absence means unsupported). */
const PROVIDER_CAPABILITY_STATES = ['full', 'partial'];

/** The bounded provider count of one process (reviewed registry size). */
const PROVIDER_REGISTRY_LIMIT = 8;

/** The registered providers, id-sorted; keyed by provider id. */
function &lekalo_provider_registry(): array
{
    static $providers = [];
    return $providers;
}

/**
 * Register one provider descriptor. The shape is closed and validated
 * before any state changes: unknown members, unknown families, unknown
 * states, a hook without its capability, a capability without its
 * hook, an api version other than the one this adapter speaks, or a
 * duplicate id each refuse with their own stable code.
 */
function lekalo_provider_register(array $provider): void
{
    $required = ['id', 'version', 'api', 'capabilities', 'hooks'];
    foreach (array_keys($provider) as $member) {
        if (!in_array($member, $required, true)) {
            throw new RequestRefusal('provider-descriptor');
        }
    }
    foreach ($required as $member) {
        if (!array_key_exists($member, $provider)) {
            throw new RequestRefusal('provider-descriptor');
        }
    }
    if (!is_string($provider['id'])
        || !preg_match('/^[a-z][a-z0-9-]{2,63}$/D', $provider['id'])) {
        throw new RequestRefusal('provider-descriptor');
    }
    if (!is_string($provider['version'])
        || !preg_match('/^\d+\.\d+\.\d+$/D', $provider['version'])) {
        throw new RequestRefusal('provider-descriptor');
    }
    // Version negotiation is deterministic: the adapter speaks exactly
    // one provider API version and refuses any other on registration,
    // before a half-negotiated provider can serve a single family.
    if ($provider['api'] !== PROVIDER_API_SCHEMA_VERSION) {
        throw new RequestRefusal('provider-version-conflict');
    }
    if (!is_array($provider['capabilities']) || !is_array($provider['hooks'])) {
        throw new RequestRefusal('provider-descriptor');
    }
    foreach ($provider['capabilities'] as $family => $state) {
        if (!in_array((string) $family, PROVIDER_FAMILIES, true)) {
            throw new RequestRefusal('provider-descriptor');
        }
        if (!in_array($state, PROVIDER_CAPABILITY_STATES, true)) {
            throw new RequestRefusal('provider-descriptor');
        }
    }
    foreach (array_keys($provider['capabilities']) as $family) {
        if (!is_callable($provider['hooks'][$family] ?? null)) {
            throw new RequestRefusal('provider-descriptor');
        }
    }
    foreach (array_keys($provider['hooks']) as $family) {
        if (!array_key_exists($family, $provider['capabilities'])) {
            throw new RequestRefusal('provider-descriptor');
        }
    }
    $registry = &lekalo_provider_registry();
    if (isset($registry[$provider['id']])) {
        throw new RequestRefusal('provider-duplicate');
    }
    if (count($registry) >= PROVIDER_REGISTRY_LIMIT) {
        throw new RequestRefusal('provider-registry-limit');
    }
    $registry[$provider['id']] = $provider;
    ksort($registry, SORT_STRING);
}

/** The registered provider summaries, id-sorted, for describe/debugging. */
function lekalo_providers(): array
{
    $out = [];
    foreach (lekalo_provider_registry() as $id => $provider) {
        $out[] = [
            'id' => $id,
            'version' => $provider['version'],
            'api' => $provider['api'],
            'capabilities' => $provider['capabilities'],
        ];
    }
    return $out;
}

/**
 * The first id-sorted provider declaring the family, or null when no
 * registered provider does. The non-throwing probe: language code that
 * merely conditions framework semantics on a provider's presence (the
 * ai-lint Eloquent islands) resolves here and degrades honestly when
 * the registry is empty.
 */
function lekalo_provider_capability(string $family): ?array
{
    if (!in_array($family, PROVIDER_FAMILIES, true)) {
        return null;
    }
    foreach (lekalo_provider_registry() as $id => $provider) {
        if (array_key_exists($family, $provider['capabilities'])) {
            return [
                'provider' => $id,
                'version' => $provider['version'],
                'state' => $provider['capabilities'][$family],
            ];
        }
    }
    return null;
}

/**
 * The capability of one family or a typed refusal: a family nobody
 * declares is `provider-missing`, never a silent pass (lekalo-php#1).
 */
function lekalo_provider_require(string $family): array
{
    $capability = lekalo_provider_capability($family);
    if ($capability === null) {
        throw new RequestRefusal('provider-missing');
    }
    return $capability;
}

/** Invoke one family's hook through the negotiated provider. */
function lekalo_provider_call(string $family, array $arguments = []): mixed
{
    $capability = lekalo_provider_require($family);
    $provider = lekalo_provider_registry()[$capability['provider']];
    return $provider['hooks'][$family](...$arguments);
}

/**
 * The merged scenario-runner declarations of every provider that
 * declares `scenario.runners`, id-sorted, with duplicate runner ids
 * refused: two providers may not silently shadow one runner. The
 * merged table is computed once per process.
 */
function lekalo_provider_scenario_runner_table(): ?array
{
    static $table = false;
    if ($table !== false) {
        return $table;
    }
    $table = null;
    $runners = [];
    foreach (lekalo_provider_registry() as $provider) {
        if (!array_key_exists('scenario.runners', $provider['capabilities'])) {
            continue;
        }
        $declaration = $provider['hooks']['scenario.runners']();
        if (!is_array($declaration)
            || !isset($declaration['runners'])
            || !is_array($declaration['runners'])
            || (isset($declaration['default']) && !is_string($declaration['default']))) {
            throw new RequestRefusal('provider-descriptor');
        }
        if ($table === null) {
            $table = ['runners' => [], 'default' => null];
        }
        foreach ($declaration['runners'] as $runnerId => $entry) {
            if (isset($runners[$runnerId])) {
                throw new RequestRefusal('provider-runner-conflict');
            }
            $runners[$runnerId] = $entry;
        }
        if ($table['default'] === null && is_string($declaration['default'] ?? null)) {
            $table['default'] = $declaration['default'];
        }
    }
    if ($table !== null) {
        ksort($runners, SORT_STRING);
        $table['runners'] = $runners;
    }
    return $table;
}

/** The provider-supplied runner registry, or null with no provider. */
function lekalo_provider_scenario_runners(): ?array
{
    $table = lekalo_provider_scenario_runner_table();
    return $table === null ? null : $table['runners'];
}

/** The provider-supplied default runner, or null with no provider. */
function lekalo_provider_default_scenario_runner(): ?string
{
    $table = lekalo_provider_scenario_runner_table();
    return $table === null ? null : $table['default'];
}

/**
 * The lint framework facts of the registered lint provider (or null):
 * the resolved model base-class FQN the provider classifies as its
 * persisted-model base. Cached per process; one provider lookup, never
 * per class declaration in the lint walk.
 */
function lekalo_provider_lint_facts(): ?array
{
    static $facts = false;
    if ($facts !== false) {
        return $facts;
    }
    $facts = null;
    if (lekalo_provider_capability('lint.framework-semantics') !== null) {
        $declared = lekalo_provider_call('lint.framework-semantics');
        if (!is_array($declared)) {
            throw new RequestRefusal('provider-descriptor');
        }
        $facts = $declared;
    }
    return $facts;
}

/** Whether the resolved base-class FQN is the provider's model base. */
function lekalo_provider_lint_classifies_model(string $resolvedBase): bool
{
    $facts = lekalo_provider_lint_facts();
    return $facts !== null
        && is_string($facts['model_base'] ?? null)
        && $resolvedBase === $facts['model_base'];
}

/** Whether observer-effect lint semantics have a provider behind them. */
function lekalo_provider_lint_observer_semantics(): bool
{
    return lekalo_provider_capability('lint.framework-semantics') !== null;
}
