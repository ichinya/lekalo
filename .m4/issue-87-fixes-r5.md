# Issue #87 — Fix Round 5 report

Branch `ichinya/m4-issue-87`, base `5a9a7db5` (r4 tip). Devin r4
verified all five r4 fixes; two tiny defects remained, both fixed here
— one commit per finding, fix-forward only (no resets, no merges, no
tags, no pushes).

Spec: `.m4/spec-fix-r5.md`.

## Per-finding table

| Id | Severity | Resolution commit | Evidence |
|---|---|---|---|
| devin r4 F-1 — `normalize_as_of` panics on multi-byte input | nit | `f06acf3` | `normalize_as_of` (main.rs) now gates on `raw.is_ascii()` before any byte-offset slicing (the earlier code sliced `&raw[11..13]` etc. after only the 20-byte length + `T`/`Z` check, so `2026-01-01T€xxxxxZ` — 20 bytes with `€` spanning offsets 11–13 — panicked on a non-char boundary instead of refusing). Non-ASCII is never a valid spelling, so the gate refuses the whole class for both the 20-byte and 10-byte branches (the latter also feeds `IsoDate::parse`, itself byte-slicing). Live probe: `--as-of '2026-01-01T€xxxxxZ'` → `LEK-CLI-001 cli.usage`, exit 1 — a clean refusal, no panic. e2e §6f battery extended with `2026-01-01T€xxxxxZ` (exactly 20 bytes, panic shape) and `2026-01-é` (10-byte multi-byte branch); all surfaces deny. |
| devin r4 F-2 — report schema does not compile (dangling `$ref`) | minor | `c0d636c0` | `data-flow-report.schema.v0.4.0.json` defined no `$defs/boundedToken` for the `generatedBy` ref — Ajv could not compile the schema, so no consumer could validate a real report. Fixed by adding the member byte-identical to the three sibling schemas (`classification-policy`, `data-classification`, `nfr`: `minLength 1 / maxLength 64 / ^[a-zA-Z0-9][a-zA-Z0-9._:/-]*$`), which matches the wire (`generated_by` non-empty ≤ 64; `GENERATED_BY = "lekalo-core"` satisfies the pattern). **Gate wired** into `test-classification-contracts.mjs` §4c: (a) Ajv compile of the published schema (hard fail on error); (b) dangling local-`$ref` scan across **all 72** `contracts/*.json` — every `#/…` ref must resolve in its own document (scan found exactly one dangling ref repo-wide before the fix: this one); (c) live validation — when the `lekalo` binary exists, a real `dataflow report --json` on the valid planner fixture must validate against the compiled schema (binary-absent is recorded explicitly as `reportLiveSkipped: "binary-missing"` because CI's contract step precedes the build step; a present binary whose output fails validation is a hard failure). Gate output now reports `reportSchemaCompiled: true, reportLiveValidated: true`. Negative check: deleting `$defs/boundedToken` from the schema in memory reproduces the compile failure ("can't resolve reference #/$defs/boundedToken"), so the class cannot regress silently. Sibling-schema scan confirmed no other dangling refs anywhere. |

## Verification

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p lekalo-core --all-targets --locked -- -D warnings` | clean (0 warnings) |
| `cargo test -p lekalo-core --lib --locked` | **554 passed; 2 failed; 2 ignored** — only the known LPAC spawn pair (documented environmental at merge-base in every round) |
| `node scripts/test-classification-cli.mjs` | `{ok:true, fixtures:{valid:1, declassified:1, invalid:7}, sentinelScanned:true}` — includes the new multi-byte as-of probes |
| `NODE_PATH=%TEMP%\lekalo-ajv-8.17.1\node_modules node scripts/test-classification-contracts.mjs` | `{ok:true, ajv:"8.17.1", registryEntries:359, predecessorEntries:321, classificationRules:16, dataflowRules:9, reportSchemaCompiled:true, reportLiveValidated:true, reportLiveSkipped:null}` |
| `node scripts/check-contract-versions.mjs` | `{ok:true, product:"0.4.0", contractArtifacts:68, base:"HEAD"}` |
| `git status` | clean (scratch logs from earlier rounds absent) |

## Commits (round 5)

| Commit | Finding |
|---|---|
| `f06acf3` | F-1 ASCII gate in `normalize_as_of` + multi-byte e2e probes |
| `c0d636c0` | F-2 `boundedToken` definition + schema-compile / dangling-ref / live-validate contract gate |

## Residual notes

1. In CI the contract step runs before `cargo build`, so the
   live-report validation records `reportLiveSkipped: "binary-missing"`
   there; the compile + dangling-ref halves — the actual regression
   class — always run, and the local/CI build step path exercises the
   live half wherever the binary is present.
2. The Windows LPAC spawn class (error 1450) remains environmental on
   this host across all rounds; CI is the gate for those suites.
