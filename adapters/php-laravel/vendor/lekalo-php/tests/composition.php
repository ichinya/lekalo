<?php
/** Package-local composition and provider conflict regression suite. */
declare(strict_types=1);
require __DIR__ . '/../build-lib.php';
require __DIR__ . '/../src/kernel.php';
require __DIR__ . '/../src/provider.php';

$checks = 0; $failed = [];
function check(bool $ok, string $label): void
{
    global $checks, $failed;
    $checks++;
    if (!$ok) { $failed[] = $label; fwrite(STDERR, "FAIL {$label}\n"); }
}
function refuses(callable $run, string $code, string $label): void
{
    try { $run(); check(false, $label); }
    catch (RuntimeException $error) { check($error->getMessage() === $code, $label); }
}
function child(array $arguments, string $input = ''): array
{
    $process = proc_open(array_merge([PHP_BINARY, '-n'], $arguments),
        [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
    if (!is_resource($process)) { throw new RuntimeException('test-spawn'); }
    fwrite($pipes[0], $input); fclose($pipes[0]);
    $stdout = stream_get_contents($pipes[1]); $stderr = stream_get_contents($pipes[2]);
    fclose($pipes[1]); fclose($pipes[2]);
    return ['code' => proc_close($process), 'stdout' => $stdout, 'stderr' => $stderr];
}
function remove_tree(string $path): void
{
    if (is_link($path) || !is_dir($path)) { unlink($path); return; }
    foreach (scandir($path) as $name) {
        if ($name !== '.' && $name !== '..') { remove_tree($path . '/' . $name); }
    }
    rmdir($path);
}
function manifest(string $path, array $changes = []): void
{
    $value = array_merge([
        'schema' => 'lekalo/php-package/v1', 'id' => 'custom-rules', 'version' => '1.0.0',
        'kind' => 'rules', 'provider_api' => PROVIDER_API_SCHEMA_VERSION,
        'modules' => ['provider.php'], 'resources' => (object) [],
    ], $changes);
    file_put_contents($path . '/lekalo.package.json', json_encode($value, JSON_UNESCAPED_SLASHES));
}
function provider(string $id, array $hooks): array
{
    return ['id' => $id, 'version' => '1.0.0', 'api' => PROVIDER_API_SCHEMA_VERSION,
        'capabilities' => array_fill_keys(array_keys($hooks), 'full'), 'hooks' => $hooks];
}

$root = dirname(__DIR__);
$temp = (getenv('BB_THREAD_STORAGE') ?: sys_get_temp_dir()) . '/lekalo-composition-' . bin2hex(random_bytes(6));
mkdir($temp);
try {
    $plain = lekalo_build_compose($root, []);
    $output = $temp . '/adapter.php';
    lekalo_build_publish($plain, $output, false);
    check(lekalo_build_publish($plain, $output, true)['ok'], 'runtime-only reproducible build');
    $metadata = child([$output, '--version-json']);
    check($metadata['code'] === 0 && $metadata['stderr'] === '', 'runtime-only artifact runs');
    check(json_decode($metadata['stdout'], true)['adapter'] === 'lekalo-target-php', 'neutral runtime identity');
    $request = json_encode(['protocol' => PROTOCOL_TOKEN, 'protocol_version' => VERSION,
        'operation' => 'describe', 'request_id' => 'req-' . str_repeat('a', 64), 'project_root' => '.']);
    $reply = child([$output], $request);
    $description = json_decode($reply['stdout'], true);
    check($reply['code'] === 0 && $reply['stderr'] === '' && $description['status'] === 'ok', 'runtime-only handshake');
    check($description['capabilities']['targets'] === ['php'], 'neutral runtime target');
    check(in_array('.lekalo/generated/php/**', $description['capabilities']['write_scopes'], true)
        && strpos($reply['stdout'], 'php-laravel') === false, 'neutral generated scopes');

    // Names, directories, and hooks belong to the author, with no core edits.
    $alpha = $temp . '/arbitrary-checkout'; $beta = $temp . '/another-checkout';
    mkdir($alpha); mkdir($beta);
    $custom = <<<'PHP'
<?php
declare(strict_types=1);
if (CUSTOM_POLICY !== "policy\n") { throw new RuntimeException('resource-not-bundled'); }
$example = <<<'TEXT'
declare(strict_types=1);
TEXT;
if ($example !== 'declare(strict_types=1);') { throw new RuntimeException('literal-was-rewritten'); }
lekalo_provider_register([
    'id' => 'custom-rules', 'version' => '1.0.0', 'api' => PROVIDER_API_SCHEMA_VERSION,
    'capabilities' => ['lint.framework-semantics' => 'partial'],
    'hooks' => ['lint.framework-semantics' => static fn(): array => ['model_base' => 'Custom\\Model']],
]);
PHP;
    file_put_contents($alpha . '/provider.php', $custom);
    file_put_contents($alpha . '/policy.txt', "policy\n");
    manifest($alpha, ['resources' => ['CUSTOM_POLICY' => 'policy.txt']]);
    file_put_contents($beta . '/provider.php', "<?php\n// independent rules\n");
    manifest($beta, ['id' => 'other-rules']);
    $composed = lekalo_build_compose($root, [$beta, $alpha]);
    check($composed === lekalo_build_compose($root, [$alpha, $beta]), 'rule order does not affect artifact or receipt');
    check(array_column($composed['packages'], 'id') === ['lekalo-php', 'custom-rules', 'other-rules'], 'package identities in receipt');
    lekalo_build_publish($composed, $output, false);
    $reply = child([$output], $request);
    check($reply['code'] === 0 && $reply['stderr'] === '', 'custom provider with bundled resources runs');
    check(count($composed['packages'][1]['files']) === 2, 'resource and source bytes are pinned');
    file_put_contents($beta . '/provider.php', "<?php\n// changed rules\n");
    check(lekalo_build_compose($root, [$alpha, $beta])['digest'] !== $composed['digest'], 'source change alters digest');
    file_put_contents($beta . '/provider.php', "<?php\n// independent rules\n");
    manifest($beta, ['id' => 'other-rules', 'version' => '1.0.1']);
    check(lekalo_build_compose($root, [$alpha, $beta])['digest'] !== $composed['digest'], 'manifest change alters digest');
    manifest($beta, ['id' => 'other-rules']);
    file_put_contents($alpha . '/policy.txt', "changed\n");
    check(lekalo_build_compose($root, [$alpha, $beta])['digest'] !== $composed['digest'], 'resource change alters digest');
    file_put_contents($alpha . '/policy.txt', "policy\n");

    // Composition reads code as bytes. It must never evaluate package inputs.
    file_put_contents($beta . '/provider.php', "<?php\nthrow new RuntimeException('do-not-execute-at-build');\n");
    check(is_array(lekalo_build_compose($root, [$beta])), 'build never executes modules');
    manifest($beta, ['id' => 'custom-rules']);
    refuses(fn() => lekalo_build_compose($root, [$alpha, $beta]), 'package-duplicate', 'duplicate package identity refused');
    manifest($beta, ['provider_api' => 'unsupported']);
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'provider-version-conflict', 'API mismatch refused');
    manifest($beta, ['kind' => 'runtime']);
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-shape', 'rules role enforced');
    manifest($beta, ['modules' => ['../outside.php']]);
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-path', 'path traversal refused');
    manifest($beta, ['modules' => ['missing.php']]);
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-file-missing', 'missing module refused');
    manifest($beta, ['modules' => ['provider.php', 'provider.php']]);
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-module', 'duplicate module refused');
    manifest($beta, ['resources' => ['ADAPTER_ID' => 'provider.php']]);
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-resource-conflict', 'reserved resource refused');
    manifest($beta, ['id' => 'other-rules', 'resources' => ['CUSTOM_POLICY' => 'provider.php']]);
    refuses(fn() => lekalo_build_compose($root, [$alpha, $beta]), 'package-resource-conflict', 'resource collision refused');
    manifest($beta);
    $valid = file_get_contents($beta . '/lekalo.package.json');
    file_put_contents($beta . '/lekalo.package.json', substr($valid, 0, -1) . ',"\u0069d":"custom-rules"}');
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-duplicate-key', 'escaped duplicate JSON key refused');
    file_put_contents($beta . '/lekalo.package.json', '{broken');
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-json', 'invalid JSON refused');
    file_put_contents($beta . '/lekalo.package.json', str_repeat(' ', 65537));
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-file-bound', 'manifest size bounded');
    manifest($beta);
    file_put_contents($beta . '/provider.php', "<?php\n" . str_repeat(' ', 4 * 1024 * 1024));
    refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-file-bound', 'module size bounded');
    unlink($beta . '/provider.php');
    if (DIRECTORY_SEPARATOR !== '\\') {
        symlink($alpha . '/provider.php', $beta . '/provider.php');
        refuses(fn() => lekalo_build_compose($root, [$beta]), 'package-link', 'symlink module refused');
        unlink($beta . '/provider.php');
    }
    $before = file_get_contents($output);
    $invalid = child([$root . '/build.php', '--rules', $beta, '--output', $output]);
    check($invalid['code'] === 1 && file_get_contents($output) === $before, 'failed CLI composition preserves previous artifact');
    refuses(fn() => lekalo_build_publish($plain, $output, true), 'composition-drift', 'check detects drift');
    check(file_get_contents($output) === $before, 'check never rewrites artifact');

    // A conflict must refuse before handling a request, without a PHP stack dump.
    file_put_contents($beta . '/provider.php', str_replace(['custom-rules', 'CUSTOM_POLICY'], ['other-rules', 'CUSTOM_POLICY'], $custom));
    manifest($beta, ['id' => 'other-rules']);
    lekalo_build_publish(lekalo_build_compose($root, [$alpha, $beta]), $output, false);
    $reply = child([$output], $request);
    $diagnostic = json_decode($reply['stderr'], true);
    check($reply['code'] === 1 && $reply['stdout'] === '' && strlen($reply['stderr']) < 256
        && ($diagnostic['diagnostic'] ?? '') === 'provider-capability-conflict', 'startup conflicts have bounded refusal');

    // Registry behavior: no provider wins by its spelling or registration order.
    $registry = &lekalo_provider_registry();
    foreach ([['aaa-rules', 'zzz-rules'], ['zzz-rules', 'aaa-rules']] as [$first, $second]) {
        $registry = [];
        lekalo_provider_register(provider($first, ['routes.emit' => static function (): void {}]));
        refuses(fn() => lekalo_provider_register(provider($second, ['routes.emit' => static function (): void {}])),
            'provider-capability-conflict', 'overlapping family refused: ' . $first);
        check(array_keys($registry) === [$first], 'conflicting registration is atomic: ' . $first);
    }
    $registry = [];
    check(lekalo_provider_lint_facts() === null && lekalo_provider_scenario_runner_table() === null, 'absent facts before registration');
    lekalo_provider_register(provider('lint-rules', ['lint.framework-semantics' => static fn(): array => ['model_base' => 'Custom\\Model']]));
    check(lekalo_provider_lint_facts()['model_base'] === 'Custom\\Model', 'late lint provider replaces absent facts');
    lekalo_provider_register(provider('runner-one', ['scenario.runners' => static fn(): array => ['runners' => ['one' => []], 'default' => 'one']]));
    check(lekalo_provider_default_scenario_runner() === 'one', 'late runner provider replaces absent table');
    lekalo_provider_register(provider('runner-two', ['scenario.runners' => static fn(): array => ['runners' => ['two' => []]]]));
    check(array_keys(lekalo_provider_scenario_runners()) === ['one', 'two'], 'nonconflicting runners merge');
    refuses(fn() => lekalo_provider_call('scenario.runners'), 'provider-capability-conflict', 'single-provider API cannot pick first runner provider');
    lekalo_provider_register(provider('runner-three', ['scenario.runners' => static fn(): array => ['runners' => ['three' => []], 'default' => 'three']]));
    refuses(fn() => lekalo_provider_scenario_runner_table(), 'provider-runner-conflict', 'conflicting runner defaults refused');
    unset($registry['runner-three']);
    lekalo_provider_register(provider('runner-duplicate', ['scenario.runners' => static fn(): array => ['runners' => ['one' => []]]]));
    refuses(fn() => lekalo_provider_scenario_runner_table(), 'provider-runner-conflict', 'duplicate runner refused');
} finally { remove_tree($temp); }

fwrite(STDOUT, json_encode(['suite' => 'php-package-composition', 'checks' => $checks, 'failures' => count($failed), 'failed' => $failed]) . "\n");
exit($failed === [] ? 0 : 1);
