# Issue #118 — brownfield private TypeScript consumer pilot: implementation report

Branch `ichinya/m6-issue-118`, milestone M6. The pilot connected a real
private TypeScript application in `observed` mode through a
reproducible harness, bound one submission use case under the issue's
conditional semantic id `task.submit`, projected the graphs and the
bounded context capsule, proved staleness detection on a controlled
change, and emitted only privacy-safe aggregate evidence. Companion
research (CLI surface map, plumbing findings): `docs/m6/issue-118-research.md`.

## Delivered surface

- `scripts/pilot-brownfield-ts.mjs` — the dependency-free pilot
  harness: copy-discipline consumer sandbox, adopt dry-run purity
  gate, wire-scan attempts with recorded refusals, in-process kernel
  fallback with `observe update` merge, explicit use-case bind,
  proposal confirmation, native-test attach, promotion, projections
  (`inspect`/`impact`/`context`/`baseline`/`bindings list`/
  `status`/`doctor`), the controlled-change staleness round trip, and
  the closed privacy-safe `metrics.json` + `report.md` emission.
- `tests/fixtures/pilot/brownfield-ts/` — the fully synthetic
  anonymized consumer fixture (npm-workspace monorepo: Hono
  `POST /tasks` route → repository → `sqliteTable` insert, one vitest
  file, authored Hono/vitest declaration stubs, generic names), provenance
  declared under the `pilot` family.
- `scripts/test-pilot-brownfield-ts.mjs` — the CI gate: runs the
  harness end to end over the fixture (building the binary when
  missing) and asserts adopt dry-run purity, the honest wire refusal
  plus fallback completion, binding recorded + confirmed + promoted,
  projections, staleness firing and cleaning on revert, and the closed
  metrics schema (member set, digest spellings, leak probes).
- `.github/workflows/ci.yml` — the gate registered in the build-test
  job next to the other pilot/gate steps (10-minute timeout).
- `C:/Users/User/orca/m6-issue-118-pilot-out/` (outside the
  repository) — the real-consumer run artifacts: `metrics.json` and
  `report.md` with the same closed schema. Nothing was written into
  the consumer tree; the run is copy-mode.

## Real pilot run (anonymized metrics)

The consumer is named only as `consumer` (`sha256:396735e980ae…` of its
canonical path); the private repository keeps its own bindings.

| Metric | Value |
| --- | --- |
| Run steps ok / failed | 20 / 0 (total 36.6 s) |
| Copy | 6,466 files; skipped: `node_modules` ×8, `.git` ×1, `dist` ×7, `coverage` ×7, grammar-unspellable entries ×415 |
| Adopt dry-run purity | intact (planned writes 2, zero bytes changed) |
| Adoption writes | `lekalo/project.yaml`, `lekalo/targets/node-typescript.yaml` only |
| Profile | 23 read roots, 7 packages, 0 exclusions |
| Wire-path scan | refused twice (manifested: `adapter.manifest-mismatch/capabilities.readScopes`; local-development: `target.operation-failed/adapter-error-partial`) |
| Cold scan | 2,970 ms |
| Incremental (warm) scan | 2,173 ms, byte-identical index |
| Scanned symbols / exported scan entries | 6,040 / 1,494 |
| Honest uncertainty rows / compiler diagnostics | 4,375 / 1,498 |
| Endpoints derived / routes without an indexed handler | 0 / 0 (provider degraded over the node_modules-free copy — finding F3) |
| Native test bindings from the scan | 0 (test suites sit under unspellable directories — finding F4) |
| Bound use case | `task.submit`: explicit bind, state current |
| Confirmed proposals / ambiguous left unconfirmed | 1 / 83 |
| Promoted symbols | 1 (explicit plan → confirm two-step) |
| Context capsule | 12 tokens (budget 4,096; fits), 1 section, 6 declared gaps — versus a 6,040-symbol broad scan |
| Impact projection | 1,494 recorded symbols, 0 stale, 83 unknown edges, completeness `incomplete` (honest) |
| Binding registry | 1,494 bindings (2 confirmed, 1,492 inferred, 1,411 current, 83 unknown, 0 stale) |
| Controlled change | signature-detail mutation → `bindings audit` invalid with 213 `observed.stale-binding` diagnostics, `observe check` refused; revert → audit clean |
| Doctor verdict over the copy | `blocked` (no Git custody inside a disposable copy — expected, recorded) |

The capsule headline: **12 tokens** to brief an agent on the bound use
case against **6,040 scanned symbols** — the bounded-context goal of
the issue, measured.

## Acceptance-criteria evidence

| Issue requirement | Evidence |
| --- | --- |
| Connect to a mature private repository without changing sources | Copy-mode harness run green (20/0); the adopt dry-run file-list purity assertion held; the only writes inside the copy are the adopted `.lekalo`/`lekalo` metadata plus the one reverted controlled change. `metrics.json` step `adopt-dry-run: purity=intact`. |
| Bind one existing use case to a semantic symbol | `observe bind task.submit --key <native> --path --line` → explicit, current (`bind-use-case: binding=explicit, state=current`); the native anchor is the consumer's own submission function (stable key from the scan). |
| Build dependency/effect/route/storage/test graph | 1,494 merged bindings with structural evidence (signatures + module-level reference rows); unknown edges counted (83), never guessed. Route and test rows honestly empty over this copy — findings F3/F4 record why and what would unlock them. |
| Useful context capsule much smaller than a broad scan | 12-token capsule over 6,040 scanned symbols (metrics above; `context: fits=true`). |
| Honest incompleteness | `impact completeness=incomplete` with `unknown=83`; 4,375 uncertainty rows and 1,498 diagnostics carried into metrics; capsule declares 6 gaps (`detected-effects-absent`, `error-contracts-unrepresentable`, `no-description`, `no-effects`, `no-policies`, `no-scenario-coverage`). |
| Stale binding detection after a change | Controlled signature-detail mutation inside the copy → `bindings audit` invalid with 213 `observed.stale-binding` diagnostics, `observe check` refused; revert → audit valid again. Gate asserts the same path over the fixture. |
| Privacy-safe evidence and Framework Lift baseline | `metrics.json`/`report.md` carry counts, sizes, durations, digests only; the CI gate greps the emitted metrics for consumer identifiers and path spellings and refuses on any leak; `observe baseline` recorded the deterministic #49 baseline inside the copy (index digest in metrics, private side). |
| Canonical model stays inside the private repository | Promotion, bindings, and the observed index live under the copy's `lekalo/`/`.lekalo/` homes and are deleted with the workdir (`--keep` exists for local inspection only). |
| Only a synthetic anonymized fixture enters Lekalo | `tests/fixtures/pilot/brownfield-ts/` — authored in-repo, generic names, provenance under the registered synthetic `pilot` family; the gate keeps it exercised. |
| Source and native tests remain primary; inferred facts stay proposals | 1,492 of 1,494 bindings are `inferred` proposals with candidates; 1 explicit (`task.submit`) plus 1 confirmed handler proposal; the only canonical promotion is the explicitly planned/confirmed use case. |

## Requirements findings for the generic adapter / Hono / Drizzle / storage profiles

- **F1 — Wire-path surface pinning.** The manifested adapter manifest
  pins `profiles: ["standalone"]` and read scopes `src/**` /
  `.lekalo/ir/**`; any monorepo profile refuses at the describe
  consistency gate (`adapter.manifest-mismatch`). Requirement: either
  widen the manifested scope vocabulary (workspace-relative read roots)
  or document the implicit local-development path as the supported
  brownfield wire.
- **F2 — Wire outcome projection vs real trees.** The closed wire
  refuses uncertainty-bearing scans (`adapter-error-partial`) and caps
  entry detail tokens at 128 bytes. A node_modules-free brownfield copy
  always carries unresolved imports, so the wire can never carry a
  first-contact scan; the kernel fallback + `observe update` is the
  working path (and matches the committed taskhub derivation).
  Requirement: an explicit "first-contact mode" wire outcome that
  carries partial inventories with uncertainty counts instead of
  refusing.
- **F3 — Framework extraction needs a spellable declaration surface.**
  With `node_modules` absent, the `hono` specifier resolves to no
  declaration; the provider records `hono-unresolved-constructor`
  uncertainty and extracts no apps/routes. Fixture-style authored stubs
  (`types/*.d.ts` + tsconfig `paths`) or an embedded declaration
  surface (analogous to the Drizzle closure) would unlock route
  extraction over clean copies. WS routes are untouched by this pilot
  for the same reason.
- **F4 — Portable-path grammar vs the vitest `__tests__` convention.**
  The grammar (`^[a-z0-9.][a-z0-9._-]*$` segments) cannot spell
  underscore-leading directories; 415 consumer entries were
  uncopyable, all native-test attachment stayed an explicit gap, and no
  exclusion spelling can name them (exclusions validate against the
  same grammar). Requirement: a spellable test-directory convention or
  a bounded exclusion vocabulary for unspellable spellings.
- **F5 — SQLite dialect coverage.** The consumer is Drizzle-over-SQLite
  (the issue says MySQL); the embedded Drizzle declaration closure maps
  only `pg-core`/`mysql-core`/`node-postgres`/`mysql2` subpaths, so
  `drizzle-orm/sqlite-core` stays unresolved and storage evidence
  degrades to explicit partial completeness (`binding-unresolved`,
  `callee-unproven`) with the owner-supplied `drizzle.bindings.json`
  unresolved. Requirement: add the SQLite subpaths to the embedded
  closure and a storage-profile dialect entry.
- **F6 — Monorepo module mapping.** Observed mode required one
  canonical module per package scope before the merge (8 modules for 7
  packages plus the operator id); the harness derives them
  mechanically, but adoption could propose the module set directly from
  the workspace manifest.
- **F7 — Snake-case id normalization.** The scan's mechanical semantic
  proposals keep source casing, while the canonical model id grammar is
  lowercase snake; the fallback document normalizes mechanically (as
  the committed taskhub scan already did). A promoted symbol requires
  the normalized spelling; the wire proposals would need the same
  normalization to be promotion-ready.

## Verification

- `node scripts/pilot-brownfield-ts.mjs --consumer
  tests/fixtures/pilot/brownfield-ts --bind
  packages/api/src/routes/tasks.ts:submitTask:task.submit
  --disposition public-fixture --out <temp>` — green end to end
  (20/0 steps).
- `node scripts/test-pilot-brownfield-ts.mjs` — green
  (`{"ok":true,"gate":"pilot-brownfield-ts"}`).
- Real consumer run — green (20/0 steps), metrics + report emitted to
  the private output directory; failures, had any step refused, would
  have been captured as findings with the report still emitted (the
  harness records per-step failures and exits non-zero while writing
  the artifacts).
- `cargo fmt`, `cargo clippy -p lekalo-cli -- -D warnings`,
  `cargo test -p lekalo-cli`, `node
  scripts/test-fixture-provenance.mjs` — green; no Rust or golden
  surfaces changed by this issue.
