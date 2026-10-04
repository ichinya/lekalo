# Adoption and ownership modes

Status: **Implemented** bounded adoption/confirmation/conformance; the contributor scan fallback is **Experimental**. Owner: bootstrap/evidence maintainers. [Issue #38](https://github.com/ichinya/lekalo/issues/38), [observed ADR](adr/0031-observed-mode.md), [contracted ADR](adr/0034-contracted-mode.md).

Use [init/adopt](adopt.md) for safe discovery and skeleton writes, [observed mode](observed-mode.md) for derived code evidence, [bindings](bindings.md) for confirmation and [contracted mode](contracted-mode.md) for maintained implementation conformance.

| Mode | Authority and evidence | Writes to implementation |
| --- | --- | --- |
| Observed | Existing code is indexed into derived observations. Origin/confidence, partial coverage and current/stale/unknown remain visible. | Scan/adoption does not rewrite source. |
| Contracted | Canonical semantics bind to maintained native implementation/tests through an owner declaration and conformance checks. | Maintained bodies stay source-owned. |
| Managed | A selected generator capability and admitted ownership plan govern eligible generated artifacts. | Only explicitly admitted outputs can be generated/replaced. |

Modes can coexist per symbol/artifact. They are not a Model `mode` property. Distinguish [authority classes](authority.md) (`canonical`, `derived`, `cached`, `runtime-only`, `direct-evidence`) from artifact lifecycles (`generated`, `scaffolded`, `checked`, `external`, `custom`). A confirmed binding does not grant regeneration authority.

## Contract one module

**Implemented.** In a disposable copy of the synthetic [contracted tutorial variant](../tests/fixtures/docs/contracted-module/README.md), derived from the existing planner corpus:

```sh docs-example=contract-planner
lekalo --json contract update --declaration declarations/initial.json
node --test --test-reporter=tap test/native.test.mjs
lekalo --no-cache --json contract check --module planner
lekalo --json contract attach planner.focus_task --native-test tutorial.focus
lekalo --json contract attach planner.list_tasks --native-test tutorial.list
lekalo --no-cache --json contract check --module planner
```

Expected: update records four bindings and two actual maintained-code tests pass, both **0/stdout**. The first check exits **1/stderr**, `status: invalid`, with exactly two `contracted.coverage-missing` findings for `planner.focus_task` and `planner.list_tasks`: executing a test does not attach its ID. Continue with the two attach commands (**0/stdout**), which record their exact external IDs. The final check returns clean conformance (**0/stdout**). If running interactively with shell fail-fast enabled, treat the first check's expected exit 1 as a deliberate refusal and continue only after verifying those findings.

The tutorial variant omits the original corpus's deliberately unimplemented count query and absent support artifact rather than claiming they passed. The gate preserves maintained source, then mutates a copied bound source and requires fingerprint drift for `planner.focus_task`. Conformance verifies declared shape/effects, fingerprints and coverage presence; the two tests do not prove all domain effects, persistence or scenarios. Follow the greenfield tutorial for broader native runtime proof.

The observed-to-contracted transition is additive: record a scan; explicitly choose/confirm a use-case mapping; preview and confirm promotion into canonical Model; ingest a contracted declaration; attach native tests/gates. Keep the plan IDs and provenance. Inferred observations never become canonical automatically.

Follow [the MySQL brownfield tutorial](tutorial-brownfield-typescript.md) for an isolated observed path and [the greenfield planner tutorial](tutorial-greenfield-planner.md) for Laravel/Vue. Neither requires a real consumer repository. An unavailable adapter or incomplete scan is a documented limitation, not permission to widen read scopes or silently install dependencies.
