<?php

/**
 * The routes mapper (issue #60): the validated routes input joined with
 * the staged evidence — the compiled IR, the canonical transport-http
 * attachment, the OpenAPI projection, and the bound operations input —
 * becomes the deterministic route inventory. Pure: no filesystem, no
 * clock, no environment. A record the authorities cannot join is a
 * typed finding, never a guessed route; the whole run refuses before
 * any emission when any finding exists.
 *
 * Method, path, parameters, body, success and error projections,
 * security, headers, and scenario links come verbatim from the IR and
 * the transport attachment. The error envelope members (category, code,
 * public payload members) come from the OpenAPI projection of the same
 * join — the emitted boundary and the published document are one
 * projection by construction, and the digests are pinned in the
 * custody sidecar.
 */

/** The closed HTTP methods the route layer registers. */
const PHP_ROUTES_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

/**
 * Map the routes inventory. `$context` is `{input, definitions,
 * typesIndex, operationsInput, transport, transportDigest, openapi,
 * irDigest, namespacePrefix}`. Returns `{state, findings, routes}`;
 * every route row carries the decode plan, the envelope table, the
 * emitted-class identities, and the exported links.
 */
function php_map_routes(array $context): array
{
    $findings = [];
    $addFinding = static function (string $code, string $semanticId, string $detail) use (&$findings): void {
        $findings[] = ['code' => $code, 'semanticId' => $semanticId, 'detail' => $detail];
    };
    $input = $context['input'];
    $definitions = $context['definitions'];
    $transport = $context['transport'];
    $openapi = $context['openapi'];

    // The evidence joins: the attachment must bind these exact IR bytes,
    // and the OpenAPI projection must be the render of the same join.
    $transportIrDigest = (string) ($transport['irRef']['digest'] ?? '');
    if ($transportIrDigest !== $context['irDigest']) {
        $addFinding('routes.transport-ir-mismatch', 'php-routes', 'the transport attachment binds different IR bytes than the staged evidence');
    }
    if (!is_array($openapi)) {
        $addFinding('routes.openapi-unbound', 'php-routes', 'the staged OpenAPI projection is missing; the routes boundary and the published document must be one projection');
    } else {
        $provenance = is_array($openapi['x-lekalo-provenance'] ?? null) ? $openapi['x-lekalo-provenance'] : [];
        $openapiIr = (string) ($provenance['irRef']['digest'] ?? '');
        if ($openapiIr !== $context['irDigest']) {
            $addFinding('routes.openapi-ir-mismatch', 'php-routes', 'the OpenAPI projection renders different IR bytes than the staged evidence');
        }
        $openapiTransport = (string) ($provenance['transportRef']['digest'] ?? '');
        if ($openapiTransport !== $context['transportDigest']) {
            $addFinding('routes.openapi-transport-mismatch', 'php-routes', 'the OpenAPI projection renders a different transport attachment than the staged evidence');
        }
    }

    // The middleware scheme index (declared in the input policy).
    $middlewareByScheme = [];
    foreach ($input['middleware'] as $binding) {
        $middlewareByScheme[$binding['scheme']] = $binding['middleware'];
    }

    // The operations input index: the handlers the wrappers invoke.
    $operationsById = [];
    foreach (($context['operationsInput']['operations'] ?? []) as $record) {
        $operationsById[(string) $record['id']] = $record;
    }
    $operationsPrefix = (string) ($context['operationsInput']['namespacePrefix'] ?? '');

    // The OpenAPI error-envelope index: one entry per declared error id
    // with its category, LEK-ERR code, and public payload members.
    $errorEnvelopeIndex = is_array($openapi) ? php_routes_error_envelope_index($openapi) : [];

    $routes = [];
    /** @var array<string, true> $claimedFqns */
    $claimedFqns = [];
    /** @var array<string, true> $claimedPaths */
    $claimedPaths = [];
    foreach ($input['routes'] as $record) {
        $mapped = php_routes_map_record(
            $record,
            $context,
            $definitions,
            $transport,
            $operationsById,
            $operationsPrefix,
            $middlewareByScheme,
            $errorEnvelopeIndex,
            $addFinding,
        );
        if ($mapped === null) {
            continue;
        }
        foreach ($mapped['artifacts'] as $artifact) {
            $fqnKey = strtolower((string) $artifact['fqn']);
            $pathKey = strtolower((string) $artifact['path']);
            if (isset($claimedFqns[$fqnKey]) || isset($claimedPaths[$pathKey])) {
                $addFinding('routes.naming-collision', $mapped['id'], 'the name or path of one emitted route class is claimed twice');
                continue 2;
            }
            $claimedFqns[$fqnKey] = true;
            $claimedPaths[$pathKey] = true;
        }
        $routes[] = $mapped;
    }
    if ($findings !== []) {
        return ['state' => 'unsupported', 'findings' => php_routes_sort_findings($findings)];
    }
    return ['state' => 'mapped', 'findings' => [], 'routes' => $routes];
}

/**
 * The OpenAPI error-envelope index: every `components.schemas` entry
 * carrying an `x-lekalo-symbol` error id and the canonical envelope
 * constants contributes `{category, code, payload[]}`. The projection
 * is the authority; an error the projection never rendered cannot be
 * mapped by the boundary.
 */
function php_routes_error_envelope_index(array $openapi): array
{
    $index = [];
    foreach (($openapi['components']['schemas'] ?? []) as $schema) {
        if (!is_array($schema)) {
            continue;
        }
        $symbol = $schema['x-lekalo-symbol'] ?? null;
        $error = is_array($schema['properties']['error']['properties'] ?? null)
            ? $schema['properties']['error']['properties']
            : null;
        if (!is_string($symbol) || $error === null) {
            continue;
        }
        $category = $error['category']['const'] ?? null;
        $code = $error['code']['const'] ?? null;
        $payload = is_array($error['payload']['properties'] ?? null) ? $error['payload']['properties'] : [];
        if (!is_string($category) || !is_string($code)) {
            continue;
        }
        $index[$symbol] = [
            'category' => $category,
            'code' => $code,
            'payload' => array_keys($payload),
        ];
    }
    return $index;
}

/**
 * Map one route record. Returns null after recording a finding for any
 * unjoinable declaration.
 *
 * @param callable(string, string, string): void $addFinding
 * @return array<string, mixed>|null
 */
function php_routes_map_record(
    array $record,
    array $context,
    array $definitions,
    array $transport,
    array $operationsById,
    string $operationsPrefix,
    array $middlewareByScheme,
    array $errorEnvelopeIndex,
    callable $addFinding,
): ?array {
    $id = (string) $record['id'];
    $operation = (string) $record['operation'];
    $mode = (string) $record['mode'];

    // The endpoint authority: the compiled IR.
    $endpoint = is_array($definitions[$id] ?? null) ? $definitions[$id] : null;
    if ($endpoint === null || ($endpoint['kind'] ?? null) !== 'endpoint') {
        $addFinding('routes.endpoint-unresolved', $id, 'the route id is not a compiled IR endpoint definition');
        return null;
    }
    if ((string) ($endpoint['invokes'] ?? '') !== $operation) {
        $addFinding('routes.invokes-mismatch', $id, 'the endpoint invokes a different operation than the record declares');
        return null;
    }
    $operationDefinition = is_array($definitions[$operation] ?? null) ? $definitions[$operation] : null;
    $operationKind = $operationDefinition === null ? '' : (string) ($operationDefinition['kind'] ?? '');
    if ($operationKind !== 'command' && $operationKind !== 'query') {
        $addFinding('routes.operation-unresolved', $id, 'the invoked operation is not a compiled command or query definition');
        return null;
    }
    // Mode/entry pairing.
    if (($mode === 'checked') !== (is_array($record['entry'] ?? null))) {
        $addFinding('routes.entry-required', $id, $mode === 'checked'
            ? 'a checked record declares its existing controller entrypoint'
            : 'a managed record never declares a foreign entrypoint');
        return null;
    }
    // The wire authority: the transport attachment binding.
    $binding = null;
    foreach ((array) ($transport['endpoints'] ?? []) as $candidate) {
        if (is_array($candidate) && ($candidate['endpoint'] ?? null) === $id) {
            $binding = $candidate;
            break;
        }
    }
    if ($binding === null) {
        $addFinding('routes.transport-unbound', $id, 'the transport attachment binds no wire surface for this endpoint');
        return null;
    }
    $method = (string) ($endpoint['method'] ?? '');
    if (!in_array($method, PHP_ROUTES_METHODS, true)) {
        $addFinding('routes.method-unsupported', $id, 'the endpoint method is outside the closed route-layer vocabulary');
        return null;
    }
    $pathTemplate = (string) ($endpoint['path'] ?? '');
    if ($pathTemplate === '' || $pathTemplate[0] !== '/') {
        $addFinding('routes.path-invalid', $id, 'the endpoint path template is not an absolute route template');
        return null;
    }
    // The handler authority: the bound operations input record.
    $operationRecord = $operationsById[$operation] ?? null;
    if ($operationRecord === null) {
        $addFinding('routes.operation-unbound', $id, 'the invoked operation has no record in the bound operations input');
        return null;
    }
    $entry = php_routes_entry_of($operationRecord, $operationsPrefix, $context['typesIndex'], $context['definitions']);
    if ($entry === null) {
        $addFinding('routes.operation-unbound', $id, 'the operation record carries no joinable typed entrypoint');
        return null;
    }

    // Naming: one thin controller per governed route, named by the
    // endpoint stem; the typed request binding exists for
    // body-carrying managed routes.
    $module = php_types_module_of($id);
    $endpointStem = php_types_stem_of($id, 'plain');
    $controllerClass = $endpointStem . 'Controller';
    $controllerFqn = $context['namespacePrefix'] . '\\' . ucfirst($module) . '\\' . $controllerClass;
    $controllerPath = PHP_ROUTES_GENERATED_ROOT . '/' . php_types_path_of($module, $controllerClass);
    $operationId = (string) ($binding['operationId'] ?? php_routes_camel_of($id));
    $request = null;
    if ($mode === 'managed' && (is_array($binding['body'] ?? null) || (array) ($binding['params'] ?? []) !== [])) {
        // The typed request binding exists for every managed route with
        // a declared decode plan: a body projection or a path-parameter
        // binding (issue #50: the bodyless planning commands) — never a
        // guessed empty input.
        $requestClass = $endpointStem . 'Request';
        $request = [
            'fqn' => $context['namespacePrefix'] . '\\' . ucfirst($module) . '\\' . $requestClass,
            'path' => PHP_ROUTES_GENERATED_ROOT . '/' . php_types_path_of($module, $requestClass),
        ];
    }

    // The middleware attach: only an input-declared scheme mapping
    // attaches middleware; the framework default is never guessed.
    $middleware = [];
    foreach ((array) ($binding['auth']['schemes'] ?? []) as $scheme) {
        $spelling = $middlewareByScheme[(string) $scheme] ?? null;
        if ($spelling !== null && !in_array((string) $spelling, $middleware, true)) {
            $middleware[] = (string) $spelling;
        }
    }
    sort($middleware, SORT_STRING);

    // The decode plan: typed path parameters plus the body projection.
    $decode = php_routes_decode_plan($binding, $operationDefinition, $context['typesIndex'], $context['definitions'], $addFinding, $id);
    if ($decode === null) {
        return null;
    }

    // The declared error table with its envelope constants.
    $errors = [];
    foreach ((array) ($binding['errors'] ?? []) as $declared) {
        $errorId = (string) ($declared['error'] ?? '');
        $envelope = $errorEnvelopeIndex[$errorId] ?? null;
        if ($envelope === null) {
            $addFinding('routes.openapi-error-unbound', $id, "the declared error `{$errorId}` has no envelope constants in the OpenAPI projection");
            return null;
        }
        $errors[] = [
            'error' => $errorId,
            'status' => (int) ($declared['status'] ?? 0),
            'category' => $envelope['category'],
            'code' => $envelope['code'],
            'payload' => $envelope['payload'],
            // The validation-category error is produced by the request
            // binding refusal, never by a handler catch: its `field`
            // payload member lives on the refusal.
            'refusal' => $envelope['category'] === 'validation',
        ];
    }
    usort($errors, static fn (array $left, array $right): int => strcmp($left['error'], $right['error']));
    // The validation binding: a body-carrying managed route declares its
    // validation-category error exactly once — the typed refusal of the
    // request binding maps there, never to an invented id.
    $validationError = null;
    if (is_array($binding['body'] ?? null) && $mode === 'managed') {
        foreach ($errors as $error) {
            if ($error['category'] === 'validation') {
                if ($validationError !== null) {
                    $addFinding('routes.validation-ambiguous', $id, 'the route declares more than one validation-category error');
                    return null;
                }
                $validationError = (string) $error['error'];
            }
        }
        if ($validationError === null) {
            $addFinding('routes.validation-unbound', $id, 'a body-carrying route declares no validation-category error; the decode refusal has no declared mapping');
            return null;
        }
    }
    // The payload members of every declared error must resolve: the
    // `field` member of a validation error comes from the request
    // refusal, every other member is a typed operation input member.
    foreach ($errors as $error) {
        foreach ($error['payload'] as $member) {
            if ($member === 'field' && $error['category'] === 'validation') {
                continue;
            }
            if (php_routes_field_type($member, $operationDefinition, $context['typesIndex'], $context['definitions']) === null) {
                $addFinding('routes.payload-unresolved', $id, "the payload member `{$member}` of `{$error['error']}` does not resolve to a typed input member");
                return null;
            }
        }
    }
    $defaultsBinding = is_array($binding['errorDefaults'] ?? null) ? $binding['errorDefaults'] : [];
    $errorDefaults = [];
    foreach (['validation', 'auth', 'conflict', 'not-found', 'domain', 'infrastructure'] as $category) {
        $errorDefaults[$category] = (int) ($defaultsBinding[$category] ?? 0);
    }

    $success = is_array($binding['success'] ?? null) ? $binding['success'] : [];
    $auth = is_array($binding['auth'] ?? null) ? [
        'actor' => (string) $binding['auth']['actor'],
        'schemes' => array_map('strval', (array) ($binding['auth']['schemes'] ?? [])),
        'policyRef' => isset($binding['auth']['policyRef']) ? (string) $binding['auth']['policyRef'] : null,
    ] : null;

    $artifacts = [];
    if ($mode === 'managed') {
        $artifacts[] = ['path' => $controllerPath, 'fqn' => $controllerFqn];
        if ($request !== null) {
            $artifacts[] = ['path' => $request['path'], 'fqn' => $request['fqn']];
        }
    }

    return [
        'id' => $id,
        'operation' => $operation,
        'operationKind' => $operationKind,
        'operationId' => $operationId,
        'mode' => $mode,
        'method' => $method,
        'pathTemplate' => $pathTemplate,
        'name' => $operationId,
        'controller' => ['fqn' => $controllerFqn, 'path' => $controllerPath, 'action' => $operationId],
        'request' => $request,
        'entry' => $entry,
        'middleware' => $middleware,
        'success' => [
            'status' => (int) ($success['status'] ?? 0),
            'bodyMode' => isset($success['body']['mode']) ? (string) $success['body']['mode'] : null,
        ],
        'errors' => $errors,
        'errorDefaults' => $errorDefaults,
        'validationError' => $validationError,
        'auth' => $auth,
        'idempotency' => is_array($binding['idempotency'] ?? null) ? [
            'header' => (string) $binding['idempotency']['header'],
            'required' => (bool) $binding['idempotency']['required'],
        ] : null,
        'correlation' => is_array($binding['correlation'] ?? null) ? [
            'headers' => array_map('strval', (array) ($binding['correlation']['headers'] ?? [])),
        ] : null,
        'scenarios' => array_map('strval', (array) ($binding['scenarios'] ?? [])),
        'decode' => $decode,
        'links' => array_filter([
            'endpoint' => $id,
            'operation' => $operation,
            'controller' => $mode === 'managed' ? $controllerFqn : null,
            'request' => $request['fqn'] ?? null,
            'scenarios' => array_map('strval', (array) ($binding['scenarios'] ?? [])),
            'openapiPointer' => '/paths/' . php_routes_escape_pointer($pathTemplate) . '/' . strtolower($method),
        ], static fn (mixed $value): bool => $value !== null),
        'artifacts' => $artifacts,
    ];
}

/**
 * The invoked operation entrypoint: managed and scaffold operations
 * derive the generated handler spelling; checked and custom operations
 * carry the declared existing entry. The typed input identity joins the
 * #58 inventory (commands) or the operations family namespace
 * (queries).
 *
 * @return array{fqn: string, method: string, inputFqn: string}|null
 */
function php_routes_entry_of(array $operationRecord, string $operationsPrefix, array $typesIndex, array $definitions): ?array
{
    $id = (string) $operationRecord['id'];
    $module = php_types_module_of($id);
    $stem = php_types_stem_of($id, 'plain');
    $inputFqn = php_routes_entry_input_fqn($operationRecord, $operationsPrefix, $typesIndex);
    if ($inputFqn === null) {
        return null;
    }
    $mode = (string) $operationRecord['mode'];
    if (in_array($mode, ['managed', 'scaffold-once'], true)) {
        return [
            'fqn' => $operationsPrefix . '\\' . ucfirst($module) . '\\' . $stem . 'Handler',
            'method' => 'handle',
            'inputFqn' => $inputFqn,
        ];
    }
    $entry = is_array($operationRecord['entry'] ?? null) ? $operationRecord['entry'] : null;
    if ($entry === null) {
        return null;
    }
    return ['fqn' => (string) $entry['fqn'], 'method' => (string) $entry['method'], 'inputFqn' => $inputFqn];
}

/**
 * The typed input FQN of one operation record (sidecar-neutral helper
 * of `php_routes_entry_of`).
 */
function php_routes_entry_input_fqn(array $operationRecord, string $operationsPrefix, array $typesIndex): ?string
{
    $id = (string) $operationRecord['id'];
    $module = php_types_module_of($id);
    $stem = php_types_stem_of($id, 'plain');
    if ((string) $operationRecord['kind'] === 'command') {
        $inputFqn = is_array($typesIndex[$id] ?? null) ? (string) ($typesIndex[$id]['fqn'] ?? '') : '';
        return $inputFqn === '' ? null : $inputFqn;
    }
    return $operationsPrefix . '\\' . ucfirst($module) . '\\' . $stem . 'Input';
}

/**
 * The decode plan of one binding: typed path parameters plus the body
 * mode. The parameter and body field types resolve through the mapped
 * type inventory — an unresolvable field is a finding, never a loose
 * array.
 *
 * @param callable(string, string, string): void $addFinding
 * @return array<string, mixed>|null
 */
function php_routes_decode_plan(array $binding, array $operationDefinition, array $typesIndex, array $definitions, callable $addFinding, string $routeId): ?array
{
    $params = [];
    foreach ((array) ($binding['params'] ?? []) as $param) {
        $type = php_routes_field_type($param['field'] ?? null, $operationDefinition, $typesIndex, $definitions);
        if ($type === null) {
            $addFinding('routes.field-unresolved', $routeId, 'a declared parameter field does not resolve to a typed operation input member');
            return null;
        }
        $params[] = [
            'name' => (string) $param['name'],
            'in' => (string) $param['in'],
            'field' => (string) $param['field'],
            'required' => (bool) ($param['required'] ?? false),
            'type' => $type,
        ];
    }
    $body = null;
    if (is_array($binding['body'] ?? null)) {
        $fields = [];
        foreach ((array) ($binding['body']['fields'] ?? []) as $field) {
            $type = php_routes_field_type($field['field'] ?? null, $operationDefinition, $typesIndex, $definitions);
            if ($type === null) {
                $addFinding('routes.field-unresolved', $routeId, 'a declared body field does not resolve to a typed operation input member');
                return null;
            }
            $fields[] = [
                'name' => (string) $field['name'],
                'field' => (string) $field['field'],
                'required' => (bool) ($field['required'] ?? false),
                'type' => $type,
            ];
        }
        $body = ['mode' => (string) $binding['body']['mode'], 'fields' => $fields];
    }
    return ['params' => $params, 'body' => $body];
}

/**
 * The mapped type identity of one declared field reference
 * (`input.<name>` for command inputs): the FQN rides the #58 mapped
 * inventory, the wire kind rides the compiled IR definition.
 *
 * @return array{ref: string, fqn: string, kind: string, base: string}|null
 */
function php_routes_field_type(mixed $field, array $operationDefinition, array $typesIndex, array $definitions): ?array
{
    if (!is_string($field) || $field === '') {
        return null;
    }
    $member = str_starts_with($field, 'input.') ? substr($field, 6) : $field;
    foreach ((array) ($operationDefinition['input'] ?? []) as $inputField) {
        if (!is_array($inputField) || ($inputField['name'] ?? null) !== $member) {
            continue;
        }
        $ref = $inputField['type']['ref'] ?? null;
        if (!is_string($ref)) {
            return null;
        }
        $entry = is_array($typesIndex[$ref] ?? null) ? $typesIndex[$ref] : null;
        $definition = is_array($definitions[$ref] ?? null) ? $definitions[$ref] : null;
        if ($entry === null || $definition === null) {
            return null;
        }
        return [
            'ref' => $ref,
            'fqn' => (string) $entry['fqn'],
            'kind' => (string) ($definition['kind'] ?? ''),
            'base' => (string) ($definition['base'] ?? ''),
        ];
    }
    return null;
}

/** The deterministic camel spelling of the default operation id. */
function php_routes_camel_of(string $semanticId): string
{
    $parts = preg_split('/[._]/', $semanticId) ?: [];
    $camel = array_shift($parts) ?? '';
    foreach ($parts as $part) {
        $camel .= ucfirst($part);
    }
    return $camel;
}

/** The RFC 6901 pointer escape of one path-template token. */
function php_routes_escape_pointer(string $template): string
{
    return str_replace(['~', '/'], ['~0', '~1'], $template);
}

/** Findings sort deterministically by semantic id, then code. */
function php_routes_sort_findings(array $findings): array
{
    usort($findings, static function (array $left, array $right): int {
        return [$left['semanticId'], $left['code']] <=> [$right['semanticId'], $right['code']];
    });
    return $findings;
}

/**
 * The checked-route join (issue #60): the declared surface of every
 * checked record joins against the observed routes evidence. Absent
 * evidence is a finding for every declared id; a stale controller
 * digest, a foreign method/uri/name/action, or a partial record is a
 * typed finding — never a pass, never a silent rewrite. No write ever
 * results.
 *
 * @param array $records the checked route records of the validated input
 * @param array $mappedRoutes the mapped route rows by endpoint id
 * @param array $definitions the IR evidence definitions by id
 * @param array|null $evidence the parsed observed routes evidence
 * @param array $input the validated routes input
 * @param string $inputDigest the digest of the exact input bytes
 * @param callable(string): ?string $digestAt the source digest probe
 * @return list<array{code: string, semanticId: string, detail: string}>
 */
function php_routes_check_bindings(
    array $records,
    array $mappedRoutes,
    array $definitions,
    ?array $evidence,
    array $input,
    string $inputDigest,
    callable $digestAt,
): array {
    $findings = [];
    if ($evidence === null) {
        foreach ($records as $record) {
            $findings[] = [
                'code' => 'routes.binding-missing',
                'semanticId' => (string) $record['id'],
                'detail' => 'the observed routes evidence is absent; a checked route never passes without scanner evidence',
            ];
        }
        return $findings;
    }
    // The evidence binds these exact IR and input bytes; a stale pin
    // makes every record unknown.
    if ($evidence['irDigest'] !== $input['irDigest'] || $evidence['routesInputDigest'] !== $inputDigest) {
        foreach ($records as $record) {
            $findings[] = [
                'code' => 'routes.binding-stale',
                'semanticId' => (string) $record['id'],
                'detail' => 'the observed routes evidence pins different IR or input bytes',
            ];
        }
        return $findings;
    }
    /** @var array<string, list<array>> $byRoute the observed rows keyed by (method, uri) */
    $byMethodUri = [];
    foreach ($evidence['routes'] as $row) {
        $byMethodUri[$row['method'] . ' ' . $row['uri']][] = $row;
    }
    foreach ($records as $record) {
        $id = (string) $record['id'];
        $mapped = null;
        foreach ($mappedRoutes as $route) {
            if ($route['id'] === $id) {
                $mapped = $route;
                break;
            }
        }
        if ($mapped === null) {
            $findings[] = ['code' => 'routes.binding-missing', 'semanticId' => $id, 'detail' => 'the checked record has no mapped route row'];
            continue;
        }
        $entry = $record['entry'];
        $candidates = $byMethodUri[$mapped['method'] . ' ' . $mapped['pathTemplate']] ?? [];
        if ($candidates === []) {
            $findings[] = [
                'code' => 'routes.binding-missing',
                'semanticId' => $id,
                'detail' => 'no observed route carries the declared method and uri',
            ];
            continue;
        }
        $matches = [];
        foreach ($candidates as $row) {
            $action = $entry['fqn'] . '@' . $entry['method'];
            if ($row['action'] === $action) {
                $matches[] = $row;
            }
        }
        if (count($matches) === 0) {
            $findings[] = [
                'code' => 'routes.binding-mismatch',
                'semanticId' => $id,
                'detail' => 'the observed routes carry the surface but not the declared entrypoint',
            ];
            continue;
        }
        if (count($matches) > 1) {
            $findings[] = [
                'code' => 'routes.binding-ambiguous',
                'semanticId' => $id,
                'detail' => 'more than one observed route carries the declared surface and action',
            ];
            continue;
        }
        $row = $matches[0];
        // Current bytes: the observed digest must match the live file
        // (the probe keeps the evidence's case-preserving native path).
        $digest = $digestAt($row['path']);
        if ($digest === null) {
            $findings[] = [
                'code' => 'routes.binding-missing',
                'semanticId' => $id,
                'detail' => 'the observed controller source is unreadable',
            ];
            continue;
        }
        if ($digest !== $row['digest']) {
            $findings[] = [
                'code' => 'routes.binding-stale',
                'semanticId' => $id,
                'detail' => 'the observed controller bytes diverge from the evidence digest',
            ];
            continue;
        }
        // The declared middleware attach must match the observed one.
        $observed = $row['middleware'];
        sort($observed, SORT_STRING);
        if ($observed !== $mapped['middleware']) {
            $findings[] = [
                'code' => 'routes.binding-mismatch',
                'semanticId' => $id,
                'detail' => 'the observed middleware differs from the declared scheme mapping',
            ];
        }
    }
    return $findings;
}
