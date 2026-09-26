# The adapter package manifest (issue #32)

The adapter package manifest is the independent, verifiable declaration
every target/profile adapter carries: identity, publisher, source
custody, license, compatibility, capabilities, executable coordinates,
platforms, integrity, permissions, hooks, conformance reference, and
lifecycle status. It exists so the core can decide — before any child
process exists — whether an adapter package is what it claims to be.

> Версионирование: контракт при изменении получает текущую версию
> проекта. Этот issue несёт перспективную версию 0.3.2; версия контракта
> независима от версий Model/IR/protocol и от версии продукта.

## Contract identity

- Schema artifact: `contracts/adapter-manifest.schema.v0.3.2.json`
  (Draft 2020-12, closed: `additionalProperties: false` everywhere, no
  optional members outside the closed vocabulary).
- Wire discriminator: `schema_version = "lekalo/adapter-manifest/v0.3.2"`.
- Contract identity: `dev.lekalo.adapter-manifest@0.3.2`.
- Canonical form: strict canonical JSON — compact, UTF-8 without BOM, no
  duplicate keys, object keys in unsigned UTF-8 byte order, semantic
  arrays sorted, exactly one trailing LF on disk. `ManifestDigest` is
  SHA-256 over the canonical bytes with the self-referential
  `manifestDigest` member excluded from the domain (the same pattern as
  the lock digest).
- The Rust implementation in `crates/lekalo-core/src/adapter_package` is
  normative for cross-field invariants; this document is the normative
  wire description.

## Members

Top-level keys, in wire (byte-sorted) order: `adapter`, `capabilities`,
`compatibility`, `conformance`, `executable`, `hooks`, `identity`,
`integrity`, `license`, `permissions`, `platforms`, `publisher`,
`revocation`, `schemaVersion`, `source`, `status`.

- `adapter` — `{id, name, version}`: the closed lowercase component-id
  grammar, a bounded display name, and one canonical SemVer without
  prerelease or build metadata. The immutable identity triple is
  `{adapter.id, adapter.version, integrity.packageDigest}`; the same
  `{id, version}` at a different digest is a distinct package.
- `publisher` — `{id, name?, trustAnchor}` with the closed anchor
  vocabulary `builtin|registry|publisher-key|none`. The anchor feeds the
  trust assignment; it is a declaration, not a proof — the proof is the
  signature/conformance path of the install that brought the package in.
- `source` — `{kind, coordinate, digest}`: `kind` is
  `path|path-exec|release|registry`; the coordinate is a closed token
  (`path:<fs-path>`, `exec:<name>`, `release:<channel>/<id>`,
  `registry:<registry-id>/<package-id>`), never a URL string; the digest
  pins the exact source snapshot bytes the manifest was derived from.
- `license` — `{spdx, file, fileDigest}`: a closed SPDX token (or
  `other`) plus a package-relative notice file and its digest.
- `compatibility` — `{protocolVersions, irVersions, extensions}`: exact
  set membership over the contract versions — never a range — mirroring
  the accepted #9 preflight semantics.
- `capabilities` — `{operations, targets, profiles, named, constraints,
  readScopes, writeScopes, transports}`: the declaration the describe
  cross-check compares against, plus the read/write scope declaration.
- `executable` — `{runtime, entry, argvPreview, assets}`: runtime kind
  and minimum version, the package-relative entry path, a display-only
  argv preview (never executed verbatim), and additional runtime files
  with their digests.
- `platforms` — `any` or stable lowercase target triples from the closed
  vocabulary.
- `integrity` — `{packageDigest, files, signaturePolicy, signature?}`:
  `files[]` is sorted by path, unique, and covers every packaged file
  including the manifest itself; `packageDigest` covers the sorted
  per-file canonical byte set; `signaturePolicy` is
  `unsigned|optional|required`; `signature` declares the closed scheme
  (`minisign|sigstore-bundle|pgp-cleartext`), the digest-addressed
  signature material, and the optional signer identity.
- `permissions` — `{filesystem, network, environment, processes,
  secrets}`: the least-privilege declaration #89 enforces. `network.mode`
  is `denied|allowlist` with closed host tokens; the environment
  allowlist names variables explicitly; child processes are
  `denied|declared`; secrets are declared handle ids, never values.
- `hooks` — pinned empty in this contract version (see below).
- `conformance` — `{reportDigest, badge, suiteRegistry}`: the reference
  to the exact #31 conformance report run, the verified badge (protocol,
  IR, profile), and the registry identity the battery belongs to.
- `status` — `active|yanked|revoked`. Publisher-supplied self-report:
  `yanked` excludes new installs and auto-selection; `revoked` denies.
- `revocation` — `null` while active; otherwise `{reason,
  noticeDigest?, supersededBy?}`. The **enforcing** revocation signal is
  the local revocation store (`docs/adapter-install.md`), so an
  adversarial manifest cannot un-revoke itself.

## Canonical digest domain

`ManifestDigest` = SHA-256 over the canonical JSON bytes of the manifest
document as stored, minus the self-referential `manifestDigest` member
when present. All other bytes participate. Two manifests with identical
semantic content therefore share one digest regardless of key order in
the stored file; the gate compares digests, never formatted bytes.

## Verification points

Checksums are verified at every custody boundary:

1. manifest decode — the digest must recompute over the stored bytes;
2. package verification — every `integrity.files[]` entry and the
   package root are re-digested **before any execution, describe
   included**;
3. staged downloads — release/registry bytes are re-digested against the
   release record **and** the manifest before promotion;
4. install promotion — the staged tree is re-verified file-by-file
   before the atomic rename;
5. execution binding — the locked entry digest must match the launched
   bytes (the existing `binding_failure` check; installed adapters pin
   the entry digest under the `installed` source kind).

Signature policy: `unsigned` declares no signature; `optional` verifies
the digest always and the signature when a shipped verifier exists;
`required` with no shipped verifier answers
`adapter.signature-unverified` (unavailable) — never a pass. v1 ships no
cryptographic verifier; minisign/ed25519 is the recommended first
scheme.

## Refusals

Every refusal carries a registered `adapter.*` rule id from the
diagnostic registry (`docs/diagnostics.md`): `adapter.manifest-invalid`,
`adapter.manifest-mismatch`, `adapter.incompatible`,
`adapter.checksum-mismatch`, `adapter.signature-unverified`,
`adapter.revoked`, `adapter.quarantined`, `adapter.trust-insufficient`,
`adapter.source-unavailable`, `adapter.install-plan-required`,
`adapter.source-changed`, `adapter.install-conflict`,
`adapter.recovery-required`, `adapter.hooks-declared`,
`adapter.permission-escalated`. Statuses map onto the accepted
0/1/3/4/5 envelope: usage and plan faults exit 1, integrity denials
exit 3, unavailable sources and verifiers exit 4, incompatible versions
exit 5.
