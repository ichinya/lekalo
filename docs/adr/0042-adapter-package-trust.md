# ADR-0042: adapter package manifest, discovery, install and trust model

Status: accepted (issue #32). Builds on ADR-0003 (canonical structure and
path safety), ADR-0009 (lockfile), ADR-0010 (diagnostics), ADR-0015
(artifact ownership), ADR-0025 (target protocol), ADR-0029 (composable
target profiles), ADR-0030 (adapter conformance), and ADR-0033 (the
foreign-implementation escape hatch). Consumers: #89 (execution
isolation), #119 (privacy enforcement), and the future AIFHub registry
import.

## Context

Issue #32 asks for a secure delivery model for third-party target/profile
adapters without hidden installation or execution of unverified code.
Today an adapter reaches the core only as an invocation-supplied argv
vector (`-- PROGRAM`), is characterized through the safe describe
handshake (ADR-0025), and is bound to `lekalo.lock` pins (ADR-0009). What
is missing is the custody chain: no declared manifest, one discovery
source, no install machinery, no trust model, no checksum-before-
execution gate, and no provenance beyond the existing lock digests.

## Decision

### 1. One closed manifest contract, layered on the existing handshake

`contracts/adapter-manifest.schema.v0.3.2.json`
(`lekalo/adapter-manifest/v0.3.2`, identity
`dev.lekalo.adapter-manifest@0.3.2`) is a closed Draft 2020-12 document
carrying every field the issue lists: identity (`id`/`name`/`version`),
publisher and trust anchor, source custody (closed coordinate tokens,
never URLs), license, exact-set protocol/IR compatibility, declared
capabilities, executable coordinates, supported platforms, integrity
(per-file checksums, package digest, signature policy), least-privilege
permissions, hooks, the #31 conformance report reference, and lifecycle
status. The manifest is the independent claim; the describe response
remains the self-asserted one. Discovery cross-checks the two and refuses
a mismatch, so self-assertion alone never qualifies an adapter.

The immutable identity triple is `{adapter.id, adapter.version,
integrity.packageDigest}`: the same `{id, version}` with a different
digest is a distinct package, never an in-place edit.

### 2. Total manifest gate; bare argv synthesizes a local-development
descriptor

Every execution path routes through one resolution gate
(`adapter_package::resolve`): source candidates → manifest decode and
validation → compatibility gate → integrity gate (checksum **before any
child process exists**, describe included) → trust and revocation gate →
describe → manifest-vs-describe consistency → selection. A bare
`-- PROGRAM` argv synthesizes an implicit unsigned local-development
descriptor from the launched entry, so the gate is total without breaking
any shipped flow (`generate`/`verify`/`scan`/`lock`/`adapter test`).

### 3. Five-level trust vocabulary, one quarantine custody

Trust levels, spelled exactly as #89 consumes them: `builtin`,
`verified`, `local-development`, `community`, `revoked`. Trust never
widens runtime permissions; it only selects candidacy, confinement
profile strictness, and auto-selection eligibility. Revocation is a local
append-only store consulted before selection **and** at lock
verification; it overrides every other signal, including `builtin`. A
manifest's own `status`/`revocation` is publisher evidence, never
self-healing.

Quarantine is a state, not a flag: staged community/unknown bytes live
under `.lekalo/adapters/quarantine/**` under opaque digest-derived
bounded names (the cache-quarantine precedent), are never executable and
never reach a lock pin until an explicitly confirmed install plan
promotes them.

### 4. Digest always, signatures honestly unsupported in v1

Checksum verification is unconditional. The signature policy is closed
(`unsigned|optional|required`) and declared per manifest; v1 ships the
full schema and **no cryptographic verifier**, so a `required` policy
answers `adapter.signature-unverified` (unavailable) rather than
inventing a pass. No network fetch exists in v1: release and registry
sources resolve only from explicit local records, which makes `--offline`
exact.

The digest domain excludes the self-referential `manifestDigest` member
**recursively** (canonical bytes minus every member of that name, at any
nesting depth), with `integrity.packageDigest` zeroed for the manifest's
own framed contribution. Both implementations — the Rust verifier and
the JS package generator — apply the identical rule; a nested
`manifestDigest` inside a JSON-typed member is outside the domain on
both sides.

### 5. Previewed-and-confirmed atomic install into a governed store

Installs go through a deterministic install plan (`planId` over the
canonical plan document) that must be confirmed byte-exactly before
anything is written (`install/update/rollback --dry-run | --confirm
sha256:PLAN_ID`). Apply stages under `.lekalo/adapters/staging/**`,
re-verifies every file digest, atomically renames into
`.lekalo/adapters/packages/<id>/<version>-<digest8>/`, and repoints the
`selected` pin in `.lekalo/adapters/inventory.json` — the only mutable
field; package bytes are immutable. Rollback is the same machinery
against an already-installed version. Permission-widening update diffs
are flagged in the plan and refused without `--allow-escalation`.

### 6. Provenance rides the lock and the evidence tree

The lock's `adapters[]` entries gain additive members `manifest_digest`,
`trust`, and `provenance {source, install_plan_id?}`, plus a new
`source.kind: installed` spelling for store paths (a project-relative
POSIX id, never absolute). Discovery/install receipts live under
`.lekalo/adapters/evidence/**` as `lekalo.adapter-evidence` authority
kinds. Four new kinds are registered in the authority matrix
(`lekalo.adapter-package`, `lekalo.adapter-inventory`,
`lekalo.adapter-quarantine`, `lekalo.adapter-evidence`) with local-private
privacy defaults; manifest documents default
`consumer-repository-only`.

### 7. No scripts by default is structural

`hooks[]` is schema-complete but pinned empty in this contract version;
an install plan refuses a non-empty declaration with
`adapter.hooks-declared` until #89's confined runner owns hook execution.
The shipped adapter's poison `package.json` scripts remain the reference
pattern.

## Consequences

- The diagnostic registry gains the closed `adapter.*` refusal family
  (LEK-ADP-006+) mapping onto the accepted 0/1/3/4/5 envelope.
- Selection gains trust as a **filter before** the deterministic
  ordering (stable reasons `trust-revoked`/`trust-quarantined`), never a
  sort key.
- AIFHub can later import the manifest plus conformance evidence through
  the existing `registry` source kind and `catalog` lock kind without a
  core protocol change.
- Versioning note: this worktree carries prospective product 0.3.2 and
  the repo's release-reserve process owns version bumps, so the new and
  changed contract families in this issue take the product version
  0.3.2 (the plan's 0.4.0 literal predates that reading of the version
  policy and is recorded here as the one deliberate deviation).
