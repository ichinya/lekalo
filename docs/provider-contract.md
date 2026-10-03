# Lekalo workflow-provider contract (issue #34)

This document is the normative process boundary between Lekalo and an
external workflow consumer (AIFHub Extension `/aif-*` lifecycle
commands). It publishes the discovery handshake, the operation
inventory, the exact command mapping, the output schema identities,
the exit/stream discipline, the side-effect classes, and the limits a
consumer may rely on. It does not change any existing command; it
describes and constrains them.

The contract identity is **`dev.lekalo.workflow-provider@0.6.3`**, wire
discriminator **`lekalo/workflow-provider/v0.6.3`**, schema
`contracts/provider-capabilities.schema.v0.6.3.json` — updated in fix
round 1 (see [issue-34-fix1.md](issue-34-fix1.md)): the operation
vocabulary is ten operations (the read-only `drift` check joined), the
`validate`/`drift` receipts have their own published describing
schemas, the prescribed `validate` argv carries `--no-cache`, and the
schema enforces per-operation const tuples. Per
[versioning.md](versioning.md), this contract takes the product version
of its implementation commit; the upstream contracts it pins keep
their own independent family versions.

## Positions

1. **Discovery is metadata, forever.** `lekalo provider describe`
   works outside any repository, reads nothing, launches nothing, and
   writes nothing. It never runs `init`, `install`, `update`, `sync`,
   `migrate`, or cleanup as part of discovery or as recovery for any
   failed operation. A consumer that wants those must invoke them as
   its own explicit, user-approved steps; the provider never will.
2. **Product, project, and target availability are separate facts.**
   The manifest proves the binary supports an operation; it never
   proves a project is initialized, a lock is fresh, or a target
   adapter is installed. A supported operation with a missing
   prerequisite fails at the operation with a typed diagnostic — never
   by silently scaffolding.
3. **Fixed command construction.** The consumer recognizes known
   operations and constructs argv from this document. There is no
   execute-by-manifest facility: command text returned by a provider is
   never executed. The manifest carries no command strings at all — the
   argv recipes live only in this document.
4. **Closed vocabulary, exact versions.** Manifest and operation
   selection is by exact identity match, not a semver range. An
   unknown operation, an unknown schema identity, or an unexpected
   product/contract combination is `unsupported` for the consumer —
   a refusal to guess, not a validation error. The prescribed argv per
   operation below is the provider-relevant subset of each command's
   grammar, not the command's complete grammar; a consumer must not
   hand-craft argv outside it.
5. **The consumer owns evidence custody.** The consumer writes its own
   aggregate evidence (for AIFHub: the accepted
   `ai-factory.provider-evidence-envelope` under
   `.ai-factory/qa/**` per the authority matrix). Lekalo remains a
   reader of that custody and never becomes a QA writer. Lekalo's own
   writes stay inside its governed `.lekalo/**` runtime custody.
6. **OpenSpec/HLV canonical paths are never written.** No operation in
   this contract writes into `openspec/**` or HLV diagnostic homes:
   those are protected homes, and a generation write plan covering one
   is refused as `target.protected-path` (exit 3). The mutating
   `generate` form writes what its reviewed adapter declares — the
   adapter-declared managed write scopes verified against the ownership
   plan (legitimately including managed source roots such as
   `src/generated/**`) plus Lekalo's own `.lekalo/generated/**`
   metadata. Consumers must scope generation's write authorization from
   the adapter declaration and the ownership plan, never from a
   `.lekalo/**` assumption.

## Discovery

```sh
lekalo provider describe --json
```

Exit 0, stdout. The receipt is `{"status":"valid","manifest":{...}}`
exactly as in `tests/fixtures/provider/describe.golden.json` and the
committed schema. Byte-identical across hosts and runs for the same
binary; no timestamps, no host paths, no environment.

The manifest advertises:

| Field | Meaning |
| --- | --- |
| `schemaVersion`, `identity`, `productVersion` | The exact workflow-provider contract discriminator, identity, and the product version this contract was implemented at (`0.6.3`; deliberately frozen — it may trail a later binary's `--version`, and consumers negotiate on `identity`/`schemaVersion`, never on this field). |
| `discoveryCommand` | `lekalo provider describe` (presentation form). |
| `targetProtocolIdentity` | `dev.lekalo.target-protocol@0.3.2`. The adapter protocol is a separate negotiated family: a supported workflow operation never implies a configured target. |
| `schemaPins` | The exact upstream output contract identities: the seven wire-discriminated families — context `lekalo/context/v0.2.16`, diagnostics `lekalo/diagnostic/v0.2.16`, doctor `lekalo/doctor/v0.3.2`, impact `lekalo/impact/v0.2.16`, orchestration `lekalo/orchestration/v0.2.16`, trace `lekalo/trace-manifest/v0.2.16`, validation profile `lekalo/validation-profile/v0.6.3` — plus the two describing schemas of this contract series for the receipt-shaped payloads without embedded discriminators: `lekalo/validation-report/v0.6.3` (`validate` success) and `lekalo/generate-check/v0.6.3` (`drift` receipts). |
| `operations` | The ten operations below, canonical order, with effect class, output schema, and the `requiresProject` / `requiresAdapter` prerequisites. |
| `bounds` | `recommendedContextBudgetTokens` 5000, `maxContextBudgetTokens` 1000000 (`context.MAX_BUDGET_TOKENS`), `maxExportBytes` 33554432 (the 32 MiB impact/trace export bound). |
| `manifestDigest` | `sha256:` over the canonical JSON (sorted keys, no whitespace) of the manifest with this field removed. Integrity metadata, not a signature. |

Unknown manifest fields, unknown operations, and unknown schema pins
are consumer-side `unsupported`. There is no compatibility range and
no alias.

## Operations

Detection is not an operation: it is the manifest plus the consumer's
installed-tool check. The closed operation vocabulary (canonical
order):

| Operation | Command (prescribed argv) | Effect | Output schema | Prerequisites |
| --- | --- | --- | --- | --- |
| `status` | `lekalo status [--project DIR]` | read-only | `lekalo/doctor/v0.3.2` | project |
| `doctor` | `lekalo doctor [--project DIR] [--trace PATH]...` | read-only | `lekalo/doctor/v0.3.2` | project |
| `impact` | `lekalo impact --changed (--base REF [--head REF] \| --worktree) [--project DIR]` | read-only | `lekalo/impact/v0.2.16` | project |
| `context` | `lekalo context --changed SYMBOLS --budget TOKENS [--project DIR]` | read-only | `lekalo/context/v0.2.16` | project |
| `validate` | `lekalo validate --no-cache [--project DIR] [--module MODULE] [--strict]` | read-only | `lekalo/validation-report/v0.6.3` | project |
| `drift` | `lekalo generate --check [--locked] [--project DIR]` | read-only | `lekalo/generate-check/v0.6.3` | project + lock (unconditional: a missing `lekalo.lock` fails with `lock.missing`; `--locked` only adds the freshness check) |
| `generate` | `lekalo generate --target TARGET [--dry-run] [--locked] [--project DIR] -- PROGRAM [ARGS...]` | generated-artifacts | `lekalo/orchestration/v0.2.16` | project + adapter |
| `verify` | `lekalo verify [--target TARGET]... [--module MODULE] [--changed] [--locked] [--trace PATH] [--project DIR]` | read-only | `lekalo/orchestration/v0.2.16` | project |
| `readiness` | `lekalo readiness --phase implement\|generate\|verify\|release\|done [--project DIR] [--trace PATH]...` | read-only | `lekalo/doctor/v0.3.2` | project |
| `trace.export` | `lekalo trace export PATH` | read-only | `lekalo/trace-manifest/v0.2.16` | none |

Effect classes describe the write surface **of the prescribed argv**:

- `read-only` operations never write. For `validate` this is true
  only with the prescribed `--no-cache`: the default cached pipeline
  materializes `.lekalo/cache/cache.sqlite` as an ordinary cache side
  effect, which would corrupt a consumer's input-inventory binding for
  a supposedly read-only phase. Never invoke the cached form from a
  provider run.
- `generated-artifacts` (only the mutating `generate` form) writes
  the adapter-declared managed write scopes verified against the
  ownership plan — legitimately including managed source roots such
  as `src/generated/**` — plus Lekalo's own `.lekalo/generated/**`
  metadata. The protected homes (`openspec/**`, `lekalo/**`,
  `lekalo.lock`, `.lekalo/{ir,cache,import,privacy,consumer}/**`) are
  refused as `target.protected-path` (exit 3).
- The `drift` operation is the read-only check variant of generation:
  it never writes and never needs an adapter.

### Mapping rules and interpretations

- **`status` / `doctor` / `readiness` are reports, not verdicts.** A
  produced report exits 0 even when its `verdict` is `degraded` or
  `blocked`. The consumer must read `verdict` and the closed check
  panel; exit 0 alone is never a pass. `readiness --phase done` is the
  accepted `release` alias and never appears on the wire. The
  readiness phases listed here are the provider-relevant subset; the
  command also accepts `model`, which a workflow consumer has no
  phase for.
- **`doctor` is read-only at this boundary.** `--fix` renders recipe
  previews (advice only) and is unnecessary here; do not pass it.
  Missing `--trace` evidence degrades explicitly.
- **`impact` never parses Git for the consumer.** `--base`/`--head`
  take committed revisions the consumer pins; `--worktree` selects the
  index/worktree candidate. A branch name alone is not evidence. A
  strict-profile denial is exit 3 (`denied`) on stdout. Large or
  incomplete results must not be presented as a complete context
  claim.
- **`context --changed` takes semantic ids, not a Git selector.** The
  consumer extracts the bounded id set from its impact result. Check
  `fits` / `minimumRequired` and the truncation manifest before using
  the capsule; a `fits:false` capsule is data, not a failure, and must
  never be delivered as complete. Use the recommended 5000-token
  budget first; the 1,000,000-token core bound is a hard ceiling, not
  a target.
- **`validate` preserves original registry ids.** Valid success and
  warnings are exit 0 on stdout; the success receipt has no embedded
  `schemaVersion` member and is negotiated through the published
  `lekalo/validation-report/v0.6.3` describing schema
  (`contracts/validation-report.schema.v0.6.3.json`), which covers all
  three reachable shapes: the zero-diagnostic receipt; the
  warning/info-bearing receipt whose envelope carries the closed
  `diagnostics` array plus the derived `reasonCodes`; and the
  default-profile classification-finding receipt, whose recorded review
  rows keep their registered `error` severity on the success envelope
  without invalidating the run (the strict profile invalidates on error
  findings instead — those runs are exit-1 failures outside this
  receipt). The profile definition document stays
  `lekalo/validation-profile/v0.6.3` (configuration, not output).
  Invalid models exit 1 on stderr with the typed diagnostics; a strict
  authorization denial is exit 3 (`denied`) on stdout. A schema-valid
  validation failure is a semantic result, never a provider crash.
- **`drift` (generate --check) has its own receipt contract.** A
  clean or findings-only check is exit 0 stdout with the
  `lekalo/generate-check/v0.6.3` receipt (operation `generate`, mode
  `check`, the exact `lockDigest` binding, verdict, counts, sorted
  non-blocking findings). Receipt findings are exactly the non-blocking
  triple — `stale`, `manual-drift`, `missing` — on entries whose
  lifecycle is one of the non-generated lifecycles (`scaffolded`,
  `checked`, `external`, `custom`); orphans and every generated-
  lifecycle finding block the run instead: exit 1 (stderr) with the
  typed diagnostic (`lock.stale`, `lock.source-changed`,
  `structure.document-missing`, `structure.runtime-unexpected-entry`).
  The receipt is never an orchestration report and is never validated
  against `lekalo/orchestration/v0.2.16`. It reports the
  ownership-manifest/lock/model/artifact state read-only. Real
  generation requires an explicit installed adapter argv after `--`;
  `--target` alone is insufficient. Generation is the only operation
  with write effects, must never run inside detection, done, or any
  read-only phase, and its receipt must be bound by the consumer to
  the exact before/after revisions.
- **`verify` is the read-only host pipeline** (validation, drift,
  per-target adapter verification, binding/scenario/trace receipts).
  Absent optional targets degrade through `unsupported` receipts with
  component states; native gate execution remains outside verify — the
  consumer runs its own native gates separately and never attributes
  them to Lekalo.
- **`trace.export` requires the explicit manifest PATH** and embeds
  the canonical bytes plus `manifestDigest`. A partial trace is valid
  data with recorded gaps; it is never full coverage. Trace collection
  (`lekalo trace collect`) writes `.lekalo/import/trace/` and is a
  separate explicit mutation outside this contract's operations.

## Exit / stream discipline

The envelope is the existing `DomainResult` contract. The consumer
classifies per operation:

| Exit | Status | Stream | Consumer interpretation |
| ---: | --- | --- | --- |
| 0 | `valid` | stdout | Parse the operation's schema and verdict; exit 0 is not a semantic pass by itself (see reports above). |
| 1 | `invalid` | stderr | Malformed usage (`LEK-CLI-001`), input errors, or semantic invalidity. Classify usage versus model failure; never label a schema-valid semantic failure an infrastructure error. |
| 3 | `denied` | stdout | Typed policy/confinement/strict-gate denial; an operation failure with a preserved reason. |
| 4 | `unsupported` / `unavailable` | stdout | Capability absence or degraded verification; use the receipt's component states to distinguish required-missing from optional findings. |
| 5 | `unsupported-version` | stderr | A contract version outside the accepted registry; never an ordinary test failure. |
| other / signal / no valid envelope | — | — | Infrastructure error. Retain bounded stream metadata; never persist raw streams in durable evidence. |

Diagnostics are closed registry items (`lekalo/diagnostic/v0.2.16`,
registry `0.4.0`) whose `code` matches `LEK-SUBSYSTEM-NNN`; preserve
the original ids, codes, severities, and categories when normalizing
into consumer evidence. The normalized consumer gate never overwrites
the native classification.

## Limits and process bounds

Core hard bounds are published in the manifest `bounds` object. The
consumer's own process timeout/output caps may be smaller; when a
result would exceed them, the consumer must surface an explicit
overflow state — never silently truncate JSON or drop diagnostics.
Nested target operations (adapter deadlines inside generate/verify)
must be aligned with the consumer's outer process budget; the native
defaults are longer than a typical consumer cap.

## Privacy

The manifest and every operation output in this contract carry no
environment values, credentials, host identity, absolute paths, or
free-form text. Context capsules and impact/trace exports may contain
project content by design; the consumer's evidence projection remains
governed by the accepted privacy policy (capsules local-private,
provider envelopes shareable-with-redaction, raw tool output
forbidden-to-export). SHA-256 digests are identity metadata, not
anonymization.

## Conformance

- Rust child-process tests: `crates/lekalo-cli/tests/provider.rs`
  (stream/exit discipline, byte-identical determinism, golden
  fixture, digest domain, closed vocabulary, no-side-effect
  discovery, prescribed-argv side-effect tests for `validate` and
  `drift`).
- Node boundary gate: `node scripts/test-provider-contracts.mjs`
  (schema + golden + live binary + digest recomputation + live
  `validate`/`drift` receipts against their describing schemas, Ajv
  8.17.1).
- Schemas: `contracts/provider-capabilities.schema.v0.6.3.json`
  (closed, `additionalProperties:false`, per-operation const tuples
  and exact pin tuples),
  `contracts/validation-report.schema.v0.6.3.json`, and
  `contracts/generate-check-receipt.schema.v0.6.3.json`.
