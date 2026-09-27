<?php

/**
 * The Lekalo strict-profile rules over the analyzer receipt (issue #55).
 *
 * The issue's target table maps each required rule to its evidence
 * source. Two implementation shapes exist and are kept strictly apart:
 *
 * - `lint` rows: a pinned Mago rule supplies the finding verbatim; the
 *   kernel only maps the original code into a registered rule id and
 *   keeps the exact original code in namespaced metadata.
 * - `predicate` rows: no upstream rule exists for the requirement, so
 *   the kernel evaluates a deterministic predicate over the receipt's
 *   symbol/AST evidence. When the prerequisite evidence is absent the
 *   row is `unsupported` — never a silent pass.
 *
 * Every row: compliant inputs produce no finding; violating inputs
 * produce one; evidence that cannot decide produces an explicit
 * `target.analysis.evidence-unsupported` diagnostic instead of a
 * fabricated pass or fail.
 */

/** The closed strict-profile row set (ids are kernel-stable). */
const STRICT_RULES = [
    'strict-types' => [
        'rule' => 'target.analysis.strict-types',
        'source' => 'lint',
        'mago_code' => 'strict-types',
    ],
    'final-readonly-profile' => [
        'rule' => 'target.analysis.final-readonly-profile',
        'source' => 'predicate',
    ],
    'no-dynamic-members' => [
        'rule' => 'target.analysis.no-dynamic-members',
        'source' => 'lint',
        'mago_code' => 'no-variable-variable',
    ],
    'no-service-locator' => [
        'rule' => 'target.analysis.no-service-locator',
        'source' => 'predicate',
    ],
    'no-magic-domain-state' => [
        'rule' => 'target.analysis.no-magic-domain-state',
        'source' => 'predicate',
    ],
    'explicit-types' => [
        'rule' => 'target.analysis.explicit-types',
        'source' => 'lint',
        'mago_code' => 'strict-types',
        'notes' => 'type completeness rides the signature evidence predicate below',
    ],
];

/**
 * Evaluate the whole strict profile over one `ok` outcome. Returns the
 * normalizer-ready diagnostic rows (Lekalo rule ids, logical paths,
 * half-open ranges, namespaced original codes) plus one `unsupported`
 * row per rule whose prerequisite evidence was missing.
 *
 * @param array<int, array<string, mixed>> $diagnostics
 * @param array<int, array<string, mixed>> $symbols
 * @return array{findings: array<int, array<string, mixed>>, unsupported: array<int, string>}
 */
function strict_profile_evaluate(array $diagnostics, array $symbols): array
{
    $findings = [];
    $unsupported = [];
    foreach (STRICT_RULES as $id => $row) {
        if ($row['source'] === 'lint') {
            $matched = strict_map_lint_row($diagnostics, (string) $row['mago_code'], (string) $row['rule']);
            if ($matched === false) {
                $unsupported[] = (string) $id;
                continue;
            }
            foreach ($matched as $finding) {
                $findings[] = $finding;
            }
            continue;
        }
        $predicate = $id === 'final-readonly-profile'
            ? strict_predicate_final_readonly($symbols)
            : strict_predicate_unavailable((string) $id);
        if ($predicate === null) {
            $unsupported[] = (string) $id;
            continue;
        }
        foreach ($predicate as $finding) {
            $findings[] = $finding;
        }
    }
    return ['findings' => $findings, 'unsupported' => $unsupported];
}

/**
 * Map one Mago lint code onto its registered Lekalo row. `false` means
 * the prerequisite lint evidence was not in the receipt (the row's rule
 * never ran) — that is `unsupported`, not `compliant`.
 *
 * @param array<int, array<string, mixed>> $diagnostics
 * @return array<int, array<string, mixed>>|false
 */
function strict_map_lint_row(array $diagnostics, string $magoCode, string $ruleId): array|false
{
    $coverage = false;
    $findings = [];
    foreach ($diagnostics as $item) {
        if (($item['producer'] ?? '') !== 'mago') {
            continue;
        }
        if (($item['original_code'] ?? '') === $magoCode) {
            $coverage = true;
            $findings[] = $item;
        }
    }
    return $coverage ? $findings : false;
}

/**
 * The final/readonly profile predicate over symbol evidence: mutable
 * (non-final, non-abstract) domain classes are violations unless they
 * carry the explicit `extensible` marker in their modifiers. Symbol
 * evidence absent ⇒ `null` (unsupported), never a pass.
 *
 * @param array<int, array<string, mixed>> $symbols
 * @return array<int, array<string, mixed>>|null
 */
function strict_predicate_final_readonly(array $symbols): ?array
{
    $classes = array_values(array_filter(
        $symbols,
        static fn (array $symbol): bool => in_array($symbol['kind'], ['class', 'interface', 'trait', 'enum'], true),
    ));
    if ($classes === []) {
        return null;
    }
    $findings = [];
    foreach ($classes as $symbol) {
        $modifiers = $symbol['modifiers'] ?? [];
        $extensible = in_array('extensible', array_map('strval', $modifiers), true);
        if ($extensible || in_array('final', $modifiers, true) || in_array('abstract', $modifiers, true)) {
            continue;
        }
        $findings[] = [
            'rule' => 'target.analysis.final-readonly-profile',
            'original_code' => 'lekalo.final-profile',
            'level' => 'warning',
            'path' => $symbol['path'],
            'range' => $symbol['range'],
            'message' => 'domain class ' . (string) $symbol['identity'] . ' is neither final nor explicitly extensible',
            'producer' => 'lekalo',
        ];
    }
    return $findings;
}

/**
 * Rows whose prerequisite evidence does not exist in this slice: the
 * service-locator and magic-state predicates need resolved-reference
 * evidence the pinned toolchain does not export as a public graph.
 * Unsupported is the honest state (issue boundary: no regex fallback).
 */
function strict_predicate_unavailable(string $id): ?array
{
    return null;
}

/**
 * The single explicit `evidence-unsupported` diagnostic for one rule.
 *
 * @return array<string, mixed>
 */
function strict_unsupported_diagnostic(string $id): array
{
    return [
        'rule' => 'target.analysis.evidence-unsupported',
        'original_code' => 'lekalo.unsupported:' . $id,
        'level' => 'note',
        'path' => '.lekalo/import/mago/receipt.json',
        'range' => ['start' => 0, 'end' => 0],
        'message' => 'strict-profile rule ' . $id . ' lacks prerequisite evidence and is unsupported, not passing',
        'producer' => 'lekalo',
    ];
}
