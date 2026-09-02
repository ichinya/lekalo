# Lekalo

Lekalo is the semantic application-model layer described by the project
architecture. The first repository contract fixes artifact ownership before the
Rust workspace and provider implementations are introduced.

- [Authority matrix and boundary policy](docs/authority.md)
- [ADR-0001: artifact authority and synchronization boundaries](docs/adr/0001-artifact-authority-boundaries.md)
- [Authority contract migration 1.2.0 to the 1.3 line](docs/authority-contract-migration-1.2-to-1.3.md)
- [Corrective migration from yanked 1.3.0 to 1.3.1](docs/authority-contract-migration-1.3.0-to-1.3.1.md)
- Accepted-version manifest: `contracts/authority-contracts.manifest.json`
- Reviewed corrective successor: `contracts/authority-matrix.v1.3.1.json`
- Rejected/yanked candidate preserved for custody: `contracts/authority-matrix.v1.3.0.json`
- Immutable historical baseline: `contracts/authority-matrix.v1.2.0.json`

Validate the contract and its allowed/forbidden/malformed fixtures, then verify
the documented CLI exit-code protocol with Node.js, without installing
dependencies:

```sh
node scripts/check-authority.mjs
node scripts/check-authority.mjs --contract-version 1.2.0
node scripts/test-authority-cli.mjs
node scripts/test-authority-contracts.mjs
node scripts/test-authority-boundaries.mjs
```

An integration adapter can check one proposed operation with:

```sh
node scripts/check-authority.mjs --operation path/to/operation.json
```

Supplying a custom contract path also requires the exact accepted
`--authority-ref contractId@version@sha256:<digest>`; a path or recomputed
digest alone is never authoritative.

## Privacy and export policy

The fail-closed privacy/export baseline accepted for issue #120:

```text
dev.lekalo.privacy-export-policy@1.0.6@sha256:99a813a89efbdf336340390c9589a4f05d0dbbc8805748708b455a3d7a329ca7
```

- [Privacy and export policy](docs/privacy.md)
- [ADR-0002: exact-custody privacy decision contract](docs/adr/0002-privacy-export-policy.md)
- Accepted policy: `contracts/privacy-policy.v1.0.6.json` with its manifest and sidecars
- Frozen history: `1.0.0` rejected-unaccepted WIP; `1.0.1`–`1.0.5` yanked candidates, preserved byte-for-byte
- Migration notes: `docs/privacy-policy-migration-*.md` for every step of the ladder

Validate one export decision or run the full protocol suites:

```sh
node scripts/check-privacy.mjs --decision path/to/decision.json
node scripts/test-privacy-contracts.mjs
node scripts/test-privacy-cli.mjs
```

Exit protocol: `0` allow, `3` well-formed deny or transform-required, `1`
malformed or custody failure. The checker trusts only the hard-pinned
accepted manifest bytes; a recomputed digest never authorizes changed
semantics.

## Canonical project structure

The portable layout of a user repository — canonical model homes, target
configuration, lockfile location, generated cache and reports — is fixed by
[the structure contract](docs/canonical-structure.md) and
[ADR-0003](docs/adr/0003-canonical-structure-and-path-safety.md). It is
neutral to the Node.js, PHP and Rust targets, aligns every `.lekalo/**` home
with accepted authority contract `1.3.1`, and defines root discovery, physical
link containment and the canonical/runtime ownership split. Model/import and
lockfile contents remain owned by their downstream issues.

Check the conformance fixtures, validate one project, discover a project
root, or probe a single project-relative path:

```sh
node scripts/check-structure.mjs
node scripts/check-structure.mjs --project relative/path/to/project
node scripts/check-structure.mjs --find-root --from relative/path/to/dir
node scripts/check-structure.mjs --validate-path lekalo/modules/shop
node scripts/test-structure-contracts.mjs
```

The exit protocol is `0` valid, `1` malformed/usage, `3` well-formed but
denied; JSON envelopes are deterministic. `--project`, `--from` and
`LEKALO_PROJECT` accept only safe invocation-relative selectors and never
replace the physical `lekalo/project.yaml` marker.

## Lekalo Model contracts and semantic IDs

The published language-neutral Model 0.1.0 contract for issue #5 remains at
[Model 0.1](docs/model.md), [ADR-0004](docs/adr/0004-model-v0.1.md), and
`contracts/model.schema.v0.1.0.json`. Its stable-ID successor is
[Model 1.0](docs/model-1.0.md), governed by the closed
[semantic-ID contract](docs/semantic-ids.md), [ADR-0005](docs/adr/0005-semantic-ids.md),
and [0.1-to-1.0 guidance](docs/model-migration-0.1.0-to-1.0.0.md). Contract
versions are independent of product releases; issue #6 is the prospective
product 0.1.4 candidate because issue #3 published product 0.1.3.

The checker recognizes only the two exact schema versions, and all documents
in one project must agree. Model 1.0 makes project/module IDs one-segment and immutable,
supports two- or three-segment symbol IDs, decouples module identity from its
directory, and adds symbol-only rename history plus permanent tombstones.
Historical IDs are traceability only and never resolve live references.

Run the dependency-free checker and its independent schema/semantic suite:

```sh
node scripts/check-model.mjs
node scripts/check-model.mjs --project tests/fixtures/model/valid-planner
node scripts/check-model.mjs --project tests/fixtures/model-v1/valid-planner
node scripts/check-model.mjs --check-id planner.event.focus_changed
node scripts/test-model-contracts.mjs
# Release gate: exact Ajv 8.17.1 is provisioned outside this checkout (CI does the same on Node 18 and 24) and exposed through NODE_PATH.
node scripts/test-model-ajv.mjs
```

Model validation uses exit `0` for valid and exit `1` for usage, structure/shape
or semantic invalidity, with stable leading `model.*` or `semantic-id.*`
reasons on stderr. The
checker runs the #4 structure validator before any model read and preserves a
physical-policy denial as exit `3` with deterministic JSON on stdout.

## Rust CLI foundation

The two-crate Rust workspace provides the target-neutral core result model and
the `lekalo` CLI foundation. See the [CLI contract](docs/cli.md) for commands,
stable output envelopes, exit codes, and the exact Rust 1.80.0 MSRV.

```sh
cargo build --workspace --locked
cargo run --locked -p lekalo-cli -- --version
```
