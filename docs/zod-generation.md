# Zod schema generation (issue #45)

The `node-typescript` target adapter generates deterministic, typechecking
[Zod](https://zod.dev) schemas from the compiled project IR. Generation is a
target-protocol `generate` exchange: the core compiles the Model to typed IR,
writes the canonical IR evidence under `.lekalo/cache/ir/<project>.json`, and
the adapter plans and applies the generated modules through the standard
plan/apply pipeline (`lekalo generate`, `--dry-run`, `--check`, `--clean`).

## Declared surface

The generation artifact declares:

- `generate` and `verify` operations, IR `0.2.16`;
- the named capability `generate.zod: "full"` — every schema-bearing
  definition kind of the accepted IR is mappable;
- the write scope `src/generated/node-typescript/zod/**`.

A launch profile is required (the same `--lekalo-project-profile-json`
injection the scanner uses); its read roots must cover the IR evidence home
(`.lekalo/cache/ir/**`). Without a launch profile the kernel honestly reports
`unsupported` — no profile, no dispatch.

## Generated files

One emission group per weakly connected component of the cross-module
reference graph; acyclic projects keep one file per Lekalo module:

```
src/generated/node-typescript/zod/
  runtime.ts            # shared helpers: LekaloDateString, lekaloBrand, normalizeIssues
  <module>.ts           # schemas + inferred types for the module's definitions
  <module>.map.json     # canonical sidecar: field path → semantic id + declaration byte ranges
  index.ts              # sorted barrel
```

Export naming is uniform and deterministic: `<ModulePascal><NamePascal>` with
`Input`/`Result`/`Payload` suffixes for command/query/event; every schema
exports an inferred type (`export type PlannerTask = z.infer<typeof
PlannerTaskSchema>`).

The sidecar micro-contract is adapter-owned (`lekalo/zod-map/v0.3.2`,
canonical compact JSON, keys in UTF-8 byte order):

```json
{
  "adapter": {"id": "…", "version": "…"},
  "contract": "lekalo/zod-map/v0.3.2",
  "declarations": [{"end": 649, "export": "NotifyChannelSchema", "id": "notify.channel", "start": 538}],
  "fields": {"channel": "notify.channel", "": "notify.user"},
  "owner": "notify"
}
```

The core ownership manifest ingests the sidecars: `declarations` become the
manifest's `source_maps` byte ranges bound to the exact inputs revision, and
written files are recorded as kind `schema` (`.ts` under a `zod/` segment) or
`data` (`.map.json`).

## Mapping rules

Precedence: wrapper rules compose bottom-up; `required` applies at the
object-property level only.

| Lekalo construct | Zod emission |
| --- | --- |
| scalar `string` / `number` / `boolean` | `z.string()` / `z.number().finite()` / `z.boolean()` |
| scalar `date` (default `date-string` policy) | `LekaloDateString` — `z.string().regex(/^\d{4}-\d{2}-\d{2}$/)`, JSON-faithful |
| scalar `date` (`date-native` policy) | `z.date()` |
| scalar `datetime` | `z.string().datetime({ offset: true })` |
| scalar `uuid` / `uri` | `z.string().uuid()` / `z.string().url()` |
| enum | `z.enum([...])`, declared order preserved (order is semantic) |
| value-object / entity / command input / event payload | closed `z.object({...}).strict()` (`.strip()` under the `strip` policy) |
| query returns | `export const <Q>ResultSchema = <expr>;` |
| `ref <id>` | referenced schema identifier (topological order intra-file, imports cross-file) |
| `list(T)` | `z.array(T)` |
| field absent `required` | `.optional()` appended outermost (key presence axis) |
| `optional(T)` wrapper | `.nullable()` innermost (value nullability axis) |
| identity-member scalar | branded: `lekaloBrand("semantic.id")` — `z.infer` yields `string & z.BRAND<"id">`, raw strings must `.parse` |

**Optional ≠ nullable.** `required` governs key presence; the `optional`
type wrapper governs value nullability. The four combinations are distinct:
`T` / `T.nullable()` / `T.optional()` / `T.nullable().optional()`, and the
committed matrix fixture asserts all four at runtime.

Constructs outside the accepted IR (`map`, `union`, `result`, `tuple`,
integer/decimal/bigint bases, constraints, defaults, cyclic references) are
unreachable in IR `0.2.16`; the mapper classifies any that appear as
`zod.unsupported-construct` findings with `symbol:<id>` details and skips the
definition. Because the v0.3.2 wire reserves `result.findings` for
validate/verify, a generate run carrying any finding surfaces as an honest
partial error naming the symbols — nothing is emitted, nothing is silently
dropped. `verify` reports the same findings (plus `zod.drift` rows comparing
expected/observed bytes) through the closed findings member.

## Codegen policy

The adapter-owned document `lekalo/targets/node-typescript.yaml` may pin two
keys:

```yaml
zod:
  date: date-string        # or: date-native
  unknown-keys: strict     # or: strip
```

Comments and blank lines are ignored. An absent file resolves the documented
defaults (`date-string`, `strict`). A malformed present file — unknown keys
or values, deeper nesting, tabs, duplicates, overbound bytes — fails the
operation in-envelope; the policy never silently falls back.

## Runtime error attribution

The generated `runtime.ts` exports `normalizeIssues(issues, fields, owner)`:
zod issue paths (dot-joined) resolve to Lekalo semantic ids through the
sibling `.map.json` — exact field path first, then the closest enclosing
mapped path, then the module owner. Unknown paths are attributed, never
dropped, so every validation failure links back to the model.

## Checking and drift

`lekalo generate --check` is the read-only drift gate: tampered or missing
generated files block (`lock.source-changed` / `structure.document-missing`),
stale inputs mark the manifest stale, and orphans under the managed root are
reported. `lekalo generate --clean --confirm <plan-id>` removes orphans. The
adapter's `verify` operation adds the semantic layer: it recomputes the
expected bytes from the IR evidence and reports `zod.drift` findings with
expected/observed digest fragments.

## Dependencies

The adapter never embeds zod. Consumers of the generated modules supply
their own zod (minimum 3.22); the pinned `zod@3.25.76` dev dependency exists
only for the committed fixture suites, which execute the generated output
against the pinned runtime and typecheck it with the exact vendored
TypeScript pin (`tsc` semantics via the compiler API, `--noEmit`, strict —
see the `zod-emit` suite; see also `THIRD_PARTY_NOTICES.md`).

One documented strict-mode carve-out: `runtime.ts` is JS-strict by
contract — simultaneously typechecked TypeScript and directly executable
ESM — so its helper parameters carry JSDoc types rather than TS
annotations, and the typecheck relaxes `noImplicitAny` for exactly that
file. The schema modules themselves are fully typed through zod's
inference and pass strict unmodified; no other option is relaxed.
