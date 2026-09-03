# Lekalo IR: typed, deterministic, target-neutral

Status: implemented for issue #8 on product candidate 0.1.6. The IR consumes
the accepted #7 loader output (#4 structure, #5 Model shapes, #6 semantic
IDs) and never rewrites them. Migration (#9) and every downstream consumer
are out of scope. The decision record is
[ADR-0007](adr/0007-ir.md). If this prose and the implementation disagree,
consumers stop; prose cannot broaden the code.

## Purpose and boundary

`lekalo-core::ir` turns the loader's normalized aggregate into one immutable
typed read model plus a separate source map:

- owned by #8: the closed typed surface (`Project`, `Module`, the closed
  `Definition` enum, `TypeRef`, `Field`, `EffectOperation`, the closed ID
  registry types), the closed decode policy with `ir.*` diagnostics,
  canonical IR bytes, the typed source map, and the IR contract identity;
- consumed, never reworked: the #7 pipeline (parsing, imports, visibility,
  short-reference expansion, compact type sugar) and its byte-identical CLI
  behavior;
- not owned: reserved-word filtering, reference target existence, reference
  kind checks, and entity identity membership stay with the Model validator
  and later semantic validation. The IR does not migrate, cache, graph, or
  invoke targets, never reads the filesystem, and never writes.

Adapters, graphs, validators, and the context builder consume the IR; none
of them re-read source YAML/JSON. The IR is usable as a Rust library without
the CLI: `loader::normalize_model` + `ir::compile`.

## Contract identity

The IR contract is `dev.lekalo.ir@0.1.0` (`FAMILY`/`VERSION`/`IDENTITY` in
`ir::version`). It is its own family: independent of the product version,
of the source Model versions, and of every other contract family. Every
compilation binds the exact source Model version it was built from
(`CompiledProject::model_version`); the IR never upgrades, downgrades, or
migrates models.

## Typed surface

Every node is a closed, immutable, typed value with no span fields. Spans
live only in the separate source map. `Definition` is a closed enum over the
twelve symbol kinds (`scalar`, `enum`, `value-object`, `entity`, `command`,
`query`, `policy`, `event`, `effect`, `endpoint`, `scenario`,
`target-binding`); `project` and `module` definitions are represented by
`Project` and `Module` with version-aware ID grammars. All variants are
exhaustively matchable without a wildcard arm, and integration tests prove
it at compile time.

- `TypeRef` mirrors the accepted Model type grammar exactly: a `ref` leaf,
  `list`, or `optional` (nullable) wrapper, at most four levels deep
  including the leaf. `map` and `result` wrappers do not exist in any
  accepted Model version; adding them here would let the IR represent models
  no source can declare. They require a reviewed Model successor and are
  deliberately absent.
- `EffectOperation` is the closed `create`/`update`/`delete` enum of the
  effect kind; `Decision`, `Visibility`, `Portability`, `ScalarBase`, and
  `HttpMethod` are closed enums with exact source literals.
- `renamed_from` and the project `id_registry` (rename history, tombstones)
  are accepted only under Model 1.0.0; under 0.1.0 they are unknown fields
  and fail closed.
- Identifier grammars are the exact JSON Schema patterns of the active Model
  version (0.1.0 module names allow hyphens; 1.0.0 symbol IDs are two or
  three segments). Reserved words stay accepted here, as in the loader; the
  Model validator owns them.

## Closed extension policy

Unknown keys fail closed with `ir.unknown-field`; nothing is ignored and
nothing is parked in a side channel. Extension metadata can only arrive
through a reviewed Model successor whose extension keys the decoder accepts
explicitly, so foreign keys can never reach core enums. There is no
namespaced extension container in IR 0.1.0 because no accepted source Model
version defines one; adding one is a reviewed IR contract successor, never a
silent widening.

## Decode diagnostics

The closed `ir.*` registry: `ir.unknown-field`, `ir.missing-field`,
`ir.kind-placement`, `ir.kind-unknown`, `ir.value-invalid` (with
`data.detail` naming the rule: grammars, closed enums, cardinality,
`type-depth`, `type-syntax`, text lengths), and `ir.duplicate-member` for
set-like arrays. Diagnostics reuse the loader's wire shape, sort order,
stream, and the 100-diagnostic bound; hostile key or value echoes are
bounded to 64 Unicode scalars plus a `…` marker. Any diagnostic fails the
whole compilation: the IR is either fully typed or absent — `invalid`, exit
1, stderr — through the accepted 0/1/3/4/5 envelope. Structure denials
(`structure.*`, exit 3) and version outcomes (exit 5) pass through the
loader unchanged; the IR adds no exits.

## Canonical serialization

`CompiledProject::to_canonical_json` emits compact UTF-8 JSON, no BOM, no
trailing LF:

- object keys in unsigned UTF-8 byte order, always the same sequence for a
  given shape;
- `definitions`, `modules`, `imports`, `derived_from`, `renamed_from`, and
  every other set-like array ordered by its unsigned UTF-8 key;
- semantically ordered arrays (fields, input, payload, enum values, identity
  members, rename history, tombstones) preserved in source order;
- absent-optional normalization: an empty optional array serializes as
  absent, exactly like an undeclared one;
- string escaping identical to the loader's canonical writer (RFC 8259
  mandatory escapes, all other Unicode scalars verbatim).

Equivalent JSON and YAML inputs produce identical bytes; repeated runs are
byte-identical. A compilation of the full-kinds fixture is frozen as
`tests/fixtures/ir/valid-full-kinds/ir.golden.json` and asserted byte-exact
by the suites.

## Source map

`Compilation::source_map` is separate from the semantic payload, never part
of equality or canonical bytes, and occurrence-safe: a sorted entry per
definition (carrying its semantic ID), per accepted mapping key, per array
member, and per type-expression node (nested wrappers at suffixed pointers
such as `/definitions/7/fields/1/type/list`). Type, `returns`, `entity`, and
`invokes` positions record their value node instead of the bare key so each
position yields the more precise span. Entries are sorted by
`(path, pointer, startByte)` and reuse the loader's wire shape
`{path, semanticId?, pointer, start, end}`. `lekalo load --ir --spans`
appends the array as the sorted `sourceMap` sibling of `ir`.

## CLI contract

`lekalo load [--project DIR] [--spans] [--ir] [--json]`:

- without `--ir`: byte-identical #7 behavior, always;
- with `--ir`: success is one compact line with the fixed key order
  `status`, `modelVersion`, `ir`, `sourceMap?`, where `ir` is exactly the
  canonical IR bytes; the human line is
  `compiled ir dev.lekalo.ir@0.1.0: <modules> modules, <definitions> definitions`;
- IR decode failures replace success with the `invalid` envelope (exit 1,
  stderr) naming the sorted `ir.*` codes.

## Fixtures

`tests/fixtures/ir/` is hermetic and hand-authored: a full-kinds project
covering all fourteen definition kinds (with compact type sugar, a
cross-module import, rename history, and tombstones), its JSON twin
(asserted byte-identical IR), frozen golden canonical bytes and span arrays,
and eight invalid fixtures pinning one `ir.*` code each. Rust suites assert
golden equality, rerun byte-identity, library-only compilation, exhaustive
matching, and source-map coverage for every definition, field type, and
reference.
