# Issue #75 — fix round 3 verification (devin)

Verdict: **ACCEPT**

Scope: codex round-3 finding R3-1 only — `context-budget --all` emitted
`scope.id: "*"`, which the published
`contracts/context-budget-report.schema.v0.6.3.json` rejected under the
`semanticId` pattern, and the gate never exercised the `--all` scope.

Verified against `8804ef1b` on `ichinya/m7-issue-75`
(Node 24.13.0, Windows, Ajv 8.17.1 + ajv-formats at the repo-convention
`LEKALO_AJV_NODE_PATH`).

## Disposition

**R3-1 — fixed.** The report schema now admits the exact `"*"` selector
through a `oneOf` (`semanticId` OR `const "*"`) plus an `allOf`
conditional that requires `scope.kind: "project"` whenever the id is the
wildcard. The `$defs.semanticId` pattern is byte-identical
(`^[A-Za-z0-9_][A-Za-z0-9_.:-]*$`) — symbol/module semantics unchanged,
no wildcard grammar leaked into shared defs. The design choice (wildcard
is a scope selector literal, not a semantic node id) is documented in
the schema description and `docs/m7/issue-75-fix3.md`.

## Live evidence (independent reproduction)

- `context-budget --all --budget 12000` against the planner fixture:
  exit 0, `status: "valid"`, `scope: {"id":"*","kind":"project"}`,
  29 subjects.
- Compiled the published schema with strict Ajv 8.17.1 and validated the
  live `--all` report body: **valid** (pre-fix: invalid at `/scope/id`).
- Negative probes on cloned live reports — all rejected:
  `kind: "module"` + `id: "*"`, `kind: "symbol"` + `id: "*"`,
  `id: "**"`, `id: "*x"` (project kind). The wildcard is the only
  permitted non-semantic selector and only under project scope.
- `node scripts/test-context-budget-contracts.mjs`: `ok: true`,
  `liveChecked: true`, `contextRules: 8`. The gate's new leg runs live
  `--all`, validates the whole report, asserts the project selector, and
  refuses wildcard symbol/module scopes and `"**"` — exactly the
  coverage the finding demanded.
- `cargo test -p lekalo-cli --test context_budget --locked`: 16/16.
- Fix report `docs/m7/issue-75-fix3.md` (142 lines) documents the
  disposition and states the added gate leg reproduced
  `project-schema` at `/scope/id` before the repair — a real
  regression witness, not a tautology.

## Weakening check

No producer change: the commit touches the schema, gate, docs, and fix
report only; `context_budget/mod.rs` is unmodified. The schema change is
a strictly-scoped selector admission gated on `kind: "project"`;
`additionalProperties: false`, `required: ["kind","id"]`, and all other
constraints are unchanged.

Issue #75 has devin ACCEPTs on fix rounds 2 and 3 plus the codex
round-3 review; merge readiness now awaits the codex re-review of this
round or a merge decision per convention.
