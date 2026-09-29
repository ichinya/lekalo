<?php
/**
 * The codec emitter of the PHP type generator (issue #58, step 2): one
 * deterministic codec per mapped type. Decoding validates the closed
 * wire shape — unknown members, missing required members, wrong
 * primitives, precision loss and unknown enum values refuse — and
 * encoding reproduces canonical JSON semantics: absent stays absent,
 * explicit null stays null, lists stay lists, objects stay objects,
 * and wire names stay verbatim (`task_id` never becomes `taskId` on
 * the wire).
 */

if (!function_exists('php_map_types')) {
    require_once __DIR__ . '/type-map.php';
}

/**
 * The class FQN of one leaf reference (the nominal wrapper, the enum,
 * or the DTO/value-object class).
 */
function php_types_leaf_fqn(array $definitions, string $prefix, string $leaf): string
{
    $definition = $definitions[$leaf];
    return php_types_fqn_of($prefix, php_types_module_of($leaf), php_types_stem_of($leaf, $definition['kind']));
}

/** Whether one leaf decodes through its own class (scalar, enum). */
function php_types_leaf_is_self_codec(array $definitions, string $leaf): bool
{
    return in_array($definitions[$leaf]['kind'], ['scalar', 'enum'], true);
}

/**
 * The decode expression of one wire value at a resolved leaf: scalars
 * and enums decode through their own class, structured values through
 * their codec class.
 */
function php_types_leaf_decode(array $definitions, string $prefix, string $leaf, string $raw): string
{
    $fqn = '\\' . php_types_leaf_fqn($definitions, $prefix, $leaf);
    return php_types_leaf_is_self_codec($definitions, $leaf)
        ? $fqn . '::fromWire(' . $raw . ')'
        : $fqn . 'Codec::decode(' . $raw . ')';
}

/**
 * The encode expression of one typed leaf value: the exact wire
 * projection (enum backed value, wrapper spelling, codec delegation).
 */
function php_types_leaf_encode(array $definitions, string $prefix, string $leaf, string $target): string
{
    $definition = $definitions[$leaf];
    if ($definition['kind'] === 'enum') {
        return $target . '->toWire()';
    }
    if ($definition['kind'] === 'scalar') {
        return $target . (($definition['base'] === 'number' || $definition['base'] === 'boolean') ? '->value()' : '->toString()');
    }
    return '\\' . php_types_leaf_fqn($definitions, $prefix, $leaf) . 'Codec::encode(' . $target . ')';
}

/**
 * The PHP property type of one mapped field row: the concrete class,
 * collection, or presence wrapper the position binds to. A
 * required-nullable position spells `?T`; an optional position spells
 * its wrapper (the wrapper owns the optionality).
 */
function php_types_field_php_type(array $field, array $definitions, string $prefix): string
{
    $expr = $field['type'];
    if (isset($expr['wrapper'])) {
        return '\\' . $expr['wrapper'];
    }
    if ($expr['list']) {
        $type = '\\' . $expr['collection'];
    } else {
        $type = '\\' . php_types_leaf_fqn($definitions, $prefix, $expr['leaf']);
    }
    return $field['nullable'] ? '?' . $type : $type;
}

/** The single-quoted PHP string literal of one wire text. */
function php_types_string_literal(string $value): string
{
    return "'" . str_replace(["\\", "'"], ["\\\\", "\\'"], $value) . "'";
}

/**
 * The decode fragment of one field: statements binding `$<property>`
 * to the decoded PHP value. Presence is exact — required-nonnull
 * refuses absent and null; required-nullable refuses absent, accepts
 * null; optional positions bind a concrete presence wrapper.
 */
function php_types_field_decode(array $field, array $definitions, string $prefix, string $ownerId): string
{
    $wireKey = php_types_string_literal($field['name']);
    $owner = php_types_string_literal($ownerId);
    $member = $field['name'];
    $wire = '$wire[' . $wireKey . ']';
    $property = '$' . $field['property'];
    $expr = $field['type'];
    $lines = [];

    $valueDecode = static function (string $raw) use ($definitions, $prefix, $expr, $field): string {
        if ($expr['list']) {
            return 'self::decode' . ucfirst($field['property']) . '(' . $raw . ')';
        }
        return php_types_leaf_decode($definitions, $prefix, $expr['leaf'], $raw);
    };

    switch ($field['presence']) {
        case 'required-nonnull':
            $lines[] = 'if (!array_key_exists(' . $wireKey . ', $wire) || ' . $wire . ' === null) {';
            $lines[] = '    throw new \\InvalidArgumentException(' . $owner . ' . \': missing required member `' . $member . '`\');';
            $lines[] = '}';
            $lines[] = $property . ' = ' . $valueDecode($wire) . ';';
            break;
        case 'required-nullable':
            $lines[] = 'if (!array_key_exists(' . $wireKey . ', $wire)) {';
            $lines[] = '    throw new \\InvalidArgumentException(' . $owner . ' . \': missing required member `' . $member . '`\');';
            $lines[] = '}';
            $lines[] = $property . ' = ' . $wire . ' === null ? null : ' . $valueDecode($wire) . ';';
            break;
        case 'optional-nonnull':
        case 'optional-nullable':
            $wrapper = '\\' . $expr['wrapper'];
            $lines[] = $property . ' = !array_key_exists(' . $wireKey . ', $wire)';
            $lines[] = '    ? ' . $wrapper . '::absent()';
            if ($field['presence'] === 'optional-nullable') {
                $lines[] = '    : (' . $wire . ' === null';
                $lines[] = '        ? ' . $wrapper . '::ofNull()';
                $lines[] = '        : ' . $wrapper . '::of(' . $valueDecode($wire) . '));';
            } else {
                // An explicit null fails inside the element decode.
                $lines[] = '    : ' . $wrapper . '::of(' . $valueDecode($wire) . ');';
            }
            break;
    }
    return implode("\n", $lines);
}

/**
 * The encode fragment of one field: statements appending the verbatim
 * wire member to `$result` — absent optionals append nothing, explicit
 * nulls append null, and the member order is the declared field order.
 */
function php_types_field_encode(array $field, array $definitions, string $prefix): string
{
    $wireKey = php_types_string_literal($field['name']);
    $property = '$value->' . $field['property'];
    $expr = $field['type'];
    $lines = [];

    $valueEncode = static function (string $target) use ($definitions, $prefix, $expr, $field): string {
        if ($expr['list']) {
            return 'self::encode' . ucfirst($field['property']) . '(' . $target . ')';
        }
        return php_types_leaf_encode($definitions, $prefix, $expr['leaf'], $target);
    };

    switch ($field['presence']) {
        case 'required-nonnull':
            $lines[] = '$result[' . $wireKey . '] = ' . $valueEncode($property) . ';';
            break;
        case 'required-nullable':
            $lines[] = '$result[' . $wireKey . '] = ' . $property . ' === null ? null : ' . $valueEncode($property) . ';';
            break;
        case 'optional-nonnull':
        case 'optional-nullable':
            $lines[] = 'if (!$value->' . $field['property'] . '->isAbsent()) {';
            if ($field['presence'] === 'optional-nullable') {
                $lines[] = '    $result[' . $wireKey . '] = $value->' . $field['property'] . '->isNull() ? null : '
                    . $valueEncode($property . '->get()') . ';';
            } else {
                $lines[] = '    $result[' . $wireKey . '] = ' . $valueEncode($property . '->get()') . ';';
            }
            $lines[] = '}';
            break;
    }
    return implode("\n", $lines);
}

/**
 * The private list decode/encode helper pair of one list field (or of
 * a query return). Element decoding validates every element; the
 * collection constructor is the only list publisher.
 *
 * Accepted wire boundary: the codec consumes PHP-decoded JSON values,
 * where a JSON object and an empty list are both `[]` and `"0":…`
 * loses its object-ness. A wire object therefore decodes as an empty
 * (or numerically keyed) list at this layer; duplicate members and
 * envelope-level shape custody belong to the kernel's JSON boundary
 * (`decode_json_document`), which refuses duplicates before any codec
 * runs (issue #58).
 */
function php_types_list_helpers(array $field, array $definitions, string $prefix, string $ownerId): string
{
    $expr = $field['type'];
    $collection = '\\' . $expr['collection'];
    $method = ucfirst($field['property']);
    $owner = php_types_string_literal($ownerId);
    $member = $field['name'];
    $nullableElements = $expr['nullableElements'] ?? false;

    $elementDecode = $nullableElements
        ? '$element === null ? null : ' . php_types_leaf_decode($definitions, $prefix, $expr['leaf'], '$element')
        : php_types_leaf_decode($definitions, $prefix, $expr['leaf'], '$element');
    $elementEncode = $nullableElements
        ? '$element === null ? null : ' . php_types_leaf_encode($definitions, $prefix, $expr['leaf'], '$element')
        : php_types_leaf_encode($definitions, $prefix, $expr['leaf'], '$element');

    return <<<PHP
    private static function decode{$method}(mixed \$raw): {$collection}
    {
        if (!is_array(\$raw) || !array_is_list(\$raw)) {
            throw new \\InvalidArgumentException({$owner} . ': member `{$member}` is not a list');
        }
        \$items = [];
        foreach (\$raw as \$element) {
            \$items[] = {$elementDecode};
        }
        return {$collection}::fromList(\$items);
    }

    private static function encode{$method}({$collection} \$value): array
    {
        \$items = [];
        foreach (\$value->all() as \$element) {
            \$items[] = {$elementEncode};
        }
        return \$items;
    }
PHP;
}
