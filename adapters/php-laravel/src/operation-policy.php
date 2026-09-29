<?php
/**
 * The closed operations policy and input validation of the PHP Laravel
 * operations generator (issue #59). Everything here is pure validation
 * over plain data: the bounded operations input document
 * (`lekalo/operations/*.operations.json`, contract
 * `dev.lekalo.php-operations-input@0.4.0`). Nothing reads the
 * filesystem, nothing writes, nothing executes project code.
 *
 * The closed grammar mirrors `contracts/php-operations-input.schema.v0.4.0.json`
 * and the core join (`crates/lekalo-core/src/php_operations/`) exactly:
 * unknown members and unknown enum values refuse. The core join is the
 * acceptance authority; this mirror is the adapter's defensive gate so
 * a request can never reach the emitter half-validated.
 */

const PHP_OPERATIONS_INPUT_SCHEMA_VERSION = 'lekalo/php-operations-input/v0.4.0';
const PHP_OPERATIONS_INPUT_IDENTITY = 'dev.lekalo.php-operations-input@0.4.0';
const PHP_OPERATIONS_MAP_SCHEMA_VERSION = 'lekalo/php-operations-map/v0.4.0';
const PHP_OPERATIONS_MAP_IDENTITY = 'dev.lekalo.php-operations-map@0.4.0';
const PHP_OPERATIONS_EVIDENCE_SCHEMA_VERSION = 'lekalo/php-operations-evidence/v0.4.0';
const PHP_OPERATIONS_EVIDENCE_IDENTITY = 'dev.lekalo.php-operations-evidence@0.4.0';

/** The generated operations root (managed custody). */
const PHP_OPERATIONS_GENERATED_ROOT = '.lekalo/generated/php-laravel/operations';

/** The user-owned scaffold home of operations generation (closed). */
const PHP_OPERATIONS_SCAFFOLD_ROOT = 'app/lekalo-operations';

/** The observed-handler evidence the checked join consumes. */
const PHP_OPERATIONS_EVIDENCE_PATH = '.lekalo/import/observed/operations-evidence.json';

const PHP_OPERATIONS_DEFAULT_NAMESPACE_PREFIX = 'Lekalo\\Generated\\Operations';
const PHP_OPERATIONS_SCAFFOLD_NAMESPACE_PREFIX = 'App\\LekaloOperations';

/** The bounded refusals of the operations input join. */
const PHP_OPERATIONS_REFUSALS = [
    'operations-input-unreadable',
    'operations-input-shape',
    'operations-input-identity',
    'operations-types-unbound',
    'operations-ir-digest',
    'operations-ir-unreadable',
    'operations-ir-shape',
    'operations-ir-identity',
    'operations-input-digest',
];

/**
 * Validate one parsed operations input document against its closed
 * shape. Returns the normalized input array, or null when the document
 * is not the accepted contract (a present-but-invalid document is an
 * authoring error, never an all-defaults fallback).
 */
function php_validate_operations_input(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_OPERATIONS_INPUT_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_OPERATIONS_INPUT_IDENTITY) {
        return null;
    }
    // The closed member set (additionalProperties: false): an unknown
    // member is an authoring error, never a silently ignored hint.
    foreach (array_keys($document) as $member) {
        if (!in_array($member, ['schemaVersion', 'identity', 'projectId', 'irDigest', 'typesInputDigest', 'policy', 'operations'], true)) {
            return null;
        }
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'typesInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $prefix = PHP_OPERATIONS_DEFAULT_NAMESPACE_PREFIX;
    if (array_key_exists('policy', $document)) {
        $policy = $document['policy'];
        if (!is_array($policy) || count($policy) !== 1
            || !isset($policy['namespacePrefix'])
            || !is_string($policy['namespacePrefix'])
            || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $policy['namespacePrefix']) !== 1) {
            return null;
        }
        $prefix = $policy['namespacePrefix'];
    }
    $operations = $document['operations'] ?? null;
    if (!is_array($operations) || $operations === [] || count($operations) > 4096) {
        return null;
    }
    $records = [];
    $previous = '';
    foreach ($operations as $record) {
        $validated = php_validate_operation_record($record);
        if ($validated === null) {
            return null;
        }
        if ($validated['id'] <= $previous) {
            // Canonical order is part of the shape.
            return null;
        }
        $previous = $validated['id'];
        $records[] = $validated;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'typesInputDigest' => $document['typesInputDigest'],
        'namespacePrefix' => $prefix,
        'operations' => $records,
    ];
}

/**
 * Validate one operation record against the closed shape.
 *
 * @return array<string, mixed>|null
 */
function php_validate_operation_record(mixed $record): ?array
{
    if (!is_array($record)) {
        return null;
    }
    // The closed record member set (additionalProperties: false).
    foreach (array_keys($record) as $member) {
        if (!in_array($member, ['id', 'kind', 'mode', 'entry', 'recipe', 'errors', 'policy', 'transaction'], true)) {
            return null;
        }
    }
    $id = $record['id'] ?? null;
    if (!is_string($id) || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $id) !== 1) {
        return null;
    }
    $kind = $record['kind'] ?? null;
    if ($kind !== 'command' && $kind !== 'query') {
        return null;
    }
    $mode = $record['mode'] ?? null;
    if (!in_array($mode, ['managed', 'scaffold-once', 'checked', 'custom'], true)) {
        return null;
    }
    $entry = null;
    if (array_key_exists('entry', $record) && $record['entry'] !== null) {
        $entry = php_validate_operation_entry($record['entry']);
        if ($entry === null) {
            return null;
        }
    }
    $recipe = null;
    if (array_key_exists('recipe', $record) && $record['recipe'] !== null) {
        $recipe = php_validate_recipe($record['recipe']);
        if ($recipe === null) {
            return null;
        }
    }
    $errors = [];
    if (array_key_exists('errors', $record) && $record['errors'] !== null) {
        if (!is_array($record['errors']) || count($record['errors']) > 64) {
            return null;
        }
        $previous = '';
        foreach ($record['errors'] as $error) {
            if (!is_string($error)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $error) !== 1
                || $error <= $previous) {
                return null;
            }
            $previous = $error;
            $errors[] = $error;
        }
    }
    $policyId = null;
    if (array_key_exists('policy', $record) && $record['policy'] !== null) {
        $policy = $record['policy'];
        if (!is_array($policy) || count($policy) !== 1) {
            return null;
        }
        $policyId = $policy['id'] ?? null;
        if (!is_string($policyId)
            || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $policyId) !== 1) {
            return null;
        }
    }
    $transaction = 'forbidden';
    if (array_key_exists('transaction', $record) && $record['transaction'] !== null) {
        $transaction = $record['transaction']['mode'] ?? null;
        if ($transaction !== 'required' && $transaction !== 'forbidden') {
            return null;
        }
    }
    // Mode pairing: checked/custom declare, managed/scaffold carry the
    // closed recipe that drives the signature (the scaffold body stays
    // the explicit unimplemented failure).
    if (in_array($mode, ['checked', 'custom'], true) !== ($entry !== null)) {
        return null;
    }
    if (in_array($mode, ['managed', 'scaffold-once'], true) !== ($recipe !== null)) {
        return null;
    }
    return [
        'id' => $id,
        'kind' => $kind,
        'mode' => $mode,
        'entry' => $entry,
        'recipe' => $recipe,
        'errors' => $errors,
        'policyId' => $policyId,
        'transaction' => $transaction,
    ];
}

/** Validate one declared native entrypoint. */
function php_validate_operation_entry(mixed $entry): ?array
{
    if (!is_array($entry) || count($entry) !== 3) {
        return null;
    }
    $fqn = $entry['fqn'] ?? null;
    $method = $entry['method'] ?? null;
    $path = $entry['path'] ?? null;
    if (!is_string($fqn)
        || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $fqn) !== 1) {
        return null;
    }
    if ($method !== 'handle') {
        return null;
    }
    if (!is_string($path) || preg_match('/^[a-z][a-z0-9_.\/-]*\.php$/', $path) !== 1
        || str_contains($path, '..')) {
        return null;
    }
    return ['fqn' => $fqn, 'method' => $method, 'path' => $path];
}

/**
 * Validate one closed recipe. Only the grammar is decided here; the
 * semantic join is the core's authority (and the core runs it before
 * any adapter exchange).
 */
function php_validate_recipe(mixed $recipe): ?array
{
    if (!is_array($recipe) || !is_string($recipe['kind'] ?? null)) {
        return null;
    }
    if ($recipe['kind'] === 'port-delegation') {
        if (count($recipe) > 4) {
            return null;
        }
        $port = $recipe['port'] ?? null;
        $method = $recipe['method'] ?? null;
        $result = $recipe['result'] ?? null;
        if (!is_string($port) || preg_match('/^[A-Z][A-Za-z0-9_]*$/', $port) !== 1) {
            return null;
        }
        if (!is_string($method) || preg_match('/^[a-z][A-Za-z0-9_]*$/', $method) !== 1) {
            return null;
        }
        if ($result !== null
            && (!is_string($result)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $result) !== 1)) {
            return null;
        }
        return ['kind' => 'port-delegation', 'port' => $port, 'method' => $method, 'result' => $result];
    }
    if ($recipe['kind'] !== 'single-entity-update') {
        return null;
    }
    foreach (['entity', 'key', 'assignments', 'kept', 'missingBehavior'] as $required) {
        if (!array_key_exists($required, $recipe)) {
            return null;
        }
    }
    if (count($recipe) > 8) {
        return null;
    }
    $entity = $recipe['entity'];
    if (!is_string($entity)
        || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $entity) !== 1) {
        return null;
    }
    $key = $recipe['key'];
    if (!is_string($key) || php_operations_is_field_name($key) !== true) {
        return null;
    }
    $assignments = [];
    if (!is_array($recipe['assignments']) || $recipe['assignments'] === []
        || count($recipe['assignments']) > 64) {
        return null;
    }
    $seen = [];
    foreach ($recipe['assignments'] as $assignment) {
        if (!is_array($assignment) || count($assignment) !== 2
            || !php_operations_is_field_name($assignment['field'] ?? null)) {
            return null;
        }
        $value = php_operations_validate_operand($assignment['value'] ?? null);
        if ($value === null || isset($seen[$assignment['field']])) {
            return null;
        }
        $seen[$assignment['field']] = true;
        $assignments[] = ['field' => $assignment['field'], 'value' => $value];
    }
    $kept = [];
    if (!is_array($recipe['kept']) || count($recipe['kept']) > 64) {
        return null;
    }
    foreach ($recipe['kept'] as $field) {
        if (!php_operations_is_field_name($field) || isset($seen[$field])) {
            return null;
        }
        $seen[$field] = true;
        $kept[] = $field;
    }
    $preconditions = [];
    if (array_key_exists('preconditions', $recipe)) {
        if (!is_array($recipe['preconditions']) || count($recipe['preconditions']) > 16) {
            return null;
        }
        foreach ($recipe['preconditions'] as $precondition) {
            if (!is_array($precondition) || count($precondition) !== 3
                || !php_operations_is_field_name($precondition['field'] ?? null)) {
                return null;
            }
            $equals = php_operations_validate_operand($precondition['equals'] ?? null);
            $error = $precondition['error'] ?? null;
            if ($equals === null || !is_string($error)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $error) !== 1) {
                return null;
            }
            $preconditions[] = ['field' => $precondition['field'], 'equals' => $equals, 'error' => $error];
        }
    }
    $missing = $recipe['missingBehavior'];
    if (!is_array($missing) || count($missing) !== 1
        || !is_string($missing['error'] ?? null)
        || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', (string) ($missing['error'] ?? '')) !== 1) {
        return null;
    }
    $emissions = [];
    if (array_key_exists('emit', $recipe)) {
        if (!is_array($recipe['emit']) || count($recipe['emit']) > 16) {
            return null;
        }
        foreach ($recipe['emit'] as $emission) {
            if (!is_array($emission) || count($emission) !== 2) {
                return null;
            }
            $event = $emission['event'] ?? null;
            if (!is_string($event)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $event) !== 1
                || !is_array($emission['payload']) || count($emission['payload']) > 64) {
                return null;
            }
            $payload = [];
            foreach ($emission['payload'] as $field => $operand) {
                $value = php_operations_validate_operand($operand);
                if ($value === null || !php_operations_is_field_name((string) $field)) {
                    return null;
                }
                $payload[$field] = $value;
            }
            $emissions[] = ['event' => $event, 'payload' => $payload];
        }
    }
    return [
        'kind' => 'single-entity-update',
        'entity' => $entity,
        'key' => $key,
        'assignments' => $assignments,
        'kept' => $kept,
        'preconditions' => $preconditions,
        'missingBehavior' => ['error' => $missing['error']],
        'emit' => $emissions,
    ];
}

/** Whether one name is a closed camelCase field identifier. */
function php_operations_is_field_name(mixed $name): bool
{
    return is_string($name)
        && preg_match('/^[a-z][a-zA-Z0-9_]*$/', $name) === 1
        && strlen($name) <= 63;
}

/** One closed typed operand, or null when the shape is foreign. */
function php_operations_validate_operand(mixed $operand): ?array
{
    if (!is_array($operand) || count($operand) !== 1) {
        return null;
    }
    if (isset($operand['fromInput'])) {
        return php_operations_is_field_name($operand['fromInput'])
            ? ['fromInput' => $operand['fromInput']]
            : null;
    }
    if (isset($operand['fromEntity'])) {
        return php_operations_is_field_name($operand['fromEntity'])
            ? ['fromEntity' => $operand['fromEntity']]
            : null;
    }
    if (isset($operand['enumCase'])) {
        $case = $operand['enumCase'];
        if (!is_array($case) || count($case) !== 2) {
            return null;
        }
        $type = $case['type'] ?? null;
        $value = $case['value'] ?? null;
        if (!is_string($type)
            || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $type) !== 1
            || !is_string($value) || $value === '' || strlen($value) > 64
            || preg_match('/^[a-zA-Z0-9_-]+$/', $value) !== 1) {
            return null;
        }
        return ['enumCase' => ['type' => $type, 'value' => $value]];
    }
    if (array_key_exists('literal', $operand)) {
        $literal = $operand['literal'];
        if (is_string($literal) && strlen($literal) <= 256) {
            return ['literal' => $literal];
        }
        if (is_bool($literal)) {
            return ['literal' => $literal];
        }
        if (is_int($literal)) {
            return ['literal' => $literal];
        }
    }
    return null;
}
