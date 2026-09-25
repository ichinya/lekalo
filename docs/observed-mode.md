# Observed mode for existing code

> Версионирование обновлено: контракт при изменении получает текущую версию проекта. Исходная точка — 0.2.16; старые схемы и миграции удалены. Правило независимой нумерации версий ниже заменено этой политикой.

Issue #39 lets Lekalo index, bind, and analyze existing code without
declaring the semantic model the owner of the implementation. The mode
semantics are fixed:

```text
Source code is primary.
Lekalo records explicit semantic bindings and evidence.
Lekalo does not generate or overwrite implementation.
```

The contract is published as
[`contracts/observed-index.schema.v0.2.16.json`](../contracts/observed-index.schema.v0.2.16.json)
(the persisted registry) and
[`contracts/observed-scan.schema.v0.2.16.json`](../contracts/observed-scan.schema.v0.2.16.json)
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
| `lekalo observe baseline [--native-plan <file>\|-]` | Record the deterministic baseline metrics document (`.lekalo/import/observed/baseline.json`, issue #49): index digest, binding-state counts, scan identity, and the validated native gate plan identity. |

Exit classes stay status-owned (0 valid, 1 invalid/usage); receipts are
pretty two-space JSON with fixed key order, and the canonical index bytes
are compact with sorted records.

## Confinement evidence (issue #89)

Every scan exchange runs under the adapter's manifest-derived
confinement budget, and the JSON scan receipt embeds a deterministic
`confinement` member: the granted budget (scope caps, environment
variable names, network/children/resources posture), the described and
effective scopes, and the honest per-dimension enforcement record —
names and tokens only, sorted, no timestamps, no host paths. The
normative shape, the enforcement vocabulary, and the escalation policy
are specified in [adapter-confinement.md](adapter-confinement.md).

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
[ADR-0035](adr/0035-bindings-registry.md); the additive 0.2.16 wire is
published next to the frozen 0.2.16 schemas.

## The pilot recipe (issue #49)

The `taskhub` pilot (`tests/fixtures/pilot/taskhub/`, exercised end to
end by `crates/lekalo-cli/tests/pilot.rs`) is the reference recipe for
connecting an existing Node consumer without rewriting it. Every name
in the fixture is fictional; the fixture family is registered
`synthetic` in the fixture-provenance manifest.

1. **Adopt.** `lekalo init --adopt` creates only the canonical skeleton
   (`lekalo/**`; the runtime home `.lekalo/**` appears with the first
   observed write). Declare the observed modules with `lekalo module
   new` — one per package scope the adapter proposes
   (`@taskhub/tasks` → `taskhub_tasks`). No source file changes.
2. **Scan.** Merge one pinned adapter scan document:
   `lekalo observe update --scan <doc>`. The pilot pins the document
   produced by the real node-typescript scanner kernel
   (`tests/fixtures/pilot/taskhub-scans/`), so the committed bytes are
   the deterministic record of that tree: stable keys, locations,
   fingerprints over the exact source bytes, and candidate sets.
3. **Bind and confirm.** The domain surface gets user-owned facts:
   `observe confirm` for confirmed adapter mappings, `observe bind` for
   explicit declarations. `observe inspect` answers per symbol with the
   card's completeness state; nothing is silent.
4. **Inspect, impact, capsule.** `observe impact` reports the recorded
   dependents (API, worker, and integration links in the pilot) and
   always states its own incompleteness; the canonical `impact` over a
   promoted symbol degrades with `observed.incomplete-graph` while
   recorded neighbors stay unpromoted; `lekalo context` builds a token-
   budgeted capsule that is materially smaller than the source corpus
   and carries the bound contracts.
5. **Native gates.** The workspace's build/typecheck/test surface is
   carried by a #48 native gate plan
   (`tests/fixtures/pilot/taskhub-gates/`): the confirmed command
   surface is recorded data. The production `lekalo native run` answers
   with the typed refusal (`unsupported` /
   `fixture-runner-not-shipped`) and never launches a command.
6. **Baseline.** `lekalo observe baseline [--native-plan <plan>]`
   records the metrics the next comparison starts from: the index
   digest, the binding-state counts, the scan revision and adapter
   identity, and the validated native plan identity. The document lives
   at `.lekalo/import/observed/baseline.json` — inside the accepted
   `lekalo.observed-model-draft` authority home — and an identical
   state writes byte-identical documents (no clock).

Privacy boundary: the consumer's repository identity, URLs, host
paths, and sensitive configuration never enter public evidence. The
`.env` file is outside every scan read root (fingerprint-proven
unread and untouched), its values never appear in any artifact, and
the fixture's own fictional slug is the only name the wire carries.

## Limits

- Lekalo does not refactor, rewrite, or regenerate implementation code.
- Missing evidence is `unknown`: the mode never claims absence of behavior.
- The inferred model is never canonical and never gate-passing; only the
  explicit promotion workflow moves facts into the canonical model.
- A source change can invalidate bindings and the index at any time; the
  gate is the detector, not a watcher.
