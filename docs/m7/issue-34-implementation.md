# Issue #34 implementation: Lekalo workflow-provider boundary

Status: delivered (lekalo side). Branch `ichinya/m7-issue-34`, base
`ichinya/M7`, product `0.6.3`. Research basis:
[issue-34-research.md](issue-34-research.md) (commit `105b3e52`).
Cross-project authority: lekalo#34 (OPEN),
aifhub-extension#136 (CLOSED, execution path unfinished there).

## Scope decision

The research split assigns the process boundary publication to this
repository and the adapter/lifecycle/evidence work to
aifhub-extension. This implementation delivers the complete
lekalo-side split and nothing else: a published, versioned discovery
contract over the existing CLI, the closed schema for it, a normative
contract document, and boundary conformance tests. Fix round 1
([issue-34-fix1.md](issue-34-fix1.md)) corrected the published
declarations after independent review: the `validate` output schema
pin, the drift-check negotiation, the `--no-cache` read-only argv,
the truthful generation write-scope statement, and a tightened
manifest schema with per-operation const tuples. The evidence-file
schema (`.ai-factory/qa/<change-id>/providers/lekalo.json`), policy
normalization, the provider-specific phase schedules, and evidence
custody are extension-owned upstream dependencies (see below); the
accepted authority matrix already reserves that custody
(`ai-factory.provider-evidence-envelope`, writers
`ai-factory`/`aifhub-adapter`, never `lekalo`), so implementing a
writer here would have violated the repo's own authority contract.

## Delivered

| Artifact | Commit | Content |
| --- | --- | --- |
| `crates/lekalo-core/src/provider/{mod,version,operations,manifest}.rs` | `1280764a` | Typed, pure manifest model: contract identity `dev.lekalo.workflow-provider@0.6.3`, discriminator `lekalo/workflow-provider/v0.6.3`, the nine-operation closed vocabulary with effect classes and pinned output schemas, workflow bounds (recommended context budget 5000; hard bounds re-exported from the owning families), and the canonical-JSON sha256 self-digest whose domain excludes the digest field. No filesystem, environment, clock, or process access. |
| `crates/lekalo-cli/src/main.rs` (`lekalo provider describe`) | `1280764a` | The discovery subcommand: a `DomainResult::receipt` (exit 0, stdout, `{"status":"valid","manifest":{...}}`) that works outside any repository and never reads, launches, or writes. Unknown `provider` subcommands remain the stable usage failure (`LEK-CLI-001`, exit 1, stderr) — no hidden init/install/update/sync/cleanup surface exists. |
| `crates/lekalo-cli/tests/provider.rs` | `1280764a` | Ten child-process conformance tests against the real binary: receipt shape, stream/exit discipline, byte-identical determinism across runs and directories, discovery side-effect-freeness (empty-dir inventory before/after), the closed operation vocabulary (forbidden lifecycle ids asserted absent), per-operation schema pins, consumer-side digest recomputation, golden-byte identity, and human summary stability. |
| `tests/fixtures/provider/describe.golden.json` (+ provenance entry) | `1280764a` | The committed golden manifest; provenance-declared `synthetic`, so the fail-closed fixture-provenance gate stays exhaustive (64 families). |
| `contracts/provider-capabilities.schema.v0.6.3.json` | `c948255e` | Closed Draft 2020-12 schema (`additionalProperties:false` everywhere), const-pinned identity triple and target-protocol separation, closed operation/effect enums, and the digest format. Version `0.6.3` per `docs/versioning.md` (new contract takes the implementation commit's product version); `check-contract-versions` passes against `HEAD` and `HEAD~1`. |
| `scripts/test-provider-contracts.mjs` (+ CI wiring) | `c948255e` | The Node boundary gate under pinned Ajv 8.17.1: schema-validates the golden fixture and the live binary receipt (skip reason when unbuilt), cross-checks operation/schema/pin invariants, recomputes the digest domain in a consumer language, asserts no path/timestamp leakage and no discovery side effects. Registered in the contracts step and the build-test step of `.github/workflows/ci.yml`. |
| `docs/provider-contract.md` | `f4783378` (docs commit) | The normative contract: positions (discovery is metadata; fixed command construction; closed exact-version negotiation; consumer-owned evidence custody; no OpenSpec/HLV canonical writes), discovery receipt, the operation table, per-operation mapping rules (reports-vs-verdicts, bounded context `fits`, `generate --check` as drift, verify degradation, trace export), exit/stream interpretation, limits, privacy, and conformance pointers. |
| `docs/cli.md`, `docs/versioning.md` | same | `provider describe` in the command surface and wire docs; the new independent workflow-provider family recorded with its product-version rule. |

All existing behavior is untouched: no command changed an envelope, an
exit, or a write path; the new surface is purely additive.

## Requirement coverage (lekalo-side items)

| #34 requirement | Where delivered |
| --- | --- |
| Provider operations detect/status, doctor, impact/context, validate, generate/verify, readiness, trace export | All nine advertised in the manifest with exact native command mapping (`docs/provider-contract.md` operation table); detection = manifest + installed-tool check per the contract's positions. |
| Stable external CLI/process JSON boundary | Real-binary child-process tests (`tests/provider.rs`), golden fixture, Node consumer gate with Ajv validation; no Rust internals cross the boundary. |
| Capability/version negotiation | `schemaPins` + per-operation `outputSchema` + exact identity matching rule (no semver ranges, unknown = unsupported); `targetProtocolIdentity` kept a separate family. |
| Deterministic output; no timestamps/host paths | Unit and child-process assertions for byte-identity and path/timestamp absence; the golden fixture is the enforced artifact. |
| No hidden install/update/init | No such subcommand exists under `provider` (usage-error tests); the schema's closed enum rejects them; the contract's positions forbid them as recovery actions. |
| No writes into OpenSpec canonical paths | No operation writes them; `generate` is effect-class `generated-artifacts` (governed `.lekalo/**` only), enforced in vocabulary tests and documented normatively. |
| Existing envelope conventions | The receipt reuses `DomainResult` (status-first envelope, LEK-* diagnostics for the usage failure path); unchanged exit/stream contract documented per operation. |
| Versioned closed schema per versioning.md | `contracts/provider-capabilities.schema.v0.6.3.json`; `node scripts/check-contract-versions.mjs --base HEAD` passes (and `HEAD~1` for the committed state). |

## Upstream dependencies (aifhub-extension; deliberately not here)

Per the research split, these belong to the extension and remain its
execution obligations for aifhub-extension#136:

1. Replace the `scripts/lekalo-provider.mjs` fail-closed stub with
   discovery validation against this manifest (exact
   `dev.lekalo.workflow-provider@0.6.3` + schema digest) and reviewed
   argv dispatch per operation.
2. Provider-specific operation plans, the implement before/after
   substep, generate/verify enum values, and the neutral
   capability/evidence schema successors (`provider-evidence`,
   `provider-capabilities`, `provider-semantic-evidence`) that today
   are HLV-shaped and reject `LEK-*` codes.
3. Policy semantics `off|optional|required` with phase scoping
   (today: only optional/required, no literal `off`), degraded-not-
   failed optional absence, and required-blocking confined to the
   configured phase.
4. Evidence custody at the exact path
   `.ai-factory/qa/<change-id>/providers/lekalo.json` (today:
   `<provider>-<phase>.json`), the aggregate observation schema, and
   freshness binding per the research draft — all inside the
   extension's already-accepted `ai-factory.provider-evidence-envelope`
   custody.
5. Lifecycle wiring (`aif-implement`/`aif-verify`/`aif-done`
   injections, done-readiness read-only freshness checks) and the
   explicit no-Lekalo-init rule in shared TOOLS/initialization docs.

Lekalo's obligations toward those consumers are all met by this
delivery: the manifest, the schema, the contract document, and the
conformance gates are the stable surface the extension pins against.

## Verification (all run in this worktree)

```text
cargo fmt --all -- --check                                   PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                    PASS
cargo test -p lekalo-cli -p lekalo-core --locked             91 suites, 0 failed
  (includes 12 provider unit tests + 10 provider CLI tests)
node scripts/check-contract-versions.mjs --base HEAD         PASS (96 artifacts)
node scripts/check-contract-versions.mjs --base HEAD~1       PASS (committed state)
node scripts/test-contract-versions.mjs                      PASS
node scripts/test-provider-contracts.mjs (Ajv 8.17.1)        PASS (9 checks, live binary)
node scripts/test-fixture-provenance.mjs                     PASS (64 families)
node scripts/check-authority.mjs                             PASS
node scripts/check-structure.mjs                             PASS
node scripts/check-privacy.mjs                               PASS
git push -u origin ichinya/m7-issue-34                       pushed
```

Commits: `1280764a` (manifest + CLI + tests + fixtures),
`c948255e` (schema + gate + CI), docs commit (contract + CLI +
versioning docs).

## Risks and boundaries honored

- The manifest pins the *current* upstream versions (context/impact/
  trace/orchestration 0.2.16, doctor 0.3.2, validation-profile 0.4.0,
  diagnostics registry 0.4.0) without claiming any of them equal the
  product version — the research's schema-drift risk is addressed by
  publishing the distinction, not papering over it.
- `readiness`/`doctor`/`status` are documented and tested as reports
  whose exit 0 is not a verdict, so the consumer's normalization can
  never inherit the false-ready failure mode.
- `generate` remains the only mutating operation, requires an explicit
  adapter argv, and is excluded from every read-only phase by contract;
  its dry-run is documented as not a global no-writes guarantee.
- Native gate execution remains outside `verify`; the contract says so
  explicitly so no consumer attributes host gates to Lekalo.
- The extension E2E (real composed lifecycle with the released CLI)
  stays an extension-side acceptance obligation; this repo's boundary
  conformance is the part it can pin against today.
