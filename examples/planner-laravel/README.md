# Laravel planner reference

Maintained Laravel Router/Database Query Builder implementation of the [same shared HTTP profile](../planner-shared/reference-matrix.json). Use the [common quickstart](../README.md). This deliberately small application reuses the public planner fixture's semantics without importing its test ports, UI or generated framework test harness.

For a standalone copy, from the repository root:

```sh
node scripts/materialize-reference-project.mjs laravel /absolute/new-planner-laravel
```

Run `composer install --no-interaction --prefer-dist --no-scripts` in the exported directory. PHP >=8.3 with `pdo_pgsql` is required. Apply `schema.sql` then `seed.sql` to an isolated PostgreSQL `lekalo_reference` database with the synthetic `postgres` role and loopback-only trust authentication. Set `DB_HOST`, `DB_PORT` (default 56329), optionally `REFERENCE_NOW`, then run:

```sh
php -S 127.0.0.1:8080 router.php
```

The actor, tenant, timezone and idempotency headers have the same meanings as the Node reference. Compose includes a second independent Laravel instance on port 56334: concurrent tests use different instances and prove both transactions overlap at the PostgreSQL lock. This avoids depending on the built-in PHP server's connection scheduling. A single development server is useful interactively but does not qualify this cross-instance race. Windows PHP without `pdo_pgsql` must use the supplied container, not a SQLite fallback.
