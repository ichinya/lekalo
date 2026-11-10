# Node planner reference

Maintained Hono/Drizzle implementation of the [shared HTTP profile](../planner-shared/reference-matrix.json). Use the [common quickstart](../README.md). The dependency lock is committed; installation is an explicit setup operation.

For a standalone copy, from the repository root:

```sh
node scripts/materialize-reference-project.mjs node /absolute/new-planner-node
```

The destination must not exist. It receives the shared canonical `lekalo/` model, source, dependency lock, schema/seed SQL and pinned reference revision. Run `npm ci --ignore-scripts --no-audit --no-fund` there. Apply `schema.sql` then `seed.sql` to an isolated PostgreSQL `lekalo_reference` database. Set `DATABASE_URL`, optionally `PORT` (default 8080), `HOST` (default loopback), and `REFERENCE_NOW`, then `npm start`. For deterministic comparison use `REFERENCE_NOW=2026-01-02T00:30:00Z`.

`X-Actor` and `X-Tenant` select fictional seeded identities. These headers are a synthetic test authentication seam, not production authentication. `X-Timezone` selects the actor-local calendar day; every mutation requires `Idempotency-Key`. The body is limited to 16 KiB. Test-only rollback injection is disabled unless `REFERENCE_TEST_FAULTS=1`; the normal quickstart leaves it disabled.

The optional query-service boundary and database-enforced read-only role are demonstrated by `extraction.compose.yaml`. Neither extraction nor observed scanning makes maintained application code generator-owned.

`REFERENCE_NOW` pins a deterministic instant when provided. Without it, the clock is evaluated for each request, so Today, carry-over, focus and pause advance while the process stays running. The runtime gate also moves a subprocess's test clock across midnight through a test-only preload module; application code has no clock-control endpoint.
