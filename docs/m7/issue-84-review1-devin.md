# Issue #84 devin review round 1

**Verdict: ACCEPT.** Reviewed `82dbeb25` (`feat(profiles): add opt-in architecture policy core`, product 0.6.5, registry successor 0.6.5 = 500 + 8 LEK-APR) on `ichinya/m7-issue-84`, base `a2aa1893`.

## Verified live, this checkout

- `cargo build --locked -p lekalo-cli` clean; `cargo test -p lekalo-core --lib` 1056 passed / 0 failed.
- `scripts/test-architecture-profile-contracts.mjs` (exact Ajv 8.17.1): **ok** — 7 families, 9 goldens, 508 entries, 8 diagnostics, 62 live probes, 4 artifact-fault cases, read-only producer asserted.
- Union successor: all 500 predecessor entries byte-identical (`JSON.stringify` equality over every entry, 0 changed); additions are exactly `architecture-profile.{adoption-invalid,baseline-incomparable,evidence-incomplete,inheritance-invalid,input-invalid,policy-denied,version-unsupported,weakening-unacknowledged}`.
- Predecessor wire compatibility: live `lekalo --json validate` on the lockfile fixture emits `registry_version:"0.6.4"` for `LEK-SEM-001` — the per-rule registry pin in `normalize.rs` keeps existing rule IDs on the predecessor version, so existing goldens did not drift.
- `DiagnosticRegistry::successor()`/`for_version()` admit only the closed 0.6.5 union; `DiagnosticSet` resolves each item via `for_version(registry_version)` — honest mixed-version acceptance, no silent promotion.
- New surface `lekalo architecture-profile {catalog,resolve,lock,assess,diff}` present in live help; advisory default with `--check` gate; dedicated architecture lock sidecar (predecessor `lekalo.lock` untouched).
- `docs/architecture-profile.md` added; `commandOwners`/`families` registered; `documentation-owners.json` + cli.md index regenerated (155 commands incl. all architecture-profile subcommands); `test-docs-ownership` static+live green at 355 surfaces.
- Gates on this branch: provider 15, classification live, context-budget live (8 rules), diagnostics 500-entry predecessor gate, contract-versions 6 cases, `cargo fmt --check`, `git diff --check` — all clean.
- AC mapping in `issue-84-implementation.md` is honest: sidecar lock instead of mutating `lekalo.lock` is disclosed as a deferral (AC5), adapter/Mago mapping and A/B outcome criteria carry explicit boundary statements rather than fabricated evidence.

## Notes, not blockers

- Product 0.6.5 on this branch is the registry-successor convention; the #104 release prep on `main` also targets 0.6.5 — merge order will need the later branch to absorb the version line, same as prior bumps.
- `validation-report.schema.v0.6.4.json` keeps `registry_version` const 0.6.4: correct, since `lekalo validate` only emits predecessor-defined rules.
