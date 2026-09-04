# The committed `lekalo.lock` (issue #10)

`lekalo.lock` makes generation and verification reproducible: it freezes the
exact product, contract, adapter, generator, profile, and capability
identities that participated in a result. Two environments holding the same
lock resolve the same components, byte for byte.

## Identity and format

- The lock is a **physical regular file** at the accepted project home
  (`lekalo.lock` beside `lekalo/`), committed together with `lekalo/**`.
- The wire is **strict canonical JSON**: compact, UTF-8 without BOM, no
  comments, no duplicate keys, no floats, object keys in unsigned UTF-8 byte
  order, semantic arrays in the documented sorted order, and exactly one LF
  at the end of the file. (Strict JSON is also a YAML 1.2 subset; no second
  YAML canonicalization exists.)
- The schema artifact is `contracts/lock.schema.v1.0.0.json`
  (Draft 2020-12, closed), identity `dev.lekalo.lock@1.0.0`; the wire
  discriminator is `schema_version = "lekalo/lock/v1.0.0"`. These versions
  are independent of the product release, the Model/IR/protocol contract
  versions, and the resolver algorithm version.
- `LockDigest` is SHA-256 over the canonical payload bytes **without** the
  final LF, so the file never carries its own digest.
- Parsing a semantically equivalent but noncanonical document succeeds only
  far enough to classify it `lock.noncanonical` (exit 1). `lock --check` and
  every `--locked` path never rewrite it.

## Closed sections

Top-level keys, in wire (byte-sorted) order: `adapters`, `capabilities`,
`contracts`, `core`, `generators`, `profiles`, `resolver`,
`schema_version`.

- `resolver.version` is the independent resolver algorithm SemVer (1.0.0 in
  v1). `resolver.request_digest` is SHA-256 over the canonical typed,
  path-free resolution request (registry identity/version, Model/IR/protocol
  versions, core version, requested target, adapter/generator/profile ids,
  required capabilities, the partial-support policy, exact prerelease
  opt-ins, and consumed catalog digests). `resolver.catalogs` lists immutable
  candidate-catalog snapshot identities — never URLs or credentials.
- `core.version` is the exact Lekalo product/CLI version that wrote the
  lock. It is pinned but not request-bound: a product patch release does not
  make a lock stale. No cross-OS core binary digest exists in v1 (issue #104
  owns release provenance).
- `contracts` pins `registry`, `model`, and `ir` as `{version, digest}`;
  `target_protocol` is `null` while the protocol family is unpublished. When
  it is `null`, `adapters`, `generators`, and `capabilities` must be empty.
- `adapters` (sorted by `id`) carries `{id, version, digest, source,
  compatibility_digest, artifacts}`. `digest` covers the immutable
  platform-neutral adapter package/manifest root; `source` is a closed
  `{kind, id, digest}` with `kind ∈ {builtin, catalog, project}` where a
  project `id` is a #4-safe project-relative POSIX path; `artifacts` is a
  non-empty array sorted by `platform` (`any` or a stable lowercase target
  triple).
- `generators` (sorted by `id`) are independently versioned logical
  implementations inside an adapter package; `adapter` must reference the
  selected adapter exactly.
- `profiles` (sorted by `id`) carry an exact version (a fabricated `0.0.0`
  never exists on the wire; an unversioned profile fails
  `lock.profile-unversioned`), `source_digest` over the canonical declared
  input, `digest` over the fully resolved, inheritance-applied, path-free
  snapshot, and sorted component references.
- `capabilities` is the one resolved snapshot sorted by
  `target/profile/id/version/provider`; `support ∈ {full, partial,
  unsupported, unknown}`; `unknown`/`unsupported` never satisfies a required
  capability, `partial` only with the explicit accepted policy.

## Digest domains

Only `sha256:<64 lowercase hex>` is accepted. Digest values are never
compared across domains. The v1 domains are:

| Field | Domain |
| --- | --- |
| `contracts.registry.digest` | exact embedded registry artifact bytes |
| `contracts.model.digest` | exact committed schema bytes for that version (`contracts/model.schema.v<version>.json`) |
| `contracts.ir.digest` | exact contract identity line `dev.lekalo.ir@<version>\n` |
| `contracts.target_protocol.digest` | exact protocol contract identity line (after #27 publishes the family) |
| `adapters.digest` | adapter package/manifest root bytes |
| `adapters.source.digest` | exact source snapshot bytes |
| `adapters.compatibility_digest` | canonical JSON of the typed #9 compatibility manifest |
| `adapters.artifacts[].digest` | exact executable/bundle bytes for that platform |
| `generators.digest` | canonical implementation descriptor/bundle bytes |
| `profiles.source_digest` / `profiles.digest` | canonical declared profile bytes / resolved profile snapshot bytes |
| `resolver.request_digest` | canonical typed request bytes (below) |
| `planId` | canonical plan-identity composition (below) |

## Resolution (pure and hermetic)

`LockResolver::resolve` consumes one typed `ResolutionRequest`, one sealed
`CandidateSet`, and the embedded #9 registry. It performs no filesystem,
`PATH`, registry, network, install, process spawn, or profile parsing; the
real candidate supply arrives with #27–#32. Filter order is fixed:

1. exact-set registry support of the request's contract versions;
2. the protocol-publication gate (with the protocol unpublished, nothing
   executable may be requested or even offered as a candidate —
   `versioning.protocol-unpublished`, exit 5);
3. catalog availability of every consumed snapshot (`lock.catalog-unavailable`);
4. per-adapter #9 compatibility preflight (IR range, protocol range,
   required extensions — `versioning.adapter-incompatible`,
   `versioning.extension-incompatible`, exit 5);
5. the requested target platform (`lock.platform-unavailable`, exit 4);
6. required capability support (`lock.component-unavailable`, exit 4);
7. platform artifact availability.

Selection takes the highest stable exact SemVer per identity; prereleases
require the exact spelling in the request's opt-in list. Two indistinguishable
candidates at one identity/version are `lock.resolution-ambiguous` (exit 1),
never a source-order choice. A newer available candidate never makes an
existing lock stale; only a changed request or contract does (`lock.stale`).

## Update: preview then apply

- `lekalo update --dry-run` is pure: it resolves against the sealed supply,
  computes the canonical after bytes, the structured sorted
  add/remove/change entries, and `planId`, and writes nothing — no
  `.lekalo` directory, no guard, no stage file. An already-current project
  previews `changed: false`.
- `planId = SHA-256` over the canonical JSON
  `{after, before, catalogs, request, resolver}` where `before` is the
  before-digest spelling or `"absent"`, `catalogs` is the ordered catalog
  digest list, `request` is the request digest, and `resolver` is the
  resolver version. Repeated identical inputs produce byte-identical plans.
- `lekalo update --apply sha256:<planId>` recomputes the plan, requires the
  exact identity (`lock.source-changed` otherwise), acquires the runtime
  guard, revalidates the pinned before state, stages create-new in the
  lock's own directory, verifies the bytes, renames atomically, and re-reads
  the committed file (an ambiguous post-replace state is
  `lock.recovery-required`). An already-current apply writes nothing and
  reports `changed: false`.
- The runtime guard is an exclusive OS lock on the empty file
  `.lekalo/cache/locks/lockfile-update` (distinct from the #9 migration
  lock, never committed, contents empty): `flock` on Unix,
  share-mode-zero open on Windows. The kernel releases it on process death;
  contention is `lock.update-in-progress`.

## Locked and offline

`lock --check` is the headless CI gate: present, canonical, supported,
request-current, contract-verified — it never creates or updates.
`LockRequirement::Required` (exported for #91, which owns the
`generate --locked` / `verify --locked` flags) additionally demands every
locked component and platform artifact in the local inventory before any
runner, cache, or output is created: missing components are
`lock.component-unavailable` / `lock.platform-unavailable` (exit 4). The
`--offline` flag forbids non-local candidate supply; with the provider seam
not yet in the tree (#27–#32), the sealed local supply is already offline.

## Tamper detection

Every declared digest is recomputed from its domain. A changed adapter
binary under the same version, a replaced artifact, or edited contract
bytes are integrity denials — `lock.digest-mismatch`, exit 3, stdout —
classified separately from validation failures (exit 1) and from
unavailability (exit 4). The only recovery is an explicit reviewed update
to the new digest; nothing auto-repairs or trusts the declared version.

## Exit and stream contract

| Exit | Status | Stream | Lock reasons |
| ---: | --- | --- | --- |
| 0 | `valid` | stdout | success |
| 1 | `invalid` | stderr | `lock.missing`, `lock.schema-invalid`, `lock.noncanonical`, `lock.reference-invalid`, `lock.resolution-ambiguous`, `lock.profile-unversioned`, `lock.stale`, `lock.preview-required`, `lock.source-changed`, `lock.update-in-progress`, `lock.commit-failed`, `lock.recovery-required` |
| 3 | `denied` | stdout | `lock.path-denied`, `lock.private-data-forbidden`, `lock.digest-mismatch`, passthrough `structure.*` denials |
| 4 | `unavailable` | stdout | `lock.catalog-unavailable`, `lock.component-unavailable`, `lock.platform-unavailable`, `lock.resolution-provider-unavailable` |
| 5 | `unsupported-version` | stderr | `lock.unsupported-schema-version`, `lock.component-version-unsupported`, reused `versioning.protocol-unpublished`, `versioning.adapter-incompatible`, `versioning.extension-incompatible` |

Failures render as `{"status": ..., "reasonCodes": [...]}` (pretty, two
spaces, one LF). The diagnostics carry only stable ids, versions, digest
domains, and logical paths — no raw OS text, absolute paths, timestamps,
host, or environment data. `lock.*` private-data refusals cover URLs,
UNC/drive/absolute shapes, `..`/`//` traversal, control characters, and
credential-shaped tokens.

## Gate

- `node scripts/test-lockfile-contracts.mjs` — hermetic contract gate
  (schema shape, canonical bytes, independently recomputed digests, closed
  wire, invalid fixtures, registry parity).
- `node scripts/test-lockfile-ajv.mjs` — the same fixtures against the
  pinned Draft 2020-12 validator (exact Ajv 8.17.1, provisioned outside the
  checkout exactly like `scripts/test-model-ajv.mjs`).
- `cargo test --workspace --locked` — wire, resolution, verification, plan,
  and CLI behavior on the accepted 0/1/3/4/5 envelope.
