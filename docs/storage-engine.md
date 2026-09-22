# Storage engine profiles (issue #69)

The storage-engine family turns the #65 storage projection into
engine-layer facts: one versioned engine capability profile, the
deterministic DDL and migration-plan emitters that consume it, the
adapter-produced checked-mode introspection evidence, the drift
comparison, and the #24 capability snapshot mapping. Three closed
contract families carry the lifecycle, all published at `0.4.0`:

| Family | Identity | Home |
| --- | --- | --- |
| Engine profile (declaration) | `dev.lekalo.storage-engine@0.4.0` | `contracts/storage-engine.schema.v0.4.0.json` |
| Introspection evidence (adapter-produced) | `dev.lekalo.storage-introspection@0.4.0` | `contracts/storage-introspection.schema.v0.4.0.json` |
| Migration plan (computed proposal) | `dev.lekalo.storage-migration-plan@0.4.0` | `contracts/storage-migration-plan.schema.v0.4.0.json` |

The separation is deliberate (ADR-0042): the profile is declared
policy, the evidence is observed fact, and the plan is a computed
proposal. One family with optional members would blur all three
boundaries.

## Boundaries

- **No execution.** Core renders and compares; it never connects,
  never executes SQL, never spawns a process. Apply is adapter-owned
  under the declared `generate.storage-ddl` /
  `plan.storage-migration` capabilities, and the plan's `planId` is
  the apply authority — the native-gate custody pattern.
- **No credentials.** Every database reference is one opaque
  connection name token (`^[a-z][a-z0-9-]*$`), resolved against a
  connection registry outside the model and evidence. Hosts, URLs,
  users, passwords, and parameters are unrepresentable by grammar.
- **Nothing invented.** A rendering the profile does not declare
  refuses with a registered diagnostic (`render-unsupported`) instead
  of guessing; a computed generated column refuses because the 0.4.0
  member carries no expression.

## The profile

The attachment pins one exact engine version (`engineVersion` must
resolve to one row of the owner-published matrix — never clamped),
and declares:

- **policies** — JSON (`jsonb` default, `json` opt-in), enums
  (`check` default: every enum column carries the bounded member-list
  CHECK `chk_<table>_<column>`, planned by the migration planner and
  compared by drift; `native_enum` refuses with `render-unsupported`
  until the 0.4.0 renderer creates enum types — never a silent
  varchar), arrays (`native` | `json` | `unsupported`), time
  (`timestamptz` instants; the naive timestamp is gateable policy
  surface no v1 domain type names), pagination (LIMIT/OFFSET and
  keyset cursors), and identifier quoting (v1 has exactly one answer:
  always `"name"`, deterministic and safe against reserved words).
- **tenancy** — `none | application | rls`; an RLS enforcement
  declares its session variable and FORCE posture and renders
  explicit `enable_rls`/`create_policy` steps for every covered
  (tenant-keyed) table. The tenant key column alone never implies
  enforcement.
- **concurrency** — `version_column` optimistic versioning with a
  wait policy; the #24 mapping answers CAS `full`,
  `etag_if_match` `partial`, `invariant.unique_concurrent` `full`.
- **introspection** — mode const `checked`, a bounded scope
  allow-list, one connection token. Absent means the capability is
  not claimed; unchecked observation is unrepresentable.
- **testLifecycle** — `database|schema|transaction` isolation,
  `create_drop|template|none` provision, `drop|truncate|rollback`
  cleanup, and `production: "forbidden"` (the only wire value).
  Adapters own create/drop/migrate/seed; core owns the policy and
  checks.
- **extensions** — the closed allow-list; observed extensions
  outside it are reported, never coerced.

## The version matrix

`postgres::version_matrix` is the owner-published evidence table
answering seventeen `storage.postgres.*` capabilities per major
(15–18). The per-version differences are the point: `sql_json` is
partial on 15, `temporal_constraint` (WITHOUT OVERLAPS) is full on 18
only. A pin below the floor refuses as
`storage-engine.version-unsupported` — unsupported, never clamped.

## The #24 seam

`snapshot::build` assembles the honest engine answers into the
`CapabilitySnapshot` that `transaction_concurrency::map_capabilities`
consumes: `isolation.snapshot` and `lock.range` answer `partial`
(PostgreSQL implements snapshot isolation through REPEATABLE READ,
and there are no key-range locks below SERIALIZABLE),
`external.compensation` answers `unsupported`. Strict mapping blocks
unsupported, unknown, and unapproved partial support — planner
guarantees are checked on PostgreSQL, not assumed.

## Introspection and drift

The adapter (declaring `scan.storage-schema`) produces one
`storage-introspection` evidence document per exchange: the
server-reported version, the bounded scope read, observed tables with
typed columns, indexes, and constraints, the installed extensions,
and the explicit `unsupported[]` records. Core normalizes it
fail-closed, then `compare_drift` reports `missing` / `extra` /
`divergent` findings per table, column (type, nullability, default
spelling, and identity), primary key, declared and derived CHECK
constraint, foreign constraint, and index — over entity tables and
join tables alike — plus the verbatim unsupported findings. The
verdict stays data: drift never invents a remediation and never
changes the exit class by itself.

## Migration plans and the gate

`plan_migration` diffs the two derived projections mechanically and
emits ordered steps — extensions, sequences, tables, join tables,
foreign keys, checks, indexes, sequence ownership and lifecycle (a
renamed table renames its `seq_*` sequence; a dropped sequence
column retires it), RLS, drops last —
each with its closed `DataRisk`. A plan with a destructive step is
`gated`; its status stays `blocked` until the caller names the exact
`planId` (`lekalo storage migrate-plan … --confirm sha256:…`), the
same custody as `update --apply`. A wrong digest refuses with
`LEK-SEN-009`. A NOT NULL change carries `backfill_required` visibly
and plans the executable order: an added NOT NULL column without a
 declared default is added nullable, backfilled, and only then held by
`SET NOT NULL` (an inline `ADD COLUMN ... NOT NULL` fails on any
non-empty table); a tightened existing column backfills before
`SET NOT NULL`; a column with a declared default adds in one step (the
fast default fills existing rows). The `backfill` step writes only the
column's declared default or its storage type's zero value — a type
with no zero value refuses (`render-unsupported`), never backfilling
NULL.

## CLI

```text
lekalo storage profile --engine postgres [--version V]
lekalo storage validate PATH
lekalo storage ddl PROFILE --projection PATH
lekalo storage migrate-plan BASE CANDIDATE --profile PATH [--confirm PLAN_ID]
lekalo storage drift SCAN --projection PATH --profile PATH
lekalo storage input PROFILE --projection PATH
lekalo storage capabilities PROFILE --projection PATH [--requirements PATH] [--profile strict|permissive]
lekalo storage conformance --profile PATH --projection PATH [--scan PATH] [--drifted PATH] [--input PATH] [--runtime PATH]…
```

## Conformance

`lekalo storage conformance` runs the fourteen-check closed battery
in fixed order (`profile.wellformed` … `input.single-document`).
Each check passes, fails, or skips with a bounded reason; a skip is
never a pass. The checks are component semantics, not protocol
checks, so #117 (MySQL) reuses the whole catalog unchanged — exactly
the shared seams the plan designates: family shells, module layout,
drift, plan gate, battery, CLI group, `LEK-SEN` diagnostics, and the
lifecycle/tenancy member shapes.

## Golden provenance

The committed storage-engine goldens are produced by hand from the
production binary and guarded by exactness assertions, not by a
committed generator script (plan §3.7's generator was not delivered;
this is the documented provenance in its place). The DDL golden
(`tests/fixtures/storage-engine/derived/postgres-ddl.json`) is the
byte-exact `lekalo storage ddl --json` output over the committed
valid pair; the runtime goldens are the byte-exact
`lekalo storage input --json` output over the same pair (one document,
three runtimes); the introspection goldens are hand-authored evidence
observing the full declared schema, guarded by the zero-drift
assertion and the canonical-form contract gate. The CLI emits exactly
one terminal LF after every domain document, while the committed
goldens store the canonical payload without it — strip the terminal
LF (or compare against the trimmed bytes) when regenerating. When an
engine-profile change alters a renderer, regenerate by re-running the
producing CLI command and re-committing the payload bytes — the
byte-equality tests make any drift loud.

Two drift verdicts report the same extension twice by design: the
allow-list check records `extensions/<name> extension-unallowlisted`,
and the evidence records re-surface the same fact verbatim as
`unsupported/extension/<name>` — provenance from two independent
sources, not duplication.

The drifted-evidence vector currently yields exactly nine typed
findings (three `missing`, two `extra`, one `divergent`, three
`unsupported`); the evidence-log expectation is pinned by the review
record, and a widened comparison that moves that count is a
deliberate contract change, not noise.
