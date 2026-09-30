# Issue #115 — Hono framework bindings: implementation report

Branch `ichinya/m6-issue-115`, milestone M6. This document maps every
acceptance criterion of [issue #115](https://github.com/ichinya/lekalo/issues/115)
to its implementing code, fixtures, and executable evidence, and marks
every partial or unsupported item explicitly. Companion research:
`docs/m6/issue-115-research.md`.

## Fix round (PR #142 review)

Both independent reviewers returned BLOCK on PR #142. Every blocker,
major, and minor finding is fixed; the rules revision is bumped to
`hono-rules-v2` (additive vocabulary + classification changes; no wire
contract change). Per finding:

- **B1 — reachability.** `collectRegistrations`/`makeEvent` classified
  no control flow, so `if (cond) app.get(...)` and
  `function setup(){ app.get(...) }` were emitted as complete facts.
  Every registration site is now classified: `top-level` straight-line
  and `called` (a named local function the same module invokes
  straight-line at top level, hoisting/TDZ respected) stay complete;
  `conditional` sites emit `incomplete` + `conditional-registration`;
  unproven `deferred` bodies emit `incomplete` +
  `deferred-registration`; sites after a same-level `return`/`throw`
  emit `unknown` + `unreachable-registration`. Mounts-router records
  keep `incomplete` instead of collapsing to `unknown`, and error/
  middleware records inherit the same penalty.
  (`reachability/` fixture; scanner suite.)
- **B2 — ancestor prefixes.** `resolveMount` recomputed the parent base
  from `standaloneBaseOf` only and recursed without the enclosing
  prefix; nested mounts now thread the full ancestor base chain
  (`app.route('/api2', api2)` + `api2.route('/v1', v1)` resolves
  `/api2/v1/...`), the depth refusal uses the canonical
  `HONO_MAX_MOUNT_DEPTH` bound, and the scanner test no longer codified
  prefix-dropped paths (`composition/` gained a depth-3 mount).
- **M1 — shared parent multi-mount.** Scope keys were
  `mount:module:order`, so a shared parent mounted twice collided: the
  second ancestor chain dropped shared grandchild routes and the same
  nested mounts-router event re-emitted byte-identical duplicate
  records. Resolution scope, dedupe keys, and middleware ancestry are
  now keyed by the full ancestor chain, and mounts-router is emitted
  once per mount occurrence event (`hub` fixture mounted at `/hub1`
  and `/hub2`; zero `hono-invalid-record` uncertainty asserted).
- **M2 — use() filter positions.** A non-literal first `use()` argument
  was resolved through `resolveHandlerChain`, fabricating
  `uses-middleware app -> adminPrefix` records from an indexed string
  const. The filter position is now decided by the compiler (literal,
  resolvable const alias, string/RegExp/string-array type, or
  unknowable any/unknown); resolvable aliases behave exactly like
  literals, and unresolvable filters stay `unknown` with
  `dynamic-path-filter` (new reason + uncertainty) while handlers bind
  from the correct argument index. Middleware covered by an unresolved
  filter stay `incomplete` (conditional-applicability +
  dynamic-path-filter).
- **M3 — path-filtered parent middleware.** Only the mount parent's
  pre-mount GLOBAL `use` events composed, so `app.use('/api/*', auth)`
  over `app.route('/api', api)` produced zero records on mounted
  routes. The chain now walks the full mount ancestry (outermost first,
  each ancestor's pre-mount uses proven by same-module order) and
  matches every parent path filter against the route's full resolved
  path: literal disjoint filters are provably excluded, wildcards stay
  conditional-incomplete (`middleware-mounts/` fixture).
- **M4 — test bindings through mounts.** `app.request` matched only
  `route.instance.key === instance.key`, so tests on a mounting app
  could never bind a mounted route. Resolved routes carry their root
  instance; matching considers the full ancestry and inherits the
  route's status/reasons (a bound-but-incomplete route never claims a
  complete flow) (`tests/` fixture: `app.request('/sub/other')`).
- **Minors.** (1) `readBytes` ignored its per-call limits argument, so
  the 64 KiB per-file bound on declared data reads never reached the
  read view — per-call bounds are translated into the view's cumulative
  counters (test grows the contract file past the bound and asserts a
  reasoned refusal, never an unbounded parse); (2) endpoint-contract
  records carry the JOINED contract method instead of `methods[0]`
  (multi-method `on` route joins a POST contract in the static
  fixture); (3) spans align with the research spec: half-open UTF-8
  byte offsets (`startByte`/`endByte`) converted from the compiler's
  UTF-16 positions against exact source bytes, proven exact against
  file bytes via a multibyte fixture anchor; (4) dead bounds removed:
  `HTTP_VERBS` deleted, `HONO_MAX_MOUNT_DEPTH` wired into the mount
  refusal, `HONO_MAX_CHAIN` enforced with explicit `chain-budget`
  uncertainty on overflow; (5) `validateHonoRecords` rebuilt records
  without the stored `framework` object so any non-default framework
  version failed its own fingerprint — the rebuild consumes it; (6)
  bare `request(...)` helpers no longer fabricate `unknown-test-app`
  uncertainty; (7) `next()` evidence resolves the middleware's own
  continuation parameter symbol instead of any identifier named
  `next` (shadowed-import fixture proves `next=absent` while real
  pass-through stays `next=detected`); (8) createRoute/openapi records
  name the failed axis (`dynamic-method` vs `dynamic-path` vs the new
  `operationid-unknown`) and emit reasoned declaration-side records
  instead of silence; (9) non-literal `use()` filters were covered by
  M2. One latent envelope defect found during the fix round: the
  canonical record order was fingerprint-byte noise — `honoCompare`
  now orders on record content.

## What shipped

The node-typescript adapter gained a Hono framework evidence provider:
`adapters/node-typescript/src/hono-{evidence,context,scanner,routes,middleware,http,tests,bindings}.mjs`,
vocabulary in `adapters/node-typescript/hono-compatibility.json`, a
trusted launch input `--lekalo-framework-policy-json` in the kernel,
and the fixture family `tests/fixtures/node-typescript-scanner/hono`
(synthetic authored declaration stubs; no real package is installed,
read, or executed at scan time).

Scope items delivered (issue "Scope" → module):

| Scope item | Module(s) |
| --- | --- |
| Application/router composition detection | `hono-scanner.mjs` (constructor resolution through the reserved import specifier plus an inventoried `.d.ts` class declaration; immutable const aliases; `basePath` views; bare `new Hono()` receivers) |
| Route method/path/handler bindings | `hono-routes.mjs` (verb methods, `all`, `on` with literal method arrays; literal/template/concat/const-alias paths; per-occurrence chain endpoints; inline occurrence keys + content digests) |
| Nested routers / base paths | `hono-routes.mjs` (`route()` mounts with mount-time snapshot semantics; transitive ESM import-closure ordering; shared children per mount occurrence; cycle/depth refusal) |
| Middleware chain + order | `hono-middleware.mjs` (global/path-filtered `use` + inline middleware; entry `ordinal` and `unwindOrdinal`; `next()` detection; conditional applicability) |
| Auth/tenant/context middleware evidence | `hono-middleware.mjs` (explicit `@lekalo-*` JSDoc roles; `c.set`/`c.get`/`c.var` literal-key records) |
| Request validation / schema bindings | `hono-routes.mjs` + `hono-http.mjs` (`zValidator`/`validator` targets, schema symbols resolved through alias chains, inline schema digests) |
| Response / error mapping | `hono-http.mjs` (`c.json/text/html/body/render/status` sites with statuses; thrown `HTTPException`; `onError`/`notFound`) |
| OpenAPI operation links | `hono-routes.mjs` (`createRoute({method, path, operationId})` + `OpenAPIHono.openapi(definition, handler)`) |
| Handler → service/command/query references | `hono-bindings.mjs` (`handler-call` records with exact from/to native ids via `getResolvedSignature`) |
| Test bindings | `hono-tests.mjs` (`app.request`, `testClient` identity, client verbs, describe/it scopes) |
| JSX/SSR routes marked separately | `hono-http.mjs` (`api/html/ssr/mixed/unknown` facet per route; `.tsx` alone is never SSR) |

Evidence model (issue "Evidence model" → record fields, all in
`hono-evidence.mjs`): semantic/native symbol (`from`/`to` with module,
native id, name, indexed flag, structural signature, content digest);
relation kind (closed `dev.lekalo.hono/…` vocabulary); source span
(1-based lines/columns plus half-open UTF-8 byte offsets converted from
the compiler's UTF-16 positions, on the logical project path); source revision
(profile provenance revision); provenance (`explicit`/`detected`/
`inferred` — `confirmed` is deliberately absent because a scanner
cannot mint confirmation); confidence (`exact|high|medium|low|unknown`,
never percentages); adapter + framework version and rules revision
(`hono-rules-v2`); freshness fingerprint (domain-separated SHA-256 over
the whole record — registration identity/order/path, handler and
middleware identity, structural signature AND body digest, mount
chain, facet, span).

Issue constraints honored:

- Dynamic route construction is never "fully known": dynamic
  paths/methods/contexts/statuses become `unknown`/`incomplete` records
  with closed machine reasons (`HONO_REASONS`).
- Middleware presence never proves authorization: the provider emits no
  authorization relation; roles are explicit annotations only; a test
  asserts no `authorizes` relation can exist.
- Context keys stay namespaced implementation evidence (`note`), never
  canonical domain fields.
- The scanner never edits source: asserted by
  `scripts/test-node-hono-readonly.mjs` and the process test
  (full-file inventories before/after three scans per fixture).
- No regex-only claims where compiler information exists: all
  recognition runs through the vendored TypeScript checker
  (symbol resolution, alias chains, resolved signatures); string
  matching appears only on the reserved import *specifier* spelling,
  paired with compiler-resolved class identity.
- Hono relations stay namespaced adapter evidence
  (`dev.lekalo.hono/…`); the closed wire contract, core schemas, and
  core vocabulary are untouched.
- Hono support is disableable without breaking the generic adapter:
  byte-identity is asserted in `hono-scanner.test.mjs` and exercised
  through the wire in `hono-process.test.mjs`.

## Acceptance criteria matrix

| Issue acceptance criterion | Status | Evidence |
| --- | --- | --- |
| Scanner связывает static Hono routes с handler symbols и HTTP endpoint contracts | **Implemented** (adapter-side; Model/IR-verified join partial, see below) | `route-handler` records bind exact native handler ids: `test/hono-scanner.test.mjs` "static fixture"; endpoint contracts join declared `lekalo/endpoints.json` data by unique compatible method+path: `test/hono-http.test.mjs` "endpoint contracts"; ambiguity (`GET /users` duplicated entry) and gaps (`/v1/status`) stay reasoned `incomplete`. The join is `inferred`/`medium` — a shape candidate, never a confirmation; the IR/transport-verified join requires the successor-contract work of research S2/S5 (out of scope here, coordinated contract change). |
| Nested routers/base paths разрешаются детерминированно | **Implemented** | `test/hono-scanner.test.mjs` "composition": app → /api → /v1 (×2 shared child), /shared, `basePath` view; v1.ts pre-mount routes `complete` via the proven import closure; post-mount registration `incomplete` with `post-mount-registration`; cycle fixture refuses the cyclic mount as `unknown` (`composition-cycle/src/{app,b}.ts`). Cold == warm byte parity asserted. |
| Middleware order виден в inspect/impact/context | **Partial** | The adapter emits ordered chains (entry `ordinal`, reverse `unwindOrdinal`, `next()` evidence, conditional applicability) with stable serialization: `test/hono-middleware.test.mjs`. Surfacing inside `inspect`/`impact`/`context` additionally requires the neutral framework-evidence transport, the generic graph overlay, and the query consumers — exactly the coordinated contract successors of research S2/S5 (closed wire `role` vocabulary, observed schema, graph model). That core-side work is **not** part of this dispatch and is not claimed; the ordered adapter evidence is the deliverable this issue's provider can honestly produce today. |
| Dynamic/ambiguous routes маркируются incomplete/unknown, а не угадываются | **Implemented** | `test/hono-scanner.test.mjs` "uncertainty" (dynamic concatenation → `unknown` + `dynamic-path`; dynamic `on` methods → `dynamic-method`; factory receivers → `unsupported-receiver`; mutable aliases refused); cycle mount → `unknown`; ambiguous endpoint join → `incomplete` + `ambiguous-endpoint-join`. |
| API и SSR routes различаются | **Implemented** | `test/hono-http.test.mjs` "SSR": `/api/ping` → `api`, `/page` + `/render` → `ssr`, `/mixed` → `mixed`, `.tsx`-but-JSON → `api`; plain-string `c.html` → `html` (http fixture). Endpoint joins refuse API claims for SSR-classified routes (`ssr-api-conflict`). |
| Route signature/source change инвалидирует stale evidence | **Implemented** (adapter-side; core-side stale recheck partial) | `test/hono-scanner.test.mjs` "freshness": a handler **body-only** edit changes the route record's fingerprint and handler digest while unrelated records stay stable; signature/order/path/span edits are fingerprint inputs by construction (unit-tested in `hono-evidence.test.mjs`). The persisted observed-index stale-state machinery (core-owned recheck of stored evidence) is successor work (research S2.2/S5.4); this dispatch ships the invalidation-carrying evidence, not the core-side recheck loop. |
| Fixture и один private brownfield consumer проходят read-only scan без source mutation | **Partial** | Fixture side: `scripts/test-node-hono-readonly.mjs` — complete inventories (including every file's bytes) before/after three scans (enabled, disabled, repeat) across four fixture projects are byte-identical; plus the wire-level process test asserting the read-only invariant. Private brownfield consumer: **not performed** — no authorized repository role, snapshot, or type roots were supplied in this dispatch; per research S6.5 it must run under qualified confinement with an admission receipt and is explicitly left as the remaining acceptance item. |
| Hono support можно отключить без нарушения generic TypeScript adapter | **Implemented** | `test/hono-scanner.test.mjs` "provider disabled or absent": enabled-minus-`frameworks` bytes == disabled bytes; absent policy behaves like disabled; wire-level scans with enabled/disabled policies terminate honestly (`hono-process.test.mjs`); the generic kernel/scanner/transport/scenario suites all pass unchanged. |

## Constraint & honesty controls (adversarial coverage)

- Lookalikes: local `class Hono`, router-shaped modules, shadowed
  imports → zero apps (`lookalike/` fixture + test).
- Unresolved constructors and cycle-mounted targets → `unknown`
  records with spans, never invented applications or routes.
- Duplicate/overlapping endpoint contracts → `ambiguous-endpoint-join`.
- Malformed/tampered framework policies → bounded launch refusals
  (`framework-policy` stderr diagnostic, nonzero exit).
- Record/uncertainty budgets (4096/1024) with explicit
  `record-budget` partial states; overflow is never silent.
- The envelope validator rejects unknown relations, `confirmed`
  provenance, percentage confidences, unknown reasons, and any
  fingerprint tampering (`hono-evidence.test.mjs`).

## Gates run (all green)

- `node adapters/node-typescript/build.mjs --check` (deterministic
  rebuild byte-parity) and `node scripts/regen-adapter-manifest.mjs`
  (committed artifact + manifest regenerated; the Rust exemplar guard
  in `crates/lekalo-core/src/adapter_package/manifest.rs` tracks the
  new canonical package digest).
- `node scripts/test-node-hono-bindings.mjs` — 38 tests across 5 suites
  (envelope, scanner/composition/disablement, middleware, HTTP/tests/
  joins, process/policy/readonly).
- `node scripts/test-node-hono-readonly.mjs` — read-only inventories.
- `node scripts/test-node-typescript-{kernel,scanner,transport}.mjs`,
  `test-node-native-gates.mjs`, `test-node-scenario-units.mjs`,
  `test-node-scenario-tests.mjs`, `test-node-openapi.mjs`,
  `test-adapter-manifest-golden.mjs`, `test-adapter-manifest-contracts.mjs`
  (Ajv 8.17.1), `test-target-protocol-contracts.mjs` (Ajv),
  `test-fixture-provenance.mjs`, `check-contract-versions.mjs`,
  `test-contract-versions.mjs`, `check-structure.mjs`,
  `check-authority.mjs`, `check-privacy.mjs`, `check-model.mjs`.
- `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
  --locked -- -D warnings`; `cargo test --workspace --locked`
  (89 binaries, 0 failures).
- CI: `.github/workflows/ci.yml` gains the two issue gates
  (`test-node-hono-bindings`, `test-node-hono-readonly`) after the
  scanner suite.

## Remaining for full acceptance (explicit, not claimed)

1. **Private brownfield consumer scan** — requires the coordinator-
   supplied authorized role, snapshot, lock/type roots, and policy
   (research S6.5); must produce an admission receipt. Not executable
   in this repository dispatch.
2. **Contract successors for core transport** — publish the
   framework-evidence member in successor target-protocol/observed
   contracts, the generic graph overlay, and the inspect/impact/context
   consumers (research S2/S5). These are coordinated cross-cutting
   contract changes (closed wire vocabularies and version registries)
   deliberately not attempted unilaterally here; the adapter side is
   ready to project into them.
3. **Runtime oracle qualification** — qualify mount/clone/order and
   `strict`/custom-`getPath` semantics against a pinned Hono runtime
   (research S1.3); until then the provider stays marked `partial` in
   `hono-compatibility.json` and mount ordering follows the documented
   snapshot rule with proven static initialization order.
