# Issue #72 — typed client SDK generation: research and implementation plan

## Scope and evidence

- Authority: `gh issue view 72 --repo ichinya/lekalo`, read first on 2026-09-26; [issue #72](https://github.com/ichinya/lekalo/issues/72), dependencies #21/#27/#62/#70.
- Inspected branch `ichinya/m5-issue-72`, baseline `e15ac298148060e55446832bec60ed00b148f640`; initial worktree clean; workspace version `0.4.0`.
- This commit changes documentation only. Paths/symbols below describe this checkout; **new** denotes proposed work, not an existing API.
- Ship TypeScript usable from Vue/web, Go and PHP from one contract; Rust emission is explicitly later. Client generation must not define server behavior.
- Evidence run: `node scripts/test-node-typescript-transport.mjs` — 6/6 passed. This proves the existing route-generation gate only; SDK acceptance, native SDK compilation and full conformance have not run.

## 1. Existing implementation map

| Area | Files / symbols | Reuse and limitation |
| --- | --- | --- |
| Typed model | `crates/lekalo-core/src/ir/mod.rs`: `CompiledProject`, `Definition`, `TypeRef`, `ScalarBase` | Endpoint owns method/path/invokes; command/query owns input/output. `Optional(T)` is nullability, independently of field `required`. No decimal scalar base. |
| HTTP contract #70 | `contracts/transport-http.schema.v0.4.0.json`; `src/transport_http/{types,validate,mapping,project,diff}.rs` under core | `TransportDocument`, `EndpointBinding`, `ValidationContext`, `validate`, `project`, `binding_json`, `compare`, `DiffClass`; explicit parameters, body projection, auth, headers, pagination, stable operation ID. |
| HTTP validation | `transport_http/validate.rs`: `ValidationContext::{with_errors,with_query_model,with_capabilities,strict}` | Cross-family validation is conditional on supplied context. SDK generation must bind all required context, not call the bare constructor and assume completeness. |
| Error contract #62 | `src/error_contract/{registry,types,result,normalize}.rs`; `contracts/error-registry.v0.2.16.json` | `ErrorRegistry::{from_bytes,embedded,binding,error}`, `ErrorContract`, `ErrorUnion`, `RetryPolicy`, `RetryCondition`, `Idempotency`, `EffectClass`; public payload projection and immutable id/code. |
| Query contract | `src/query_model/`; `tests/fixtures/transport-http/query-model.json` | Query parameters and page/stream semantics are separate from IR; needed for a faithful request signature and pagination helper. |
| OpenAPI | `src/openapi/{render,schema,check,bind,diff}.rs`: `render`, `SchemaMapper::{map_type,map_error_type,object_schema}`, `check` | Useful mapping/check precedents, not the SDK source of truth. Partial projections must not turn into complete client types. |
| Generation/apply #27/#91 | `src/orchestration/generate.rs`: `GenerateRequest`, `generate`; `src/target_protocol/{plan,transport,scopes,wire}.rs`: `TargetClient` | Validated evidence, confined adapter, dry-run/apply plans, lock/manifest binding. `generate.rs` currently supplies IR, transport and OpenAPI evidence; no SDK evidence. |
| Adapter | `adapters/node-typescript/src/{generation-composite,transport-extension,openapi-gen,zod-gen}.mjs` | One composed `generate` operation; `unionOutcomes` binds all writes. Only shipped adapter is Node/TypeScript. No SDK generator or client-specific capability. |
| Important OpenAPI gap | `adapters/node-typescript/src/openapi-gen.mjs`: `openapiGenerateOperation`, `renderDocument` | Reads transport + IR, explicitly reports partial because error registry/query model are absent. Do not generate SDKs from this incomplete document. |
| Ownership #21 | `src/artifacts/{types,check,source_map}.rs`: `ArtifactManifest`, `GenerateService`; `contracts/artifact-manifest.schema.v0.2.16.json` | Lifecycle, digest drift, safe clean, semantic byte ranges. Existing source kind can represent SDK code. |
| Source-map integration | `src/orchestration/generate.rs`: `source_map_binding_for`, `module_path_of`, `artifact_kind_for` | Reads `declarations[{id,start,end}]`; `module_path_of` hardcodes the sibling `.ts`. Go/PHP maps need an explicit, validated artifact binding. |
| Impact | `src/impact/{analyzer,input,mod,canonical,risk,gate}.rs`: `analyze`, `NamedItem`, `ChangedInputSet` | Existing graph/effect traversal and target impact. `analyzer.rs` emits `artifacts: unknown_summary(), items: []`; no SDK consumer catalog or wire-attachment-to-client join. |
| Conformance | `src/target_protocol/{capability,conformance}.rs`; `docs/adapter-conformance.md` | Shared protocol battery and transport checks exist; no SDK checks. Skipped checks are not passing evidence. |

`src/` in the table means `crates/lekalo-core/src/`. CLI integration is in `crates/lekalo-cli/src/main.rs` (`run_generate`, `run_openapi`, `openapi_check`) and its impact edge modules.

Existing tests/fixtures to extend, rather than creating a second planner domain:

- Core: `crates/lekalo-core/tests/{transport_http,openapi_render,error_contract,artifact_manifest,impact,adapter_conformance}.rs`.
- CLI: `crates/lekalo-cli/tests/{generate,impact}.rs`; inspect the existing orchestration tests before adding lifecycle probes.
- `tests/fixtures/transport-http/{project,valid/planner.transport.json,query-model.json,diff,invalid,projected,scenarios}`; `transport_http.rs::full_context` demonstrates the complete join.
- Planner fixture operations include `planner.focus_task`, `planner.count_focused`, `planner.list_tasks`, `planner.tasks_by_project`; endpoint definitions are in that project's `bindings.yaml`.
- `tests/fixtures/openapi/{valid,invalid,merge,diff,trace}` and `tests/fixtures/artifacts/wire/{valid,invalid}` supply reproducibility/ownership precedents.
- Scripts: `test-transport-contracts.mjs`, `test-openapi-contracts.mjs`, `test-node-typescript-transport.mjs`, `test-node-openapi.mjs`, `test-node-zod-gen.mjs`, `test-artifact-manifest-contracts.mjs`, `test-impact-contracts.mjs`.
- `.github/workflows/ci.yml` already separates schema gates, Rust tests, adapter bundle rebuild, native gates and conformance. Preserve that separation.

## 2. Gap against every acceptance criterion

| Acceptance criterion | Current gap | Required proof |
| --- | --- | --- |
| Vue client for planner endpoints compiles and passes contract tests | No client functions, transport interface or Vue consumer fixture | Generate TS; strict TS and Vue SFC compilation; execute request/response/error tests against fake transport and a local fixture HTTP server. |
| Error union preserves semantic codes | Core registry exists, but adapter OpenAPI is partial and no SDK union exists | Literal id/code/category variants and public payload types match registry exactly; unknown/malformed failures never masquerade as a declared error. |
| Changed endpoint impact identifies SDK artifacts/consumers | Artifact section is unknown/empty; consumer dependencies and transport-only change roots absent | Exact affected client artifact + registered consumer IDs for endpoint/type/error/wire changes, including deleted endpoints; unrelated clients excluded. |
| Generated clients reproducible and drift-checked | Generic manifests exist; SDK policy/digests/source maps/checked mode absent | Repeated clean generation byte-equal in all languages; edit/delete/stale-input probes fail check; maintained client check is read-only. |
| Unsafe automatic retries rejected | Error retry consistency exists, but no runtime client guard | Invalid retry policy refused before I/O; unsafe writes, unknown errors, timeout-after-send and reconciliation-only failures make one attempt; allowed retries reuse the same key/body and stop at a bound. |
| Multiple language clients derive from one contract | Only Node adapter exists; runtime route parity is not SDK parity | TS, Go, PHP consume the same canonical SDK projection; compile and run the same wire vectors with equivalent results. |
| Client adapter passes conformance fixtures | Generic/HTTP battery exists, no client fixture checks | Real bundled adapter through `TargetClient`; client rows execute and pass, plus confinement, apply, manifest and determinism checks. |

## 3. Recommended architecture and decisions

### Shared contract, not three independent interpretations

1. Add **new** `crates/lekalo-core/src/client_sdk/{mod,types,project,validate,canonical,mapping,retry,check,impact,diagnostic,version}.rs` and export from `src/lib.rs`.
2. Define `ClientContract`, `ClientOperation`, `ClientType`, `ClientError`, `ClientRetryPolicy`, `ClientConfig`, `ClientBinding`, `ConsumerBinding` and `ClientArtifactIndex` (proposed symbols).
3. `client_sdk::project(transport, context, config)` joins validated transport, compiled IR, error registry and query model once. Carry endpoint/invoked-operation/type/error semantic IDs, effective operation IDs, full wire bindings, public payloads, retry/effect metadata and scenario refs.
4. Retain complete structured parameter data (`style` AND `explode`), response headers and default header rules. The route JSON alone is insufficient: e.g. `project.rs::param_json` does not emit `explode`, and route errors carry references rather than full #62 types.
5. Resolve runtime-independent data in core; emit language syntax in adapter renderers. Do not read arbitrary server source, infer safety from HTTP method, change server policy, or synthesize output fields missing from contracts.
6. Add **new** closed `contracts/client-sdk.schema.v<landing-version>.json` and client config/index schemas, with bounded collections, unknown-field rejection, canonical order and digest vectors. `landing-version` means `Cargo.toml` version when implementation lands, not a literal filename.
7. Add `client-sdk.*` diagnostic rules to the current diagnostic registry/schema and update affected contract identities/gates according to `docs/versioning.md`; do not freeze new contracts at the historical IR version.

### Runtime API and wire semantics

- Generate a factory/client class with constructor-supplied base URL, transport, credential provider and bounded retry options. No default production URL, persisted credentials, background requests, analytics or telemetry.
- Transport input is method/path/query/headers/body plus cancellation; result is status/headers/body. TS gets an injected transport and optional fetch wrapper; Go an injected interface with context; PHP a callable/interface. The wrapper owns encoding, error decoding and retries; injected transport must not independently retry.
- Bind method names to `effective_operation_id`; preserve endpoint and operation IDs as metadata. Escape reserved names deterministically; reject normalization/case-insensitive PHP collisions rather than append order-dependent suffixes.
- Return discriminated success/declared-error results; keep transport/protocol/unknown-server failures separate from the semantic union. Never invent a declared ID for generic infrastructure failure.
- URL-encode path segments; serialize only declared query/header/cookie/body fields, honor explicit aliases/subsets and `style`/`explode`, and handle empty 204 responses. Refuse unsupported serialization styles/capabilities explicitly.
- Inject auth via declared schemes; idempotency key and correlation headers are explicit per-call inputs. Reject case-insensitive header collisions. Browser cookie credentials use transport policy; do not promise setting forbidden browser headers.
- Pagination helpers follow declared offset/cursor bindings and result shape only; expose bounded/cancellable iteration, preserve cursor type, detect repeated cursors and stop on the declared end condition. Ambiguous termination is a refusal, not a guessed response envelope.
- Optional runtime validation is a config switch; identity/envelope/retry checks remain mandatory. Implement shared shape descriptors with equivalent lightweight validators across languages; reuse Zod mapping tests, without requiring server-only imports or forcing Zod into every client.
- Generate fake clients keyed by stable operation ID with typed queued replies and call capture; scenario fixtures drive them, no real networking. Do not claim they execute server business logic.

### Explicit type mapping

| Contract concept | TypeScript | Go | PHP |
| --- | --- | --- | --- |
| Required, non-null field | `x: T` | required DTO field + decoder presence check | typed property + constructor/decoder presence check |
| Absent permitted | `x?: T` | generated presence wrapper | generated presence flag/wrapper |
| Nullable `Optional(T)` | `T \| null` | nullable wrapper with distinct presence/null/value | nullable value plus independent presence flag |
| Date / datetime | branded ISO string, no implicit `Date` | validated string alias, no implicit timezone conversion | validated string value object |
| Number / enum / list | `number` / literal union / `T[]` | numeric mapping with range validation / named enum / slice | float/int policy with validation / constants or value type / typed collection metadata + decoder |
| Decimal | explicit string codec policy by semantic scalar ID | string-backed exact value | string-backed exact value |

Decimal is **not** an existing IR base type. Add an explicit client mapping only for an already string-backed declared scalar with an agreed decimal format; reject numeric-wire-to-decimal coercion. A new decimal wire primitive requires its owning model/transport change, outside this issue's client-only scope. Do not reinterpret every `number` as decimal.

### Retry safety table

- Default: no automatic retry. Positive retry budget alone cannot authorize retries.
- A recognized declared error may retry only according to its `RetryPolicy` and `EffectClass`/`Idempotency`, preserving `check_retry_consistency` semantics from #62.
- `never`: stop; `safe`: allow only with the declared safe contract (writes need `guaranteed`); `conditional(idempotency-key)`: require declared key support and a nonempty per-call key reused byte-for-byte.
- `conditional(reconciliation)`: no automatic retry; return a typed indication requiring caller reconciliation. Do not automatically invoke a callback and presume reconciliation succeeded.
- Timeout/network failure without a recognized error: one attempt in v1; there is no current operation-level declaration proving retry safety for unknown execution state. HTTP 429/503 alone is insufficient.
- Cap attempts and delay; inject clock/sleep in tests, honor cancellation. Validate user configuration and contradictory contract data before the first transport call.

## 4. Concrete implementation sequence (separate logical commits)

### A — core SDK projection and fixtures

- Implement module/types/schemas above; factor shared naming/type helpers where justified, without making SDK generation depend on lossy OpenAPI output.
- Require a registry binding for each operation whose error contract is needed; accept explicit registry/query-model inputs using the existing transport CLI loading pattern. The embedded planner registry is a fixture/default precedent, not a universal production registry.
- Bind actual input digests/version/project to the projection. Existing transport fixtures use placeholder model/IR digests; add correctly pinned SDK fixtures instead of claiming those placeholders establish freshness.
- Add `tests/fixtures/client-sdk/{project,contracts,invalid,wire,golden,diff}` using the planner domain above; include complete outputs and #62 operation bindings, all field-presence/nullability combinations, Unicode/reserved names and explicit decimal string mapping.
- Add `crates/lekalo-core/tests/client_sdk.rs`, `scripts/test-client-sdk-contracts.mjs` and a deliberate golden regeneration script. Normal tests compare committed goldens, never update them.

### B — generation evidence, policy and TS/Vue

- Extend `orchestration/{generate,version}.rs` to prepare `.lekalo/cache/client-sdk/<project>.json` before planning. Load/pin all context through a single helper shared with a proposed `lekalo client render|check|inspect` CLI surface.
- Add `adapters/node-typescript/src/{client-sdk-gen,client-sdk-policy,client-sdk-typescript,client-sdk-runtime}.mjs`; `clientSdkGenerateOperation` reads only validated SDK evidence and policy through the confined read view.
- Configure opt-in `clientSdk` target policy: languages, output roots, operation selection, stable consumer bindings, validation/mapping settings, mode (`generated` or `checked`). Put parsing beside `openapi-policy.mjs`; validate closed vocabulary and safe paths.
- Compose the SDK operation in `generation-composite.mjs` with separate applicability logic so opted-in SDK generation is not suppressed by unrelated Zod/server-route limitations. Required SDK evidence missing or stale must fail, not silently skip.
- Register `generate.client-sdk` / `verify.client-sdk` in `target_protocol/capability.rs`, adapter descriptors and related profile fixtures; advertise implemented coverage honestly.
- Default output `.lekalo/generated/clients/{typescript,go,php}/`; emit code, compatibility metadata, byte-range maps and fake client. Keep dependency manifests pinned, with no install/postinstall side effects.
- Add `tests/fixtures/client-sdk/vue-consumer/` with pinned Vue/compiler tooling, a real `.vue` component importing planner client types, positive usage and expected compile-error cases. Do not add a Vue runtime dependency to the SDK itself.

### C — Go/PHP backends and ownership

- Add adapter renderers `client-sdk-go.mjs`, `client-sdk-php.mjs` over the exact same `ClientContract`. Use the existing bundled Node adapter as the multi-language emitter; no new Go/PHP adapter launchers are necessary merely to write code.
- Add generated Go package and PHP namespace fixtures with DTOs, result/error variants, optional-presence codecs, injected transports, retry guards and fake clients; compile/test each natively.
- Refactor `source_map_binding_for`/`module_path_of` to accept a closed map document with explicit artifact path for new SDK maps; retain legacy TS map handling. Validate path, manifest membership, digest and byte ranges against the actual `.ts`/`.go`/`.php` artifact.
- Preserve maps for untouched targets during manifest updates; test generating one SDK after another. Keep generated/checked ownership distinct and never overwrite maintained files or delete files outside owned roots.
- Extend `artifacts` tests and `crates/lekalo-cli/tests/generate.rs` for SDK apply, repeat apply, rollback/refusal behavior, drift and clean. Bundle via `build.mjs`, regenerate adapter manifest hashes, and run its golden gate.

### D — compatibility, impact and maintained clients

- Compatibility metadata records generator/SDK contract version, Model/IR/transport/error/query digests, wire dialect, target language/mapping profile, operation set and per-operation transitive type/error fingerprints. No timestamps, machine paths or credentials.
- `ClientArtifactIndex` relates semantic artifact ID to safe logical path, language, contract fingerprint, endpoints/types/errors/scenarios and declared consumer IDs. Consumers are explicit maintained declarations, never inferred from network traffic.
- Add `client_sdk::affected_clients` using `transport_http::compare` plus IR/type/error semantic changes; contract-only edits must seed endpoints even when the Model file is unchanged. Keep before/after indexes so removal still finds old consumers.
- Extend `impact::analyze` through an optional typed SDK evidence entry point, preserving existing callers. Populate artifact IDs; add a versioned consumer section in `impact/{mod,canonical}.rs` and the impact schema, or a typed SDK section carrying both. Do not shoehorn paths into `NamedItem.id: NodeId`.
- Update the CLI impact handoff to load current/base SDK evidence and identify transport/error/query changes, including deletions. Missing/stale consumer inventory stays incomplete/unknown; strict wire-consumer mode blocks unproven compatibility.
- SDK check compares expected projection/fingerprints AND owned bytes, so regeneration or a clean code digest cannot hide a breaking wire change. Emit affected consumer/artifact IDs and a required regeneration/consumer-validation gate.
- Implement read-only maintained-client `client check` via explicit bindings: endpoint ID, exported symbol, source path and observed signature. Prefer TS compiler scanner reuse; for Go/PHP require a declared observation provider or checked native contract harness, otherwise return unsupported, never a full pass from a handwritten sidecar alone.
- Fixtures cover matching maintained client, signature/code drift, wrong endpoint binding, duplicate binding, missing source, manual unbound inventory and unchanged hand-written bytes after check. State supported checked-mode language coverage explicitly.

### E — adapter conformance and CI

- Extend `target_protocol/conformance.rs` plus `tests/fixtures/adapter-conformance/` with capability-gated `client.projection-parity`, `client.error-identity`, `client.retry-safety`, `client.drift`, `client.consumer-impact` checks; update closed catalogs/report schemas and tests that pin row counts/order.
- Exercise the real bundled adapter via `TargetClient`, not direct renderer calls alone. Include wrong evidence digest, stale lock, path traversal/symlink, missing required context, unsupported capabilities and nondeterministic output.
- Add `scripts/test-client-sdk-runtime.mjs` (TS), Go `*_test.go`, PHP executable assertions and `scripts/test-client-sdk-parity.mjs` comparing normalized wire captures/results across languages.
- Register schema/core/runtime/native language/Vue/conformance gates in `.github/workflows/ci.yml`; pin Go/PHP/TS/Vue tooling and dependencies. A missing compiler or skipped client row cannot satisfy release acceptance.
- Document supported checked-mode scope, transport obligations, retry matrix, type mappings and reproduction commands in **new** `docs/client-sdk.md` and link from README/adapter conformance docs.

## 5. Verification commands and fixture matrix

Existing commands (run after implementation; preserve the repository's platform/confinement CI setup):

```text
cargo test --locked -p lekalo-core --test transport_http --test openapi_render --test error_contract --test artifact_manifest --test impact --test adapter_conformance
cargo test --locked -p lekalo-cli --test generate --test impact
node scripts/test-node-typescript-transport.mjs
node scripts/test-node-openapi.mjs
node scripts/test-node-zod-gen.mjs
node scripts/check-contract-versions.mjs
node adapters/node-typescript/build.mjs --check
node scripts/test-adapter-manifest-golden.mjs
```

New gates to implement and run (these commands are not available on the research baseline):

```text
cargo test --locked -p lekalo-core --test client_sdk
cargo test --locked -p lekalo-cli --test client_sdk
node scripts/test-client-sdk-contracts.mjs
node scripts/test-client-sdk-runtime.mjs
node scripts/test-client-sdk-parity.mjs
```

- Install schema tooling exactly as CI does (Ajv 8.17.1); install adapter build dependencies with `npm ci --prefix adapters/node-typescript --ignore-scripts --no-audit --no-fund`. Its package scripts are deliberately poisonous; do not use `npm test` there.
- New parity harness must run pinned `tsc --noEmit`, `vue-tsc --noEmit`, `go test ./...`, PHP syntax checks plus executable assertions in their fixture directories. TS/Vue dependencies belong in a separate pinned consumer fixture.
- New conformance profile must invoke `cargo run --locked -p lekalo-cli -- adapter test --profile default --report junit -- node adapters/node-typescript/adapter.mjs` with the existing `--lekalo-project-profile-json` mechanism configured for SDK evidence/output scopes; assert all client rows actually pass.
- Wire vectors: path escaping, query lists/style/explode, explicit body subset, auth/correlation/key injection, empty 204, all declared semantic errors, unknown/malformed responses, null vs absent, date/timezone strings, exact decimal strings, pagination termination/cancellation.
- Retry vectors: unsafe POST, safe read error, guaranteed write, key-required retry absent/present, changed key attempt, reconciliation-only, transport disconnect after send, unknown 503, capped attempts and cancellation. Assert exact call count and bytes, not just returned status.
- Drift/impact vectors: endpoint deletion, path/method change, response type narrowing, error code/status change, auth/idempotency policy change, wire dialect bump, shared-type edit, unrelated endpoint edit, stale index and unregistered consumer. Test endpoint-only attachment edits without IR edits.
- Determinism: regenerate in two clean temp roots, compare all three languages + manifest/maps/fakes; different working paths/order must not change bytes. Tamper one code file, metadata digest and map independently; each must fail the appropriate check.

## 6. Risks and recommended resolutions

1. **Scope size:** all three first targets are in scope; do not satisfy multi-language acceptance with TS plus route JSON. Stage implementation commits A–E, with native compilation gates for TS/Go/PHP before acceptance; defer Rust only.
2. **Missing semantic evidence:** reject SDK generation without required registry/query bindings. Existing partial OpenAPI behavior is useful diagnostics, not acceptable typed SDK completeness.
3. **Checked existing clients:** arbitrary semantic equivalence of hand-written networking code is not decidable here. Guarantee verified signatures plus fixture-based wire behavior for declared bindings; report unbound/manual/unsupported inventory honestly.
4. **Language/toolchain levels:** pin supported minima in fixture manifests/CI before choosing language features. Prefer portable interfaces/DTOs; do not force modern enums/generics where an agreed older PHP/Go baseline cannot compile them.
5. **Contract freshness:** transport validation checks identity/version but is not proof every attached digest equals current model bytes. SDK evidence must be derived/pinned from actual inputs; failing stale pins must not be “fixed” by silently rewriting user declarations.
6. **Retry ambiguity:** existing #62 policies are per declared error, not proof of safety after an unclassified network failure. Conservative one-attempt behavior is the recommended v1 resolution; any broader policy needs an explicit contract extension.
7. **Cross-issue overlap:** coordinate changes to generation composite, capability catalog, versioned schemas, impact and adapter bundle with the milestone owner. Rebase and revalidate these seams before implementation; this research does not authorize changing other worktrees or merging M5.
