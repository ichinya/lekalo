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
    // The semantic join (issue #59): the mirror of the core join's
    // operation-level reconciliation. A record naming a non-definition,
    // a policy that does not apply, an unresolvable effect, an
    // uncovered recipe, or a write recipe without the required
    // transaction binding is a typed finding here — never a silent
    // emission and never a kernel crash.
    if (!php_operations_semantic_join($record, $definitions, $addFinding)) {
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
    // it from the IR returns through the type map — a scalar ref maps to
    // the nominal type, a list ref maps to the mapped collection class
    // of its element entity (issue #50: the planning day lists).
    $recipe = $record['recipe'];
    $returnsRef = $definitions[$id]['returns'] ?? null;
    if ($kind === 'query') {
        if (is_array($returnsRef) && isset($returnsRef['list']['ref'])) {
            $elementRef = $returnsRef['list']['ref'];
            $collection = null;
            foreach ((array) ($context['collections'] ?? []) as $candidate) {
                if (is_array($candidate)
                    && ($candidate['element'] ?? null) === $elementRef
                    && ($candidate['nullableElements'] ?? false) === false) {
                    $collection = $candidate;
                    break;
                }
            }
            if ($collection === null || !is_string($collection['fqn'] ?? null)) {
                $addFinding('operations.type-unresolved', $id, 'the query list element type has no mapped collection class');
                return null;
            }
            $resultRole = ['fqn' => (string) $collection['fqn']];
        } else {
            if (!is_array($returnsRef) || !isset($returnsRef['ref'])) {
                $addFinding('operations.recipe-unsupported', $id, 'a managed query needs a scalar-ref or list-ref returns declaration in v0.4.0');
                return null;
            }
            $resultFqn = $context['typesIndex'][(string) $returnsRef['ref']]['fqn'] ?? null;
            if (!is_string($resultFqn)) {
                $addFinding('operations.type-unresolved', $id, 'the query return type is not part of the mapped type inventory');
                return null;
            }
            $resultRole = ['fqn' => $resultFqn];
        }
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


// ---------------------------------------------------------------------------
// The semantic join (issue #59): the adapter mirror of the core join's
// operation-level reconciliation over the compiled IR definitions. Both
// authorities agree on the closed vocabulary; a finding here vetoes the
// whole run exactly like a core finding.
// ---------------------------------------------------------------------------

/** The canonical spelling of one closed IR type expression. */
function php_operations_type_spelling(mixed $typeExpr): string
{
    if (!is_array($typeExpr)) {
        return '';
    }
    if (isset($typeExpr['ref']) && is_string($typeExpr['ref'])) {
        return $typeExpr['ref'];
    }
    if (isset($typeExpr['optional'])) {
        return php_operations_type_spelling($typeExpr['optional']) . '?';
    }
    if (isset($typeExpr['list'])) {
        return 'list<' . php_operations_type_spelling($typeExpr['list']) . '>';
    }
    return '';
}

/** The field-index map (`name => type expr`) of one structured definition. */
function php_operations_field_index(array $definition): array
{
    $fields = [];
    foreach ([['fields'], ['input'], ['payload']] as $members) {
        $candidate = $definition;
        foreach ($members as $member) {
            $candidate = $candidate[$member] ?? null;
        }
        if (is_array($candidate)) {
            foreach ($candidate as $field) {
                if (is_array($field) && isset($field['name']) && is_string($field['name'])) {
                    $fields[$field['name']] = $field['type'] ?? null;
                }
            }
            break;
        }
    }
    return $fields;
}

/**
 * The full semantic join of one record. Returns false after recording
 * at least one typed finding; true means the record reconciles with the
 * compiled IR.
 *
 * @param callable(string, string, string): void $addFinding
 */
function php_operations_semantic_join(array $record, array $definitions, callable $addFinding): bool
{
    $id = (string) $record['id'];
    $ok = true;
    $note = static function (string $code, string $detail) use ($addFinding, &$ok, $id): void {
        $ok = false;
        $addFinding($code, $id, $detail);
    };
    $definition = $definitions[$id] ?? null;
    if (!is_array($definition) || ($definition['kind'] ?? null) !== $record['kind']) {
        $note(
            'operations.operation-unresolved',
            "the operation id `$id` is not a compiled IR definition of kind `{$record['kind']}`",
        );
        return false;
    }
    // The transaction binding: a write recipe requires exactly one bound
    // transaction (the body runs inside the TransactionPort run); a
    // query never opens one.
    if ($record['kind'] === 'query' && $record['transaction'] === 'required') {
        $note('operations.transaction-unsupported', 'a query never opens a transaction');
    }
    $recipe = $record['recipe'];
    if (is_array($recipe) && ($recipe['kind'] ?? '') === 'single-entity-update') {
        if ($record['kind'] === 'query') {
            $note('operations.query-write', 'a query can never carry a write recipe; reads stay reads');
            return false;
        }
        if ($record['transaction'] !== 'required') {
            $note(
                'operations.transaction-required',
                'a write recipe requires the required transaction binding: the body runs inside the TransactionPort',
            );
        }
    }
    if (is_array($recipe) && ($recipe['kind'] ?? '') === 'port-delegation'
        && $record['kind'] === 'query' && $recipe['result'] !== null) {
        $note(
            'operations.query-write',
            'a query derives its result from the IR returns; a declared recipe result is redundant',
        );
    }
    // The policy binding: an IR policy definition whose applies_to
    // names this operation.
    if ($record['policyId'] !== null) {
        $policyId = (string) $record['policyId'];
        $policy = $definitions[$policyId] ?? null;
        $applies = is_array($policy)
            && ($policy['kind'] ?? null) === 'policy'
            && in_array($id, is_array($policy['applies_to'] ?? null) ? $policy['applies_to'] : [], true);
        if (!$applies) {
            $note(
                'operations.policy-unresolved',
                "the policy `$policyId` is not a compiled policy applying to this operation",
            );
        }
    }
    // The declared errors: the #62 registry binding equality is the
    // core join's authority (the embedded registry is not staged for
    // the adapter), so the mirror checks what the IR admits: the
    // recipe's failure references must be declared errors.
    if (is_array($recipe) && ($recipe['kind'] ?? '') === 'single-entity-update') {
        php_operations_join_update_recipe($record, $recipe, $definitions, $note);
    }
    return $ok;
}

/**
 * The single-entity-update reconciliation: entity, key typing, full
 * field coverage, typed operands, declared failure references, and the
 * effect-resolved event emissions.
 *
 * @param callable(string, string): void $note
 */
function php_operations_join_update_recipe(array $record, array $recipe, array $definitions, callable $note): void
{
    $id = (string) $record['id'];
    $entityId = (string) $recipe['entity'];
    $entity = $definitions[$entityId] ?? null;
    if (!is_array($entity) || ($entity['kind'] ?? null) !== 'entity') {
        $note('operations.entity-unresolved', "the updated definition `$entityId` is not a compiled entity");
        return;
    }
    $identity = $entity['identity'] ?? [];
    if (!is_array($identity) || count($identity) !== 1) {
        $note('operations.recipe-unsupported', 'v0.4.0 updates only single-identity entities');
        return;
    }
    $entityFields = php_operations_field_index($entity);
    $identityField = (string) $identity[0];
    $inputFields = php_operations_field_index($definitions[$id] ?? []);
    // The key operand types.
    $keyType = $inputFields[(string) $recipe['key']] ?? null;
    $identityType = $entityFields[$identityField] ?? null;
    if ($keyType === null) {
        $note('operations.recipe-coverage', 'the key `' . (string) $recipe['key'] . '` is not a command input field');
    }
    if ($keyType !== null && $identityType !== null
        && php_operations_type_spelling($keyType) !== php_operations_type_spelling($identityType)) {
        $note(
            'operations.type-mismatch',
            'the key input field type `' . php_operations_type_spelling($keyType)
            . '` must equal the identity type `' . php_operations_type_spelling($identityType) . '`',
        );
    }
    // Coverage: every non-identity entity field exactly once, either
    // assigned or kept; the identity carries over.
    $covered = [];
    foreach ($recipe['assignments'] as $assignment) {
        $field = (string) $assignment['field'];
        if (isset($covered[$field])) {
            $note('operations.recipe-coverage', "the field `$field` is covered twice");
            continue;
        }
        $covered[$field] = true;
        if (!isset($entityFields[$field])) {
            $note('operations.recipe-coverage', "the assigned field `$field` is not an entity field");
            continue;
        }
        php_operations_join_operand(
            $assignment['value'],
            $entityFields[$field],
            $inputFields,
            $entityFields,
            $definitions,
            $note,
        );
    }
    foreach ($recipe['kept'] as $field) {
        $field = (string) $field;
        if (isset($covered[$field])) {
            $note('operations.recipe-coverage', "the field `$field` is covered twice");
            continue;
        }
        $covered[$field] = true;
        if (!isset($entityFields[$field])) {
            $note('operations.recipe-coverage', "the kept field `$field` is not an entity field");
        }
    }
    foreach (array_keys($entityFields) as $field) {
        if ($field === $identityField) {
            continue;
        }
        if (!isset($covered[$field])) {
            $note('operations.recipe-coverage', "the entity field `$field` is neither assigned nor kept");
        }
    }
    // Preconditions: entity fields, typed operands, declared errors.
    foreach ($recipe['preconditions'] as $precondition) {
        $field = (string) $precondition['field'];
        if (!isset($entityFields[$field])) {
            $note('operations.recipe-coverage', "the precondition field `$field` is not an entity field");
        } else {
            php_operations_join_operand(
                $precondition['equals'],
                $entityFields[$field],
                $inputFields,
                $entityFields,
                $definitions,
                $note,
            );
        }
        if (!in_array((string) $precondition['error'], $record['errors'], true)) {
            $note(
                'operations.registry-binding',
                'the precondition error `' . (string) $precondition['error'] . '` is not a declared error of this operation',
            );
        }
    }
    if (!in_array((string) $recipe['missingBehavior']['error'], $record['errors'], true)) {
        $note(
            'operations.registry-binding',
            'the missing-record error `' . (string) $recipe['missingBehavior']['error'] . '` is not a declared error of this operation',
        );
    }
    // Emissions: the event must be an IR event emitted by one of the
    // command's effects, and every payload field must carry a typed
    // operand exactly once.
    $emittedEvents = [];
    foreach ($definitions[$id]['effects'] ?? [] as $effectId) {
        $effect = $definitions[(string) $effectId] ?? null;
        if (is_array($effect) && ($effect['kind'] ?? null) === 'effect') {
            foreach ($effect['emits'] ?? [] as $emitted) {
                $emittedEvents[] = (string) $emitted;
            }
        }
    }
    foreach ($recipe['emit'] as $emission) {
        $eventId = (string) $emission['event'];
        if (!in_array($eventId, $emittedEvents, true)) {
            $note(
                'operations.effect-unresolved',
                "the event `$eventId` is not emitted by any effect of this command",
            );
        }
        $event = $definitions[$eventId] ?? null;
        if (!is_array($event) || ($event['kind'] ?? null) !== 'event') {
            $note('operations.type-unresolved', "the definition `$eventId` is not a compiled event");
            continue;
        }
        $eventFields = php_operations_field_index($event);
        foreach (array_keys($eventFields) as $field) {
            if (!isset($emission['payload'][$field])) {
                $note('operations.recipe-coverage', "the event field `$field` has no operand");
            }
        }
        foreach ($emission['payload'] as $field => $operand) {
            if (!isset($eventFields[(string) $field])) {
                $note('operations.recipe-coverage', "the payload operand `$field` is not an event field");
                continue;
            }
            php_operations_join_operand(
                $operand,
                $eventFields[(string) $field],
                $inputFields,
                $entityFields,
                $definitions,
                $note,
            );
        }
    }
}

/**
 * One closed typed operand against one target type expression: exact
 * input/entity field existence and type identity, declared enum cases,
 * and no untyped literal targets.
 *
 * @param array<string, mixed> $inputFields
 * @param array<string, mixed> $entityFields
 * @param callable(string, string): void $note
 */
function php_operations_join_operand(
    array $operand,
    mixed $targetExpr,
    array $inputFields,
    array $entityFields,
    array $definitions,
    callable $note,
): void {
    $target = php_operations_type_spelling($targetExpr);
    if (isset($operand['fromInput'])) {
        $field = (string) $operand['fromInput'];
        if (!isset($inputFields[$field])) {
            $note('operations.type-mismatch', "`$field` is not a command input field");
            return;
        }
        $spelling = php_operations_type_spelling($inputFields[$field]);
        if ($spelling !== $target) {
            $note('operations.type-mismatch', "the input field `$field` carries `$spelling`, the target needs `$target`");
        }
        return;
    }
    if (isset($operand['fromEntity'])) {
        $field = (string) $operand['fromEntity'];
        if (!isset($entityFields[$field])) {
            $note('operations.type-mismatch', "`$field` is not an entity field");
            return;
        }
        $spelling = php_operations_type_spelling($entityFields[$field]);
        if ($spelling !== $target) {
            $note('operations.type-mismatch', "the entity field `$field` carries `$spelling`, the target needs `$target`");
        }
        return;
    }
    if (isset($operand['enumCase'])) {
        $enumId = (string) $operand['enumCase']['type'];
        $value = (string) $operand['enumCase']['value'];
        if ($target !== $enumId) {
            $note('operations.type-mismatch', "the enum case targets `$enumId`, the field carries `$target`");
            return;
        }
        $enum = $definitions[$enumId] ?? null;
        $declared = false;
        if (is_array($enum) && ($enum['kind'] ?? null) === 'enum') {
            foreach ($enum['values'] ?? [] as $candidate) {
                if (is_array($candidate) && (string) $candidate['value'] === $value) {
                    $declared = true;
                    break;
                }
            }
        }
        if (!$declared) {
            $note('operations.type-unresolved', "the enum case value `$value` is not declared by `$enumId`");
        }
        return;
    }
    // A literal operand cannot prove the target definition's shape in
    // v0.4.0: unsupported, exactly like the core join.
    $note('operations.type-mismatch', "a literal operand cannot carry the definition target `$target`");
}
