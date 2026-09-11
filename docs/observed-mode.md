# Observed mode for existing code

Issue #39 lets Lekalo index, bind, and analyze existing code without
declaring the semantic model the owner of the implementation. The mode
semantics are fixed:

```text
Source code is primary.
Lekalo records explicit semantic bindings and evidence.
Lekalo does not generate or overwrite implementation.
```

The contract is published as
[`contracts/observed-index.schema.v1.0.0.json`](../contracts/observed-index.schema.v1.0.0.json)
(the persisted registry) and
[`contracts/observed-scan.schema.v1.0.0.json`](../contracts/observed-scan.schema.v1.0.0.json)
(the adapter scan document). Both versions are independent of the product
release, of the Model/IR/protocol contract versions, and of the diagnostic
registry version.

## What the mode guarantees

- **Adapter scans, never source parsing.** Lekalo never parses source code.
  A registered adapter produces a typed scan document — symbols, endpoints,
  schema digests, references, and confidence — and the core validates and
  merges it. A scan can never widen its own bounds: closed key sets,
  identifier/digest grammars, and section bounds fail closed.
- **Three distinguishable fact classes.** `explicit` bindings are
  user-declared, `confirmed` bindings are user-confirmed adapter mappings,
  and `inferred` bindings are adapter-owned. The status is wire data, never
  decoration: only explicit and confirmed facts promote, and a scan never
  downgrades a user-owned fact.
- **Confidence and provenance for every fact.** Confidence is the closed
  word vocabulary (`exact/high/medium/low/unknown`), never a number;
  provenance records the origin, the adapter, and the exact scan revision.
- **Stale detection.** Every binding pins a fingerprint of the source it
  was observed at. `observe check` re-fingerprints the tree: a mismatch or
  a missing source fails the gate with registered diagnostics; a binding
  without evidence is `unknown`, never the absence of behavior.
- **Move survival through adapter resolution.** A binding recorded under an
  adapter stable key survives a source move: the moved path is history
  (`moved`), never a break. Without a stable key, a path change is stale.
- **Impact states its own incompleteness.** Recorded code that references
  the canonical graph degrades the impact result: the completeness state
  drops to `incomplete`, the `observed.incomplete-graph` warning is
  attached, and the recorded dependents stay queryable through
  `observe impact`. The canonical graph alone never sees existing code.
- **Clean protection.** The observed index lives in the accepted
  `lekalo.observed-model-draft` authority home (`.lekalo/import/**`), and
  observed source files are user-owned. `lekalo generate --clean` plans
  deletions only inside `.lekalo/generated/**`, and the generated-artifact
  manifest cannot claim observed files at all — its path grammar refuses
  anything outside the managed root.
- **Explicit promotion into `contracted`.** (The contracted-mode
  surface that consumes promoted symbols is issue #40; see
  [docs/contracted-mode.md](contracted-mode.md).) Promotion is the authority
  brownfield-adoption action (`explicitAdoption`, `reviewed`, `provenance`):
  a planned, explicitly confirmed two-phase write that materializes
  canonical Model definitions from confirmed evidence, per symbol or per
  module. Inferred facts refuse; unknown evidence refuses; policy
  definitions refuse (their semantics cannot come from scan evidence
  without over-inference); silent promotion does not exist. A symbol
  whose canonical definition cannot be rendered (an unresolved reference
  to neither the canonical model nor the plan) is excluded from the plan
  and recorded only as ineligible with its reason — it is never
  advertised as planned. `--confirm` marks only symbols with a written
  canonical entry as promoted; a plan whose symbols and written entries
  diverge is a refusal, never a success receipt.

## The command surface

| Command | Behavior |
|---|---|
| `lekalo observe update --scan <file>` | Validate and merge one adapter scan into the index. |
| `lekalo observe bind SYMBOL --path <p> [--key <k>]` | Create or refresh an explicit binding; the fingerprint is captured now. |
| `lekalo observe confirm SYMBOL` | Confirm an inferred binding; only inferred bindings confirm. |
| `lekalo observe check` | The staleness gate: exit 0 current, exit 1 with `observed.stale-binding` per finding. |
| `lekalo observe attach SYMBOL --native-test <ids> --gate <ids>` | Attach verbatim native test and gate ids. |
| `lekalo observe inspect SYMBOL` | The observed card: identity, binding, evidence, provenance, attachments, completeness. |
| `lekalo observe impact SYMBOL` | The recorded impact of one symbol with its explicit incompleteness. |
| `lekalo observe promote (--symbol S \| --module M) (--dry-run \| --confirm <plan-id>)` | Plan or apply the explicit promotion into the canonical model. |

Exit classes stay status-owned (0 valid, 1 invalid/usage); receipts are
pretty two-space JSON with fixed key order, and the canonical index bytes
are compact with sorted records.

## Connection without touching source files

`observe` reads the project through the accepted structure and loader seams
and writes only to Lekalo-owned homes (the runtime index, and canonical
model documents during a confirmed promotion). Connecting an existing
project never modifies, generates, or rewrites a source file. On the
integration line, `lekalo init --adopt` (issue #38) creates the canonical
skeleton first; `observe` consumes that state and never duplicates it.

## Binding registry (issue #42)

Issue #42 turns this index into the full binding registry:
`lekalo scan` fills it through a target adapter (discovery, strict
capability selection, and the confined `scan` exchange of the #27/#28
protocol), `lekalo bindings` proposes, confirms, and audits rows, and
ambiguous adapter mappings record their whole candidate set and refuse
confirmation until one candidate is named. Native test bindings are the
registry's `verifies` rows, endpoint bindings its `exposes` rows, and
the declared target and adapter profile are set once per registry. The
normative contract is [bindings.md](bindings.md) and
[ADR-0035](adr/0035-bindings-registry.md); the additive 1.1.0 wire is
published next to the frozen 1.0.0 schemas.

## Limits

- Lekalo does not refactor, rewrite, or regenerate implementation code.
- Missing evidence is `unknown`: the mode never claims absence of behavior.
- The inferred model is never canonical and never gate-passing; only the
  explicit promotion workflow moves facts into the canonical model.
- A source change can invalidate bindings and the index at any time; the
  gate is the detector, not a watcher.
