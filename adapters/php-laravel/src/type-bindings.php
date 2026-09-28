<?php
/**
 * The checked-type binding join of the PHP generator (issue #58, step
 * 3): a checked custody generation emits NOTHING and instead joins the
 * declared semantic ids against the observed class-shape evidence the
 * scanner published. The join is read-only and strict:
 *
 *   - a missing evidence document, a missing claim, an ambiguous
 *     claim, a stale source digest, and any shape divergence are
 *     typed findings — never conformant, never a rewrite;
 *   - an absent observer can never pass checked acceptance;
 *   - observed evidence alone grants nothing: the declared mapping is
 *     the authority, the evidence only confirms it.
 */

if (!function_exists('php_validate_types_input')) {
    require_once __DIR__ . '/type-policy.php';
}

const PHP_TYPES_BINDING_MISSING = 'php-types.binding-missing';
const PHP_TYPES_BINDING_AMBIGUOUS = 'php-types.binding-ambiguous';
const PHP_TYPES_BINDING_MISMATCH = 'php-types.binding-mismatch';

/**
 * Join one mapped inventory against the parsed evidence document.
 * `evidence` is the parsed `.lekalo/import/observed/types-evidence.json`
 * or null when absent (absence is a finding for every declared id, per
 * the required-evidence rule). Returns wire-shaped findings.
 */
function php_check_type_bindings(array $mapped, ?array $evidence, ?callable $fileDigest): array
{
    $findings = [];
    $records = [];
    // Evidence paths are project-relative: the declared artifact path
    // resolves under the custody root the mapping names.
    $root = $mapped['policy']['custody'] === 'scaffold-once'
        ? (string) $mapped['policy']['scaffoldRoot']
        : PHP_TYPES_GENERATED_ROOT;
    if (is_array($evidence) && ($evidence['schemaVersion'] ?? null) === PHP_TYPES_EVIDENCE_SCHEMA_VERSION
        && ($evidence['identity'] ?? null) === PHP_TYPES_EVIDENCE_IDENTITY
        && is_array($evidence['classes'] ?? null)) {
        foreach ($evidence['classes'] as $record) {
            if (!is_array($record) || !is_string($record['semanticId'] ?? null)) {
                continue;
            }
            $records[$record['semanticId']][] = $record;
        }
    } else {
        // No evidence at all: every declared id is a missing binding.
        // Checked acceptance without an observer is impossible.
        foreach ($mapped['types'] as $entry) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_MISSING,
                'semanticId' => $entry['semanticId'],
                'detail' => 'no-evidence-document',
            ];
        }
        return php_types_sort_binding_findings($findings);
    }

    foreach ($mapped['types'] as $entry) {
        $id = $entry['semanticId'];
        $claiming = $records[$id] ?? [];
        if ($claiming === []) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_MISSING,
                'semanticId' => $id,
                'detail' => 'no-observed-class',
            ];
            continue;
        }
        if (count($claiming) > 1) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_AMBIGUOUS,
                'semanticId' => $id,
                'detail' => 'claimed-by-' . count($claiming) . '-classes',
            ];
            continue;
        }
        $record = $claiming[0];
        $problems = php_types_check_binding_record($entry, $record, $root, $fileDigest);
        foreach ($problems as $problem) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_MISMATCH,
                'semanticId' => $id,
                'detail' => $problem,
            ];
        }
    }
    return php_types_sort_binding_findings($findings);
}

/**
 * One evidence record against one declared entry: identity, path,
 * freshness, and observed shape.
 *
 * @return list<string> the divergence details (empty = conforms)
 */
function php_types_check_binding_record(array $entry, array $record, string $root, ?callable $fileDigest): array
{
    $problems = [];
    $kind = is_string($record['kind'] ?? null) ? $record['kind'] : null;
    $fqn = is_string($record['fqn'] ?? null) ? $record['fqn'] : null;
    $path = is_string($record['path'] ?? null) ? $record['path'] : null;
    $expectedPath = $root . '/' . $entry['path'];
    if ($kind !== $entry['kind']) {
        $problems[] = 'kind-diverges';
    }
    if ($fqn !== $entry['fqn']) {
        $problems[] = 'fqn-diverges';
    }
    if ($path !== null && $path !== $expectedPath) {
        $problems[] = 'path-diverges';
    }
    // Freshness: the exact observed source bytes must still be on disk.
    // Tampered or regenerated class bytes invalidate the evidence.
    $digest = is_string($record['sourceDigest'] ?? null) ? $record['sourceDigest'] : null;
    if ($digest !== null && $fileDigest !== null) {
        $actual = $fileDigest($path ?? $entry['path']);
        if ($actual === null) {
            $problems[] = 'observed-file-missing';
        } elseif ($actual !== $digest) {
            $problems[] = 'stale-source-digest';
        }
    }
    // Observed shape: enum cases and property spellings must match the
    // declared mapping exactly; extra or missing members diverge.
    if ($entry['kind'] === 'enum' && is_array($record['enumCases'] ?? null)) {
        $declared = [];
        foreach ($entry['values'] as $value) {
            $declared[$value['case']] = $value['value'];
        }
        $observed = [];
        foreach ($record['enumCases'] as $case) {
            if (!is_array($case) || !is_string($case['case'] ?? null) || !is_string($case['value'] ?? null)) {
                $problems[] = 'evidence-shape';
                break;
            }
            $observed[$case['case']] = $case['value'];
        }
        if ($declared !== $observed) {
            $problems[] = 'enum-cases-diverge';
        }
    }
    if (in_array($entry['kind'], ['value-object', 'entity', 'command', 'event'], true)
        && is_array($record['properties'] ?? null)) {
        $declared = [];
        foreach ($entry['fields'] as $field) {
            $declared[$field['property']] = true;
        }
        $observed = [];
        foreach ($record['properties'] as $property) {
            if (!is_array($property) || !is_string($property['name'] ?? null)) {
                $problems[] = 'evidence-shape';
                break;
            }
            $observed[$property['name']] = true;
        }
        if ($declared !== $observed) {
            $problems[] = 'properties-diverge';
        }
    }
    return $problems;
}

/** Binding findings sort deterministically by semantic id, then code. */
function php_types_sort_binding_findings(array $findings): array
{
    usort($findings, static fn (array $left, array $right): int => strcmp(
        $left['semanticId'] . '|' . $left['code'],
        $right['semanticId'] . '|' . $right['code'],
    ));
    return $findings;
}

/**
 * Validate the parsed evidence document shape (closed): returns the
 * document or null. A present-but-invalid document refuses upstream
 * rather than joining over untyped bytes.
 */
function php_validate_types_evidence(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_TYPES_EVIDENCE_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_TYPES_EVIDENCE_IDENTITY
        || !is_array($document['classes'] ?? null)) {
        return null;
    }
    foreach ($document['classes'] as $record) {
        if (!is_array($record)
            || !is_string($record['semanticId'] ?? null)
            || !is_string($record['fqn'] ?? null)
            || !is_string($record['path'] ?? null)
            || !is_string($record['sourceDigest'] ?? null)
            || !is_string($record['kind'] ?? null)) {
            return null;
        }
    }
    return $document;
}
