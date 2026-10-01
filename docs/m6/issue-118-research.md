# Issue #118 — brownfield private TypeScript consumer pilot: research

Deliverable of [issue #118](https://github.com/ichinya/lekalo/issues/118)
(milestone M6, pilot: Brownfield): connect one existing private
TypeScript application in `observed` mode, bind one use case, project
graphs and a context capsule, prove staleness detection on a controlled
change, and emit only privacy-safe aggregate evidence. Companion
implementation report: `docs/m6/issue-118-implementation.md`.

## Approach

The pilot is a **harness**, not a one-off shell session:
`scripts/pilot-brownfield-ts.mjs` runs the whole path end to end
against any consumer root, so the run is reproducible and the private
material never needs to enter this repository. The same harness runs
the committed synthetic fixture (`tests/fixtures/pilot/brownfield-ts/`)
in CI through `scripts/test-pilot-brownfield-ts.mjs`.

- **Copy, never mutate.** The consumer is copied into a disposable
  working directory with a `.gitignore-lite` skip list (`node_modules`,
  `.git`, `dist`, `coverage` — their presence is recorded) plus a
  grammar skip: entries the portable path grammar cannot spell are
  counted and left out of the copy. An opt-in `--in-place` flag exists
  but the shipped run does not use it. The real run wrote nothing into
  the consumer tree; the adopt dry-run file-list purity assertion in
  the copy is checked every run.
- **Observed mode only.** Every recorded fact enters through the #39
  observed seam (`observe update`, `observe bind`, `observe confirm`,
  `observe attach`); the single canonical promotion is the explicitly
  planned and confirmed `observe promote` of the one bound use case.
  No generation, no source mutation beyond the one reverted controlled
  change inside the copy.
- **The bound use case** carries the issue's conditional semantic id
  `task.submit`, anchored by an explicit `observe bind` at the native
  submission function the operator names on the command line
  (`--bind <file>:<export>[:<id>]`). The scan supplies the native
  stable key, location, and fingerprint; the operator owns the
  semantic id. This mirrors the committed taskhub scan derivation,
  where semantic ids are authored-to-grammar over kernel output.
- **Privacy.** Emitted artifacts (`metrics.json`, `report.md`) carry
  counts, sizes, durations, digests, and statuses only: the consumer is
  named as `consumer` plus the sha256 of its canonical path; no file
  names, identifiers, snippets, test names, or absolute paths. The
  scan document and observed index live only inside the working
  directory and are removed with it unless `--keep` is passed.

## Observed-mode CLI surface map (what the pilot actually used)

| Command | Role in the pilot |
| --- | --- |
| `lekalo init --adopt --target node-typescript --dry-run` | Prints the adoption plan; the harness snapshots the copy before/after and asserts byte-identity (purity gate). |
| `lekalo init --adopt --target node-typescript` | Fixes the project id (derived from the workspace root manifest) and writes only `lekalo/project.yaml` + `lekalo/targets/node-typescript.yaml` inside the copy. |
| `lekalo module new <id>` | Declares the semantic modules the scan will name (one per package scope). Required before the merge: `observe update` refuses a scan naming an undeclared module. |
| `lekalo scan --target node-typescript --profile standalone --project . node <adapter.mjs> --lekalo-project-profile-json '<json>' --lekalo-framework-policy-json '<json>'` | The #42 wire path: everything after the scanner program rides the adapter argv verbatim, so BOTH trusted launch inputs reach the adapter. Refused for this consumer shape — see the plumbing finding below. |
| `observe update --scan <doc>` | Merges one `lekalo/observed-scan/v0.2.16` document; used by the wire-fallback path. Already records bindings (the wire scan merges through the same seam — no separate `observe update` after `lekalo scan`). |
| `observe bind <symbol> --key --path --line` | Records the explicit, user-owned use-case binding under `task.submit`. |
| `bindings propose` / `bindings confirm <prop-…>` | Derives the confirmation proposals and confirms one unambiguous proposal of the flow's handler chain. |
| `observe attach <symbol> --native-test <ids>` | Attaches verbatim native test identities to the use case. |
| `observe promote --symbol … --dry-run` / `--confirm <plan>` | Two-step explicit promotion of the use case into the canonical model (required before `context` can project it). |
| `observe inspect <symbol>` | The observed card: binding state, evidence, native tests, endpoints, completeness. |
| `observe impact <symbol>` | Direct/reverse impact with the recorded unknown-edge count and completeness verdict. |
| `context [--json] --budget <tokens> <symbol>` | The bounded capsule (Markdown and `lekalo/context/v0.2.16` JSON) with estimator, coverage, and gap metadata. |
| `observe baseline` | The deterministic #49 baseline over the merged index (digest + counts). |
| `bindings list` | The registry projection: implement/expose/verify rows with state counts. |
| `status` / `doctor --json` | Freshness panel and readiness verdict over the copy. |
| `bindings audit` / `observe check` | The staleness gate: re-fingerprints every binding; both refuse when the controlled change lands. |

## Framework-policy plumbing (finding)

The framework policy is a **trusted launch input**, not wire data
(issue #115): the closed wire request (`RequestEnvelope`) cannot enable
a provider, and the scanner reads enablement only from the decoded
`--lekalo-framework-policy-json` launch value. The pilot verified the
`lekalo scan` wire path **can** carry the policy: trailing adapter
arguments pass through verbatim, so the policy reaches the spawned
scanner. What the wire path cannot carry is **this consumer's shape**:

1. The manifested path prefers `adapter.manifest.json` beside the
   launched entry and cross-checks the describe outcome. The manifest
   declares `profiles: ["standalone"]` and read scopes pinned to
   `src/**`/`.lekalo/ir/**`; a monorepo pilot profile
   (`packages/*/src` roots) fails the consistency gate
   (`adapter.manifest-mismatch / capabilities.readScopes`).
2. The implicit local-development path (the same committed bundle
   copied manifest-free) passes the gates and completes the exchange,
   but the adapter refuses to serialize the outcome: a scan with any
   uncertainty returns the `partial` state (`uncertainty-present`),
   which the wire maps to `adapter-error-partial` — a node_modules-free
   copy always carries unresolved imports. (The closed 128-byte entry
   detail cap additionally excludes scoped-package monorepos whose
   tokens exceed it, as the committed taskhub derivation already
   documented.)
3. The harness therefore falls back to driving the committed kernel
   directly — `adapter.mjs`'s `__lekaloKernel`/`__lekaloScanner` seams,
   the same derivation `scripts/gen-taskhub-scan.mjs` uses — with the
   identical profile and framework policy, then merges the assembled
   document through `observe update`. Both wire refusals are recorded
   in the run report as findings; the fallback stays inside the
   committed adapter artifact (no forked scanner logic — the document
   assembly mirrors `scan_service.rs::build_document` and the closed
   #39 grammar).

## SQLite vs MySQL (honesty note)

The issue describes the consumer as Drizzle-over-MySQL. The actual
private consumer uses the **SQLite** dialect (`drizzle-orm/sqlite-core`,
`sqliteTable`). Recorded honestly rather than silently normalized: the
pilot ran against the real dialect, and the adapter's embedded Drizzle
declaration closure maps only the `drizzle-orm`, `relations`, `sql`,
`pg-core`, `mysql-core`, `node-postgres`, and `mysql2` subpaths —
`drizzle-orm/sqlite-core` (and `drizzle-orm/better-sqlite3`) stay
unresolved. Drizzle evidence therefore degrades to explicit partial
completeness with `binding-unresolved`/`callee-unproven` limitations
over the SQLite schema (owner-supplied `drizzle.bindings.json` at the
copy root stayed unresolved for the same reason). SQLite-dialect
coverage is a concrete requirement finding for the generic storage
profiles; the synthetic fixture intentionally pins the same honest
behavior in CI.

## Portable-path grammar vs the vitest convention (finding)

The read-view and inventory grammar accepts only portable lowercase
segments (`^[a-z0-9.][a-z0-9._-]*$`). The consumer's vitest suites live
under `__tests__` directories, which the grammar cannot spell: the
copy skips them (counted), the scan cannot claim their names, and no
exclusion spelling can name them either (exclusions validate against
the same grammar). Native-test attachment for this consumer is
therefore an explicit gap; the requirement finding is a spellable
test-directory convention or a bounded exclusion vocabulary for
unspellable spellings.

## Framework extraction over a node_modules-free copy (finding)

With `node_modules` skipped, the `hono` specifier resolves to no
declaration, and the provider (which recognizes the class through the
reserved specifier **plus** a resolved declaration) records
`hono-unresolved-constructor` uncertainty and extracts no routes. The
committed Hono fixtures work because they ship authored `types/*.d.ts`
stubs mapped through tsconfig `paths`. The requirement finding: a
spellable declaration surface for framework specifiers when the copy
has no installed packages (fixture-style stubs, or an embedded
declaration surface analogous to the Drizzle closure).

## Metrics

The pilot metrics the issue lists are covered by `metrics.json`:
context capsule size/tokens, included sections, scan
file/symbol/entry counts, uncertainty and unknown-edge counts,
cold vs incremental (warm) scan durations, staleness detection result,
binding registry counts, and per-step durations. Agent-side call
counts and verified success/fix cycles with/without Lekalo are
out of the harness's reach and are not claimed.
