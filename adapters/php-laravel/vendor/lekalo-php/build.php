#!/usr/bin/env php
<?php
declare(strict_types=1);
require __DIR__ . '/build-lib.php';
try {
    $options = ['--runtime' => __DIR__, '--output' => 'adapter.php', '--id' => 'lekalo-target-php', '--target' => 'php'];
    $rules = []; $seen = []; $check = false;
    $args = array_slice($_SERVER['argv'] ?? [], 1);
    for ($i = 0; $i < count($args); $i++) {
        $key = $args[$i];
        if ($key === '--check' && !$check) { $check = true; continue; }
        if (($key !== '--rules' && (!array_key_exists($key, $options) || isset($seen[$key]))) || !isset($args[$i + 1])) {
            throw new RuntimeException('usage: build.php [--runtime DIR] [--rules DIR] [--id ID] [--target TARGET] [--output FILE] [--check]');
        }
        $value = $args[++$i]; $seen[$key] = true;
        if ($key === '--rules') { $rules[] = $value; } else { $options[$key] = $value; }
    }
    $result = lekalo_build_compose($options['--runtime'], $rules, $options['--id'], $options['--target']);
    fwrite(STDOUT, json_encode(lekalo_build_publish($result, $options['--output'], $check), JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
} catch (Throwable $error) {
    fwrite(STDERR, $error instanceof RuntimeException ? $error->getMessage() . "\n" : "composition-invalid\n");
    exit(1);
}
