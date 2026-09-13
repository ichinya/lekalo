# ADR-0032: Doctor, status, and the readiness report

Date: 2026-09-09
Status: accepted for issue #92

Custody: this issue carries the **reserved product candidate 0.2.7**
(RELEASE-POLICY.md per-issue tag order) in every accepted path: the
workspace `Cargo.toml`, both `lekalo` packages in `Cargo.lock`, the
regenerated committed golden lock and its digest sidecar, the
`--version` behavior and its pinning tests, `README.md`, and
`docs/cli.md`. The doctor contract version
(`lekalo/doctor/v1.0.0`, identity `dev.lekalo.doctor@1.0.0`) is
independent of the product release, of the Model/IR/graph/effect/lock
contract versions, and of the diagnostic registry by design. The
reserved diagnostic registry version 1.17.0 is **not consumed**: the
doctor family introduces no new diagnostic rules. Every preserved rule
id it echoes is an already-registered rule of an accepted predecessor
(`structure.*`, `loader.*`, `versioning.*`, `lock.*`); doctor is a
projection over existing rules, not a rule publisher.

## Context

Issue #92 asks for one command that diagnoses the readiness of a
Lekalo project — project root/layout, Model/schema/IR version
compatibility, imports/references, lockfile freshness, installed
adapter versions/digests, capability/profile resolution, cache health,
binding/index freshness, generated artifact drift, native tool/gate
availability, optional OpenSpec/HLV/AI Factory integration status,
filesystem permissions/path confinement, and platform limitations —
before planning, implementation, generation, verification, or done.
Readiness must distinguish required from optional checks per phase,
explain blockers and degraded items, keep doctor read-only by default,
never repair/install/update in secret, emit a versioned JSON result an
AIFHub Extension can consume, preserve the original provider/adapter
diagnostics, carry the exact git/model/lock revision, keep a stable
exit policy, and disclose no secret or environment values.

## Decision

### 1. One closed document family for three commands

`lekalo doctor`, `lekalo status`, and `lekalo readiness --phase PHASE`
project one document
([`contracts/doctor.schema.v1.0.0.json`](../../contracts/doctor.schema.v1.0.0.json),
discriminator `lekalo/doctor/v1.0.0`). The report kind selects the
panel: `doctor` runs the full thirteen-check vocabulary, `status`
runs the freshness quartet (lock, cache, bindings, artifacts) plus the
revisions block, `readiness` runs the full panel and marks the
phase-required checks. The document carries `status: "valid"` in
receipt house style: the report is the product.

### 2. Read-only by default and forever

Doctor is a projection, never a repair: every internal load runs with
the cache bypassed (`#20` pipeline bypass path), the lock check only
reads and verifies (`#10` `LockVerifier` with the `Optional`
requirement over an empty inventory), the artifact check is the
accepted read-only `generate --check`, the cache health probe reports
a missing home without creating it, and the platform probe only stats
existing paths. Nothing spawns an adapter, applies a plan, or writes a
byte. `--fix` renders the closed safe-fix recipes as a preview —
advice with a `mutating` flag and bounded steps — and executes
nothing. The mutating recipes name the exact explicit CLI
confirmations (`lekalo lock`, `lekalo update --apply`, `lekalo cache
clear --yes`, `lekalo migrate --rollback`) a human must run.

### 3. Closed checks, closed reasons, closed recipes

Thirteen check ids form the closed panel. Each check state is one of
`ok`, `degraded`, `blocked`, or `unknown` (could not run because an
upstream input was unavailable), with a closed reason token that
distinguishes the recorded states (for example `fresh`/`stale`/
`absent`/`invalid` for the lock, `clean`/`drift`/`stale` for
artifacts, `missing`/`empty`/`ok`/`corrupt`/`quarantined`/`locked`
for the cache) and exactly one closed next-action recipe id when the
state is not `ok`. Missing required adapters/profiles (declared via
`lekalo/targets/**` with an empty resolved inventory) are blockers
with `resolve-adapters`; stale/absent locks and drifted artifacts
degrade with their recipes. The bounded `diagnostics` array echoes
the preserved registry rule ids of the underlying refusal — the
original provider/adapter/lock/loader diagnostics survive verbatim as
rule identities, never re-summarized into new ones.

### 4. Verdict rule and stable exit policy

The verdict is derived, never hand-set: `blocked` when a required
check is blocked or unknown (for `doctor`, any blocked or unknown
check; for `status`, any blocked check), `degraded` when any check is
not `ok`, `ready` otherwise. Missing optional evidence — HLV gate
evidence above all — degrades and is never a core failure. The exit
policy is stable and boring: a produced report is `valid` (exit 0,
stdout), whatever verdict it records, because the JSON is the product
an agent consumes; only a malformed invocation is the usage failure
(exit 1, stderr). CI gating per concern stays with the existing
gates (`validate`, `lock --check`, `generate --check`); doctor
aggregates and explains, it does not re-gate them.

### 5. Exact revisions, no host disclosure

The revisions block carries the exact Git facts (state, the exact
commit identity grammar-validated as sha1/sha256 hex, the dirty
boolean), the exact Model contract version, and the exact lock state
and payload digest. Git is invoked only by the read-only CLI-edge
adapter (`doctor_git.rs`, the same architectural seam as the issue
#16 Git adapter) under a bounded five-second wait, with argv only and
stderr discarded; timeout maps to `unavailable`. No absolute paths,
no environment values, no host identity, no branch or file names, and
no secret material appear anywhere in the document.

### 6. Optional integrations as typed evidence

OpenSpec/HLV/AI Factory status enters as `--trace PATH` evidence: the
CLI reads each named manifest, the core's own `TraceManifest` parser
makes every decision, and a typed evidence handoff (reason ids, gap
count, sorted externalRef source tokens) crosses into the report.
Unsupplied evidence degrades (`not-supplied`) with the
`trace-validate` recipe; supplied-but-invalid manifests block the
check with the preserved wire diagnostics; gaps degrade with the
citation tokens (`ref-hlv`, `ref-openspec`, `ref-aifhub`,
`ref-source-native`, `ref-lekalo`) as closed notes.

### 7. Platform limitations are informational

`platform.limits` never blocks. The case-sensitivity token is probed
read-only (the case-flipped project root spelling resolves on a
case-insensitive volume); the path-grammar tokens
(`windows-path-syntax`/`posix-path-syntax`), the Windows
`symlink-privilege-required`, and the macOS
`unicode-normalization-sensitive` facts are static per target
platform. Together with the confinement check (which splits the
`#4` structure outcome into layout and `structure.path-*`
confinement halves and probes the read-only flag), this covers the
Windows/Linux/macOS path and tool diagnostics.

## Consequences

- `cache status` no longer creates `.lekalo/cache` on a bare run: the
  health projection is now strictly read-only, matching its published
  description. The load pipeline owns home creation.
- The doctor document is a new independent contract family; it is
  validated by the Rust suite, the pinned golden fixtures under
  `tests/fixtures/doctor/`, and the `scripts/test-doctor-contracts.mjs`
  Node gate (schema, closed invariants, preserved-registry-id
  cross-check) on Node 18 and 24 in CI.
- The check vocabulary, reasons, recipes, and notes are closed; adding
  one is a doctor contract version change, not a free-form extension.
