# Issue #75 — fix round 3 finding disposition

Branch: `ichinya/m7-issue-75`. Starting HEAD:
`462ad54b13edd65ab33bb91438fe678481d7b9e4`. Addressed **R3-1** from
[codex round 3](issue-75-review3-codex.md): the live project report used
`scope.id: "*"`, which the published report schema rejected despite a
valid exit-0 envelope.

## Per-finding disposition

| Finding | Disposition | Implementation and verification evidence |
| --- | --- | --- |
| **R3-1** — `--all` emits a project selector rejected by its own schema; the live gate never exercises that scope. | **fixed** | The report schema deliberately permits the exact `"*"` selector only when `scope.kind` is `"project"`. The shared `semanticId` definition is unchanged. The gate now runs live `--all`, validates its entire report, checks the project selector, and refuses wildcard symbol/module selectors and `"**"`. Before the schema repair, the added live leg reproduced exit 1 `project-schema` at `/scope/id`; after repair, the gate passes. An independent live `--all --budget 200` report exits 0, selects 29 subjects, and validates with strict Ajv 8.17.1. |

## Contract choice and documentation

Chose the explicitly permitted scope-local wildcard grammar repair. The
existing producer's `Scope::All` means every definition of the loaded
project, matching the three-selector command semantics and project-wide
examples in [context-budget documentation](../context-budget.md). Here
`scope.id` records the selector; the provenance tuple binds the report to
the measured compilation. A wildcard is a meaningful project-wide selector
even when the graph has no declared project semantic id
(`DependencyGraph::project_id` is optional).

This is the smallest repair preserving that behavior: `scope.id` accepts
either the existing semantic-id grammar or the exact `"*"` constant, with
a conditional requiring `scope.kind: "project"` for the wildcard. It
introduces no wildcard into `$defs.semanticId`, no arbitrary-string
acceptance, and no fabricated project identity or new missing-id refusal.
The schema's stated semantics remain a closed, metadata-only derived
report of one symbol, module or project; the scope description now explains
the project-wide selector exception. The user documentation states the
same rule.

This is an intentional, narrowly scoped expansion of accepted schema
inputs: `{"kind":"project","id":"*"}` becomes valid. It is not a claim
that the schema accepts exactly the old input set. Wildcard symbol/module
selectors remain invalid, and all previously valid semantic scope
identifiers remain valid.

## Golden and wire shape

No producer or wire bytes change, so no existing golden needs regeneration
or replacement under the requested wire-change condition. The mandatory
gate generates the symbol report from the built CLI and compares its
canonical payload with
`tests/fixtures/context-budget/golden/planner.over-budget.json`, retaining
the existing live/golden flow and all its assertions. That equality passes;
the golden and all fixture inputs remain untouched. The added project leg
validates its live output rather than replacing evidence with a manually
edited fixture.

## No-weakening statement

- The deliberate project-selector admission above is the only change to
  schema acceptance. Comparing parsed base/current schemas after removing
  `properties.scope` proves structural equality everywhere else, including
  `$defs.semanticId`, provenance pins, metric states, ledger reason/class
  coupling, bounds and closed object constraints. The other three
  context-budget schemas are unchanged.
- No gate leg, assertion, negative vector or policy threshold is removed
  or relaxed. All 63 previous distinct Node failure reasons remain; five
  new reasons cover project status/schema/selector and selector negatives.
  The gate diff adds 19 lines and removes none. Mandatory binary refusal,
  determinism, privacy, consumed-byte pins, denied payloads, comparison,
  reconciliation and golden equality remain enforced.
- No Rust test is removed, skipped or modified. The requested CLI suite
  still passes 16 tests with zero ignored; the focused core suite passes
  57 with zero ignored.
- Independent schema probes additionally reject empty/null/missing ids,
  missing kind, extra scope properties and wildcard module identifiers
  outside scope. Existing semantic symbol/module/project identifiers pass.
- The contract stays at the current 0.6.3 product generation; the requested
  base-relative version check passes against `origin/ichinya/M7`.

## Verification output — actual local runs

Windows/PowerShell, Node 24.13.0, Cargo 1.98.0, exact Ajv 8.17.1. Node gates
used:

```powershell
$env:NODE_PATH = 'C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
```

The new live gate leg was first run against the unchanged schema:

```text
node scripts/test-context-budget-contracts.mjs
exit 1 (expected reproduction before the repair)
{"ok":false,"reason":"project-schema","detail":[{
  "instancePath":"/scope/id",
  "schemaPath":"#/$defs/semanticId/pattern",
  "keyword":"pattern"
}]}
```

After the scoped schema repair, every requested command passes:

```text
cargo build -p lekalo-cli --locked
  exit 0; Finished dev profile in 0.24s
cargo test -p lekalo-cli --test context_budget --locked
  exit 0; 16 passed; 0 failed; 0 ignored
cargo test -p lekalo-core --lib context_budget --locked
  exit 0; 57 passed; 0 failed; 0 ignored; 957 filtered out
node scripts/test-context-budget-contracts.mjs
  exit 0; {"ok":true,"ajv":"8.17.1","registryEntries":457,
           "contextRules":8,"liveChecked":true,
           "golden":"tests/fixtures/context-budget/golden/planner.over-budget.json"}
node scripts/test-fixture-provenance.mjs
  exit 0; {"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}
node scripts/check-contract-versions.mjs --base origin/ichinya/M7
  exit 0; {"ok":true,"product":"0.6.3","contractArtifacts":99,
           "base":"origin/ichinya/M7"}
cargo fmt --all -- --check
  exit 0; no output
cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings
  exit 0; Finished dev profile in 0.32s
git diff --check
  exit 0
```

Independent live/schema probe:

```text
lekalo --json context-budget --all --budget 200
exit: 0
status: valid
scope: {"id":"*","kind":"project"}
subjects: 29
Ajv schemaValid: true
invalid scope vectors refused: 8
wildcard in a subject's module identifier refused: true
```

These are local runs with the existing Cargo target cache, not a hosted CI
result or a cold cross-platform build. The CI placement after the binary
build remains unchanged. Verification creates no fixture residue or
scratch scripts; the only worktree changes are the report schema, live
gate, user documentation and this fix report. Fix and report are committed
together; no push is performed.
