# Issue #75 — review round 4 (agent: codex)

Reviewer: codex, also the author of fix-round-3 commit `8804ef1b` in the
preceding task. This review records fresh runtime/schema verification
conducted separately from [Devin's round-4 review](issue-75-review4-devin.md).

Scope: **R3-1 only**, from [codex round 3](issue-75-review3-codex.md): the
project-wide selector `scope.id: "*"` failed the published report schema,
and the gate never validated live `--all` output. Reviewed
`8804ef1b9fa84ba8a2e11b2482336f543d2d3212` against its parent `462ad54b`.
Worktree HEAD at verification was `eb4c1ca6eeb210836afdbf508fec012d5ee55801`;
the only subsequent candidate change is Devin's review document. Branch:
`ichinya/m7-issue-75`. Initial worktree status was clean.

## Verdict: ACCEPT

**R3-1 is fixed.** Strict Ajv 8.17.1 validates the real project report.
The exact wildcard is accepted only under project scope; all four
requested negative probes are rejected. The shared semantic-id definition
and every constraint outside the deliberate scope-selector repair are
unchanged. Runtime tracing proves the new gate leg executes the binary;
an in-memory pre-fix-schema control fails specifically at that live leg.
No remaining issue was found within this review's scope.

## Live evidence and reproduction

Windows, Node 24.13.0. Validation loaded exact Ajv 8.17.1 from the
`LEKALO_AJV_NODE_PATH` convention, asserted the package version, and compiled
the actual published Draft 2020-12 schema with
`new Ajv({strict: true, allErrors: true})`.

```powershell
$env:LEKALO_AJV_NODE_PATH = 'C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:NODE_PATH = $env:LEKALO_AJV_NODE_PATH
```

From `tests/fixtures/context-budget/planner`, using this worktree's
`target/debug/lekalo.exe`:

```text
lekalo --json context-budget --all --budget 200
exit: 0
status: valid
scope: {"id":"*","kind":"project"}
subjects: 29
strict Ajv schemaValid: true
```

The same unmodified live report fails the parent commit's schema at
`/scope/id`, keyword `pattern`; it passes the candidate schema. Cloning
that live report and replacing only `scope` produced:

| scope.kind | scope.id | Strict validation |
| --- | --- | --- |
| project | `*` | accepted |
| symbol | `*` | rejected, conditional `const`/`if` |
| module | `*` | rejected, conditional `const`/`if` |
| project | `**` | rejected, `pattern`/`const`/`oneOf` |
| project | `*x` | rejected, `pattern`/`const`/`oneOf` |

Six additional scope probes reject null/empty/missing id, missing kind,
an unknown kind and extra properties. Ordinary semantic symbol/module/
project identifiers pass. A wildcard in a subject's module identifier
still fails, independently proving the exception is local to scope.

The minimal strict-Ajv reproduction in the round-3 review still applies:
running it on this candidate now prints `schemaValid: true`; applying the
four scope replacements above yields false.

## Schema and gate weakening check

`context-budget-report.schema.v0.6.3.json:87-108` preserves the closed
scope object, required `kind`/`id`, and kind enum. The only acceptance
expansion is the exact project wildcard: `scope.id` has
`oneOf [semanticId, const "*"]`, and the wildcard conditional requires
`kind: "project"`. `$defs.semanticId` is unchanged, including the pattern
`^[A-Za-z0-9_][A-Za-z0-9_.:-]*$`.

Reconstructed the expected candidate schema from the parent's parsed
schema by changing only the scope description, id alternatives and
project conditional, then asserted deep equality with the entire actual
candidate schema. Thus there is no other constraint change, including
inside scope. The checked-out schema equals the candidate's Git object.
The commit changes only that schema, the gate, user documentation and
the fix report; producer, other schemas, Rust tests and golden are unchanged.
The selector interpretation is documented consistently with
[context-budget.md](../context-budget.md) and
[fix-round-3 report](issue-75-fix3.md).

The gate delta adds 19 lines and deletes none. All 63 prior distinct
failure reasons remain; five new reasons check project status/schema/
selector and selector negatives. Existing live/golden equality, privacy,
determinism, provenance, comparison, reconciliation and negative checks
remain intact and execute in the passing gate.

## The new gate leg executes the binary

`scripts/test-context-budget-contracts.mjs:199-213` calls the same
`execFileSync` runner used by the other live scopes, then validates its
returned report. There is no conditional skip around this leg.

For independent runtime evidence, wrapped Node's `execFileSync` in memory,
called its original implementation with unchanged arguments/results, and
ran the actual gate. Exactly one project invocation was observed:

```text
executable: lekalo.exe
arguments: --json context-budget --all --budget 12000
cwd: tests/fixtures/context-budget/planner
returned status: valid
returned scope: {"id":"*","kind":"project"}
returned subjects: 29
gate exit: 0
```

Then ran the same gate with only its schema read substituted in memory by
`git show 8804ef1b^:contracts/context-budget-report.schema.v0.6.3.json`.
The real `--all` child still executed and returned 29 subjects; the gate
refused with **exit 1, `project-schema`, `/scope/id`, `pattern`**. This
proves the new leg catches the original defect; `liveChecked: true` is
already set by the earlier symbol run. These controls edit no
source, schema, binary or fixture files.

## Reviewer-run checks and cleanliness

```text
node scripts/test-context-budget-contracts.mjs
  exit 0; {"ok":true,"ajv":"8.17.1","registryEntries":457,
           "contextRules":8,"liveChecked":true}
cargo test -p lekalo-cli --test context_budget --locked
  exit 0; 16 passed; 0 failed; 0 ignored
git diff --check
  exit 0
```

The live-schema probes, exact schema-delta comparison and gate runtime
controls above were also executed in this round. Before authoring this
document, `git status --porcelain=v1 --untracked-files=all` remained empty.
Only this review document is changed and committed; no push is performed.

These results verify the requested R3-1 repair locally on Windows. A
hosted CI run, cold build, full workspace suite and broader issue-75
acceptance were not rerun or claimed.
