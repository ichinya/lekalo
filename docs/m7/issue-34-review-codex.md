# Issue #34 independent Codex review

Verdict: **ISSUES**. Findings: **0 blocker, 4 major, 1 minor**.

Reviewed on 2026-10-02, branch `ichinya/m7-issue-34`, candidate
`a21a005c55539df82c620316390841b94e99e36c`, against
`origin/ichinya/M7` = `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`.
The implementation itself ends at `2fa5f1e5`; the candidate also includes
another review document, which was not used to derive these findings.
Read `issue-34-research.md` and `issue-34-implementation.md` first and
retrieved the live [issue #34](https://github.com/ichinya/lekalo/issues/34)
with `gh issue view 34 --repo ichinya/lekalo --json ...`; it remains OPEN.

This is a review of the Lekalo boundary publication. Extension policy,
lifecycle scheduling, aggregate evidence custody and composed E2E are
explicit upstream dependencies, not implemented acceptance criteria.
That repository split is respected; their absence is not counted as an
additional code finding. The published boundary still needs the four
major corrections below before consumers can rely on its declarations.
Only this review document was changed.

## Findings

1. **major — `validate` advertises a configuration schema as its result schema.**
   Evidence: `crates/lekalo-core/src/provider/operations.rs:149` and
   `docs/provider-contract.md:93` select
   `lekalo/validation-profile/v0.4.0`. However,
   `contracts/validation-profile.schema.v0.4.0.json:9` describes a profile
   definition requiring `schema_version`, `identity`, `profile_id`,
   `version`, `diagnostic_registry_version`, `scope` and `rules`.
   Actual success is constructed at `crates/lekalo-cli/src/main.rs:2852`
   and `crates/lekalo-core/src/validator/report.rs:99` as
   `{"status":"valid","modelVersion":"0.2.16","validation":{...}}`,
   with `profile`, `profileVersion`, `registryVersion`, `rulesEnabled`
   and `counts`. On a temporary copy of
   `tests/fixtures/validation/valid/base`, `lekalo validate --json`
   exited 0, but Ajv 8.17.1 rejected both the complete receipt and its
   `validation` payload against the advertised schema. A consumer
   following exact output negotiation cannot validate a successful
   operation and must either reject it or invent an unpublished parser.
   Publish and advertise the actual validation-result contract, retain
   the profile pin as configuration metadata, and validate real success,
   warning and failure results at the process boundary. Current pin
   tests merely repeat the same wrong constant
   (`scripts/test-provider-contracts.mjs:115`).

2. **major — the documented read-only `validate` command writes a cache.**
   Evidence: `crates/lekalo-core/src/provider/operations.rs:148` marks
   validation `read-only`, and `docs/provider-contract.md:99` explicitly
   promises that such operations never write. The fixed argv at
   `docs/provider-contract.md:93` does not include `--no-cache`.
   `crates/lekalo-cli/src/main.rs:2628` calls the cached compilation
   pipeline with the default bypass flag false;
   `crates/lekalo-core/src/cache/pipeline.rs:526` opens/creates SQLite
   and the pipeline persists derived records. Independent filesystem
   inventory of the valid base fixture showed that
   `lekalo validate --json` exited 0 and added
   `.lekalo/cache/cache.sqlite`. The same command with `--no-cache`
   exited 0 with identical JSON and no added files. This is a defect in
   the new effect declaration, despite caching being existing behavior:
   a read-only provider phase now has an undeclared mutation and can
   invalidate an input inventory that binds ignored `.lekalo` inputs.
   Make the boundary's prescribed validation argv use `--no-cache`,
   or publish a truthful effect contract, and enforce it with a before/
   after filesystem test on an initialized project. Discovery-only
   inventory tests do not cover this operation.

3. **major — the required drift-check variant has no negotiated output contract.**
   Evidence: `docs/provider-contract.md:131` includes
   `generate --check` in the workflow mapping, but
   `crates/lekalo-core/src/provider/operations.rs:109` advertises only
   `lekalo/orchestration/v0.2.16` for generation, with a mandatory
   adapter prerequisite. The check path at
   `crates/lekalo-cli/src/main.rs:8773` instead returns
   `crates/lekalo-core/src/artifacts/mod.rs:172`'s `CheckReceipt`, which
   has no schema discriminator/identity and is not an orchestration
   receipt. In a temporary valid fixture, explicit `lekalo lock --json`
   followed by `lekalo generate --check --json` returned exit 0 and
   `{"status":"valid","operation":"generate","mode":"check",...,
   "verdict":"clean"}` without an adapter. Ajv rejected that document
   against `contracts/orchestration-report.schema.v0.2.16.json`.
   Therefore a consumer cannot negotiate the documented read-only drift
   operation through the published manifest; treating it as ordinary
   generation also conflicts with its effect/prerequisite declarations.
   Publish a versioned check-result schema and an explicit operation or
   mode-specific mapping with read-only/no-adapter prerequisites, and
   test clean, drift and missing/stale-input responses through the CLI.

4. **major — the advertised generation write scope excludes legitimate source artifacts.**
   Evidence: `crates/lekalo-core/src/provider/operations.rs:15`,
   `contracts/provider-capabilities.schema.v0.6.3.json:53` and
   `docs/provider-contract.md:99` declare that generation writes
   exclusively under `.lekalo/**`; the contract's position at line 50
   repeats that restriction. Actual generation supports adapter-declared
   managed source roots. The committed reference adapter declares
   `src/generated/node-typescript/**` at
   `tests/fixtures/orchestration/project/adapters/node-typescript/node-adapter.mjs:83`.
   Independent execution in a copied orchestration fixture, using explicit
   `lock -- node adapters/node-typescript/node-adapter.mjs` followed by
   `generate --target node-typescript --json -- node ...`, exited 0 and
   created `src/generated/node-typescript/planner.ts` as well as the
   `.lekalo/generated/manifests/ownership.json` metadata. The existing
   passing test `crates/lekalo-cli/tests/generate_orchestrate.rs:216`
   explicitly asserts this source file. A workflow consumer trusting
   the newly published restriction cannot construct a correct write
   authorization/inventory for the actual operation. Describe separately
   Lekalo runtime metadata and the target's reviewed managed write scopes,
   with ownership/plan enforcement, or introduce an explicit constrained
   provider mode. Preserve the existing protected-home checks: this
   finding does not establish any OpenSpec canonical write.

5. **minor — the published schema does not enforce its declared inventory invariants.**
   Evidence: `contracts/provider-capabilities.schema.v0.6.3.json:144`
   says pins are sorted, unique and cover exactly seven known families;
   line 151 promises sorted, unique operation IDs. The implementation
   only checks array lengths and broad discriminator patterns
   (lines 61, 78, 148 and 155), without unique IDs, exact family pins
   or operation-specific effect/prerequisite constraints. Ajv accepted
   four independently mutated receipts after recomputing their valid
   self-digests: nine copies of `context`, seven
   `lekalo/alien/v99.0.0` pins, `generate` marked `read-only`, and an
   unknown `validate.outputSchema`. The genuine binary emits the correct
   inventory; this is a validation-contract gap, not evidence of unsafe
   dispatch in the absent extension adapter. The document requires
   consumer-side refusal, but the release gate only checks invariants
   on the golden and compares the live result to it
   (`scripts/test-provider-contracts.mjs:198`); it supplies no negative
   manifest vectors. Constrain expressible invariants in the schema and
   publish/test any required semantic validation separately, so schema
   success cannot be confused with completed capability negotiation.

Apply the repository's contract-versioning rule to added or changed
contracts rather than introducing unversioned result parsers.

## Issue requirements checklist

`PASS` means verified for this repository's boundary; `PARTIAL` includes
an identified local gap; `UPSTREAM` means documented but not executed or
implemented by this change. Upstream rows must remain open for full #34
acceptance.

| Requirement | Result | Code, test or documented dependency |
| --- | --- | --- |
| detect/status, doctor, impact/context, validate, generate/verify, readiness, trace export | PARTIAL | Nine manifest IDs; discovery is the detection handshake (`provider/operations.rs:91`, CLI `main.rs:4515`, `tests/provider.rs:129`). Existing commands are reachable; validation and drift output negotiation fail findings 1/3, and effect/write-scope guarantees fail findings 2/4. |
| `off \| optional \| required` | UPSTREAM | Explicit extension work at `issue-34-implementation.md:68`; no host policy implementation/test here. |
| Stable external CLI/process JSON boundary | PARTIAL | Public child-process discovery tests and Node live gate pass; native command tests pass. Findings 1/3 prevent schema validation of two documented result forms. |
| Capability/version negotiation | PARTIAL | Provider identity/discriminator 0.6.3 and separate target protocol 0.3.2 are published; digest recomputation passes. Findings 1/3/5 limit completeness; extension acceptance/refusal still upstream. |
| Optional absence degrades rather than failing | UPSTREAM | `issue-34-implementation.md:69`; native blocked/degraded reports are available but do not implement optional host gating. |
| Required failure blocks only the configured phase | UPSTREAM | `issue-34-implementation.md:70`; no extension phase-policy matrix executed here. |
| `.ai-factory/qa/<change-id>/providers/lekalo.json` evidence | UPSTREAM | Exact path and aggregate custody at `issue-34-implementation.md:72`; authority matrix `ai-factory.provider-evidence-envelope` reserves the writer to AI Factory/AIFHub. Lekalo does not fake a writer. |
| No OpenSpec canonical writes | PASS for inspected boundary | Discovery inventory remains empty. Existing adapter protection includes `openspec` (`target_protocol/scopes.rs:33`); legitimate generated source paths are broader than `.lekalo/**` (finding 4). No write path changed in this diff. Full composed lifecycle hash checks remain upstream. |
| Exact change/revision binding | UPSTREAM for workflow evidence | Native receipts retain model/IR/lock identities; change ID, source inventory, policy/tool/input identities and before/after generation binding are explicitly extension obligations (`issue-34-implementation.md:75`, research evidence-schema draft). No aggregate freshness proof here. |
| No hidden install/update/init | PASS for discovery; upstream lifecycle open | Only `provider describe` is wired; hostile/absent PATH discovery and unknown `provider init/install` tests pass. Closed IDs omit setup operations. Extension shared-initialization safeguards are listed at `issue-34-implementation.md:80`. |

## Live issue acceptance checklist

- [ ] **AI Factory-only projects continue working:** no extension changes or
  mandatory workflow were introduced; host regression/E2E remains upstream.
- [ ] **OpenSpec + Lekalo + HLV can be active simultaneously:** separate
  ownership is documented; composed extension execution is not delivered.
- [ ] **`/aif-implement` receives a bounded context capsule:** native context
  tests pass, including `fits:false` and changed-symbol scope; delivery into
  the lifecycle is upstream (`issue-34-implementation.md:63`).
- [ ] **`/aif-verify` persists original diagnostics and normalized gates:**
  native LEK diagnostics remain available; normalization and persistence are
  upstream (`issue-34-implementation.md:65`). Finding 1 must be fixed first.
- [ ] **Stale evidence blocks done by policy:** readiness reports and native
  drift exist, but aggregate freshness/policy are upstream; finding 3 leaves
  the documented drift result unversioned/unnegotiated.
- [ ] **Provider crashes are distinct from implementation failures:** existing
  process exits and LEK diagnostics remain stable, but the extension's
  transport/failure-class normalization is neither implemented nor exercised.
- [ ] **E2E uses the published protocol rather than internal Rust crates:**
  the discovery portion is verified using a real CLI child process and Ajv;
  the full `/aif-*` lifecycle with a released CLI is explicitly upstream.

These unchecked items are acceptance limits, not assertions that extension
work was silently expected inside this repository.

## Independent validation and scope audit

- `git diff origin/ichinya/M7...HEAD --stat`: 18 files, 2517 insertions,
  one deletion; examined all implementation hunks and relevant existing
  result, cache, ownership, schema and CLI paths. Changes are additive and
  within the reported scope; the pre-existing review document was preserved.
- `cargo test -p lekalo-cli --test provider --locked`: 10 passed.
- `cargo test -p lekalo-core provider:: --locked`: 20 matching unit tests
  passed (the filter also matches existing provider modules); process exited 0.
- `cargo test -p lekalo-cli --test context --test doctor --test trace
  --test impact --test validate_semantic --test generate_orchestrate --locked`:
  57 passed (10/8/9/8/7/15 respectively), no failures.
- `node scripts/test-provider-contracts.mjs` with external Ajv 8.17.1:
  9 checks passed, including the live binary; no live-binary skip.
- `node scripts/check-contract-versions.mjs --base origin/ichinya/M7`:
  PASS, product 0.6.3, 96 artifacts. Also passed contract-version tests
  (6 cases), fixture provenance (64 synthetic families), authority,
  structure and privacy checkers. Existing gates were not weakened;
  the new Node gate is wired to both contracts and build-test jobs,
  with the latter running after `cargo build --workspace --locked`.
- Independent Node/Ajv and filesystem probes reproduced findings 1–5.
  Valid fixture copies ran in temporary directories, without absolute
  `--project` selectors; setup for the drift probe was an explicit lock
  command; generation used the committed adapter with explicit lock and
  apply commands. Probe directories were removed. A separate readiness probe
  confirmed exit 0/stdout with `verdict:blocked` and the done-to-release alias.
- Discovery bytes match the golden, are LF-only and deterministic across
  cwd/argv placements with environment cleared. Digest canonicalization
  sorts keys and excludes `manifestDigest`; the receipt keeps the existing
  status-first pretty projection. No UUID, timestamp, environment value or
  host absolute path appeared in discovery. No new diagnostic rule was
  invented: `LEK-CLI-001`/`cli.usage` is reused from registry 0.4.0 and
  retains exit 1/stderr; success retains exit 0/stdout.
- `git diff --check origin/ichinya/M7...HEAD` passed. The worktree was clean
  before this document. No full workspace/MSRV/clippy rerun, hosted CI
  qualification, release publication or extension lifecycle E2E is claimed.

The green focused gates establish discovery and existing native behavior;
they do not establish the accuracy of all new schema/effect declarations.
