# Issue #87 — Fix Round 4 report

Branch `ichinya/m4-issue-87`, base `14e3d2de` (r3 tip). Split verdict
accepted: cline r3 ACCEPT; devin r3 CHANGES REQUIRED with five small
findings, all fixed here — one commit per finding, fix-forward only
(no resets, no merges, no tags, no pushes).

Spec: `.m4/spec-fix-r4.md`; review: `.m4/issue-87-review-devin-r3.md`.

## Per-finding table

| Id | Severity | Resolution commit | Evidence |
|---|---|---|---|
| devin r3 F-1 — `--as-of`/`LEKALO_AS_OF` unvalidated (fail-open) | minor | `9959329` | `normalize_as_of`/`resolve_as_of` at the CLI boundary (main.rs): accepts the wire shape `YYYY-MM-DDTHH:MM:SSZ` (date part via `nfr::IsoDate::parse`, time part digit/range-checked) and bare `YYYY-MM-DD` normalized to midnight UTC; anything else is `cli.usage` (LEK-CLI-001, exit 1) on `classification validate`, `classification inspect`, and `dataflow report` — malformed input now **denies, never passes**. Live probes on `expired-public-grant`: `--as-of '!'` → exit 1 usage; `--as-of ''` → exit 1; `LEKALO_AS_OF=garbage` → exit 1; `--as-of 2019-01-01` → normalized, grants live, `classification valid` exit 0; `--as-of 2019-01-01T00:00:00Z` → exit 0. e2e §6f pins the battery: 5 malformed spellings × 3 surfaces all deny, env garbage denies, bare date and wire shape validate. |
| devin r3 F-2 — report `modelRef.digest` = sha256("0.2.16") | minor | `d85da50` | `compile_digests` now takes the canonical Model bytes `run_report` already receives and emits `sha256(canonical model bytes)` — byte-for-byte the pin `validate_custody` verifies; `irRef` spelling (canonical IR JSON) unchanged. Regression `the_report_pins_the_real_custody_model_digest` (tests/dataflow.rs): real pins satisfy custody, and the derived report's `modelRef.digest`/`irRef.digest` equal `sha256(canonical model bytes)` / `sha256(canonical IR bytes)` — the old spelling asserted would fail. |
| devin r3 F-3 — `classification.self-approved` unreachable by grammar | nit | `5891fd2` | Kept as depth-defense and documented in both places: `validate.rs` check comment and `docs/classification.md` hard rule ("the wire grammar already makes it unreachable on parsed input — an `approvedBy` review reference forbids `@`, a grant id requires it — so the rule protects library callers constructing grants in memory"). Regression `review_ref_and_contract_ref_spellings_are_disjoint` (types.rs): a grant-id spelling parses as `ContractRef` but is rejected by `ReviewRef`, and a review spelling vice versa — pinning the disjointness the documentation claims. |
| devin r3 F-4 — report schema `gate.reason` enum is a superset | nit | `db62e80` | `contracts/data-flow-report.schema.v0.4.0.json` drops `low-confidence` and `inputs-incomplete` (the GateReason variants were removed in r3 F-4/R2-5); the enum is now exactly the nine producible reasons. New gate block in `test-classification-contracts.mjs` (§4b) pins the schema enum to that exact list — future drift fails the contract gate. `check-contract-versions.mjs` green (`contractArtifacts:68`), registry/catalog rows unchanged. |
| devin r3 F-5 — suppression path untested + `approved-by` reuses generic id | nit | `690380b` | (a) **Fixture**: `expired-public-grant` now classifies `planner.focus_task` `personal` (both copies), so its expired `personal→public` grant *genuinely applies* to the exposed subject. e2e §6g: default as-of → exit 3 denied with BOTH `dataflow.exposed-private-field:planner.focus_task` and `classification.expired-declassification:planner.focus_task` (non-suppression + fold-in proven together); the same surface with `--as-of 2019-01-01T00:00:00Z` → exit 0 `verdict:"pass"` (a live grant genuinely suppresses). (b) **Diagnostic id**: `classification.malformed-review-ref` (LEK-CLS-016) registered additively (byte-canonical, id-sorted; contract gate count 358→359, family 15→16) and wired into `wire.rs` grant parsing — an `approvedBy` shape failure no longer surfaces as LEK-CLS-001. Regression `a_malformed_review_ref_refuses_under_its_dedicated_id` (validate.rs): `approvedBy: "review@2025-001"` refuses with exactly `classification.malformed-review-ref`. ADR-0043 range updated to `LEK-CLS-001..016`. |

## Verification

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p lekalo-core --all-targets --locked -- -D warnings` | clean (0 warnings) |
| `cargo test -p lekalo-core --lib --locked` | **554 passed; 2 failed; 2 ignored** — only the known LPAC spawn pair (`target_protocol` confinement/conformance, error 1450, documented at merge-base by every round) |
| `cargo test -p lekalo-core --locked --no-fail-fast` | all failures are the documented Windows LPAC `TransportFailed { detail: "spawn" }` class in adapter-launch suites (`adapter_conformance`, `node_typescript_*`, `target_protocol*`); **verified pre-existing at base**: a throwaway worktree at `14e3d2de` reproduces `adapter_conformance` exactly (6 passed / 10 failed), then removed |
| `node scripts/test-classification-cli.mjs` | `{ok:true, fixtures:{valid:1, declassified:1, invalid:7}, sentinelScanned:true}` — now including the §6f as-of battery and §6g suppression pair |
| `NODE_PATH=%TEMP%\lekalo-ajv-8.17.1\node_modules node scripts/test-classification-contracts.mjs` | `{ok:true, ajv:"8.17.1", registryEntries:359, predecessorEntries:321, classificationRules:16, dataflowRules:9}` — includes the new §4b report-schema gate-enum pin |
| `node scripts/check-contract-versions.mjs` | `{ok:true, product:"0.4.0", contractArtifacts:68, base:"HEAD"}` |
| Scratch cleanup | `r3-clippy.log` / `r3-core.log` do not exist at the worktree root; `git status` clean |

## Commits (round 4)

| Commit | Finding |
|---|---|
| `9959329` | F-1 as-of boundary validation + e2e battery |
| `d85da50` | F-2 report model digest == custody pin + regression |
| `5891fd2` | F-3 self-approved documented as depth-defense + grammar pin |
| `db62e80` | F-4 report schema gate-reason enum tightened + gate |
| `690380b` | F-5 genuine suppression fixture + `classification.malformed-review-ref` |

## Residual notes

1. The Windows LPAC spawn class (error 1450) remains environmental on
   this host across all rounds; CI is the gate for those suites.
2. The registry grew by one row within the frozen `0.4.0` successor
   (additive, byte-canonical, id-sorted); every contract gate stays
   green.
