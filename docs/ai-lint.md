# AI readability lint (issue #76)

Product `0.6.5` also accepts the reviewed `ai-lint-waivers` successor and emits
neutral facts with `--waiver-facts`. See [scoped governance waivers](waivers.md)
for justified expiry, profile eligibility and machine-readable audit evidence.
The predecessor report and waiver contracts retain their existing versions.

`lekalo ai-lint` reports optional, evidence-bound readability findings for
AI-assisted review. It is advisory by default: advisory runs emit findings with
exit 0, and a report never changes source, updates a baseline, or writes files.
Normal validate/observe/inspect flows do not enable lint. Capture stdout
explicitly when a reviewed report is needed.

```sh
lekalo --json ai-lint --module planner --project tests/fixtures/ai-lint/planner
lekalo --json ai-lint --symbol planner.focus_task --project PROJECT
lekalo --json ai-lint --all --project PROJECT
lekalo --json ai-lint --module planner --config lint-config.json --lint-profile ci --check
lekalo --json ai-lint --module planner --scan-target node-typescript --source-file src/events.ts
```

Exactly one selection is required: `--symbol`, `--module`, or `--all`.
`--lint-profile` selects the closed `off`/`advisory`/`ci` profile set
(default `advisory`). Explicit `--check` applies the configured gate and returns
denied with exit 3 while retaining the report. Profiles are independent of
mandatory semantic validation, and mandatory diagnostic families cannot be
waived by this service.

## Evidence and collectors

Lint findings must bind admitted evidence. `--evidence` accepts a separately
produced closed evidence document; `--scan-target` plus `--source-file` invokes
an installed, read-only target collector (Node/TypeScript or PHP/Laravel).
Collectors never execute application imports, scripts, or project files, and
they refuse or degrade honestly at the published document bounds rather than
emitting unbounded locations. `--transitions`, `--trace`, and `--artifacts`
attach their respective closed inputs only for the corresponding joins.

Unknown, unsupported, or withheld evidence states stay unknown: an unavailable
input degrades declared coverage instead of fabricating a hidden-effect finding
or a policy denial. Collection and replay enforce identical evidence bounds.

## Baselines, waivers and comparison

`--baseline` compares against the report object extracted from an earlier
envelope (`envelope.report`, or `envelope.payload.report` on denial), never the
outer CLI envelope. `--as-of` selects the comparison point, `--waivers` applies
the closed waiver document, and `--config` selects thresholds, required
coverage, minimum gate confidence, comparable-baseline and regression policy.
Baselines are immutable inputs; the command does not rewrite them.

New contracts are `lekalo/ai-lint-report`, `lekalo/ai-lint-evidence`,
`lekalo/ai-lint-config`, `lekalo/ai-lint-waivers` and
`lekalo/ai-lint-comparison`, all at `v0.6.4` with closed schemas under
`contracts/`. Finding families are `ambiguity.*`, `hidden.*` and
`indirection.*`, registered in the active diagnostic registry. Discovery
publishes the lint capability on workflow-provider 0.6.4 while Model/IR and
earlier protocol versions retain their own pins.
