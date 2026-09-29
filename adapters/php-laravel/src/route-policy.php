<?php

/**
 * The closed routes policy and input validation of the PHP Laravel
 * routes generator (issue #60). Everything here is pure validation over
 * plain data: the bounded routes input document
 * (`lekalo/routes/*.routes.json`, contract
 * `dev.lekalo.php-routes-input@0.4.0`). Nothing reads the filesystem,
 * nothing writes, nothing executes project code.
 *
 * The closed grammar mirrors `contracts/php-routes-input.schema.v0.4.0.json`
 * and the core join (`crates/lekalo-core/src/php_routes/`) exactly:
 * unknown members and unknown enum values refuse. The core join is the
 * acceptance authority; this mirror is the adapter's defensive gate so
 * a request can never reach the emitter half-validated.
 */

const PHP_ROUTES_INPUT_SCHEMA_VERSION = 'lekalo/php-routes-input/v0.4.0';
const PHP_ROUTES_INPUT_IDENTITY = 'dev.lekalo.php-routes-input@0.4.0';
const PHP_ROUTES_MAP_SCHEMA_VERSION = 'lekalo/php-routes-map/v0.4.0';
const PHP_ROUTES_MAP_IDENTITY = 'dev.lekalo.php-routes-map@0.4.0';
const PHP_ROUTES_EVIDENCE_SCHEMA_VERSION = 'lekalo/php-routes-evidence/v0.4.0';
const PHP_ROUTES_EVIDENCE_IDENTITY = 'dev.lekalo.php-routes-evidence@0.4.0';

/** The generated routes root (managed custody). */
const PHP_ROUTES_GENERATED_ROOT = '.lekalo/generated/php-laravel/routes';

/** The observed-route evidence the checked join consumes. */
const PHP_ROUTES_EVIDENCE_PATH = '.lekalo/import/observed/routes-evidence.json';

const PHP_ROUTES_DEFAULT_NAMESPACE_PREFIX = 'Lekalo\\Generated\\Routes';

/** The closed wire identity the staged transport evidence must carry. */
const PHP_ROUTES_TRANSPORT_SCHEMA_VERSION = 'lekalo/transport-http/v0.4.0';
const PHP_ROUTES_TRANSPORT_IDENTITY = 'dev.lekalo.transport-http@0.4.0';

/**
 * Validate one parsed routes input document against its closed shape.
 * Returns the normalized input array, or null when the document is not
 * the accepted contract (a present-but-invalid document is an authoring
 * error, never an all-defaults fallback).
 */
function php_validate_routes_input(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_ROUTES_INPUT_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_ROUTES_INPUT_IDENTITY) {
        return null;
    }
    // The closed member set (additionalProperties: false): an unknown
    // member is an authoring error, never a silently ignored hint.
    foreach (array_keys($document) as $member) {
        if (!in_array($member, ['schemaVersion', 'identity', 'projectId', 'irDigest', 'transportDigest', 'typesInputDigest', 'operationsInputDigest', 'policy', 'routes'], true)) {
            return null;
        }
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'transportDigest', 'typesInputDigest', 'operationsInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $prefix = PHP_ROUTES_DEFAULT_NAMESPACE_PREFIX;
    $middleware = [];
    if (array_key_exists('policy', $document)) {
        $policy = $document['policy'];
        if (!is_array($policy)) {
            return null;
        }
        foreach (array_keys($policy) as $member) {
            if (!in_array($member, ['namespacePrefix', 'middleware'], true)) {
                return null;
            }
        }
        $declared = $policy['namespacePrefix'] ?? null;
        if (!is_string($declared) || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,5}$/', $declared) !== 1) {
            return null;
        }
        $prefix = $declared;
        if (array_key_exists('middleware', $policy)) {
            if (!is_array($policy['middleware']) || count($policy['middleware']) > 16) {
                return null;
            }
            foreach ($policy['middleware'] as $binding) {
                if (!is_array($binding)) {
                    return null;
                }
                foreach (array_keys($binding) as $member) {
                    if (!in_array($member, ['scheme', 'middleware'], true)) {
                        return null;
                    }
                }
                $scheme = $binding['scheme'] ?? null;
                $spelling = $binding['middleware'] ?? null;
                if (!is_string($scheme) || preg_match('/^[a-z][a-z0-9_]*$/', $scheme) !== 1 || strlen($scheme) > 64) {
                    return null;
                }
                if (!is_string($spelling)
                    || $spelling === ''
                    || strlen($spelling) > 128
                    || preg_match('/^[A-Za-z][A-Za-z0-9:_.-]*$/', $spelling) !== 1) {
                    return null;
                }
                $middleware[] = ['scheme' => $scheme, 'middleware' => $spelling];
            }
            $schemes = array_map(static fn (array $row): string => $row['scheme'], $middleware);
            if (count($schemes) !== count(array_unique($schemes))) {
                return null;
            }
        }
    }
    $routes = $document['routes'] ?? null;
    if (!is_array($routes) || $routes === [] || count($routes) > 2048) {
        return null;
    }
    $records = [];
    foreach ($routes as $route) {
        $record = php_validate_route_record($route);
        if ($record === null) {
            return null;
        }
        $records[] = $record;
    }
    $ids = array_map(static fn (array $record): string => $record['id'], $records);
    $sorted = $ids;
    sort($sorted, SORT_STRING);
    if ($ids !== $sorted || count(array_unique($ids)) !== count($ids)) {
        return null;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'transportDigest' => $document['transportDigest'],
        'typesInputDigest' => $document['typesInputDigest'],
        'operationsInputDigest' => $document['operationsInputDigest'],
        'namespacePrefix' => $prefix,
        'middleware' => $middleware,
        'routes' => $records,
    ];
}

/**
 * One closed route record: the endpoint symbol, the invoked operation,
 * and the per-route custody mode — never a restated wire fact.
 */
function php_validate_route_record(mixed $route): ?array
{
    if (!is_array($route)) {
        return null;
    }
    foreach (array_keys($route) as $member) {
        if (!in_array($member, ['id', 'operation', 'mode', 'entry'], true)) {
            return null;
        }
    }
    $id = $route['id'] ?? null;
    if (!is_string($id) || preg_match('/^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/', $id) !== 1) {
        return null;
    }
    $operation = $route['operation'] ?? null;
    if (!is_string($operation) || preg_match('/^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/', $operation) !== 1) {
        return null;
    }
    $mode = $route['mode'] ?? null;
    if ($mode !== 'managed' && $mode !== 'checked') {
        return null;
    }
    $entry = null;
    if (array_key_exists('entry', $route) && $route['entry'] !== null) {
        if ($mode !== 'checked' || !is_array($route['entry'])) {
            return null;
        }
        foreach (array_keys($route['entry']) as $member) {
            if (!in_array($member, ['fqn', 'method'], true)) {
                return null;
            }
        }
        $fqn = $route['entry']['fqn'] ?? null;
        $method = $route['entry']['method'] ?? null;
        if (!is_string($fqn) || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $fqn) !== 1) {
            return null;
        }
        if (!is_string($method) || preg_match('/^[a-z][A-Za-z0-9_]{0,63}$/', $method) !== 1) {
            return null;
        }
        $entry = ['fqn' => $fqn, 'method' => $method];
    } elseif ($mode === 'checked') {
        return null;
    }
    return ['id' => $id, 'operation' => $operation, 'mode' => $mode, 'entry' => $entry];
}

/**
 * Validate one parsed observed-routes evidence document (the checked
 * route mode's scanner evidence). Returns the normalized document, or
 * null when it is not the accepted contract.
 */
function php_validate_routes_evidence(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_ROUTES_EVIDENCE_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_ROUTES_EVIDENCE_IDENTITY) {
        return null;
    }
    foreach (array_keys($document) as $member) {
        if (!in_array($member, ['schemaVersion', 'identity', 'projectId', 'irDigest', 'routesInputDigest', 'producer', 'sources', 'routes'], true)) {
            return null;
        }
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'routesInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $producer = $document['producer'] ?? null;
    if (!is_array($producer)) {
        return null;
    }
    foreach (array_keys($producer) as $member) {
        if (!in_array($member, ['tool', 'version', 'receiptPath', 'receiptDigest'], true)) {
            return null;
        }
    }
    if (!is_string($producer['tool'] ?? null) || preg_match('/^[a-z][a-z0-9-]{0,31}$/', (string) $producer['tool']) !== 1) {
        return null;
    }
    if (!is_string($producer['version'] ?? null) || preg_match('/^[0-9]+\.[0-9]+\.[0-9]+$/', (string) $producer['version']) !== 1) {
        return null;
    }
    if (array_key_exists('receiptPath', $producer)
        && (!is_string($producer['receiptPath']) || preg_match('/^[.a-z][a-z0-9_\/.-]*\.json$/', (string) $producer['receiptPath']) !== 1)) {
        return null;
    }
    if (array_key_exists('receiptDigest', $producer) && !is_sha256_digest($producer['receiptDigest'])) {
        return null;
    }
    $sources = $document['sources'] ?? null;
    if (!is_array($sources) || $sources === [] || count($sources) > 4096) {
        return null;
    }
    $normalizedSources = [];
    foreach ($sources as $source) {
        if (!is_array($source)) {
            return null;
        }
        foreach (array_keys($source) as $member) {
            if (!in_array($member, ['path', 'digest'], true)) {
                return null;
            }
        }
        $path = $source['path'] ?? null;
        if (!is_string($path) || preg_match('/^[a-z][a-z0-9_\/.-]*\.php$/', (string) $path) !== 1) {
            return null;
        }
        if (!is_sha256_digest($source['digest'] ?? null)) {
            return null;
        }
        $normalizedSources[] = ['path' => $path, 'digest' => $source['digest']];
    }
    $routes = $document['routes'] ?? null;
    if (!is_array($routes) || $routes === [] || count($routes) > 4096) {
        return null;
    }
    $normalizedRoutes = [];
    foreach ($routes as $route) {
        $record = php_validate_observed_route($route);
        if ($record === null) {
            return null;
        }
        $normalizedRoutes[] = $record;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'routesInputDigest' => $document['routesInputDigest'],
        'producer' => $producer,
        'sources' => $normalizedSources,
        'routes' => $normalizedRoutes,
    ];
}

/** One observed route row of the scanner evidence. */
function php_validate_observed_route(mixed $route): ?array
{
    if (!is_array($route)) {
        return null;
    }
    foreach (array_keys($route) as $member) {
        if (!in_array($member, ['method', 'uri', 'name', 'action', 'path', 'digest', 'middleware'], true)) {
            return null;
        }
    }
    $method = $route['method'] ?? null;
    if (!is_string($method) || !in_array($method, ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'], true)) {
        return null;
    }
    $uri = $route['uri'] ?? null;
    if (!is_string($uri) || preg_match('#^/[A-Za-z0-9_{}/-]*$#', (string) $uri) !== 1) {
        return null;
    }
    $name = $route['name'] ?? null;
    if (!is_string($name) || strlen($name) > 192 || preg_match('/^[a-zA-Z0-9._-]*$/', (string) $name) !== 1) {
        return null;
    }
    $action = $route['action'] ?? null;
    if (!is_string($action) || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}@[a-z][A-Za-z0-9_]{0,63}$/', (string) $action) !== 1) {
        return null;
    }
    $path = $route['path'] ?? null;
    if (!is_string($path) || preg_match('/^[A-Za-z][A-Za-z0-9_\/.-]*\.php$/', (string) $path) !== 1) {
        return null;
    }
    if (!is_sha256_digest($route['digest'] ?? null)) {
        return null;
    }
    $middleware = [];
    if (array_key_exists('middleware', $route)) {
        if (!is_array($route['middleware']) || count($route['middleware']) > 8) {
            return null;
        }
        foreach ($route['middleware'] as $spelling) {
            if (!is_string($spelling) || $spelling === '' || strlen($spelling) > 128 || preg_match('/^[A-Za-z][A-Za-z0-9:_.-]*$/', (string) $spelling) !== 1) {
                return null;
            }
            $middleware[] = $spelling;
        }
        $sorted = $middleware;
        sort($sorted, SORT_STRING);
        if ($middleware !== $sorted) {
            return null;
        }
    }
    return [
        'method' => $method,
        'uri' => $uri,
        'name' => $name,
        'action' => $action,
        'path' => $path,
        'digest' => $route['digest'],
        'middleware' => $middleware,
    ];
}

/** The parsed observed routes evidence, or null when absent/corrupt. */
function php_read_routes_evidence(): ?array
{
    $text = read_view_file(PHP_ROUTES_EVIDENCE_PATH);
    if ($text === null) {
        return null;
    }
    try {
        $document = json_decode($text, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return null;
    }
    return php_validate_routes_evidence($document);
}
