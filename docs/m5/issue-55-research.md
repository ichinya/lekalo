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
| Lock | `core/lockfile/{types,parse,canonical,resolution,verify}.rs`, `contracts/lockfile.schema.v0.3.2.json`: exact versions/digests and capability snapshots. `ProviderKind` is only Adapter/Generator; there is no arbitrary external-tool lock entry. Do not mislabel Mago as an adapter. |
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
