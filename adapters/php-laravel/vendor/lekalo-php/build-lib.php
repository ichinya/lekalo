<?php
/** Package-owned, deterministic PHP composition. Reads inputs; never executes them. */
declare(strict_types=1);

function lekalo_build_read(string $root, string $path, int $limit): string
{
    $parts = explode('/', $path);
    if (strlen($path) > 256) { throw new RuntimeException('package-path'); }
    $full = $root;
    foreach ($parts as $part) {
        if ($part === '.' || $part === '..' || !preg_match('/^[A-Za-z0-9_.-]+$/D', $part)) {
            throw new RuntimeException('package-path');
        }
        $full .= '/' . $part;
        if (is_link($full)) { throw new RuntimeException('package-link'); }
    }
    if (!is_file($full)) { throw new RuntimeException('package-file-missing'); }
    $handle = fopen($full, 'rb');
    if ($handle === false) { throw new RuntimeException('package-unreadable'); }
    try { $bytes = stream_get_contents($handle, $limit + 1); }
    finally { fclose($handle); }
    if ($bytes === false || strlen($bytes) > $limit) { throw new RuntimeException('package-file-bound'); }
    return $bytes;
}

/** json_decode validates syntax; this scan rejects decoded duplicate object keys. */
function lekalo_build_json(string $bytes): object
{
    try { $document = json_decode($bytes, false, 16, JSON_THROW_ON_ERROR); }
    catch (JsonException $error) { throw new RuntimeException('package-json'); }
    if (!is_object($document)) { throw new RuntimeException('package-shape'); }
    $stack = [];
    for ($i = 0, $length = strlen($bytes); $i < $length; $i++) {
        $c = $bytes[$i];
        if ($c === '{' || $c === '[') { $stack[] = []; }
        elseif ($c === '}' || $c === ']') { array_pop($stack); }
        elseif ($c === '"') {
            $start = $i++;
            while ($i < $length && $bytes[$i] !== '"') {
                if ($bytes[$i] === '\\') { $i++; }
                $i++;
            }
            $next = $i + 1;
            while ($next < $length && strpos(" \t\r\n", $bytes[$next]) !== false) { $next++; }
            if ($next < $length && $bytes[$next] === ':') {
                $key = json_decode(substr($bytes, $start, $i - $start + 1), true, 16, JSON_THROW_ON_ERROR);
                $top = count($stack) - 1;
                if (array_key_exists($key, $stack[$top])) { throw new RuntimeException('package-duplicate-key'); }
                $stack[$top][$key] = true;
            }
        }
    }
    return $document;
}

/** Remove only a leading strict_types declaration, never text in a PHP literal. */
function lekalo_build_fragment(string $source): string
{
    $fragment = substr($source, 6); $cursor = 0; $length = strlen($fragment);
    while ($cursor < $length) {
        $cursor += strspn($fragment, " \t\r\n", $cursor);
        if (substr($fragment, $cursor, 2) === '/*') {
            $end = strpos($fragment, '*/', $cursor + 2);
            if ($end === false) { break; }
            $cursor = $end + 2;
        } elseif (substr($fragment, $cursor, 2) === '//' || substr($fragment, $cursor, 1) === '#') {
            $end = strpos($fragment, "\n", $cursor);
            if ($end === false) { break; }
            $cursor = $end + 1;
        } else { break; }
    }
    $declaration = "declare(strict_types=1);\n";
    if (substr($fragment, $cursor, strlen($declaration)) === $declaration) {
        return substr($fragment, 0, $cursor) . substr($fragment, $cursor + strlen($declaration));
    }
    return $fragment;
}

function lekalo_build_package(string $path, string $kind): array
{
    if (is_link($path)) { throw new RuntimeException('package-link'); }
    $root = realpath($path);
    if ($root === false || !is_dir($root)) { throw new RuntimeException('package-root'); }
    $bytes = lekalo_build_read($root, 'lekalo.package.json', 65536);
    $manifest = lekalo_build_json($bytes);
    $keys = array_keys(get_object_vars($manifest)); sort($keys);
    if ($keys !== ['id', 'kind', 'modules', 'provider_api', 'resources', 'schema', 'version']
        || $manifest->schema !== 'lekalo/php-package/v1' || $manifest->kind !== $kind
        || !is_string($manifest->id) || !preg_match('/^[a-z][a-z0-9-]{2,63}$/D', $manifest->id)
        || !is_string($manifest->version) || !preg_match('/^[0-9]+\.[0-9]+\.[0-9]+$/D', $manifest->version)
        || !is_string($manifest->provider_api) || strlen($manifest->provider_api) > 128
        || !is_array($manifest->modules) || count($manifest->modules) < 1 || count($manifest->modules) > 64
        || !is_object($manifest->resources) || count(get_object_vars($manifest->resources)) > 16) {
        throw new RuntimeException('package-shape');
    }
    $seen = []; $modules = []; $resources = []; $files = []; $total = 0;
    foreach ($manifest->modules as $name) {
        if (!is_string($name) || isset($seen[$name])) { throw new RuntimeException('package-module'); }
        $seen[$name] = true;
        $source = lekalo_build_read($root, $name, 4 * 1024 * 1024);
        if (!str_starts_with($source, "<?php\n")) { throw new RuntimeException('package-module-tag'); }
        $total += strlen($source);
        if ($total > 16 * 1024 * 1024) { throw new RuntimeException('package-content-bound'); }
        $files[$name] = 'sha256:' . hash('sha256', $source);
        // Fragments share the artifact's strict_types declaration.
        $modules[$name] = lekalo_build_fragment($source);
    }
    foreach (get_object_vars($manifest->resources) as $constant => $name) {
        if (!preg_match('/^[A-Z][A-Z0-9_]{0,95}$/D', $constant) || !is_string($name)) {
            throw new RuntimeException('package-resource');
        }
        $resources[$constant] = lekalo_build_read($root, $name, 1024 * 1024);
        $total += strlen($resources[$constant]);
        if ($total > 16 * 1024 * 1024) { throw new RuntimeException('package-content-bound'); }
        $files[$name] = 'sha256:' . hash('sha256', $resources[$constant]);
    }
    ksort($resources, SORT_STRING); ksort($files, SORT_STRING);
    return ['manifest' => $manifest, 'modules' => $modules, 'resources' => $resources,
        'receipt' => ['id' => $manifest->id, 'version' => $manifest->version,
            'manifestDigest' => 'sha256:' . hash('sha256', $bytes), 'files' => $files]];
}

/** Runtime first, then rules sorted by package id. No repository-name dispatch. */
function lekalo_build_compose(string $runtime, array $rules, string $id = 'lekalo-target-php', string $target = 'php'): array
{
    if (count($rules) > 8 || !preg_match('/^[a-z][a-z0-9-]{2,63}$/D', $id)
        || !preg_match('/^[a-z][a-z0-9-]{0,63}$/D', $target)) { throw new RuntimeException('composition-shape'); }
    $language = lekalo_build_package($runtime, 'runtime'); $providers = [];
    foreach ($rules as $path) {
        if (!is_string($path)) { throw new RuntimeException('composition-shape'); }
        $provider = lekalo_build_package($path, 'rules'); $name = $provider['manifest']->id;
        if ($name === $language['manifest']->id || isset($providers[$name])) { throw new RuntimeException('package-duplicate'); }
        if ($provider['manifest']->provider_api !== $language['manifest']->provider_api) {
            throw new RuntimeException('provider-version-conflict');
        }
        $providers[$name] = $provider;
    }
    ksort($providers, SORT_STRING);
    $packages = array_merge([$language], array_values($providers));
    $quote = static fn(string $value): string => "'" . str_replace(['\\', "'"], ['\\\\', "\\'"], $value) . "'";
    $artifact = "<?php\n// GENERATED by lekalo-php package composition. Do not edit.\ndeclare(strict_types=1);\n"
        . "define('ADAPTER_ID', " . $quote($id) . ");\ndefine('TARGET_TOKEN', " . $quote($target) . ");\n";
    // Registration happens before main(). Preserve the same bounded refusal
    // transport for a conflicting or broken provider at process startup.
    $artifact .= <<<'PHP'
set_exception_handler(static function (Throwable $error): void {
    $code = $error instanceof RequestRefusal ? $error->getMessage() : 'provider-startup';
    $code = substr(preg_replace('/[^a-z0-9._-]/', '-', $code) ?? 'provider-startup', 0, 128);
    fwrite(STDERR, json_encode(['kernel' => ADAPTER_ID, 'diagnostic' => $code], JSON_UNESCAPED_SLASHES) . "\n");
    exit(1);
});
PHP;
    $artifact .= "\n";
    $constants = ['ADAPTER_ID' => true, 'TARGET_TOKEN' => true, 'ADAPTER_VERSION' => true, 'PROVIDER_API_SCHEMA_VERSION' => true, 'PROVIDER_API_IDENTITY' => true];
    foreach ($packages as $package) {
        foreach ($package['resources'] as $constant => $bytes) {
            if (isset($constants[$constant])) { throw new RuntimeException('package-resource-conflict'); }
            $constants[$constant] = true;
            $artifact .= 'const ' . $constant . ' = ' . $quote($bytes) . ";\n";
        }
    }
    foreach ($packages as $package) {
        $artifact .= "\n// package " . $package['manifest']->id . '@' . $package['manifest']->version
            . ' ' . $package['receipt']['manifestDigest'] . "\n";
        foreach ($package['modules'] as $name => $source) {
            $artifact .= "\n// module {$name}\n" . $source . "\n";
            if (strlen($artifact) > 32 * 1024 * 1024) { throw new RuntimeException('composition-bound'); }
        }
    }
    $artifact .= "\nexit(main());\n";
    return ['artifact' => $artifact, 'packages' => array_column($packages, 'receipt'),
        'digest' => 'sha256:' . hash('sha256', $artifact)];
}

function lekalo_build_publish(array $result, string $output, bool $check): array
{
    if (is_link($output)) { throw new RuntimeException('output-link'); }
    if ($check) {
        if (!is_file($output) || file_get_contents($output) !== $result['artifact']) { throw new RuntimeException('composition-drift'); }
    } else {
        // Complete and validate composition before touching the output.
        if (!is_dir(dirname($output))) { throw new RuntimeException('output-directory'); }
        $temporary = tempnam(dirname($output), '.lekalo-build-');
        if ($temporary === false) { throw new RuntimeException('output-write'); }
        try {
            if (file_put_contents($temporary, $result['artifact']) !== strlen($result['artifact'])
                || !rename($temporary, $output)) { throw new RuntimeException('output-write'); }
        } finally { if (is_file($temporary)) { unlink($temporary); } }
    }
    return ['ok' => true, 'check' => $check, 'digest' => $result['digest'], 'bytes' => strlen($result['artifact']), 'packages' => $result['packages']];
}
