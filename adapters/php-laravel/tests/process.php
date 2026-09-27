<?php
/**
 * The process-level suite of the PHP kernel (issue #54): the one-shot
 * entry contract (one bounded request, exactly one response, bounded
 * stderr diagnostics on refusal), the request-file transport, hostile
 * inputs, and the packaging determinism of the generated artifact.
 * Run with `php adapters/php-laravel/tests/process.php` from any cwd.
 */

declare(strict_types=1);

require __DIR__ . '/../src/kernel.php';

$GLOBALS['__lekalo_failures'] = [];
$GLOBALS['__lekalo_checks'] = 0;

function check(bool $condition, string $name): void
{
    $GLOBALS['__lekalo_checks']++;
    if (!$condition) {
        $GLOBALS['__lekalo_failures'][] = $name;
        fwrite(STDERR, "FAIL {$name}\n");
    }
}

/**
 * Run the committed artifact as a real child process: exactly the argv
 * the confined core uses (php adapter.php [--lekalo-request-file f]).
 */
function run_artifact(?string $stdin, array $extraArgs = []): array
{
    $artifact = dirname(__DIR__) . '/adapter.php';
    $command = escapeshellarg(PHP_BINARY) . ' -n ' . escapeshellarg($artifact);
    foreach ($extraArgs as $argument) {
        $command .= ' ' . escapeshellarg($argument);
    }
    $process = proc_open($command, [
        0 => ['pipe', 'r'],
        1 => ['pipe', 'w'],
        2 => ['pipe', 'w'],
    ], $pipes);
    if (!is_resource($process)) {
        return ['exit' => -1, 'stdout' => '', 'stderr' => 'spawn-failed'];
    }
    if ($stdin !== null) {
        fwrite($pipes[0], $stdin);
    }
    fclose($pipes[0]);
    $stdout = stream_get_contents($pipes[1]);
    $stderr = stream_get_contents($pipes[2]);
    fclose($pipes[1]);
    fclose($pipes[2]);
    $exit = proc_close($process);
    return ['exit' => $exit, 'stdout' => (string) $stdout, 'stderr' => (string) $stderr];
}

function describe_json(): string
{
    return json_encode([
        'protocol' => PROTOCOL_TOKEN,
        'protocol_version' => VERSION,
        'operation' => 'describe',
        'request_id' => 'req-' . str_repeat('7', 64),
        'project_root' => '.',
    ], JSON_UNESCAPED_SLASHES);
}

// --- the one-shot entry contract -------------------------------------------

$result = run_artifact(describe_json());
check($result['exit'] === 0, 'clean exit on a legal request');
$decoded = json_decode($result['stdout'], true, 64);
check(is_array($decoded) && $decoded['status'] === 'ok', 'exactly one JSON envelope');
check($decoded['evidence']['adapter']['id'] === ADAPTER_ID, 'envelope binds the identity');
check($result['stderr'] === '', 'no stderr noise on success');

// response byte-bounds and determinism
check(strlen($result['stdout']) <= MAX_RESPONSE_BYTES, 'response inside the transport bound');
$again = run_artifact(describe_json());
check($again['stdout'] === $result['stdout'], 'repeated exchanges are byte-identical');

// --- hostile and malformed inputs ------------------------------------------

$hostile = [
    'not json' => 'this is not json',
    'empty' => '',
    'truncated' => substr(describe_json(), 0, 20),
    'wrong token' => '{"protocol":"lekalo.target/v2","protocol_version":"0.3.2","operation":"describe","request_id":"' . str_repeat('0', 4) . '","project_root":"."}',
    'duplicate key' => '{"protocol":1,"protocol":2}',
    'array document' => '[1,2,3]',
];
foreach ($hostile as $name => $bytes) {
    $outcome = run_artifact($bytes);
    check($outcome['exit'] !== 0, "hostile input refuses: {$name}");
    check($outcome['stdout'] === '', "hostile input never fabricates an envelope: {$name}");
    check($outcome['stderr'] !== '' && strlen($outcome['stderr']) <= 256, "hostile input bounded diagnostic: {$name}");
    $diagnostic = json_decode(trim($outcome['stderr']), true);
    check(is_array($diagnostic) && ($diagnostic['kernel'] ?? '') === ADAPTER_ID, "hostile input diagnostic shape: {$name}");
}

// oversized request: one byte over the bound
$oversize = str_repeat(' ', MAX_REQUEST_BYTES + 1);
$outcome = run_artifact($oversize);
check($outcome['exit'] !== 0 && $outcome['stdout'] === '', 'oversized request refused');

// request at exactly the bound but malformed content still refuses gracefully
$exact = str_pad('{"protocol":"lekalo.target/v1"', MAX_REQUEST_BYTES, ' ');
$outcome = run_artifact($exact);
check($outcome['exit'] !== 0, 'malformed document at the bound refused');

// --- the request-file transport ---------------------------------------------

$requestFile = tempnam(sys_get_temp_dir(), 'lekalo-req-');
file_put_contents($requestFile, describe_json());
$outcome = run_artifact(null, ['--lekalo-request-file', $requestFile]);
check($outcome['exit'] === 0 && $outcome['stdout'] === $result['stdout'], 'request-file transport identical to stdin');
unlink($requestFile);

$outcome = run_artifact(null, ['--lekalo-request-file']);
check($outcome['exit'] !== 0, 'missing request-file value refused');
$outcome = run_artifact(null, ['--lekalo-request-file', sys_get_temp_dir() . '/lekalo-missing-' . getmypid() . '.json']);
check($outcome['exit'] !== 0, 'missing request file refused');

// --- the version probe ------------------------------------------------------

$probe = run_artifact(null, ['--version-json']);
check($probe['exit'] === 0, 'version probe exits cleanly');
$metadata = json_decode(trim($probe['stdout']), true);
check(($metadata['adapter'] ?? '') === ADAPTER_ID && isset($metadata['php']), 'version probe shape');

// --- packaging determinism --------------------------------------------------

$build = dirname(__DIR__) . '/build.php';
$checkOutput = shell_exec(escapeshellarg(PHP_BINARY) . ' -n ' . escapeshellarg($build) . ' --check 2>&1');
$checkResult = json_decode((string) $checkOutput, true);
check(is_array($checkResult) && ($checkResult['ok'] ?? false) === true && ($checkResult['check'] ?? false) === true, 'build --check holds');
check(is_string($checkResult['digest'] ?? null) && str_starts_with((string) $checkResult['digest'], 'sha256:'), 'artifact digest reported');
$committed = file_get_contents(dirname(__DIR__) . '/adapter.php');
check($committed !== false && hash('sha256', $committed) === substr((string) $checkResult['digest'], 7), 'committed bytes match the reported digest');

// --- summary ----------------------------------------------------------------

fwrite(STDOUT, json_encode([
    'suite' => 'php-laravel-process',
    'checks' => $GLOBALS['__lekalo_checks'],
    'failures' => count($GLOBALS['__lekalo_failures']),
]) . "\n");
exit($GLOBALS['__lekalo_failures'] === [] ? 0 : 1);
