<?php
/**
 * The closed type policy and input validation of the PHP type generator
 * (issue #58). Everything in this module is pure validation over plain
 * data: the bounded types-input document (`lekalo/types/*.types.json`,
 * contract `dev.lekalo.php-types-input@0.4.0`) and the compiled project
 * IR evidence it names. Nothing reads the filesystem, nothing writes,
 * nothing executes project code.
 *
 * Defaults are documented in `contracts/php-types-input.schema.v0.4.0.json`
 * and enforced here with the same closed vocabulary: unknown members and
 * unknown enum values refuse. The policy deliberately has no library
 * codec, date library, or Composer member in v0.4.0 — their absence is
 * the explicit unsupported boundary, never a silent fallback.
 */

// ---------------------------------------------------------------------------
// Closed policy vocabulary.
// ---------------------------------------------------------------------------

const PHP_TYPES_INPUT_SCHEMA_VERSION = 'lekalo/php-types-input/v0.4.0';
const PHP_TYPES_INPUT_IDENTITY = 'dev.lekalo.php-types-input@0.4.0';
const PHP_TYPES_MAP_SCHEMA_VERSION = 'lekalo/php-types-map/v0.4.0';
const PHP_TYPES_MAP_IDENTITY = 'dev.lekalo.php-types-map@0.4.0';
const PHP_TYPES_EVIDENCE_SCHEMA_VERSION = 'lekalo/php-types-evidence/v0.4.0';
const PHP_TYPES_EVIDENCE_IDENTITY = 'dev.lekalo.php-types-evidence@0.4.0';
const PHP_TYPES_IR_IDENTITY = 'dev.lekalo.ir@0.2.16';

/** The generated types root (managed custody) and its read/doc input home. */
const PHP_TYPES_GENERATED_ROOT = '.lekalo/generated/php-laravel/types';
const PHP_TYPES_INPUT_HOME = 'lekalo/types';

/**
 * The user-owned scaffold home of type generation (issue #58). The
 * accepted v0.4.0 value is closed: the core lifecycle classifier and the
 * adapter agree on user ownership by this exact path convention, so a
 * policy cannot silently move a scaffold under an unrecognized root.
 */
const PHP_TYPES_SCAFFOLD_ROOT = 'app/lekalo-types';

/** The observed class-shape evidence the checked join consumes. */
const PHP_TYPES_EVIDENCE_PATH = '.lekalo/import/observed/types-evidence.json';

const PHP_TYPES_CUSTODY_MODES = ['managed', 'scaffold-once', 'checked'];
const PHP_TYPES_DEFAULT_NAMESPACE_PREFIX = 'Lekalo\\Generated\\Types';

/** The closed scalar base vocabulary (Model `$defs.scalarDefinition`). */
const PHP_TYPES_SCALAR_BASES = ['string', 'number', 'boolean', 'date', 'datetime', 'uuid', 'uri'];

/** The closed IR definition kinds and which of them map to a type. */
const PHP_TYPES_IR_KINDS = [
    'scalar', 'enum', 'value-object', 'entity', 'command', 'query', 'event',
    'effect', 'endpoint', 'target-binding', 'policy', 'scenario', 'module', 'project',
];
const PHP_TYPES_MAPPED_KINDS = [
    'scalar', 'enum', 'value-object', 'entity', 'command', 'query', 'event',
];

/**
 * The bounded refusal codes of the input join. A refusal is an
 * in-envelope `failed` outcome class, never a guessed plan.
 */
const PHP_TYPES_REFUSALS = [
    'types-input-unreadable',
    'types-input-shape',
    'types-input-identity',
    'types-input-policy',
    'types-input-digest',
    'types-ir-unreadable',
    'types-ir-shape',
    'types-ir-identity',
];

/**
 * Validate one parsed types-input document against its closed shape and
 * return the normalized input `{projectId, irDigest, policy}`, or null
 * when the document is not the accepted contract (a present-but-invalid
 * document is an authoring error, never an all-defaults fallback).
 */
function php_validate_types_input(mixed $document): ?array
{
    if (!is_array($document)) {
        return null;
    }
    if (($document['schemaVersion'] ?? null) !== PHP_TYPES_INPUT_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_TYPES_INPUT_IDENTITY) {
        return null;
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    $irDigest = $document['irDigest'] ?? null;
    if (!is_sha256_digest($irDigest)) {
        return null;
    }
    $policy = php_validate_type_policy($document['policy'] ?? null);
    if ($policy === null) {
        return null;
    }
    return ['projectId' => $projectId, 'irDigest' => $irDigest, 'policy' => $policy];
}

/**
 * Validate the closed type policy (issue #58 step 1). `null` policy is
 * the all-defaults policy; a present-but-invalid policy refuses. The
 * custody/scaffold-root pairing is closed: scaffold-once requires the
 * recognized consumer root, and the other modes forbid it outright.
 */
function php_validate_type_policy(mixed $policy): ?array
{
    if ($policy === null) {
        $policy = [];
    }
    if (!is_array($policy)) {
        return null;
    }
    $prefix = PHP_TYPES_DEFAULT_NAMESPACE_PREFIX;
    if (array_key_exists('namespacePrefix', $policy)) {
        $value = $policy['namespacePrefix'];
        if (!is_string($value)
            || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $value) !== 1) {
            return null;
        }
        $prefix = $value;
    }
    $custody = 'managed';
    if (array_key_exists('custody', $policy)) {
        $value = $policy['custody'];
        if (!is_string($value) || !in_array($value, PHP_TYPES_CUSTODY_MODES, true)) {
            return null;
        }
        $custody = $value;
    }
    $scaffoldRoot = null;
    if (array_key_exists('scaffoldRoot', $policy)) {
        $value = $policy['scaffoldRoot'];
        if ($value !== PHP_TYPES_SCAFFOLD_ROOT) {
            return null;
        }
        $scaffoldRoot = $value;
    }
    if (($custody === 'scaffold-once') !== ($scaffoldRoot !== null)) {
        return null;
    }
    $classMap = true;
    if (array_key_exists('classMap', $policy)) {
        $value = $policy['classMap'];
        if (!is_bool($value)) {
            return null;
        }
        $classMap = $value;
    }
    return [
        'namespacePrefix' => $prefix,
        'custody' => $custody,
        'scaffoldRoot' => $scaffoldRoot,
        'classMap' => $classMap,
    ];
}

/**
 * Structural validation of the consumed compiled-IR evidence. The IR is
 * core-validated upstream, so this check is a bounded trust-but-type
 * gate over exactly the members the mapper consumes, plus the explicit
 * refusals the type policy owes (unknown default metadata, unknown type
 * tags). Returns null when the evidence cannot be typed at all (shape
 * refusal) — the mapper's `default-unsupported` finding covers the
 * well-formed-but-unsupported field metadata case.
 *
 * @return array{definitions: list<array<string, mixed>>}|null
 */
function php_check_ir_document(mixed $ir): ?array
{
    if (!is_array($ir) || ($ir['contract'] ?? null) !== PHP_TYPES_IR_IDENTITY) {
        return null;
    }
    if (!is_array($ir['definitions'] ?? null)) {
        return null;
    }
    foreach ($ir['definitions'] as $definition) {
        if (!is_array($definition) || !is_string($definition['id'] ?? null)
            || !is_string($definition['kind'] ?? null)
            || !in_array($definition['kind'], PHP_TYPES_IR_KINDS, true)) {
            return null;
        }
        $fields = null;
        switch ($definition['kind']) {
            case 'scalar':
                if (!is_string($definition['base'] ?? null)
                    || !in_array($definition['base'], PHP_TYPES_SCALAR_BASES, true)) {
                    return null;
                }
                break;
            case 'enum':
                if (!is_array($definition['values'] ?? null)) {
                    return null;
                }
                foreach ($definition['values'] as $value) {
                    if (!is_array($value) || array_key_exists('default', $value)) {
                        return null;
                    }
                    if (!is_string($value['value'] ?? null)) {
                        return null;
                    }
                }
                break;
            case 'value-object':
            case 'entity':
                $fields = $definition['fields'] ?? null;
                break;
            case 'command':
                $fields = $definition['input'] ?? null;
                break;
            case 'event':
                $fields = $definition['payload'] ?? null;
                break;
            case 'query':
                if (array_key_exists('returns', $definition)
                    && php_check_type_ref($definition['returns']) === null) {
                    return null;
                }
                break;
        }
        if (is_array($fields)) {
            $names = [];
            foreach ($fields as $field) {
                $checked = php_check_ir_field($field);
                if ($checked === null) {
                    return null;
                }
                $names[] = $checked;
            }
            if (count($names) !== count(array_unique($names))) {
                // Duplicate field names cannot carry a closed wire contract.
                return null;
            }
        }
    }
    return ['definitions' => $ir['definitions']];
}

/**
 * One IR field: closed members only. A `default` member — or any other
 * unconsumed metadata slot — is the explicit unsupported default case:
 * the mapper reports it as a typed finding instead of inventing a
 * defaulting rule.
 *
 * @return string|null the field name, or null when the field is not well-formed
 */
function php_check_ir_field(mixed $field): ?string
{
    if (!is_array($field)) {
        return null;
    }
    $name = $field['name'] ?? null;
    if (!is_string($name) || preg_match('/^[a-z][a-zA-Z0-9_]*$/', $name) !== 1) {
        return null;
    }
    foreach (array_keys($field) as $member) {
        if (!in_array($member, ['name', 'type', 'required', 'description'], true)) {
            // Unknown field metadata (a default, a regex, a range) has no
            // owning contract: refused, never silently ignored.
            throw DefaultMetadataUnsupported::fromField($name, is_string($member) ? $member : '?');
        }
    }
    if (php_check_type_ref($field['type'] ?? null) === null) {
        return null;
    }
    if (array_key_exists('required', $field) && !is_bool($field['required'])) {
        return null;
    }
    if (array_key_exists('description', $field) && !is_string($field['description'])) {
        return null;
    }
    return $name;
}

/**
 * One closed IR type expression: a leaf ref, a list, or an optional,
 * at most one of each wrapper layer (deeper nesting is a mapper
 * unsupported finding, not a shape refusal). Returns the normalized
 * expression or null when the tag is not part of the closed grammar.
 *
 * @return array{leaf: string, list: bool, nullableElements: bool, nullable: bool}|null
 */
function php_check_type_ref(mixed $typeRef): ?array
{
    if (!is_array($typeRef)) {
        return null;
    }
    $keys = array_keys($typeRef);
    if (count($keys) !== 1) {
        return null;
    }
    $tag = $keys[0];
    switch ($tag) {
        case 'ref':
            $leaf = $typeRef['ref'];
            if (!is_string($leaf)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $leaf) !== 1) {
                return null;
            }
            return ['leaf' => $leaf, 'list' => false, 'nullableElements' => false, 'nullable' => false];
        case 'list':
            $element = php_check_type_ref($typeRef['list'] ?? null);
            if ($element === null || $element['list']) {
                // A nested list wrapper is not a closed wire shape.
                return null;
            }
            // A list of optionals is representable: the element type
            // carries the optionality as nullable elements.
            $element['nullableElements'] = $element['nullable'];
            $element['list'] = true;
            $element['nullable'] = false;
            return $element;
        case 'optional':
            $inner = php_check_type_ref($typeRef['optional'] ?? null);
            if ($inner === null || $inner['nullable']) {
                // Nested optionals collapse no state: refuse closed.
                return null;
            }
            // The whole expression is nullable on top of whatever shape
            // the inner layer carries (leaf, list, or nullable-element list).
            $inner['nullable'] = true;
            return $inner;
        default:
            return null;
    }
}

/**
 * A well-formed field carrying metadata no owning contract defines
 * (issue #58: "Reject unknown default metadata now"). Thrown by the
 * field check; the mapper converts it into the bounded
 * `php-types.mapping-unsupported` finding with reason
 * `default-unsupported`.
 */
final class DefaultMetadataUnsupported extends RuntimeException
{
    public function __construct(
        public readonly string $fieldName,
        public readonly string $member,
    ) {
        parent::__construct('default-unsupported');
    }

    public static function fromField(string $fieldName, string $member): self
    {
        return new self($fieldName, $member);
    }
}
