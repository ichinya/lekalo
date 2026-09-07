# ADR-0018: Bounded context capsules with a token budget

Date: 2026-09-06
Status: accepted for issue #17

Custody: issue #18 published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`); issue #24 published product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #25 published product 0.1.28 (annotated tag `v0.1.28` on `967bf52`); issue #62 published product 0.1.29 (annotated tag `v0.1.29` on `de6f8a7`); issue #26 now carries the **prospective product candidate 0.1.30**
in every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock` including the regenerated committed golden lock and its
digests, the `--version` behavior and its pinning tests, `README.md`,
`docs/cli.md`); this issue published product 0.1.25 (annotated tag
`v0.1.25` on `e627fe5`); issue #16 published product 0.1.24 (annotated tag
`v0.1.24` on `b4109e5`); issue #20 published product 0.1.23 (annotated tag
`v0.1.23` on `15be55a`). The context contract version
(`lekalo/context/v1.0.0`, identity `dev.lekalo.context@1.0.0`) is
independent of the product release, of the Model/IR/graph/effect/protocol
contract versions, and of the diagnostic registry by design.

## Context

Issue #13 shipped the dependency graph of semantic symbols and issue #14
the effect graph; together with the typed IR they answer what a symbol
is, what it references, and what it does. An AI agent, however, needs a
bounded, explainable context document for one symbol or one change set —
not the whole repository, and not lossy prose. Issue #17 owns that
surface: `lekalo context SYMBOL --budget TOKENS` and
`lekalo context --changed SYMBOLS --budget TOKENS`.

The research worker_done msg_7ff8f2db4fd4 (run run_088695f63032) and the
brief messages it references were not retrievable from this worker's
mailbox (the recorded issue #8 precedent, msg_bee216a0fbd8), so the
decisions below were taken directly on the live issue text, the
dispatch scope, and the accepted #8/#11/#12/#13/#14 surfaces, following
the narrow recommendations of the recorded #13/#14 precedents.

## Decision

### 1. One normalized capsule, two projections, one new contract file

The capsule is a single normalized in-memory product: the core owns
selection, estimation, and both projections; the binary only selects and
renders. `--json` emits the structured capsule
(`{"status":"valid","context":{...}}`, payload contract
[`contracts/context-capsule.schema.v1.0.0.json`](../../contracts/context-capsule.schema.v1.0.0.json),
discriminator `lekalo/context/v1.0.0`); the human stream emits the
agent-facing Markdown rendering of the exact same capsule.
`contracts/` gains exactly this one new file; the diagnostic registry is
unchanged. AI Factory owns persisted `.ai-factory/context/**` artifacts;
Lekalo produces the neutral in-memory projection only and never writes
one.

### 2. Pure deterministic selection over the accepted typed surfaces

Selection consumes only the accepted surfaces — the compiled typed IR,
the #13 dependency graph, and the #14 effect graph (declared projection,
CLI path) — read-only, in memory. There is no source scan, no Git, no
clock, no environment, no floats, and no changed-input inference (#16
owns that; `--changed` is a typed handoff exactly like
`effects conflicts --changed`). The same compilation, scope, budget, and
spans flag always produce byte-identical JSON and Markdown.

Sections are a closed v1 vocabulary in protection order:
`symbol` (root cards: identity, purpose, and the canonical kind
contract), `policies`, `effects`, `dependencies`, `scenarios`,
`public-impact` (direct non-private dependents; scenarios and policies
are excluded there because they have their own sections), `bindings`
(target bindings of the roots' modules), then the ranked supporting
context `types` (referenced type cards) and `closure` (a bounded forward
walk, depth 2, 2,000 nodes, 10,000 edges, with an explicit
`closure-bounded` gap and `complete: false` instead of silent truncation).

### 3. Protected semantic facts are never collapsed

The protected facts are emitted as typed records — fields with structured
type references (`{ref}|{list}|{optional}`, never a string grammar),
policy decisions, effect edges with closed kinds/actions/confidences,
edge records with closed relations — and never rewritten into ambiguous
prose by the budget. A budget that cannot fit everything produces, in
order: full manifest rows for every candidate (included rows with exact
tokens, excluded rows with `reason: "budget"`), `budget.fits: false`,
and `budget.minimumRequired`: the exact token total of the whole
candidate set, i.e. the minimum budget with zero exclusions. Nothing is
silently dropped; nothing is lossily summarized.

### 4. The fixed offline estimator profile

v1 ships exactly one estimator:
`dev.lekalo.estimator.chars-4@1.0.0`, the offline deterministic
fallback. Its rule is fixed: a content string of `n` Unicode scalars
estimates `max(1, ceil(n / 4))` tokens, empty content zero; content is
the fact's semantic text values joined by single spaces in a fixed
order. The estimate is per typed fact, so it never depends on the output
format, platform, or locale. The profile identity, version, and the
`sha256` digest over the exact rule text are pinned into every capsule
(`context.estimator`). Real model tokenizers enter later as additional
versioned identities; they never silently replace this one, and a
capsule always names the estimator that produced its numbers.

### 5. Budget policy and diagnostics

A budget of zero or beyond the recorded v1 bound
(`MAX_BUDGET_TOKENS = 1,000,000`) is a fatal `graph.input-invalid`
failure (exit 1, stderr, accepted envelope); an unknown root is the
registered `graph.unknown-node`; an over-bound root set or manifest
rejects with `graph.traversal-limit`. All echo data stays bounded
tokens. An *exhausted but in-range* budget is not a diagnostic at all:
it is the in-band truncation metadata of a valid capsule (decision 3),
because a too-small budget is a normal, explainable outcome, not an
error. Confidence gaps are closed in-band rows (`context.gaps`), never
diagnostics: the standing `error-contracts-unrepresentable` (the
accepted Model has no error grammar), `detected-effects-absent` (no
evidence envelopes attached), `no-description`, `no-effects`,
`no-policies`, `no-scenario-coverage`, `no-relevant-bindings`, and
`closure-bounded`. The context contract adds no registry rules in v1 —
the registry file is unchanged by design, exactly as in ADR-0013.

### 6. Raw source stays out; spans are the restricted evidence path

The capsule never reads files and never carries file bytes, secrets,
`.env` content, or absolute paths — the loader seam supplies only
canonical semantic data, and the span sidecar is opt-in (`--spans`) and
carries logical project-relative paths with half-open ranges resolved
through the #8 source map. Raw source *bytes* are not representable in
the v1 capsule contract by design; any future raw-source path requires
an explicit request, a reviewed contract successor, and the privacy
owners (#privacy authority matrix) — it cannot sneak in through this
surface.

### 7. Limits, determinism, and coverage (v1, owner-approved)

Budgets above 1,000,000 tokens and scopes above 128 roots reject;
manifests above 50,000 rows reject; the closure walk is bounded by
depth 2, 2,000 nodes, and 10,000 edges. Canonical bytes are compact
UTF-8 JSON with byte-sorted object keys; JSON sections appear
byte-sorted (the canonical key order shared with #13/#14) while the
Markdown and the manifest walk express the protection order. Coverage
(`candidates`/`included`/`excluded`) is exact arithmetic over the
manifest. The required benchmark is pinned as a deterministic gate: the
hermetic planner capsule must estimate at least 4× fewer tokens than the
token-equivalent of the whole canonical graph export (the deterministic
whole-repository projection).

### 8. CLI handoff

`lekalo context SYMBOL --budget TOKENS [--spans] [--project DIR]` and
`lekalo context --changed SYMBOLS --budget TOKENS [--spans]
[--project DIR]` only select, render, and map exits onto the accepted
0/1 envelope. Exactly one of the positional symbol and `--changed` is
required; an empty changed entry is malformed usage (`cli.usage`).
Unknown symbols exit 1 on stderr with the registered rule — never an
empty success.

## Consequences

- #16 can hand its derived change sets to `context --changed` without a
  shape change; #18 and later AI-facing consumers get one stable capsule
  wire plus the Markdown rendering.
- Successor estimator profiles and section kinds enter as versioned
  additive records, never by silently changing v1 semantics.
- The protected-fact guarantee depends on the accepted surfaces staying
  typed; any future "summarize into prose" feature must extend the
  contract explicitly, not reinterpret `context` sections.

## References

- [docs/context.md](../context.md) — the capsule surface and guarantees.
- [ADR-0012](0012-dependency-graph.md) — the dependency graph consumed here.
- [ADR-0013](0013-effect-graph.md) — the effect graph and the diagnostic
  seam precedent this ADR follows.
- [ADR-0007](0007-ir.md) — the typed IR the facts are read from.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
