# Generated-artifact ownership manifest and drift detection (issue #21)

Every generated, scaffolded, checked, external, and custom artifact is
recorded in one derived ownership manifest that binds the artifact to its
semantic owner, the exact locked adapter and generator identity, the exact
lockfile revision, the exact generation inputs, and the SHA-256 of the
file's observed bytes. `lekalo generate --check` is the read-only drift
gate; `lekalo generate --clean` previews and, only on an exact
confirmation, applies a deterministic clean of orphaned generated files.

## Contract and location

- Wire contract:
  [`contracts/artifact-manifest.schema.v1.0.0.json`](../contracts/artifact-manifest.schema.v1.0.0.json),
  discriminator `lekalo/artifact-manifest/v1.0.0`, identity
  `dev.lekalo.artifact-manifest@1.0.0` — independent of the product
  release, of the Model/IR/graph/lock/protocol contract versions, and of
  the diagnostic registry.
- The manifest is Lekalo-owned runtime-derived data under the accepted
  authority kind `lekalo.generated-intermediate`, stored at
  `.lekalo/generated/manifests/ownership.json`. It is never a canonical
  model file, lockfile, cache database, report bundle, or release
  attestation, and never the sole copy of a semantic decision.

## Canonical form and digest domains

- Compact UTF-8 JSON, no BOM, no comments, object keys in unsigned UTF-8
  byte order; the file is the payload plus exactly one LF.
- `artifacts` are sorted by `(semantic_owner, path, artifact_kind)` bytes
  with unique keys; `source_maps` by the same key bytes; map entries by
  `(start, end, semantic_id)`; `input_refs` sorted and unique. Array order
  is part of the closed format; unsorted or duplicate identities are
  `lock.reference-invalid`.
- `manifest_digest` is SHA-256 over the canonical payload bytes of the
  document with the `manifest_digest` property removed and without the
  final LF: non-self-referential by construction. A tampered document is
  an integrity denial (`lock.digest-mismatch`, exit 3), never silently
  accepted as a new baseline.
- Every artifact `content.digest` is SHA-256 over the file's exact
  observed bytes (`canonicalization: exact-file-bytes`). Newline,
  encoding, and whitespace changes are drift, never normalized away.

## Bindings

- `lock_ref` pins the exact `LockDigest` of the generating lockfile
  revision; a manifest from any other revision is stale as a whole
  (`lock.stale`, exit 1) and must be regenerated, never patched.
- `inputs` pins the exact canonical Model payload (version + digest) and
  canonical typed-IR payload (version + digest). Source maps carry an
  `input_revision` equal to the digest over the recorded inputs, binding
  every range to the exact generation inputs.
- `adapter_ref` deep-equals the resolved lock component: id, canonical
  version, package digest, every platform artifact pin, and the locked
  target protocol version. A version-only match is invalid. While the
  lock pins no published target protocol, no adapter can exist in the
  lock, so `adapter_ref` must be absent; after protocol publication it is
  required (the Rust checker is normative for this cross-binding).
- `generator_ref` is the logical generator identity supplied by that
  locked adapter, never a separately invented install.

## Lifecycles and regeneration policy

| Lifecycle    | Policy            | Drift/missing behavior                          |
| ------------ | ----------------- | ----------------------------------------------- |
| `generated`  | `on-input-change` | Blocking: staleness, drift, and absence fail the check. |
| `scaffolded` | `once`            | Reported, never overwritten, no automatic repair. |
| `checked`    | `validate-only`   | Reported; the adapter only validates the contract. |
| `external`   | `reference-only`  | Reported; no generation or clean authority.      |
| `custom`     | `manual-only`     | Reported; never overwritten or generic-cleaned.  |

The policy is an explicit per-entry field paired one-to-one with the
lifecycle. Blocking verdicts are `stale`, `manual-drift`, `missing` on
`generated` entries, and any `orphan`; all other findings are reported in
the receipt without blocking and never repaired.

## Orphans and safe clean

- v1 declares exactly one managed root: `.lekalo/generated/` (the
  reserved runtime home). The scan is explicit and bounded — it never
  classifies arbitrary project files — and refuses symlink, reparse, and
  special entries fail-closed. Unclaimed files with non-portable names
  are refused rather than reported. Adapters with typed write plans
  (#27/#91) extend the declared roots later.
- `generate --clean --dry-run` computes a deterministic plan — orphan
  paths, exact content digests, sizes — whose `planId` is SHA-256 over
  that closed payload plus the bound manifest and lock digests. Planning
  writes nothing.
- `generate --clean --confirm sha256:<planId>` revalidates the entire
  plan, then deletes one file at a time with an immediate re-check before
  each removal; any refusal aborts the remaining plan with zero further
  deletions. Only orphans inside the managed root are ever removed.
- The manifest's own bookkeeping under `.lekalo/generated/manifests/` is
  never generated output and can never be orphaned or cleaned.

## Diagnostics and exits

The registry is closed; this surface adds no rules. Verdicts are typed
result data in the receipt; failures map onto registered rules with
bounded tokens only:

| Situation                             | Rule                                  | Exit |
| ------------------------------------- | ------------------------------------- | ---- |
| Noncanonical manifest bytes           | `lock.noncanonical`                   | 1    |
| Invalid wire shape                    | `lock.schema-invalid`                 | 1    |
| Future manifest discriminator         | `lock.unsupported-schema-version`     | 5    |
| Manifest integrity digest mismatch    | `lock.digest-mismatch`                | 3    |
| Other lock revision or inputs         | `lock.stale`                          | 1    |
| Generated drift                       | `lock.source-changed`                 | 1    |
| Generated artifact missing            | `structure.document-missing`          | 1    |
| Orphan under the managed root         | `structure.runtime-unexpected-entry`  | 1    |
| Source-map invariant failure          | `lock.reference-invalid`              | 1    |
| Link/special entry refusal            | `structure.path-link` / `path-special`| 3    |
| Missing lock                          | `lock.missing`                        | 1    |
| Mutating clean without preview        | `lock.preview-required`               | 1    |
| Plan or project changed before apply  | `lock.source-changed`                 | 1    |

Human and JSON are projections of the same `DomainResult`; the receipt is
pretty two-space JSON (`status`, `operation`, `mode`, `manifestDigest`,
`lockDigest`, `verdict`, `counts`, sorted `findings`). No field carries
an absolute path, URL, credential, host, user, timestamp, or source
snippet.

## Source maps

`source_maps` map semantic ids to half-open byte ranges
(`start` inclusive, `end` exclusive) inside exactly one recorded
artifact, bound to the exact inputs revision. Ranges must lie inside the
observed bytes, may not overlap for the same semantic id, and a map on a
never-generated lifecycle carries no ranges. Mapping is traceability,
never ownership transfer: the source remains the owner of its generated
code until an explicit reviewed adoption.

## Fixture matrix and gate

- Wire fixtures: `tests/fixtures/artifacts/wire/{valid,invalid}/`.
- Runtime behavior: `crates/lekalo-core/tests/artifact_manifest.rs`.
- CLI integration: `crates/lekalo-cli/tests/generate.rs`.
- Independent structural gate: `node scripts/test-artifact-manifest-contracts.mjs`
  (exact Ajv 8.17.1, Node 18 and 24).
- The wire fixtures are authored once with
  `node scripts/gen-artifact-fixtures.mjs`; outputs are committed and
  never regenerated in CI.
