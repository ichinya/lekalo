# Issue #90 implementation: the unified deterministic golden fixture suite

Branch `ichinya/m7-issue-90`. Spec: `docs/m7/issue-90-research.md`
(commit `c255c892`). This report describes what was built, the
decisions taken (including documented deviations), and how each issue
acceptance criterion is verified by committed gates.

## What was built

A new, fully additive fixture family `tests/fixtures/suite/` (declared
`synthetic` in `tests/fixtures/fixture-provenance.json`, family count
63 -> 64; the provenance gate stays green) plus nine new gates under
`scripts/`. No existing fixture, gate, or workflow behavior was removed
or weakened; `.github/workflows/ci.yml` received only the minimal wiring
listed below.

### Suite layout and versioning (research section 3)

```text
tests/fixtures/suite/
  README.md                          authoring/byte-policy/update contract
  schema/                            closed v1.0.0 JSON Schemas
    fixture.schema.v1.0.0.json       case descriptor (dev.lekalo.fixture@1.0.0)
    catalog.schema.v1.0.0.json       catalog (dev.lekalo.fixture-catalog@1.0.0)
    coverage.schema.v1.0.0.json      coverage index
    run-manifest.schema.v1.0.0.json  repeat-run manifest
    golden-update.schema.v1.0.0.json update plan
  v1/
    catalog.json                     exact case inventory (20 cases) + imported evidence
    run-manifest.json                pinned cold-1 execution manifest (39 rows)
    coverage/diagnostic-rules.json   all 449 active registry rules with evidence state
    coverage/kinds.json              all 12 definition kinds with witnesses
    minimal/project/                 F01 minimal project + 4 golden envelopes
    diagnostics/<rule>/              F06: 19 paired trigger/non-trigger projects
    checksums/                       sha256 sidecars (.expect.json convention preserved)
```

Case identity is `dev.lekalo.fixture@1.0.0` with stable dotted case IDs
(`minimal.project`, `diagnostic.type-recursion.pair`) and per-case
`revision`. Product contracts are pinned independently
(`dev.lekalo.model@0.2.16`, `dev.lekalo.ir@0.2.16`, diagnostic registry
`0.4.0`, `dev.lekalo.graph@0.2.16`); the suite version never bumps a
product contract. Runner IDs and update-recipe IDs resolve to closed
registries in `scripts/lib/fixture-catalog.mjs`; fixture data cannot
supply executable paths.

### Gates (research section 4, stages implemented)

| Stage | Gate / script | Responsibility |
| --- | --- | --- |
| 1 | `scripts/lib/fixture-catalog.mjs`, `scripts/test-golden-catalog.mjs` | Ajv 8.17.1 validation of catalog + descriptors; unique sorted case IDs; path policy; digest-pinned inputs/expected; coverage-index integrity (every registry rule exactly once, evidence paths exist); provenance registration; orphan-file control |
| 2 | `scripts/test-golden-normalization.mjs` | LF-only wire bytes; repo-relative/NFC/no-`..` path policy over every tracked suite file; UTF-8-byte-order vs JS-order divergence control; JSON numeric-precision premise; case-fold/reserved-name control |
| 3 | `scripts/run-golden.mjs` | Read-only verifier: materializes inputs in fresh external sandboxes, runs the cargo-built binary, checks exit/status/reason contracts, enforces trigger/non-trigger polarity, `--verify` compares bytes; separate actual-output directory; never writes tracked fixtures |
| 4 | `scripts/test-golden-diagnostic-coverage.mjs` | Joins the embedded registry (449 active rules) to the coverage index and to fresh execution receipts; negative controls prove it fails closed (dropping an index row or corrupting a pair fails) |
| 5 | `scripts/test-golden-determinism.mjs`, `scripts/update-golden-run-manifest.mjs` | cold-1 / cold-2 / warm-cache lanes in independent sandboxes; full outcome-manifest comparison (case, revision, role, status, exit, reason codes, envelope digest); tracked-tree pollution detection; committed `run-manifest.json` must match the re-executed reference lane |
| 6 | `scripts/test-golden-adapter-shared.mjs` | Shared IR/scenario/trace corpus digest-pinned across core `adapter_conformance/fixture.rs` `include_str!` paths, catalog imported-evidence rows, and the IR goldens; unregistered byte-equal copies are rejected |
| 7 | `scripts/test-golden-planner-e2e.mjs` | The P0 chain: materialize shared Planner project -> load/validate/compile IR -> graph/inspect/impact/context projections -> semantic diff (equal-formatting control + behavioral mutation with affected seeds) -> scenario lane (the committed 6-scenario dry-run/apply/verify corpus with rerun-stability and unsupported-race honesty) -> trace manifest export/validate. Each stage consumes real upstream bytes in a fresh sandbox |
| 8 | `scripts/test-golden-hygiene.mjs` | Closed credential/host-path patterns (secret assignments, JWTs, AWS keys, private keys, GitHub tokens, URL credentials, file URIs, UNC/drive paths) over the whole suite tree; live scanner controls (hostile strings caught, clean markers pass); workspace/TEMP/HOME roots refused; no exemptions |
| 9 | `scripts/update-golden-case.mjs` (`plan`/`apply`), `scripts/test-golden-update-policy.mjs` | The deliberate update flow (below) |

Generators (deterministic, run only through the reviewed update flow):
`scripts/gen-suite-diagnostic-pairs.mjs` (the 19 pairs + descriptors),
`scripts/gen-suite-coverage.mjs` (coverage index + catalog writer).

### The deliberate golden update flow (AC5)

```text
node scripts/update-golden-case.mjs plan --case <id> --reason "<issue + rationale>" [--out <dir>]
node scripts/update-golden-case.mjs apply --plan <plan-file> --accept-plan-sha256 <reviewed digest>
node scripts/run-golden.mjs --case <id> --verify
```

- `plan` has zero tracked writes: the producer runs twice in fresh
  sandboxes and both runs must agree byte-for-byte; the plan binds
  before/after file digests and a semantic summary (`semanticChanges`
  with closed change kinds + a mandatory `humanExplanation`). A
  hash-only summary cannot validate against the closed plan schema.
- `apply` refuses a wrong accept digest, a stale case revision, drifted
  preimages, and candidate/declared count mismatches; it writes only
  the reviewed, positionally-mapped expected files.
- `test-golden-update-policy.mjs` rehearses the flow end to end in a
  scratch checkout and proves **no CI workflow references the update
  flow**.

## Decisions and documented deviations

1. **Paired-evidence scope (F06).** The registry embeds 449 active
   rules across 51 subsystems; many are adapter/runtime seams not
   reachable by project-shaped CLI inputs (per the scope guard, the
   coverage INDEX is authoritative). The index assigns every rule one
   explicit evidence state, verified live by the catalog gate:
   - `suite-pair` (19): this suite owns a trigger + non-trigger pair,
     re-executed by every coverage-gate run (all 20 `semantic.*`
     validator rules).
   - `family-fixture` (109): a named committed fixture file pins the
     rule (loader/structure/ir/target-protocol/storage/
     extended-effects/implementation/expressions matrices); the gate
     proves every path exists.
   - `test-witness` (308): a named Rust/Node gate asserts the rule
     (gate file paths are real; notes name the asserting test).
   - `interaction-only` (13): reachable only via host interaction
     (OS I/O denial, killed children, wall-clock timeouts, exclusive
     locks); each row names the concrete interaction.
   Growing `suite-pair` coverage is incremental and cannot silently
   regress: any registry addition without an index row fails the gate.

2. **SARIF (G02) is not implemented.** The research confirms core has
   no SARIF emitter; manufacturing one in a test would violate the
   issue's own boundary. This is recorded as a product-capability gap,
   not a suite gap.

3. **F15 chain scope.** The integrated chain uses the catalog-registered
   shared corpora (orchestration project, adapter-conformance IR,
   diff cases, trace golden) rather than duplicating a Planner copy —
   the research's shared-fixture-drift risk #6 addresses exactly this.
   Stage 4 delegates to the committed scenario lane instead of
   re-implementing the adapter exchange, keeping one audited producer.

4. **Warm-cache lane.** All three lanes run `--no-cache` producers in
   separate roots; the warm lane additionally proves rerun stability
   through the scenario corpus's durable-run-record assertions. A
   cache-enabled lane that would silently weaken the byte contract is
   deliberately not introduced.

5. **CI wiring is minimal:** the Node-only suite gates (catalog,
   normalization, hygiene, update-policy, adapter-shared) run in the
   existing `contracts` job (Node 18/24 matrix, Ajv already
   provisioned); the binary-dependent gates (`run-golden --verify`,
   determinism, diagnostic coverage, planner e2e) run in the existing
   `build-test` job on all three OSes right after the workspace tests.

## Acceptance-criteria verification

| AC | Verification |
| --- | --- |
| AC1 byte-stable repeat runs | `node scripts/test-golden-determinism.mjs`: 3 lanes x 20 cases (39 rows), full manifest equality incl. envelope digests, pollution detection, committed `run-manifest.json` cross-check |
| AC2 path/newline normalization | `node scripts/test-golden-normalization.mjs` (LF-only bytes, path policy, UTF-8/order and case-fold controls) + the same gates wired into `build-test` on ubuntu/windows/macos; descriptors are repo-relative so producer bytes are OS-independent |
| AC3 positive AND negative per rule | `node scripts/test-golden-diagnostic-coverage.mjs`: every one of the 449 active rules has an evidence row; all 19 suite pairs execute both polarities freshly; family-fixture paths proven present; negative controls (drop a row, corrupt a pair) fail the gate |
| AC4 adapter conformance reuses shared fixtures | `node scripts/test-golden-adapter-shared.mjs`: shared IR/scenario/trace digests pinned across `fixture.rs` includes, catalog evidence, and goldens; unregistered copies rejected |
| AC5 deliberate update + reviewed semantic summary | `node scripts/test-golden-update-policy.mjs`: plan determinism, semantic-summary schema, wrong-digest refusal, digest-bound apply, no CI write path |
| AC6 planner P0 chain | `node scripts/test-golden-planner-e2e.mjs`: six stages over shared inputs, upstream-digest-linked, equal-formatting + mutation diff controls, 6-scenario lane, trace validation |
| AC7 no secrets/host paths | `node scripts/test-golden-hygiene.mjs`: closed pattern scan over the whole suite + live controls; host roots (workspace/TEMP/HOME) refused; zero exemptions |

All gates are wired in `.github/workflows/ci.yml` (`contracts` and
`build-test` jobs) so the evidence is produced on every push/PR across
Linux, Windows, and macOS.

## Local verification (this checkout, Windows)

- `node scripts/test-fixture-provenance.mjs` — ok, 64 families
- `node scripts/test-golden-catalog.mjs` — ok (Ajv 8.17.1)
- `node scripts/test-golden-normalization.mjs` — ok (214+ files)
- `node scripts/test-golden-hygiene.mjs` — ok
- `node scripts/run-golden.mjs --verify` — ok (39 rows)
- `node scripts/test-golden-determinism.mjs` — ok (3 lanes)
- `node scripts/test-golden-diagnostic-coverage.mjs` — ok (449 rules)
- `node scripts/test-golden-adapter-shared.mjs` — ok
- `node scripts/test-golden-planner-e2e.mjs` — ok (6 stages)
- `node scripts/test-golden-update-policy.mjs` — ok
- `cargo fmt --check`, `cargo clippy -p lekalo-cli -p lekalo-core -- -D warnings`,
  `cargo test -p lekalo-cli -p lekalo-core` — final run recorded in the
  completion report (no Rust product code changed; the suite is
  script/fixture-only).
