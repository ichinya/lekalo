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
- [Contract versioning, support policy, and migrations](docs/versioning.md)
- [ADR-0008: versioning and migrations for Model, IR, and protocol](docs/adr/0008-versioning-and-migrations.md)
- [The committed `lekalo.lock`: reproducible resolution](docs/lockfile.md)
- [ADR-0009: the committed lekalo.lock and reproducible resolution](docs/adr/0009-lockfile.md)
- [The stable machine-readable diagnostic contract](docs/diagnostics.md)
- [ADR-0010: the stable machine-readable diagnostic contract](docs/adr/0010-diagnostics.md)

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

## Loader: YAML/JSON, imports, canonical model

Issue #7 implements `lekalo load`: strict spanned JSON/YAML frontends,
module-ID imports with direct visibility and cycle detection, compact type
normalization, and one deterministic canonical model. The loader consumes
the #4 structure rules, rejects symlinks/junctions/reparse points, never
writes, and emits the typed exit protocol (0 valid, 1 invalid, 3 denied,
5 unsupported-version). See [docs/loader.md](docs/loader.md),
[ADR-0006](docs/adr/0006-loader.md), and the hermetic fixtures under
`tests/fixtures/loader/`.

Issue #8 compiles the loaded model into the typed, deterministic,
target-neutral Lekalo IR (`dev.lekalo.ir@0.1.0`): `lekalo load --ir`
prints canonical IR bytes with an optional occurrence-safe source map, and
the `lekalo-core::ir` library surface exposes the closed typed read model
(exhaustive definition/effect enums, resolved references, closed extension
policy) for Rust consumers without the CLI. See
[docs/ir.md](docs/ir.md), [ADR-0007](docs/adr/0007-ir.md), and the
fixtures with golden canonical bytes under `tests/fixtures/ir/`.

## Reproducibility: the committed `lekalo.lock`

Issue #10 makes resolution reproducible: `lekalo lock` freezes the exact
product, contract, adapter, generator, profile, and capability identities
into one canonical JSON file, `lekalo lock --check` is the headless CI
gate, and `lekalo update --dry-run` / `--apply` preview and apply explicit
plans under a byte-level compare-and-swap. Digest mismatches (exit 3) are
classified separately from validation (exit 1), unavailability (exit 4),
and unsupported versions (exit 5), and the wire never carries absolute
paths or credentials. See [docs/lockfile.md](docs/lockfile.md),
[ADR-0009](docs/adr/0009-lockfile.md), and the hermetic fixtures under
`tests/fixtures/lockfile/`.

## Dependency graph of semantic symbols

Issue #13 projects the deterministic dependency graph over the typed IR:
twelve core node kinds and twelve core relations with closed provenance
and confidence, precomputed reverse indexes, forward/reverse and
direct/transitive traversal, shortest paths, module boundaries, bounded
slices, kind-specific cycle policy (`requires`/`derived_from` acyclic),
and byte-identical canonical export. `writes`/`implements`/`verifies`
stay registered but unemitted until their typed owners land. The thin
`lekalo graph show | callers | path | export` handoff keeps every decision
in the core; the contract, guarantees, and limits live in
[docs/graph.md](docs/graph.md), [ADR-0012](docs/adr/0012-dependency-graph.md),
and `contracts/graph.schema.v1.0.0.json`; hermetic fixtures are under
`tests/fixtures/graph/`.

## Stable machine-readable diagnostics

Issue #11 freezes the diagnostic wire: every failure carries closed wire
diagnostics (`lekalo/diagnostic/v1.0.0`) with immutable `LEK-SUBSYSTEM-NNN`
codes, registered category/severity/message/data semantics, deterministic
sorting and dedup, and derived `reasonCodes`; human and JSON renderers are
projections of the same `DomainResult`. Exit classes stay status-owned
(0/1/3/4/5) and severity never computes an exit. See
[docs/diagnostics.md](docs/diagnostics.md),
[ADR-0010](docs/adr/0010-diagnostics.md), and the embedded
`contracts/diagnostic-registry.v1.2.0.json` (issue #12 extended it with the
`semantic.*`/`validate.*` families and issue #13 added the `graph.*`
family, each as a minor increment).

## Semantic validation

Issue #12 implements `lekalo validate` over the typed IR: phase-ordered
pure rules with stable `semantic.*` ids and `LEK-SEM-NNN` codes, exact
source spans from the #8 source map, selectable built-in severity profiles
(`--strict`), module-scoped reporting (`--module`) that never hides
mandatory cross-module errors, and byte-identical deterministic output on
the 0/1 exit classes. The registry successor, closed profile contract, and
recorded owner decisions live in [docs/validation.md](docs/validation.md)
and [ADR-0011](docs/adr/0011-semantic-validation.md); the hermetic fixture
matrix is under `tests/fixtures/validation/`.

## Lekalo Model contracts and semantic IDs

The published language-neutral Model 0.1.0 contract for issue #5 remains at
[Model 0.1](docs/model.md), [ADR-0004](docs/adr/0004-model-v0.1.md), and
`contracts/model.schema.v0.1.0.json`. Its stable-ID successor is
[Model 1.0](docs/model-1.0.md), governed by the closed
[semantic-ID contract](docs/semantic-ids.md), [ADR-0005](docs/adr/0005-semantic-ids.md),
and [0.1-to-1.0 guidance](docs/model-migration-0.1.0-to-1.0.0.md). Contract versions are
independent of product releases; issue #13 carries prospective product
0.1.11 (issue #12 published product 0.1.10 at `fdfbcb5`; issue #11
published product 0.1.9 at `5b885bf`; issue #10
published product 0.1.8 at `8ddbbf0`; issues #8/#9
published products 0.1.6/0.1.7, the latter at `f0b3784`).

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
