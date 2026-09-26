#!/usr/bin/env php
<?php
/**
 * Deterministic generator of the committed single-file artifact
 * `adapter.php` (issue #54).
 *
 * The kernel is dependency-free by design, so the build is a fixed-order
 * concatenation: a generated banner, then the exact kernel source bytes.
 * There are no timestamps, absolute paths, environment lookups, or
 * filesystem traversal anywhere in the pipeline, so repeated builds are
 * byte-identical.
 *
 * `build.php --check` rebuilds in memory and compares the bytes to the
 * committed artifact without ever rewriting it.
 */

declare(strict_types=1);

const ADAPTER_ID = 'lekalo-target-php-laravel';
const ADAPTER_VERSION = '0.1.0';

function fail(string $message): never
{
    fwrite(STDERR, $message . "\n");
    exit(1);
}

$adapterRoot = dirname(__FILE__);
$artifactPath = $adapterRoot . '/adapter.php';
$kernelPath = $adapterRoot . '/src/kernel.php';

$kernel = file_get_contents($kernelPath);
if ($kernel === false || $kernel === '') {
    fail('build: kernel source is missing or empty');
}

$banner = <<<BANNER
<?php
/**
 * The committed single-file adapter artifact of `{ADAPTER_ID}`
 * (issue #54).
 *
 * GENERATED FILE — regenerate with `php adapters/php-laravel/build.php`;
 * verify with `php adapters/php-laravel/build.php --check`. Never edit.
 *
 * Self-contained: PHP built-ins only (json, hash, SPL), no Composer
 * package, no extension beyond always-compiled basics, so the confined
 * core runtime can copy the interpreter plus exactly this script.
 */

BANNER;
$banner = str_replace('{ADAPTER_ID}', ADAPTER_ID, $banner);

// The kernel source already opens with its own `<?php` declare block; the
// artifact embeds the banner's opening tag, then the kernel body minus
// its duplicated opening tag line.
$kernelBody = $kernel;
if (str_starts_with($kernelBody, "<?php\n")) {
    $kernelBody = substr($kernelBody, 6);
} else {
    fail('build: kernel source must start with the open tag');
}

$artifact = $banner . $kernelBody
    . "\nexit(main());\n";
$digest = 'sha256:' . hash('sha256', $artifact);

$check = in_array('--check', array_slice($_SERVER['argv'] ?? [], 1), true);
if ($check) {
    if (!is_file($artifactPath)) {
        fail('--check: committed artifact is missing');
    }
    $committed = file_get_contents($artifactPath);
    if ($committed !== $artifact) {
        fwrite(STDERR, "--check: committed adapter.php differs from the deterministic rebuild\nrebuild digest: {$digest}\n");
        exit(1);
    }
    fwrite(STDOUT, json_encode([
        'ok' => true,
        'check' => true,
        'digest' => $digest,
        'bytes' => strlen($artifact),
    ], JSON_UNESCAPED_SLASHES) . "\n");
    exit(0);
}

file_put_contents($artifactPath, $artifact);
fwrite(STDOUT, json_encode([
    'ok' => true,
    'written' => 'adapter.php',
    'digest' => $digest,
    'bytes' => strlen($artifact),
    'php' => PHP_VERSION,
], JSON_UNESCAPED_SLASHES) . "\n");
