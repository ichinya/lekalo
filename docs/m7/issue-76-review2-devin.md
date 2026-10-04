# Issue #76 — Independent review round 2 (devin)

Reviewer: devin (re-verification of fix round 1)
Branch: `ichinya/m7-issue-76` @ `48b838e5` (worktree `m7-issue-76`)
Prior verdicts: devin ACCEPT r1 (`bc21e516`); codex ISSUES r1
(`be0c78a9`, findings R1 + R2)
Fix under review: `48b838e5` `feat(ai-lint): bound evidence and preserve
unavailable actions` + report `docs/m7/issue-76-fix1.md`

## Verdict

**ACCEPT** — both codex findings are resolved with real controls;
zero contract/registry delta; all gates re-run green.

## Finding dispositions

| # | Codex finding | Disposition | Independent evidence |
|---|---------------|-------------|----------------------|
| R1 | Collectors emitted >32 document locations; installed
`--scan-target` admitted what `--evidence` replay refused | **FIXED** |
Collectors (`adapters/node-typescript/src/ai-lint.mjs::collectAiLint`,
`adapters/php-laravel/src/ai-lint.php::lint_collect`) now bound-aware —
retain whole records within the 32-location union, emit partial coverage
+ explicit `document-location-limit` limitation instead of exceeding.
Core `ai_lint/input.rs::admit_evidence` applies the same `validate_tree`
wire bounds on the installed path, so collection and replay share the
bound. Verified: gate `adapterCases` exercises exactly 32/33 cases for
BOTH adapters through BOTH admission paths (gate lib lines ~251-269
assert `locations.length <= 32`, limitation present iff count=33,
identical disposition on replay); core test
`ai_lint::input::tests::protocol_evidence_and_file_replay_share_document_location_bounds`
passes (1/1). |
| R2 | `{state:unknown}`/`{state:unsupported}` effect action → false
high-confidence `hidden.observer-write`/`undeclared-effect` that can
deny CI | **FIXED** | `ai_lint/mod.rs::analyze`: `declared` now requires
`operation.known() && resource.known() && key.known()` — an unavailable
action yields `None`, never `Some(false)`, so no negative-match finding.
New `incomplete_effect` coverage logic emits `CoverageState::Partial` +
`effect-comparison-input-unavailable` for observer/direct rules whose
effect inputs are unavailable. `declared_effect` takes `&str` (known
action) — the `is_some_and` false-on-missing trap is gone. Verified:
gate lib lines ~141-149 iterate mechanism × {known, unknown,
unsupported, withheld} asserting no finding + partial coverage +
limitation for unavailable states. |

## Re-verification record

- All five family gates re-run on this build: `ai-lint-report/evidence/
  config/waivers/comparison` → `{"ok":true,...,"live":true}` each
  (evidence gate executes both shipped collector bundles).
- `check-contract-versions.mjs --base be0c78a9` →
  `{"ok":true,"contractArtifacts":109}` — **zero contract/registry delta
  from review HEAD**: no schema weakened, no registry entry touched
  (fix needed no new diagnostics — `ai-lint.coverage-incomplete` covers
  the uncertainty envelope).
- `cargo build -p lekalo-cli --locked` clean; `cargo fmt --all -- --check`
  clean; `git diff --check` 0 issues; worktree clean.
- Adapter manifest digests regenerated against deterministic bundle
  bytes (fix doc §boundary controls) — the only golden delta is
  `producer.artifactDigest`, appropriate.

## Notes

- The `document-location-limit` path keeps records whole and reports
  partial coverage — honest degradation, matching the evidence-bound
  design. No complete-coverage claim on an exhausted slice.
- PHP reaches the bound faster (method+call spans) — the fix doc's
  boundary table documents 16-method vs 17-method dispositions, both
  gate-covered.

## Recommendation

Merge-ready pending codex re-review of the fix (round 2), per M7
convention.
