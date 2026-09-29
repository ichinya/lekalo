<?php
/**
 * The operations mapper (issue #59): the validated input plus the
 * compiled IR evidence plus the #58 type-map naming rules become one
 * deterministic operations map — signatures, injected ports, role FQNs
 * and the artifact inventory — before any byte is emitted. Reuses the
 * type-map naming and codec helpers; never re-derives type spellings.
 *
 * Unsupported projections become bounded wire findings with ZERO writes,
 * exactly like the type mapper.
 */

/**
 * Map the validated operations input. `$context` is
 * `{input, ir, definitions, typesPrefix, typesIndex, adapterVersion, irDigest, inputDigest, typesInputDigest, root, namespacePrefix}`.
 * `definitions` is the id-indexed parsed IR; `typesIndex` maps semantic
 * ids to `{fqn, path, codec}` from `php_map_types`.
 */
function php_map_operations(array $context): array
{
    $findings = [];
    $addFinding = static function (string $code, string $semanticId, string $detail) use (&$findings): void {
        $findings[] = ['code' => $code, 'semanticId' => $semanticId, 'detail' => $detail];
    };
    $input = $context['input'];
    $definitions = $context['definitions'];
    $prefix = $context['namespacePrefix'];
    $scaffold = $context['root'] === PHP_OPERATIONS_SCAFFOLD_ROOT;
    $operations = [];
    $files = [];
    /** @var array<string, true> $claimedFqns */
    $claimedFqns = [];
    /** @var array<string, true> $claimedPaths */
    $claimedPaths = [];
    $anyTransaction = false;
    $anyDomainError = false;

    $claim = static function (string $fqn, string $path, string $semanticId) use (&$claimedFqns, &$claimedPaths, $addFinding): bool {
        $fqnKey = strtolower($fqn);
        $pathKey = strtolower($path);
        if (isset($claimedFqns[$fqnKey]) || isset($claimedPaths[$pathKey])) {
            $addFinding('operations.naming-collision', $semanticId, "the name `$fqn` or path `$path` is claimed twice");
            return false;
        }
        $claimedFqns[$fqnKey] = true;
        $claimedPaths[$pathKey] = true;
        return true;
    };

    foreach ($input['operations'] as $record) {
        $mapped = php_operations_map_record($record, $context, $addFinding);
        if ($mapped === null) {
            continue;
        }
        $anyTransaction = $anyTransaction || ($mapped['transaction'] === 'required');
        $anyDomainError = $anyDomainError || ($mapped['errors'] !== []);
        foreach ($mapped['artifacts'] as $artifact) {
            if ($artifact['role'] === 'declared') {
                continue;
            }
            $claim($artifact['fqn'] ?? $artifact['path'], $artifact['path'], $mapped['id']);
        }
        $operations[] = $mapped;
    }
    if ($findings !== []) {
        return ['state' => 'unsupported', 'findings' => php_operations_sort_findings($findings)];
    }

    // The typed error classes: one artifact per unique class, owned by
    // the first sorted declaring operation, emitted once per root.
    $lifecycle = $scaffold ? 'scaffolded' : 'generated';
    $seenErrors = [];
    foreach ($operations as $mapped) {
        foreach ($mapped['errors'] as $error) {
            $key = strtolower((string) $error['fqn']);
            if (isset($seenErrors[$key])) {
                continue;
            }
            $seenErrors[$key] = true;
            $path = $context['root'] . '/' . php_types_snake_of(substr((string) $error['fqn'], strlen($context['namespacePrefix']) + 1)) . '.php';
            if (!$claim((string) $error['fqn'], $path, $mapped['id'])) {
                continue;
            }
            $files[] = [
                'path' => $path,
                'role' => 'errors',
                'fqn' => $error['fqn'],
                'lifecycle' => $lifecycle,
                'operation' => $mapped['id'],
            ];
        }
    }

    // Shared artifacts: deterministic, emitted once, only when used.
    $scaffold = $context['root'] === PHP_OPERATIONS_SCAFFOLD_ROOT;
    $lifecycle = $scaffold ? 'scaffolded' : 'generated';
    $addShared = static function (string $class, string $text, string $role) use ($context, $lifecycle, &$files, $claim): void {
        $fqn = $context['namespacePrefix'] . '\\' . $class;
        $path = php_types_snake_of($class) . '.php';
        if (!$claim($fqn, $path, 'php-operations')) {
            return;
        }
        $files[] = [
            'path' => $context['root'] . '/' . $path,
            'text' => $text,
            'digest' => 'sha256:' . hash('sha256', $text),
            'role' => $role,
            'fqn' => $fqn,
            'lifecycle' => $lifecycle,
        ];
    };
    if ($anyDomainError) {
        $addShared('OperationError', php_operations_operation_error_text($context), 'shared');
    }
    if ($anyTransaction) {
        $addShared('TransactionPort', php_operations_transaction_port_text($context), 'shared');
    }
    $addShared('ActorContext', php_operations_actor_context_text($context), 'shared');

    usort($operations, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));
    usort($files, static fn (array $left, array $right): int => strcmp($left['path'], $right['path']));
    return [
        'state' => 'mapped',
        'findings' => [],
        'operations' => $operations,
        'files' => $files,
    ];
}

/** Sort findings deterministically by semantic id, then code. */
function php_operations_sort_findings(array $findings): array
{
    usort($findings, static function (array $left, array $right): int {
        return [$left['semanticId'], $left['code']] <=> [$right['semanticId'], $right['code']];
    });
    return $findings;
}

/**
 * Map one operation record: naming, roles, ports and per-operation
 * artifacts. Returns null after recording a finding for any unsupported
 * projection.
 *
 * @param callable(string, string, string): void $addFinding
 * @return array<string, mixed>|null
 */
function php_operations_map_record(array $record, array $context, callable $addFinding): ?array
{
    $id = $record['id'];
    $kind = $record['kind'];
    $mode = $record['mode'];
    $definitions = $context['definitions'];
    $prefix = $context['namespacePrefix'];
    $module = php_types_module_of($id);
    if (in_array(strtolower($module), ['errors', 'optional'], true)) {
        $addFinding('operations.module-reserved', $id, 'module namespace segment collides with the emitted Errors/Optional namespace');
        return null;
    }
    $leaf = php_types_leaf_name_of($id);
    $stem = php_types_stem_of($id, 'plain');
    $moduleSegment = ucfirst($module);
    $handlerClass = $stem . 'Handler';
    $handlerFqn = $prefix . '\\' . $moduleSegment . '\\' . $handlerClass;
    $handlerPath = strtolower($module) . '/' . strtolower($leaf) . '/handler.php';
    $entry = ['fqn' => $handlerFqn, 'method' => 'handle', 'path' => $context['root'] . '/' . $handlerPath];

    $inputRole = null;
    $resultRole = null;
    $ports = [];
    $body = null;

    // The input role: commands reuse the #58 command input class;
    // queries get an explicit emitted empty-input DTO.
    if ($kind === 'command') {
        $inputFqn = $context['typesIndex'][$id]['fqn'] ?? null;
        if (!is_string($inputFqn)) {
            $addFinding('operations.type-unresolved', $id, 'the command input type is not part of the mapped type inventory');
            return null;
        }
        $inputRole = ['fqn' => $inputFqn];
    } else {
        $inputClass = $stem . 'Input';
        $inputFqn = $prefix . '\\' . $moduleSegment . '\\' . $inputClass;
        $inputRole = ['fqn' => $inputFqn];
    }

    // The result role: command recipes may declare a ref; queries derive
    // it from the IR returns through the type map.
    $recipe = $record['recipe'];
    $returnsRef = $definitions[$id]['returns'] ?? null;
    if ($kind === 'query') {
        if (!is_array($returnsRef) || !isset($returnsRef['ref'])) {
            $addFinding('operations.recipe-unsupported', $id, 'a managed query needs a scalar-ref returns declaration in v0.4.0');
            return null;
        }
        $resultFqn = $context['typesIndex'][(string) $returnsRef['ref']]['fqn'] ?? null;
        if (!is_string($resultFqn)) {
            $addFinding('operations.type-unresolved', $id, 'the query return type is not part of the mapped type inventory');
            return null;
        }
        $resultRole = ['fqn' => $resultFqn];
    } elseif ($recipe['kind'] === 'port-delegation' && $recipe['result'] !== null) {
        $resultFqn = $context['typesIndex'][$recipe['result']]['fqn'] ?? null;
        if (!is_string($resultFqn)) {
            $addFinding('operations.type-unresolved', $id, 'the declared result type is not part of the mapped type inventory');
            return null;
        }
        $resultRole = ['fqn' => $resultFqn];
    }

    $policyFqn = null;
    if ($kind === 'query' && ($recipe['kind'] ?? '') === 'single-entity-update') {
        // Capability honesty: reads stay reads. The finding vetoes the
        // whole run; no write plan can label a query writable.
        $addFinding('operations.query-write', $id, 'a query can never carry a write recipe; reads stay reads');
        return null;
    }
    if ($record['policyId'] !== null) {
        $policyClass = $stem . 'Policy';
        $policyFqn = $prefix . '\\' . $moduleSegment . '\\' . $policyClass;
        $ports[] = ['name' => $policyClass, 'fqn' => $policyFqn, 'role' => 'policy', 'slot' => php_operations_slot_of($policyClass)];
    }
    $eventsFqn = null;
    $emissions = $recipe['emit'] ?? [];
    if ($emissions !== []) {
        $eventsClass = $stem . 'Events';
        $eventsFqn = $prefix . '\\' . $moduleSegment . '\\' . $eventsClass;
        $ports[] = ['name' => $eventsClass, 'fqn' => $eventsFqn, 'role' => 'events', 'slot' => php_operations_slot_of($eventsClass)];
    }
    $repository = null;
    if (($recipe['kind'] ?? '') === 'single-entity-update') {
        $entityId = (string) $recipe['entity'];
        $entityEntry = $context['typesIndex'][$entityId] ?? null;
        if (!is_array($entityEntry)) {
            $addFinding('operations.type-unresolved', $id, 'the updated entity is not part of the mapped type inventory');
            return null;
        }
        $entityModule = php_types_module_of($entityId);
        $entityStem = php_types_stem_of($entityId, 'plain');
        $repositoryClass = $entityStem . 'Repository';
        $repositoryFqn = $prefix . '\\' . ucfirst($entityModule) . '\\' . $repositoryClass;
        $repositoryPath = strtolower($entityModule) . '/' . php_types_snake_of($repositoryClass) . '.php';
        $repository = [
            'class' => $repositoryClass,
            'fqn' => $repositoryFqn,
            'path' => $repositoryPath,
            'entity' => $entityId,
            'entityFqn' => $entityEntry['fqn'],
            'key' => (string) $recipe['key'],
            'identity' => (string) ($definitions[$entityId]['identity'][0] ?? ''),
        ];
        $ports[] = ['name' => $repositoryClass, 'fqn' => $repositoryFqn, 'role' => 'repository', 'slot' => php_operations_slot_of($repositoryClass)];
    }
    $delegate = null;
    if (($recipe['kind'] ?? '') === 'port-delegation') {
        $delegate = [
            'class' => (string) $recipe['port'],
            'fqn' => $prefix . '\\' . $moduleSegment . '\\' . (string) $recipe['port'],
            'method' => (string) $recipe['method'],
        ];
        $ports[] = ['name' => $delegate['class'], 'fqn' => $delegate['fqn'], 'role' => 'delegation', 'slot' => php_operations_slot_of($delegate['class'])];
    }
    if ($record['transaction'] === 'required') {
        $ports[] = ['name' => 'TransactionPort', 'fqn' => $prefix . '\\TransactionPort', 'role' => 'transactions', 'slot' => 'transactions'];
    }
    usort($ports, static fn (array $left, array $right): int => strcmp($left['name'], $right['name']));

    // Domain error ROLE rows: one per declared error id. The artifact
    // rows are claimed once per unique class by the caller, because the
    // #62 binding sets of sibling operations overlap by design.
    $errors = [];
    foreach ($record['errors'] as $errorId) {
        $errorModule = php_types_module_of($errorId);
        if (in_array(strtolower($errorModule), ['errors', 'optional'], true)) {
            $addFinding('operations.module-reserved', $id, "the error module segment of `$errorId` collides with the Errors namespace");
            return null;
        }
        $errorLeaf = php_types_leaf_name_of($errorId);
        $class = ucfirst(php_operations_pascal_of($errorLeaf)) . 'Error';
        $fqn = $prefix . '\\' . ucfirst($errorModule) . '\\Errors\\' . $class;
        $errors[] = ['id' => $errorId, 'fqn' => $fqn, 'class' => $class];
    }
    usort($errors, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));

    $scaffold = $context['root'] === PHP_OPERATIONS_SCAFFOLD_ROOT;
    $lifecycle = $scaffold ? 'scaffolded' : 'generated';
    $artifacts = [];
    if ($mode === 'managed' || $mode === 'scaffold-once') {
        $artifacts[] = [
            'path' => $entry['path'],
            'fqn' => $handlerFqn,
            'role' => 'handler',
            'lifecycle' => $lifecycle,
        ];
        if ($kind === 'query') {
            $inputPath = strtolower($module) . '/' . strtolower($leaf) . '/input.php';
            $artifacts[] = [
                'path' => $context['root'] . '/' . $inputPath,
                'fqn' => $inputFqn,
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($policyFqn !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . strtolower($module) . '/' . strtolower($leaf) . '/policy.php',
                'fqn' => $policyFqn,
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($eventsFqn !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . strtolower($module) . '/' . strtolower($leaf) . '/events.php',
                'fqn' => $eventsFqn,
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($delegate !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . strtolower($module) . '/' . strtolower($leaf) . '/delegate.php',
                'fqn' => $delegate['fqn'],
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($repository !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . $repository['path'],
                'fqn' => $repository['fqn'],
                'role' => 'repository',
                'lifecycle' => $lifecycle,
            ];
        }
    } else {
        // checked/custom: the declared entrypoint is inventoried, never
        // written.
        $declared = $record['entry'];
        $artifacts[] = [
            'path' => strtolower((string) $declared['path']),
            'fqn' => (string) $declared['fqn'],
            'role' => 'declared',
            'lifecycle' => 'checked',
        ];
        $entry = ['fqn' => (string) $declared['fqn'], 'method' => (string) $declared['method'], 'path' => $declared['path']];
    }

    return [
        'id' => $id,
        'kind' => $kind,
        'mode' => $mode,
        'recipe' => $recipe,
        'recipeKind' => is_array($recipe) ? (string) $recipe['kind'] : 'maintained',
        'entry' => $entry,
        'input' => $inputRole,
        'result' => $resultRole,
        'errors' => $errors,
        'ports' => $ports,
        'policyId' => $record['policyId'],
        'transaction' => $record['transaction'],
        'effects' => php_operations_declared_effects($record, $definitions),
        'repository' => $repository,
        'delegate' => $delegate,
        'artifacts' => $artifacts,
    ];
}

/** The injected property slot of one port class name. */
function php_operations_slot_of(string $class): string
{
    return lcfirst($class);
}

/** The Pascal spelling of one snake identifier. */
function php_operations_pascal_of(string $spelling): string
{
    return implode('', array_map(
        static fn (string $part): string => ucfirst($part),
        explode('_', $spelling),
    ));
}

/** The declared IR effect ids of one command record, sorted. */
function php_operations_declared_effects(array $record, array $definitions): array
{
    if ($record['kind'] !== 'command') {
        return [];
    }
    $effects = $definitions[$record['id']]['effects'] ?? [];
    $effects = array_values(array_filter($effects, 'is_string'));
    sort($effects);
    return $effects;
}
