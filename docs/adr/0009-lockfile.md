# ADR-0009: The committed `lekalo.lock` and reproducible resolution

Date: 2026-09-04
Status: accepted for issue #10

Custody: this issue carried the prospective product candidate 0.1.8 in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock`, the `--version` behavior and its pinning tests, `README.md`,
`docs/cli.md`); the candidate was published as product 0.1.8 (annotated
tag on `8ddbbf0`). Issue #9 published product 0.1.7 at `f0b3784`; issue
#11 published product 0.1.9 at `5b885bf`; #12 published product
0.1.10 at `fdfbcb5`; issue #13 published product 0.1.11 at `007c01d`; issue
#14 published product 0.1.12 at `81666da`; issue #23 published product
0.1.20 (annotated tag `v0.1.20` on `eef1863`); issue #22 published
product 0.1.19 (annotated tag `v0.1.19` on `31468e9`); issue #15
published product 0.1.21 (annotated tag `v0.1.21` on `9ab5b07`);
issue #21 published product 0.1.22 (annotated tag `v0.1.22` on
`2dab70e`); issue #20 published product 0.1.23 (annotated tag
`v0.1.23` on `15be55a`); issue #16 now carries prospective product
0.1.24. The lock
schema version (`lekalo/lock/v1.0.0`, identity `dev.lekalo.lock@1.0.0`), the
resolver algorithm version (1.0.0), and every contract version remain
independent of the product release by design.

## Context

Issues #3–#9 shipped the result envelope, the physical project rules, two
Model versions, the loader, the typed IR, and versioning with an embedded
registry. Nothing yet froze the exact component identities behind a result:
generation was not reproducible, CI had no immutable mode, and adapter
package identity did not exist. Issue #10 owns only the lock surface: the
closed wire, the pure resolution, verification, preview/update, and the
read-only preflight seam downstream commands call.

## The eight recorded owner decisions

1. **Format.** v1 is strict canonical JSON with the discriminator
   `lekalo/lock/v1.0.0` and the schema identity `dev.lekalo.lock@1.0.0`;
   general YAML and the issue's illustrative `lekalo/lock/v1` shorthand are
   rejected. One canonicalization, one spelling per value, one LF.
2. **Provider seam.** #10 lands the lock schema, the pure resolver/verifier/
   writer, and a contract-only CLI with injected synthetic multi-adapter
   acceptance; #27–#32 later supply real adapters, profiles, and packages.
   The resolver is hermetic: no filesystem, network, install, or process
   access, only sealed typed inputs.
3. **Generator identity.** A generator is an independently versioned logical
   implementation inside an adapter package, with adapter-owned execution;
   a separately installable generator would belong to #32 and require a
   schema successor.
4. **Profile versions.** Exact SemVer is required, plus `source_digest` and
   resolved `digest`; a fabricated `0.0.0` never serializes
   (`lock.profile-unversioned`).
5. **Platform digests.** One platform-neutral package-root digest plus
   sorted per-platform artifact digests detects same-version binary
   replacement while keeping the lock portable; #32 later owns discovery
   and signing of artifacts.
6. **Update acceptance.** Two-step only: `--dry-run` prints the plan and its
   deterministic `planId`; `--apply sha256:<planId>` re-plans, requires the
   exact identity, and applies under a byte-level compare-and-swap. A plain
   mutating update is `lock.preview-required`. Major or prerelease movement
   is never inferred.
7. **Command ownership.** #10 owns `lekalo lock` (create/check), `lekalo
   update`, `LockRequirement`, and `lock --check`; #91 owns the
   `generate --locked` / `verify --locked` flags and must reuse the same
   `Required` verifier rather than reimplementing it. No orchestration
   stubs were added.
8. **Release boundary.** The lock schema, resolver, and component versions
   are independent of the product release. The product version only moves
   through the established custody process; this issue carried prospective
   0.1.8 without tagging, publishing, or touching release automation
   (#104).

## Decision highlights

### Closed wire with exact digest domains

Every section is closed (`additionalProperties: false`), every version is
one canonical SemVer spelling without build metadata, and every digest is
`sha256:<64 lowercase hex>` bound to a documented domain (registry artifact
bytes, committed Model schema bytes, IR/protocol identity lines, package
roots, artifacts, manifests, profile inputs and snapshots, the canonical
request, and the plan identity). Digests are never compared across domains.

### Canonical bytes or refusal

A lock parses only when the bytes equal the canonical payload (with or
without exactly one trailing LF). Anything else that still parses to the
same value — reordered keys, whitespace, duplicate keys, CRLF, extra LFs —
is `lock.noncanonical` and is never rewritten. Comments, BOMs, unknown
fields, malformed versions/digests are `lock.schema-invalid`; a future
discriminator is `lock.unsupported-schema-version` (exit 5).

### Deterministic resolution and tamper detection

Filter order is fixed; selection takes the highest stable exact SemVer with
explicit prerelease opt-in; ambiguity is a refusal. Verification recomputes
every digest from its domain before anything runs: changed bytes under the
same version are `lock.digest-mismatch` (exit 3, stdout), missing local
components under `Required` are unavailable (exit 4), and a changed request
is `lock.stale` (exit 1). `lock --check` is the headless CI gate; #91 owns
the `--locked` flags and reuses `LockRequirement::Required`.

### Capability-safe single-file transaction

Apply acquires an OS-owned guard on the empty runtime file
`.lekalo/cache/locks/lockfile-update` (flock on Unix, a share-mode-zero
open on Windows — the kernel releases both on process death; the file
carries no PID, path, or identity), revalidates the pinned before bytes,
stages create-new beside the lock, verifies the staged bytes, renames
atomically, flushes the parent, and re-reads the committed file. A plan
whose before state drifted is `lock.source-changed` with zero writes; the
stage file is the only other file ever touched, and it is removed on every
failure path. Private data — absolute/UNC/drive paths, URLs, traversal,
credentials — is refused as `lock.private-data-forbidden` (exit 3) before
serialization.

## Consequences

- Two environments with the same lock resolve byte-identical components;
  CI falls on missing/stale locks via `lock --check`; the update surface
  cannot change a lock without an explicit bound plan.
- Real adapter, profile, and package populations arrive with #27–#32; until
  then the CLI creates and checks contract-only locks and the multi-adapter
  machinery is exercised hermetically through typed candidate sets and a
  synthetic published-protocol registry.
- The lock is never a runtime mutex, migration journal, package cache, or
  release attestation; #9 keeps `.lekalo/cache/migrations`, #104 keeps
  release provenance, and #21 may later bind artifacts to the lock digest.
