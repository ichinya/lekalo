# Issue #106: adapter SDKs and templates research

Research only, 2026-10-06. Acceptance authority: live [issue #106](https://github.com/ichinya/lekalo/issues/106), read with `gh issue view 106 --repo ichinya/lekalo --json title,body,labels`. Parent [#1](https://github.com/ichinya/lekalo/issues/1) and dependencies [#11](https://github.com/ichinya/lekalo/issues/11), [#27](https://github.com/ichinya/lekalo/issues/27), [#28](https://github.com/ichinya/lekalo/issues/28), [#31](https://github.com/ichinya/lekalo/issues/31), [#32](https://github.com/ichinya/lekalo/issues/32) were read for context; their historical examples do not supersede accepted schemas or production owners. House references: `docs/m7/issue-34-research.md`, `docs/m7/issue-90-research.md`, and `docs/versioning.md`.

Inspected checkout: branch `ichinya/m8-issue-106`, starting HEAD `15bd13917654dc20ad19b089a5bb8e5e29c2099f`, supplied fresh `ichinya/M8` base; initial working tree clean. **The inspected product is still `0.6.5` in `Cargo.toml:9`; the assigned M8 product is `0.8.0`.** Product advancement is an implementation/release prerequisite, not a change authorized by this research. Only `docs/m8/` and a local documentation commit are authorized here.

**Decision (proposal):** build four optional, independently packaged SDKs over canonical JSON schemas and one shared vector corpus. Start with a generated compatibility inventory, then a bounded runtime and templates in Rust/TypeScript, qualify them through the existing confined `lekalo adapter test`, and port the same semantics to Go/PHP. SDK adoption must not affect executable eligibility, host policy, protocol semantics, or conformance verdicts. A standalone fake adapter without an SDK remains the independence control.

**Proposal convention:** every new path, API name, schema member, command, default, gate and package layout in sections headed “Proposed” is future work. Existing inventories are source facts, not results of a runtime acceptance run. Proposed new/changed product contracts take **`0.8.0`** under the assigned M8 release; unchanged contracts keep their exact accepted bytes and versions. Package release versions, protocol wire versions, product version and fixture-format version are separate identities.

## Existing-surface inventory

| Surface | Current owner / evidence | Implication and gap |
| --- | --- | --- |
| Protocol identities and limits | `crates/lekalo-core/src/target_protocol/version.rs:13`, `:27`, `:66`; `contracts/target-protocol.schema.v0.3.2.json`, `contracts/target-protocol.schema.v0.6.4.json`. | Token `lekalo.target/v1`; base `0.3.2`, lint successor `0.6.4`, exact supported set `[0.3.2, 0.6.4]`. Limits: 1 MiB request, 8 MiB stdout, 64 KiB stderr; host default deadline 600000 ms. Generated SDKs must pin both schemas and distinguish their operation sets. |
| Closed wire and semantic validation | `crates/lekalo-core/src/target_protocol/wire.rs:33`, `:305`, `:506`, `:624`, `:986`, `:1097`; client operation validation in `target_protocol/mod.rs`. | Typed request/response/evidence/error/write structures; duplicate keys, null optionals, unknown members and bounds refused. JSON Schema is only part of admission: operation-specific pairing, echo, identity, negotiation and actual writes also matter. |
| Capability selection | `target_protocol/capability.rs:24`, `discovery.rs`, `selection.rs`. | Canonical definitions are an embedded source table, identity `dev.lekalo.target-capabilities@0.6.4`, with per-entry definition versions; there is no standalone capability-definitions JSON contract here. Exact IR compatibility, named full/partial/unsupported/unknown states, provenance and explicit partial policy already exist. SDK cannot turn unknown into full or select by framework name. |
| Permission enforcement | `target_protocol/mod.rs`, `scopes.rs:26`, `plan.rs`, `confinement.rs`, `confinement_windows.rs`; `adapter_package/permissions.rs`. | Host stages a private view, checks policy and consumes plan authority before publication. SDK lexical helpers are defense in depth; they cannot grant roots, network, processes, or real-project write access. |
| Process lifetime | `target_protocol/transport.rs:28`, `:37`; `TargetClient` cancellation APIs in `target_protocol/mod.rs`. | Direct argv invocation, stdin or `--lekalo-request-file PATH`, bounded stdout/stderr, caller cancellation and deadline termination. There is no wire `cancel` operation or streaming JSON sequence to implement. |
| Shared conformance | `adapter_conformance/{mod.rs,check.rs,fixture.rs,report.rs,version.rs}`; `check.rs:305`; CLI `main.rs:1091`, `:4542`; CLI/core `tests/adapter_test.rs` / `tests/adapter_conformance.rs`. | One real host battery, **29 checks**, report identity `dev.lekalo.adapter-conformance@0.2.16`, JSON/JUnit, default/strict profiles. Strict requires the nine base operations including plan-native; lint has separate negotiated coverage. No SDK dependency is required by the suite. |
| Handwritten independence control | `tests/fixtures/target-protocol/fake-adapter.mjs`; `target_protocol.rs`, `target_protocol_boundaries.rs`, `target_protocol_security.rs`. | Dependency-free Node executable already demonstrates the wire boundary and hostile modes. Preserve it as an independently maintained control, not generated from SDK code. |
| Existing adapter helpers | `adapters/node-typescript/src/kernel.mjs:459`, `:689`, `:906`, `:1230`, `:1306`, `:2684`; `adapters/php-laravel/src/kernel.php`. | Reusable behavior exists inside target kernels, but no public adapter SDK packages. Node helper `buildResponse` enforces error pairing; its existence does not establish full validate-before-write SDK semantics. Extract wire-only logic without compiler, scanner, framework or `lekalo-core` coupling. |
| Diagnostics | `contracts/diagnostic.schema.v0.2.16.json`; registry `diagnostic-registry.v0.6.4.json`; `diagnostics/version.rs:54`, `target_protocol/diagnostic.rs`. | Diagnostic wire remains `0.2.16`, ordinary embedded registry `0.6.4`, architecture opt-in successor `0.6.5`. Wire `Finding`, `OperationError`, and full core `Diagnostic` are different types; the protocol has no arbitrary diagnostics-array slot. |
| Manifest and scenario evidence | `contracts/artifact-manifest.schema.v0.2.16.json`; `artifacts/{types.rs,canonical.rs}`; `contracts/scenario-ir.schema.v0.2.16.json`, `scenario-run.schema.v0.4.0.json`; `scenario_evidence.rs`. | Artifact canonicalization/digest and scenario pass/fail/unsupported/infrastructure/degraded semantics have owners. A generated type alone cannot validate cross-field links or assert host-owned artifact publication. |
| Package trust and custody | `contracts/adapter-manifest.schema.v0.3.2.json`, successor `v0.6.4.json`; `adapter_package/{manifest.rs,integrity.rs,signature.rs,trust.rs}`; CLI `main.rs:4590`. | Adapter test performs manifest/integrity/trust/revocation gating before child launch, preferring a real adjacent manifest over an implicit local-development descriptor. SDK packages and adapter executable manifests are distinct release artifacts. |
| Fixture and CI reuse | `tests/fixtures/adapter-conformance/`, `tests/fixtures/target-protocol/`; `tests/fixtures/suite/v1/catalog.json`; `tests/fixtures/fixture-provenance.json`; `.github/workflows/ci.yml:90`, `:223`, `:385`, `:412`. | Existing synthetic, shared and golden infrastructure is present at this base. Add SDK cases to it; do not assume the older M7 research gaps still describe this checkout. Provision/build before binary-dependent gates. |
| Context-adapter scope | No separate accepted context-provider schema/owner appeared in the inspected contract/source inventory; roadmap parent names #83 as its API owner. | Generic lifecycle, diagnostics, bounded codecs and harness can be reused. A target `scan` reply is not a newly accepted context-provider result. Context-specific transport/types need the owning contract before a context template can claim conformance. |

Source outranks stale prose: `docs/target-protocol.md` still describes singleton `0.3.2` acceptance and `docs/adapter-conformance.md` says 28 checks, while source includes the lint version and `classification.preservation` as check 29. Correct those guides in implementation; no such edits occur here. Existing endpoint/client SDK (`docs/client-sdk.md`, issue #72) is a different product surface.

## Proposed goal decomposition and ownership

All increments below belong to eventual implementation. Each has a reviewable exit condition; completion of the Rust/TypeScript acceptance lane alone does not silently remove Go/PHP from the issue's SDK contents.

| Increment | Proposed paths / owner seams | Deliverable and exit condition |
| --- | --- | --- |
| I1: canonical input inventory and compatibility | `sdk/README.md`, `sdk/schema-inputs.json`, `scripts/generate-adapter-sdk.mjs`; new `contracts/adapter-sdk-compatibility.schema.v0.8.0.json`, `contracts/adapter-sdk-generation.schema.v0.8.0.json`. | Enumerate schemas, transitive local refs, registry/capability pins, type projections and semantic obligations; pin generator/tool bytes and versions. A deterministic `--check` verifies committed output and compatibility vectors without tracked writes. |
| I2: language-neutral helpers and vectors | `sdk/spec.md`; `tests/fixtures/adapter-sdk/{valid,invalid,semantic,canonical,security,lifecycle,upgrade}/`; per-language generated and runtime modules. | One documented decode/build/write contract with shared positive/negative vectors, error tokens, canonical bytes and bounded transport rules. Distinguish schema-generated members from manually implemented semantic checks. |
| I3: Rust and TypeScript SDKs | `sdk/rust/{Cargo.toml,src/generated/,src/runtime/,tests/}`; `sdk/typescript/{package.json,src/generated/,src/runtime/,test/}`. | Publishable `lekalo-adapter-sdk-rust` and `@lekalo/adapter-sdk`, no `lekalo-core` import/dependency. Independent package roots/lockfiles and explicit MSRV/toolchain pins; keep publish configuration separate from the current unpublished core workspace. Typed builders reject malformed success before emission. |
| I4: templates and the SDK-free control | `sdk/templates/{rust,typescript}/{scan,generate,verify,reference}/`; `scripts/test-adapter-sdk-conformance.mjs`; preserve existing fake adapter. | Minimal public-author projects build offline after explicit provisioning. Rust and TS reference templates implement the suite's complete base surface and pass strict conformance; capability-limited scan/generate/verify examples pass default plus their required feature rows, with honest skips. Compare wire bytes and failure classes with the handwritten control. |
| I5: Go and PHP parity | `sdk/go/{go.mod,generated/,runtime/,tests/}`; `sdk/php/{composer.json,src/Generated/,src/Runtime/,tests/}`; matching template directories. | `lekalo-adapter-sdk-go` and `lekalo/adapter-sdk-php` reuse the same vectors and process suite. Composer/NPM lifecycle scripts are absent by default; Go module import path is documented independently from the issue's package display name. Missing required runtime is a failed setup, not skipped acceptance. |
| I6: release, author docs and integration | `docs/adapter-sdk.md`, updates to `docs/adapter-conformance.md`, `docs/target-protocol.md`, `docs/versioning.md`; `scripts/test-adapter-sdk-packages.mjs`, CI/release jobs. | Document lifecycle, policy, exact compatibility and standalone authoring; verify package contents, checksum/provenance and detached consumer installation. Local package qualification precedes any separately authorized public release. |

Runtime API proposals use equivalent concepts in all languages: `decodeRequest`, `validateRequest`, `describe`, `buildSuccess`, `buildError`, `validateResponse`, `writeResponse`, `validateLogicalPath`, `validateWritePlan`, `buildFinding`, `buildDiagnostic`, `buildArtifactManifest`, `normalizeScenarioResult`, `stderrLogger`, and `runConformance`. Generated types have no ambient I/O. SDK-local errors use a closed `SdkError` enum with safe reason tokens; the host owns the public domain result and status/exit mapping.

Context adapters reuse I1-I3's generic package layers. Document the context API as **Planned**, and do not ship a target-protocol wrapper labelled a verified context provider. If that owner supplies an accepted schema during implementation, add its exact pin, generated types and consumer conformance lane in a separate reviewed increment. If a new context wire contract is introduced under M8, it takes `0.8.0`; no implicit core import or permissions are added.

## Proposed CLI and lifecycle contract

**No new core CLI subcommand or flag is required for the minimum #106 implementation.** SDK conformance clients are bounded wrappers over the existing command, using executable plus argv arrays with no shell. They retain native verdict/status/exit, JSON or JUnit, check IDs/classes/skips and exact badge pins; they never award a badge themselves or reduce a refusal to boolean success.

Existing author commands (source-verified syntax; paths to new templates are proposals):

```text
lekalo adapter test --profile strict --report json --repeats 3 --timeout-ms 60000 node <standalone-typescript-reference.mjs>
lekalo adapter test --profile strict --report junit --repeats 3 --timeout-ms 60000 <rust-reference-executable>
lekalo adapter test --profile default --report json --repeats 3 --timeout-ms 60000 node <standalone-scan-example.mjs>
lekalo adapter test --profile strict --report json --repeats 3 --timeout-ms 60000 node tests/fixtures/target-protocol/fake-adapter.mjs
```

Core flags must precede the program; all later arguments belong to the adapter. `--repeats` accepts 2-8, `--timeout-ms` 1000-600000. There is no `--fixture`, `--sdk`, or `--report-file` option on this command. Wrapper report persistence uses an explicitly granted destination outside input fixtures, validates both terminal envelopes and completed report documents, and does not assume that a preflight package refusal emits a completed suite report.

Proposed maintainer commands:

```text
node scripts/generate-adapter-sdk.mjs --check
node scripts/generate-adapter-sdk.mjs --write --language all
node scripts/test-adapter-sdk-contracts.mjs
node scripts/test-adapter-sdk-vectors.mjs --language all
node scripts/test-adapter-sdk-conformance.mjs --language rust --profile strict --repeats 3 --timeout-ms 60000
node scripts/test-adapter-sdk-conformance.mjs --language typescript --profile strict --repeats 3 --timeout-ms 60000
node scripts/test-adapter-sdk-packages.mjs --verify
```

`--write` is an explicit development-only generator action bounded to generated SDK files and their generation records; CI permits only `--check`. `--language` is closed to rust/typescript/go/php/all. Test wrappers resolve fixed, built template entries from reviewed metadata; fixture content cannot supply shell strings. Both per-language conformance commands run the **same** core suite. Provision dependencies beforehand with pinned lockfiles; no generator, SDK runtime, harness or adapter operation downloads dependencies, phones home, installs runtimes, or updates templates.

Proposed lifecycle rules:

1. Read exactly one bounded UTF-8 JSON request from stdin or the host-owned `--lekalo-request-file PATH`. This file coordinate is transport-owned, not a portable project path or an extra project-read grant. Reject ambiguous input sources, trailing documents, duplicate decoded keys, invalid UTF-8, unsupported versions and invalid operation shapes before invoking callbacks.
2. Echo host `request_id`, operation and negotiated version. `describe` reports actual executable identity/digest, precise operations/scopes/capabilities/IR pins. Unknown and partial support remain explicit; scope/capability claims do not widen host policy. No hidden handshake cache authorizes a later process; host session custody controls every call.
3. Default adapter configuration is read-only, with no write scopes or network/subprocess helper. Generation examples opt into a narrow output scope and plan before apply; IR read scope is explicit. SDK file helpers operate only inside the injected view, validate lexical and physical confinement, refuse links/special entries and protected homes, and never discover the ambient repository as recovery.
4. Dry-run computes a sorted plan and exact output byte hashes without writing. Apply echoes the host's opaque `plan_id` and checks the supplied plan, input/before-state and supported action; it cannot mint or persist host authority. The host retains whole-stage verification, consumption of apply authority and actual publication/rollback.
5. Build into an in-memory bounded response, then validate schema **and semantic invariants** before writing any stdout bytes. Success is operation-specific: false `ok`, findings, unknown/unsupported evidence, wrong identity/echo, extra members, invalid writes or mismatched hashes cannot silently pass. For a known request, emit a valid closed error envelope when possible; for undecodable input or invalid builder output, emit bounded safe stderr and exit nonzero without a fabricated success.
6. Emit a single compact JSON response on stdout. Logs use a bounded, redacted stderr logger and never raw request/source/environment content. Progress stays in the accepted response member; stdout banners and streaming records are forbidden. Human output is outside adapter wire mode.
7. Expose cooperative deadline/cancellation tokens to callbacks using accepted request limits and local process termination signals where supported. Cap local work and output at the tighter of host request and SDK bounds. Hard cancellation and descendant reaping remain host responsibilities; do not add a `cancel` operation, retry automatically, or catch termination and emit success. A new attempt requires fresh host negotiation/plan custody.

## Proposed contract artifacts and version decisions

| Artifact | Proposed identity / fields | Version and compatibility decision |
| --- | --- | --- |
| SDK compatibility descriptor schema | `contracts/adapter-sdk-compatibility.schema.v0.8.0.json`; identity `dev.lekalo.adapter-sdk-compatibility@0.8.0`, discriminator `lekalo/adapter-sdk-compatibility/v0.8.0`. Closed fields: language, SDK package/version, wire token, sorted exact protocol versions, IR versions, per-version operation sets, schema identities/digests, diagnostic registry pin, capability-definition pin, host-policy prerequisite and generated test-vector references. | New descriptive contract `0.8.0`. Exact supported set initially `0.3.2` and `0.6.4`; **not** a blanket numeric range. Upgrades are opt-in generated successors and compatibility tests. It grants no permissions or runtime installation authority. |
| Generation record schema | `contracts/adapter-sdk-generation.schema.v0.8.0.json`; identity `dev.lekalo.adapter-sdk-generation@0.8.0`, discriminator `lekalo/adapter-sdk-generation/v0.8.0`. Closed fields: generator/version/digest, ordered schema/ref inputs with SHA-256, language/toolchain/options, output relative paths/digests, compatibility descriptor digest and semantic-validation coverage mapping. | New reproducibility contract `0.8.0`; deterministic, no host roots/timestamps/secrets. It records build inputs; it is not proof that a publisher signed a release. |
| Generated outputs | `sdk/<language>/generated/` plus per-language `compatibility.json` and `generation.json`. | Records validate against the new schemas; types retain exact upstream wire spellings, null/absence rules and limits. Package version can start independently (proposed `0.1.0`), without changing protocol versions or claiming product `0.1.0`. |
| Existing target protocol | `target-protocol.schema.v0.3.2.json` and `v0.6.4.json`. | Freeze both; new SDKs are consumers, not a reason to bump the wire. Transitive refs, including lint evidence, are generated from a bounded local schema inventory, never fetched over network at runtime. |
| Diagnostic and capability definitions | Diagnostic `0.2.16`; ordinary registry `0.6.4` / architecture opt-in `0.6.5`; embedded capability-table identity `0.6.4` with individually versioned definitions. | Freeze exact accepted pins. SDK diagnostic projection is generated from a selected canonical registry, not a second source of codes. Export capability definitions from the reviewed canonical owner into the generation inputs, with identity/source digest and per-entry versions; do not invent an existing JSON schema or require SDK users to import its Rust implementation. A real new rule/capability requires its owner and a `0.8.0` successor with consumers updated. |
| IR, artifact and scenario contracts | IR/Scenario IR/artifact manifest `0.2.16`; scenario-run `0.4.0`; adapter-package manifest versions `0.3.2` / `0.6.4`. | Freeze unchanged bytes. Protocol `writes` and host artifact-manifest entries are not interchangeable. SDK artifact builders produce validated candidates; the host verifies ownership/lock/input/content links and owns durable publication. |
| Conformance report and fixture format | Existing report `0.2.16`, existing suite catalog/fixture format `1.0.0`. | Reuse without an SDK-specific badge/report dialect. New fixture cases do not force format changes. If a new report field or context suite is actually required, publish an explicit `0.8.0` successor rather than editing a frozen artifact. |

Generation resolves only reviewed local `$ref` targets, rejects missing/cyclic unsupported generator constructs explicitly, and emits no handwritten broad `any`/unchecked map as a successful fallback. Each schema definition maps to a generated type or an explicit bounded hand-maintained codec obligation. Strict decoders retain numbers/Unicode safely: JS integer precision, PHP array/object ambiguity, Go number decoding and Rust collection order require shared boundary vectors. UTF-8 byte ordering, preserved ordered arrays and contract-specific LF/digest framing must match production; a universal JSON sorter is insufficient.

On a future protocol upgrade, regenerate the exact compatibility matrix and types together: old accepted inputs retain their result, newly admitted inputs succeed only under the new version, new fields/operations under the old version fail, unknown/future versions fail before IR/callback execution, and downgrade never drops evidence silently. Run through the actual host decoder as well as SDK codecs. Advertising a schema just because generation succeeded is insufficient without this matrix.

## Diagnostic-family and registry implications

**Proposed new `LEK-*` codes: none for the minimum SDK work.** Existing host rules cover malformed replies, unsupported versions, confinement, lifecycle and package failures. SDK-local generator/build errors are bounded development failures, not new public registry entries. A full diagnostic builder validates a selected canonical registry and bounded metadata; it cannot choose a new stable public code, fabricate a source span, or put full diagnostic records into a wire `Finding` slot.

The following exact existing mappings were read from `contracts/diagnostic-registry.v0.6.4.json`; runtime mapping belongs to `target_protocol/diagnostic.rs` and `adapter_conformance/diagnostic.rs`:

| Existing rule / code | SDK/helper obligation and host classification |
| --- | --- |
| `target.request-invalid` / `LEK-TGT-001`; `target.handshake-required` / `LEK-TGT-002` | Validate request shape and lifecycle before callback/apply; host invalid result. |
| `target.protocol-mismatch` / `LEK-TGT-003`; `target.capability-unsupported` / `LEK-TGT-004`; `target.ir-unsupported` / `LEK-TGT-016` | Refuse exact incompatibility before IR disclosure or generation; host unsupported result, never optimistic success. |
| `target.scope-violation` / `LEK-TGT-005`; `target.protected-path` / `LEK-TGT-006`; `target.dry-run-mutation` / `LEK-TGT-007` | Check scope/path/planning defaults; host denied result. SDK checks do not replace OS confinement. |
| `target.plan-mismatch` / `LEK-TGT-008`; `target.operation-failed` / `LEK-TGT-009` | Preserve invalid plan/operation failure; SDK cannot normalize a partial error into success. |
| `target.response-invalid` / `LEK-TGT-010`; `target.transport-failed` / `LEK-TGT-011`; `target.timeout` / `LEK-TGT-012`; `target.crash` / `LEK-TGT-013`; `target.output-limit` / `LEK-TGT-014`; `target.cancelled` / `LEK-TGT-015` | Host unavailable/infrastructure path remains distinct from model/test failure. Validate-before-write and bounded logging prevent SDK-created malformed replies. |
| `adapter.diagnostic-invalid` / `LEK-ADP-001`; `adapter.check-failed` / `LEK-ADP-002`; `adapter.process-failure` / `LEK-ADP-003`; `adapter.protocol-failure` / `LEK-ADP-004`; `adapter.security-failure` / `LEK-ADP-005` | Preserve shared suite classifications, failure precedence and native diagnostic evidence. Hard security/protocol failures cannot be offset by passing feature rows. |
| `adapter.manifest-invalid` / `LEK-ADP-006`; `adapter.checksum-mismatch` / `LEK-ADP-007`; `adapter.manifest-mismatch` / `LEK-ADP-008`; `adapter.permission-escalated` / `LEK-ADP-021` | Package/integrity/policy gates remain before execution. SDK package provenance does not bypass adapter host trust. |

No registry/profile/provider successor is required merely to publish SDK code. If implementation finds an uncovered externally visible failure, allocate its code through the diagnostic owner, publish registry changes at `0.8.0`, and update pinned builders, golden trigger/non-trigger pairs, profiles/consumers only where they reference the changed registry. Do not preallocate an unneeded `LEK-SDK-*` family in this research.

## Proposed fixtures, gates and CI evidence

| Family | Required cases and meaningful gate |
| --- | --- |
| Generated-schema inventory | Exact schema/ref closure, identity/digests, supported operation/version set, deterministic regeneration, removed/unknown definition and deliberately stale generated file. `test-adapter-sdk-contracts.mjs` plus generator `--check` fail on drift and omissions. |
| Codec and semantic parity | Every operation's valid request/response and invalid near-neighbor; duplicate/escaped keys, explicit null, extra fields, invalid UTF-8, oversized/deep input, wrong request/adapter/plan echo, error/result pairing, false success and bounds. `test-adapter-sdk-vectors.mjs` compares all four codecs with production Rust admission, including schema-valid semantic negatives. |
| Canonicalization and builders | Permuted keys/sets, ordered arrays, ASCII/Cyrillic/astral/combining strings, numeric limits, digest framing and trailing LF; malformed artifact/source-map/scenario links and unsupported/infrastructure outcomes. Repeat bytes and cross-language semantic comparisons; mutation of one result/hash must fail, not be normalized away. |
| Permission and write plan | Traversal, absolute/drive/UNC/backslash paths, DOS names, segment/path limits, symlink/reparse/hardlink attacks, protected homes, empty reads, undeclared write, write-only source disclosure, read-only dry run, consumed/stale apply plan. Run host security probes against SDK and bypassing hostile executables; verify canonical/outside files unchanged and no stage publication after refusal. |
| Lifecycle/logging | Stdin/file transport, deterministic response, stderr logs without stdout pollution, timeout/hang, output overflow, callback crash, cancellation/recovery, fresh handshake and replan. Assert actual terminal class and descendant cleanup; no successful response from an invalid builder. |
| Reference/template conformance | Shared `adapter-conformance` IR, invalid refs, transaction/concurrency and protected markers; strict SDK Rust/TS references plus handwritten fake; default examples assert required feature rows individually. Check every mandatory row, exact versions/badge, report shape and report replay bytes. All-skipped feature probes do not qualify the advertised example. |
| Upgrade compatibility | Generated exact-version matrix over base/lint; unknown version, incorrect cross-version operation/member, IR mismatch, new protocol candidate and stale output. Tests require host support before a new version can be published as compatible. |
| Packages and provenance | Offline detached author builds from packed artifacts, complete import/runtime asset closure, no internal crate coupling, no lifecycle hooks/telemetry/network, integrity/checksum recomputation, missing/tampered file, unpack path escape, publisher/signature-policy metadata and source/build linkage. `test-adapter-sdk-packages.mjs --verify` reads release candidates and never publishes. |

Register new `adapter-sdk` fixtures as synthetic in `tests/fixtures/fixture-provenance.json`, add descriptors/consumer refs in the existing suite catalog, and reuse existing source fixtures by exact digest instead of cloning Planner or bundling target compilers. Adversarial paths/secrets are synthetic runtime recipes; reports must not contain actual workstation paths, credentials, raw stdout/stderr or private source.

CI additions belong in the existing workflow: schema/generator gates in contracts; Rust/TS compile/package gates; provisioned Go/PHP parity lanes; confined template conformance on Linux/Windows/macOS; digest aggregation for portable codec outputs. Build core and template binaries/bundles in the **same job before** wrappers invoke them. Provision pinned Ajv 8.17.1 externally and expose both `LEKALO_AJV_NODE_PATH` and `NODE_PATH` for the existing schema gate. Linux bwrap/macOS sandbox-exec/Windows confinement prerequisites remain required; no unconfined fallback. Network-disabled runtime probes verify the declared no-network behavior independently of source scans.

Proposed implementation validation, after provisioning/building the selected packages:

```text
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo build --workspace --locked
cargo test -p lekalo-core --test target_protocol --test target_protocol_boundaries --test target_protocol_security --test adapter_conformance --locked
cargo test -p lekalo-cli --test adapter_test --locked
node scripts/test-target-protocol-contracts.mjs
node scripts/test-adapter-manifest-contracts.mjs
node scripts/test-adapter-manifest-golden.mjs
node scripts/test-fixture-provenance.mjs
node scripts/test-golden-adapter-shared.mjs
node scripts/check-contract-versions.mjs --base HEAD^
cargo test --workspace --locked
```

Add I1-I6's proposed commands to these existing regressions and run the applicable standalone package formatting/tests/builds. Root workspace checks do not validate an independent SDK package automatically. Required evidence distinguishes schema admission, semantic admission, real confined exchanges, packed-package author builds, hosted platform results and public package publication; a green local codec run proves only its own lane.

## Acceptance-criteria-to-test mapping

The left column preserves issue #106's seven acceptance criteria **verbatim**, including checklist text. The right column is proposed implementation evidence, not completion of the issue by this document.

| Issue acceptance criterion (verbatim) | Increment / required evidence |
| --- | --- |
| - [ ] Reference fake adapter can be implemented with and without SDK. | I3-I4: independently maintained handwritten fake plus Rust/TS SDK references implement the same exact wire surface; run the same strict host battery and compare deterministic exchanges and controlled failures. SDK-free adapter remains eligible without an SDK manifest or Rust library. |
| - [ ] TypeScript and Rust templates pass conformance suite. | I4: build detached packed-package templates, run real confined `adapter test --profile strict --report json` and JUnit projection for both reference templates; require pass, mandatory rows and exact badge. Feature-limited examples separately prove their declared operation rows under default. |
| - [ ] SDK never grants broader permissions than adapter host policy. | I2-I5: scope/write checks and no network/shell defaults; SDK and malicious bypass variants hit the same production host denial, no canonical/outside writes or overbroad read exposure. Inspect request/profile plumbing and actual private-view publication, not only helper return values. |
| - [ ] Invalid response cannot be emitted silently as success. | I2-I5: mutate builder result/echo/identity/write hashes/nulls/bounds and error pairing; writer rejects before any stdout bytes or returns a valid error for a known request. Raw malformed bypass is rejected by host as response invalid, and invalid scenario evidence never rolls up as pass. |
| - [ ] Protocol upgrade produces generated compatibility tests. | I1-I2: generator emits version/operation/schema-digest matrix and accept/refuse tests; candidate schema mutation fails `--check` until generated outputs/tests are reviewed. Execute old/new/unknown and cross-version cases through SDK and host codecs, refusing unsupported negotiation. |
| - [ ] Documentation covers lifecycle and security boundaries. | I6: public author guide covers describe, per-exchange process lifetime, limits/transports/logging, cancellation, dry-run/apply authority, isolation/trust, exact compatibility, errors/redaction and package release custody. Copyable examples are exercised by template gates. |
| - [ ] Third-party adapter author does not need to understand internal Rust crates. | I3-I6: detached Rust/TS/Go/PHP projects consume published-format package candidates and canonical public schemas only; no path dependencies/imports to internal crates, monorepo-relative assets or compiler internals. Author walkthrough builds/runs/conforms from a clean external root and documents the SDK-free alternative. |

### Requirements traceability

| Issue SDK content / requirement | Planned proof |
| --- | --- |
| Generated protocol request/response types; generated from canonical schemas where practical | I1-I3/I5 schema closure, generated outputs, explicit semantic-codec obligations and stale-output negative controls. |
| Handshake/capability helpers; exact protocol compatibility range | Exact-set descriptors and base/lint matrix; unknown/partial refusals; host version/IR handshake before operational data. |
| Diagnostic builders; scenario/evidence result types | Selected registry validation, separate Finding/OperationError/Diagnostic APIs; exact Scenario IR/run types with status propagation and bounded evidence. |
| Path/write-plan validation helpers; artifact manifest builders | Shared lexical/physical path vectors, dry-run/apply checks, independent host enforcement; artifact digest/custody rules checked against production. |
| Cancellation/timeout helpers; logging to stderr | Local cooperative token and safe logger tests, real host deadlines/cancel/output caps; no new wire opcode or stdout logs. |
| Deterministic sorting/serialization; fixture harness; conformance client | Contract-specific byte policies, shared synthetic corpus, cross-language comparison and wrappers over the one host suite. |
| Minimal templates; scan/generate/verify examples | Detached per-language author examples, strict Rust/TS references and operation-specific feature assertions. |
| SDKs remain optional; no core library coupling beyond wire contracts | Existing fake remains independent; SDK dependency/import audits and detached package consumers. |
| No hidden telemetry/network; read-only, bounded output, no shell | Dependency-free runtime behavior where practical, explicit read/write views, network-disabled runs and process argv audits. Build-time provisioning is explicit and separate. |
| Independent release/versioning; package provenance/checksums | Independent package versions, exact frozen upstream pins, packed file/checksum inventories and source/build attestation verification; no implication that ordinary hashes are publisher signatures. |
| Four first SDK names | Rust, TS, Go and PHP package roots in I3/I5 use the issue's names; Go import coordinates and publisher namespace availability are finalized before public release. |

## Risks, non-goals and decisions requiring implementation evidence

1. **Generated types mistaken for validated values.** Schema generation cannot encode every operation/identity/plan/canonical invariant. Keep validate-before-write, production-oracle tests and explicit semantic coverage; unchecked casts/maps cannot satisfy admission.
2. **Four languages drift in Unicode/numbers/serialization.** Define common byte vectors and per-contract framing, including error-code code-point bounds, JS precision and PHP object/list distinctions. Compare real bytes instead of replacing them with a universal pretty-printer.
3. **SDK helpers mistaken for host security.** Third-party executables can bypass helpers. Qualify actual host denials and publication custody; never add permissions, change confinement policy or enable arbitrary subprocess/network execution to make templates pass.
4. **Runtime asset closure fails under confinement.** Current host copies an executable and first-argument script; arbitrary external package assets have no implicit grant. Bundle TS/PHP references into approved standalone entries or use an explicitly owned runtime-bundle contract. A local unconfined import test is insufficient.
5. **Conformance pass hides skipped claims.** Strict base surface and required example feature rows have separate obligations; a default describe-only pass cannot prove scan/generate/verify behavior or context-provider compatibility. Preserve hard failure precedence and skips.
6. **Context API is unresolved.** The issue title includes context adapters but no accepted context wire contract was found. Do not invent a protocol dialect or call target scan semantic context verification. Reusable helpers can ship; context-specific verification remains explicit owner-dependent work.
7. **Schema/version documentation drift.** Inspected product `0.6.5` differs from assigned `0.8.0`; old prose differs from source. Advance the product in the proper implementation/release change before adding `0.8.0` contracts, update accepted references, and keep unchanged contract bytes frozen.
8. **Supply-chain promises exceed evidence.** Package namespace availability, deterministic archives, signing/attestation provider and public registry coordinates need live release-time verification. SDK package provenance neither signs generated adapters nor overrides #32 trust/quarantine/revocation.
9. **Dependency/MSRV creep.** Existing core MSRV is Rust 1.80.0; standalone SDKs need explicit supported compiler/runtime matrices. Pin generator dependencies/toolchains and avoid introducing internal crates or target compiler bundles into a general-purpose SDK.
10. **Independent SDK releases accidentally negotiate new protocols.** A package version upgrade cannot widen an exact accepted wire set. Re-run generated compatibility and host conformance for each declared version; unsupported future versions remain refusals.

Non-goals: implementing target frameworks/scanners/generators, the #72 endpoint client SDK, automatic install/update/registry publication, AIFHub integration, new context semantics, core Rust ABI/plugins, changing Model/IR/ownership/privacy authority, relaxing confinement, hidden telemetry, native application execution, or claiming external provider/public package acceptance from local fixtures.

## Research validation and completion boundary

Read the live issue and its Parent/Depends bodies, the M7 research format and versioning rules; inspected protocol/host/registry/conformance/package/helper sources, schemas, fixture provenance/catalog and CI wiring at the pinned base. No implementation tests, generators, dependency installation, SDK builds, real adapter run, hosted CI or package publication were performed for this documentation task. No public library API choice is claimed; implementation must verify the selected generation/build tooling against its current primary documentation.

Documentation validation checks exact presence of all seven issue acceptance strings, existing inventory paths, required research sections, UTF-8/LF form, whitespace, and the staged one-file custody boundary. The requested outcome is this file in a local HEAD commit named `docs(m8): research issue-106`; no other path changes or push are authorized. SDK implementation and all proposed gates remain future work.
