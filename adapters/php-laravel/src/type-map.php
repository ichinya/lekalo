<?php
/**
 * The closed type mapping of the PHP generator (issue #58, step 1):
 * one compiled-IR evidence document plus the validated policy map to a
 * closed inventory of PHP constructs — nominal scalar wrappers (opaque
 * ids stay distinct types), string-backed enums in declared order,
 * immutable value objects, entity/command/event DTOs, and query result
 * codecs — with the four presence cases resolved and every unsupported
 * projection reported as a bounded finding BEFORE any byte is planned.
 *
 * The mapping is pure: it reads plain data, it never reads the
 * filesystem, and it never invents a construct the IR does not declare
 * (no maps, no arbitrary unions, no unbound generics, no defaults).
 */

if (!function_exists('php_validate_types_input')) {
    require_once __DIR__ . '/type-policy.php';
}

/** The one wire finding code of unsupported type mapping (issue #58). */
const PHP_TYPES_UNSUPPORTED = 'php-types.mapping-unsupported';

/** The bounded reasons the mapping reports. */
const PHP_TYPES_UNSUPPORTED_REASONS = [
    'recursive-codec-unsupported',
    'nested-optional',
    'default-unsupported',
    'type-unsupported',
    'name-reserved',
    'name-collision',
    'path-collision',
    'module-reserved',
    'ref-unresolved',
];

/**
 * PHP reserved words that must never become a generated class stem
 * (case-insensitive), plus the primitive spellings a nominal wrapper
 * must never shadow. The list is closed; additions are a contract
 * change, not an emission-time guess.
 */
const PHP_TYPES_RESERVED_STEMS = [
    'abstract', 'and', 'array', 'as', 'break', 'callable', 'case', 'catch',
    'class', 'clone', 'const', 'continue', 'declare', 'default', 'do',
    'echo', 'else', 'elseif', 'enum', 'extends', 'false', 'final', 'finally',
    'fn', 'for', 'foreach', 'function', 'global', 'goto', 'if', 'implements',
    'include', 'instanceof', 'interface', 'isset', 'list', 'match', 'namespace',
    'new', 'null', 'or', 'print', 'private', 'protected', 'public', 'readonly',
    'require', 'return', 'static', 'switch', 'throw', 'trait', 'true', 'try',
    'unset', 'use', 'var', 'while', 'xor', 'yield', 'int', 'float', 'string',
    'bool', 'void', 'mixed', 'never', 'object', 'iterable', 'self', 'parent',
];

/**
 * Reserved module namespace segments. `Optional` is the emitted
 * wrapper sub-namespace, so a semantic module spelled `optional`
 * (case-insensitively) would collide with it.
 */
const PHP_TYPES_RESERVED_MODULES = ['optional'];

/** The fixed role suffixes: DTO role suffixes are fixed, never traversal-dependent. */
const PHP_TYPES_KIND_SUFFIXES = [
    'entity' => 'Dto',
    'command' => 'Input',
    'event' => 'Payload',
];

/**
 * Map the compiled IR to the closed type inventory. `input` is
 * `{ir, policy, irDigest, inputDigest}` with `ir` the parsed evidence
 * and `policy` the validated policy. Returns the mapped inventory with
 * one bounded finding per unsupported projection; the caller plans no
 * write while any finding exists.
 */
function php_map_types(array $input): array
{
    $policy = $input['policy'];
    $prefix = $policy['namespacePrefix'];
    try {
        $checked = php_check_ir_document($input['ir']);
    } catch (DefaultMetadataUnsupported $unsupported) {
        return [
            'state' => 'unsupported',
            'findings' => [php_types_finding(
                'default-unsupported',
                $unsupported->semanticId,
                $unsupported->pointer,
                'field `' . $unsupported->fieldName . '` carries unsupported metadata member `' . $unsupported->member . '`',
            )],
            'policy' => $policy,
            'digests' => ['ir' => $input['irDigest'], 'input' => $input['inputDigest']],
        ];
    }
    if ($checked === null) {
        return ['state' => 'refused', 'refusal' => 'types-ir-shape'];
    }
    $definitions = [];
    $order = [];
    foreach ($checked['definitions'] as $index => $definition) {
        $id = $definition['id'];
        if (isset($definitions[$id])) {
            // Duplicate semantic ids cannot carry a closed inventory.
            return ['state' => 'refused', 'refusal' => 'types-ir-shape'];
        }
        $definition['pointer'] = '/definitions/' . $index;
        $definitions[$id] = $definition;
        $order[] = $id;
    }

    $findings = [];
    $addFinding = static function (array $finding) use (&$findings): void {
        $findings[] = $finding;
    };

    // Cycles first: a recursive codec cannot be emitted, so every type
    // on the cycle is unsupported before any naming work happens.
    php_find_type_cycles($definitions, $addFinding);

    $types = [];
    $collections = [];
    $wrappers = [];
    foreach ($order as $id) {
        $definition = $definitions[$id];
        $kind = $definition['kind'];
        if (!in_array($kind, PHP_TYPES_MAPPED_KINDS, true)) {
            // Effects, endpoints, policies, scenarios and the structural
            // kinds are not domain types: no entry, no refusal.
            continue;
        }
        $module = php_types_module_of($id);
        if (in_array(strtolower($module), PHP_TYPES_RESERVED_MODULES, true)) {
            $addFinding(php_types_finding('module-reserved', $id, $definition['pointer'],
                'module namespace segment collides with the emitted wrapper namespace'));
            continue;
        }
        $stem = php_types_stem_of($id, $kind);
        if (in_array(strtolower($stem), PHP_TYPES_RESERVED_STEMS, true)) {
            $addFinding(php_types_finding('name-reserved', $id, $definition['pointer'],
                'class stem is a reserved PHP identifier'));
            continue;
        }
        $entry = [
            'semanticId' => $id,
            'kind' => $kind,
            'fqn' => php_types_fqn_of($prefix, $module, $stem),
            'path' => php_types_path_of($module, $stem),
            'description' => $definition['description'] ?? null,
        ];
        switch ($kind) {
            case 'scalar':
                $entry['base'] = $definition['base'];
                $entry['codec'] = $entry['fqn'];
                $entry['codecPath'] = $entry['path'];
                break;
            case 'enum':
                $cases = php_types_enum_cases($definition, $addFinding);
                if ($cases === null) {
                    continue 2;
                }
                $entry['values'] = $cases;
                $entry['codec'] = $entry['fqn'];
                $entry['codecPath'] = $entry['path'];
                break;
            case 'query':
                $returns = null;
                if (array_key_exists('returns', $definition)) {
                    $returns = php_check_type_ref($definition['returns']);
                }
                if ($returns === null) {
                    $addFinding(php_types_finding('type-unsupported', $id, $definition['pointer'],
                        'query return expression is not a closed mapped shape'));
                    continue 2;
                }
                if (php_types_resolve_expr($returns, $definitions, $id, $definition['pointer'], $addFinding) === null) {
                    continue 2;
                }
                if ($returns['list']) {
                    // A list-valued query return needs its collection class.
                    php_types_register_collection($returns, $definitions, $prefix, $collections);
                }
                // A query declares a direct output, never an envelope: the
                // result artifact is the returns codec passthrough, so the
                // entry's class IS its codec.
                $entry['fqn'] = php_types_fqn_of($prefix, $module, $stem . 'ResultCodec');
                $entry['codec'] = $entry['fqn'];
                $entry['path'] = php_types_path_of($module, $stem . 'ResultCodec');
                $entry['codecPath'] = $entry['path'];
                $entry['returns'] = $returns;
                break;
            default:
                $rawFields = match ($kind) {
                    'command' => $definition['input'],
                    'event' => $definition['payload'],
                    default => $definition['fields'],
                };
                $fields = php_types_map_fields($rawFields, $definitions, $prefix, $id,
                    $definition['pointer'], $collections, $wrappers, $addFinding);
                if ($fields === null) {
                    continue 2;
                }
                $entry['fields'] = $fields;
                $entry['codec'] = $entry['fqn'] . 'Codec';
                $entry['codecPath'] = php_types_path_of($module, php_types_entry_codec_class($entry['codec']));
                break;
        }
        $types[] = $entry;
    }

    $unsupported = static function () use (&$findings, $policy, $input): array {
        return [
            'state' => 'unsupported',
            'findings' => php_types_sort_findings($findings),
            'policy' => $policy,
            'digests' => ['ir' => $input['irDigest'], 'input' => $input['inputDigest']],
        ];
    };

    if ($findings !== []) {
        return $unsupported();
    }

    usort($types, static fn (array $left, array $right): int => strcmp($left['semanticId'], $right['semanticId']));
    usort($collections, static fn (array $left, array $right): int => strcmp($left['fqn'], $right['fqn']));
    usort($wrappers, static fn (array $left, array $right): int => strcmp($left['fqn'], $right['fqn']));

    // Naming custody before any emission: exact duplicate FQNs and
    // case-insensitive path collisions both refuse, because one of the
    // two files would silently shadow the other on a real filesystem.
    $artifacts = php_types_collect_artifacts($types, $collections, $wrappers, $policy, $addFinding);
    if ($findings !== []) {
        return $unsupported();
    }

    $index = [];
    foreach ($types as $entry) {
        $index[$entry['semanticId']] = [
            'fqn' => $entry['fqn'],
            'path' => $entry['path'],
            'codec' => $entry['codec'],
        ];
    }
    return [
        'state' => 'mapped',
        'findings' => [],
        'policy' => $policy,
        'types' => $types,
        'collections' => $collections,
        'wrappers' => $wrappers,
        'artifacts' => $artifacts,
        'index' => $index,
        'digests' => ['ir' => $input['irDigest'], 'input' => $input['inputDigest']],
    ];
}

/** Findings sort deterministically by semantic id, then reason. */
function php_types_sort_findings(array $findings): array
{
    usort($findings, static fn (array $left, array $right): int => strcmp(
        ($left['semanticId'] ?? '|') . '|' . $left['reason'],
        ($right['semanticId'] ?? '|') . '|' . $right['reason'],
    ));
    return $findings;
}

/** One bounded unsupported finding: code, reason, semantic id, pointer. */
function php_types_finding(string $reason, ?string $semanticId, ?string $pointer, string $detail): array
{
    $finding = ['code' => PHP_TYPES_UNSUPPORTED, 'reason' => $reason, 'detail' => $detail];
    if ($semanticId !== null) {
        $finding['semanticId'] = $semanticId;
    }
    if ($pointer !== null) {
        $finding['pointer'] = $pointer;
    }
    return $finding;
}

/** The module segment of one semantic id (the prefix before the first dot). */
function php_types_module_of(string $semanticId): string
{
    $cut = strpos($semanticId, '.');
    return $cut === false || $cut === 0 ? $semanticId : substr($semanticId, 0, $cut);
}

/** The last dot segment of one semantic id. */
function php_types_leaf_name_of(string $semanticId): string
{
    $cut = strrpos($semanticId, '.');
    return $cut === false ? $semanticId : substr($semanticId, $cut + 1);
}

/**
 * The Pascal-case class stem of one symbol: the final id segment splits
 * on `_`, every part capitalizes, and structured kinds append their
 * fixed role suffix. `planner.task_id` → `TaskId`;
 * `planner.task` (entity) → `TaskDto`; `planner.focus_task` (command)
 * → `FocusTaskInput`; `planner.task_focused` (event) →
 * `TaskFocusedPayload`; `planner.count_focused` (query) →
 * `CountFocusedResult`.
 */
function php_types_stem_of(string $semanticId, string $kind): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9_]/', '_', php_types_leaf_name_of($semanticId));
    $parts = explode('_', (string) $sanitized);
    $stem = implode('', array_map(
        static fn (string $part): string => ucfirst($part),
        $parts,
    ));
    return $stem . (PHP_TYPES_KIND_SUFFIXES[$kind] ?? '');
}

/** The deterministic camel-case property spelling of one wire name. */
function php_types_property_of(string $wireName): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9_]/', '_', $wireName);
    $parts = explode('_', (string) $sanitized);
    $first = array_shift($parts);
    return $first . implode('', array_map(
        static fn (string $part): string => ucfirst($part),
        $parts,
    ));
}

/** The full FQN of one generated class. */
function php_types_fqn_of(string $prefix, string $module, string $class): string
{
    return $prefix . '\\' . ucfirst($module) . '\\' . $class;
}

/**
 * The snake-case file spelling of one class (or namespace-qualified
 * class): the wire path grammar is lowercase, so the emitted paths
 * mirror the FQN deterministically (`TaskId` → `task_id.php`,
 * `Optional\OptionalDueDate` → `optional/optional_due_date.php`).
 */
function php_types_snake_of(string $spelling): string
{
    $withSlashes = str_replace('\\', '/', $spelling);
    $snake = preg_replace('/([a-z0-9])([A-Z])/', '$1_$2', $withSlashes);
    return strtolower((string) $snake);
}

/**
 * The artifact path of one generated class, relative to the generated
 * root: the lowercase module directory plus the snake-cased class
 * spelling. PHP-identifier case stays in the FQN, never in the path.
 */
function php_types_path_of(string $module, string $class): string
{
    return strtolower($module) . '/' . php_types_snake_of($class) . '.php';
}

/**
 * The enum cases of one enum definition: declared values preserved
 * verbatim, case names derived deterministically, declared order kept.
 * A case-name derivation collision (two values normalizing to one
 * identifier) is a bounded naming finding, never a silent merge.
 */
function php_types_enum_cases(array $definition, callable $addFinding): ?array
{
    $cases = [];
    $byName = [];
    foreach ($definition['values'] as $value) {
        $raw = (string) $value['value'];
        $name = ucfirst((string) preg_replace('/[^a-zA-Z0-9]/', '_', $raw));
        if ($name === '' || preg_match('/^[A-Za-z_][A-Za-z0-9_]*$/', $name) !== 1
            || in_array(strtolower($name), PHP_TYPES_RESERVED_STEMS, true)) {
            $addFinding(php_types_finding('name-reserved', $definition['id'], $definition['pointer'],
                'enum value does not normalize to a usable PHP identifier'));
            return null;
        }
        if (isset($byName[strtolower($name)])) {
            $addFinding(php_types_finding('name-collision', $definition['id'], $definition['pointer'],
                'enum case name collision on ' . $name));
            return null;
        }
        $byName[strtolower($name)] = true;
        $cases[] = ['case' => $name, 'value' => $raw];
    }
    return $cases;
}

/**
 * Map one definition's fields to closed field rows: verbatim wire
 * name, camel-case property, normalized type expression, and exactly
 * one of the four presence cases. List positions register collection
 * classes; optional positions register presence wrappers; unknown or
 * unresolved references are bounded findings. Returns null when the
 * definition is unsupported as a whole.
 */
function php_types_map_fields(
    array $rawFields,
    array $definitions,
    string $prefix,
    string $ownerId,
    string $pointer,
    array &$collections,
    array &$wrappers,
    callable $addFinding,
): ?array {
    $fields = [];
    $properties = [];
    foreach ($rawFields as $fieldIndex => $field) {
        try {
            $expr = php_check_type_ref($field['type'] ?? null);
        } catch (DefaultMetadataUnsupported $unsupported) {
            $addFinding(php_types_finding(
                'default-unsupported',
                $ownerId,
                $pointer . '/fields/' . $fieldIndex,
                'field `' . $field['name'] . '` carries unsupported metadata member `' . $unsupported->member . '`',
            ));
            return null;
        }
        if ($expr === null) {
            $addFinding(php_types_finding('type-unsupported', $ownerId,
                $pointer . '/fields/' . $fieldIndex,
                'field `' . $field['name'] . '` is not a closed mapped shape'));
            return null;
        }
        if (php_types_resolve_expr($expr, $definitions, $ownerId,
            $pointer . '/fields/' . $fieldIndex, $addFinding) === null) {
            return null;
        }
        $required = $field['required'] ?? false;
        $presence = match (true) {
            $required && !$expr['nullable'] => 'required-nonnull',
            $required && $expr['nullable'] => 'required-nullable',
            !$required && !$expr['nullable'] => 'optional-nonnull',
            default => 'optional-nullable',
        };
        $property = php_types_property_of($field['name']);
        if (isset($properties[strtolower($property)])) {
            $addFinding(php_types_finding('name-collision', $ownerId,
                $pointer . '/fields/' . $fieldIndex,
                'property spelling collision on ' . $property));
            return null;
        }
        $properties[strtolower($property)] = true;
        $collectionClass = null;
        if ($expr['list']) {
            $collectionClass = php_types_register_collection($expr, $definitions, $prefix, $collections);
        }
        if ($presence === 'optional-nonnull' || $presence === 'optional-nullable') {
            // Presence wrappers exist only for optional positions: the
            // required-nullable case is a plain `?T` constructor type.
            php_types_register_wrapper($expr, $definitions, $prefix, $collectionClass,
                $presence === 'optional-nullable', $wrappers);
        }
        $row = [
            'name' => $field['name'],
            'property' => $property,
            'type' => [
                'leaf' => $expr['leaf'],
                'list' => $expr['list'],
                'nullable' => $expr['nullable'],
            ],
            'presence' => $presence,
            'nullable' => $expr['nullable'],
        ];
        if ($expr['list']) {
            $row['type']['collection'] = $collectionClass['fqn'];
            if ($expr['nullableElements']) {
                $row['type']['nullableElements'] = true;
            }
        }
        if ($presence === 'optional-nonnull' || $presence === 'optional-nullable') {
            $row['type']['wrapper'] = $wrappers[count($wrappers) - 1]['fqn'];
        }
        $fields[] = $row;
    }
    return $fields;
}

/**
 * Resolve one normalized expression against the definition index: the
 * leaf must exist and map to a type. Returns null (with a finding)
 * when the reference is unresolved.
 */
function php_types_resolve_expr(
    array $expr,
    array $definitions,
    string $ownerId,
    string $pointer,
    callable $addFinding,
): ?array {
    $leaf = $expr['leaf'];
    $definition = $definitions[$leaf] ?? null;
    if ($definition === null || !in_array($definition['kind'], PHP_TYPES_MAPPED_KINDS, true)) {
        $addFinding(php_types_finding('ref-unresolved', $ownerId, $pointer,
            'reference `' . $leaf . '` does not resolve to a mapped type'));
        return null;
    }
    return $expr;
}

/** The bare codec class name of one mapped entry (the final FQN segment). */
function php_types_entry_codec_class(string $codecFqn): string
{
    $cut = strrpos($codecFqn, '\\');
    return $cut === false ? $codecFqn : substr($codecFqn, $cut + 1);
}

/**
 * Register (or find) the immutable collection class one list position
 * needs. Keyed by element and element nullability, so the same list
 * shape shares one class across every definition.
 */
function php_types_register_collection(
    array $expr,
    array $definitions,
    string $prefix,
    array &$collections,
): array {
    $leaf = $expr['leaf'];
    $nullableElements = $expr['nullableElements'];
    $key = $leaf . '|' . ($nullableElements ? 'nullable' : 'plain');
    foreach ($collections as $collection) {
        if ($collection['key'] === $key) {
            return $collection;
        }
    }
    $definition = $definitions[$leaf];
    $module = php_types_module_of($leaf);
    $class = php_types_stem_of($leaf, $definition['kind']) . 'List';
    if ($nullableElements) {
        // The null-permitting twin of a list class keeps its own name so
        // both variants can coexist collision-free.
        $class = php_types_stem_of($leaf, $definition['kind']) . 'NullableList';
    }
    $collection = [
        'key' => $key,
        'element' => $leaf,
        'elementKind' => $definition['kind'],
        'nullableElements' => $nullableElements,
        'class' => $class,
        'fqn' => php_types_fqn_of($prefix, $module, $class),
        'path' => php_types_path_of($module, $class),
    ];    $collections[] = $collection;
    return $collection;
}

/**
 * Register (or find) the concrete presence wrapper one optional
 * position needs. `acceptsNull` separates the optional-nonnull flavor
 * (absent | value, null refused) from the optional-nullable flavor
 * (absent | null | value, three distinct states).
 */
function php_types_register_wrapper(
    array $expr,
    array $definitions,
    string $prefix,
    ?array $collection,
    bool $acceptsNull,
    array &$wrappers,
): array {
    $leaf = $expr['leaf'];
    $key = $leaf . '|' . ($collection !== null ? 'list' : 'plain') . '|' . ($acceptsNull ? 'nullable' : 'plain');
    foreach ($wrappers as $wrapper) {
        if ($wrapper['key'] === $key) {
            return $wrapper;
        }
    }
    $definition = $definitions[$leaf];
    $module = php_types_module_of($leaf);
    $stem = $collection !== null ? $collection['class'] : php_types_stem_of($leaf, $definition['kind']);
    $class = 'Optional' . ($acceptsNull ? 'Nullable' : '') . $stem;
    // Wrapper classes live in the module's Optional sub-namespace.
    $wrapper = [
        'key' => $key,
        'element' => $leaf,
        'elementKind' => $definition['kind'],
        'ofList' => $collection !== null,
        'acceptsNull' => $acceptsNull,
        'class' => $class,
        'fqn' => php_types_fqn_of($prefix, $module, 'Optional\\' . $class),
        'path' => php_types_path_of($module, 'Optional\\' . $class),
        'collection' => $collection['fqn'] ?? null,
    ];
    $wrappers[] = $wrapper;
    return $wrapper;
}

/**
 * Detect reference cycles among the structured definitions (a codec
 * for a recursive type cannot terminate). Every type on a cycle gets
 * one bounded finding; the emitter stays silent.
 */
function php_find_type_cycles(array $definitions, callable $addFinding): void
{
    $state = []; // 1 = open, 2 = done
    $stack = [];
    $visit = static function (string $id) use (&$visit, &$state, &$stack, $definitions, $addFinding): void {
        if (($state[$id] ?? 0) === 2) {
            return;
        }
        if (($state[$id] ?? 0) === 1) {
            $cycle = array_slice($stack, (int) array_search($id, $stack, true));
            $cycle[] = $id;
            foreach ($cycle as $member) {
                $definition = $definitions[$member];
                $addFinding(php_types_finding('recursive-codec-unsupported', $member,
                    $definition['pointer'],
                    'recursive reference cycle: ' . implode(' -> ', $cycle)));
            }
            return;
        }
        $state[$id] = 1;
        $stack[] = $id;
        $definition = $definitions[$id];
        $fields = match ($definition['kind']) {
            'command' => $definition['input'] ?? [],
            'event' => $definition['payload'] ?? [],
            'value-object', 'entity' => $definition['fields'] ?? [],
            default => [],
        };
        foreach ($fields as $field) {
            try {
                $expr = php_check_type_ref($field['type'] ?? null);
            } catch (DefaultMetadataUnsupported) {
                // The field mapper owns that refusal; the cycle walk
                // only needs the reference graph.
                continue;
            }
            if ($expr === null) {
                continue;
            }
            $target = $definitions[$expr['leaf']] ?? null;
            if ($target !== null && in_array($target['kind'], ['value-object', 'entity', 'command', 'event'], true)) {
                $visit($expr['leaf']);
            }
        }
        array_pop($stack);
        $state[$id] = 2;
    };
    foreach (array_keys($definitions) as $id) {
        if (in_array($definitions[$id]['kind'], ['value-object', 'entity', 'command', 'event'], true)) {
            $visit($id);
        }
    }
}

/**
 * The complete path-sorted artifact inventory of one mapped inventory,
 * with naming custody applied (duplicate FQN and case-insensitive
 * path collisions are bounded findings).
 */
function php_types_collect_artifacts(
    array $types,
    array $collections,
    array $wrappers,
    array $policy,
    callable $addFinding,
): array {
    $artifacts = [];
    $byFqn = [];
    $byPath = [];
    $record = static function (array $artifact) use (&$artifacts, &$byFqn, &$byPath, $addFinding): void {
        $lowerFqn = strtolower($artifact['fqn']);
        $lowerPath = strtolower($artifact['path']);
        if (isset($byFqn[$lowerFqn])) {
            $addFinding(php_types_finding('name-collision', $artifact['semanticId'] ?? null, null,
                'duplicate class FQN ' . $artifact['fqn']));
            return;
        }
        if (isset($byPath[$lowerPath])) {
            $addFinding(php_types_finding('path-collision', $artifact['semanticId'] ?? null, null,
                'case-insensitive artifact path collision ' . $artifact['path']));
            return;
        }
        $byFqn[$lowerFqn] = true;
        $byPath[$lowerPath] = true;
        $artifacts[] = $artifact;
    };
    foreach ($types as $entry) {
        if ($entry['kind'] !== 'query') {
            $record([
                'path' => $entry['path'],
                'fqn' => $entry['fqn'],
                'role' => 'type',
                'semanticId' => $entry['semanticId'],
            ]);
        }
        if ($entry['codecPath'] !== $entry['path']) {
            // Scalar wrappers and enums are their own codec: one file,
            // one artifact row.
            $record([
                'path' => $entry['codecPath'],
                'fqn' => $entry['codec'],
                'role' => 'codec',
                'semanticId' => $entry['semanticId'],
            ]);
        }
    }
    foreach ($collections as $collection) {
        $record([
            'path' => $collection['path'],
            'fqn' => $collection['fqn'],
            'role' => 'collection',
            'semanticId' => $collection['element'],
            'element' => $collection['element'],
        ]);
    }
    foreach ($wrappers as $wrapper) {
        $record([
            'path' => $wrapper['path'],
            'fqn' => $wrapper['fqn'],
            'role' => 'optional',
            'semanticId' => $wrapper['element'],
            'element' => $wrapper['element'],
        ]);
    }
    if ($policy['classMap']) {
        $record([
            'path' => 'classmap.php',
            'fqn' => $policy['namespacePrefix'] . '\\ClassMap',
            'role' => 'class-map',
        ]);
    }
    $record([
        'path' => 'types.map.json',
        'fqn' => $policy['namespacePrefix'] . '\\TypesMap',
        'role' => 'class-map',
    ]);
    usort($artifacts, static fn (array $left, array $right): int => strcmp($left['path'], $right['path']));
    return $artifacts;
}
