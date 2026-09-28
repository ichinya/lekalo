# Issue #55 — Mago integration: research and implementation plan

## Scope and baseline

- Authority: `gh issue view 55 --repo ichinya/lekalo`, read first on 2026-09-27; [issue](https://github.com/ichinya/lekalo/issues/55). Dependencies: #11, #14, #27, #42, #54.
- Inspected branch `ichinya/m5-issue-55`, baseline `aca1bb8e` (merged #54 MVP). This document is research, not implementation or acceptance evidence.
- Unlike the older #54 research snapshot, **the PHP adapter exists**. Extend its kernel; do not create a second adapter or treat protocol conformance as semantic PHP validation.
- Mago supplies PHP parsing/analysis evidence, never canonical Model. No PHP parser reimplementation, internal Mago Rust crate coupling, implicit fix, Composer install, or application boot during scanning.
- Paths below are repository-relative; `core/` means `crates/lekalo-core/src/`. Proposed files/symbols are explicitly marked as new.

## Existing implementation and contracts

| Area | Verified entry points and implication |
| --- | --- |
| PHP adapter | `adapters/php-laravel/src/kernel.php::{describe_capabilities,scan_response,dispatch}`. `SCAN_ROOTS` contains only `.lekalo/ir`, `.lekalo/cache`; scan enumerates files as `kind=ir`. `scan.symbols` is unsupported; validate/verify return `ok=true,findings=[]`. These are the integration points, not existing analysis. |
| Packaging | `build.php` concatenates the kernel into `adapter.php`; `adapter.manifest.json` pins artifact integrity and denies children/network. New modules must enter deterministic bundling; never hand-edit generated adapter bytes. Manifest currently grants no PHP source roots. |
| Process protocol | `core/target_protocol/{wire,client,transport,confinement,confinement_windows}.rs`, `contracts/target-protocol.schema.v0.3.2.json`. `Finding` carries only path/code/detail; `ScanEntryEvidence` carries signature and at most eight target/role/confidence references. No diagnostic span, provider namespace, per-edge origin, or full graph transport. |
| Confinement | Only executable and first script argument are copied. Arbitrary sibling Mago/vendor/runtime files are not available. PHP kernel is subprocess-free; Windows/macOS PHP runtime dependency restrictions are documented in its README. Missing confinement cannot trigger ambient execution. |
| Discovery | `core/target_protocol/discovery.rs` probes describe, tracks declared/probed/verified evidence and digest-bound capabilities. It discovers adapters, not Mago executable capabilities. Named IDs require registry definitions. |
| Diagnostics | `core/diagnostics/provider.rs::{ProviderDiagnosticWire,normalize}`, `diagnostics/{registry,normalize,types}.rs`. Registered IDs, namespace/original_code, safe logical path and range already exist. Range is half-open, byte offsets 0-based, lines/columns 1-based. Raw provider messages/stacks are prohibited; 256 diagnostics/result. Provider wire has no fix payload. |
| Observed bindings | `core/observed/{scan_service,index,types,wire,bindings,view}.rs`; `scan_service::run`, `Evidence`, `ReferenceEvidence`, `Provenance`. Carries signatures/references into observed bindings; ambiguity and confidence must survive, signature changes invalidate bindings. Not a full native PHP symbol graph. |
| Graph/effects | `core/graph/{mod,build}.rs` exposes `GraphRegistry` extension kinds/relations; `core/effects/mod.rs::{attach_detected,EffectGraph::compare}` attaches validated `EvidenceEnvelope` without changing declared Model. A PHP method call is not automatically a Model effect. |
| Profiles | `core/target_profile/{component,resolution,portability}.rs`: Mago advertises lint full, types partial at catalogue level. Catalogue declarations do not establish executable availability or Laravel inference quality. |
| Lock | `core/lockfile/{types,parse,canonical,resolution,verify}.rs`, `contracts/lock.schema.v0.3.2.json`: exact versions/digests and capability snapshots. `ProviderKind` is only Adapter/Generator; there is no arbitrary external-tool lock entry. Do not mislabel Mago as an adapter. |
| Native gates | `core/native_gate/`, `docs/native-gates.md`; PHP `plan_native_response` returns unsupported. Do not disguise Mago as an existing Node package-manager recipe. |
| Existing gates | `adapters/php-laravel/tests/{protocol,process}.php`; `scripts/test-php-laravel-adapter.mjs`; `crates/lekalo-core/tests/{php_laravel_kernel,adapter_conformance,target_protocol_security,target_protocol_boundaries,target_discovery,diagnostics,observed,graph,effects,lockfile,target_profiles}.rs`. Strict adapter battery has nine operations including plan-native, but does not prove Mago semantics. |
| Analogues | `adapters/node-typescript/src/{scanner,kernel,transport-extension}.mjs` and scanner/extension tests: deterministic identities, bounded reads, extension evidence, refusal of lossy wire projections. Reuse contracts and test patterns, not the TypeScript implementation. |

## Acceptance gap matrix

| Issue acceptance criterion | Current gap | Required proof / implementation step |
| --- | --- | --- |
| Mago version/capabilities discovered safely | No Mago runner, pin or probe; adapter discovery alone is insufficient. | S1/S2: explicit executable, hash/version/help probes under bounded isolation; missing, unsupported, denied, failed states distinct; no install or shell. |
| Diagnostics normalized with original code/source range | Normalizer exists; PHP emits no analyzer evidence and Finding is too narrow. | S2/S3: JSON and SARIF recordings produce identical canonical diagnostics, exact original_code and verified ranges; malicious/stale reports refused. |
| Laravel relations/routes/container links augment graph with provenance | No PHP semantic scan or Laravel extension; current wire lacks per-edge provenance. | S4: Eloquent relation, route-handler and DI binding fixtures with origin, confidence, source, input digest; ambiguous/dynamic links remain unknown. |
| Lekalo strict profile rules covered by fixtures | `strict_types` in adapter source is not target-project enforcement. | S5: each rule below gets compliant, violating and uncertain cases; diagnostics and graph/effect expectations asserted end to end. |
| Safe fixes not applied without explicit command | No fixes run today, but no Mago preview or metadata seam exists. | S6: immutable analysis + preview tests; even safe fixes stay advice. Actual apply remains unavailable unless explicit command and publication authority are implemented. |
| Upgrade compatibility captured in lock/evidence | Adapter integrity exists, external tool identity/schema compatibility absent. | S1/S7: reviewed toolchain lock and evidence receipts; two-version replay/upgrade diff, digest mismatch refusal, no automatic latest selection. |
| PHP adapter testable with fake analyzer | Protocol/process tests do not substitute Mago output. | S2/S7: injected Analyzer interface, recorded fake process outputs and integration through production TargetClient, no installed Mago needed for fake gate. |

## External evidence and limits

Context7 `/carthage-software/mago` was resolved and queried; official upstream reporting documentation was also read through GitHub API. These are moving documentation references, not a tested binary pin.

- [CST inspection](https://github.com/carthage-software/mago/blob/main/docs/content/en/guide/inspecting-the-cst.md) documents machine-readable syntax and name resolution. Retrieved snippets mix `cst` and `ast`; S1 must determine the actual command for the selected release. Do not hard-code an alias from a mixed-version documentation index.
- [Reporting options](https://github.com/carthage-software/mago/blob/main/docs/content/en/fundamentals/shared-reporting-options.md) lists JSON/SARIF, report destination and threshold controls; select format explicitly and prohibit mutating baseline operations. Nonzero exit alone cannot distinguish findings from infrastructure failure.
- [Linter usage](https://github.com/carthage-software/mago/blob/main/docs/content/en/tools/linter/usage.md) documents `mago lint --fix --dry-run` preview; [linter reference](https://github.com/carthage-software/mago/blob/main/docs/content/en/tools/linter/command-reference.md) documents `--list-rules --json`.
- [Guard reference](https://github.com/carthage-software/mago/blob/main/docs/content/en/tools/guard/command-reference.md) is the starting point for architectural rules. Guard reporting flags/schema/exit behavior must be captured independently from analyzer/linter.
- Public resolved reference graph export, complete signature compatibility output, incremental API, and stable JSON schema guarantees were **not established**. CST/name traversal may project explicit references; unresolved dispatch must not be represented as a complete native graph. No fallback regex parser or internal-crate dependency.

## Implementation sequence (one reviewable commit per step)

### S1 — Pin the public CLI and freeze capability recordings

1. Add `adapters/php-laravel/mago-toolchain.lock.json` (new closed adapter-owned lock), `mago-compatibility.json`, and `tests/fixtures/mago/toolchain/`. Record exact Mago version, platform artifact SHA-256, supported PHP syntax versions, configuration/stubs digest, decoder revision, capability snapshot and recording hashes. No host paths or credentials.
2. Provision the binary outside adapter execution. Probe the explicit resolved executable with `--version`, `--help`, and subcommand help in an empty read-only project, with cleared nonessential environment, bounded output/deadline and no network. Verify artifact hash before launch; never use project PATH or a Composer script.
3. Capture help and minimal positive/negative machine outputs for syntax/names, analyze, lint, guard, JSON, SARIF, lint rule listing and fix preview. Probe one capability at a time; syntax/name success is not reference-graph completeness. Bind findings exit codes to valid decoded outputs; classify other exits separately.
4. Candidate commands to qualify: `mago --version`; `mago lint --list-rules --json`; `mago analyze --reporting-format json`; `mago lint --reporting-format json`; `mago guard --reporting-format json`. Repeat reporting in SARIF. Determine `ast` versus `cst`, `--json` and `--names` combinations from the pinned help; record exact argv rather than guessing.
5. Select a release only after these recordings pass. Unknown or changed output schema means unsupported compatibility, not an empty successful report. Do not claim current upstream HEAD is an exact supported release.

### S2 — Evidence custody and process boundary before semantic integration

1. Recommended architecture: a core-owned bounded external-tool runner produces an evidence receipt; the existing PHP adapter remains a subprocess-free evidence consumer. Add `core/provider_evidence/{mod,wire,runner,ingest}.rs` and register the module in `core/lib.rs`; add a closed versioned `contracts/provider-evidence.schema.v0.1.0.json` and contract/version/privacy ownership entries using the repository publication process. Names are proposed.
2. Receipt fields: tool/version/artifact digest, compatibility decoder version, exact input manifest, config/profile/IR/binding digests, operation, completion status, bounded diagnostics, syntax/symbol/reference payload references and their hashes. Core owns `.lekalo/import/mago/`; adapter gets only an explicitly staged read-only receipt. Hashes alone do not prove an untrusted report came from a real execution: record producer custody and distinguish imported/unverified from runner-verified receipts.
3. Add a proposed `lekalo analyze --target php-laravel --report json` dispatch in `crates/lekalo-cli/src/main.rs`: verify lock, run isolated tools, ingest and normalize, then expose receipt to scan/validate/verify. Separate analysis from scenario verification (#56); Mago success must not satisfy `verify.scenarios`. Add CLI tests for command status, missing tool and stale receipt.
4. Runner stages only declared source/config/stub inputs plus verified executable/runtime dependencies, reuses transport deadline/output/tree termination principles, and disallows writes except owned temporary cache. Because existing target confinement cannot simply launch Mago from PHP, add an explicit external-tool execution profile to confinement with pinned runtime assets; no broad PATH/vendor grant. Start with qualified Linux isolation; Windows/macOS must report unavailable until behavioral tests qualify them.
5. Add `adapters/php-laravel/src/{analyzer,mago-evidence}.php` with new `Analyzer::capabilities()/analyze()`, `MagoEvidenceAnalyzer` and `FakeAnalyzer` test implementation. Inject at dispatch composition; no production request field may select a fake or an executable. Fake outputs traverse the same strict decoder as real receipts.
6. Extend `kernel.php::{dispatch,scan_response,describe_capabilities}` and manifest read scopes for the receipt; absent/stale/failed evidence must refuse semantic claims, replacing unconditional empty validate/verify success for analysis-enabled profiles. Use evidence capability/version checks independently of protocol negotiation.
7. Keep target v0.3.2 Finding as a bounded summary only; full provider diagnostics are normalized by the core ingest consumer and referenced by receipt identity. Do not hide JSON in `Finding.detail`. If a receipt reference must cross the wire, publish a negotiated successor schema with frozen 0.3.2 rejection tests, not undeclared extra members.

### S3 — Diagnostics, spans and format decoders

1. Add `adapters/php-laravel/src/mago-diagnostics.php` (`decode_mago_json`, `decode_mago_sarif`, `map_mago_diagnostic`) plus `core/provider_evidence/diagnostics.rs` (new). Pin decoders to recorded versions; unknown variants fail closed. Normalize SARIF rule IDs, severity and primary physical location; retain bounded related-location evidence separately until supported by provider wire.
2. Register proposed rule families for native analysis/lint/guard findings, strict-profile violations and tool failures in the next diagnostic-registry publication; update `core/diagnostics/registry.rs` embedding, fixtures and version custody together. Do not misuse `adapter.diagnostic-invalid` for valid Mago findings: it means malformed provider input.
3. Preserve exact original Mago code in namespaced metadata (e.g. namespace `software.carthage.mago:<pin>`); retain registry-owned public messages. Unknown valid upstream codes map to a registered generic native-finding rule with original_code intact. Lekalo-only rules use a Lekalo namespace, not fabricated Mago codes.
4. Convert offsets against the exact input bytes: UTF-8, CRLF/LF, BOM, EOF, multi-line and multibyte tests; validate ordered ranges inside file bounds. SARIF URI decoding must reject absolute/escaping paths and external locations; never fetch URIs. Respect SARIF column encoding, reconstruct missing byte offsets from captured source, and refuse ambiguous coordinates rather than inventing precision.
5. Reject duplicate JSON keys, unknown closed fields, null optionals, unbounded nesting, huge reports, invalid UTF-8 and >256 public diagnostics. Report overflow explicitly without arrival-order truncation. Suppressed/baselined findings must not establish strict-profile completeness; strict mode analyzes without suppressing required rules.

### S4 — Native symbols, Laravel graph and detected effects

1. Add `src/{mago-scan,mago-signature,laravel-evidence}.php`. Traverse **Mago-produced syntax/names**, not PHP text, to emit classes/interfaces/traits/enums/functions/methods/properties, declaration spans, resolved identities, inheritance/trait links and explicit reads/calls/writes. Identity uses package + qualified name + symbol kind/member slot; line movement alone must not rename a symbol.
2. Signature digest covers ordered parameters, return/property types, nullable/union/intersection/shape types, by-reference/variadic flags, visibility/static and relevant final/readonly modifiers. Compatibility comes from pinned analyzer results and explicit normalized type evidence; equality of signature hashes is not a subtype proof. Unknown dynamic types remain unknown.
3. Add a versioned native graph in the provider receipt and `core/provider_evidence/graph.rs` projection. Every edge carries producer (Mago or Laravel extension), source span, confidence, derivation and source/config digests. Reject unresolved endpoints, collisions and contradictory exact evidence; deterministic sort/dedupe. More than eight references must not silently truncate into ScanEntryEvidence; retain full graph in receipt and refuse an incomplete legacy projection.
4. Integrate accepted symbol mappings through `core/observed/scan_service.rs`/`index.rs`, retaining ambiguity/freshness rules. Add `core/graph/native.rs` (new bounded evidence overlay) and a native-graph query projection; `graph::build_with_registry` alone only builds IR edges and does not ingest external graph rows. Keep Model graph and native overlay distinct but linked by reviewed bindings.
5. Laravel extension statically recognizes explicit Eloquent relation declarations (including polymorphic/ambiguous cases), route-to-controller methods and container interface-to-implementation bindings from Mago syntax. No `artisan route:list`, PHP include/eval, service-provider boot or database access. Dynamic routes, macros, contextual bindings and facades receive qualified uncertain evidence; conflicting candidates are not silently chosen.
6. Map calls/writes into `effects::provenance::EvidenceEnvelope` only through known semantic bindings and classified resources; use `effects::attach_detected` then `EffectGraph::compare`. Validate source freshness in ingest because the effects envelope itself validates shape, not execution truth. Unknown external calls are uncertainty, not invented database writes or evidence of purity.
7. Add `tests/fixtures/mago/{symbols,signatures,references,laravel,effects}/`: aliases/imports, namespace collisions, traits/enums, generics/PHPDoc, nullable shapes, inheritance changes, indirect calls, relation targets, route groups and container ambiguity. Golden assertions include provenance and negative absence of invented exact edges.

### S5 — Strict profile and architectural rules

Add `adapters/php-laravel/profiles/strict/mago.toml`, `strict-rules.json` (new rule-to-provider mapping) and `src/strict-profile.php`. Register profile/capability definitions only after fixture proof. Each row gets pass/fail/unknown fixtures in `tests/fixtures/mago/strict/<rule>/`, with expected original code, normalized range and reason; pin actual Mago rule names from S1 rather than assuming them.

| Required rule | Implementation and distinguishing fixture |
| --- | --- |
| `declare(strict_types=1)` | Mago lint rule if available, otherwise syntax-evidence predicate; absent/0 vs 1, comments and namespace placement. |
| final/readonly profiles | Explicit opt-in profile predicates; immutable DTO vs intentionally extensible framework model. No blanket finalization of Laravel models/proxies. |
| no dynamic properties/variable variables | Mago analysis/lint plus syntax predicates; declared property vs unknown dynamic receiver and `$$name`. |
| no service locator/facades in portable core | Resolve aliases/FQNs then Guard/profile layer boundaries; reject core `app()`/facade use, permit declared Laravel boundary implementation. |
| no magic `__get/__set` domain state | Domain ownership + method evidence; forbidden domain magic vs framework-owned Eloquent behavior outside portable core. |
| explicit nullable/array shapes/types | Signature evidence and pinned analyzer diagnostics; missing/mixed shape and implicit nullable vs declared `?T`/shapes; unsupported shape inference is unknown. |
| declared effect vs detected calls/writes | S4 effects comparison; declared write vs actual extra write, read-only method, unclassified call and stale binding. |
| target boundary/dependency rules | Guard config generated from resolved profile/dependencies; direct and aliased forbidden reference, allowed adapter bridge, indirect unresolved dependency. |
| generated file drift | Compare deterministic owned artifact/manifest digest and before-state, not Mago lint; edited/deleted generated file vs untouched output. Never overwrite drift during verify. |

If an upstream rule is absent, use a predicate over exported Mago evidence; unavailable prerequisite evidence makes the rule unsupported, never pass. Guard should execute native dependency constraints, while Lekalo supplies semantic ownership/effect rules.

### S6 — Safe fix advice and preview only

1. Add `src/mago-fixes.php` and bounded fix records in provider evidence: original rule, applicability, affected logical files, exact before hashes, edit ranges/replacements and proposed diff digest. Validate non-overlap, file boundaries and ownership. Raw source patches stay local/private; normal public diagnostics expose only registered fix metadata.
2. Standard scan/analyze/validate/verify never pass `--fix`, `--staged`, baseline-write or formatting flags. Explicit proposed CLI preview `lekalo analyze --target php-laravel --fix-preview` may invoke the qualified `mago lint --fix --dry-run` only in a read-only disposable snapshot. Dry-run text is not authoritative structured edits; if no edit export exists, label it a preview only.
3. Snapshot hashes and no-write sentinel tests cover success, findings, crash, timeout and malicious fake analyzer. Map safe/potentially-unsafe/unsafe conservatively; safe means eligible advice, not permission. #55 may ship without fix apply: reject apply requests explicitly. A later apply command (#73/#96) must consume fresh plan authority and core publication checks, never forward arbitrary provider patches into the real project.

### S7 — Lock binding, reproducibility and acceptance gates

1. Bind the adapter-owned toolchain lock digest into provider receipts and adapter package integrity; kernel refuses mismatched tool/config/profile/input digests. Include lock bytes in deterministic packaging inputs so adapter digest changes when the supported toolchain changes. Existing `lekalo.lock` binds that adapter digest; no unsupported Tool provider kind is introduced.
2. Extend `build.php` to bundle new PHP modules in fixed order and embed toolchain compatibility identity; regenerate `adapter.php` and manifest through existing scripts. Update `README.md` with supported pins/platforms/limitations; add `scripts/test-mago-integration.mjs` with separate required fake and real modes.
3. Start with a full deterministic analysis of the bounded input snapshot. Adapter evidence cache keys include tool/decoder, source inventory, config/stubs, Composer lock, profile, bindings and IR digests. Changed/deleted dependency or tool bytes invalidates the cache. Incremental analysis is optional only after pinned capability and cold/warm equivalence tests; no timestamp-only cache.
4. Upgrade procedure: change the exact lock in a reviewed commit, replay old/new recordings and real corpus, review diagnostics/rule/fix/graph diffs, update compatibility decoder and evidence hashes, regenerate adapter/manifest, then run full gates. New unreviewed versions fail compatibility; upgrades never refresh a lock automatically during scan.
5. CI in `.github/workflows/ci.yml`: fake suite requires PHP but no Mago/network; real Linux job installs a checksum-verified pin during setup, then denies network for execution. Require real bad-code and valid-code runs with artifact capture. Platform skip must be visible and must not count as Mago acceptance.

## Test commands and completion evidence

Existing regression commands (run after implementation; all paths already exist):

```sh
php -n adapters/php-laravel/tests/protocol.php
php -n adapters/php-laravel/tests/process.php
php -n adapters/php-laravel/build.php --check
node scripts/test-php-laravel-adapter.mjs
node scripts/test-adapter-manifest-golden.mjs
node scripts/test-target-protocol-contracts.mjs
node scripts/test-diagnostic-contracts.mjs
cargo test --locked -p lekalo-core --test php_laravel_kernel --test adapter_conformance --test target_protocol_security --test target_protocol_boundaries --test target_discovery --test diagnostics --test observed --test graph --test effects --test lockfile --test target_profiles
```

New gates to implement and require:

```sh
node scripts/test-mago-integration.mjs --fake
node scripts/test-mago-integration.mjs --real --require-available
cargo test --locked -p lekalo-core --test provider_evidence --test mago_integration
cargo test --locked -p lekalo-cli --test mago_analysis
```

- Fake matrix: missing/version mismatch, denied spawn, timeout/cancel, crash, valid findings nonzero, malformed/oversized/duplicate-key output, path escape/symlink, UTF-8/SARIF ranges, stale receipts, unsupported capability and malicious fix attempts. Assert status and no source/lock/index mutations on every failure.
- Real corpus: all declaration kinds + nine strict-rule groups + Laravel graph + effect mismatch; compare JSON/SARIF canonical results, two identical runs, cold/warm cache, modified inputs and reviewed tool upgrade. Record exact executable digest, stdout schema version and terminal exit evidence.
- Production proof: new CLI runner → validated receipt → PHP adapter via TargetClient → observed/native graph and effects comparison. Unit decoding, direct PHP launch, or adapter conformance alone is insufficient.
- Research validation: only this Markdown is changed; `git diff --check`, path/symbol review, line count <=300 and commits on the task branch. No Mago executable was found locally; no runtime acceptance or Mago compatibility pin is claimed by this research.

## Risks and recommended resolutions

| Risk / ambiguity | Resolution |
| --- | --- |
| Public CST/names output incomplete or unstable | S1 is an admission gate. Pin recordings and decoder; implement supported syntax projections, advertise partial references/types, and retain unresolved evidence. If required data is unavailable, document blocked capability/upstream request rather than coupling to internal crates. |
| Laravel filename case vs lowercase wire path grammar | Normal Laravel `app/Models/User.php` cannot be lowercased safely. Before real-source ingest, define a versioned native-path domain preserving case, with traversal/device/case-collision checks and a source-map reference into canonical logical diagnostic paths. Extend provider source mapping and confinement deliberately; never globally relax Model paths or silently rename files. Include both case-sensitive Linux and Windows collision tests. |
| Full graph/provenance cannot fit existing scan wire | Receipt/native overlay owns full evidence; legacy scan projection is bounded and must refuse lossy claims. New graph query must expose provenance; a JSON artifact with no consumer is not AC3 completion. |
| Config/plugins/stubs can widen authority | Resolve controlled config and explicit dependency/stub inventory before staging; reject outside-root paths and links. Do not execute project PHP to infer framework links. Bound vendor traversal and report incomplete coverage. |
| Existing sandbox/manifest forbids Mago launch | Core-owned runner and explicit tool runtime profile, then subprocess-free PHP consumer. No manifest permission toggle alone, unconfined fallback, or hidden Composer recipe. |
| Diagnostic metadata and safe-fix privacy | Preserve original codes/ranges through typed ingest; keep raw reports and source diffs local, not in public message/data strings. Core-produced fix metadata needs a typed ingest seam, not provider-controlled diagnostic objects. |
| Lock acceptance ambiguous | Adapter-local exact lock + artifact-integrity binding + execution receipt satisfies tool upgrade custody without altering generic lock provider kinds. If a first-class tool node in `lekalo.lock` is required, publish a separate versioned lock extension before claiming it exists. |
| Parallel #54/#56 work | Share receipt/Analyzer boundaries; #55 owns Mago runner/decoders/rules/graph evidence, #54 adapter lifecycle, #56 scenario execution. Keep PHP type generation and Laratesto acceptance separate. |
