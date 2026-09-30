# Hono framework-binding fixtures (issue #115)

Synthetic TypeScript projects exercising the node-typescript adapter's
Hono framework provider. Every file here is authored in-repo (origin:
synthetic, recorded in `tests/fixtures/fixture-provenance.json` under the
registered `node-typescript-scanner` family). The `types/*.d.ts` stubs
are minimal authored declarations of the recognized Hono surface — they
are NOT the real packages, not vendored third-party code, and make no
conformance claim. No package is installed at scan time; the scanner
never reads node_modules and never executes project code.

Projects:
- `static/`      — apps, static routes, handler→service calls, endpoint contracts
- `composition/` — nested routers, basePath views, shared children
- `composition-cycle/` — mount cycle (uncertainty, never fabricated routes)
- `middleware/`  — use chains, order, roles (JSDoc), context keys
- `http/`        — validators, responses, HTTPException, onError/notFound
- `openapi/`     — OpenAPIHono + createRoute + operationId
- `ssr/`         — JSX/SSR pages vs API routes vs mixed
- `tests/`       — app.request/testClient bindings
- `uncertainty/` — dynamic paths/factories/mutable aliases (unknown, never guessed)
- `lookalike/`   — negative controls: local class Hono, fake router, shadowing
