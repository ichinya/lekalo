# brownfield-ts

A synthetic npm-workspace consumer fixture for the issue #118 brownfield
pilot: the in-repo, privacy-safe stand-in for a real private TypeScript
consumer. Every package, symbol, table, and credential-shaped string in
this tree is fictional and authored in-repo (origin: synthetic, declared
in `tests/fixtures/fixture-provenance.json` under the `pilot` family).

## Shape

The tree mirrors the monorepo layout the pilot harness expects:

- `packages/api` — the HTTP surface: a Hono app mounting one
  `POST /tasks` route whose named handler delegates to the repository,
  plus one vitest file covering the submission flow.
- `packages/data` — the storage surface: a Drizzle `sqliteTable`
  definition and the repository functions inserting through it.

Dependency direction: `api` consumes `data`; `data` is a leaf.

The `types/*.d.ts` stubs are minimal authored declarations of the
recognized Hono and vitest surfaces — they are NOT the real packages,
not vendored third-party code, and make no conformance claim. No
package is installed at scan time; the scanner never reads
node_modules and never executes project code. Drizzle specifiers
resolve through the adapter's embedded declaration closure (issue
#116); the owner-supplied `drizzle.bindings.json` at the workspace root
uses generic names only.

The `task.submit` semantic id exercised by the pilot harness is the
issue #118 conditional use-case id.
