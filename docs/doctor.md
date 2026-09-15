# Doctor, status, and the readiness report (issue #92)

One command diagnoses the readiness of a Lekalo project — project
root and layout, Model/schema/IR version compatibility, imports and
references, lockfile freshness, installed adapter versions and
digests, capability and profile resolution, cache health, binding and
index freshness, generated artifact drift, native tool and gate
availability, optional OpenSpec/HLV/AI Factory integration evidence,
filesystem permissions and path confinement, and platform limitations
— before planning, implementation, generation, verification, or done.
The contract is
[`contracts/doctor.schema.v1.0.0.json`](../contracts/doctor.schema.v1.0.0.json)
(discriminator `lekalo/doctor/v1.0.0`, identity
`dev.lekalo.doctor@1.0.0`), independent of the product release and of
every other contract family. The recorded owner decisions live in
[ADR-0032](adr/0032-doctor-readiness.md).

## Commands

```sh
lekalo doctor [--project DIR] [--trace PATH]... [--fix]
lekalo status [--project DIR]
lekalo readiness --phase model|implement|generate|verify|release [--project DIR] [--trace PATH]...
```

`--json` is global and renders the document; the human projection is
one stable summary line. `readiness` also accepts the `done` alias of
`release`. All three commands are read-only: internal loads bypass the
cache, the lock check verifies without updating, the artifact check is
the accepted read-only `generate --check`, and a missing cache home is
reported without being created. Nothing spawns an adapter, repairs,
installs, or updates.

## Checks

Thirteen closed check ids form the full `doctor` panel; `status`
reports the freshness quartet only; `readiness` marks the
phase-required checks.

| Check | Answers | Required for |
| --- | --- | --- |
| `project.root` | The selection resolves; the layout validates | every phase |
| `model.version` | The exact Model version is in the accepted registry | `model` |
| `model.references` | Imports, references, and semantic rules are clean | `model` |
| `fs.confinement` | No path-confinement violation; root not read-only | `model` |
| `lock.freshness` | The committed lock is fresh (`fresh`/`stale`/`absent`/`invalid`) | `implement` |
| `adapters.inventory` | Locked adapter/generator versions and digests resolve | `generate` |
| `capabilities.profiles` | Locked capability/profile resolution is satisfied | `generate` |
| `bindings.freshness` | Bindings validate; declared generators have derived bindings | `generate` |
| `artifacts.drift` | The ownership manifest matches the exact bytes | `generate` |
| `tools.gates` | Git (the one native tool) is available | `verify` |
| `cache.health` | The cache is `ok`/`missing`/`corrupt`/`quarantined`/`locked` | optional |
| `integrations.hlv` | Supplied trace evidence is current (`openspec`/`hlv`/`aifhub`) | optional |
| `platform.limits` | Closed platform facts (case sensitivity, path grammar) | optional |

Every check carries one state (`ok`, `degraded`, `blocked`, or
`unknown` — the check could not run because an upstream input was
unavailable), a closed reason token that distinguishes the recorded
states, one closed next-action recipe id when the state is not `ok`,
the bounded preserved registry rule ids of any underlying refusal, and
closed informational notes where applicable. Adding to any of these
vocabularies is a doctor contract version change.

## Verdict and exit policy

The verdict is derived: `blocked` when a required check is blocked or
unknown (for `doctor`, any blocked or unknown check; for `status`, any
blocked check), `degraded` when any check is not `ok`, `ready`
otherwise. Missing optional evidence — HLV above all — degrades and is
never a core failure. A produced report is `valid` (exit 0, stdout)
whatever verdict it records: the versioned JSON is the product an
agent consumes, and per-concern CI gating stays with `validate`,
`lock --check`, and `generate --check`. Only a malformed invocation is
the usage failure (exit 1, stderr).

## Revisions and evidence

The revisions block carries the exact Git facts (state, grammar-
validated commit identity, dirty flag), the exact Model contract
version, and the exact lock state and payload digest. Git runs only in
the read-only CLI-edge adapter under a bounded five-second wait; its
stderr is discarded and a timeout maps to `unavailable`. No absolute
paths, environment values, host identity, or secret material appear in
any report.

Optional OpenSpec/HLV/AI Factory status enters as `--trace PATH`
evidence: each named manifest is parsed by the core's own trace
validator, and the check reports `evidence-current`, degraded gaps, or
blocked wire refusals with the preserved rule ids. Unsupplied evidence
degrades — it is never a core failure.

## Safe-fix recipes

`--fix` renders the closed recipe preview (advice only, with a
`mutating` flag and bounded steps):

```json
{
  "id": "create-lock",
  "mutating": true,
  "steps": [
    "Move the unusable `lekalo.lock` aside (invalid state only).",
    "Run `lekalo lock` to create the committed lock.",
    "Commit `lekalo.lock` with the project."
  ]
}
```

`id`: `fix-structure`, `create-lock`, `preview-lock-update`,
`validate`, `migrate-model`, `resolve-adapters`, `regenerate`,
`clear-cache`, `recover-migration`, `install-git`, `init-git`,
`grant-write`, `trace-validate`. Doctor executes nothing; the mutating
recipes name the exact explicit CLI confirmations a human must run.

## Example

```sh
lekalo doctor
# doctor degraded : 13 checks (12 ok, 1 degraded, 0 blocked)

lekalo status --json
# {
#   "status": "valid",
#   "schemaVersion": "lekalo/doctor/v1.0.0",
#   "identity": "dev.lekalo.doctor@1.0.0",
#   "report": "status",
#   "productVersion": "0.2.7",
#   "verdict": "ready",
#   "revisions": { "git": {...}, "model": {...}, "lock": {...} },
#   "checks": [ ...freshness quartet... ]
# }

lekalo readiness --phase generate
# readiness generate blocked : 13 checks (10 ok, 2 degraded, 1 blocked)
```

The wire is pinned by the golden fixtures under
`tests/fixtures/doctor/` and validated by
`scripts/test-doctor-contracts.mjs` (schema, closed invariants,
preserved-registry-id cross-check) on Node 18 and 24 in CI.
