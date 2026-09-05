# Lekalo inspect: one semantic symbol

Issue #15 gives humans and AI consumers a compact, complete, and
deterministic answer to the single-symbol question — what exactly is this
symbol and what does it touch — without reading the whole repository.

```sh
lekalo inspect planner.focus_task
lekalo inspect planner.focus_task --json
lekalo inspect planner.focus_task --include bindings,scenarios
```

The command is a thin handoff: the binary selects, renders, and maps exits;
the core owns every decision. The projection is read-only and consumes
exactly the accepted typed surfaces — the #8 IR, the #13 dependency graph,
and the #14 effect graph. It never parses source bytes, never reads target
files, never re-scans or rebuilds indexes, and never writes anywhere.

## Selector resolution

- The selector is grammar-validated (`^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*){0,2}$`,
  at most 191 bytes) before any project discovery and is never passed to a
  filesystem API. Path-like, URI-like, control, whitespace, uppercase,
  Unicode, and overlength spellings are stable `cli.usage` failures that
  echo nothing.
- A valid full semantic id resolves case-sensitively through the canonical
  definition index. An exact miss may consult the accepted #6 alias
  registry (`rename_history`): one live target resolves with selector mode
  `alias`. Tombstoned, dead, or conflicting alias targets fail closed with
  `inspect.symbol-unknown` and a fixed detail token
  (`exact-id-miss` / `tombstoned` / `alias-unresolved`). There is no fuzzy
  or case-insensitive fallback, and deleted ids are never reused.
- A valid one-segment token is a safe short name that matches final name
  segments only. Zero matches are `inspect.short-name-unknown`
  (`LEK-INS-002`); more than one match are
  `inspect.short-name-ambiguous` (`LEK-INS-003`) carrying a bounded,
  unsigned-UTF-8-sorted list of up to 32 fully qualified candidates plus
  the exact `matched` total. Ambiguity always requires disambiguation;
  there is no first-match selection.

Diagnostics ride the accepted #11 contract: `inspect.symbol-unknown`,
`inspect.short-name-unknown`, `inspect.short-name-ambiguous`, and
`inspect.output-limit` were added to the embedded registry as a
wire-shape-preserving minor increment (`1.2.0` → `1.3.0`, ADR-0014).

## One result, two views

Human and JSON are two projections of the same normalized object. The JSON
payload (`lekalo/inspect/v1.0.0`, identity `dev.lekalo.inspect@1.0.0`) is
compact UTF-8 with a fixed top-level and section wire order:

1. `schemaVersion`, `identity`, `modelVersion`, `project`, `selector`,
   `symbol` (the identity card: id, kind, module, version, description,
   visibility, portability, logical source location),
2. `contract` (per-kind typed body), `invariants`, `policies`, `effects`,
   `dependencies`, `dependents`, `scenarios`, `bindings`, `ownership`,
   `portability`, `trace`,
3. `completeness` and `diagnostics`.

Set-like arrays sort by unsigned UTF-8 bytes of their typed ids; ordered
semantic fields preserve IR declaration order. Outputs carry no raw
`Path`/`PathBuf`, source snippets, physical roots, usernames, URLs,
timestamps, locale values, or adapter transcripts. Reruns are
byte-identical and independent of filesystem order, locale, timezone, and
invocation directory.

## Section states: never a silent omission

Every mandatory section is present in every result. Each carries a closed
state plus a `complete` flag:

- `available` — facts projected from accepted data,
- `empty` — a valid symbol that declares nothing here,
- `unknown` — degraded evidence,
- `unsupported` — no accepted data source (or the section was not
  requested via `--include`), always with a fixed `reason` token,
- `truncated` — a bound was crossed; the section keeps the deterministic
  prefix and `bounds` reports `limit`, `returned`, `omitted`, `reason`,
  and the `frontier` (the sort key of the first omitted item).

`ownership` and `trace` are explicitly `unsupported` in v1 (`reason:
owner-not-accepted`): their owners (#21, #22) have not landed, and this
projection never preempts them. Richer scenario detail (Given/When/Then,
assertions, evidence) stays with #23 and is recorded as
`scenario-details-unavailable`. `completeness` aggregates the fixed reason
tokens and the total omitted-item count; v1 results are `partial` while
those owners are absent — visibly degraded, never silently incomplete.

## Sections per kind

- **entity** — typed fields with requiredness, identity membership as the
  one Model-declared invariant, effect summary counts (readers/writers/
  emitters) from the #14 reverse indexes, applicable policies, covering
  scenarios, module bindings, portability, and graph relations.
- **command** — input contract, declared effect edges (create, emit-event,
  …), applicable policies with decisions, and the full dependency/dependent
  relations.
- **query** — declared reads, the named return type, read effect edges;
  the card never claims writes from a query by omission.
- **policy** — the closed decision (`allow`/`deny`) and its `authorizes`
  relations.
- **event** — the payload contract and reverse emitter counts.
- **scenario** — summary and covers; nothing richer is invented.
- **effect / endpoint / scalar / enum / value-object / target-binding** —
  represented through the same model (operation + target + emits, invokes
  + method + path, base, values, fields, target name).

Binding items carry `{bindingId, moduleId, targetId, status, confidence}`;
namespaced target details are typed and optional — no #29 profile records
exist yet, so v1 emits `status: "declared"` with canonical confidence and
no details. Ownership is a projection of the accepted #21 manifest only;
trace links of #22 are typed ids and statuses only.

## Bounds

The independently versioned bound profile (ADR-0014) is fixed; no caller
may raise it:

| Bound | Value |
| --- | --- |
| Section items returned | 256 |
| Ambiguity candidates | 32 |
| Semantic id echo | 192 bytes |
| Description/message echo | 4 096 bytes |
| Provenance refs per item | 8 |
| Whole payload | 1 MiB (`inspect.output-limit`) |

Only the whole-payload bound fails the invocation; every other bound
degrades the affected section to `truncated` with returned/omitted counts
and the frontier. Sections are never silently dropped.

## Determinism and privacy

No physical roots, raw source text, timestamps, hosts, runtime values, or
provider transcripts enter the result; the only location is the project-
relative logical source path and pointer from the #8 source map. The
canonical bytes depend only on the compiled model: randomized module
directories, LF/CRLF, JSON/YAML twins, locale, timezone, and cwd produce
byte-identical output. The pinned goldens
(`tests/fixtures/inspect/golden/`) are gate-checked with exact Ajv 8.17.1
on Node 18 and 24 (`scripts/test-inspect-contracts.mjs`).
