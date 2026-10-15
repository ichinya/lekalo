<?php

declare(strict_types=1);

// Fixed managed worker; the Rust execution owner stages these exact embedded
// bytes, chooses its run/database, starts and observes each process, and checks
// durable state independently. Its stdout is not an importable receipt.
ini_set('display_errors', 'stderr');
try {
    if ($argc !== 12) { throw new RuntimeException('worker-arguments'); }
    require $argv[1];
    require $argv[2];
    if (Composer\InstalledVersions::getPrettyVersion('laravel/framework') !== 'v13.33.0'
        && Composer\InstalledVersions::getPrettyVersion('laravel/framework') !== '13.33.0') {
        throw new RuntimeException('laravel-version-unqualified');
    }
    [$unused, $autoload, $implementation, $database, $operation, $run, $policy, $capability, $intent, $token, $task, $maximum] = $argv;
    $maximum = filter_var($maximum, FILTER_VALIDATE_INT);
    if ($maximum === false || $maximum < 1 || $maximum > 64) { throw new RuntimeException('worker-attempt-bound'); }
    $store = new App\Lekalo\TaskLinksStore($database);
    echo "ready\n";
    flush();
    if (trim((string) fgets(STDIN)) !== 'start') { throw new RuntimeException('worker-barrier'); }
    $boundary = static function (): void {
        echo "boundary\n";
        flush();
        // Only a controller-sent resume permits commit/ack. Closing stdin
        // refuses and rolls back rather than accidentally proving a kill leg.
        if (trim((string) fgets(STDIN)) !== 'resume') { throw new RuntimeException('worker-interrupted'); }
    };
    $result = match ($operation) {
        'init' => (static function () use ($store): string { $store->initialize(); return 'initialized'; })(),
        'attach' => $store->attach($task),
        'request' => (static function () use ($store, $intent, $run, $policy, $capability, $maximum): string { $store->request($intent, $run, $policy, $capability, $maximum); return 'committed'; })(),
        'request-kill' => (static function () use ($store, $intent, $run, $policy, $capability, $maximum, $boundary): string { $store->request($intent, $run, $policy, $capability, $maximum, $boundary); return 'committed'; })(),
        'claim' => $store->claim($intent, $token, $policy, $capability),
        'accept-kill' => (static function () use ($store, $intent, $token, $boundary): string { $store->accept($intent, $token); $boundary(); return 'accepted'; })(),
        'recover' => $store->recover($intent),
        'reconcile' => $store->reconcile($intent),
        'fail' => $store->fail($intent, $token),
        'cas' => $store->compareAndSwap(1, $token),
        default => throw new RuntimeException('worker-operation'),
    };
    echo json_encode(['runId' => $run, 'operation' => $operation, 'result' => $result,
        'runtime' => 'laravel/framework@13.33.0', 'engine' => 'pdo-sqlite'], JSON_THROW_ON_ERROR)."\n";
} catch (Throwable $error) {
    fwrite(STDERR, "task-links-worker-refused\n");
    exit(1);
}
