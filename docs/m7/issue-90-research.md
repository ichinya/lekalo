# Issue #90: deterministic golden fixture suite research

Research only; no fixture, runner, product, or CI implementation is included in this commit. Authority: [GitHub issue #90](https://github.com/ichinya/lekalo/issues/90), read live on 2026-10-01. Survey baseline: `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`, branch `ichinya/m7-issue-90` (M7 base). The issue remains open; the acceptance mapping below is proposed work, not a claim that the suite already passes.

## 1. Current-state inventory

### Scope and evidence

The tracked inventory is **2,992 files under `tests/fixtures/`, 63 family directories, and 100 `scripts/test-*.mjs` gates**. Counts use `git ls-files`, including hidden fixture homes; plain `rg --files tests/fixtures` reports 2,956 because of its filtering. There is substantial existing semantic coverage, but no suite-wide case schema, rule-to-positive/negative coverage index, repeat-run manifest, or single reviewed update workflow. Product contract versions and a directory called `model-v1` do not constitute a versioned fixture format.

The survey enumerated all families, test gates, Rust integration-test include patterns, generators, provenance declarations, and `.github/workflows/ci.yml`; representative producers and consumers were inspected for each issue output class. It is a source/inventory survey, not an execution of all 2,992 files or a full acceptance audit. The following dependency-free gates were actually run successfully in this Windows checkout:

```text
node scripts/test-fixture-provenance.mjs
node scripts/test-versioning-contracts.mjs
node scripts/test-adapter-manifest-golden.mjs
```

Provenance reports 63 synthetic families and zero evidence-backed families. The version gate reports Model/IR `0.2.16` and protocol `0.3.2`. The adapter gate recomputes both shipped Node and PHP package manifests. Rust tests, installed-runtime suites, regeneration commands, and Linux/macOS execution were not run for this documentation task.

### Complete family inventory

Every row names an existing directory directly below `tests/fixtures/`. `T(name)` means `scripts/test-name.mjs`; Rust references are integration test stems under `crates/lekalo-core/tests/` or `crates/lekalo-cli/tests/` unless stated otherwise. A listed consumer is an evidence pointer, not proof of exhaustive coverage.

| Family | Existing fixtures / consumers |
| --- | --- |
| `adapter-conformance` | Shared compiled IR, invalid refs, concurrency Scenario IR, transport evidence, canonical project/OpenSpec markers; core `adapter_conformance`, `src/adapter_conformance/fixture.rs`. |
| `adapter-manifest` | Valid/invalid package manifests; `T(adapter-manifest-contracts)`, core package tests. |
| `adapter-security` | Hostile adapter executable; `target_protocol_security` and containment tests. |
| `adopt` | Empty, Go, Laravel, Node-monorepo projects; `init_adopt_boundary`, CLI `init`. |
| `artifacts` | Project plus wire manifests, plans, ownership/write cases; `artifact_manifest`, CLI `generate`, `T(artifact-manifest-contracts)`. |
| `authority` | Allowed/forbidden/malformed vectors; authority script gates. |
| `authorization` | Planner, no-auth, ownership, stale/uncovered, mapping and malformed cases plus golden; Rust authorization tests and `T(authorization-contracts)`. |
| `bindings` | App, scans, fake scanner; Rust bindings tests and `T(bindings-contracts)`. |
| `cache` | Records, health and invalid cases; CLI `cache`, `T(cache-contracts)`. |
| `classification` | Valid/invalid classification cases; `T(classification-contracts)`, `T(classification-cli)`. |
| `client-sdk` | Generated TypeScript/Go goldens, invalid inputs, Vue consumer and runtime harness; `client_sdk`, three client-SDK gates. |
| `context` | Planner plus JSON/envelope/Markdown goldens; Rust context tests, `T(context-contracts)`. |
| `contracted` | Planner ownership slice; Rust contracted tests, `T(contracted-contracts)`; not by itself a runnable Scenario IR corpus. |
| `diagnostics` | Six JSON/human envelopes for load, IR, usage and protocol failures; CLI `diagnostics`, `T(diagnostic-contracts)`. |
| `diff` | Eight base/candidate cases (`behavior`, `equal-formatting`, `fields`, `fields-tighten`, `history`, `signature`, `symbols`, `taxonomy-rest`), `expect.json` and profile outputs; Rust diff tests, `T(semantic-diff-contracts)`. |
| `doctor` | Status/readiness/doctor goldens and invalid envelopes; CLI `doctor`, `T(doctor-contracts)`. |
| `effects` | Planner plus effect-graph golden; Rust effects tests, `T(effect-graph-contracts)`. |
| `error-contract` | Valid/invalid error mappings; `error_contract`, `T(error-contracts)`. |
| `expressions` | Valid/invalid expressions, vectors, builtin support and diff; `expressions`, `expressions_projection`, `T(expressions-contracts)`. |
| `extended-effects` | Planner effect attachment, invalid cases with sidecars, diff/permutation; `extended_effects`, `T(extended-effects-contracts)`. |
| `graph` | Planner model, IR/spans and graph goldens; Rust graph tests, `T(graph-contracts)`. |
| `impact` | Planner/rename models and golden; Rust impact tests, `T(impact-contracts)`. |
| `implementation` | Wire, semantic and invalid implementation attachments with expectations; `implementation`, `T(implementation-contracts)`. |
| `inspect` | Planner plus twelve golden files; Rust inspect tests, `T(inspect-contracts)`. |
| `invariant-transition` | Valid graph/invariants, invalid sidecars, diff/permutation; `invariant_transition`, `T(invariant-transition-contracts)`. |
| `ir` | Full kinds, JSON twin, invalid typed inputs, Zod matrix/source tree and unsupported findings; Rust IR tests, `T(node-zod-gen)`. |
| `laravel-migrations` | Inputs, invalid cases, SQL/plan/manifest goldens; `T(laravel-migration-contracts)`, `T(php-laravel-migrations)`. |
| `loader` | YAML/JSON twins, parser errors, version refusals, ambiguous/duplicate refs, import cycles/escape, encoding errors; CLI `load`, `gen-loader-fixtures.mjs`. |
| `lockfile` | Project, valid/invalid locks and digest sidecars; Rust lock tests, two lock contract gates. |
| `mago` | Toolchain recordings, violating source, JSON/SARIF-related runtime probes; `T(mago-integration)`. |
| `model` | Schema/checker failures and Planner; `T(model-contracts)`, `T(model-ajv)`, shared `model-fixture-manifest.mjs`. |
| `model-v1` | Minimal/Planner, identity grammar, alias, rename, tombstone and version cases; same two model gates. Both model roots currently use schema `0.2.16`. |
| `nfr` | Planner, invalid/diff cases and JSON/Markdown goldens; `nfr`, `nfr_report`, `T(nfr-contracts)`. |
| `node-native-gates` | npm/pnpm projects, policy/view/plan/run-result goldens and digest vector; `T(native-gate-contracts)`, `T(node-native-gates)`. |
| `node-typescript-drizzle` | MySQL/PostgreSQL and edge/callee/regression source corpora; `T(node-drizzle)` -> adapter `drizzle-evidence.test.mjs`. |
| `node-typescript-kernel` | Project, requests, expected results, profiles and extensions; core kernel test and adapter `fixtures.test.mjs`. |
| `node-typescript-scanner` | ESM/CJS, aliases, workspaces/references, incremental/uncertainty, Hono/framework evidence and protocol profiles; scanner and Hono gates. |
| `node-typescript-transport` | Transport conformance profile; transport-configured adapter CI battery. |
| `observed` | Task domain and observed golden; Rust observed tests, `T(observed-contracts)`. |
| `openapi` | Valid/invalid, merge, metadata, diff and trace/export golden; `openapi_render`, CLI `openapi`, OpenAPI gates. |
| `orchestration` | Planner project, six scenarios, test port, fake adapter, invalid receipts and generate/verify goldens; CLI `generate_orchestrate`, Node scenario gates. |
| `php-laravel` | Planner application, native gates, operations, routes, type source/evidence goldens; PHP adapter and runtime gates. This adapter exists at the surveyed baseline. |
| `pilot` | Taskhub/observed baseline, scan/plan fixtures, equivalence and brownfield TS consumer; CLI `pilot`, pilot gates. |
| `privacy` | Allowed/forbidden/ambiguous/malformed/transform-required policy inputs; privacy checker, parity and CLI gates. |
| `privacy-leaks` | Synthetic secret/PII/path markers with manifest and clean control; `T(privacy-leak-corpus)`. Not a whole-tree secret scanner. |
| `query-model` | Project, valid/invalid and diff; CLI `query_model`, `T(query-model-contracts)`. |
| `reference-evaluation` | Model/scenarios, deterministic result traces, digest list and clock/unsupported/invariant regressions; `reference_evaluation`, contract gate. |
| `requirements` | Planner, lexical/parser vectors, invalid/diff and trace/report goldens; Rust requirements tests, `T(requirements-contracts)`. |
| `run-history` | Valid observations/records, invalid schema cases; core internal store/clock tests, CLI `history`, two run-history gates. |
| `scenario` | Six valid Scenario IR documents and invalid cases with sidecars; `scenario`, `T(scenario-contracts)`. |
| `storage-engine` | Plans, derived/introspection/runtime/migration inputs; `storage_engine`, `T(storage-engine-contracts)`. |
| `storage-engine-profile` | Profiles, invalid sidecars and portability projections; corresponding Rust/script tests. |
| `storage-introspection` | Valid/invalid/drift cases and sidecars; corresponding Rust/script tests. |
| `storage-mysql` | Drizzle source and expectation; `T(storage-mysql-binding)` (supplemented by compiler-backed Drizzle family). |
| `storage-projection` | Planner storage, invalid sidecars, diffs and derived schema artifacts; corresponding Rust/script tests. |
| `structure` | Minimal/discovery, ownership-denied and opaque-import cases; `T(structure-contracts)`, CLI/bootstrap coverage. |
| `target-profile` | Valid/invalid/semantic profiles with sidecars; `target_profiles`, `T(target-profile-contracts)`. |
| `target-protocol` | Valid/invalid requests/responses, fake/boundary adapters, grammar/error vectors and `frozen-0.3.1`; Rust protocol/transport tests and contract gate. |
| `trace` | Full/partial/invalid manifests, canonical Planner golden and SHA-256 sidecar; Rust trace tests, `T(trace-contracts)`. |
| `transaction-concurrency` | Valid/invalid sidecars and diff/permutation; corresponding Rust/script tests. |
| `transport-http` | Project/attachment/scenarios, invalid/diff and derived projection expectations; corresponding Rust tests, transport gates. |
| `validation` | Valid base, 19 invalid project directories, warning case, strict JSON and default human goldens; CLI `validate_semantic`, profile contract gate. |
| `versioning` | Compatibility golden and unpublished-protocol family; core `versioning`, CLI `compatibility`/`migrate`, `T(versioning-contracts)`. |

### Script and Rust infrastructure

All 100 script gates fall into the following complete inventory. Names in this list are the middle of `scripts/test-<name>.mjs`:

- Contract/schema/inventory (54): `adapter-manifest-contracts`, `artifact-manifest-contracts`, `authority-contracts`, `authorization-contracts`, `bindings-contracts`, `cache-contracts`, `classification-contracts`, `client-sdk-contracts`, `context-contracts`, `contract-versions`, `contracted-contracts`, `diagnostic-contracts`, `doctor-contracts`, `effect-graph-contracts`, `error-contracts`, `expressions-contracts`, `extended-effects-contracts`, `fixture-provenance`, `graph-contracts`, `impact-contracts`, `implementation-contracts`, `inspect-contracts`, `invariant-transition-contracts`, `laravel-migration-contracts`, `lockfile-ajv`, `lockfile-contracts`, `model-ajv`, `model-contracts`, `native-gate-contracts`, `nfr-contracts`, `observed-contracts`, `openapi-contracts`, `orchestration-contracts`, `php-operations-contracts`, `php-types-contracts`, `query-model-contracts`, `reference-evaluation-contracts`, `requirements-contracts`, `run-history-contracts`, `scenario-contracts`, `semantic-diff-contracts`, `storage-engine-contracts`, `storage-engine-profile-contracts`, `storage-introspection-contracts`, `storage-mysql-binding`, `storage-projection-contracts`, `structure-contracts`, `target-profile-contracts`, `target-protocol-contracts`, `trace-contracts`, `transaction-concurrency-contracts`, `transport-contracts`, `validation-contracts`, `versioning-contracts`.
- Authority/privacy/CLI: `authority-boundaries`, `authority-cli`, `classification-cli`, `run-history-cli`; `privacy-authorization-subject`, `privacy-authorizing-evidence`, `privacy-cli`, `privacy-coherence`, `privacy-contracts`, `privacy-decisions`, `privacy-evaluator-parity`, `privacy-leak-corpus`, `privacy-output-schema`, `privacy-review-regressions`, `privacy-runtime-cli`, `privacy-schema-parity`.
- Node/generation: `adapter-manifest-golden`, `client-sdk-runtime`, `node-client-sdk`, `node-drizzle`, `node-hono-bindings`, `node-hono-readonly`, `node-native-gates`, `node-openapi`, `node-scenario-tests`, `node-scenario-units`, `node-typescript-kernel`, `node-typescript-scanner`, `node-typescript-transport`, `node-zod-gen`.
- PHP/pilot: `mago-integration`, `php-laravel-adapter`, `php-laravel-migrations`, `php-laravel-native-gates`, `php-laravel-operations`, `php-laravel-parity`, `php-laravel-routes`, `php-laravel-scenario-bindings`, `php-laravel-scenario-tests`, `php-laravel-type-roundtrip`, `php-laravel-types`, `php-laravel-ui`, `pilot-brownfield-ts`, `pilot-laravel-vue`, `planner-model-neutrality`, `observed-baseline`.

Some gates validate committed JSON with pinned Ajv 8.17.1 and semantic invariants; some execute the CLI, launch adapter test files, run generated code, or recompute digests. Schema validity alone does not prove producer/golden byte equality. `model-fixture-manifest.mjs` is a useful existing fail-closed inventory: both model gates require exact, disjoint, sorted schema-valid/schema-invalid directory partitions. Its current-schema-only assertions cannot directly serve as historical fixture policy.

Rust `include_str!`/`include_bytes!` occurs in **22 integration-test files**: core `expressions`, `expressions_projection`, `extended_effects`, `invariant_transition`, `lockfile`, `reference_evaluation`, `requirements`, `scenario`, `storage_engine`, `storage_engine_profile`, `storage_introspection`, `storage_projection`, `target_protocol`, `target_protocol_boundaries`, `trace`, `transaction_concurrency`, `versioning`; CLI `context`, `generate_orchestrate`, `lock`, `requirements`, `trace`. These includes embed both inputs and expectations; an include is not automatically a golden comparison. Other suites dynamically read `expect.json` or discover directories: notably CLI `ir`, `load`, `validate_semantic`, `diagnostics`, and graph/inspect/impact/diff tests. Keep both mechanisms, but require their cases to be registered and actually executed.

Examples of real existing safeguards:

- CLI `ir.rs` runs `load --ir --json`, checks status/reason IDs and IR/source-map goldens; core `ir.rs` checks repeated canonical output and YAML/JSON equality.
- CLI `diagnostics.rs::golden_projections_are_byte_stable_across_reruns` compares both executions and committed output bytes. `validate_semantic.rs` requires each invalid fixture to emit exactly its expected located rule; its valid base compares repeated bytes. This is not a 449-rule coverage check.
- Core `trace.rs` compares canonical bytes and a fixed digest, tests permutation/occurrence safety and full/partial coverage. Several typed attachment suites pair `include_bytes!` invalid documents with `.expect.json` detail expectations.
- Core `adapter_conformance/fixture.rs` embeds common IR/Scenario/transport/storage inputs; Node scenario and PHP parity gates already consume shared committed IR and the orchestration scenario corpus. `ir-minimal.json` is documented as full-kinds compiled IR, despite its name.
- `test-node-scenario-tests.mjs` checks repeated durable run records. `test-php-laravel-parity.mjs` compares semantic step tuples and a shared failing mutation; only the race scenario is unsupported-only. Preserve unsupported as an outcome.

### Provenance, regeneration, and CI

`tests/fixtures/fixture-provenance.json` is `dev.lekalo.fixture-provenance@0.1.0`. `test-fixture-provenance.mjs` requires its family list to equal non-hidden on-disk directories exactly, restricts origin to `synthetic` or `evidence-backed`, and requires opaque permission/license/consent evidence IDs for the latter. It validates declaration structure, not every file's content, license authenticity, consumer coverage, or absence of leaks. A new suite family must be registered; do not weaken this gate.

Existing explicit writers are:

- `scripts/regen-golden-plan.mjs`: executes the bundled native planner over a temporary pnpm fixture; writes `plan.golden.json` and `digest-vector.json`.
- `scripts/regen-adapter-manifest.mjs`, `regen-php-adapter-manifest.mjs`: rewrite shipped package integrity claims using framed path/length/content digests and the manifest self-reference rules. These are release artifact writers, not general snapshot updates.
- `scripts/reserve-030-regen.mjs`, `reserve-031-regen.mjs`, `reserve-032-regen.mjs`, `reserve-040-regen.mjs`: historical version-specific recipes. They write lock/receipt fixtures **and Rust digest constants**. Do not reuse them as a suite-wide update command.
- `scripts/gen-artifact-fixtures.mjs`, `gen-loader-fixtures.mjs`, `gen-run-history-fixtures.mjs`, `gen-storage-engine-profile-fixtures.mjs`, `gen-storage-introspection-fixtures.mjs`, `gen-storage-projection-fixtures.mjs`, `gen-taskhub-scan.mjs`, `gen-trace-fixtures.cjs`, `gen-transport-fixtures.mjs`, `gen-validation-fixtures.mjs`, `gen-zod-golden.mjs` are separate builders. The Zod writer explicitly documents manual invocation/review and replaces its expected directory; validation documents that CI never regenerates it.
- Core examples `regen-client-sdk.rs`, `regen-planner-routes.rs`, `regen-portability.rs`, `regen-storage-derived.rs` and `canonicalize-screen-scenarios.rs` provide additional writers. All must be accounted for before treating an update policy as complete.

No general `update-golden` entry point or `UPDATE_GOLDEN`/`UPDATE_EXPECT`/`UPDATE_SNAPSHOT`/`insta::` mechanism was found in the searched source/scripts. Explicit commands already exist, but there is no shared semantic review record or scope/digest guard.

CI already runs Rust build/tests and many runtime gates on Linux, Windows and macOS; schema contracts run on Ubuntu with Node 18/24, and real Mago runs on Linux. Existing platform restrictions for confinement/native tools are explicit. **There is no shared artifact digest comparison across OS jobs.** Six script names are not directly invoked by `ci.yml`: `test-observed-baseline.mjs`, `test-php-laravel-operations.mjs`, `test-php-laravel-ui.mjs`, `test-php-operations-contracts.mjs`, `test-pilot-laravel-vue.mjs`, `test-planner-model-neutrality.mjs`. The pilot aggregator references four of these, but is itself absent from the workflow; account for this when choosing P0 coverage rather than assuming every script runs in CI.

## 2. Class-by-class gap table

`Partial` means useful existing evidence without all issue #90 guarantees. New paths below are relative to the proposed `tests/fixtures/suite/v1/`; imported cases can refer to the existing families without moving them.

| ID / fixture class | Existing evidence | Gap and concrete addition |
| --- | --- | --- |
| F01 Minimal valid project | `model-v1/valid-minimal`, `structure/valid-minimal`, `loader/valid-zero-modules`. | Partial: choose one executable minimum, register `minimal/project`, golden load/IR/validate/empty graph and declared output channels. Preserve distinctions between empty model and structurally valid project. |
| F02 Every definition/type/effect kind | `ir/valid-full-kinds`, `valid-zod-matrix`, extended-effects, transaction/invariant/error/expression attachments. | Partial: `coverage/kinds.json` must map every current closed definition enum, type constructor and effect kind to a valid case and invalid boundary; add `kinds/<kind>/valid` and `invalid` only where existing cases cannot supply it. Include scalar/enum/value-object/entity/command/query/policy/event/effect/endpoint/scenario/target-binding and nested collection/ref types from the actual enums, not a copied list that can drift. |
| F03 Malformed YAML/JSON | Loader parser/encoding/number/duplicate cases and model parse failure. | Partial: `syntax/{yaml,json}/...` register truncated, duplicate-key, invalid UTF-8/BOM and numeric-bound cases with raw-byte input mode; assert exact parser rule, exit/channel and stable spans. Add valid nearest-neighbor controls. |
| F04 Unknown/duplicate/ambiguous references | Loader ambiguous-short-reference/duplicate-definition; model/model-v1 alias and unresolved cases; invalid conformance IR. | Partial: `references/{unknown,duplicate,ambiguous}` paired cases must reach the intended resolver layer and assert exact IDs/locations, not merely any failure. |
| F05 Imports/cycles/path safety | Loader import-cycle/duplicate/grammar/missing/self/path-escape; structure/discovery and protocol scopes. | Partial: `imports-cases/*` plus runtime `paths/*` recipes for parent/absolute/drive/UNC/symlink escapes and nearest-root discovery; require zero outside reads/writes and portable refusal. Separate legal dependency cycles from forbidden import cycles. |
| F06 Semantic validation failures | `validation/invalid`, warning profile and strict/default goldens; many attachment negatives. | Missing global obligation: `coverage/diagnostic-rules.json` derived against the embedded current registry, with trigger/non-trigger witnesses for **all 449 active rules**, not just `semantic.*`/`validate.*`. Add minimal cases by rule and actual execution receipts. |
| F07 Dependency/effect graph | `graph/golden/planner.graph.json`, `effects/golden`, graph/effects tests. | Partial: `graphs/{dependency,effect}` cases for relation/kind matrix, cycles, duplicate occurrences, declared/detected provenance and input permutation; golden bytes and digests. |
| F08 Inspect/impact/context outputs | Dedicated Planner families and canonical JSON, envelopes, Markdown; rename cases. | Partial: `queries/{inspect,impact,context}` pin selector/options/profiles and missing/stale/partial states; bind outputs to the same Planner IR and add per-output byte comparisons to the shared runner. |
| F09 Semantic diff classifications | Eight model diff pairs, `expect.profiles.json`, attachment-specific diff/permutation cases. | Partial: `diff/<class>` coverage for `unknown`, `data-loss-risk`, `storage-migration-required`, `wire-breaking`, `source-breaking`, `behavioral`, `target-specific`, `additive`, plus semantic equality. Assert profile verdicts, stable change IDs and ordered reasons, including invalid/ambiguous rename history. |
| F10 Scenario IR/results | Scenario contract corpus; reference-evaluation traces/digests; six orchestration scenarios and real Node/PHP parity. | Partial: `scenarios/{happy,idempotent,conflict,denied,rollback,race}` register shared inputs and expected pass/fail/unsupported/infrastructure distinctions; pin clocks/IDs and link generated tests to result evidence. Do not promote race unsupported to pass. |
| F11 Target protocol success/failure | Protocol valid/invalid/frozen fixtures, fake/boundary/hostile adapters, conformance battery. | Partial: `protocol/<operation>/{success,failure}` for describe/scan/bind/validate/generate/verify/plan-clean/clean/plan-native; shared exchange transcripts and exact status/error/request IDs. Timeout, malformed, version/capability and containment probes remain explicit cases. |
| F12 Generated artifact manifests | Artifacts wire corpus, lock/orchestration receipts, Node/PHP package integrity and generated source goldens. | Partial: `generation/{node-typescript,php-laravel}` must inventory every generated relative file, bytes, content digest, ownership and input links; include dry-run/apply/verify, drift, conflict and unowned-file preservation. Package manifests and project artifact manifests are separate contracts. |
| F13 Migration/version fixtures | Version compatibility golden, current-baseline migrate no-op/refusal, `frozen-0.3.1`, historical contract files; storage/Laravel migration fixtures. | Partial: `compat/<family>/<version>` immutable historical inputs plus expected supported/refused/migrated status. Current Model/IR registry has no migration steps or aliases and one supported version per family; do not invent successful historical migrations. Record real future migration edges with before/after/no-op/failure outputs when implemented. |
| F14 Security/path traversal/nondeterminism | Adapter security/protocol tests, project_fs_directory, privacy-leaks, cache/root tests, repeat/permutation tests. | Partial: `security/*` synthetic runtime attack recipes; `determinism/*` varied temp roots, cwd, env, seed, locale, EOL, enumeration order and cache modes. Add whole-tree input/output hygiene and normalizer negative controls. |
| F15 Planner end-to-end model | Multiple Planner copies, orchestration, scenario/reference evaluation, Node/PHP generation, Laravel/Vue and brownfield pilot harnesses. | Missing one audited suite chain: `integrated/planner-p0/fixture.json` connects requirements/model -> load/validate/IR -> graphs -> queries/diff -> scenario -> protocol/generation -> verification/results -> trace/context and manifests. Details in section 4. Brownfield in-process scan fallback is recorded as such, never counted as wire success. |

| ID / golden output | Existing evidence | Gap and concrete addition |
| --- | --- | --- |
| G01 Normalized IR JSON | `ir/valid-full-kinds/ir.golden.json`, spans golden, graph Planner IR, adapter shared IR. | Add `expected/ir.json` plus separate `ir.spans.json`, explicit contract/serializer/newline mode and links to Model inputs. Verify all shared aliases against one declared origin/digest. |
| G02 Diagnostics JSON/SARIF | Six diagnostic envelopes, validation goldens, many expected rule/detail sidecars; Mago SARIF runtime checks. | JSON partial; **no core diagnostics SARIF emitter/golden was found**. Add JSON output per registered rule witness and `diagnostics.sarif.json` once a production SARIF projection exists; required schema/renderer work is a dependency, not a test-only fabricated conversion. Mago SARIF does not satisfy core SARIF. |
| G03 Graph/impact/context JSON | Existing dedicated families and inspect JSON/envelopes. | Register exact documents and selectors as `expected/{graph,effects,inspect,impact,context}.json`; test producer bytes, IR links, canonical order and partial states. Preserve Markdown as additional useful coverage. |
| G04 Semantic diff | `diff/cases/*/expect.json`, profile expectations and attachment diffs. | Register baseline/candidate inputs, all classification/profile witnesses and `expected/diff.json`; review stable semantic changes rather than raw hashes alone. |
| G05 Trace manifest | `trace/golden/planner.trace.json` + `.sha256`, full/partial manifests, requirements/OpenAPI trace outputs. | Add chain-produced `expected/trace.json`/digest with referenced scenario/gate/artifact identities proven present and current; do not manufacture links to claim completeness. |
| G06 Adapter requests/responses | Kernel request/expected files, protocol wire fixtures, dynamically built conformance exchanges. | Add readable `expected/protocol/<step>.request.json` and `.response.json` from shared cases; compare sequence, operation, stable IDs, capabilities and errors, while preserving transport validation and confinement. |
| G07 Generated source trees | Zod `expected/*.ts`, client SDK TS/Go, PHP type/route and migration goldens. | `expected/generated/<target>/...` retains individual reviewable source files and exact generated bytes. Add complete-tree comparison (including unexpected/missing files), mode policy and input provenance; no single base64/archive snapshot. |
| G08 Checksums/manifests | `.expect.json` digest fields, trace `.sha256`, evaluator `digests.txt`, native digest-vector, locks, adapter package manifests. | Add case `checksums.json` and suite `run-manifest.json` with declared digest domains; recompute, never hand-patch hashes. `.expect.json` also holds semantic status/detail or golden filenames, so never assume every sidecar is a digest. |

## 3. Proposed layout and versioning

Use an additive catalog; do not bulk-move or reformat existing fixtures and break `include_*` paths. Introduce one declared synthetic family, `suite`, and preserve the existing 63 families and their gates.

```text
tests/fixtures/
  fixture-provenance.json                 # add suite; retain all existing entries
  <existing-family>/...                   # keep paths, raw bytes, historical data
  suite/
    README.md                            # authoring, byte modes, update/review contract
    schema/
      fixture.schema.v1.0.0.json
      catalog.schema.v1.0.0.json
      run-manifest.schema.v1.0.0.json
      golden-update.schema.v1.0.0.json
    v1/
      catalog.json                       # exact case inventory and consumer coverage
      toolchains.json                    # runner/tool identities and required lanes
      coverage/diagnostic-rules.json
      coverage/kinds.json
      imports/<family>/<case>.fixture.json # descriptors for existing immutable paths
      minimal/project/fixture.json
      kinds/<kind>/<case>/fixture.json
      syntax/<format>/<case>/fixture.json
      references/<case>/fixture.json
      imports-cases/<case>/fixture.json
      diagnostics/<rule-id>/<valid-or-invalid>/fixture.json
      graphs/<case>/fixture.json
      queries/<case>/fixture.json
      diff/<case>/fixture.json
      scenarios/<case>/fixture.json
      protocol/<operation>/<case>/fixture.json
      generation/<target>/<case>/fixture.json
      security/<case>/fixture.json
      determinism/<case>/fixture.json
      compat/<contract-family>/<exact-version>/<case>/fixture.json
      integrated/planner-p0/
        fixture.json
        inputs/...                       # only genuinely new/reconciled inputs
        expected/...                     # separate G01-G08 files, not one blob
        checksums.json
      reviews/<change-id>.json
      reviews/<change-id>.md
```

Each new owned case has `inputs/`, `expected/` and `checksums.json`; imported descriptors refer to existing files. Paths in descriptors are slash-separated **repository-relative**, resolved from a declared repository root, with no `..`, absolute path, symlink escape, or ambient cwd. Dangerous path inputs are content/recipes in security cases, never descriptor paths to follow. Every tracked fixture file is accounted for as input, expected output, descriptor, support source, schema, historical fixture, or reviewed metadata. Support files are not falsely counted as individual test cases.

Define `fixtureSchema = dev.lekalo.fixture@1.0.0`, a separate suite/catalog version, stable case ID (`ir.full-kinds`, `diagnostic.semantic.type-ref-unresolved.invalid`) and monotonically increasing case revision. Pin actual product contract identities independently (e.g. IR `dev.lekalo.ir@0.2.16`, protocol contract `0.3.2`, diagnostics registry `0.4.0`); do not bump product contracts just to version the harness. Major fixture-schema changes get a new directory/reader; compatible metadata additions use an explicit schema successor. A corrected expectation changes its case revision with a reviewed rationale, rather than pretending a fixture-format change occurred.

Required descriptor members: purpose/issue, origin reference, contract pins, input roles and digests, runner ID, argv/options and timeout, platform/tool requirements, expected exit/stdout/stderr roles, rule/kind/class coverage, expected output list and byte mode, determinism seed/clock/ID policy, dependencies on prior outputs, update recipe ID, and size limit. Runner and recipe IDs resolve to a trusted closed registry; fixture data cannot supply arbitrary executable paths or shell strings. Use parameterized production seams for internal-only rules; record the named Rust test entry point as an executable witness.

Historical inputs retain exact bytes/digest and their version, even if the current binary correctly refuses them. Separate immutable historical inputs from version-specific expected compatibility verdicts. Preserve `target-protocol/frozen-0.3.1` and all accepted old contracts. Do not reinterpret `loader/valid-yaml-1-0-0` or `model-v1` as supported old versions merely because of names. The compatibility gate must use the live version registry and explicitly test unsupported unknown/future versions.

## 4. Gate/script list to create

All paths and command interfaces in this section are proposals. Implement in this order, keeping existing gates enabled throughout:

| Stage | New or extended path | Concrete responsibility |
| --- | --- | --- |
| 1 | `scripts/lib/fixture-catalog.mjs`; `scripts/test-golden-catalog.mjs` | Validate fixture/catalog schemas and provenance, exact file/case inventory, refs, unique IDs, contract pins, consumer/recipe entries, bounded sizes and no orphan expected outputs. Adapt `model-fixture-manifest.mjs` without losing its schema/checker partition semantics. |
| 2 | `scripts/lib/fixture-byte-policy.mjs`; `scripts/test-golden-normalization.mjs` | Implement declared per-contract framing and allowed path projections; independent vectors for UTF-8 ordering, numbers, Unicode, LF/CRLF, paths, ordered arrays, invalid bytes and unknown-field preservation. Never recanonicalize every product document using one JSON sorter. |
| 3 | `scripts/run-golden.mjs`; `crates/lekalo-core/tests/golden_suite.rs`; `crates/lekalo-cli/tests/golden_suite.rs` | Read-only verifier: materialize inputs in fresh external sandboxes, run registered producers, check exit/channels, compare bytes and complete trees, recompute digests, emit execution receipts. Rust drivers exercise non-CLI production decoders/projections. Keep a separate actual-output directory; test failure cannot update expected files. |
| 4 | `scripts/test-golden-diagnostic-coverage.mjs`; `scripts/test-golden-kind-coverage.mjs` | Join registry/enums to declared cases **and successful execution receipts**. Require matched trigger/non-trigger witnesses for all active rules, every kind/type/effect and each diff class. Reject nonexistent tests, duplicate IDs, vacuous non-trigger cases, stale registry pins and missing cases. |
| 5 | `scripts/test-golden-determinism.mjs`; `scripts/compare-golden-runs.mjs` | Two cold executions plus warm-cache lane, perturbation variants, before/after checkout hashes, complete manifest comparison, and final cross-OS aggregation. Fail on missing/extra outputs or skipped mandatory rows. |
| 6 | `scripts/test-golden-adapter-shared.mjs` | Feed catalog-referenced shared IR/Scenario/transport/storage through core conformance, Node and PHP consumers; compare semantic outputs and per-target exact trees, reject stale duplicate fixture copies. Extend `adapter_conformance/fixture.rs` and adapter fixture tests to assert catalog IDs/digests without reading user projects. |
| 7 | `scripts/test-golden-planner-e2e.mjs` | Execute the linked P0 chain below with positive and deliberate failure mutations; use actual upstream stage bytes/digests as downstream inputs. This runner must not substitute a bundled canned result for a stage. |
| 8 | `scripts/test-golden-hygiene.mjs`; `scripts/test-golden-compatibility.mjs` | Scan tracked and generated fixture content for credentials/host paths/private data; exercise synthetic attack controls. Freeze historical input digests and execute compatibility/refusal/migration behavior from registered version edges. |
| 9 | `scripts/update-golden.mjs`; `scripts/lib/golden-semantic-summary.mjs`; `scripts/test-golden-update-policy.mjs` | Scoped two-phase generation/review/apply, semantic summary bound to before/after digests, no CI write path, no implicit update env flags. Test stale plan, scope escape, changed input, altered summary, unsupported recipe and partial-write refusal. |
| 10 | `.github/workflows/ci.yml`; existing `scripts/gen-*`, `scripts/*regen*`, core regen examples | Wire the required jobs in section 6. Convert current writers into catalog recipes with explicit candidate-output directories or keep them quarantined as historical/release-only tools. Do not invoke destructive old writers in verification. |

Before stage 4 closes, produce an explicit 449-entry backlog from `contracts/diagnostic-registry.v0.4.0.json` via `src/diagnostics/registry.rs`, not the older registry used by `test-diagnostic-contracts.mjs`. That older gate reads `0.2.16`, whereas production and the validation profile gate use `0.4.0`; retain the old schema gate as compatibility coverage and add current-registry validation. Pair invalid trigger with a minimally changed valid non-trigger **at the same processing stage**; assert stable rule ID/code/severity/location/data and allowed status. A valid project that never reaches the relevant adapter/cache/privacy rule is not a negative witness. Boundary/internal rules may use deterministic injected I/O failure through the production component and a successful control. Unreachable/unimplemented registered rules remain acceptance blockers until implemented or changed through the owning contract process; an exemption list must not turn incomplete coverage green.

Core SARIF is a product capability gap, so schedule a real diagnostics projection (`crates/lekalo-core/src/diagnostics/sarif.rs` and CLI report selection, with the appropriate contract decision) before its G02 gate is required. Compare JSON/SARIF rule IDs, severities, messages and locations, validate against a pinned SARIF schema, and include empty/multiple/Unicode/path cases. Do not present Mago-only validation or a harness-only converter as this implementation.

### One Planner P0 chain

Use the full-kinds model and orchestration scenario corpus as audited starting inputs, reconciling exact semantic IDs with transport/storage/requirements attachments. Describe any necessary model enrichment explicitly in the fixture review; unrelated Planner copies are not interchangeable. The integrated descriptor records this stage DAG:

1. Materialize canonical Model, requirements and typed attachments; load, validate and compile IR/source maps. All references resolve, or a negative mutation stops with the exact diagnostic.
2. Build dependency/effect graphs from that IR; run inspect, impact and context selections. Check stable IDs and links, including declared/detected evidence and missing coverage states.
3. Apply one declared model mutation; compute semantic diff classifications and affected seeds, then verify impact/context consume those seeds and the candidate revision. Formatting-only input must have no semantic diff.
4. Compile/evaluate Scenario IR with fixed clock/ID source: focus happy, idempotent replay, conflict, denial, rollback and unsupported race. Check durable semantic results, not runner log text.
5. Negotiate the real Node adapter and record validate/generate dry-run/apply/verify exchanges. Compare generated source tree, artifact manifest, input digests and write plan; preserve a user-owned sentinel. Re-running must not change source bytes or silently rewrite owned conflicts.
6. Execute generated scenario tests and compare result evidence; require Node/PHP semantic parity where declared capabilities match. Run PHP as a separately provisioned required parity lane; retain explicit unsupported capability results. Add a lying-port mutation that must fail the expected assertion and a stale artifact mutation that must fail verify.
7. Build/check the trace manifest and final context from **the produced** Model/IR, diff, artifacts and result evidence; assert each requirement/scenario/gate/artifact edge references the same revision/digest. Assert partial trace on a removed evidence link. Record lock/checksum manifests and a complete run manifest.

Transport generation and pure protocol exchanges belong in the portable mandatory lane; native process-confinement qualification retains its real platform matrix. Any required unsupported P0 stage must be reported blocked rather than replaced with success. Reuse the brownfield pilot as an additional observed-mode integration fixture, recording its wire-refusal/in-process-fallback distinction; it cannot prove the contracted Planner chain on its own.

### Deliberate golden update

Proposed maintainer flow (metavariables below describe actual plan values, not literal fixture data):

```text
node scripts/update-golden.mjs plan --case <case-id> --reason <issue-and-rationale> --out <external-review-directory>
node scripts/update-golden.mjs apply --plan <plan-file> --accept-plan-sha256 <reviewed-plan-digest>
node scripts/run-golden.mjs --case <case-id> --verify
```

`plan` has no tracked writes: run the normal producer twice in fresh directories, validate schema/semantics/hygiene, show every changed/added/deleted file, and create readable candidate files plus JSON/Markdown semantic summary. Include input, producer/toolchain, normalizer-policy and old/new output hashes, case/contract versions, diagnostic additions/removals/severity changes, graph edges, diff classes, scenario outcomes, protocol capabilities/errors and generated public signatures/file inventory. A hash-only summary is insufficient. Unknown semantic changes require explicit human explanation; large updates are split by family/case. Default selection is mandatory and bounded; no accidental update-all.

`apply` requires that exact explicit plan digest, checks all preimages and source/tool identities again, refuses unrelated/stale/dirty expected paths or any write outside the reviewed list, and preserves unrelated work. Preflight all files before writing; keep an external recovery journal for an interrupted multi-file apply. It writes only listed expected files, their case revisions/checksums and the versioned review record. Inputs, contract schemas, provenance evidence and support code cannot be silently changed as part of accepting outputs. Update those separately and regenerate a new plan.

CI rejects update mode, verifies review-record schema and before/after bindings against the PR diff, recomputes the semantic summary, and requires every golden delta to be explained. Human review is the repository review process; a local `reviewed: true` flag is not proof of review. Keep JSON/Markdown summaries in the PR diff for the approving maintainer and configure the golden-policy check as required. Historical fixtures and release package rewrites require their own scoped recipes; never run `reserve-*` to refresh all expectations.

## 5. Determinism and portability strategy

### Reuse existing mechanisms precisely

| Mechanism already present | Suite policy / remaining hazard |
| --- | --- |
| IR/loader canonical writers sort object keys by unsigned UTF-8 bytes, sort semantic sets, preserve ordered fields/values/history. | Compare their production bytes directly. JS UTF-16 default sort or locale ordering is not interchangeable with UTF-8; use independent non-ASCII/numeric vectors. Never sort behaviorally ordered arrays. |
| Trace canonical serialization uses declared struct/contract field order, not the IR object's alphabetic order; CLI adds one LF, digest excludes it. | Declare output-specific digest domain and newline framing. Keep fixed-order contracts intact. |
| Lockfiles have exactly one trailing LF; lock digest excludes that LF. Other projection fixtures may have no final LF; digest sidecars vary by family. | Modes such as `canonical-payload`, `cli-json-lf`, `text-lf`, `exact-bytes` identify exact expectations. Do not use `trim()` to hide extra whitespace or apply a universal trailing newline. |
| `.gitattributes` pins LF for JSON, SHA-256, Markdown, scripts `.mjs`, YAML, text, TS, PHP and Go. | Extend only as needed for generated SQL/XML/JS/CJS or other text; test Git checkout behavior. Materialize CRLF/BOM/invalid-byte adversarial inputs from recipes or preserve with explicit byte attributes; never rewrite parser test bytes as cleanup. |
| Graph/attachment canonicalizers, BTree maps/sets, sorted fixture discovery and content-based IDs; reference evaluator pins clocks/IDs. | Perturb insertion/enumeration order and physical roots while holding semantic input fixed. Check both stable output and meaningful mutation sensitivity. |
| Adapter package digests frame relative path, byte count and content, excluding/zeroing self-references per contract. | Reuse exact domain, including recursive manifest member stripping. Do not replace package digests with a generic hash of pretty JSON. |
| `run_history/clock.rs` supplies injectable `Clock` and `Ids`; production uses wall time and SQLite-generated opaque IDs. | Use deterministic internal test constructors for exported record semantics; do not golden SQLite database bytes or install a production environment override that weakens real randomness. |
| CLI orchestration tests normalize `targets[*].planId` to `plan-normalized`; parity tests project semantic rows. | Existing normalizations are local, not a suite contract. Prefer deterministic inputs/injection; if an operational value remains variable, validate its syntax/references first and allow only named JSON-pointer projections. Bind hashes consistently; reject new volatile fields. |

### Concrete execution rules

- Run each case in independent external roots using their canonical real paths (Windows aliases and macOS `/var` symlinks already matter to existing guards). Use fixed **logical** repository-relative identities, clock sequences, UUID/ID seeds and locale/timezone for semantic output. Test hostile `LEKALO_PROJECT`, cwd, HOME/TEMP-like environment influence in a controlled subprocess without changing the real user environment.
- Define the portable semantic artifact set explicitly: raw IR, diagnostics, queries, diff, trace, protocol fixtures and generated trees. Operational timing/log/process metadata goes to a separately typed execution receipt, not secretly stripped from arbitrary output. Schema-meaningful timestamps/UUIDs remain tested with fixed values; live history behavior gets relational assertions where injection is unavailable. Do not claim raw live runtime timing is byte-stable.
- Path projections operate only on validated path fields and known sandbox roots; fail if any unknown absolute root remains. Never globally replace backslashes in strings, alter source literals, or normalize a traversal attempt before the path validator sees it. Assert runtime outputs never leak actual workspace/user/temp paths **before** making comparison-only projections.
- Preserve source spans: LF/CRLF versions may have different byte offsets while line/column meaning remains equivalent. Golden each raw span map under its correct input-byte mode and compare a declared semantic location projection across EOL variants. Do not erase byte positions to force equality.
- Strictly validate numeric domains before JS parsing/rewriting can lose integer precision; maintain the existing canonical number/string rules. Include large integers, escaping, Unicode, empty-vs-absent fields, negative zero where accepted, duplicate keys and array-order controls. A normalizer must not turn invalid JSON into valid fixtures.
- Compare full file inventories and byte hashes, not only known filenames. Sort paths by a specified byte order; reject case-fold collisions/reserved names for portable source-tree outputs. Exercise case-sensitive names and symlink restrictions as platform-specific security expectations where necessary, not by weakening production checks.
- Read-only test runs hash catalog inputs/expected paths before and after; no `.lekalo` caches, SQLite databases, vendor trees or reports inside tracked fixtures. Provision tools before running gates and use pinned versions/lockfiles. Cache-disabled and warm-cache runs must agree on semantic outputs while cache-specific receipts are tested separately.
- Set output/file/depth/time limits and emit targeted semantic differences and file paths on failure. Keep compact contract JSON if it is the canonical format, but provide review summaries and small fixtures; do not hide semantics in archives/base64 or thousands of unrelated fields. A checksum accompanies inspectable artifacts, never replaces them.

Observed hazards to resolve/test include locale-dependent `localeCompare` in `adapters/node-typescript/src/transport-extension.mjs`, scattered clock/temporary-path use, hardcoded Rust digest constants alongside sidecars, repeated Planner copies, broad `planId` substitution, raw-source fingerprints sensitive to CRLF, and current-only model inventory assumptions. These are test/design risks from source inspection, not demonstrated regressions in this research run.

## 6. Acceptance-criteria-to-test mapping

Extend the existing workflow rather than replacing product tests. Add catalog/schema/coverage/update-policy checks to `contracts` (existing pinned Ajv and Node 18/24), run portable goldens in `build-test` on all three OSes after Rust/Node provisioning, and add a `golden-cross-platform` job depending on all three manifests. Keep PHP/Mago/native qualification setup explicit. No dependency downloads or writes to expected files occur inside golden verification.

| Issue acceptance criterion | Required verification and CI evidence |
| --- | --- |
| AC1 Clean test run is byte-stable across repeated executions. | `test-golden-determinism.mjs`: two cold runs in distinct roots of the same pinned case set, then warm-cache comparison. `compare-golden-runs.mjs` compares every output path, size, bytes/hash, exit/channel and required case outcome. Input/expected pre/post hashes plus Git status prove no tracked/untracked fixture pollution; passing only one representative fixture is insufficient. |
| AC2 Linux/Windows/macOS path and newline normalization. | All three `build-test` jobs run `test-golden-normalization.mjs` and `run-golden.mjs --verify`; LF/CRLF/Unicode/root-with-spaces fixtures exercise both native separators and explicit drive/UNC recipes. Upload portable output manifests and actual artifact files. `golden-cross-platform` requires identical case IDs, contract/policy versions and normalized semantic byte digests on all OSes, with diagnostics for any absent artifact. Platform-specific confinement/security results are separate mandatory per-platform expectations; they cannot excuse missing portable P0 outputs. |
| AC3 Every core diagnostic rule has positive and negative fixture. | `test-golden-diagnostic-coverage.mjs` obtains the embedded current registry (449 active entries at baseline), validates catalog links, and joins fresh Rust/CLI/adapter execution receipts. Each rule requires a trigger and minimally different non-trigger that reaches the rule's stage, exact diagnostic contract fields, and profile behavior where relevant. Delete a pair or add a rule in a harness negative control: CI must fail. Registry-schema validity or filenames mentioning a rule do not count. |
| AC4 Adapter conformance reuses shared fixtures. | `test-golden-adapter-shared.mjs` verifies catalog IDs/input digests used by core `fixture.rs`, Node test consumers and PHP parity are identical to their registered source; run both successful and deliberately invalid exchanges. Compare per-target source bytes and shared semantic scenario tuples. Missing runtimes in required lanes fail setup; unsupported capabilities remain explicit assertions. |
| AC5 Golden update shows reviewed semantic summary. | `test-golden-update-policy.mjs` recomputes before/after semantic summary for golden changes against PR base and verifies bound input/tool/output digests and explicit update intent. Stale/tampered summary, unlisted file, generic env-update flag and CI-apply probes fail. The PR includes readable summary and generated-source diff for human approval; CI verifies evidence consistency, not whether a human actually read it. |
| AC6 Planner fixture covers P0 end-to-end chain. | `test-golden-planner-e2e.mjs` executes the section 4 stage DAG and asserts all consumed/produced digest links, diagnostic/mutation outcomes, generated source verification, scenario result evidence and final trace coverage. Attach stage manifest; any missing required stage or canned preexisting downstream input fails the gate. Keep Node/PHP parity and explicit unsupported-race expectation. |
| AC7 No secrets or host-specific absolute paths. | `test-golden-hygiene.mjs` scans all catalogued tracked inputs, expected outputs, review summaries and fresh actual outputs before publication. Reuse privacy leak detector/corpus controls, plus current workspace/temp/home roots and drive/UNC/file-URI patterns. Materialize hostile absolute paths from synthetic recipes; if retaining existing synthetic corpus strings, narrowly bind their intentional pattern/path/digest exemptions with justification, never a family-wide skip. Real credentials and actual host identities have no exemption. Check provenance exactness in parallel. |

Required evidence artifacts: case execution list with no missing required rows; per-case output manifests; diagnostic/kind/diff coverage report; semantic update summary when applicable; cross-OS comparison; Planner stage manifest. CI logs that merely say schema-valid, request succeeded, or adapter exited zero do not fulfill these criteria. Full completion requires the SARIF gap and all rule pairs to be resolved, not marked future work in a green suite.

## 7. Risk list

1. **Coverage magnitude and false completion.** 449 active registry entries span more than semantic validation. Inventory source cases first, budget minimal paired witnesses by subsystem, and fail the coverage gate on gaps; do not scope the requirement silently to 19 validator negatives.
2. **Missing core SARIF capability.** G02 requires production renderer/CLI work beyond organizing snapshots. Resolve its contract and owner before claiming issue completion.
3. **Canonicalization can hide regressions.** Sorting ordered arrays, trimming all newlines, dropping volatile-looking members or erasing source spans can make wrong results equal. Use narrow byte policies and mutation tests that prove meaningful changes still fail.
4. **Historical compatibility can be fabricated by filenames.** Current registries have one supported Model/IR/protocol version and no migration chains. Retain old bytes with honest refusal semantics; fixture retention does not promise unsupported migration behavior.
5. **Multiple ownership domains.** Package integrity, artifact manifests, locks, source fingerprints, trace and fixture provenance use different authorities/digest domains. New suite metadata must not rewrite them or assign a new canonical trace owner/home.
6. **Shared fixture drift.** Full-kinds IR, conformance IR and multiple Planner projects can diverge while local tests pass. Catalog lineage/digest checks and integrated stage linkage address this without a disruptive bulk move.
7. **Update self-approval and partial writes.** A writer using the same faulty producer can bless a regression. Keep independent semantic/rule assertions, digest-bound preview, per-case review and all-path preflight; human PR review remains necessary.
8. **Platform and runtime differences.** Symlink permissions, case folding, path aliases, Node Unicode ordering, PHP dependencies, Go formatting, Mago and native confinement differ. Pin provisioning, separate portable and genuine platform outcomes, and never convert a missing required backend into a pass.
9. **CI discovery gaps.** Existing directories/includes/gates do not guarantee execution; six script gates are not directly wired. Require catalog-to-runner-to-CI traceability and fresh execution receipts, while preserving existing behavioral suites.
10. **Privacy versus adversarial fixtures.** Existing synthetic leak/path markers intentionally look sensitive; broad allowlists would hide real leakage. Generate host-dependent attacks at runtime and scope reviewed synthetic exceptions to exact content, with clean and failing scanner controls.
11. **Large/generated corpus cost.** The shipped Node artifact is about 14.8 MB and the fixture tree already has thousands of files. Do not snapshot the adapter bundle again or golden database/vendor/cache blobs; reuse release manifest validation, shard independent cases and retain one complete required aggregate.
12. **Research limit.** The three successful lightweight gates validate only their stated scopes. This report contains no implementation or full-suite/cross-platform pass evidence; its deliverable is the inventory and concrete implementation/CI plan.
