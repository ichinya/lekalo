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
