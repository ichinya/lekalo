# Contracted mode for AI-written implementation

Issue #40 is the ownership mode for the maintained implementation. The
mode semantics are fixed:

```text
Lekalo Model is primary for public contract/effects/invariants.
Target source is maintained code.
Adapter checks conformance and may generate support artifacts only.
```

The maintained implementation is ordinary native code — plain
TypeScript, PHP, Go, or whatever the target speaks. It carries no
Lekalo annotations, and Lekalo never rewrites command or query handler
bodies, custom SQL or algorithms, application services, or maintained
tests outside the generated scenario layer.

The conformance evidence lives in the conformed-binding registry at
`.lekalo/import/contracted/registry.json` inside the accepted
`lekalo.observed-model-draft` authority home (`.lekalo/import/**`),
mirroring the observed index of issue #39. It binds canonical symbols
to maintained source locations, records the typed signature and
declared-effect claims captured at declaration time, the ownership
manifest of every generated support artifact, and the attached native
tests. The contract is published as
[`contracts/contracted-declaration.schema.v1.0.0.json`](../contracts/contracted-declaration.schema.v1.0.0.json)
(the adapter declaration document; the registry itself is Lekalo-owned
derived state with canonical compact bytes).

## What the mode guarantees

- **The model is primary.** The conformance gate recomputes the
  canonical signature and declared effects of every bound symbol from
  the typed IR on every run. A declaration captured against an older
  contract (an input type changed, an effect added or removed, an event
  dropped from `emits`) is drift, registered as
  `contracted.binding-drift` with the fixed detail `signature` or
  `effects`.
- **Maintained code is never overwritten.** The only writes contracted
  mode performs are the derived registry inside `.lekalo/import/**`
  and support artifacts inside `.lekalo/generated/**`; a support claim
  naming any other path refuses (`contracted.declaration-invalid`,
  detail `support-path-refused`). Handler bodies, SQL, services, and
  maintained tests are outside both homes by construction.
- **Source drift is detected.** Every binding pins a fingerprint of the
  maintained source. `contract check` re-fingerprints the tree: a
  changed or missing source fails the gate
  (`contracted.binding-drift`, details `fingerprint-mismatch` and
  `source-missing`); a binding without a fingerprint is `unknown`,
  never the absence of conformance.
- **Support artifacts have an ownership manifest.** Every registered
  support artifact records its owner symbol, closed kind (`openapi`,
  `types`, `test-skeleton`, `binding-manifest`, `conformance-metadata`,
  `migration-hint`), lifecycle (`generated`, `scaffolded`, `checked`),
  and the SHA-256 of its exact observed bytes. `contract check`
  re-digests every fingerprinted artifact: a changed or missing file
  fails the gate (`contracted.stale-artifact`). Generated OpenAPI
  fragments, DTO/type fragments, binding manifests, and conformance
  metadata are the supported kinds; actual rendering pipelines stay
  with the target adapters (#91).
- **Scenario failures link to semantic symbols.** Native tests attach
  to canonical symbols as verbatim external ids; every scenario that
  covers a bound operation resolves through the model's `covers`
  references, so a failing native test traces to its semantic symbol
  through the registry record.
- **Missing evidence is never success.** A bound operation with neither
  an attached native test nor a `test-skeleton` support artifact is a
  registered gap (`contracted.coverage-missing`). A module slice
  scoped with `--module` must implement every canonical command and
  query; a missing implementation is drift (detail `unimplemented`).

## Checks (the issue's gate list)

| Check | Mechanism | Finding |
|---|---|---|
| binding/signature compatibility | canonical signature recomputed from the typed IR vs the declaration claim | `contracted.binding-drift` (`signature`) |
| declared vs detected effects | declared effect expansion (operation, entity, emits) vs the declaration claim | `contracted.binding-drift` (`effects`) |
| required policy/transaction semantics | canonical policies and transaction contracts stay enforced by the model; implementations without evidence of enforcement surface as coverage/attachment gaps | `contracted.coverage-missing` |
| scenario/native test coverage | model `covers` + attached native tests / test skeletons | `contracted.coverage-missing` |
| stale generated support artifacts | exact-bytes re-digest of every fingerprinted support artifact | `contracted.stale-artifact` |
| public contract drift | canonical model recompilation; a symbol the registry binds that the model no longer declares is drift | `contracted.binding-drift` (`symbol-missing-from-model`) |

## The command surface

| Command | Behavior |
|---|---|
| `lekalo contract update --declaration <file>` | Validate and merge one adapter declaration into the registry. |
| `lekalo contract check [--module M]` | The conformance gate; exit 0 clean, exit 1 with registered findings. |
| `lekalo contract attach SYMBOL --native-test <ids> --gate <ids>` | Attach verbatim native test and gate ids to one bound symbol. |
| `lekalo contract support SYMBOL --kind <kind> --path <path> [--digest <sha256>] [--lifecycle <lifecycle>]` | Register a support artifact in the ownership manifest; the path must stay inside `.lekalo/generated/**`. |

## From observed to contracted without a full rewrite

The transition is additive and per symbol:

1. `lekalo observe update --scan <file>` records the existing code.
2. `lekalo observe bind` / `confirm` make the facts user-owned.
3. `lekalo observe promote --module M --confirm <plan-id>` materializes
   the canonical definitions (issue #39).
4. `lekalo contract update --declaration <file>` binds the same
   maintained sources to the now-canonical symbols.

No source file is rewritten at any step; the canonical model documents
are the only canonical writes, and both registries are derived state.

## Limits

- Lekalo does not execute, parse, or rewrite implementation code; the
  declaration is adapter-produced evidence, and its accuracy is the
  adapter's custody.
- Policy and transaction semantics remain model-side declarations: the
  registry binds implementations and their native tests, but runtime
  enforcement claims stay with the authorization (#25), transaction
  (#24), and adapter (#27/#31) owners.
- Rendering support artifacts (OpenAPI, DTOs) is a generation-pipeline
  capability (#91); this issue owns the permission boundary, the
  ownership manifest, and the staleness gate.
- A missing fingerprint is `unknown`, not a failure; run `contract
  update` with a fresh declaration to capture it.
