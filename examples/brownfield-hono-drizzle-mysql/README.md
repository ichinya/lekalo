# Synthetic Hono / Drizzle / MySQL reference

The maintained source is [the public synthetic brownfield fixture](../../tests/fixtures/pilot/brownfield-mysql/README.md); there is one source tree rather than a second diverging copy. It has fictional tasks, routes/middleware, storage, transaction and driver tests with committed npm locks. It is separate from the PostgreSQL planner profile.

From the repository root, export a standalone application:

```sh
node scripts/materialize-reference-project.mjs brownfield /absolute/new-brownfield
```

In that new directory run `npm ci --ignore-scripts --no-audit --no-fund`, `npm run check` and `npm test`. The real database lane is `npm run test:mysql`, requiring an isolated MySQL 8.4 service and `LEKALO_DOCS_MYSQL_PORT`. The existing CI `mysql-docs` job provisions the service and replays the [brownfield tutorial](../../docs/tutorial-brownfield-typescript.md). Offline repository mocks do not prove MySQL persistence.

Follow that tutorial for observed scanning and explicit binding selection. No real consumer repository is required or admitted. The exporter verifies the source and records its exact revision; it installs no packages and opens no database connections.
