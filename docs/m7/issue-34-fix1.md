# Issue #34 fix round 1

Both independent reviews (`issue-34-review-devin.md`: 2 major,
3 minor; `issue-34-review-codex.md`: 4 major, 1 minor) returned
ISSUES. Every finding below was verified against actual binary
behavior in this worktree before fixing, then fixed. Dispositions:
**fixed** or **rebutted** (with evidence). No gate, test, or schema
was weakened; two new closed describing-schema contracts were added
under the versioning policy (new contracts take the implementation
commit's product version `0.6.3`).

## Devin findings

### Devin-1 (MAJOR) — `generated-artifacts` overclaims `.lekalo/**` write confinement — FIXED

Verified first: generated a real source file
`src/generated/node-typescript/planner.ts` in a fixture copy with the
committed reference adapter, and confirmed the protected-home refusal
list (`target_protocol/scopes.rs` `PROTECTED_HOMES`:
`openspec`, `lekalo`, `lekalo.lock`, `.lekalo/{ir,cache,import,privacy,consumer}`).
The stated bound was wrong; the enforced rule is adapter-declared
write scopes verified against the ownership plan plus protected-home
refusal.

- `crates/lekalo-core/src/provider/operations.rs` — effect-class docs
  rewritten to the real rule.
- `contracts/provider-capabilities.schema.v0.6.3.json` — `effectClass`
  description states the real surface.
- `docs/provider-contract.md` — position 6 and the effect-class
  section state the real rule and name `src/generated/**` as
  legitimate.

### Devin-2 (MAJOR) — `validate` `outputSchema` pin does not describe the output — FIXED

Verified: `lekalo validate --json` emits
`{"status":"valid","modelVersion":"0.2.16","validation":{profile,
profileVersion,registryVersion,moduleScope?,rulesEnabled,counts}}`
with no embedded `schemaVersion`; the pinned profile schema is the
profile *input* document and rejects the output closed.

Fix: published `contracts/validation-report.schema.v0.6.3.json`
(`lekalo/validation-report/v0.6.3`) as a closed describing schema of
the actual success receipt, pinned it for the `validate` operation in
the manifest/Rust/Node/contract doc, and kept
`lekalo/validation-profile/v0.4.0` pinned as configuration metadata.
The live Ajv gate now validates a real `validate` receipt against the
new schema (gate check grown from 9 to 11).

### Devin-3 (MINOR) — position 3 says the manifest carries command strings — FIXED (trivially)

It does not (`OperationRef` serializes five fields; golden confirms).
Position 3 now reads "The manifest carries no command strings at all —
the argv recipes live only in this document."

### Devin-4 (MINOR) — `productVersion` is a frozen literal documented as the producing version — FIXED (trivially)

Documentation truthed in three places
(`provider/version.rs` doc on `PRODUCT_VERSION`, manifest field doc,
schema description, contract doc table): the value names the contract
generation (implementation commit's product version), deliberately
frozen, may trail a later binary's `--version`; consumers negotiate on
`identity`/`schemaVersion`. Freezing is the correct behavior under the
versioning policy (unchanged contracts keep their version across
releases); tracking `CARGO_PKG_VERSION` would make the pinned
contract's identity drift, so the docs were fixed instead.

### Devin-5 (MINOR) — presentation command strings are not the complete CLI grammar — FIXED (trivially)

Position 4 now states the argv recipes are the provider-relevant
subset, not the complete grammar; the readiness mapping rule notes the
unlisted `model` phase; the `drift` split (below) removes the
naive-consumer `generate --check`/adapter conflict.

## Codex findings

### Codex-1 (MAJOR) — `validate` advertises a configuration schema as its result schema — FIXED

Same defect as Devin-2 (independent discovery). Fixed identically; the
new gate additionally validates the exact emitted receipt shape and
the pin tests assert the new identity rather than repeating a wrong
constant.

### Codex-2 (MAJOR) — the documented read-only `validate` command writes a cache — FIXED

Verified: `validate` (cached pipeline, `cache::load_compiled`)
materializes `.lekalo/cache/cache.sqlite`; with `--no-cache` it exits
0 with identical JSON and creates nothing (filesystem inventory before
/ after, both ways). None of the other read-only operations write
(`impact`, `context`, `doctor`, `readiness`, `verify`, and
`generate --check` were probed the same way — no `.lekalo` creation).

Fix: the prescribed provider argv for `validate` is now
`lekalo validate --no-cache ...` everywhere (contract doc operation
table + effect-class section), with the rationale stated. Enforced by
a new child-process test (`prescribed_validate_argv_with_no_cache_
writes_nothing`: exit 0, schema-valid receipt, no `.lekalo` home,
identical file count) and by the Node live gate (receipt schema +
`.lekalo` absence). The gate never passed before on this point; it
now fails if the guarantee regresses.

### Codex-3 (MAJOR) — the required drift-check variant has no negotiated output contract — FIXED

Verified: `generate --check` on a locked fixture returns exit 0
`{"status":"valid","operation":"generate","mode":"check","lockDigest":
...,"verdict":"clean",...}` — a `CheckReceipt`, no discriminator, and
it fails the orchestration schema; drift/missing findings fail exit 1
with `LEK-LOCK-018` / `LEK-STR-004`.

Fix: promoted the check variant to its own negotiated **`drift`
operation** (`lekalo generate --check [--locked]`), read-only, no
adapter, pinned to the new closed describing schema
`contracts/generate-check-receipt.schema.v0.6.3.json`
(`lekalo/generate-check/v0.6.3`), with the mapping rule documenting
clean (exit 0 receipt) vs blocking-drift (exit 1 typed diagnostic)
responses. Enforced by a new child-process test
(`drift_check_clean_receipt_writes_nothing`: exit 0, receipt validates,
`verdict: clean`, `lockDigest` present, no filesystem additions) and
by the Node live gate validating a real clean receipt against the new
schema. The vocabulary grows 9 → 10 operations, canonical sorted
order, with all manifest/gate/schema/test layers updated.

### Codex-4 (MAJOR) — the advertised generation write scope excludes legitimate source artifacts — FIXED

Same defect as Devin-1 (independent discovery, with the reproduced
`src/generated/node-typescript/planner.ts` probe). Fixed identically:
the contract now separately describes Lekalo's own `.lekalo/**`
runtime metadata and the target's reviewed managed write scopes with
ownership/plan enforcement, and preserves the protected-home refusals
(no OpenSpec canonical write exists or was introduced).

### Codex-5 (MINOR, schema-enforcement gap) — the published schema does not enforce its declared inventory invariants — FIXED

Fix: the manifest schema now pins every operation as a const tuple
(id, effect, outputSchema, requiresProject, requiresAdapter) at its
canonical position (`prefixItems` + `items:false`), and the pin list
as an exact sorted 9-tuple of const objects. Verified with the
reviewer's own attack vectors — all rejected by Ajv even after
recomputing a valid self-digest:

```text
golden valid: true
rejected: nine copies of context
rejected: alien schema pin
rejected: generate marked read-only
rejected: validate wrong outputSchema
rejected: swapped op order
rejected: unknown operation id
rejected: extra pin
rejected: wrong productVersion
rejected: pins in wrong order
9/9 negative vectors rejected
```

Schema success still does not replace capability negotiation; the
contract's position 4 says exactly that, and the release gate checks
both the schema and the golden/live equivalence.

## Verification (all re-run in this worktree after the fixes)

```text
cargo fmt --all -- --check                                        PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                         PASS
cargo test -p lekalo-cli -p lekalo-core --locked                  91 suites, 0 failed
  (provider: 12 CLI tests incl. 2 new argv/no-write tests,
   25 provider/diagnostics unit tests)
node scripts/test-provider-contracts.mjs (Ajv 8.17.1)             PASS, 11 checks
  (schema + golden + live binary + digest recomputation
   + live validate receipt vs validation-report schema
   + no-cache side-effect check + live drift receipt vs
   generate-check schema)
node scripts/check-contract-versions.mjs --base HEAD              PASS (98 artifacts)
node scripts/check-contract-versions.mjs --base origin/ichinya/M7 PASS (98 artifacts)
node scripts/test-contract-versions.mjs                           PASS (6 cases)
node scripts/test-fixture-provenance.mjs                          PASS (64 families)
node scripts/check-authority.mjs                                  PASS
node scripts/check-structure.mjs                                  PASS
node scripts/check-privacy.mjs                                    PASS
```

Commits: `31d22945` (manifest/schema/tests/gate fixes + two new
contracts), `ba3586a5` (truthful contract/CLI docs), plus this report
and the implementation-report refresh. Branch pushed to origin.

## Rebuttals

None. Every finding reproduced cleanly against the binary; there was
nothing factually wrong or out of scope to rebut.
