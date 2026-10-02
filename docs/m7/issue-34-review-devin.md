# Issue #34 independent review (devin)

Reviewer: dispatched worker `task_9a521580264e`. Review-only; no
implementation changes. Branch `ichinya/m7-issue-34`, diff base
`origin/ichinya/M7` = `9510dd07`, HEAD `2fa5f1e5`. Scope per dispatch:
the lekalo-side provider boundary (manifest module, `lekalo provider
describe`, `contracts/provider-capabilities.schema.v0.6.3.json`,
`docs/provider-contract.md`, boundary tests + Ajv gate).

## Verdict: ISSUES

The delivery is genuinely solid in its mechanics — deterministic golden
manifest, closed schema, correct versioning rule, enforced
stream/exit discipline, real child-process and Node consumer gates, no
scope creep — but two claims in the normative contract are wrong about
the boundary they publish, and an extension implementer who takes the
document literally will hit both.

## Findings

### 1. MAJOR — `generated-artifacts` overclaims write confinement to `.lekalo/**`

Three normative places state that `generate` writes only inside
`.lekalo/**`:

- `crates/lekalo-core/src/provider/operations.rs:14-17` — effect-class
  doc: "write only into the governed `.lekalo/**` generated/lock custody".
- `contracts/provider-capabilities.schema.v0.6.3.json:53` —
  `effectClass` description repeats the same bound (part of the
  versioned artifact).
- `docs/provider-contract.md:48-52` (position 6) and `:99-101` —
  "writes exclusively into the governed `.lekalo/**` generated/lock
  custody".

That is not the enforced boundary. Adapter write plans are confined to
the adapter's declared write scopes minus `PROTECTED_HOMES`
(`crates/lekalo-core/src/target_protocol/wire.rs:1153-1211`,
`scopes.rs:25-34,1175-1186`): `src/**`-style scopes are legal
(`scopes.rs:178-179` tests `is_scope("src/**")` /
`scope_covers("src/**", "src/a/b.ts")`), and the authority matrix's
`generated.code` kind allows `**` (`contracts/authority-matrix.v0.3.2.json`).
Core itself classifies writes under `tests/lekalo/**`,
`app/lekalo-types/**`, `app/lekalo-operations/**` — all outside
`.lekalo/**` — as scaffolded lifecycle
(`orchestration/generate.rs:1089-1104`).

The safety property the issue actually requires *is* enforced:
`openspec/**`, `lekalo/**`, `lekalo.lock`, and the runtime homes
`.lekalo/{ir,cache,import,privacy,consumer}/**` are protected —
a covering scope or planned write is refused as
`target.protected-path` (exit 3). What is wrong is the stated *bound*:
a consumer bounding `generate`'s mutation surface to `.lekalo/**` per
this contract (e.g. for before/after revision binding) under-scopes.
Fix by stating the real rule — "writes confined to adapter-declared
write scopes verified against the plan; protected homes refused" — in
all three places.

### 2. MAJOR — `validate`'s `outputSchema` pin does not describe the output

`operations.rs:147-153` pins `lekalo/validation-profile/v0.4.0` as
`validate`'s output schema, and the field doc (`operations.rs:43-44`,
contract operation table `provider-contract.md:87-98`) defines
`outputSchema` as "the exact `lekalo/.../v<version>` discriminator on
the wire". For `validate` that discriminator is not on the wire and the
pinned schema does not describe the payload:

- Actual success output is
  `{"status":"valid","modelVersion":...,"validation":{profile,profileVersion,registryVersion,moduleScope?,rulesEnabled,counts}}`
  (`crates/lekalo-cli/src/main.rs:2846-2859`,
  `lekalo-core/src/validator/report.rs:99-132`) — no `schemaVersion`
  member at all.
- `contracts/validation-profile.schema.v0.4.0.json` requires
  `schema_version`,`identity`,`profile_id`,`version`,
  `diagnostic_registry_version`,`scope`,`rules` — it is the *profile
  input document* schema. The validate output fails it closed.

Every other operation's pin does describe the emitted payload:
`doctor`/`status`/`readiness` emit `schemaVersion:
lekalo/doctor/v0.3.2` (`doctor/model.rs:256,285`), `impact` emits
`lekalo/impact/v0.2.16` (`impact/canonical.rs:53`), `context` emits
`lekalo/context/v0.2.16` (`context/render.rs:86`), `verify`/`generate`
emit `lekalo/orchestration/v0.2.16` (`orchestration/receipt.rs:14`),
and `trace export` embeds the validated `lekalo/trace-manifest/v0.2.16`
document (`trace/parse.rs:250`). A consumer performing the documented
exact-identity negotiation and validating `validate` output against the
pin will reject a valid result. Either ship a real validation-report
output contract, or redefine the field as the governing family (the
profile identity is already carried as `validation.profile` +
`profileVersion`/`registryVersion`).

### 3. MINOR — Position 3 says the manifest carries command strings; it does not

`docs/provider-contract.md:35-36`: "The manifest carries
presentation-form command strings for documentation only." The wire
manifest omits `command` entirely — `OperationRef`
(`manifest.rs:104-111,140-150`) serializes only `id`, `effect`,
`outputSchema`, `requiresProject`, `requiresAdapter`, and the committed
golden confirms it (`tests/fixtures/provider/describe.golden.json:38-102`).
The truth is stricter than the text; the sentence should be dropped or
corrected.

### 4. MINOR — `productVersion` is a frozen literal documented as the producing version

`provider/version.rs:46-49` pins `PRODUCT_VERSION: &str = "0.6.3"` and
`manifest.rs:81` documents it as "the exact producing product version
(`CARGO_PKG_VERSION`)". It is not `CARGO_PKG_VERSION` — it is a literal,
and the test (`version.rs:77-80`) asserts the literal rather than
comparing against the workspace version. On the next product release
without a contract rev (none is required — unchanged contracts keep
their versions), the manifest will report `productVersion: 0.6.3` from
a 0.6.4 binary, and `productVersion` will silently diverge from
`lekalo --version`. If freezing is intended (product version *of the
implementation commit*), the docs should say that; if it should track
the producing binary, it should be `env!("CARGO_PKG_VERSION")`.

### 5. MINOR — Presentation command strings are not the complete CLI grammar

`operations.rs:126` / `provider-contract.md:96` present
`readiness --phase implement|generate|verify|release|done`, but the CLI
also accepts `model` (`main.rs:911` `DoctorPhase::Model`). Similarly the
`impact` presentation omits the `lekalo impact SYMBOL` and bare
`--changed` forms (`main.rs:168-171`, `ImpactArgs` at `main.rs:45-85` —
`--base`/`--worktree` are not required once `--changed` is set;
`run_impact` accepts it, `main.rs:8999-9008`). These are
documentation-only forms, so harm is limited to a consumer
hand-crafting argv outside the documented subset — but the normative
doc should mark them as the provider-relevant subset, not the command's
grammar. Related nuance: `generate` is flagged `requiresAdapter: true`
while its `--check` (drift) mode needs no adapter; the mapping rules
explain this, but the flag is about the mutating form only and a naive
consumer could refuse a valid drift check.

## Acceptance-criteria checklist (lekalo-side slice of #34)

| Issue requirement | Status | Evidence |
| --- | --- | --- |
| Ops: detect/status | Delivered | `status` op + detection = manifest + installed-tool check (`provider-contract.md:83-85`); matches research proposal |
| Ops: doctor | Delivered | `doctor` op, doctor schema pin |
| Ops: impact/context | Delivered | `impact`, `context` ops with bounds (`bounds.recommendedContextBudgetTokens` 5000) |
| Ops: validate | Delivered, pin wrong | `validate` op exists; `outputSchema` mismatch — finding 2 |
| Ops: generate/verify | Delivered | `generate` (`generated-artifacts`, `requiresAdapter`), `verify`; write-scope text wrong — finding 1 |
| Ops: readiness | Delivered | `readiness` op incl. `done`→`release` alias (verified `DoctorPhase::Done`, `main.rs:921-934`) |
| Ops: trace export | Delivered | `trace.export`, `requiresProject:false` matches `trace export PATH` (no project arg, `main.rs:1143-1146`) |
| External CLI/process JSON boundary | Pass | child-process tests `crates/lekalo-cli/tests/provider.rs` + Node Ajv gate `scripts/test-provider-contracts.mjs`; no Rust internals cross |
| Capability/version negotiation | Pass with caveat | exact `dev.lekalo.workflow-provider@0.6.3` + 7 schemaPins + `manifestDigest` recomputable in Node (gate check 6); caveat = finding 2 |
| policy off\|optional\|required | Correctly out of scope | consumer-owned; deferral is consistent with the accepted authority matrix |
| Evidence `.ai-factory/qa/<change-id>/providers/lekalo.json` | Correctly out of scope | `ai-factory.provider-evidence-envelope` under `.ai-factory/qa/**` allows writers `ai-factory`,`aifhub-adapter` only (`authority-matrix.v0.3.2.json`) — a lekalo-side writer would violate custody |
| No writes into OpenSpec canonical paths | Enforced | `openspec` is a protected home — `target.protected-path` exit 3 (`scopes.rs:25-34`, `wire.rs:1175-1177`) |
| Exact change/revision binding | Pass | ops embed `inputs` digests / git revisions; consumer binds before/after per contract |
| No hidden install/update/init | Pass | only `describe` exists; unknown subcommands → `LEK-CLI-001` exit 1 stderr (test `unknown_provider_subcommands_stay_usage_errors`); closed enum in schema+tests rejects `init|install|update|sync|cleanup|migrate|detect` |
| Deterministic output, no timestamps/host paths | Pass | byte-identical golden verified live by reviewer (empty dir, empty stderr, exit 0, zero filesystem entries created); unit + CLI + Node assertions |
| `--json` envelope / LEK-* conventions | Pass | `DomainResult::receipt` reuses the status-first envelope; usage failures keep `LEK-CLI-001`/`cli.usage`; exit/stream table documented and consistent with `result.rs:43-71` |
| Closed-schema versioning | Pass | `contracts/provider-capabilities.schema.v0.6.3.json` = product version of implementation commit; `check-contract-versions` picks it up via `contracts/*.v*.json` glob |
| Scope creep / unwired tests / docs-code mismatch | Pass (scope/tests); minor doc issues | additive diff only; tests wired in CI (`ci.yml:80,188-193`, binary built at `:181` before the live check); doc mismatches = findings 3-5 |

Issue-level ACs that remain extension-owned (correctly, per the
research split and authority matrix): AI Factory-only operation,
OpenSpec+Lekalo+HLV coexistence, `/aif-implement` capsule delivery,
normalized gate + stale-evidence blocking, and the extension E2E —
`docs/m7/issue-34-implementation.md:54-80` lists them as upstream
obligations; verified the custody claim against the matrix rather than
taking it on faith.

## Reviewer-verified gates (this worktree, Windows, Node 24.13, ajv 8.17.1 via LEKALO_AJV_NODE_PATH)

- `cargo test -p lekalo-cli --test provider --locked` — 10/10 PASS
- `cargo test -p lekalo-core provider:: --locked` — all provider tests PASS
- `cargo fmt --all -- --check` — PASS
- `cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings` — PASS
- `node scripts/check-contract-versions.mjs --base HEAD|origin/ichinya/M7|9510dd07` — PASS (96 artifacts)
- `node scripts/test-provider-contracts.mjs` — PASS, 9 checks including live binary
- `node scripts/test-fixture-provenance.mjs`, `check-authority.mjs`, `check-privacy.mjs`, `check-structure.mjs` — PASS
- Manual: `target/debug/lekalo.exe provider describe --json` in an empty dir — byte-identical to `describe.golden.json`, no stderr, no writes.

## Bottom line

Landable after correcting finding 1 and finding 2 — both are normative
text/metadata errors in the published boundary, not implementation
defects; the underlying enforcement (protected homes, exit/stream
discipline, digest domain, determinism) is real and verified. The three
minor findings are doc-accuracy fixes in the same files.
