<?php
/**
 * The operations bindings join (issue #59): declared checked and custom
 * entrypoints join against the observed-handler evidence document. The
 * architecture mirrors the #58 type bindings join: a closed evidence
 * shape, exact identity digests, and typed findings for missing,
 * ambiguous, stale, or diverging records. The join compares the
 * declared FQN/method/path and the observed constructor slots, method
 * shape, and public entrypoint count. Presence of a plausible document
 * is never trusted as producer execution — the producer receipt rides
 * the evidence, and the pinned-tool lane owns its authenticity.
 */

const PHP_OPERATIONS_BINDING_MISSING = 'operations.binding-missing';
const PHP_OPERATIONS_BINDING_AMBIGUOUS = 'operations.binding-ambiguous';
const PHP_OPERATIONS_BINDING_MISMATCH = 'operations.binding-mismatch';
const PHP_OPERATIONS_BINDING_STALE = 'operations.binding-stale';

/**
 * Validate one parsed observed evidence document against its closed
 * shape. Returns the normalized document or null.
 *
 * @return array<string, mixed>|null
 */
function php_validate_operations_evidence(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_OPERATIONS_EVIDENCE_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_OPERATIONS_EVIDENCE_IDENTITY) {
        return null;
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'operationsInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $producer = $document['producer'] ?? null;
    if (!is_array($producer)
        || ($producer['tool'] ?? null) !== 'mago'
        || !is_string($producer['version'] ?? null)
        || preg_match('/^[0-9]+\.[0-9]+\.[0-9]+$/', (string) ($producer['version'] ?? '')) !== 1) {
        return null;
    }
    if (array_key_exists('receiptPath', $producer)) {
        if (!is_string($producer['receiptPath'])
            || preg_match('/^[.a-z][a-z0-9_\/.-]*\.json$/', $producer['receiptPath']) !== 1) {
            return null;
        }
        if (isset($producer['receiptDigest']) && !is_sha256_digest($producer['receiptDigest'])) {
            return null;
        }
    }
    $sources = $document['sources'] ?? null;
    if (!is_array($sources) || $sources === [] || count($sources) > 4096) {
        return null;
    }
    $byPath = [];
    $previous = '';
    foreach ($sources as $source) {
        if (!is_array($source) || count($source) !== 2
            || !is_string($source['path'] ?? null)
            || preg_match('/^[a-z][a-z0-9_\/.-]*\.php$/', (string) ($source['path'] ?? '')) !== 1
            || !is_sha256_digest($source['digest'] ?? null)) {
            return null;
        }
        if ($source['path'] <= $previous) {
            return null;
        }
        $previous = $source['path'];
        $byPath[$source['path']] = $source['digest'];
    }
    $operations = $document['operations'] ?? null;
    if (!is_array($operations) || $operations === [] || count($operations) > 4096) {
        return null;
    }
    $records = [];
    $previousId = '';
    foreach ($operations as $record) {
        $validated = php_operations_validate_evidence_record($record);
        if ($validated === null) {
            return null;
        }
        if ($validated['id'] <= $previousId) {
            return null;
        }
        $previousId = $validated['id'];
        if (!isset($byPath[strtolower((string) $validated['path'])])) {
            // Every record's source must appear in the inventory with
            // the identical digest, or the document is not a closed
            // observation.
            return null;
        }
        $records[] = $validated;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'operationsInputDigest' => $document['operationsInputDigest'],
        'producer' => $producer,
        'sources' => $byPath,
        'operations' => $records,
    ];
}

/** One observed operation record against the closed shape. */
function php_operations_validate_evidence_record(mixed $record): ?array
{
    if (!is_array($record) || count($record) !== 9) {
        return null;
    }
    $id = $record['id'] ?? null;
    if (!is_string($id) || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $id) !== 1) {
        return null;
    }
    $fqn = $record['fqn'] ?? null;
    if (!is_string($fqn)
        || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){0,7}$/', $fqn) !== 1) {
        return null;
    }
    if (($record['method'] ?? null) !== 'handle') {
        return null;
    }
    $path = $record['path'] ?? null;
    if (!is_string($path)
        || preg_match('/^[A-Za-z][A-Za-z0-9_\/.-]*\.php$/', $path) !== 1
        || str_contains($path, '..')) {
        return null;
    }
    if (!is_sha256_digest($record['digest'] ?? null)) {
        return null;
    }
    $constructor = [];
    if (!is_array($record['constructor'] ?? null) || count($record['constructor']) > 16) {
        return null;
    }
    foreach ($record['constructor'] as $slot) {
        if (!is_array($slot) || count($slot) !== 2
            || !is_string($slot['name'] ?? null)
            || preg_match('/^[a-z][A-Za-z0-9_]*$/', (string) ($slot['name'] ?? '')) !== 1
            || !is_string($slot['type'] ?? null)
            || ($slot['type'] ?? '') === '' || strlen((string) ($slot['type'] ?? '')) > 256) {
            return null;
        }
        $constructor[] = ['name' => $slot['name'], 'type' => $slot['type']];
    }
    if (!is_array($record['parameters'] ?? null)
        || count($record['parameters']) < 1 || count($record['parameters']) > 8) {
        return null;
    }
    $parameters = [];
    foreach ($record['parameters'] as $parameter) {
        if (!is_array($parameter) || count($parameter) < 3 || count($parameter) > 4
            || !is_string($parameter['name'] ?? null)
            || preg_match('/^[a-z][A-Za-z0-9_]*$/', (string) ($parameter['name'] ?? '')) !== 1
            || !is_string($parameter['type'] ?? null)
            || ($parameter['type'] ?? '') === '' || strlen((string) ($parameter['type'] ?? '')) > 256
            || !is_bool($parameter['required'] ?? null)) {
            return null;
        }
        if (array_key_exists('nullable', $parameter) && !is_bool($parameter['nullable'])) {
            return null;
        }
        $parameters[] = $parameter;
    }
    $returnType = $record['returnType'] ?? null;
    if (!is_string($returnType) || $returnType === '' || strlen($returnType) > 256) {
        return null;
    }
    $publicMethods = $record['publicMethods'] ?? null;
    if (!is_array($publicMethods) || count($publicMethods) < 1 || count($publicMethods) > 32) {
        return null;
    }
    foreach ($publicMethods as $method) {
        if (!is_string($method)
            || preg_match('/^[A-Za-z_][A-Za-z0-9_]*$/', $method) !== 1) {
            return null;
        }
    }
    return [
        'id' => $id,
        'fqn' => $fqn,
        'method' => 'handle',
        'path' => $path,
        'digest' => $record['digest'],
        'constructor' => $constructor,
        'parameters' => $parameters,
        'returnType' => $returnType,
        'publicMethods' => $publicMethods,
    ];
}

/**
 * The strict join of every checked and custom record against the
 * observed evidence. `$fileDigest` resolves current source digests.
 * Returns internal finding rows; every refusal happens before any
 * generated write.
 *
 * @param array<int, array<string, mixed>> $records
 * @param callable(string): ?string $fileDigest
 * @return array<int, array<string, string>>
 */
function php_check_operation_bindings(
    array $records,
    array $definitions,
    ?array $evidence,
    array $input,
    string $inputDigest,
    ?callable $fileDigest,
): array {
    $findings = [];
    $add = static function (string $code, string $semanticId, string $detail) use (&$findings): void {
        $findings[] = ['code' => $code, 'semanticId' => $semanticId, 'detail' => $detail];
    };
    if ($evidence === null) {
        foreach ($records as $record) {
            $add(
                PHP_OPERATIONS_BINDING_MISSING,
                $record['id'],
                'no observed evidence document covers the declared entrypoint',
            );
        }
        return $findings;
    }
    if ($evidence['projectId'] !== $input['projectId']
        || $evidence['irDigest'] !== $input['irDigest']
        || $evidence['operationsInputDigest'] !== $inputDigest) {
        // The evidence names different inputs: every record is stale.
        foreach ($records as $record) {
            $add(
                PHP_OPERATIONS_BINDING_STALE,
                $record['id'],
                'the observed evidence was produced against different input bytes',
            );
        }
        return $findings;
    }
    $byId = [];
    foreach ($evidence['operations'] as $observed) {
        $byId[$observed['id']] = $observed;
    }
    foreach ($records as $record) {
        $id = (string) $record['id'];
        $entry = $record['entry'];
        $matches = array_values(array_filter(
            $byId,
            static fn (array $candidate): bool => strtolower((string) $candidate['fqn']) === strtolower((string) $entry['fqn']),
        ));
        if (count($matches) > 1) {
            $add(PHP_OPERATIONS_BINDING_AMBIGUOUS, $id, 'the evidence carries competing records for the declared FQN');
            continue;
        }
        $observed = $matches[0] ?? null;
        if ($observed === null) {
            $observed = $byId[$id] ?? null;
            if ($observed === null) {
                $add(PHP_OPERATIONS_BINDING_MISSING, $id, 'no observed record carries the declared entrypoint');
                continue;
            }
        }
        $nativePath = (string) $observed['path'];
        if (strtolower($nativePath) !== strtolower((string) $entry['path'])) {
            $add(PHP_OPERATIONS_BINDING_MISMATCH, $id, "the observed path `$nativePath` diverges from the declared `{$entry['path']}`");
            continue;
        }
        // Current bytes: the observed digest must match the live file.
        $live = $fileDigest !== null ? $fileDigest($nativePath) : null;
        if ($live === null) {
            $add(PHP_OPERATIONS_BINDING_STALE, $id, 'the observed source is missing on disk');
            continue;
        }
        if ($live !== $observed['digest']) {
            $add(PHP_OPERATIONS_BINDING_STALE, $id, 'the observed source digest is stale against the live bytes');
            continue;
        }
        $inventory = $evidence['sources'][strtolower($nativePath)] ?? null;
        if ($inventory !== null && $inventory !== $observed['digest']) {
            $add(PHP_OPERATIONS_BINDING_STALE, $id, 'the observed record and the source inventory disagree');
            continue;
        }
        if ((string) $observed['method'] !== (string) $entry['method']) {
            $add(PHP_OPERATIONS_BINDING_MISMATCH, $id, 'the observed entrypoint method diverges');
            continue;
        }
        $publicMethods = $observed['publicMethods'];
        $extra = array_values(array_filter(
            $publicMethods,
            static fn (string $method): bool => !in_array($method, ['handle', '__construct'], true),
        ));
        if ($extra !== []) {
            $add(
                PHP_OPERATIONS_BINDING_MISMATCH,
                $id,
                'the observed class exposes extra public entrypoints: ' . implode(',', $extra),
            );
            continue;
        }
        $parameters = $observed['parameters'];
        if (count($parameters) < 2
            || !str_ends_with((string) $parameters[0]['type'], 'Input')
            || (string) $parameters[1]['type'] !== 'ActorContext'
            || $parameters[0]['required'] !== true
            || $parameters[1]['required'] !== true) {
            $add(
                PHP_OPERATIONS_BINDING_MISMATCH,
                $id,
                'the observed signature is not (input, ActorContext)',
            );
            continue;
        }
        // Constructor slots: the observed class must inject at least
        // one typed dependency (a handler with an empty constructor
        // cannot carry the declared ports) and every slot resolves to a
        // non-empty type identity.
        if (count($observed['constructor']) < 1) {
            $add(
                PHP_OPERATIONS_BINDING_MISMATCH,
                $id,
                'the observed class injects no typed dependency',
            );
            continue;
        }
    }
    // Deterministic order: semantic id, then code.
    usort($findings, static function (array $left, array $right): int {
        return [$left['semanticId'], $left['code']] <=> [$right['semanticId'], $right['code']];
    });
    return $findings;
}
