# Versioned diagnostic fixes

Status: **Implemented** in product 0.8.0 (issue #74). Owner: diagnostic fix service.

A fix is a reproducible recipe, not a free-form patch or an executable adapter
script. Recipes pin their ID, independent semantic version, risk class,
originating diagnostic codes, structured operation, operation scope,
preconditions and post-validation. The closed wire contracts are
[`fix-recipe`](../contracts/fix-recipe.schema.v0.8.0.json) and
[`fix-result`](../contracts/fix-result.schema.v0.8.0.json).

## Preview first, explicit application second

```sh
lekalo fix --list --json
lekalo fix LEK-FORMAT-001 --dry-run --project tests/fixtures/fix/project --json
lekalo fix --safe --project tests/fixtures/fix/project
```

`--list` is project-independent catalogue metadata: its null fingerprint does
not authorize execution. Selecting a diagnostic ID, code, or recipe ID prepares
a bound recipe. `core.canonical-format` and
`core.canonical-format@1.0.0` select the same pinned implementation.
`lekalo fix --safe` selects only safe recipes and **defaults to preview**.
Neither discovery nor preview creates caches, locks, receipts or source writes.

Human preview shows the semantic summary and complete file patches. JSON preview
contains the same owner-produced semantic and file diffs, the source-bound
recipe, exact input pins, impact/generation projections, and `planId`. Review it,
then explicitly supply `--apply --expect-plan <reviewed-planId>` with the same
selection, project and policy. There is no automatic confirmation and no silent
application, including when `--json` is used. A partial or over-limit preview
cannot be applied.

An unknown selector such as `LEK-REF-014` currently returns
`fix.recipe-unavailable` (exit 4): that illustrative code is not in this
registry. The service never invents a replacement for an unregistered rule.

## Risk classes and supported operations

- **safe**: no public behavior, contract, storage or wire change. The admitted
  `core.canonical-format@1.0.0` recipe uses the canonical formatter's structured
  syntax/trivia renderer, refuses comment loss, and proves normalized Model
  **and** compiled IR equality before sealing. It addresses `LEK-FORMAT-001`
  (`format.diff`); previews also emit the additive `LEK-FIX-009` diagnostic with
  an inert, registered recipe suggestion.
- **unsafe**: may change implementation semantics. An explicitly selected
  `--recipe <file>` plus `--allow-unsafe` is required for application. A semantic
  delta classified as source-, wire-, storage- or data-loss-breaking cannot be
  relabeled unsafe. Conservatively, only the command-effect add/remove vertical
  can be implementation-only unsafe; field, enum, signature and definition
  shape operations require the breaking class even for additive changes.
- **breaking**: changes contract, storage or wire compatibility. Requires an
  explicitly selected recipe and `--allow-breaking`. The existing
  [change-policy](change.md) evidence requirements still apply to storage
  migration and data-loss risk; the acknowledgement flag cannot bypass them.
- **manual-only**: explanation only. Preview can display the explanation without
  even loading a project. `--apply` is always denied, regardless of flags.

Unsafe and breaking recipes lower a `model-change` operation to the closed
[change request](change.md), never raw text edits. Their originating diagnostic
codes must actually be present in the default semantic validator before
preparation. The candidate must pass validation; the same diagnostic codes must
be absent after the actual writes. Diagnostics owned by unavailable external
producers cannot be guessed or marked resolved. This release does not fabricate
built-in unsafe or breaking repair suggestions for such producers.

## Source custody, rollback and replay

The source fingerprint hashes the sorted complete admitted `(logical path,
sha256 bytes)` inventory. Even whitespace drift or added/deleted source files
changes it. An imported executable recipe must include the exact
`preconditions.sourceFingerprint` obtained from a producer's source snapshot;
unbound imported recipes are refused. Its exact encoding is SHA-256 of compact
JSON for the path-sorted array of `[logicalPath, "sha256:<file hash>"]` tuples
(UTF-8, no trailing newline). A safe preview exposes that same fingerprint. The sealed change plan also pins audit history, target
configuration and generation evidence. Changed fingerprints, preconditions,
inventory, policy or preview identity refuse rather than guessing a nearby edit.

Application reuses the [change transaction](change.md), including its shared
cross-service writer lock, no-follow project filesystem, before/after digests,
verified backups, journal and reader barrier. Actual written sources are
reloaded and checked against the entire expected inventory before the commit
marker. Compilation, default semantic validation and the originating recipe's
validator then run **inside** that transaction. Failure restores all source,
history and local custody bytes; failed restoration retains the journal and
requires explicit `lekalo change recover --transaction <id>`. No separate fix
journal, lock, unverified rollback writer or history format exists.

Successful application persists a custody-backed change receipt in
`lekalo/change-history.json`. Its `fix:apply` row identifies `recipeId@version`
and the exact descriptor digest; the receipt binds the request, plan, file
before/after digests, accepted policy, transaction and hash-chain anchor. No-op
application also passes freshness checks and records an explicit audit receipt.
Re-applying the same reviewed plan returns the original locally verified receipt
with `state: replayed`, no writes, and a fresh originating-diagnostic check.
Re-discovering a safe fix after application produces an empty, semantically equal
preview. Editable or imported audit history alone cannot authorize replay.

## Adapter-owned recipes

An adapter-owned descriptor uses `owner: adapter.<target>`,
`id: adapter.<target>.<recipe-name>`, and the matching `target` value. User-owned
descriptors use `owner: user` and `id: user.<recipe-name>`; they cannot impersonate
a core recipe. All descriptor objects are closed, byte-bounded, versioned and
reject duplicate JSON keys.

The `scope.operations` and `scope.subjects` arrays must admit **every** requested
semantic operation and subject. For example, an adapter may explicitly declare
`entity-field:add` on `tasks.task`; it cannot remove a field, change another
entity, supply filesystem paths, or patch arbitrary canonical Model bytes. The
core resolves actual source locations through the typed change executor. Adapter
ownership never grants a generic file writer or permission to rewrite Model
outside those operations. Target artifact scripts/raw generated-file patching
are deliberately not an executable operation in this contract. Existing target
regeneration remains owned by the adapter protocol, not an implicit side effect
of `fix`.

## Machine results and gates

Use the global `--json` flag for `listed`, `preview`, `committed`, `replayed`, or
`no-op` result documents. Manual-only and rollback denials preserve their
explanatory fix result in the normal domain envelope's `payload`; diagnostic
IDs and process exits retain the shared [diagnostic contract](diagnostics.md).
A successful dry run has `dryRun: true`, no receipt, and post-validation
`not-run`; it is not evidence of a commit. Post-write failure reports
`rolled-back`, never a successful fix.

Contract and CLI gates are `scripts/test-fix-contracts.mjs` and
`scripts/test-fix-cli.mjs`. `tests/fixtures/fix/` and core golden tests cover safe
application/replay, stale fingerprints, conflicts, rollback of existing
history/custody, manual-only refusal, risk gates, and adapter scope attacks.
