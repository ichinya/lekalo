# Issue #61: Composer/Laravel native gates and targeted verification

Research and implementation plan only. No runtime, contract, fixture, or CI changes are made by this document.

## Authority and inspected baseline

- Acceptance authority: [issue #61](https://github.com/ichinya/lekalo/issues/61), read on 2026-09-28; depends on #16, #28, #54, #55, #56. The requested first command, `gh issue view 61 --repo ichinya/lekalo`, failed because the configured proxy refused connection. The GitHub connector then returned the live issue, including all seven acceptance criteria.
- Inspected HEAD: `a92754277fce1d44119a8f200240193c7dc954df`, branch `ichinya/m5-issue-61`. Initial working tree was clean. Paths below are repository-relative and were checked against this checkout.
- “Existing” describes source inspection, not a newly executed runtime test. “Proposed” identifies new files, symbols, fields, policies, or behavior. Existing research documents under `docs/m5/` are historical plans, not evidence of current implementation completeness.
- Main finding: #55/#56 supply useful analyzer, scenario, fixture, and evidence seams, but the PHP adapter still refuses native planning and production native execution remains deliberately disabled. #61 requires a host execution integration and versioned contracts, not just another Composer command template.

## 1. Native-gate architecture today and Composer port

**Existing Node planner.** `adapters/node-typescript/src/native-gate-extension.mjs::planNativeOperation` reads bounded inventory and verifies confirmations against actual manifest/script bytes via `verifyConfirmations`; `buildToolCatalog` constructs the tool inventory. `adapters/node-typescript/src/native-plan.mjs::{parseConfirmedScript,computeAffectedClosure,buildNativePlan}` implement literal argv confirmation, deepest package ownership, reverse dependent closure, reasons/exclusions, and deterministic plan hashing. Bare unattributed symbols become uncertainty. Package managers and shell scripts are refused by the Node script parser; arbitrary package scripts are not launched.

**Existing Rust boundary.** `crates/lekalo-core/src/native_gate/wire.rs::{MANAGERS,decode_plan,validate_plan}` accepts only `pnpm-workspace`, `npm-standalone`, `npm-workspaces`, `yarn`, `bun`; Composer is absent. Gates are only `build`, `typecheck`, `lint`, `test`. Shape, package references, argv limits/metacharacters, logical cwd, custody digests, and plan digest are checked. `policy.rs::{PolicyConfirmation,validate_policy}` binds package, script, manifest/script digests, tool recipe and rule. `receipt.rs::{validate_run_request,validate_run_result}` validates approval and terminal receipts. `view.rs::{build_view,validate_view}` is the observed-view seam, not execution authority.

**Execution is not shipped.** `crates/lekalo-core/src/native_gate/mod.rs::{production_run,decision_for}` returns `blocked/confinement-required` for private/untrusted plans and `unsupported/fixture-runner-not-shipped` for synthetic fixtures. `fixture_tests.rs::{run_fixture_plan,execute_command,run_bounded}` is compiled only under `cfg(test)`: trusted catalog, disposable copy, bounded child, mutation audit, original verification, cleanup. Its actual receipt says `test-only-bounded-spawn`, network/process containment `unavailable`; Windows kills only the leader. The module's confinement wording must not be mistaken for integration with the #89 sandbox. `docs/native-gates.md` documents this production restriction; newer #89 code does not itself remove it.

**Existing PHP boundary.** `adapters/php-laravel/src/kernel.php::plan_native_response` returns `native-planning-unsupported`; `dispatch` routes to it. `adapter.manifest.json` lists the operation but supplies no implemented native-planning capability. Its permissions allow bounded IR/import/test-port reads, limited generated writes, no network, no env grants, and no children. It cannot read arbitrary Composer manifests or launch project tools under those permissions.

**Proposed port, in order:**

1. Reserve a contract successor for native plan/policy/run/view and any changed target envelope; do not silently broaden published v0.3.2 enums. Update Rust validators/types, JSON schemas, diagnostic/authority/privacy registrations and shared goldens together. Existing contracts: `contracts/native-gate-plan.schema.v0.3.2.json`, `contracts/native-gate-policy.schema.v0.3.2.json`, `contracts/native-gate-run.schema.v0.3.2.json`, `contracts/native-gate-view.schema.v0.3.2.json`, `contracts/target-protocol.schema.v0.3.2.json`. The successor version is a release decision, not assigned here.

   Implementation note (issue #61): the successor was assigned `v0.4.0` per `docs/versioning.md` — a changed contract takes the product version of the changing commit, and the workspace version at implementation time is 0.4.0. The target-protocol envelope is deliberately unchanged (the selection document lives inside the plan, not the target wire), so only the four native-gate contracts moved.
2. Add proposed `adapters/php-laravel/src/native-plan.php::php_build_native_plan`: pure planning from verified Composer metadata, explicit module/test bindings, observed graph and confirmed recipes. Initially support one Composer root (`composer-project`, proposed manager); multiple roots/path repositories require explicit bounded inventories and edges, never filesystem guessing. Vendor dependencies are tool/runtime inputs, not automatically local modules.
3. Add proposed `adapters/php-laravel/src/native-policy.php::php_verify_confirmations`: exact command/cwd/script/transitive recipe checks. Join several confirmations per package by stable gate ID; do not copy Node's `confirmationByPackage` map, which currently retains only the last confirmation for each package.
4. Wire planning into the existing kernel, bundle via `adapters/php-laravel/build.php`, and regenerate `adapter.php` and manifest custody with `scripts/regen-php-adapter-manifest.mjs`. Grant only the metadata read scopes the planner needs; keep the compiler adapter process-free.
5. Add proposed host `crates/lekalo-core/src/native_gate/runner.rs::run_confirmed_plan`, with a distinct runtime staging boundary backed by the existing confinement primitives. Production enablement follows runtime qualification; merely adding Composer to `MANAGERS` must not enable execution.

## 2. Gate inventory: reusable parts and new work

- **Composer scripts:** reuse exact confirmation/digest/approval and receipt machinery. New: Composer script decoder, alias/reference DAG, transitive recipe custody, executable catalog and dispatch support. The planner fixture's `tests/fixtures/php-laravel/planner/composer.json` currently has no `scripts` section. Proposed fixture scripts must be checked in explicitly. Never rewrite a project's script into a different tool command. If a script contains shell interpolation, PHP callbacks, unbounded aliases or unsupported dispatch, report unsupported/blocked until that exact form has a qualified execution path.
- **Mago format/lint/analyze/guard:** `adapters/php-laravel/src/analyzer.php::{Analyzer,MagoEvidenceAnalyzer,mago_decode_receipt,mago_check_compatibility}` consumes bounded evidence without spawning. `mago-toolchain.lock.json` pins Mago 1.0.0 and lint/analyze/guard argv, output-format/exit probes and Windows/Linux digests; `scripts/test-mago-integration.mjs::{runFakeSuite,runRealSuite}` tests fake and real tool modes. Reuse custody/decoding, add a host runner producing evidence for the actual selected snapshot. Format-check is a new probe/recipe: the lock has no formatter-check command. Do not substitute fix-preview for formatting or run mutating fixes. Analyze/guard stay full-scope unless pinned tool behavior proves sound targeting; the lock marks incremental support unknown.
- **Laratesto/Testo:** reuse `adapters/php-laravel/src/scenario-map.php`, `scenario-emit.php::{php_emit_scenario_tests,php_native_test_id,php_module_of,php_reporter_text}`, generated/scaffolded/checked binding custody, and `tests/fixtures/php-laravel/planner/testo.php`. Existing E2E invokes PHP with `vendor/bin/testo run --config testo.php --suite=Laravel`. New: selection manifest, observed discovered/executed test reconciliation and native receipt linkage. Do not assume an unprobed Testo filter flag. A reviewed selection wrapper/config must preserve the project's runner and lifecycle.
- **Optional Pest/PHPUnit:** new project-specific confirmed recipes and result parsers. Neither is declared by the planner fixture's Composer manifest. Execute only if selected by policy; no automatic dependency installation. Record stable test ownership and suite identity, including suites transitively invoked by Composer scripts, to prevent duplicate execution.
- **Artisan checks:** existing `tests/fixtures/php-laravel/planner/artisan`, `bootstrap/app.php`, and `tests/fixtures/php-laravel/planner/tests/Support/PlannerConsoleKernel.php` provide the bootable fixture. New: explicit allowlisted command recipes, capability probes and boot-result classification. A command being registered with Artisan does not authorize it. Do not infer safe read-only behavior from its name.
- **Migration/schema validation:** reuse `scripts/test-php-laravel-migrations.mjs` and `contracts/laravel-migration-input.schema.v0.4.0.json`. Existing harness covers deterministic plans/artifacts and conditionally PostgreSQL 16 execution through Docker; absent Docker is a recorded skip. New native recipe must separate static validation from disposable DB execution, with a governed DB/runtime capability and no production credentials. A static/SQLite pass is not PostgreSQL acceptance.
- **Package discovery/boot smoke:** new confirmed gate using the fixture's boot path. Discovery/cache writes go to an isolated stage; providers may run arbitrary PHP. Confirm provider inventory and outputs. A successful interpreter `--version` is not application boot evidence.
- **Full release gate:** reuse policy identity and selection reasons. New: explicitly enumerate all mandatory project, cross-module, boot and migration gates, not just the affected closure. No full fallback on a failed command or missing tool. Full scope requires a checked release rule and exact approval.

Initially map formatter/lint/guard to `lint`, analyze to `typecheck`, scenarios/legacy to `test`, and boot/migration/project checks to confirmed `build` or `test` recipes. Proposed `gate_kind` metadata distinguishes these semantics in the successor rather than inventing values in the old four-value enum.

## 3. Direct process execution, allowlists, scopes and confinement

**Existing enforcement is layered.** `target_protocol/scopes.rs::{is_logical_path,is_scope,scope_covers,protected_home}` supplies portable lowercase logical paths and protected output homes. `crates/lekalo-core/src/adapter_package/budget.rs::SessionBudget` supplies the manifest permission ceiling, env/secret-name grants, network posture and child policy; it is not in `target_protocol` and is not a command/cwd allowlist. `target_protocol/confinement.rs::{SandboxPolicy::from_budget,Sandbox::new,Sandbox::run}` stages scoped inputs and selects platform enforcement; `confinement_windows.rs` owns Windows enforcement. `transport.rs::{run_private,run_impl,pump}` uses direct `Command`, a cleared private environment, bounded pipes and termination. These enforce adapter sessions, not today's native production runner.

**Proposed host preflight and launch:**

1. Resolve an exact approved plan digest and recheck source snapshot, policy, script closure, tool catalog, vendor manifest and capabilities. Stale input means zero gate spawns; no automatic reapproval.
2. Confirm the whole tuple `(package/module, gate ID, logical cwd, argv, executable digest, entrypoint digest, config digest, script closure digest, env recipe)`. Resolve the executable through the trusted catalog, never ambient PATH or a project `.bat`/`.cmd` shim. On Windows invoke the pinned PHP executable with a pinned PHP entrypoint/Composer PHAR as separate argv elements.
3. Composer is a dispatcher, so direct parent argv alone is insufficient. Analyze strings/arrays, `@script` references, aliases, hooks, plugins and callbacks as a bounded DAG, reject cycles and unapproved behavior. A simple direct literal recipe may be supported first; complex recipes remain unsupported without substituting scripts. Allow no automatic `install`, `update`, `require`, plugin activation or network dependency resolution, including nested invocations.
4. Stage source/config/vendor/runtime inputs read-only; give Laravel only named cache/storage/test DB/temp output homes. Validate cwd against the staged package root using canonical containment and reject traversal, symlink/junction escape, reparse-point replacement and case collisions. Keep native case in the staged filesystem: `app/Models/Task.php` cannot be lowercased to satisfy today's logical-path grammar. Proposed native-file mapping must bind opaque/logical IDs to exact case-preserved paths and digests, with Linux/Windows collision tests.
5. Use cleared env plus explicit grants; keep secrets out of argv, plan literals and inherited variables. Deny network by default. `SessionBudget`'s destination allowlist presently degrades to network denial, not working network access. Apply per-command and run deadlines, cancellation, concurrent stdout/stderr caps, memory/process limits and descendant cleanup. Cancel dependent commands with unknown measurements, never fabricated success.
6. Persist capability evidence and audit stage/original mutations and cleanup. Missing required containment blocks execution; a private stage plus post-run audit is not an OS sandbox. Test suites needing children need an explicit execution budget, not an unconditional weakening of the adapter's `children: denied`.

Integration gaps: `Sandbox::new` rejects non-logical paths and caps copied content at 64 MiB; PHP runtime siblings/extensions and a Laravel vendor tree need a reviewed runtime bundle and separate byte/file limits. The existing PHP adapter harness reports unavailable confined PHP runtimes on some platforms. `adapters/php-laravel/tests/process.php::run_artifact` uses an escaped command string; it is a test harness, not a compliant argv runner to copy into #61.

## 4. Targeted selection and request/response design

**Existing request seam:** `target_protocol/wire.rs::{NativeRequest,NativeChanges,NativePlanRef}` carries bounded file/symbol changes, `scan_ref`, optional `observed_ref`, `execution_policy_ref`, input/tool/capability digests. Only `plan-native` carries `native_request`; its result carries a digest/summary, never a generation `plan_id` or publish authority. Preserve this outer model.

**Proposed successor fragments** (illustrative shape, not valid v0.3.2 payloads; `sha256:<64hex>` denotes a real digest supplied by the host):

```json
{
  "native_request": {
    "changes": {"files": [{"path": "app/application.php", "change": "modified"}], "symbols": []},
    "scan_ref": {"digest": "sha256:<64hex>"},
    "execution_policy_ref": {"digest": "sha256:<64hex>"},
    "input_manifest_digest": "sha256:<64hex>",
    "tool_catalog_digest": "sha256:<64hex>",
    "capability_snapshot_digest": "sha256:<64hex>",
    "selection_ref": "sha256:<64hex>",
    "bootstrap_receipt_ref": "sha256:<64hex>"
  }
}
```

Proposed `selection_ref` addresses a host-validated document with module roots, symbol ownership, native file mapping, scenario IDs, checked test bindings, test/suite inventory, mandatory cross-module gate IDs and explicit targeting capabilities. Request callers cannot supply arbitrary argv or declare their own graph complete. Existing `NativeContentRef` fields retain their object representation; the new selection/bootstrap references are proposed digest strings.

```json
{
  "native_plan": {"kind": "native-plan", "digest": "sha256:<64hex>", "commands": 3, "packages": 1},
  "selection": {
    "mode": "targeted",
    "modules": ["planner"],
    "tests": ["planner-focus"],
    "mandatory_gate_ids": ["application-boot"],
    "excluded": [],
    "uncertainties": [],
    "fallback_rule_ref": null
  }
}
```

Here `selection` belongs to the proposed digest-addressed plan document, shown beside the response summary for readability; it is not a new free-floating target-envelope field. Bounds and nullability must be fixed in the successor schemas. Each proposed command adds `gate_id`, `required`, `selection_ref`, `covers_suite_ids`, and `depends_on` command IDs; all are inside the approved digest. Selection artifacts cannot be replaced after approval.

**Selection algorithm (proposed):**

1. Attribute changed exact paths/symbols to declared modules; traverse reverse dependencies and scenario/test bindings, retaining graph provenance and reasons. Composer autoload mappings alone do not prove business dependencies. Unknown ownership or incomplete Mago/scanner references must not yield an empty green selection.
2. Union affected checks with mandatory cross-module gates and prerequisites. Build a command DAG for all confirmations, topologically sort deterministically and reject cycles. Retain excluded modules/tests and reason codes so omission is reviewable.
3. Use targeted argv only when the pinned runner's selection capability is tested and the selected inventory is complete. A conservative full gate may be explicitly confirmed as mandatory; otherwise unsupported targeting needs a release-full policy rule or a blocker. Changes to lock/config/routes/providers/shared bootstrap force policy-defined wider checks.
4. Full fallback must enumerate the full declared inventory and record the rule digest and reason. Node's current `buildNativePlan` derives a `release-full` label after constructing commands from the affected list; it does not establish full expansion. `validate_policy` also does not independently require a missing `rule_digest` in that branch. The successor needs both schema and Rust semantic checks plus full-set assertions.

   Implementation note (issue #61): the Rust `validate_policy` gap was real and is fixed — a `release-full` fallback without a `rule_digest` is now a refusal. A further validator divergence found while porting: the Node closed validator refused the schema's own `"."` cwd const (only the logical-path branch was checked), fixed in the same slice; `depends_on` moved to command ids in the successor as this section proposes.
5. Deduplicate by confirmed suite ownership and exact execution identity. If a Composer aggregate already runs Testo/PHPUnit, either record those covered suites under the aggregate with trusted child results or select separately confirmed leaf gates. Never silently run both, strip parts of scripts, or claim coverage from exit zero alone. Release policy may intentionally request a second configuration, with a distinct ID/reason.

## 5. Evidence JSON and redaction

**Existing reusable shape:** `native_gate/types.rs::{NativeCommandResult,NativeRunResult,NativeValueState,NativeOutputRef}` already has cwd, argv, tool ref/version, env names, exit/duration states, reasons, output reference, mutation/original/cleanup and containment evidence. `fixture_tests.rs` currently leaves `tool_version` unset. An actual toolchain catalog is needed, not just optional strings. `contracts/scenario-run.schema.v0.4.0.json` is closed and has neutral assertion rows, not process measurements. Keep it unchanged; link it from gate evidence.

`scenario-emit.php::php_reporter_text` emits `ScenarioReporter::writeToolchainCustody`, recording observed `PHP_VERSION`, installed Laravel/Laratesto/Testo versions and Composer lock digest under `.lekalo/import/toolchain/`. This is a precedent for a separate custody artifact, but it does not record the Composer executable version or prove arbitrary vendor contents. `tests/fixtures/php-laravel/planner/testo.php::ResultRecorderPlugin` records terminal test statuses, useful for detecting failures outside assertion bodies.

**Proposed command-result fragment in the successor:**

```json
{
  "command_id": "planner-scenarios",
  "package_id": ".=lekalo/planner-laravel-fixture",
  "gate_id": "laratesto",
  "required": true,
  "cwd": ".",
  "argv": ["php", "vendor/bin/testo", "run", "--config", "testo.php", "--suite=Laravel"],
  "tool_ref": "php-runtime",
  "toolchain_ref": "sha256:<64hex>",
  "env_names": [],
  "exit": {"state": "known", "value": 0},
  "duration_ms": {"state": "known", "value": 420},
  "outcome": "passed",
  "failure_class": null,
  "reason_codes": [],
  "scenario_run_refs": ["sha256:<64hex>"],
  "output_ref": {"state": "withheld"}
}
```

The numbers illustrate shape only, not measurements from this research. Proposed toolchain artifact contains exact observed PHP and Composer versions, PHP extension/runtime configuration identity, installed Laravel/Laratesto/Testo/Mago versions, binary/entrypoint digests, Composer manifest/lock/vendor digests, OS/architecture, and bootstrap receipt. Missing versions have explicit unavailable state; required compatibility is checked before launch. Record execution selection and per-command coverage; reconcile required test discovery, terminal results and assertion records. Exit 0 with absent records, zero discovered required tests, or unsupported rows is not passed coverage.

**Redaction rules (proposed):**

- Publish project-relative cwd and canonical safe argv, not resolved host executable paths. Reject secret-bearing argv during planning; resolve secret handles only in the child env. Evidence stores env names and recipe digest, never values, `.env`, Composer auth, tokens, DSNs or host home paths.
- Drain raw output only into bounded memory; parse into allowlisted diagnostic fields, remove host paths/control text and scrub sensitive values before any log/artifact write. Withhold output on uncertain classification or parser failure. Timeout/flood/error messages use stable bounded tokens, not echoed command output.
- `output_ref` hashes only redacted bytes. The synthetic fixture runner hashes captured raw stdout; do not carry that behavior into real secret-bearing runs. Avoid secret-value hashes as evidence, including low-entropy values hidden inside recipe digests.
- Apply privacy classification/export rules to structured evidence and linked records. Preserve assertion kind/outcome and safe identifiers while withholding arbitrary failure detail. Host-produced receipts must bind plan/run IDs and output digests; an adapter-supplied success record alone is not trustworthy execution proof.

## 6. Required, optional, degraded and incompatible tools

**Proposed policy classification:** PHP/runtime containment and a verified source/vendor snapshot are required for executable Laravel checks. Mago and Laratesto/Testo are required for the Planner acceptance profile. Composer is required if a Composer script/check is selected; a PHP-only gate may use a separately attested Composer provisioning receipt, explicitly distinguishing provision-time from run-time versions. Laravel and the configured PHP extensions are required for boot/scenario gates. PostgreSQL/Docker or another qualified DB runtime is required when migration execution is mandatory. No missing prerequisite triggers installation.

Pest/PHPUnit is optional only when policy declares it supplementary and it covers no mandatory tests. Formatter or other checks may be optional in a development profile, but release policy can require them. Optional describes requirement, not a blanket permission to ignore a real failing test. A present optional tool producing a genuine assertion/static/boot failure still fails the requested verification.

Native v0.3.2 has no `degraded` outcome. Proposed successor adds a verification summary `verdict: passed|failed|blocked|degraded` and explicit coverage completeness, while retaining native command outcomes. Missing optional tool: command `missing`, reason `optional-tool-missing`, summary `degraded`; missing required tool: command `missing`, summary `blocked`. Never fabricate a command launch/exit or report degraded as full acceptance. Optional incompatible tool behaves similarly with command `unsupported`; required incompatible tool blocks. Missing enforcement or privacy controls is always a blocker, never optional degradation.

## 7. Failure classes mapped to receipts

The existing closed command/run outcomes in `native_gate/receipt.rs::OUTCOMES` are `passed`, `failed`, `missing`, `blocked`, `unsupported`, `infrastructure`, `security`. Proposed `failure_class` is separate from these outcomes and needs the successor schema. Map as follows:

- **assertion:** `failed`, proposed reason `assertion-failed`; valid terminal test result/neutral failure row required. Use existing `NativeGateFailure::RunFailed` / `native-gate.run-failed` for host diagnostics.
- **static-analysis:** `failed` with valid blocking diagnostics. Mago's lock documents exit 1 as findings **or infrastructure** and exit 0 as possibly warning-only; classify using completed parseable results and the confirmed severity policy. Malformed/truncated output is infrastructure, not a clean analysis.
- **boot:** `failed` when the confirmed application check reports a real boot/provider/config failure on a compatible provisioned runtime. Boot never reached because an extension/tool is absent or spawn fails belongs to missing-tool/incompatible/infrastructure instead. Testo lifecycle failures outside test bodies must remain visible.
- **missing-tool:** `missing`; required/optional rollup follows section 6. Preflight misses use `exit: {"state":"unknown"}` and unknown duration, not 0. Existing diagnostic mapping can use `RunFailed { outcome: "missing" }`; richer tool-specific reasons are proposed.
- **incompatible:** `unsupported` with proposed reason `tool-version-incompatible` or `runtime-platform-incompatible`; preserve expected/observed versions. Missing OS enforcement maps to existing `CapabilityMissing` / `native-gate.capability-missing`, not an application failure.
- **infrastructure:** `infrastructure` for spawn/crash/signal, deadline, cancellation, pipe flood, corrupt/incomplete evidence or cleanup failure; existing `RunInfrastructure` / `native-gate.run-infrastructure`. Record actual nonzero exits only when known; do not put a negative signal code into the existing unsigned exit field.

Policy/approval denial remains `blocked`; input drift uses `PlanStale`; unexpected writes/env escape/original mutation use `security` and `SecurityViolation`. Rollup precedence must preserve security first, then infrastructure, then required blockers, then genuine failures, then optional degradation, then pass; retain all per-gate causes even when a stronger terminal class wins. `validate_run_result` already refuses masked original mutation and passed-with-cleanup-failure; extend coherence checks for selection/required coverage.

## 8. CI/local parity and Planner qualification

**Existing CI:** `.github/workflows/ci.yml` runs `build-test` on Ubuntu, Windows and macOS, provisions PHP 8.3 with extensions and Composer, then explicitly installs the fixture lock with scripts disabled before scenario/parity gates. PHP/Composer patch versions are not fixed by that setup. Mago fake mode runs in the matrix; real Mago has a separate Ubuntu job with checksum provisioning and `--real --require-available`. Its lock contains Windows/Linux binaries, no macOS artifact. This is not proof of all-OS real Mago/native confinement parity.

Existing focused checks to preserve: `scripts/test-php-laravel-adapter.mjs` (protocol/process/analyzer/package), `test-php-laravel-scenario-bindings.mjs` (compile/drift/custody), `test-php-laravel-scenario-tests.mjs` (real runtime), `test-php-laravel-parity.mjs` (Node/Laravel semantic records and lying-port failure), `test-mago-integration.mjs`, `test-php-laravel-migrations.mjs`, `test-native-gate-contracts.mjs`, `test-node-native-gates.mjs`, and Rust `native_gate` tests. `semanticRecord`/`semanticRow` in the parity script compare scenario identity/IR, binding mode and assertion semantics, excluding backend details/timing; reuse that comparison principle.

**Proposed `scripts/test-php-laravel-native-gates.mjs` and `tests/fixtures/php-laravel/native-gates/`:**

1. Use a disposable copy of the existing planner fixture and provisioned runtime. Add explicit Planner module/test bindings and a second synthetic module/cross-module gate. A change to `app/application.php` must select relevant Mago and Laratesto checks plus mandatory boot/shared checks, with unaffected module exclusion proven.
2. Verify multiple gates per module, stable ordering, aggregate script/legacy ownership and exactly-once execution. Test both targeted support and unsupported selection; release-full must contain every mandatory command, not just report the label.
3. Exercise real passing/failing assertions, static diagnostics, boot failure, optional/required absence, incompatible versions, zero tests, unsupported scenario rows and stale/tampered linked evidence. Compare expected verdict, coverage, reasons and normalized records locally and in CI.
4. Add hostile argv/cwd, nested script/install, tool/config/vendor drift, case/symlink/junction escape, env/output leak canaries, cancellation/timeout/flood, descendant survival, unexpected write and cleanup failure cases. Assert zero spawns on denied/stale plans and no raw secrets in files/logs/receipts.
5. Run the same harness on all three OSes with explicit PHP/Composer pins and observed extension versions. Provision Mago artifacts per platform; macOS needs a reviewed pin/runtime solution before claiming real coverage. Distinguish pure planner/decoder parity from runtime qualification. An unsupported platform reports its capability gap and cannot satisfy a required execution leg.
6. Compare semantic verdicts for matching declared capabilities, not byte-identical timing, host paths or OS binary digests. CI must require each mandatory leg; visible skips remain pending acceptance. Keep the existing Node/Laravel parity mutation as a regression against false green evidence.

## 9. No-install gate execution and bootstrap custody

Existing #56 provisioning is a useful starting point, with an important local exception: both `scripts/test-php-laravel-scenario-tests.mjs::ensureVendor` and `scripts/test-php-laravel-parity.mjs::ensureVendor` call `composer install --no-interaction --prefer-dist --no-scripts` when `vendor/autoload.php` is absent. Those are test harnesses, not the adapter, but invoking them unchanged as #61 gates would violate the requested no-automatic-install rule. CI's explicit preceding provisioning step usually hides that fallback.

**Proposed lifecycle:** separate operator/CI bootstrap from planning and verification. Bootstrap consumes committed `composer.json`/`composer.lock`, pinned PHP/Composer and extensions, and materializes a vendor bundle outside gate execution. Disable plugins as well as lifecycle scripts by default; any required plugin gets its own reviewed bootstrap policy. Never `update` to repair a failing lock. The existing fixture pins Laravel `13.33.0`, Laratesto `v0.7.3`, Testo `0.10.53` and a Composer platform PHP value `8.3.35`; that platform value is solver input, not proof of the interpreter used.

Proposed bootstrap receipt binds manifest/lock digests, Composer executable/version, PHP/extensions/platform, dev-dependency mode, installed package versions, vendor file inventory and bundle digest, allowed bootstrap steps and runtime assets. Verify actual platform requirements and installed state; existence of `vendor/autoload.php` alone is insufficient. Cache keys include OS/architecture/PHP ABI/lock/config/tool pins; verify restored bytes, reject links escaping the bundle, and copy into isolated runtime staging without modifying the user's vendor tree.

At gate time, validate the bootstrap receipt and all pins before launching any project command. Missing/stale vendor means blocker with a separate provisioning instruction, zero downloads and zero install/update subprocesses. Refactor the existing harnesses in the implementation phase to require provisioned vendor, or expose a distinct explicit bootstrap entrypoint; verification must never invoke their current `ensureVendor` fallback. Laravel package discovery, if necessary, is a separately confirmed stage-local gate, not a Composer install hook. Preserve the project's original scripts/config and original filesystem custody.

Current upstream documentation was fetched through Context7 (`/composer/composer`): [script dispatch and aliases](https://github.com/composer/composer/blob/main/doc/articles/scripts.md), [run-script CLI](https://github.com/composer/composer/blob/main/doc/03-cli.md), [disabling plugins and scripts](https://github.com/composer/composer/blob/main/doc/faqs/how-to-install-untrusted-packages-safely.md), and [platform requirement checks](https://github.com/composer/composer/blob/main/src/Composer/Command/CheckPlatformReqsCommand.php). These support the bootstrap/dispatch distinctions; the actual selected Composer binary still needs pin-specific probes. No unverified Mago formatter or Testo targeted-selection syntax is promised here.

## Delivery sequence and acceptance mapping

Proposed implementation slices: (A) successor contracts and Rust/PHP golden vectors; (B) pure Composer inventory, confirmations, multi-gate selection and full fallback; (C) bootstrap/runtime custody and confined host execution; (D) tool result adapters, redacted evidence and verdict rollup; (E) Planner end-to-end, adversarial tests and all-OS CI qualification. Each slice retains production refusals until its required execution capabilities are proved.

All issue acceptance criteria map to concrete future evidence:

1. **Planner change runs relevant Mago/Laratesto:** sections 2/4/8; real Planner mutation, selection manifest, executed command receipts and reconciled tests.
2. **Additional legacy suite without double-run:** sections 2/4/8; Composer aggregate plus optional legacy fixture, stable suite ownership and invocation-count assertion.
3. **Optional missing degrades, required blocks:** sections 6/7; paired same-tool absence cases, unknown measurements and distinct summary verdicts.
4. **JSON cwd/argv/exit/duration/tool versions:** section 5; successor schema/round-trip tests and real host measurements linked to observed toolchain custody.
5. **No raw sensitive env/output persisted:** sections 3/5/8; canaries on success, failure, timeout and flood, inspected artifacts/logs and cleared child environment.
6. **Targeting retains mandatory cross-module gates:** section 4; multi-module closure, shared boot/config changes, mandatory-union and full-release set equality tests.
7. **Comparable CI/local verdict:** sections 8/9; same pinned inputs/harness, normalized coverage/verdict comparisons, explicit capability gaps and mandatory runtime legs.

Research validation: inspected the merged source and live issue, checked referenced paths and whitespace, and reviewed the proposed/current distinction. No native runtime, Composer installation, test suite or CI run was executed for this documentation-only task. The acceptance tests above remain proposed; this document does not claim issue #61 implementation or acceptance.
