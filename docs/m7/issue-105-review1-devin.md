# Issue #105 — implementation review round 1 (devin)

Verdict: **ACCEPT**

Scope: the complete implementation delta `a56ee578..967436a7` on
`ichinya/m7-issue-105` — 63 files, +5.7k/-1.6k, no crates/contracts
changes. Verified live on Node 24.13.0, Windows, Ajv 8.17.1.

## Verified claims

- **13/13 required pages exist.** The eight previously missing
  (`architecture`, `project-layout`, `adoption`, `security`,
  `integrations`, `tutorial-greenfield-planner`,
  `tutorial-brownfield-typescript`, `roadmap`) are present; README
  opens with the required positioning: "one application model for
  multiple targets".
- **`docs/documentation-owners.json` has exactly 318 records.**
  `test-docs-ownership.mjs` passes in BOTH modes — `--static` and
  default live-help (surfaces 318, requiredDocs 13).
- **Example replay is real.** `test-docs-examples.mjs` portable lane:
  9 examples, 24 literal-argv commands, 18 controls,
  `sourcePreserved: true`. `--static` mode: 12 examples, 2 setup
  blocks, 4 controls. The gate rejects unregistered fences and
  executes producers in disposable copies — not markdown-lint
  theatre.
- **CI wiring is honest.** The planner lane step sits after the
  existing #56 vendor-provisioning step in the same job and appends
  `lekalo-client-sdk-vue` to NODE_PATH for the TS/Vue pins; the
  dedicated `mysql-docs` job pins `mysql:8.4.5@sha256:679e…` with a
  health-gated service and `npm ci` from the committed fixture lock.
  Local `--lane planner` fails `planner-vendor-missing` — the honest
  required-lane failure the design promises instead of a passing
  skip.
- **Fixture/provenance discipline.** `test-fixture-provenance.mjs`:
  69 synthetic families, 0 evidence-backed — the docs + pilot
  families registered as claimed. Golden catalog (21 cases) and
  hygiene (259 files, 6 controls) unaffected.
- **No weakening.** `git diff --check` clean; no schema, gate or
  fixture edits outside the additive doc/replay surface;
  `test-contract-versions` 6/6.
- **Residual honesty.** The report documents two pre-existing
  product defects it refused to hide. I reproduced the first:
  `context --budget 1` on the planner fixture exits 0 with
  `sections: {}`, violating the capsule schema's `minProperties: 1`
  (contracts/context-capsule.schema.v0.2.16.json:263). Out of scope
  for this docs issue but a real inherited producer/schema defect —
  worth its own issue.

## Not verified here (as documented)

Remote CI runs (unpushed), the MySQL service lane (requires the
provisioned fixture npm tree + live MySQL), and hosted-platform
confinement qualification — the report itself bounds these to CI
evidence.

## Pre-existing defects surfaced (for the tracker)

1. `context --budget 1` → exit-0 `sections: {}` vs schema
   `minProperties: 1` (producer must refuse or emit a section).
2. `contracted.stale-artifact` puts a path value in `symbol`,
   violating the diagnostic semantic-ID grammar (same defect class
   as the #75 wildcard — worth a sweep for non-semantic values in
   semantic-id fields).
