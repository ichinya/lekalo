# Issue #60: Laravel routes, request/response bindings and the OpenAPI projection — research and implementation plan

## Scope and evidence

Research, branch `ichinya/m5-issue-60`, base `9501f628` (the tip of
`ichinya/m5-issue-59`). The issue body was retrieved with
`gh issue view 60` (no comments, depends on #25/#29/#54/#58/#59/#70):
link the Lekalo endpoint contracts to the Laravel routing/HTTP kernel
without placing business logic in controllers. All seven acceptance
criteria are mapped in the final section. Paths are repository-relative;
**proposed** marks new work; everything else was inspected on this base.

Read: `docs/m5/issue-59-research.md` (the operations family this issue
mirrors), `docs/php-laravel-operations.md`, ADR 0045, and the #59
implementation commits `7b9665a9` (core join, contracts, admission) and
`ad74d6db` (the join runs before any adapter exchange).

Recommendation: do not build a second route authority. The
`transport-http` attachment (#70) already owns the whole wire surface and
projects a canonical route surface per runtime; the new PHP Laravel
routes family must consume that evidence through the same composed
evidence path #58/#59 established, add only per-route custody metadata,
and emit thin wrappers plus the OpenAPI projection as one deterministic,
digest-bound plan.

## 1. What already exists (the authorities this family joins)

- **Endpoint authority — the Model.** `Definition::Endpoint` in
  `crates/lekalo-core/src/ir/` carries `id`, `method`, `path`,
  `invokes` (the command/query symbol). Nothing else may restate them
  (`transport-http` deliberately does not).
- **Wire authority — the transport attachment (#70).**
  `contracts/transport-http.schema.v0.4.0.json`, source home
  `lekalo/transport.yaml`
  (`transport_http::source::SOURCE_PATH`). It binds parameters with
  locations and field refs, the body mode (`whole-input`/`explicit`),
  the success projection, the explicit per-error status map plus
  `errorDefaults` (validation/auth/conflict/not-found/domain/
  infrastructure), security schemes over the authorization actor
  vocabulary, idempotency/correlation headers, pagination, rate limit,
  cache, API version, capabilities, and scenario references.
  `transport_http::project::project(document, context, "laravel")`
  renders the canonical `RouteSurface` whose laravel handler identity is
  `{ProjectPascal}/{module}/{tail}Controller` (unit-tested in
  `project.rs`).
- **Orchestration evidence path.** `orchestration/generate.rs` already
  stages, per generate run, under `.lekalo/cache/`:
  `ir/<project>.json` (canonical IR bytes), `transport/<project>.json`
  (canonical attachment bytes of the validated `lekalo/transport.yaml`),
  and `openapi/<project>.json` (the #46 canonical OpenAPI render of the
  same validated join, with the embedded #62 error registry bound).
  These three staged documents are the only transport/OpenAPI input an
  adapter may read.
- **OpenAPI projection (#46).** `openapi::render` is core-owned and
  deterministic; the node adapter serializes it to YAML. The document
  self-pins `x-lekalo-provenance.generator = lekalo-core/openapi@0.4.0`.
- **The #58/#59 family pattern.** Types and operations each have: a
  closed v0.4.0 input contract under `lekalo/<home>/`, a core-side join
  module that is the acceptance authority before any adapter exchange
  (`php_operations::check_join`, wired in `generate.rs` as
  `run_operations_join` → registered `php-operations.join-invalid`
  diagnostic `LEK-OPS-001`), an adapter mirror validation, a pure mapper
  + emitter pair, a custody sidecar (`operations.map.json`), a
  deterministic `classmap.php` loading authority, four custody modes,
  and an empirical harness (`scripts/test-php-laravel-operations.mjs`)
  that drives the real one-shot adapter exchange.
- **Capability registry.** `generate.transport-http` is already defined
  ("Generates the HTTP route layer from the transport-http evidence")
  and currently declared `unsupported` by the PHP kernel;
  `verify.transport-http` is black-box scenario execution — explicitly
  not this family. #59 added its own `generate.operations`/
  `verify.operations` pair rather than stretching existing ids.
- **Node precedent.** `adapters/node-typescript/src/transport-extension.mjs`
  already renders route declarations from the transport evidence joined
  with the compiled IR (refusing unjoined endpoints) and states the
  canonical error envelope: `{ok:false, error:{id, code, category,
  payload}}`, public payload fields only.
- **The planner fixture.** `tests/fixtures/php-laravel/planner/` boots a
  hand-constructed Laravel application (`app/application.php`) with the
  route group registered in `$app->booted(...)`,
  `POST /api/tasks/{task_id}/focus` → `TaskFocusController::focus` — a
  controller that itself does Eloquent reads/writes and fires an event.
  `Tests\Support\PlannerPort::invoke` dispatches scenarios through the
  real HTTP kernel. The scenario/parity gates (4 committed scenarios)
  run against this surface and must stay green.

The gap: nothing links the endpoint/wire authorities to Laravel routing
artifacts. The fixture's only HTTP route is hand-written, and its
controller carries exactly the business logic issue #60 forbids there.

## 2. Proposed family: `php-routes` (custody metadata, not a second wire authority)

**New closed v0.4.0 contracts** (product version of the landing commit):

1. `php-routes-input.schema.v0.4.0.json` — `lekalo/php-routes-input/v0.4.0`,
   authored at `lekalo/routes/<project>.routes.json` (new canonical home
   registered in `project_fs.rs::CANONICAL_ROOT_ENTRIES`). Closed
   members: `schemaVersion`, `identity`, `projectId`, `irDigest`,
   `transportDigest`, `typesInputDigest`, `operationsInputDigest`,
   `policy { namespacePrefix, middleware? }`, `routes[]`. A route record
   is `{ id, operation, mode, entry? }`: `id` is the Model endpoint
   symbol; `operation` is the invoked command/query symbol; `mode` is
   `managed` (generated thin wrapper) or `checked` (existing route
   verified against scanner evidence, `entry {fqn, method}` required).
   Method, path, parameters, body, errors, statuses, security, headers,
   scenarios stay single-sourced in Model + transport — the input never
   restates them. `middleware` optionally maps a declared security-scheme
   id to the project's own middleware spelling; without it no middleware
   is attached (the framework default is never guessed).
2. `php-routes-map.schema.v0.4.0.json` — the custody sidecar
   `routes.map.json` (the `operations.map.json` precedent, same
   `.map.json` neutrality in the source-map ingester): identity, adapter
   pin, input digests `{ir, transport, input, operationsInput, openapi}`,
   id-sorted route entries (`operationId`, `method`, `pathTemplate`,
   `name`, controller/request FQN+path, entry FQN/method, the declared
   error→status table, auth binding, scenario and endpoint links, the
   OpenAPI pointer/digest), and the path-sorted artifact inventory with
   digests. This sidecar is the exported route/operation/symbol/scenario
   link table the acceptance criteria require.
3. `php-routes-evidence.schema.v0.4.0.json` — the scanner-evidence
   document the checked mode consumes,
   `.lekalo/import/observed/routes-evidence.json`: producer receipt,
   source inventory with digests, and observed route rows
   (`method`, `uri`, `name`, `action`, `middleware`). Shape joins only,
   exactly like #59's operations evidence: the document validates the
   producer receipt reference but never certifies producer execution —
   that proof belongs to the real `artisan route:list --json` lane.

**Diagnostic registry:** one new rule `php-routes.join-invalid`
(`LEK-RTE-001`, starting a new LEK-RTE family — cleaner than stretching
LEK-OPS, mirroring `php-operations.join-invalid` 1:1). The registry is
additive, so the hardcoded entry-count gate in
`scripts/test-classification-contracts.mjs` grows by exactly `+1` with a
comment naming the routes family.

**Capabilities:** new definitions `generate.routes` and `verify.routes`
(definition version 0.4.0, issue #60), added to the closed id-sorted
table in `target_protocol/capability.rs` and its pinned list test; the
PHP kernel declares both `partial`. `generate.transport-http` stays
`unsupported` for this adapter (the family is custody-input-driven; the
bare transport projection remains the Node adapter's shape).

## 3. Proposed core join (`crates/lekalo-core/src/php_routes/`)

Mirrors `php_operations` exactly in shape and strictness:

- `parse_input` — closed member sets everywhere (unknown members are
  authoring errors), canonical id-sorted unique route order, bounded
  grammars for ids/FQNs/digests.
- `check_join(root, input, compilation)` — every route record joins
  against the owned authorities only: the staged IR evidence and the
  bound types/operations input documents by exact digest
  (`routes.ir-digest`, `routes.types-unbound`,
  `routes.operations-unbound`); the transport attachment parsed from
  `lekalo/transport.yaml`, canonicalized, and digest-compared
  (`routes.transport-digest`), with per-record endpoint resolution
  (`routes.endpoint-unresolved`) and transport binding resolution
  (`routes.transport-unbound`); the invoked operation must be a compiled
  command/query (`routes.operation-unresolved`); `checked` requires
  `entry` (`routes.entry-missing`), `managed` forbids it
  (`routes.entry-declared`). Absence is never read as a default.
- Wired in `orchestration/generate.rs` right after the operations join,
  same pattern as `ad74d6db`: a present `lekalo/routes/<project>.routes.json`
  must join before any adapter exchange; a refused join is the
  registered `php-routes.join-invalid` invalid result with the typed
  finding codes in its data.

## 4. Proposed adapter modules and emitted surface

`src/route-policy.php` (constants + closed input validation),
`src/route-map.php` (pure mapper), `src/route-emit.php` (pure emitter),
loaded in that order by the kernel and `build.php`. The mapper joins the
same staged evidence the core staged — IR evidence, canonical transport
evidence, OpenAPI evidence — re-checking identity digests
(`routes.transport-ir-mismatch` when the attachment's `irRef` disagrees
with the staged IR bytes; `routes.openapi-unbound` when the OpenAPI
evidence is absent). The composed run requires the operations input and
runs the #59 operations family in the same plan: a route wrapper without
its handler would be dead code.

Emitted under `.lekalo/generated/php-laravel/routes/` (namespace
`Lekalo\Generated\Routes`):

- `routes.php` — one registration per managed route: method, uri (path
  template with `{param}` placeholders, the Laravel spelling),
  `[Controller::class, 'action']`, `->name($operationId)`, and only the
  input-declared middleware. No discovery, no attributes, no glob: every
  line traces to one input record joined with scanner-grade evidence
  (IR + transport). This file is the whole registration surface —
  `routes/api.php` and every manual route stay untouched.
- `<module>/<tail>Controller.php` — thin wrappers. Constructor-injected
  handler + map dependencies (resolved by the Laravel container from
  bindings the application declares), exactly one public action per
  route named by the operation id: decode (path params + JSON body per
  the decode plan, building the #58 typed input), bind the actor, call
  the operation entrypoint, encode the success envelope, catch the
  declared typed `OperationError` subclasses and map each through the
  route's declared table. **No catch-all 500**: only declared domain
  errors and the validation refusal are converted; any other throwable
  propagates to the application's exception handling.
- `HttpEnvelope.php` + `ErrorHttpMap.php` — the canonical-v1 envelope
  encoder (`{ok:false, error:{id, code, category, payload}}`, public
  payload fields only) and the explicit per-route error→status tables
  with the declared category defaults. These two are the entire
  transport-boundary contract surface; no business logic exists in
  generated HTTP code.
- `openapi.json` — the staged OpenAPI evidence bytes, verbatim and
  digest-bound. Code and document are projections of one validated join,
  so a changed attachment re-projects both; `verify` compares the
  emitted digest against the freshly staged bytes (`routes.openapi-drift`),
  making code+OpenAPI drift a typed finding, never a silent divergence.
- `classmap.php` — the deterministic loading authority (`php -n`, no
  Composer); `routes.map.json` — the custody sidecar.

Ownership-aware merge: the application merges by requiring the generated
`routes.php` (one maintained line in the fixture's bootstrap); manual
routes keep their own file and registration order, and the generated
plan never writes outside `.lekalo/generated/php-laravel/routes/**`.
Verified routes (checked mode) join declared surface (method, uri, name,
action, controller source digest) against the observed routes evidence;
absent/stale evidence is the typed `routes.binding-*` finding with zero
writes, mirroring the #59 checked join.

## 5. Fixture and harness

New corpus `tests/fixtures/php-laravel/routes/`: a planner model with
`planner.today` (query, returns `list<planner.task>`) and the existing
`planner.focus_task` command, endpoint bindings `planner.endpoint_today`
(`GET /today`) and `planner.endpoint_focus_task_by_id`
(`POST /tasks/{task_id}/focus`, explicit body field `task_id`), plus
`lekalo/transport.yaml` binding both endpoints, and input templates for
types/operations/routes. The IR evidence is compiled once from the model
with the CLI and committed; every digest rides as a zero placeholder the
harnesses replace.

Planner fixture (Laravel app) gains only maintained integration code: a
service-provider binding the generated ports to Eloquent-backed
maintained adapters, an authentication middleware + exception mapping
for the unauthorized case, the one-line ownership-aware require of the
generated routes file, and a `planner.today` port implementation. The
existing manual `/api/tasks/{task_id}/focus` route and its controller
are untouched (they are the "manual routes outside managed scope" the
acceptance criterion protects; the generated surface registers
`/today` and `/tasks/{task_id}/focus` at the root, no collision).

New gate `scripts/test-php-laravel-routes.mjs` in the
`test-php-laravel-operations.mjs` style — empirical stages over the real
adapter exchange: composed custody with dry-run/apply byte equality;
byte stability across roots; the thin-controller surface (exactly one
public action, no Eloquent/facade/business constructs) that
syntax-checks and loads under `php -n`; the emitted error→status table
equality with the declared transport map; checked-mode absent/conforming/
stale evidence; manual-scope immunity of the plan; OpenAPI drift; and a
real Laravel runtime stage (the scenario-tests provisioning contract:
committed `composer.lock`, `--no-scripts`) driving actual HTTP requests
through the generated surface — Today 200, focus happy path, missing
task 404, conflict 409, malformed body 400, unauthenticated 401 — each
asserted against the canonical envelope and the OpenAPI document.

## 6. Acceptance mapping

1. Planner Today/focus endpoints work in the Laravel fixture — §5
   runtime stage through the real kernel.
2. Request/response/errors match OpenAPI and Lekalo types — one join
   projects both; the harness compares envelope shape/error ids/statuses
   against the declared tables and the emitted `openapi.json`.
3. Unauthorized/validation/domain mappings covered — 401 middleware
   path, 400 validation refusal, 404/409/403/503 declared error table.
4. Existing route checked in contracted mode — `checked` route record +
   observed routes evidence join.
5. Controller has no undeclared business effects — thin-wrapper surface
   stage; the wrapper only decodes/delegates/encodes.
6. Route/operation/symbol/scenario links exported — `routes.map.json`
   link table (endpoint symbol, operation id, controller/request symbols,
   scenario refs, OpenAPI pointer).
7. Manual routes outside managed scope untouched — plan-scope stage;
   `routes/api.php` byte-identical before/after.

Research validation: live issue retrieval, source/schema/CI inspection on
this base, the #59 implementation read end-to-end, CLI compile of the
operations fixture model. No runtime suite was executed for this
document; every gate above is an implementation requirement of the
following commits.
