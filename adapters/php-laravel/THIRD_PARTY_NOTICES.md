# THIRD_PARTY_NOTICES

This adapter ships no embedded third-party software. The single-file
artifact `adapter.php` is built exclusively from the project's own
kernel source (`adapters/php-laravel/src/kernel.php`) by the
deterministic build script (`adapters/php-laravel/build.php`); the
project's own code is MIT OR Apache-2.0.

Running the adapter requires a PHP interpreter (8.3+) on the host, but
the interpreter is ambient system tooling, exactly like the Node binary
for the Node/TypeScript adapter — it is not vendored, bundled, or
distributed by this package, and it invokes no package manager, no
Composer operation, and no network access.
