# Issue #84: measurable architecture profile implementation

**Implemented core milestone, 2026-10-04.** Authority: [issue #84](https://github.com/ichinya/lekalo/issues/84), the committed [research](issue-84-research.md), and the subsequent implementation authorization. Branch: `ichinya/m7-issue-84`; research/base commit: `60b54b51166240afb17668ba1ca3341a8f4ed231`. Product advances from `0.6.4` to `0.6.5` so every new contract uses the implementing product version. This is local implementation evidence, not a release, hosted CI result, native runtime acceptance, complete fifteen-rule certification or A/B outcome.

## Delivered surfaces and custody

The new `architecture-profile` command has `catalog`, `resolve`, `lock`, `assess` and `diff` subcommands (`crates/lekalo-cli/src/main.rs:275`, `:2450`; `crates/lekalo-cli/src/architecture_profile.rs:52`, `:106`). Every command is read-only. Explicit caller persistence is needed for lock/report/baseline documents; assessment does not generate, execute, promote observed facts, rewrite handlers or update a baseline. Existing `--profiles`, `--policy`, semantic validation, producer profiles and Model/IR semantics retain their owners.

| Contract family, version 0.6.5 | Published schema | Synthetic positive/live golden |
| --- | --- | --- |
| architecture-rule-catalog | `contracts/architecture-rule-catalog.schema.v0.6.5.json` | `tests/fixtures/architecture-profile/golden/catalog.json`; catalog instance also in `contracts/architecture-rule-catalog.v0.6.5.json` |
| architecture-profile | `contracts/architecture-profile.schema.v0.6.5.json` | `tests/fixtures/architecture-profile/golden/profiles.json`; four-profile collection also in `contracts/architecture-profile.v0.6.5.json` |
| architecture-profile-resolved | `contracts/architecture-profile-resolved.schema.v0.6.5.json` | `tests/fixtures/architecture-profile/golden/resolved.json` |
| architecture-profile-lock | `contracts/architecture-profile-lock.schema.v0.6.5.json` | `tests/fixtures/architecture-profile/golden/lock.json` |
| architecture-profile-report | `contracts/architecture-profile-report.schema.v0.6.5.json` | `tests/fixtures/architecture-profile/golden/report.json`, `violation.json`, `adopted.json` |
| architecture-profile-diff | `contracts/architecture-profile-diff.schema.v0.6.5.json` | `tests/fixtures/architecture-profile/golden/diff.json` |
| architecture-adoption | `contracts/architecture-adoption.schema.v0.6.5.json` | `tests/fixtures/architecture-profile/golden/adoption.json` |

Closed DTOs and explicit known/unknown/unsupported/withheld states are in `crates/lekalo-core/src/architecture_profile/wire.rs:4`, `:10`, `:57`, `:76`, `:88`, `:92`, `:109`. Admission checks headers, exact catalog/parent/assignment pins, complete root inventory, sorted unique selections, single-parent chains bounded to eight levels, scope grammar, default-error protection and producer-specific limit eligibility (`crates/lekalo-core/src/architecture_profile/mod.rs:105`, `:153`, `:166`, `:176`). Typed core callers undergo document admission as well as CLI callers. Strict JSON parsing rejects duplicate keys; file/DTO admission is bounded to 8 MiB.

`contracts/diagnostic-registry.v0.6.5.json` and its schema are an additive union successor: **508 entries = all 500 unchanged predecessor entries + LEK-APR-001 through LEK-APR-008**. Diagnostic wire remains `v0.2.16`. The original embedded registry/version remain `0.6.4`; `successor()` and `for_version()` admit the closed successor independently (`crates/lekalo-core/src/diagnostics/version.rs:54`; `registry.rs:22`, `:151`, `:162`). Existing IDs still emit registry `0.6.4`; architecture service IDs emit `0.6.5` (`normalize.rs:53`, `:182`). The live gate and Rust test compare every predecessor entry, not merely counts. No predecessor file under `contracts/` changed.

The generator projects eleven deterministic artifacts, including the registry successor, seven schemas, catalog and profile instances (`scripts/gen-architecture-profile-contracts.mjs:19`, `:45`, `:59`, `:68`, `:101`). Authoring is explicit `--write`; acceptance uses read-only `--check`. Core report measurements remain owned by their existing producers; the architecture recipe governs admission, selection and projection rather than redefining those metrics.

The live gate is `scripts/test-architecture-profile-contracts.mjs`. It requires the built binary, exact Ajv **8.17.1**, all schemas and goldens, synthetic provenance, registry union preservation, generator parity and independent semantic/count expectations. `.github/workflows/ci.yml:295` places it in `build-test` after the existing `cargo build --workspace --locked` at line 204. Existing gates remain present. Temporary copies prove failure when binary, schema, catalog or provenance is missing. The fixture family is declared synthetic in `tests/fixtures/fixture-provenance.json`; its README explains the synthetic planner source and evidence boundary.

`scripts/lib/docs-maintenance.mjs:47`, `:61` register the command owner and all seven new families. Running `scripts/update-docs-owners.mjs --write` regenerated `docs/documentation-owners.json` at derived product `0.6.5` and the `docs/cli.md:1144` command index from live recursive help. The public usage/boundary reference is [architecture-profile](../architecture-profile.md).

## Profiles, scoping, limits and adoption

The resolved chain is `legacy-observed` -> `contracted-standard` -> `ai-strict` -> `managed-generated`. Legacy disables optional architecture signals; standard collects the available signals; strict requires complete declared identity/import/context evidence; managed additionally requires repeated-generation evidence and therefore reports an honest core evidence gap. `enforced-core` means only the selected core obligations passed. All three production measured/semantic rules have unknown optional limits by default; no synthetic gate maximum becomes a product default.

Profile severity is explicit and bounded by catalog severity; mandatory default-error identities cannot be disabled, reduced or adopted. Every inherited enablement, severity, required-evidence or limit weakening is refused. V1 deliberately refuses weakening instead of introducing an acknowledgement escape. Separate root policy changes are explicit documents and visible in diff. Project/module assignments use exact semantic IDs, with no glob/path/recursive grammar; duplicate, unknown, foreign and conflicting assignments fail closed. A module selection cannot weaken the selected project profile. Ordinary whole-project semantic errors remain mandatory outside the selected architecture module.

Fan-out/token limits require a named `calibrationRef`; it is a caller review identity, not proven empirical calibration. Equality passes; complete measured one-over evidence creates a violation. Partial/unsupported facts do not become clean evidence or numeric style violations. An explicitly required missing signal has `evidence-gap` disposition. Assessment is advisory until `--check`; then violations or required gaps deny with exit 3 while retaining the full report and rationale/alternative. A baseline alone grants no exemption.

Adoption pins the entire immutable baseline and the current architecture snapshot. Baseline admission checks recipe, scope, policy, inventory, profile/selection/coverage consistency, witness order, condition derivation and disposition arithmetic (`crates/lekalo-core/src/architecture_profile/mod.rs:814`). Debt needs an exact complete measured prior violation, condition digest, owner, reason, review reference, calendar-valid expiry and explicit `--as-of`. Active unchanged debt becomes `adopted`; raw values, severity and witnesses stay intact. New/changed violations and expired debt still deny. Core semantic errors cannot be debt. Source hashes identify content, not historical authenticity; baseline review remains caller custody. General trend budgets, native observed-evidence joins and detailed orphan/expiry audit rows are future work.

The architecture lock is a dedicated sidecar; `assess --architecture-lock FILE` verifies it against the admitted current document (`mod.rs:254`, `:272`). Profile diff reports severity, enablement, evidence, limit, membership, assignment and document changes (`mod.rs:279`). This implements explicit architecture persistence/verification and policy diff without altering predecessor `lekalo.lock` or semantic-diff envelopes. Joining the sidecar into those existing envelopes requires a separately reviewed successor.

## Fifteen-rule evidence inventory

All rows are published with concrete rationale/alternative and explicit owner in the catalog. “Complete” below qualifies only the stated declared measurement, never the entire native architectural intent.

| Issue rule | Current producer/measurement | Core boundary |
| --- | --- | --- |
| R1 explicit IO/error/effect contracts | `contracted/error-contract`, operationContractCoverage | Unsupported aggregate; no invented command output/error member or current native conformance claim. |
| R2 stable semantic IDs | `compiled-ir`, declaredSemanticIds | Complete declared inventory; compilation/semantic errors remain mandatory. Lifetime native stability is unproven. |
| R3 strict types | adapter, nativeTypeCoverage | Unsupported native inventory; portable declared types do not prove native checker coverage. |
| R4 immutability by default | adapter, mutationCoverage | Unsupported; advisory basis, no style threshold. |
| R5 explicit dependencies | `compiled-ir`, fanOutModules | Complete declared module-import count/witnesses; native dependency injection completeness is unsupported. |
| R6 bounded module/context closure | context-budget 0.6.3, minimumRequiredSemanticTokens | Complete declared closure estimate when the producer is complete; otherwise partial/unknown. Reuses the producer's published budget bound. |
| R7 single entrypoint | adapter, entrypointCoverage | Unsupported native entrypoint/dispatch inventory. |
| R8 no hidden observers/globals | adapter/ai-lint, observerCoverage | Unsupported aggregate native coverage; no hidden-effect clean claim. |
| R9 no service locator/magic | adapter/ai-lint, locatorCoverage | Unsupported target classification; no core PHP recognizer. |
| R10 deterministic generation | artifacts, repeatedGenerationEquality | Unsupported repeated-output receipt join; managed check identifies the gap. |
| R11 explicit target bindings | bindings/adapter, nativeBindingCoverage | Unsupported exact current native binding aggregate; target name alone is insufficient. |
| R12 scenario-linked behavior | compiled-ir/trace, publicOperationsWithoutScenario | Partial declaration-level missing-cover count; no branch, error or passing native gate proof. |
| R13 errors as values | error-contract/adapter, nativeErrorValueCoverage | Unsupported native error escape/value classification. |
| R14 no premature abstraction | coupling 0.6.4, sharedAbstractionRadius | Partial structural radius; advisory only, no “premature” verdict or blocking limit. |
| R15 tracked duplication | coupling/adapter, replicaTrackingCoverage | Unsupported full clone/tracking inventory; no claim that declared obligations cover every duplicate. |

For the synthetic project, the committed report has 30 rows (15 each for `notify` and `planner`). Complete token maxima are **143 / 384**, fan-out **0 / 1**, declared symbol counts **6 / 21**. Partial abstraction radius is **4 / 10** and missing public scenario declarations **2 / 5**. `scripts/test-architecture-profile-contracts.mjs:80` compares planner's 384 with fresh existing `context-budget --module planner --budget 1000000` output; `:101` checks identical Model/IR pins under changed architecture policy. These are structural facts and estimates, not a weighted readability score or agent outcomes.

## Acceptance mapping

| Acceptance criterion | Concrete evidence | Status / boundary |
| --- | --- | --- |
| AC1 profile schema and rule catalog published | Seven schema/golden pairs above; four-profile instance; 15 rule rows; generator --check; strict core/CLI admission; read-only live gate. | **Implemented core.** Adapter-owned catalogs and target rule admission need a later versioned surface. |
| AC2 incremental adoption for legacy/contracted projects | `legacy-observed` default, standard/strict inheritance, exact module/project opt-in, foreign/conflicting scope and weakening refusals, exact measured baseline/adoption/expiry and increased-debt tests (`scripts/test-architecture-profile-contracts.mjs:109`, `:116`, `:135`, `:141`). | **Implemented for declared core evidence.** Existing observed/contracted modes retain authority; the profile does not qualify/promote inferred native facts. |
| AC3 Laravel strict maps to Mago without core PHP coupling | Core catalog owner is language-neutral and the gate rejects PHP/Laravel/Mago-specific names in it. Existing adapter surface remains `adapters/php-laravel/src/strict-profile.php:23`, `:67`, `:105`, `:130`, `:165`; kernel selection at `kernel.php:3392`; pinned Mago lock at `mago-toolchain.lock.json:9`. | **Adapter boundary, not delivered.** No architecture-to-adapter catalog/evidence join or actual Mago run was implemented or claimed. Existing adapter rule maps alone do not complete AC3. |
| AC4 every violation has rationale and explicit alternative | Catalog requires both strings; every report row carries producer, measure, coverage, witnesses, severity, rationale and alternative (`wire.rs:57`, `:92`). `golden/violation.json` contains the witnessed fan-out violation; adopted output preserves it; `scripts/test-architecture-profile-contracts.mjs:103` asserts warning severity, explanation and `notify` witness. | **Implemented core.** Native source-span explanations wait for qualified adapter evidence. |
| AC5 profile changes visible in lock/diff/evidence | Dedicated lock/resolved/report pins; exact lock verification rejects tampering; typed policy-change rows include limit strength and provenance; unchanged Model/IR pins under changed policy are asserted; `golden/lock.json`, `resolved.json`, `diff.json`. | **Implemented as explicit architecture sidecars.** Integration into predecessor lock/semantic-diff envelopes is intentionally deferred to successors. |
| AC6 no subjective style rule blocks without semantic/measurable basis | Mandatory semantic validation; only complete declared fan-out/context measurements accept calibrated maxima; equality/one-over, partial/unsupported, absent calibration, all weakening dimensions and subjective abstraction-limit refusal probes (`scripts/test-architecture-profile-contracts.mjs:154`, `:170`, `:175`). Defaults have no numeric limits. | **Implemented core governance.** Required unavailable evidence denies as a named gap, never as proof of style violation. Calibration and empirical safety are not authenticated outcomes. |
| AC7 AI-agent task A/B improvements | Context/fan-out/radius/scenario values and exact producer/Model/IR/policy pins are available for a future experiment. | **Not measured in this milestone.** No randomized task execution, timing, regression, comprehension, review-quality or causal improvement claim. Controlled A/B evaluation remains separate work. |

## Validation ledger

Node gates use an external installation of **Ajv 8.17.1** via `LEKALO_AJV_NODE_PATH` and `NODE_PATH`; no dependency installation or node_modules mutation occurred in the checkout. The new gate copies the synthetic fixture to an OS temporary directory and invokes the current `target/debug/lekalo.exe` with `--json --no-cache`. Acceptance runs never write committed fixtures.

```text
cargo build --workspace --locked -j 1
Finished dev profile, product 0.6.5

node scripts/gen-architecture-profile-contracts.mjs --check
architecture profile generator: 11 artifacts --check

node scripts/update-docs-owners.mjs --write
{"ok":true,"commands":160,"contracts":135,"protocols":56,"globals":4,"p0Owners":13}

node scripts/test-docs-ownership.mjs
{"ok":true,"gate":"docs-ownership","mode":"live-help","surfaces":355,"requiredDocs":13,"p0Owners":13,"pageOwnerControls":33}

node scripts/test-fixture-provenance.mjs
{"ok":true,"families":78,"synthetic":78,"evidenceBacked":0}
```

Final acceptance after the typed-API admission guard:

```text
cargo test --locked -p lekalo-core --lib architecture_profile::tests
5 passed; 0 failed; 1053 filtered out

cargo test --locked -p lekalo-core --lib -- --test-threads=1
1056 passed; 0 failed; 2 ignored; 0 filtered out

cargo clippy --locked --workspace --all-targets -j 1 -- -D warnings
Finished dev profile; exit 0

cargo fmt --all -- --check
exit 0

node scripts/test-architecture-profile-contracts.mjs
{"ok":true,"product":"0.6.5","ajv":"8.17.1","families":7,"goldens":9,"registryEntries":508,"diagnostics":8,"liveProbes":62,"artifactFaults":4,"mode":"read-only"}

node scripts/test-docs-examples.mjs --static
{"ok":true,"gate":"docs-examples","mode":"static","examples":12,"setup":2,"controls":7}

node scripts/check-contract-versions.mjs
{"ok":true,"product":"0.6.5","contractArtifacts":125,"base":"HEAD"}

node scripts/test-ai-lint-report-contracts.mjs
{"ok":true,"family":"ai-lint-report","schema":"0.6.4","live":true}
node scripts/test-ai-lint-evidence-contracts.mjs
{"ok":true,"family":"ai-lint-evidence","schema":"0.6.4","live":true}
node scripts/test-ai-lint-config-contracts.mjs
{"ok":true,"family":"ai-lint-config","schema":"0.6.4","live":true}
node scripts/test-ai-lint-waivers-contracts.mjs
{"ok":true,"family":"ai-lint-waivers","schema":"0.6.4","live":true}
node scripts/test-ai-lint-comparison-contracts.mjs
{"ok":true,"family":"ai-lint-comparison","schema":"0.6.4","live":true}
```

The core suite ran outside the restricted process sandbox so its existing Windows deadline child could be torn down. The five focused tests include typed-caller admission, exact registry union/predecessor emission, catalog/chain coverage boundaries, mandatory/error/inherited weakening, and duplicate-key/nested-member/cycle refusals. The live gate also checks increased measured debt against a comparable module baseline, expiry, immutable baseline/ledger bytes, forged reports, whole-project semantic errors outside the selected module, identical Model/IR across policy changes, independent producer arithmetic and deterministic repeated bytes.

Existing schema/live regressions passed: diagnostic (500 predecessor entries), validation profiles (registry 0.6.4, 23 owned rules), target profile (4 valid documents/8 refusals), coupling (all declared acceptance checks), context-budget (8 context rules, live binary), target protocol (19 goldens/30 invalid vectors), lockfile, semantic diff (16 documents/92 changes), errors (19 documents), observed (4 scan fixtures/11 rules), contracted (6 refusals), trace assessment (38 live synthetic cases), golden catalog and diagnostic coverage (500 predecessor rules). Authority, privacy and structure checks passed. Hosted platform/MSRV/workspace integration-suite execution is not claimed by this local ledger.

The product bump changes the existing lock's productVersion and therefore the AI-lint attachment and selected finding-condition hashes. Five **additive** `.product-0.6.5.json` snapshots in the existing AI-lint report/waiver/comparison synthetic families preserve every original golden and every predecessor schema/config/registry pin. `scripts/lib/ai-lint-contract-gate.mjs:17`, `:99`, `:284` admit the explicit product snapshot, validate the original golden against the unchanged schema, require the built product to match selection, and retain exact current golden equality plus all previous negative/live assertions. Evidence/config goldens remain byte-identical. All five existing gates are rechecked read-only after authoring.

Verification recovery: an initial parallel core run had a cache-path test failure which passed in isolation; a sandboxed serial run hung in the existing native-gate process-tree deadline test and was terminated with only this worktree's test tree targeted. Unsandboxed serial execution then passed 1,055 tests with two existing ignores. The initial parallel all-target Clippy run exhausted memory; `-j 1` passed. These environmental attempts are not treated as successful runs, and no existing test/gate/schema was weakened to suppress them.

Final custody check: predecessor `contracts/` paths and original AI-lint snapshots remain unchanged; only the authorized current worktree and disposable temporary fixtures were used. Local conventional feat commit only; no push, other-worktree edit or remote publication.
