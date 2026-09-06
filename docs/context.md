# The bounded context capsule

Issue #17 projects a minimal-but-sufficient context document for one
symbol or one explicitly supplied change set: `lekalo context SYMBOL
--budget TOKENS` and `lekalo context --changed SYMBOLS --budget TOKENS`.
The capsule is selected deterministically from the accepted typed
surfaces — the compiled IR, the dependency graph (#13), and the effect
graph (#14) — never from raw source. The core owns every decision
(selection, estimation, truncation, projection); the binary only selects,
renders, and maps exits onto the accepted 0/1 envelope. The recorded
owner decisions live in [ADR-0018](adr/0018-context-capsules.md).

## Contract identity

- Discriminator: `lekalo/context/v1.0.0`
- Identity: `dev.lekalo.context@1.0.0`
- Schema: [`contracts/context-capsule.schema.v1.0.0.json`](../contracts/context-capsule.schema.v1.0.0.json)
- Independent of the product release, the Model/IR/graph/effect/protocol
  versions, and the diagnostic registry version.

## Sections

Closed v1 vocabulary in protection order — protected semantic facts
first, ranked supporting context last:

```text
symbol         root cards: identity, purpose, canonical kind contract
policies       policies applying to the roots (closed decision)
effects        declared effect edges of the root operations
dependencies   direct outgoing edges of the roots
scenarios      scenarios directly covering the roots
public-impact  direct dependents that are not module-private
bindings       target bindings of the roots' modules
types          supporting: referenced type cards
closure        supporting: bounded forward walk (depth 2, 2000 nodes,
               10000 edges; explicit closure-bounded gap when stopped)
```

Facts are typed records — structured type references, closed decisions,
kinds, actions, relations, and confidences — never ambiguous prose.
Protected facts are not rewritten by the budget; when the budget cannot
fit a fact, the fact is excluded with an explainable manifest row and
the truncation is explicit metadata.

## Budget policy

- `--budget` is required; `0` and values beyond the recorded bound
  (`1,000,000`) are fatal `graph.input-invalid` failures.
- The budget walk follows the protection order and includes a fact only
  when its exact estimate fits the remaining budget.
- `budget.estimated` is the emitted capsule; `budget.minimumRequired`
  is the exact minimum budget with zero exclusions; `budget.fits`
  is `false` exactly when exclusions exist.
- Every candidate appears in exactly one manifest row: included rows
  carry their token estimate, excluded rows carry `reason: "budget"`.

## Token estimator

v1 pins one profile: `dev.lekalo.estimator.chars-4@1.0.0`, the offline
deterministic fallback. A content string of `n` Unicode scalars
estimates `max(1, ceil(n / 4))` tokens; empty content estimates zero.
The estimate is computed per typed fact from the fact's semantic text
values, so it never depends on the output format, platform, or locale.
Every capsule records the profile identity, version, and the `sha256`
digest over the exact rule text. Successor profiles enter as additional
versioned identities; they never silently replace this one.

## Confidence gaps

Closed in-band rows, never diagnostics: `error-contracts-unrepresentable`
(the accepted Model has no error grammar — a standing honesty gap),
`detected-effects-absent` (no evidence envelopes attached),
`no-description`, `no-effects`, `no-policies`, `no-scenario-coverage`,
`no-relevant-bindings`, and `closure-bounded`. Failures reuse the
registered graph-family rules through the accepted #11 seam:
`graph.unknown-node` (unknown symbol), `graph.input-invalid`
(out-of-range budget), and `graph.traversal-limit` (over-bound root set
or manifest), all with bounded token echoes.

## Privacy

The capsule never reads files and never carries file bytes, secrets,
`.env` content, or absolute paths. The only source evidence is the
opt-in `--spans` sidecar: declaration spans resolved through the #8
source map as logical project-relative paths. Raw source bytes are not
representable in the v1 contract; any future raw-source path requires an
explicit request, a reviewed contract successor, and the privacy owners.

## Output

`--json` emits `{"status":"valid","context":{...}}` with the canonical
compact capsule (byte-sorted object keys); the human stream emits the
agent-facing Markdown rendering of the same capsule. Both are
deterministic: the same compilation, scope, budget, and spans flag
produce byte-identical bytes on every platform, whatever the frontend or
directory order was.

## CLI

```sh
lekalo context planner.focus_task --budget 5000
lekalo context planner.focus_task --budget 5000 --spans
lekalo context --changed planner.focus_task,planner.edit_task_cmd --budget 12000 --json
```

- Exactly one of the positional symbol and `--changed` is required; the
  changed set is a typed handoff (comma-separated ids) — the tool never
  parses Git or infers changed symbols (#16 owns that).
- Duplicated roots collapse; roots sort canonically; every changed
  symbol's card is present.
- Unknown symbols exit 1 on stderr with `graph.unknown-node`
  (`LEK-GRAPH-007`) — never an empty success.

## Fixtures and gates

- Hermetic planner fixture: `tests/fixtures/context/planner/` (two
  modules, cross-module `requires`, commands, queries, effects, events,
  a policy, an endpoint, a scenario, a target binding, requirements).
- Pinned goldens: `tests/fixtures/context/golden/planner.context.json`
  (payload contract), `planner.context.envelope.json` and
  `planner.context.md` (the two CLI projections).
- Independent Node gate: `node scripts/test-context-contracts.mjs` — the
  exact Ajv 8.17.1 schema gate plus coverage arithmetic, manifest walk
  order, gap vocabulary, and privacy invariants, run in CI on Node 18
  and 24.
- Rust suites: `crates/lekalo-core/tests/context.rs` (selection,
  determinism, budget walk, changed scope, gaps, estimator, privacy,
  benchmark proxy) and `crates/lekalo-cli/tests/context.rs` (goldens,
  failures, grammar, spans, custody).
