# Synthetic brownfield consumer: Hono + Drizzle + MySQL

Status: **Implemented** fixture for issue #105; all names, records and credentials are fictional. Origin: synthetic under the `pilot` provenance family. This is a separate MySQL project; the `brownfield-ts` SQLite pilot is unchanged.

The API package depends on the data package. Runtime checks use actual locked Hono, Drizzle `0.44.7` and mysql2 packages. Declarations under `packages/api/types` and `packages/data/types` are authored scanner aids only, excluded from `tsconfig.runtime.json`. The query mock supplies offline evidence only. Scan never installs packages, executes the application or connects to a database.

Native setup: `npm ci --ignore-scripts --no-audit --no-fund`. `npm run check` typechecks real dependencies; `npm test` performs real in-memory Hono requests with a fake repository. `npm run test:mysql` requires an isolated MySQL 8.4 service and `LEKALO_DOCS_MYSQL_PORT`; absence fails. That lane applies the included synthetic migration and proves create/read/duplicate-key refusal and rollback through the actual driver. It permits only loopback and the dedicated `lekalo_docs_tutorial` database.

See [the tutorial](../../../../docs/tutorial-brownfield-typescript.md) for the disposable observed-mode harness and its explicitly experimental scan-wire fallback. No real consumer is required or permitted.
