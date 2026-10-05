# Brownfield TypeScript: synthetic Hono + Drizzle + MySQL

Status: **Implemented** MySQL fixture/runtime tests; the observed contributor harness is **Experimental** because its public scan-wire path can refuse. Owner: Node evidence fixture maintainers. [#118](https://github.com/ichinya/lekalo/issues/118), [#115](https://github.com/ichinya/lekalo/issues/115), [#116](https://github.com/ichinya/lekalo/issues/116).

The `brownfield-consumer` is the NEW synthetic [brownfield-mysql fixture](../tests/fixtures/pilot/brownfield-mysql/README.md). It uses Hono `4.13.12`, Drizzle `0.44.7`, mysql2 `3.24.5`, a `mysqlTable` schema and a real runtime persistence entrypoint. Every package/name/record is fictional. The existing `brownfield-ts` fixture uses SQLite and is not this tutorial.

## Native setup and offline HTTP checks

Use Node 24. In a disposable copy of the new fixture, provision the committed package lock explicitly:

```sh docs-setup=mysql-packages
npm ci --ignore-scripts --no-audit --no-fund
```

This visible setup is performed before replay. Scanning installs nothing. Runtime typechecking excludes all authored Hono/Drizzle scanner declarations; the request tests use the actual Hono package with an injected fake repository:

```sh docs-example=mysql-http
node node_modules/typescript/bin/tsc --noEmit -p tsconfig.runtime.json
node --test --test-reporter=tap test/http.test.mjs
```

Expected: exit 0; actual-package typecheck and one HTTP test pass, including 201 response and invalid-input/JSON 400 controls with no persistence call. No database claim follows from this lane.

## Adopt, confirm and project (B-D)

**Experimental** contributor path, from the Lekalo repository root:

```sh docs-example=mysql-observed
node scripts/pilot-brownfield-ts.mjs --consumer tests/fixtures/pilot/brownfield-mysql --bind packages/api/src/routes/tasks.ts:submitTask:task.submit --disposition public-fixture
```

Expected: a fresh disposable consumer copy, adoption dry-run purity, adoption, explicitly scoped scan, confirmed `task.submit`, promotion, inspect/impact/context and mutation/revert staleness checks; exit 0 only when every stage passes. The script records the actual wire result first. A documented direct-kernel fallback is not a successful `lekalo scan` protocol exchange. No scope/manifest/uncertainty refusal is weakened to pass this example.

The offline scope reads package `src`, authored scan types and selected manifests; `packages/data/runtime/` configuration is outside that scope and is tested in the native lane. Drizzle declarations/query shapes are evidence only; database state stays unknown. Runtime coverage is not inferred from an offline scan. [Drizzle evidence limits](drizzle-bindings.md), [observed mode](observed-mode.md), [adoption](adoption.md), [security](security.md).

## Real MySQL persistence

**Implemented**, required for the persistence claim. Provision a NEW isolated MySQL `8.4.5` service, image digest `sha256:679e7e924f38a3cbb62a3d7df32924b83f7321a602d3f9f967c01b3df18495d6`. Use only loopback, database `lekalo_docs_tutorial`, user `tutorial` and fictional password `synthetic-tutorial-only`; publish no production URL. CI provides this service in a dedicated job. Set `LEKALO_DOCS_MYSQL_PORT` to its published port in your shell; no default database or missing-service skip is allowed.

In the provisioned fixture copy:

```sh docs-example=mysql-persistence
node --test --test-reporter=tap test/mysql.test.mjs
```

Expected: exit 0; one real MySQL/Hono/mysql2 test passes. It explicitly applies the included migration, creates and reads a synthetic task through the actual runtime repository, refuses duplicate primary keys and proves transaction rollback. Missing port/service or a non-MySQL-8.4 server fails. The evidence adapter never applies migrations. SQLite and mocked query results cannot satisfy this lane.

The documentation gate's `--lane mysql` requires HTTP/typecheck and persistence commands, dependency custody and source preservation. Portable replay and native persistence are separate required CI lanes, with separate results.
