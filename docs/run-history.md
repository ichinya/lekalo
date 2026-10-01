# Local run history and metrics recorder

Status: delivered by issue #121 (milestone M6). This document describes the
local-only run history and metrics recorder: what a run record contains, how
the store isolates and protects evidence, how retention and deletion update
indexes and dependent references, and which surfaces deliberately do not
exist.

## What it is

`lekalo history` records reproducible source measurements of pilots and
Framework Lift evaluation runs in a repository-local, tenant-scoped store
under `.lekalo/history/`. The recorder is **fully offline**: no account, no
network, no provider, no shell, and no adapter process participates in any
history operation. The store is SQLite at exactly
`.lekalo/history/store.sqlite`; the home carries a generated `*` ignore
protection, and `history init` verifies in Git projects that no history
content is tracked (a local read-only `git ls-files` query — a missing or
failing Git fails closed).

The normative wire contracts are:

- `contracts/run-record.schema.v0.4.0.json`: one durable run record
  (`dev.lekalo.run-record@0.4.0`);
- `contracts/run-assertions.schema.v0.4.0.json`: the separately stored
  assertion set (`dev.lekalo.run-assertions@0.4.0`);
- `contracts/run-observation.schema.v0.4.0.json`: the closed harness
  observation consumed by `lekalo history record --input -`
  (`dev.lekalo.run-observation@0.4.0`);
- `contracts/run-history-store.schema.v0.4.0.json`: the logical store state
  projection (`dev.lekalo.run-history-store@0.4.0`).

Canonical form is compact JSON with byte-sorted keys and one trailing LF —
the same rule the other Lekalo canonicalizers use. Stored assertion bytes
and record bytes carry SHA-256 digest bindings; recovery re-verifies every
raw record digest before rebuilding the secondary index.

## What a run record contains

- identity: a random opaque 128-bit run token, the occurrence timestamp, and
  the store-authored ingestion timestamp (`recordedAt`), plus the recorder
  artifact-kind label `history.run-record` (a closed local label of the
  schema; authority-matrix admission of recorder kinds stays with the #120
  successor procedure — the recorder claims no accepted boundary kind);
- scope: locally generated opaque `repositoryId`/`tenantScopeId` tokens and
  the #120 `repositoryRole` alias — no repository name, remote URL, tenant
  name, or path hash is representable;
- pilot: `greenfield|brownfield` mode and the measured
  `observed|contracted|hybrid` scope state; untouched legacy stays
  `observed` and is never promoted by recording;
- provenance: exact pins as value states — Git (`commit`, `dirty`,
  `workingSetDigest`), model (`revision`, `digest`, `irDigest`), lock
  (`version`, `digest`; a missing lock stays unknown and never hashes an
  empty file), core build, adapters (`id`, `version`, `manifestDigest`,
  `bundleDigest`), effective profile, and the harness identity pins;
- operation: the closed supported-operation vocabulary and the affected
  stable semantic ids;
- status: `pass|warn|fail|unsupported|infrastructure` plus
  `complete|incomplete|unknown` coverage;
- metrics: duration, file counts, tool-call counts, retry/replan counts,
  harness-reported tokens, cost (`amount`/`currency`/`basis` with
  `reported` vs `estimated`; the recorder never looks up pricing), and
  context-capsule metadata (byte size, estimator identity and version,
  fact counts, coverage ratio) — all as value states;
- measurement sources: which component supplied each known metric;
- test/gate summaries: safe ids, the original source outcome (`degraded`,
  `blocked`, `security`, `missing`, ... preserved verbatim), the mapped
  recorder outcome, count wrappers, and coverage;
- diagnostics: registered stable codes only (validated against the embedded
  diagnostic registry), registry severity, and a positive count — no
  messages, spans, or interpolated details;
- assertionsRef: the digest-bound reference to the separately stored
  assertion set (absence means no assertion evidence, never a pass);
- repeat: the parent run reference, the exact input fingerprint over the
  full provenance pin tuple, and `exact|changed|incomplete` comparability —
  exact requires a live same-scope parent and all required pins known and
  equal; a repeat link never promises deterministic model output;
- privacy: the #120 data-sensitivity label, constant `local-private`
  disposition, the exact frozen accepted #120 policy/authority references,
  and constant `ineligible` export eligibility.

**Missing metrics stay unknown.** Every provenance and metric leaf is a
value state: a missing harness observation normalizes to the `unknown`
spelling before persistence — never to zero, null, or an empty string. A
known zero round-trips as a known zero, and `withheld`/`unsupported` remain
distinct states.

**Assertions are stored separately from metrics.** Assertion rows carry only
identity, subject, kind, and outcome — never expected/actual values,
expressions, failure prose, or source excerpts. The run record binds the set
by opaque id and SHA-256 digest; the store keeps them in separate tables and
`history show` names them apart for the local operator.

## Safe fields by default

The closed typed model is the allowlist: unknown fields refuse. Identifiers
accept only a bounded lowercase token grammar that cannot express absolute
paths (`/`, `\`, `:` are excluded), URLs, or free-form text; digests must be
`sha256:<64 hex>`; timestamps must be RFC 3339 UTC; cost amounts are decimal
strings; currency is ISO-4217 shaped. A conservative second-layer scan
refuses credential-looking or encoded-blob spellings even when they fit the
token grammar. Every refusal carries a fixed detail token — the rejected
value is never echoed, on stdout or stderr.

## Atomicity, retention, and deletion

One logical history mutation is one SQLite transaction (`BEGIN IMMEDIATE`,
rollback-journal mode, `synchronous=EXTRA`, `secure_delete=ON`,
`temp_store=MEMORY`, `foreign_keys=ON`, bounded 5 s busy wait — the
effective PRAGMAs are read back and verified). The raw record row, the
separate assertion row, the index projection, every dependent invalidation,
and the generation bump commit together: a reader never observes half a
deletion, and a crash leaves the old complete state or the new complete
state.

Retention is configurable store-wide (`history retention`, defaults: 30
days, 10,000 records, 64 MiB logical payload) and enforced per tenant scope
under the strictest applicable bound, on append and explicit prune, in
stable `(recordedAt, runId)` order. Retention age uses the store-authored
`recordedAt`; a rolled-back clock never accelerates deletion. A record
exceeding the byte bound refuses (`history.retention-limit`).

`history delete` and `history prune` remove the raw row, metrics, assertion
set, and index rows, and transitively invalidate every dependent
`index|claim|aggregate-input` reference in the same transaction. Invalidated
dependents keep only their opaque id, kind, and generation; their source
identity lists are removed. `history dependents resolve` revalidates every
bound digest against live same-scope bytes right now — a cached claim
without live references can never resolve. Repeated deletion is an explicit
absent result with no cross-scope disclosure.

## Isolation

The store resolves only the current project home — never a shared
Git-common-dir, a home-wide location, or a caller-selected path. Every
lookup is composite `(tenantScopeId, ...)`, so cross-scope disclosure is
structurally impossible; an unknown scope token is a denial that discloses
nothing. Scope tokens select isolation, not authentication: mutually
untrusted actors need separate OS users; a malicious same-user process is
outside this boundary. The physical home is re-checked component by
component against symlinks, Windows reparse points, and hard-linked database
files, with owner-only permissions on Unix.

## Recovery

`history recover` verifies SQLite integrity, foreign keys, the store
identity, and every record digest, then rebuilds the secondary index only
from validated surviving records — it never silently resets the database,
treats truncated input as success, or resurrects invalidated claims.
Corruption refuses with `history.corrupt`. `history compact` runs `VACUUM`
only after deletes have committed and reports its failure separately;
compaction never undoes a committed logical deletion. (`secure_delete`
reduces recoverable cell content, but neither setting proves erasure from
SSD wear-leveling, filesystem snapshots, or external backups.)

## What deliberately does not exist

- **No export.** There is no `export`, `upload`, `publish`, or `aggregate`
  subcommand, no destination argument, no raw-debug output, and no
  user-selected output file. Records are `local-private` and
  `exportEligibility: ineligible` by construction; the public aggregate
  payload is built only by issue #102, which consumes the typed local
  dependent-reference seam (`history dependents register/resolve`).
- **No raw-record escape hatch.** Raw prompts, source snippets, secrets, and
  absolute paths are not representable; there is no redaction-off mode.
- **No pricing source of truth.** Cost metrics are exactly what the harness
  reports, `reported` or `estimated`; the recorder never infers billing.
- **No policy definition.** Records reference the exact accepted #120
  custody family; they never amend it.

## CLI map

```
lekalo history init [--role ROLE] [--project DIR]
lekalo history scope create [--project DIR]
lekalo history record --input - --scope TOKEN [--project DIR]
lekalo history list --scope TOKEN [--limit N] [--cursor TOKEN] [--project DIR]
lekalo history show RUN_ID --scope TOKEN [--project DIR]
lekalo history retention --scope TOKEN [--max-age-days D] [--max-records N] [--max-bytes B] [--project DIR]
lekalo history delete RUN_ID --scope TOKEN (--dry-run | --apply) [--project DIR]
lekalo history prune --scope TOKEN (--dry-run | --apply) [--project DIR]
lekalo history clear --scope TOKEN --apply [--project DIR]
lekalo history recover [--project DIR]
lekalo history compact [--project DIR]
lekalo history dependents register ID --kind KIND (--run RUN_ID... | --depends-on ID...) --scope TOKEN [--project DIR]
lekalo history dependents resolve ID --scope TOKEN [--project DIR]
```

Exits follow the closed result envelope: valid 0, invalid 1, denied 3,
unavailable 4, unsupported-version 5. The recorder's exit proves ingestion
only: a recorded operation whose own status is `fail` still ingests
validly, and a recorder failure never rewrites the operation outcome.

## Gates

- `node scripts/test-run-history-contracts.mjs` (Ajv 8.17.1, canonical form,
  adversarial vectors);
- `node scripts/test-run-history-cli.mjs` (offline end-to-end over the real
  binary);
- `cargo test -p lekalo-core --lib run_history` (validation, store,
  retention, recovery, dependents, isolation);
- `cargo test -p lekalo-cli --test history` (CLI surface).
