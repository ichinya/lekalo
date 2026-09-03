# Loader: YAML/JSON, imports, canonical normalization

Status: implemented for issue #7 on product candidate 0.1.5. The loader
consumes the closed #4 structure, #5 Model shapes, and #6 semantic-ID
contracts; it never rewrites them. IR (#8) and migration (#9) are out of
scope. The decision record is [ADR-0006](adr/0006-loader.md). If this prose
and the implementation disagree, consumers stop; prose cannot broaden the
code.

## Purpose and boundary

`lekalo load` turns a validated project tree into one deterministic,
canonical model plus exact source locations:

- owned by #7: bytes-to-spanned-tree, fixed-file discovery, document shape,
  the exact-version gate, import visibility/cycles, pre-merge collisions,
  short-reference expansion, compact type sugar, deterministic aggregation;
- consumed, never reworked: #4 physical/path rules, #5 Model v0.1/v1.0
  document shapes, #6 semantic-ID identity rules;
- not owned: closed fields, kind placement, typed reference target/kind
  checks, recursion, entity identity membership, target existence, and #6
  rename/registry invariants stay with the Model validator. The loader does
  not build IR, run migrations, or touch `.lekalo/**`.

## Pipeline and phase order

1. root selection (`--project` / `LEKALO_PROJECT`) or discovery from the
   working directory;
2. #4 structure validation (`structure.*` outcomes pass through unchanged —
   malformed structure is exit 1 stderr, policy denial is exit 3 stdout);
3. capability-safe enumeration and reads with the encoding and size gates;
4. strict spanned parsing in sorted logical-path order;
5. document shape and `schema_version` extraction;
6. exact-version gate;
7. version-dispatched decoding: imports, collisions, graph, cycles;
8. reference/type normalization;
9. canonical aggregate and optional source map.

No later phase runs when an earlier phase produced diagnostics. Within one
phase, up to 100 diagnostics are collected, then the phase stops. They are
sorted by `(path, startByte, code, data)`. If the earliest failing phase
contains a `loader.path-escape` policy denial, the phase fails as
`denied` (exit 3 stdout); otherwise it is `invalid` (exit 1 stderr).

## Documents and version dispatch

Source documents are exactly the fixed `.yaml` homes of #4:
`lekalo/project.yaml`, `lekalo/modules/<dir>/module.yaml`, and the seven
optional kind files (`entities`, `commands`, `queries`, `policies`,
`events`, `scenarios`, `bindings`). "JSON frontend" means strict JSON
content inside those `.yaml` files; `.json` siblings are not legal homes.
`lekalo/targets/**` and `.lekalo/**` are never model input.

`schema_version` is read from the parsed top-level mapping, never by a text
scan. The only accepted literals are exactly `0.1.0` and `1.0.0` — no
ranges, prerelease, or build forms. Any other literal (including `v1`,
`1`, `2.0.0`, `1.x.0`) is `versioning.unsupported-version` (exit 5,
stderr). If every document carries a supported literal but they disagree,
the result is `versioning.mixed-versions` (exit 1) with sorted
`{path, version}` data. Dispatch is an exhaustive match into `V0_1_0` or
`V1_0_0`; the source Model version is preserved in the output, never
upgraded, downgraded, or migrated.

Document shape is a loader check (`loader.document-shape`): root must be a
mapping; `schema_version` must be a string; `definitions` must be a
non-empty array; project and module documents must expose exactly one
definition; each definition must carry a string `id`.

## Frontends

Selection is deterministic: the first non-whitespace byte `{` or `[`
selects strict JSON; anything else selects YAML. A JSON failure never
falls back to YAML.

Input is UTF-8 without BOM or NUL; lone CR is rejected (`loader.encoding`,
with a stable machine reason in `data`). LF and CRLF are accepted; spans
are computed against the original bytes. Limits are enforced as
`loader.limit-exceeded`: 8 MiB per document, 64 MiB total, 64 nesting
levels, 100,000 nodes per document.

Every syntax node carries `Span { start, end }` with `start` inclusive and
`end` exclusive; `byte` is a 0-based offset into the original file, `line`
is 1-based, `column` is a 1-based Unicode-scalar count, and CRLF is one
line break. Spans delimit the source token exactly as written (quotes and
braces included).

JSON (RFC 8259): comments, trailing commas, duplicate keys, raw control
characters, leading zeros, `+` signs, non-finite numbers, and integers
beyond i128 are `loader.json-parse` or `loader.duplicate-key`. Duplicate
object keys cite both spans (`firstSpan`, `duplicateSpan`).

YAML 1.2 core, single document, block style only. Accepted: block
mappings/sequences; plain, single-quoted, double-quoted, and block
(literal/folded) scalars; comments; core `null`/`true`/`false`; base-10
integer tokens. Rejected (`loader.yaml-unsupported`): directives, `---`/`...`
markers, multi-document streams, anchors, aliases, tags, merge keys (`<<`),
flow style, non-string mapping keys, and numeric forms outside the surface
(core floats, `.inf`/`.nan`, `0x`/`0o`/`0b`, underscore digit groups).
Malformed syntax (including tabs in indentation and inconsistent
indentation) is `loader.yaml-parse`. Duplicate keys are
`loader.duplicate-key` with both spans.

## Imports, visibility, and collisions

`module.yaml` alone declares `imports`; each entry is an exact semantic
module ID under the active Model version (0.1.0 module names allow hyphens,
1.0.0 module IDs do not). Module IDs — never directory names — key the
graph; filesystem paths always come from the #4 discovery result.

- An import containing `/`, `\`, `..`, `%`, `:`, `~`, or an absolute marker
  is a policy denial `loader.path-escape` (exit 3).
- Other invalid spellings and duplicate imports are `loader.import-invalid`.
- An import naming no discovered module is `loader.import-missing`.
- Cycles (including self-imports) are `loader.import-cycle`; the graph is
  walked in sorted order and each closed cycle is reported with the
  lexicographically smallest vertex first, repeated at the end, e.g.
  `["alpha","beta","alpha"]`.
- Duplicate module IDs across physical directories are
  `loader.duplicate-module-id`.
- Duplicate symbol IDs inside one document are
  `loader.duplicate-definition`; the same live symbol ID in different
  documents is `loader.conflicting-declaration` even when the payloads are
  byte-identical. Project, module, and symbol namespaces stay separate.

`loader.path-escape` and `loader.import-invalid` diagnostics echo the
offending import token in `data.import`, bounded to its first 64 Unicode
scalar values; longer tokens are truncated at that bound with a `…`
marker appended, and tokens at or under the bound are echoed verbatim.
The bound is fixed regardless of document size, so no failure envelope
scales with hostile input.

Visibility is direct only. A fully qualified reference to another module
requires a direct import edge (`loader.reference-without-import`). A short
reference (no dot) resolves only against live symbols of the current
module: zero candidates is `loader.short-reference-unresolved`; more than
one is `loader.ambiguous-short-reference` with the sorted candidate list.
Historical IDs (`renamed_from`, rename history, tombstones) are never
aliases and are never rewritten.

## Reference surfaces and type normalization

The loader normalizes exactly these surfaces:
`fields[].type` (also `input` and `payload` field lists), `returns`,
`effects`, `reads`, `applies_to`, `entity`, `emits`, `invokes`, and
`covers`. `target-binding.target`, `identity`, provenance, and rename
metadata are not reference surfaces.

Compact type grammar is exact: `Type := Ref | list<Type> | Type?` with
lowercase `list`, no internal whitespace, and a maximum of four levels
including the leaf (three wrappers). Violations are `loader.type-syntax`;
deeper nesting is `loader.type-depth`. Canonical form is the one-key
recursive object: `{"ref": id}`, `{"list": ...}`, `{"optional": ...}`.
Structured input forms normalize to the same bytes as their compact twins.
Non-string reference values pass through untouched for the Model validator.

## Canonical output and source map

Success is one compact UTF-8 JSON line plus LF:

```
{"status":"valid","modelVersion":"1.0.0","model":{"definitions":[...],"modules":[...],"project":{...}}}
```

The envelope top level is the fixed contract order `status`,
`modelVersion`, `model` — exactly the sequence shown above, never
re-keyed. Inside `model`, object keys are recursively ordered by raw
UTF-8 bytes; `modules` and `definitions` are ordered by semantic ID;
each module's `imports` array is sorted by module ID (it is declared
set-like). Every other array keeps its
source order — fields, effects, history, and covers are never reordered
into semantics. Equivalent JSON and YAML inputs produce identical model
bytes; repeated runs are byte-stable. The aggregate contains no paths,
spans, timestamps, OS data, or injected fields.

`lekalo load --json --spans` adds a sibling `sourceMap` array sorted by
`(path, pointer, startByte)` with entries
`{path, semanticId?, pointer, start:{byte,line,column}, end:{...}}`.
Entries exist for every definition (`/project`, `/modules/i`,
`/definitions/i`), every normalized type or reference position, and — for
modules — recorded import/type crumbs. `sourceMap` never inserts `span`
into model definitions and is excluded from model equality.

## CLI contract

- `lekalo load [--project <relative-dir>] [--spans] [--json]`
- `--project` beats `LEKALO_PROJECT`; with neither, the root is discovered
  from the working directory upward.
- Exit and stream protocol: valid = 0 stdout; malformed/shape/collision/
  import/cycle/reference/type/mixed = 1 stderr; path/policy denial = 3
  stdout; unsupported Model version = 5 stderr. Human failures print one
  line `<status>: <codes>`; human success is one summary line.

## Path safety and no-write guarantee

Root selection and validation port the accepted #4 rules: closed selector
and path grammars, NFC, DOS-device and short-name denial, fail-closed
enumeration, symlink/reparse/special-file denial, nested-root denial,
closed module file sets, and scan limits (depth 64, 10,000 entries). On
Unix the loader opens descriptor-relative (`openat` with `O_NOFOLLOW` and
`O_DIRECTORY`) and re-checks identity (device, inode, type) after opening;
on Windows it checks every component and rejects every reparse point
(symlinks and junctions) plus 8.3 aliases, re-verifying classification and
length after opening. The loader performs no writes: no cache, reports,
backups, lockfiles, generated output, or in-place normalization.
