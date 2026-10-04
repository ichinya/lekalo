# Issue #76 — Independent review round 1 (devin)

Reviewer: devin (independent verification; not the author)
Branch: `ichinya/m7-issue-76` @ `fda97b81` (worktree `m7-issue-76`)
Base: `a56ee578` (post-merge M7 with #34/#103/#75)
Inputs audited: `docs/m7/issue-76-research.md` (`7bfdb79f`),
`docs/m7/issue-76-implementation.md`, commits `d996d26f`, `2868a3b2`, `fda97b81`.

## Verdict

**ACCEPT** — all acceptance criteria independently reproduced; no findings.

## What was verified (evidence, not the author's claims)

### Gates (reproduced on this checkout, Ajv 8.17.1 via `LEKALO_AJV_NODE_PATH`)

All five new family gates pass with live producers:

- `test-ai-lint-report-contracts.mjs` → `{"ok":true,"family":"ai-lint-report","schema":"0.6.4","live":true}`
- `test-ai-lint-evidence-contracts.mjs` → ok, live (executes the shipped
  Node + PHP collector bundles and the production CLI join)
- `test-ai-lint-config-contracts.mjs` → ok, live
- `test-ai-lint-waivers-contracts.mjs` → ok, live
- `test-ai-lint-comparison-contracts.mjs` → ok, live

`check-contract-versions.mjs --base a56ee578` → `{"ok":true,"product":"0.6.4","contractArtifacts":109}` (base 104 + 5 new families).

### Registry successor — independently diffed

`diagnostic-registry.v0.6.3.json` → `v0.6.4.json`: **459 → 476 entries,
zero predecessor entries changed** (byte-compared each entry). Exactly the
17 documented rules added: 6 `ai-lint.*`, 3 `ambiguity.*`, 7 `hidden.*`
(HIDDEN-007/008 deliberately unallocated per impl doc), 1 `indirection.*`.

### Live binary probes (built `target/debug/lekalo.exe` on this worktree)

1. `lekalo ai-lint --help` → "Optional evidence-bound AI readability
   analysis" — correct text, no copy-paste defect (checked explicitly
   after the #77 finding).
2. `--json ai-lint --all` on `tests/fixtures/ai-lint/model` → exit 0,
   `status:"valid"`, 0 findings, per-rule coverage entries reporting
   `unknown` states with `target-or-threshold-evidence-required` — the
   honest no-evidence-no-claims behavior.
3. `--json ai-lint --symbol planner.focus_task --evidence <foreign golden>`
   → exit 1, `status:"invalid"`, `ai-lint.input-invalid` — pinned evidence
   from a different normalized model is refused rather than silently
   joined. Evidence binding works as designed (matching pins are produced
   inside the gates via the real collector pipeline).
4. `--json ai-lint --all --check` → exit 0, `status:"valid"`,
   `ai-lint.coverage-incomplete` diagnostic, report retained — advisory by
   default; required-coverage denial is config-gated (exercised in the
   evidence/config gates' negative cases).

### Structure and hygiene

- Core is a pure analysis service (`crates/lekalo-core/src/ai_lint/`:
  mod/input/wire/collect/depth/compare/diagnostic) — takes Model/IR/graph
  facts + admitted closed evidence; language/framework recognition stays
  adapter-side (`adapters/node-typescript/src/ai-lint.mjs`,
  `adapters/php-laravel/src/ai-lint.php`).
- `cargo fmt --all -- --check` clean; `git diff --check a56ee578..HEAD`
  0 issues; worktree clean; only the 4 issue commits on the branch.
- `cargo test -p lekalo-core --locked --lib` → **1049 passed, 0 failed**
  (includes all ai_lint module tests).
- Protocol negotiation is additive: old-version Describe omits lint
  members; the gates' frozen v0.3.2 protocol schema remains byte-stable
  (checked via contract-versions predecessor rules).

### AC spot-mapping (verified through gate/live evidence)

AC1 ambiguity never resolves silently — covered by report/comparison gate
cases + observed refusal path. AC2 hidden effects visible in PHP/Node —
evidence gate runs both shipped collectors. AC3 scoped/expiring waivers —
waivers gate. AC4 symbol+span linking — evidence gate span/fingerprint
checks. AC5 confidence honesty — evidence gate lowers confidence → info
findings; verifiedEffects asserted 0. AC6 revision trend — comparison gate
raw-vs-active deltas. AC7 optional by default — live `--all` advisory path
verified above.

## Residual risk / out of scope

- Collectors declare partial language coverage by design; no
  execution-proof producer exists, so `verifiedEffects` is always 0 —
  documented and enforced, not a defect.
- Windows-local verification; adapter PHP 8.5 / Node 24 lanes are CI's.
- Remote CI not run (branch unpushed) — as with all sibling branches.

## Recommendation

Merge-ready pending second independent review (codex/cline), per M7
convention. No fix round required from this reviewer.
