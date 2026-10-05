# Issue #103: headless CI reports and official GitHub Action

Research only; no CLI, contracts, adapters, workflow, or external repository changes are delivered by this document.

Authority: [live issue #103](https://github.com/ichinya/lekalo/issues/103), read with `gh issue view 103 --repo ichinya/lekalo --json number,title,body,url,state` on 2026-10-01. Source baseline: `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`, branch `ichinya/m7-issue-103`, product `0.6.3`. The issue depends on #11, #18, #22, #23, #31, #91, and #92. Its seventh acceptance criterion, in addition to the six summarized in the dispatch, requires examples for core, a Node consumer, and a Laravel fixture.

Recommendation: preserve `DomainResult` as the command/exit authority, capture typed evidence before failure aggregation, and add one versioned CI report model with pure JSON/JUnit/SARIF/Markdown projections. Correct readiness gating explicitly, retain existing conformance JUnit compatibility, and ship the public action from `ichinya/lekalo-action`. Reporting must not run generation, update locks/baselines, turn unavailable checks into passes, or authorize exporting local history.

## Existing-surface inventory

All paths and symbol names below were inspected at the baseline above. Line anchors describe that baseline, not a future implementation.

### CLI envelopes and evaluation boundaries

`crates/lekalo-cli/src/main.rs` has global `--json` and `--no-cache` flags (`Cli`, lines 86-98). `runtime` (2236) parses commands into `DomainResult`; `emit` (2454) renders its JSON or human projection on the status-owned stream. Invalid command syntax also becomes a domain envelope when `--json` is present before the adapter `--` delimiter. Help remains ordinary help; stdout/stderr write failure returns `OUTPUT_FAILURE = 1`. Panics are resumed by `main`, not converted to a domain result.

| Surface | Existing emit/producer locus | Payload and gap relevant to #103 |
|---|---|---|
| `validate [--strict]` | CLI `run_validate` (2601), `render_validate_success`; core `validator/report.rs`, `validator/profile.rs` | Loads/compiles, applies validation profile and classification/authorization/transport reviews, then `DomainResult::validation`; failures return early. A renderer cannot recover missing provenance by parsing the final envelope alone. |
| `scan` | CLI `run_scan` (9508); core `observed/scan_service.rs::run` | Successful `ScanReceipt` uses `DomainResult::receipt`, failures pass through. The adapter scan operation is read-only, but the host calls `observed::update_index` (334), updating derived observed state. Do not silently schedule `scan` as a pure CI check. |
| `verify` | CLI `run_verify` (8806); core `orchestration/verify.rs::run`, `receipt.rs` | Validates model/drift/targets and consumes existing scenario evidence. Successful receipt binds model/IR/lock. Blocked results return `aggregate_envelopes` and degraded results return `UnsupportedOperation`, losing the assembled component receipt. `native.gates` remains a declared optional unsupported component (307-312), despite a separate native runner existing. |
| `generate --check [--locked]` | CLI `run_generate` (8689, check branch 8732); core `artifacts/check.rs`, `artifacts/diagnostic.rs` | Already reads without adapters or writes. `Prepared` binds current model/IR/lock; blocking stale/manual-drift/missing/orphan findings become `ArtifactFailure::Drift`, then `invalid`/1. Report-only lifecycle findings can stay nonblocking. `--check` rejects generation/clean operands. |
| `impact` | CLI `run_impact` (8938), `render_impact` (9071), `git_input.rs`; core `impact/` | JSON is `DomainResult::impact`, warnings remain structured; strict bounded denial is distinct. Reuse revision-aware changed-input handling, not raw `git diff` text in artifacts. |
| `diff` | CLI `run_diff` (2505); core `diff/` | Compares two project selectors, accepts `--base` and `--profiles`; successful comparison is `{"status":"valid","diff":...}` even when profile verdicts reject a change. `--format json` is already a command option. CI breaking-policy evaluation is a separate missing step. |
| `doctor`, `status`, `readiness` | CLI `run_doctor`/`run_status`/`run_readiness` (4779/4793/4804); core `doctor/mod.rs::report`, `model.rs`, `checks.rs`; CLI `doctor_git.rs` | `report` deliberately returns a valid receipt whenever constructed, including `blocked` verdicts. Existing CLI tests assert exit 0 for report production. Git commit/dirty and lock digest exist, but model revision only contains version; no complete effective profile digest panel. |
| `history ...` | CLI `run_history` (4586); core `run_history/`; `docs/run-history.md` | Local-only scoped SQLite receipts via `DomainResult`; record input is stdin-only. Provenance value-state vocabulary is useful design precedent, but history records are `local-private` and export-ineligible. No CI job may initialize, collect, upload, or repackage the store automatically. |
| `adapter test --report json\|junit` | CLI `run_adapter_test` (4179), `AdapterRun::Document` (2426); core `adapter_conformance/report.rs` | Existing format selector owns stdout for a completed suite; failing results can also emit a JSON envelope on stderr. Preflight/transport failures can occur before a suite document exists. Global `--report <path>` would collide with this established meaning. |
| `native run --plan` | CLI `run_native_run` (9481); core `native_gate/runner.rs`, `receipt.rs`, `types.rs` | Closed plan approval and terminal receipts already distinguish assertion/static-analysis/boot/missing-tool/incompatible/infrastructure and mutation/security states. A successful receipt write is not proof its inner run passed; CI needs typed mapping. |

`crates/lekalo-core/src/result.rs` owns status, exit, stream, normalized diagnostics, and `reasonCodes`. There is no top-level domain `failed` status: that word occurs in subordinate target/native outcomes. Receipt payloads are not one uniform schema; some carry their own status/verdict spelling. A wrapper must read the typed result, not infer domain status from arbitrary nested `status` fields.

### Diagnostics, existing formats, and scenario harnesses

The diagnostic item is `contracts/diagnostic.schema.v0.2.16.json`; the currently embedded registry is `contracts/diagnostic-registry.v0.4.0.json`, selected by `diagnostics/version.rs`. `diagnostics/registry.rs`, `normalize.rs`, `types.rs`, `id.rs`, `provider.rs`, and `render.rs` implement rule identity, normalization, bounds, safe locations, provider decoding, and human output. `docs/diagnostics.md` explicitly assigns SARIF/report files to #103; its older version-history paragraphs are not the current registry authority.

Diagnostics preserve both dotted rule ID and immutable `LEK-*` code, registry version, severity (`info|warning|error`), category, safe default English message, optional semantic symbol/source, bounded data, related locations, causes, fixes, and namespaced original provider code. Severity does not determine exit. Logical paths reject absolute/drive/UNC/backslash/URI/traversal forms; current diagnostic path grammar also restricts case/characters. Positions use zero-based half-open byte ranges and one-based Unicode-scalar line/column coordinates. Normalization has a 256-diagnostic bound, deterministic ordering, and exact deduplication; overflow is a failure, not silent truncation.

Search performed: case-insensitive `sarif|junit|aif-gate-result` over tracked source/docs plus `.github`.

| Format | What exists | What is missing |
|---|---|---|
| Native JSON | Domain envelopes, conformance report, doctor report, orchestration receipt, scenario/native records | A closed CI envelope that survives failure and binds all revisions consistently. |
| JUnit | `adapter_conformance/report.rs::Report::junit` (229): one testcase per catalog check; pass/failure/skipped, deterministic and escaped, no raw output/timing. Tests in `crates/lekalo-core/tests/adapter_conformance.rs` and `crates/lekalo-cli/tests/adapter_test.rs`. CI prints it. | General scenario/native/script-gate JUnit, infrastructure `<error>` mapping, durable report output, and missing-suite cases. |
| SARIF | PHP adapter requests Mago SARIF in `adapters/php-laravel/adapter.php` and its toolchain lock; `tests/fixtures/mago/toolchain/recording.sarif.json`, PHP analyzer tests, and `scripts/test-mago-integration.mjs` check upstream Mago output. | No Lekalo diagnostic-to-SARIF exporter or upload/annotation workflow. Upstream Mago output is not a Lekalo report. |
| Markdown | Human CLI summaries and repository docs | No common CI job-summary renderer or `$GITHUB_STEP_SUMMARY` publication in `ci.yml`. |
| `aif-gate-result` | External references in trace/readiness and the authority boundary described in `docs/authority.md` | No matching in-repo emitter/schema. AIFHub Extension owns conversion and its destination contract. |
| Bundle manifest | Generated-artifact ownership manifest and various evidence receipts | No dedicated report-bundle manifest. Reusing the generation manifest would conflate generated source custody with CI report packaging. |

Scenario evidence is already neutral: `contracts/scenario-run.schema.v0.4.0.json`, `scenario_evidence.rs::RunRecord`, and generated reporters in `adapters/node-typescript/src/scenario-emit.mjs` / `adapters/php-laravel/src/scenario-emit.php`. Records go to `.lekalo/import/scenario-runs/`; assertion outcomes are exactly `pass|fail|unsupported|infrastructure|degraded`. They bind scenario IR, runner, profile (nullable), test fingerprint, and binding mode. `orchestration/verify.rs` validates/rolls up records, detects stale IR, missing expected tests, and malformed evidence, and can project trace relations. Preserve this decoding path instead of grepping console output. Extend its custody checks where a report requires current test/profile/run-occurrence binding beyond the existing IR comparison.

`scripts/test-node-scenario-tests.mjs` drives committed adapter generation and real `node --test`; its own output is `ok - ...`/`FAIL - ...`, stack text, and a text footer. `scripts/test-php-laravel-scenario-tests.mjs` runs the provisioned Laravel/Laratesto fixture, with similar lines and a small `{ok,gate,...}` footer. Both inspect durable records and explicit unsupported concurrency scenarios. `scripts/test-php-laravel-parity.mjs` compares semantic tuples and a shared failing mutation, and offers separate `--emit-evidence` comparison output. These are acceptance harnesses, not general safe CI-report producers: raw stacks/temporary paths are currently printable, and temporary scenario trees are removed. Report export must happen from typed rows before cleanup. Do not count an intentionally failing negative-control scenario as a failure of the enclosing harness gate; keep the specimen suite and the harness expectation separate.

### Current CI and contract conventions

`.github/workflows/ci.yml` runs on `push` and `pull_request`, with `contents: read` globally. It contains:

| Job | Existing coverage |
|---|---|
| `contracts` | Ubuntu, Node 18.x/24.x; checkout depth 2; exact Ajv 8.17.1 outside checkout; schema gates, runtime protocol vectors, authority/privacy/structure/model gates; `check-contract-versions.mjs --base HEAD^`. |
| `fmt`, `clippy` | Rustfmt check and workspace Clippy with warnings denied; stable Rust channel. |
| `msrv` | Rust 1.80.0, Ubuntu/Windows/macOS, check and Clippy, matrix fail-fast false. |
| `build-test` | Same three OSes, Bash steps, build/tests and extensive script gates; PHP/Composer fixture provisioning; Node adapter/scenario/native suites; conformance JUnit printed to stdout. Linux bubblewrap/AppArmor prerequisite and `always()` cleanup remain significant. |
| `mago-real` | Ubuntu, 15-minute job timeout; checksum-pinned external Mago install; real-toolchain gates require availability. |

There are **no report artifact uploads**, SARIF uploads, or job-summary steps. No explicit dependency cache action is configured; setup-node automatic package-manager caching is disabled. Several steps have timeouts, but only `mago-real` has a job-level timeout; no workflow concurrency cancellation is declared. Actions mostly use mutable major/channel refs, runtime selectors include `stable`, `24.x`, and PHP `8.3`. Report exact observed versions even when CI intentionally exercises a floating channel; cache keys cannot use the selector as an exact version.

`docs/versioning.md` and `scripts/check-contract-versions.mjs` require each new/changed contract to take the product version of the implementation commit, including filename, `$id`, identity/discriminator, embedded constants, consumers, and hashes. Untouched contracts keep their accepted versions. The checker has a narrow archival exception for exact historical bytes restored at the same path; it is not permission to introduce new report content under an old version. Let **R** mean that implementation product version (currently 0.6.3, not a reservation for future work). Never invent a fresh independent `v1.0.0` report wire merely because the action uses `@v1`.

## Report-format design

### Command surface and implementation seams

Add `crates/lekalo-core/src/ci_report/` (new: `mod.rs`, `model.rs`, `version.rs`, `policy.rs`, `provenance.rs`, `junit.rs`, `sarif.rs`, `markdown.rs`, `bundle.rs`) and CLI `report_output.rs` / `report_git.rs` (new). Keep rendering pure; filesystem/Git acquisition stays at the CLI or accepted core filesystem seams. Introduce an internal typed command outcome that carries the original `DomainResult`, checks/suite rows, and a pre-execution provenance snapshot. Extract this before `verify` discards its receipt and before readiness chooses its envelope. Do not parse human text or rerun an adapter to create another format.

Recommended proposed surface:

```text
lekalo validate --strict --json --report-file <out>/validate.json --report-format json
lekalo generate --check --locked --json --report-file <out>/drift.json --report-format json
lekalo verify --locked --ci-policy <policy> --as-of YYYY-MM-DD --gate-observation <safe-observation.json> --json --report-file <out>/verify.json --report-format json
lekalo readiness --phase release --json --report-file <out>/readiness.json --report-format json
lekalo report render --input <out>/verify.json --format junit --output <out>/junit.xml
lekalo report render --input <out>/verify.json --format sarif --output <out>/diagnostics.sarif
lekalo report render --input <out>/verify.json --format md --output <out>/summary.md
lekalo report bundle --input <out>/verify.json --artifact junit=<out>/junit.xml --artifact sarif=<out>/diagnostics.sarif --artifact md=<out>/summary.md --output <out>/manifest.json
```

These are proposals, not commands available at the researched commit. Use `--report-file` plus `--report-format json|junit|sarif|md` rather than global `--report`/`--format`: existing `adapter test --report json|junit` and `diff --format json` retain their meaning. `--report-file` defaults its format to JSON; reject a report format without a path and reject ambiguous output combinations before work. A named file is a side channel; `--json`, human output, and existing stream ownership stay stable. `report render` is an offline transformation: exit 0 means conversion succeeded, not that the stored run passed. The action must retain the producing command's exit code.

Wire the side-channel into validate, generate-check, verify, readiness first; diff/impact/scan and adapter/native receipt projections share the same plumbing. Scan reporting never authorizes an additional scan. Explicitly reject CI export flags on `history` operations until a separately accepted export contract exists. No new `report collect` command may scrape `.lekalo/history`, arbitrary JSON directories, or `.ai-factory/qa`.

`--ci-policy` is a closed, digest-bound evaluation policy, not a script runner. Add policy evaluation to `orchestration/verify.rs` and the typed readiness path, plus a reusable evaluator for diff/native/script observations. It declares expected checks, requirement levels, unavailable behavior, coverage thresholds, waiver rules, and deadlines. It may strengthen built-in required checks; weakening a base-branch required check requires explicit maintainer policy review, never a PR-supplied default. Existing services perform model/lock/drift/trace checks; native/scenario runners execute separately under their accepted approval/confinement policy and provide checked evidence. Reading an old success file is not proof of this invocation's execution.

Add optional repeatable `verify --gate-observation <path>` for explicitly named native/script observations; absence of this operand never starts a runner. Decode through a new `ci_report/observation.rs` bridge, bound to the trusted workflow's invocation receipt and the same input/policy/profile tuple. Required fields include schema/producer identity, check/suite ID, input-set and source-receipt digests, original outcome/failure class, expected/observed case inventory, and observed terminal process status. Reject conflicting duplicates, unknown producers, incomplete terminal records, mismatched invocation/provenance, and undeclared checks. Keep self-reported fork evidence labeled as untrusted execution evidence; digest validity cannot grant release approval. The core harness may emit these files directly via its shared reporter; JSON console footers are not an ingest format.

### Closed JSON contracts and draft shapes

Propose `contracts/ci-report.schema.vR.json`, `ci-policy.schema.vR.json`, `ci-gate-observation.schema.vR.json`, and `ci-bundle.schema.vR.json`. Add Rust decoders and `scripts/test-ci-report-contracts.mjs` using the existing exact Ajv setup. `ci-gate-observation` is the opt-in bridge for script harnesses and native receipt adapters: typed IDs, checked source pins, observed process termination, source outcome, and safe counts. It grants no authority by itself. Extend existing orchestration/doctor contracts only if their public payload changes; update their owner tests and references with version R. Keep diagnostic item/registry versions unchanged unless adding registered CI rules, in which case publish the registry successor deliberately.

Every object is closed (`additionalProperties: false`; appropriate `unevaluatedProperties: false` with unions). Define bounded arrays/strings, exact enums, safe path types, digest syntax, status/exit coherence, unique IDs, count derivation, and rejection of unknown versions/members. Semantic validators must supplement JSON Schema. Canonical JSON is UTF-8, compact, byte-sorted object keys, one LF; arrays sort by stable identities except assertion execution order and cause chains. No wall-clock duration, timestamp, absolute path, user/host name, remote URL, environment value, or raw child transcript enters canonical report content. An explicit `asOf` is an evaluated policy input and therefore bound, not ambient metadata.

Draft JSON below shows field shapes. `R`, `GIT_SHA`, and `<...>` are metasyntactic values, not valid fixture hashes. Implementation fixtures must use exact accepted versions and real complete hashes.

```json
{
  "schema_version": "lekalo/ci-report/vR",
  "identity": "dev.lekalo.ci-report@R",
  "producer": {"id": "lekalo", "version": "R", "buildDigest": "sha256:<64hex>"},
  "invocation": {"command": "verify", "mode": "full", "targets": ["node-typescript"], "modules": [], "locked": true, "asOf": "2026-10-01"},
  "provenance": {
    "git": {"state": "known", "commit": "GIT_SHA", "dirty": false, "workingSetDigest": "sha256:<64hex>", "baseCommit": null, "eventHeadCommit": null},
    "model": {"state": "known", "version": "0.2.16", "digest": "sha256:<64hex>", "irVersion": "0.2.16", "irDigest": "sha256:<64hex>"},
    "lock": {"state": "known", "version": "0.2.16", "digest": "sha256:<64hex>"},
    "profiles": [{"role": "validation", "target": null, "state": "known", "id": "default", "version": "0.4.0", "digest": "sha256:<64hex>"}],
    "adapters": [{"id": "node-typescript", "version": "<exact-version>", "manifestDigest": "sha256:<64hex>", "bundleDigest": "sha256:<64hex>"}],
    "policy": {"identity": "dev.lekalo.ci-policy@R", "digest": "sha256:<64hex>"},
    "inputSetDigest": "sha256:<64hex>",
    "matrix": {"target": "node-typescript", "os": "linux", "arch": "x64"}
  },
  "commandResult": {"status": "valid", "exitCode": 0},
  "evaluation": {"status": "valid", "exitCode": 0, "verdict": "degraded", "coverage": "incomplete", "complete": true},
  "checks": [{"id": "integration.hlv", "target": null, "required": false, "sourceOutcome": "unavailable", "failureClass": "missing-component", "effectiveOutcome": "warning", "diagnosticIndexes": [], "evidenceDigest": null}],
  "suites": [],
  "diagnostics": [],
  "waivers": [],
  "publication": {"classification": "ci-derived", "policyRef": "<accepted-policy-ref>", "decision": "allowed"}
}
```

`complete` means the declared plan reached a terminal evaluation; coverage can still be incomplete because an explicitly optional check was unavailable. `commandResult` preserves the underlying observation; `evaluation` is the authoritative gated exit/verdict, and the producing CLI/action returns it. Invalid/denied/version failures and actual execution failures are never downgraded by formatting; only a typed optional absence is eligible for the declared policy. Do not accept arbitrary receipt payloads as a free-form `data` object. Reference source receipts by validated kind/version/digest and project only fields admitted by each adapter.

Value states are discriminated unions: `known` requires the complete corresponding pin; `unknown` requires a closed reason and forbids fabricated values; `not-applicable` requires a declared applicability reason. Partial collection on early failure is legal and explicit. Missing lock is unknown, never the hash of an empty file. A required provenance failure itself prevents release readiness. Record all effective validation/target/storage/native policy profiles, including fully resolved profile bytes and resolver version, not only a profile name or declared inheritance chain. Profile array above is shortened for illustration.

Capture checked-out Git SHA (the tested merge SHA on default PR checkout), actual event head and semantic comparison base separately. Hash canonical model/IR via their owners and canonical lock via `LockService`; hash effective profiles through `target_profile/` resolution and validation profile bytes. Define `workingSetDigest` over the sorted relative path/content-digest input inventory, with an algorithm/version, excluding output directories and runtime caches. Recheck inputs at finalization; a changed input yields stale/incomplete evidence, never a report bound retroactively to new bytes. Non-Git trees and unreadable/invalid models produce unknown states without leaking paths. Run occurrence metadata used to reject replay belongs in a bounded companion receipt; it is not a random field in deterministic report content.

Draft suite and policy fragments (nested schemas are also closed):

```json
{
  "suite": {
    "id": "scenario.node-typescript.planner",
    "kind": "scenario",
    "target": "node-typescript",
    "sourceDigest": "sha256:<64hex>",
    "cases": [{"id": "planner.focus_happy/emitted/0", "scenarioId": "planner.focus_happy", "testId": "planner.focus_happy", "stepId": "emitted", "assertionKind": "event", "required": true, "sourceOutcome": "fail", "failureClass": "assertion", "detailCode": "expectation-mismatch", "diagnosticIndexes": []}]
  },
  "policy": {
    "schema_version": "lekalo/ci-policy/vR",
    "identity": "dev.lekalo.ci-policy@R",
    "checks": [{"id": "integration.hlv", "required": false, "onUnavailable": "warn", "onDegraded": "warn"}],
    "requiredProvenance": ["git", "model", "lock", "profiles"],
    "waivers": {"requireExpiry": true, "warnWithinDays": 14},
    "limits": {"commandTimeoutMs": 300000, "reportBytes": 8388608, "cases": 10000}
  }
}
```

Proposed enum distinctions: original outcome preserved from its source contract; normalized outcomes `pass|fail|unavailable|unsupported|degraded|denied|cancelled|not-run`; coverage `complete|incomplete|unknown`; failure class `assertion|static-analysis|boot|missing-component|incompatible|infrastructure|security|policy|evidence-invalid|output`. The adapters must enumerate their source vocabularies, including native `missing`/`blocked`, not accept arbitrary tokens. Policy `onUnavailable` is `fail|warn|skip`, only for explicitly optional declared checks. It cannot suppress unknown receipt corruption, security denial, actual assertion failures, report failure, or cancellation. Fixed limits are draft bounds to qualify with corpus tests; exceedance must be visible and nonzero.

### JUnit projection

Use `<testsuites>` containing separate scenario, adapter-conformance, native-gate, and script-gate suites; include target/profile in stable suite identity. For scenario records emit one testcase per assertion, with stable scenario/test/step/kind identity and source ordinal to disambiguate repeated assertions. Preserve scenario grouping in `classname` and suite properties so test counts are clearly assertion counts. Emit an explicit coverage testcase when an expected scenario/test is absent; do not turn an empty records directory into a passing empty suite.

| Source observation | JUnit row | Evaluation |
|---|---|---|
| `pass` / native `passed` / conformance pass | testcase with no failure/error/skip child | Pass, only with valid current evidence. |
| Scenario `fail`, conformance semantic/check failure, native assertion/static-analysis/boot `failed` | `<failure type="assertion|static-analysis|boot|adapter" message="safe-code"/>` | Nonzero. A broken application boot stays a boot failure when that is the source classification. |
| Provider/runner/conformance infrastructure failure, timeout, malformed/missing expected evidence | `<error type="infrastructure|timeout|evidence-invalid" message="safe-code"/>` | Required failure; never an assertion failure or pass. Preserve conformance process/protocol/security class when choosing the appropriate error or required-unavailable row. |
| Optional `unsupported`/unavailable allowed by policy | `<skipped message="unsupported|unavailable"/>`, source outcome and policy in properties | Skip/warn is visible, with incomplete coverage; exit 0 only under the accepted optional policy. |
| Required unsupported/unavailable, or policy-promoted degraded row | `<error type="required-unavailable|policy"/>` | Nonzero even when a native test runner calls it skipped. |
| Denied/security | `<error type="security|denied"/>` | Nonzero; never waived as optional absence. |
| Cancellation/not-run after abort | Synthetic execution error plus explicit not-run/skipped planned rows | Incomplete, cancelled; never green. Hard kill can prevent file emission. |

Counts derive from emitted cases; include `errors` separately from `failures`. No `<system-out>`/`<system-err>`, source snippets, actual/expected values, or uncontrolled exception text. XML-escape attributes/text and reject illegal XML characters rather than producing malformed XML. Use standard `<properties>` for provenance/status/coverage/report digest instead of depending on custom attributes. Preserve the existing `adapter test --report junit` bytes until a documented successor is accepted; the new CI renderer may provide richer errors without silently changing the old projection. Validate with a real XML parser and the selected consumer dialect; JUnit XML has multiple consumer dialects, so do not claim a universal closed JUnit standard.

### SARIF and annotations

Emit SARIF 2.1.0 with a Lekalo tool driver, exact producer version, registry-derived `rules`, deterministic rule indexes, and normalized diagnostic results. `ruleId` is immutable `LEK-*`; rule `name` and result properties retain the dotted ID, registry version, category, and safe symbol. Map error/warning/info to error/warning/note; do not invent numeric security severity from a category. Domain/policy verdict remains separate from diagnostic level. Store exact report/provenance references in a closed Lekalo property projection; do not upload arbitrary raw metadata.

```json
{
  "$schema": "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json",
  "version": "2.1.0",
  "runs": [{
    "tool": {"driver": {"name": "Lekalo", "semanticVersion": "R", "rules": [{"id": "LEK-ADP-002", "name": "adapter.check-failed", "shortDescription": {"text": "A conformance check failed for the target adapter."}}]}},
    "columnKind": "unicodeCodePoints",
    "originalUriBaseIds": {"%SRCROOT%": {"description": {"text": "Repository root"}}},
    "automationDetails": {"id": "lekalo/node-typescript/linux/"},
    "results": [{"ruleId": "LEK-ADP-002", "ruleIndex": 0, "level": "error", "message": {"text": "A conformance check failed for the target adapter."}}],
    "properties": {"lekaloReportDigest": "sha256:<64hex>", "lekaloStatus": "invalid", "lekaloExitCode": 1}
  }]
}
```

The example is intentionally locationless: conformance-wide failure has no source span. For a located rule add `locations[].physicalLocation.artifactLocation = {"uri":"<safe-repo-relative-path>","uriBaseId":"%SRCROOT%"}` and `region` from the validated source range. Preserve one-based lines/columns and exclusive ends; do not reinterpret byte offsets as columns. Declare `unicodeCodePoints`. The SARIF root may omit its absolute URI for privacy/determinism; OASIS explicitly permits this. [SARIF specification, sections 3.4, 3.14.14, 3.14.27, and 3.30](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)

Before rendering, convert project-relative paths to repository-relative paths using a verified project-root prefix, important for monorepos/fixtures. Enforce containment with `project_fs`-style no-follow checks; reject symlinks/junctions escaping the checkout, drive/UNC/URI forms, traversal (including encoded variants), control characters, and ambiguous aliases. Preserve source spelling/case; never lowercase a native file path to fit the diagnostic grammar. If native source names exceed the current diagnostic contract, coordinate a versioned location-contract extension; until then produce a locationless diagnostic with an explicit location-unavailable reason. Never invent line 1 or map an invalid span to unrelated code. Validate source digest/range against the analyzed revision, including Unicode and CRLF.

Related locations retain safe paths and numeric IDs; primary location is deterministic. Rules with no usable path stay in SARIF/JSON and produce job-level annotations. No source contents, environment, command line, full remote URI, or executable fixes are exported. A stable versioned diagnostic fingerprint may use safe rule/symbol/location identity; never label a homemade hash as GitHub's `primaryLocationLineHash`. Let the pinned upload action calculate GitHub fingerprints when the exact checkout is available, and test repeat-upload deduplication. GitHub matches relative paths to the repository and uses separate analysis categories for distinct matrix reports; validate both schema and ingestion behavior. [GitHub SARIF support](https://docs.github.com/en/code-security/reference/code-scanning/sarif-files/sarif-support), [SARIF uploads](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/integrate-with-existing-tools/upload-sarif-file)

The action emits bounded `::error`/`::warning`/`::notice` annotations from the same normalized diagnostics, with repository-relative `file` and validated position when available. Escape workflow-command data/properties (`%`, CR/LF and property delimiters); never relay raw child stdout as workflow commands. Annotate a changed-file fixture to prove inline placement; a location outside a PR diff is not guaranteed an inline PR comment. Reaching an annotation limit adds a summary count and retains complete JSON/SARIF, never implies complete inline presentation.

### Markdown, AIFHub adapter, and optional bundle

Markdown contains command/verdict/exit, required failed and optional unavailable counts, scenario coverage, exact commit/model/lock/profile pins, and a bounded diagnostic table with safe file positions. Escape Markdown/HTML/link syntax and omit raw details. Include explicit truncation counts and links only to caller-approved local artifact names. Append those bytes to `$GITHUB_STEP_SUMMARY` in the action; a rendered document is not itself a published summary. [GitHub workflow commands](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-commands)

The AIFHub Extension consumes the canonical report with schema/version/digest checks and maps it to its **own pinned** `aif-gate-result` contract. Lekalo core does not write `.ai-factory/qa/**` or claim an AIF/HLV verdict. Proposed handoff contents are provider identity/version, original command status and reason codes, effective required policy, per-gate outcomes including unsupported/infrastructure, exact provenance, and report artifact digest. These are mapping requirements, not a fabricated AIF wire schema. Implementation must obtain the actual Extension schema/version, add a compatibility fixture and Extension-side conversion tests, and prove lossless mapping or reject unsupported values. Missing Extension is optional-unavailable by default, configurable required by policy; no network/token required to produce native reports. Credentials/destination ownership for any later upload are a separate decision.

Optional bundle draft:

```json
{
  "schema_version": "lekalo/ci-bundle/vR",
  "identity": "dev.lekalo.ci-bundle@R",
  "reportDigest": "sha256:<64hex>",
  "inputSetDigest": "sha256:<64hex>",
  "complete": true,
  "artifacts": [
    {"path": "verify.json", "format": "json", "mediaType": "application/json", "sha256": "sha256:<64hex>", "bytes": 1234},
    {"path": "junit.xml", "format": "junit", "mediaType": "application/xml", "sha256": "sha256:<64hex>", "bytes": 567}
  ],
  "publication": {"policyRef": "<accepted-policy-ref>", "decision": "allowed"}
}
```

Hash exact emitted bytes after redaction. Artifacts sort by unique bundle-relative path; reject links, traversal, duplicates, unlisted files, mismatched provenance, and hash/size mismatch. Do not include the manifest in its own digest set. It is a reproducibility/integrity manifest, not a signature or proof that untrusted execution told the truth. Publish the manifest last after all required files are atomically finalized; partial bundles cannot claim `complete`. The action uploads only this allow-list, not the entire checkout or `.lekalo/**`.

Report writes use a separately granted output directory, preferably a fresh runner-temp directory outside analyzed roots. Reject destinations overlapping source/model/locks/baselines/history, symlink/reparse traversal, existing arbitrary files, and concurrent writers. Render bounded bytes before opening output, stage within that directory, atomically rename, and clean only that invocation's temporary files. With no report flag the read-only check remains zero-write; with explicit report flags the sole permitted writes are report outputs. Filesystem failures are typed report failures, not a successful check with a missing artifact.

Privacy applies before all sinks: JSON/XML/SARIF/Markdown/annotations/action logs/artifacts. Reuse diagnostic allow-lists and `privacy/redact.rs` leak scanning; suppress raw process output, actual test values, argv/env, stack traces, URLs, and absolute paths. A bounded text field is not automatically secret-free. Apply classification/export decisions using accepted `privacy/context.rs`, `privacy/evaluate.rs`, `privacy/export.rs` and authority policy; redaction alone cannot make a prohibited artifact publishable. CI report artifact kinds/destinations need explicit successor admission under the #120 ownership process. Never relabel history or HLV evidence to bypass it. Use synthetic secret canaries in every output format test.

## Exit-policy mapping

### Existing contract to preserve

| Domain status | Exit | Stream | Meaning |
|---|---:|---|---|
| `valid` | 0 | stdout | Command succeeded; an inner report verdict may currently still block. |
| `invalid` | 1 | stderr | Invalid input, failed validation, drift, assertion/conformance failure, usage. |
| `denied` | 3 | stdout | Security/permission/custody/policy refusal. |
| `unsupported` | 4 | stdout | Negotiated capability unsupported. |
| `unavailable` | 4 | stdout | Required component/infrastructure unavailable. Distinguish from unsupported in JSON. |
| `unsupported-version` | 5 | stderr | Contract version outside accepted registry. |

`result.rs::Status` is authoritative; no exit 2 is promised by the CLI. Output failure is exit 1 even if the original check passed; process signal/panic codes can be outside this set and must remain identifiable at the action boundary. Do not treat every exit 4 as an optional skip.

Existing `orchestration/generate.rs::aggregate_envelopes` selects `unsupported-version > invalid > denied > unsupported > unavailable` and preserves only diagnostics from the winning status class. Keep this documented precedence for legacy callers; CI reports retain **all** component rows and reasons before choosing the same deterministic top-level class. A policy decision must not depend on worker completion order or numeric maximum exit. Native receipts have their own verdict precedence and must retain their original outcomes rather than being overwritten by this domain precedence.

### Required changes and explicit evaluation rules

1. Make `readiness` a gate: ready -> 0; blocked required checks -> the classified nonzero domain status; missing required component -> 4; invalid model -> 1; security refusal -> 3; unsupported contract -> 5. Preserve the doctor/status informational behavior. This is a deliberate behavioral change to ADR-0032 and `crates/lekalo-cli/tests/doctor.rs`, requiring release notes and owner review. If maintainers require compatibility, introduce an explicit `readiness --check` first and require it in the official action; do not claim plain readiness already gates.
2. For `verify`, retain the full receipt for blocked/degraded outcomes and apply a versioned policy to optional unavailable/degraded rows. Default required failures and actual failures are nonzero. Explicit optional missing/unsupported may warn or skip with exit 0 and incomplete coverage; strict policy promotes them to 4. A required component cannot be downgraded by a report formatter or a candidate PR's policy file.
3. Diff computation remains valid when successful; a separate CI policy check maps disallowed breaking/security/migration verdicts to `denied`/3. Preserve semantic change detail and base/candidate pins. `impact` is scope evidence, not proof that an omitted check passed.
4. `generate --check` retains current no-write drift behavior and invalid/1 findings; do not replace it with `generate` followed by `git diff`. Lock absence/mismatch/version refusal retains its actual status, not a blanket drift code.
5. Provider infrastructure/timeouts have `failureClass=infrastructure` and nonzero `unavailable`/4; source assertion failures remain `invalid`/1. Bad protocol evidence remains a typed invalid/unsupported result as defined by its owner, with original cause retained. A successful provider HTTP/process exchange is not a passing gate.
6. Cancellation yields incomplete evidence and cannot be accepted under optional-unavailable policy. The action forwards cancellation to child process trees, honors grace/kill deadlines on Linux/macOS/Windows, records a typed terminal observation if possible, and preserves the runner's cancelled conclusion. Hard termination cannot guarantee any report; a missing terminal artifact is not success.
7. Malformed reports, requested output write failure, or required artifact publication failure fail the reporting step. If analysis also failed, preserve its exit in outputs and report the publication failure separately; never overwrite a failure with a renderer/upload exit 0. Unknown process exit/panic/signal creates a bounded infrastructure observation, not guessed diagnostics or a forged completed report.

## Action design recommendation

Use a separate repository **`ichinya/lekalo-action`** with root `action.yml`, `src/` launcher, committed distributable, `package.json`/lock, contract fixtures, cross-OS tests, and release workflow. A JavaScript action is preferable to a shell-heavy composite here because argv validation, streaming suppression/redaction, cancellation, and Windows process handling need one reviewed implementation. Keep policy/rendering in the Rust CLI so action logic stays small. Qualify the Actions runtime/toolkit version when implementing; do not assume the repository's Node matrix version selects the JavaScript action runtime.

An in-repo composite action would simplify source dogfooding but does not satisfy the public `ichinya/lekalo-action@v1` location and couples consumer upgrades to compiler development. Do not maintain two independent launchers. Initially dogfood the CLI via a fixed repository test harness; once released, invoke the public action pinned to a reviewed full commit SHA and supply the just-built CLI through an explicitly restricted development input. The consumer-facing `@v1` example remains, with a documented immutable-SHA alternative and actual release provenance.

Proposed action inputs: `command` (allow-list: validate, generate-check, verify, readiness), `locked` (default true where applicable), exact `version` (no runtime latest lookup), project subdirectory, target/profile/phase, timeout, report formats, policy reference, base SHA, and artifact publication mode. `generate-check` expands to `generate --check`; the issue's `command: verify` / `locked: true` works unchanged. Inputs representing data become argv elements with `shell: false`; reject unsupported combinations. No arbitrary shell-command input, automatic `lock`, `update`, `observe baseline`, `migrate`, `doctor --fix`, or generation apply. Check-only lock/profile preflight is explicit where a command has no `--locked` flag.

Resolve the action's bundled exact default CLI pin or explicit exact version from a reviewed release manifest, verify archive and binary checksums, and fail if that OS/architecture artifact is missing. No fallback to latest or an unverified system binary. A source-built `cli-path` input is confined to same-repository dogfood tests, marked in producer provenance, and not used in the minimal consumer example. Installation/provisioning is separate from checking and recorded with exact observed tool versions.

Outputs: original command exit, evaluated exit/status/verdict/coverage, report directory and per-format paths, report digest, manifest path, publication status. Produce outputs and safe annotations/summary before failing the action on a required check. Use a documented finalization path so failure artifacts can still be uploaded; a success from upload must never reset the check failure. Do not expose run tokens or private paths in outputs.

On malformed CLI invocation, emit the existing usage envelope and only write a report if the output destination was safely validated; do not attempt an unsafe best-effort path parse. The action supervisor records a bounded startup failure when no command report exists. Keep that supervisor observation distinct from a CLI-completed run, and test preflight refusal before any suite starts. The issue's minimal public entry remains:

```yaml
- uses: ichinya/lekalo-action@v1
  with:
    command: verify
    locked: true
```

### Permissions and secrets decisions

Default check jobs run `pull_request` with `contents: read`, no privileged secrets, `persist-credentials: false`, and disposable hosted runners. Fork and Dependabot runs never use `pull_request_target` to execute candidate code. Provision public fixtures with locked installs and disabled lifecycle hooks/plugins where supported; native gate execution still requires the accepted checked-in approval policy and confinement. Do not silently fall back to an unconfined process because an OS lacks a backend. Context7 confirmed GitHub's fork-token downgrade and untrusted-checkout guidance. [GitHub secure use](https://docs.github.com/en/actions/reference/security/secure-use), [workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)

Annotations and job summaries need no PAT. SARIF **file generation** is permission-independent. SARIF **upload** is an explicit workflow choice: branch upload normally requires `security-events: write` and supported repository code-scanning entitlement; GitHub documents a `pull_request` upload exception, including Dependabot. Therefore do not assert that all fork SARIF uploads are impossible. Default fork execution still gets no privileged secrets; qualify the ordinary PR upload path in a test repository, otherwise retain SARIF as an artifact plus annotations with an explicit publication-unavailable outcome. Never solve a 403 by granting a PAT to candidate code. [SARIF upload permissions](https://docs.github.com/en/code-security/how-tos/find-and-fix-code-vulnerabilities/integrate-with-existing-tools/upload-sarif-file), [read-only PR exception](https://docs.github.com/en/code-security/reference/code-scanning/troubleshoot-analysis-errors/resource-not-accessible)

Decisions needing maintainer ownership: creating/releasing the action repository and `v1` tag; exact release/tool/action pins; code-scanning permissions/entitlement and whether upload is required; approved CI export policy and retention; AIFHub schema/destination/auth; provider/native/DB qualification environments; required vs optional target capabilities; readiness compatibility rollout. Research resolves no credentials or repository settings. Core/native reports must work without an external provider token; any privileged integration job belongs behind a trusted event/environment, separate from fork execution and with its result clearly scoped to what was tested.

## ci.yml dogfooding plan

Retain existing job coverage and job identities initially. Do not replace hundreds of script gates with an action call that only validates a fixture. Introduce reporting incrementally in this order:

1. Add report-schema/golden/security gates to `contracts` using pinned Ajv. Keep `--base HEAD^` version check and add an explicit PR merge-base contract comparison when qualifying the entire candidate delta; checkout depth 2 alone does not guarantee the semantic base exists.
2. Add a small shared **new** `scripts/lib/ci-gate-report.mjs` opt-in reporter for existing harnesses. A gate observation is written in `finally` from structured assertions, with bounded safe codes; raw stacks never become uploaded artifacts. Top-level bootstrap crashes are captured by a **new** `scripts/run-ci-gate.mjs` argv supervisor with fixed script allow-list and deadlines. A negative control that the harness correctly detects is a passed harness assertion; its specimen output is separately labeled. Rust compile/fmt/Clippy jobs get gate-level observations from exit plus safe classified diagnostics, not a pretend scenario record.
3. Build the CLI once per required OS as currently, record binary hash/version, and run report smoke/golden tests on existing copied fixtures. Capture failing scenarios/native observations before temporary-tree cleanup; feed them through the same decoder as verify. Extend `build-test` to write per-suite JUnit/JSON and one job summary. Preserve Linux AppArmor cleanup; use bounded finalization that runs after failures and cancellation when possible.
4. Add job-level deadlines to contracts/fmt/clippy/msrv/build-test, retain narrower process deadlines, and add concurrency grouping by workflow and PR number/ref with `cancel-in-progress: true`. Keep matrix fail-fast false for independent OS/target evidence. A timed-out job cannot be reported as an optional skip.
5. Upload only finalized report allow-lists after analysis failure as well as success, with bounded upload timeout and `if-no-files-found: error` when the report is required and analysis actually started. Use unique immutable names such as `lekalo-<job>-<os>-<target>-<run-attempt>`. Check cancelled-before-start separately from missing-required-report. GitHub matrix artifacts must have distinct names; summaries are appended to the job environment file. [Artifact workflow guidance](https://docs.github.com/en/actions/tutorials/store-and-share-data), [concurrency guidance](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)
6. Add explicit SARIF upload only to qualified jobs, with unique stable category per target/OS/check scope so matrix legs do not replace each other. Validate schema and safe paths before upload. For jobs lacking upload permission use annotations and downloadable SARIF; record publication status. A publisher with write permissions must not execute candidate code or trust arbitrary artifacts as commands.
7. Cache dependency downloads/build products only after exact identity capture: OS/arch, full Rust/Node/PHP/Composer versions, CLI binary/version, Cargo/npm/Composer lock hashes, adapter manifest/bundle hashes, report/policy schema hashes, resolved profile hash, and relevant input hashes for derived caches. No broad restore-key fallback for evidence; no history, secrets, raw logs, run receipts, or approval records in caches. Revalidate restored content, keep fork cache writes isolated from privileged release jobs, and never accept a cache hit as check completion.
8. Add three executable example workflows (proposed paths below), then use the released action at a reviewed SHA for consumer smoke tests. The core job tests the just-built CLI at the candidate commit; a released binary smoke test is a separate compatibility check. Implemented example YAML must have real immutable action SHAs, not the symbolic placeholders used in this research.

| Example to add | Concrete fixture/workflow content | Required negative control |
|---|---|---|
| `examples/ci/core.yml` | Candidate-built CLI; existing `tests/fixtures/orchestration/project` plus validation/drift fixtures; validate, check-only lock compatibility, generate-check, verify/readiness; no source generation on checked tree. Existing core Rust/script gates remain mandatory. | Invalid source with an inline annotation, drift exit 1, blocked readiness nonzero; clean snapshot unchanged. |
| `examples/ci/node-consumer.yml` | Node runtime pinned exactly; committed npm lock and adapter bundle checked; actual Node scenario suite from the shared Planner corpus, native gate policy, scenario evidence then verify; JSON/JUnit/SARIF/summary artifacts. | Known failing scenario visible in JUnit and nonzero job; supported-vs-unsupported concurrency remains explicit. |
| `examples/ci/laravel-fixture.yml` | `tests/fixtures/php-laravel/planner` committed Composer lock, PHP/extensions provisioned explicitly, Laratesto scenario runner and native Composer gates, actual receipts then verify. Preserve real-toolchain separation for Mago and storage services. | Shared lying-port assertion failure, broken bootstrap classified as boot/infrastructure according to actual source, and missing required PHP/tool/DB failure. |

Workflow examples execute tests in disposable copies as their existing harnesses do; fixture provisioning is distinct from `generate --check` and does not excuse writes by that command. Consumer expected locks/baselines are checked in ahead of time. No CI repair step creates or refreshes them.

### All requested CI checks: implementation and tests

| Issue check | Existing locus and implementation delta | Required verification |
|---|---|---|
| Model/schema validation | `validator/`, loader/IR, validate CLI; new report adapter; keep existing Ajv gates | Valid/invalid/warning/strict cases with same domain exit and format-independent findings. |
| Format check | Existing `fmt` job (`cargo fmt --all -- --check`); target-native format gates via `native_gate/` | A deliberately misformatted fixture fails without formatter writes. No general `lekalo fmt` command is assumed. |
| Lock/profile compatibility | `lockfile/verify.rs`, `lockfile/resolution.rs`, `target_profile/`, `orchestration::locked_check` | Missing/stale/tampered lock and incompatible resolved profile, exact digest output, no auto-update. |
| Generated artifact drift | `artifacts/check.rs`, `crates/lekalo-cli/tests/generate.rs` | Stale/manual/missing/orphan/report-only lifecycle table and filesystem snapshot equality. |
| Adapter conformance/native gates | `adapter_conformance/`, `native_gate/`; replace verify's placeholder only with validated approved native receipt integration | Failing catalog row, boot/missing/incompatible/timeout/security native outcomes; no unsandboxed fallback. |
| Scenario coverage | `scenario_evidence.rs`, `orchestration/verify.rs`, Node/PHP reporters | Expected vs observed inventory, assertion-level JUnit, missing/duplicate/stale/profile/test-fingerprint/run-occurrence rejection; no empty-suite pass. |
| Semantic diff vs base | `diff/`, CLI `run_diff`, `git_input.rs`, new base materialization at CLI/action boundary | Resolve actual trusted base SHA once; read its model in a separate bounded snapshot without changing checkout; missing/shallow base fails explicitly; both revisions bound. |
| Breaking/security/migration policy | Diff profile verdicts, authorization/classification/dataflow/storage migration owners; new `ci_report/policy.rs` adapter | Successful diff containing forbidden break yields policy denial; stale/missing migration evidence and security findings cannot be waived as unavailable. No migration apply in CI. |
| Traceability/readiness | `trace/`, `requirements/`, `doctor/`, verify trace summary; typed completeness/required-policy bridge | Missing/stale trace/requirements evidence and required blocked/unknown checks fail; allowed optional HLV absence degrades visibly. |
| Expiring waivers | `error_contract` coverage waivers already have optional `expires`; NFR evidence has `expiresOn`; classification grants have `expiresAt`; new policy evaluation retains distinct kinds | Fixed explicit `asOf`; before/on/after expiry, missing expiry under require-expiry, warning window, owner/scope/ref/revision mismatch, invalid date; never extend expiry automatically. Define date waivers valid through expiry date (expired when `asOf > expires`), timestamp grants retain their owner's exact boundary rule. |

Date-validity checks alone are insufficient: error-contract constructors currently validate expiry shape, not a unified CI time policy. Waivers must name the exact check/scope and current revisions with reviewer/owner evidence; an untrusted PR cannot introduce its own waiver to disable a protected gate. `asOf` comes from the trusted workflow's recorded evaluation date; reproductions pass that same date explicitly. Do not conflate expiring NFR observations or declassification grants with approval to ignore a failing test.

## Test plan per acceptance criterion

All tests below are future implementation work. Research validation only checks this document and repository state; no runtime acceptance, actual GitHub annotation, action release, or provider qualification is claimed.

| Acceptance criterion from #103 | Implementation loci | Evidence required to close it |
|---|---|---|
| A1: diagnostics inline through SARIF/annotations | New `ci_report/sarif.rs`, CLI `report_output.rs`, action annotation formatter, workflow upload step | `crates/lekalo-core/tests/ci_report.rs` (new): rule/level/location/related-location/Unicode/CRLF tests; `scripts/test-ci-report-contracts.mjs` validates SARIF against a pinned official schema plus Lekalo property schema; hostile paths/command-injection tests. In a dedicated GitHub acceptance repository, introduce a located diagnostic on a changed line and verify actual inline annotation and code-scanning ingestion, including a fork fallback. Archive run/commit proof, not just HTTP success. |
| A2: scenario failures in JUnit | Scenario record decoder, verify evidence retention, new JUnit renderer, Node/PHP harness opt-in exports | Extend `scripts/test-node-scenario-tests.mjs`, `test-php-laravel-scenario-tests.mjs`, `test-php-laravel-parity.mjs`; run both real backends, parse XML, assert the shared mutation is a `<failure>`, unsupported cases are explicit, infrastructure is `<error>`, missing expected evidence fails, totals match. Existing conformance JUnit goldens stay unchanged. |
| A3: required failure nonzero; optional unavailable policy-driven | `result.rs`, `ci_report/policy.rs`, readiness/verify/native adapters, action finalizer | New `crates/lekalo-cli/tests/ci_report.rs`: all six domain statuses on correct streams, required/optional and strict/default table, mixed precedence in every order, source vs effective result preservation, warning severity alone never changes exit, genuine optional assertion failure still fails, output/upload errors cannot mask failure, cancellation/panic never green. Update `crates/lekalo-cli/tests/doctor.rs` for chosen readiness rollout. |
| A4: generate-check drift without writes | Existing `artifacts/check.rs`, `crates/lekalo-cli/tests/generate.rs`, new output path confinement | Extend current recursive size/hash snapshots to metadata where stable; compare clean, drift, early refusal, links/junctions, locked failures, output failure and cancellation. Assert zero adapter spawns; deny/report attempted writes to source/model/lock/baseline/history/cache. With reporting, only the explicitly separate report directory may change. |
| A5: fork/untrusted PR safe by default | Action argv/install/output/cancel handling, `.github/workflows/ci.yml`, new workflow fixture checker | New `scripts/test-ci-workflow-policy.mjs` plus action tests: no secret/PAT/id-token forwarding, no privileged untrusted checkout, no persisted checkout credentials, no shell interpolation, trusted policy cannot be weakened, no automatic repair commands, cache poisoning and artifact-path attacks rejected. Execute fork and Dependabot test workflows; validate actual permission-dependent publication behavior. |
| A6: exact git/model/lock/profile revision binding | New provenance adapter, existing `doctor_git.rs`/`git_input.rs`, model/IR/lock/profile owners, bundle manifest | Pin positive fixtures; mutate each input independently, including profile inheritance, binary/adapter bytes, dirty/untracked inputs, policy, and base SHA; fingerprint changes or evidence is rejected. Missing values stay unknown. Reject pre/post input races, stale scenario/native records, and cross-commit/matrix misbinding. Re-render identical captured inputs on all three OSes and compare deterministic bytes (matrix identity differences are intentional). |
| A7: core, Node consumer, Laravel fixture workflows | Three proposed `examples/ci/*.yml`, action repository tests, core workflow smoke jobs | Parse/lint examples; run each positive and negative control with actual pinned dependencies. Collect job conclusion, CLI exit, JSON/JUnit/SARIF/Markdown and bundle digests; no expected-failure fixture may produce a green consumer gate. Document unsupported OS/backend legs as unavailable, not qualified. |

Additional per-format coverage: JSON closed-union/unknown-version/extra-member/duplicate/count-coherence rejection; JUnit valid UTF-8/XML and deterministic grouping; SARIF registry identity/fingerprint/dedup/import; Markdown hostile links/HTML/truncation; AIFHub exact Extension schema conversion round trip and unsupported-version refusal; bundle allow-list/hash/size/atomicity/missing-member tests. Use Rust 1.80.0-compatible implementation/dependencies and retain Node 18 contract-gate compatibility.

### Requirements traceability

| Requirement | Implementation owner | Test evidence |
|---|---|---|
| R1 deterministic exit policy | `result.rs`, new policy evaluator, typed command finalizer | A3 table/permutation tests and legacy stream goldens. |
| R2 repository-relative safe annotations | `diagnostics/`, new SARIF/location adapter and action formatter | A1 containment, monorepo prefix, Unicode, encoded traversal, junction, no-location cases. |
| R3 logs/artifacts redact secrets | Existing diagnostic/provider allow-lists, privacy evaluator/leak scanner, new safe gate reporter/action output capture | Canary tokens/PII/URL/absolute paths/argv/newline commands across all six formats and failure logs; denied export produces no bytes. |
| R4 fork PRs get no privileged secrets | Minimal-permission workflow/action and isolated integration jobs | A5 real fork/Dependabot runs and workflow policy fixtures. |
| R5 exact version/hash cache keys | Action/tool provisioning and workflow cache configuration | Changing any lock/profile/bundle/tool/OS/arch/policy pin misses or invalidates cache; fork content cannot seed trusted evidence. |
| R6 no automatic baseline/lock updates | Action command allow-list; artifact/lock services remain check-only | A4 snapshots plus A5 rejected command/input cases; scan/index mutation excluded from default chain. |
| R7 cancellation and timeouts | Existing transport cancellation/deadlines, new action process-tree supervisor, job timeouts/concurrency | Cross-OS hanging parent/descendant fixtures, cancelled-before-start/during-run/while-writing, cleanup bound, no orphan process/no stale success. |
| R8 provider infrastructure distinguishable | Provider/native/scenario source vocabulary, new typed failure adapter | A2/A3 infrastructure vs assertion vs incompatible vs security tests; transport success with invalid content is failure. |
| R9 targets/OS matrix support | Existing matrices plus target/profile-qualified report paths/categories | A6/A7 unique artifact identities, Windows path tests, matrix merge mismatch rejection, unavailable confinement reflected in policy. |

### Implementation sequence and validation commands

1. Agree readiness compatibility, optional policy defaults, output authority/export ownership, and external AIF contract pin. Publish report/policy/observation/bundle schemas at R with fixtures and strict Rust/Ajv parity tests.
2. Extract typed report inputs without changing domain outputs; preserve verify receipts on failure; add exact provenance and source/evidence binding checks. Wire policy evaluation and the readiness gate deliberately.
3. Add pure renderers, safe report writer, and offline render/bundle commands; retain legacy conformance JUnit. Implement script observation bridge, case identity/coverage handling, and native receipt integration.
4. Add safe action in its own repository, then update core CI and all three examples; run external integration acceptance only with maintainer-approved publication permissions.

Focused future commands: `cargo test --locked -p lekalo-core --test ci_report`, `cargo test --locked -p lekalo-cli --test ci_report`, existing `--test diagnostics`, `--test adapter_conformance`, CLI `--test adapter_test`, `--test generate`, `--test generate_orchestrate`, `--test doctor`, `--test diff`, and existing privacy/authority/script gates affected by new artifact admission. Add `node scripts/test-ci-report-contracts.mjs` and `node scripts/test-ci-workflow-policy.mjs`; run real Node/PHP scenario suites after their explicit provisioning. Check `node scripts/check-contract-versions.mjs --base <actual-base>` plus owner schema/hash gates, `cargo fmt --all -- --check`, and workspace Clippy/MSRV. These proposed new test files do not yet exist.

## Risks

- **Behavior compatibility:** readiness's current exit-0 contract is deliberate and tested. Changing only its formatter would leave the defect; silently changing doctor/status too would expand scope. Settle default-gate versus transitional `--check` explicitly.
- **Evidence loss and false completion:** verify's early/aggregate returns discard component detail; native gates are still a placeholder in verify; scenario records can predate the current invocation. Require typed complete/incomplete/unknown evidence and exact custody before emitting success.
- **Different meaning of statuses:** `valid` envelope, doctor `ready`, native `passed`, and a successful renderer are different facts. Preserve originals and give CI evaluation one documented owner.
- **Paths and confidentiality:** source diagnostics can be safe yet still reveal proprietary symbol/path names. Safe encoding/redaction is not export permission. Native case-sensitive paths may exceed current diagnostic grammar; do not fake source positions to satisfy inline acceptance.
- **Publication authorization:** the accepted privacy/authority references in `privacy/refs.rs` are frozen. New report artifacts require reviewed successor admission; no automatic owner alias or generic `.lekalo` export. Local history stays ineligible.
- **External contract dependency:** the actual AIFHub `aif-gate-result` schema was not available in this checkout; cross-repository conversion and its tests remain explicit follow-up work, not an invented standard.
- **Provider/OS qualification:** Linux confinement prerequisites and unavailable Windows/macOS backends cannot be solved by marking a row passed. Matrix policy must state which legs are required and provision genuine runtime gates separately from fake/planner tests.
- **Platform publication limits:** GitHub code scanning eligibility, permissions, result limits, and PR-diff placement vary; downloadable files and annotations must remain usable without secrets. Recheck action/tool pins at implementation time; current CI's major tags are not immutable pins.
- **Reproducibility versus occurrence:** deterministic content hashes prove byte identity, not execution authenticity or freshness. Bind trusted run occurrence separately, reject stale inputs, and treat fork-produced artifacts as untrusted data even with valid checksums.
- **Scope:** this commit delivers a design and implementation/test map only. No action repository/tag, runtime reports, workflow changes, upload permission, secret, lock/baseline update, or production result is created.
