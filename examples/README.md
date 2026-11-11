# Executable planner reference projects

Status: maintained synthetic Node and Laravel HTTP/PostgreSQL references for [issue #109](https://github.com/ichinya/lekalo/issues/109). Owner: reference-project maintainers. These are small onboarding/conformance examples, not a mandatory application architecture.

One canonical [Lekalo Model](planner-shared/lekalo/project.yaml), [HTTP profile](planner-shared/reference-matrix.json), [portable HTTP scenario corpus](planner-shared/scenarios.json), SQL schema and deterministic seed are shared by both targets. The model was derived from the existing public routes fixture; that historical fixture remains a separate regression input. `reference-projects.lock.json` pins exact shared/example bytes, dependency locks and native adapter manifests. No verification step rewrites locks or installs dependencies.

## Quickstart

Prerequisites: Docker Compose, Node 24 and Composer with PHP >=8.3. Run from the repository root. Dependency installation is explicit setup, before verification:

```sh
npm ci --ignore-scripts --no-audit --no-fund --prefix examples/planner-node
composer install --no-interaction --prefer-dist --no-scripts --working-dir examples/planner-laravel
docker compose -p lekalo-reference-109 -f examples/compose.yaml up -d --build
```

Endpoints: Node `http://127.0.0.1:56330`, Laravel `http://127.0.0.1:56331`, health `/health`. A second independent Laravel instance at port 56334 is used to prove cross-instance concurrency. PostgreSQL is isolated in this Compose project, exposed only on loopback port 56329, and contains fictional data. It uses trust authentication for this local exercise; it is not a production deployment recipe. Set `REFERENCE_PG_PORT`, `REFERENCE_NODE_PORT`, `REFERENCE_LARAVEL_PORT` or `REFERENCE_LARAVEL_PEER_PORT` before starting to override ports. The native gate must receive matching `REFERENCE_DATABASE_URL`, `REFERENCE_NODE_URL`, `REFERENCE_LARAVEL_URL` and `REFERENCE_LARAVEL_PEER_URL` overrides.

Request headers: `X-Actor: user-a`, `X-Tenant: workspace-a`, optional `X-Timezone: UTC`, and `Idempotency-Key` for every mutation. The fixed reference clock is `2026-01-02T00:30:00Z`. Synthetic tasks have IDs `00000000-0000-4000-8000-000000000001` through `...000005`; actors/workspaces are listed in `planner-shared/seed.sql`.

```sh
curl -H 'X-Actor: user-a' -H 'X-Tenant: workspace-a' http://127.0.0.1:56330/planning/today
curl -H 'X-Actor: user-a' -H 'X-Tenant: workspace-a' -H 'Idempotency-Key: first-plan' -H 'Content-Type: application/json' -d '{"task_id":"00000000-0000-4000-8000-000000000001","planned_for":"2026-01-02"}' http://127.0.0.1:56330/planning
```

The curl commands above use Bash quoting. Use `curl.exe` in Windows PowerShell when `curl` is its web-request alias and adapt native JSON argument quoting for your PowerShell version. The [Node](planner-node/README.md) and [Laravel](planner-laravel/README.md) pages document standalone local exports, database setup and exact differences. Both implement the same profile; move/complete/count operations are outside this reference revision and are not included in its canonical model. Unsupported operations are not silently counted as passed.

## Conformance

Read-only source/lock check, with no database or installed application dependencies:

```sh
node scripts/test-reference-projects.mjs --static
```

For the real gate enable the explicit test-only post-write rollback seam and recreate the applications. Bash:

```sh
REFERENCE_TEST_FAULTS=1 docker compose -p lekalo-reference-109 -f examples/compose.yaml up -d
node scripts/test-reference-projects.mjs
```

PowerShell uses `$env:REFERENCE_TEST_FAULTS='1'` followed by the same Compose and Node commands. The gate **resets planning rows and receipts in this dedicated synthetic database** between scenarios; do not point it at a shared database. It executes all 16 cases on each backend with real HTTP, independent connections and PostgreSQL. Concurrency is proven by holding an actor lock until two request transactions are simultaneously waiting. Laravel competitors use its two independent instances, avoiding the built-in server's connection scheduling. The partial unique index also refuses an invariant-violating direct write. A real mutated Node application runs separately and must fail the focus scenario, proving semantic drift detection. Missing services, missing prerequisites, `unsupported` and infra errors are failures. No application packages are installed during the gate.

The historical six-case Scenario IR parity remains independently exercised by existing gates; this new portable HTTP corpus is an additional acceptance layer, not a claim that the historical unsupported concurrency row now passed. New HTTP evidence is not serialized as invented Scenario IR receipts.

The shared corpus includes plan → reorder → plan → reorder, requiring new rows to preserve the current day's reorder version. A separate real HTTP subprocess test runs without `REFERENCE_NOW` and advances its test clock across midnight, checking Today, carry-over, focus and pause without restarting the app. Fixed-clock conformance remains covered by the shared corpus.

## Extract a query module

```sh
REFERENCE_TEST_FAULTS=1 docker compose -p lekalo-reference-109 -f examples/compose.yaml -f examples/extraction.compose.yaml up -d
node scripts/test-reference-projects.mjs --extraction
```

Node forwards read queries over HTTP to a separate reader service; mutations remain local. The same 16 cases must still pass. Additional controls prove that the extracted module refuses HTTP writes and its PostgreSQL role cannot write. Reader port: 56333. This demonstrates the boundary and behavior-preserving extraction without inventing a Go/Rust adapter or treating a shared database as independent service storage.

## Ownership and workflow

[The workflow example](planner-shared/workflow.md) links the OpenSpec requirements, optional HLV/native evidence, read-only AI Factory MCP descriptor and bounded ownership exercises. `node scripts/test-reference-contract.mjs`, after building the matching CLI, verifies 16 owner-declared bindings against the actual exported Node/Laravel model and sources: missing coverage refuses, explicit attachment enables clean conformance, and changed source refuses. It does not execute the attached tests; the separate HTTP gate emits their actual IDs. Observed and managed exercises retain their documented fixture scopes and do not claim whole-application managed generation. The [synthetic MySQL brownfield project](brownfield-hono-drizzle-mysql/README.md) has one maintained source and a standalone export path.

Go and Rust examples remain deferred behind [#67](https://github.com/ichinya/lekalo/issues/67) and [#68](https://github.com/ichinya/lekalo/issues/68). Do not treat the current slice as automatic closure of #109. No consumer repository, private identifiers, production data or production credentials are used.

## Cleanup and maintenance

```sh
docker compose -p lekalo-reference-109 -f examples/compose.yaml -f examples/extraction.compose.yaml down --volumes --remove-orphans
```

The PostgreSQL container has disposable storage; removing it resets the synthetic database. A deliberate source/model/lock change requires reviewing the diff and explicitly running `node scripts/update-reference-projects.mjs`, then rerunning checks. Restart the application containers after editing maintained source (`docker compose -p lekalo-reference-109 -f examples/compose.yaml restart node laravel laravel-peer`): Node does not hot-reload a bind-mounted file. Schema/seed changes require the documented disposable-runtime cleanup and a fresh initialization. The updater never runs as part of verification. Never refresh a lock merely to hide unexpected drift.
