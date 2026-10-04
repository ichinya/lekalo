# Issue #35 — Independent review round 1 (devin)

Reviewer: devin (independent verification; not the author)
Branch: `ichinya/m7-issue-35` @ `2868c285` (worktree `m7-issue-35`)
Base: `a56ee578` (post-merge M7 with #34/#103/#75)
Inputs audited: `docs/m7/issue-35-research.md` (`b89b2691`),
`docs/m7/issue-35-implementation.md`, commits `dded99f9` + `2868c285`.

## Verdict

**ACCEPT** — all acceptance criteria independently reproduced; no findings.

## What was verified (evidence, not the author's claims)

### Gates (reproduced on this checkout)

- `node scripts/test-trace-evidence-contracts.mjs` →
  `{"ok":true,"gate":"trace-evidence-contracts","ajv":"8.17.1","checks":48,"synthetic":true}`
- `node scripts/test-trace-assessment-contracts.mjs` →
  `{"ok":true,"gate":"trace-assessment-contracts","ajv":"8.17.1","cases":38,"live":true,"execution":"synthetic receipts; no HLV invocation"}`
- `node scripts/check-contract-versions.mjs --base a56ee578` →
  `{"ok":true,"product":"0.6.4","contractArtifacts":106}` (base 104 + 2 new schemas)
- `cargo fmt --all -- --check` clean; `git status` clean; worktree contains only the
  three issue commits.

### Live binary probes (built `target/debug/lekalo.exe` on this worktree)

1. `lekalo trace assess trace.json --evidence input/ready.json --json`
   → exit 0, `status:"valid"`, `assessment.coverage:"complete"`, 1 chain,
   0 findings. Envelope `{status, assessment}`.
2. `--evidence input/missing-mapping.json`
   → exit 3, `status:"denied"`, diagnostics:
   `trace.bridge-mapping-missing`, `trace.bridge-execution-unverified`,
   `trace.bridge-chain-uncovered`, `trace.bridge-policy-denied`.
   `payload.assessment` retains the full derived report (`coverage:"partial"`,
   1 chain) — denial-retains-report confirmed live, matching the
   `denial-retains-report` gate check.
3. `lekalo trace assess --help` describes the actual command
   ("Assess an explicit neutral mapping and revision-bound provider
   receipts") — no copy-paste defect of the kind found in #77.

### Contract/structure checks

- `contracts/trace-manifest.schema.v0.2.16.json` — **byte-identical** to base
  (`git diff` empty). The neutral manifest contract is genuinely unchanged.
- `trace-validation-evidence.schema.v0.6.4.json` — closed object
  (`additionalProperties:false`, 15 required fields).
- `trace-assessment.schema.v0.6.4.json` — `oneOf` discriminated envelope
  (valid/denied) over 20 `$defs`; both branches exercised by the 38-case gate.
- Registry successor: 459 → 468 entries, nine `trace.bridge-*` rules
  (`LEK-TRACE-001..009`); predecessor preservation is enforced inside the
  assessment gate cases.
- Provider: 11 operations advertised incl. `trace.assess`; new schema pins
  covered by contract-versions (106 artifacts).
- CLI layer: `tests/trace_assessment.rs` 5/5, `tests/trace.rs` 9/9,
  `tests/requirements.rs` trace 4/4, `lekalo-core` trace-filtered 28/28,
  `generate_orchestrate` trace 1/1 — all green on this build.

### Boundary claims (checked honestly)

- "No HLV invocation" is real: the gate runs with synthetic receipts and the
  assessment service never shells out; the denied path distinguishes
  missing-mapping from execution-unverified rather than conflating them.
- HLV/AIFHub acceptance remains explicitly unclaimed — the implementation
  report states external integration is out of scope; nothing in the branch
  asserts a real provider run.
- Synthetic receipts are labelled synthetic in gate output.

### Minor notes (non-blocking)

- `git diff --check origin/main...HEAD` flags 2 blank-line-at-EOF warnings in
  `docs/m7/issue-103-review{,2}-cline.md` — both files predate this branch
  (merged earlier work), none of #35's commits touch them. Not a #35 defect.

## Residual risk / out of scope

- No real HLV executable run — by design; the neutral bridge is the
  deliverable, external binding is downstream work.
- Windows-local verification; Linux/macOS confinement is CI's lane.

## Recommendation

Merge-ready pending second independent review (codex/cline), per M7
convention. No fix round required from this reviewer.
