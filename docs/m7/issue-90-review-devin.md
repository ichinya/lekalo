# Issue #90 independent review — unified deterministic golden fixture suite

Reviewer: independent dispatch (Devin), read-only review of branch
`ichinya/m7-issue-90`, diff base `origin/ichinya/M7` (`9510dd07`).
Scope: `git diff origin/ichinya/M7...HEAD` — 233 files, +14,037 / -0;
the only modified files are `.github/workflows/ci.yml` (+16) and
`tests/fixtures/fixture-provenance.json` (+4). Everything else is new.
Docs read first: `docs/m7/issue-90-research.md` (`c255c892`),
`docs/m7/issue-90-implementation.md` (`224488a9`).

**Verdict: ISSUES** — the suite is genuinely additive and most gates are
real, but the CI wiring breaks the `contracts` job on every run, the
`--verify` byte-comparison path is dead code so the declared expected
goldens are never compared, and the coverage index silently counts 10
rules with no evidence under `test-witness`.

## Verification performed locally (Windows checkout, debug binary)

| Gate | Result |
| --- | --- |
| `test-fixture-provenance.mjs` | ok (64 families, suite=synthetic) |
| `test-golden-catalog.mjs` | ok with `LEKALO_AJV_NODE_PATH` set; **exit 1 `ajv-8.17.1-unavailable` without it** |
| `test-golden-normalization.mjs` | ok (214 files) |
| `test-golden-hygiene.mjs` | ok (217 files, 6 controls) |
| `run-golden.mjs --verify` | ok (39 rows) — but see finding 2 |
| `test-golden-determinism.mjs` | ok, run **twice**, identical `manifestDigest` `sha256:3c8df36b…eaf9c6` both times |
| `test-golden-diagnostic-coverage.mjs` | ok (449 rules; 19/109/308/13 states) |
| `test-golden-adapter-shared.mjs` | ok |
| `test-golden-planner-e2e.mjs` | ok (6 stages) |
| `test-golden-update-policy.mjs` | ok |

Manual spot checks: trigger/non-trigger pair diff for
`semantic.type-recursion` is a genuine minimal edit (paired polarity is
real); the four `minimal/expected/*.json` files are byte-identical to
current CLI output for `load`, `validate --strict`, and `graph export`
(run by hand — the suite itself never does this, finding 2); no
timestamps/uuids/`Math.random` in the new scripts; `.gitattributes`
pins `eol=lf`; suite schemas carry `$id`s `dev.lekalo.*@1.0.0` with
`additionalProperties: false`; registry code/category join against
`diagnostic-registry.v0.4.0` (449/449 active) is real.

## Findings

### 1. BLOCKER — `test-golden-catalog.mjs` is wired into a CI step without the Ajv env; contracts job fails on every run

`scripts/test-golden-catalog.mjs:20-31` requires `ajv@8.17.1`, falling
back to `LEKALO_AJV_NODE_PATH`. In `.github/workflows/ci.yml` that env is
set only on the step-scoped `env:` of "Run pinned Ajv schema release
gate" (lines 33-34). The new gate was added at `ci.yml:101` inside "Run
contract checkers and suites" (lines 82-83), which has **no** `env:` and
no `NODE_PATH=` prefix. The repo has no `node_modules` and no root
`package.json`, so both resolution paths fail and the gate exits 1 with
`ajv-8.17.1-unavailable`. Verified: `env -u LEKALO_AJV_NODE_PATH node
scripts/test-golden-catalog.mjs` → exit 1. This fails the contracts job
on both Node 18 and 24 on every push/PR. Fix is trivial (move the gate
into the Ajv step or add the env to the step), but as committed the CI
wiring the report claims ("gates run in the existing contracts job") is
broken.

### 2. MAJOR — `--verify` is a dead flag; declared `expected` golden outputs are never byte-compared

`scripts/run-golden.mjs:36-46` parses `--verify` and never uses the
variable again; the header (lines 5-15) and the implementation report
("`--verify` compares bytes") claim it "compares every expected output
byte-for-byte with the pinned file" — nothing does. Concretely,
`minimal.project` declares four `expected` outputs
(`load/ir/validate-strict/graph` envelopes, `byteMode: cli-json-lf`) that
the runner never even produces: only `cli-validate` executes
(`run-golden.mjs:111-115` record all other runners as `"declared"`
without running them). The catalog gate checks `expected` files for
existence and — only *if* a `digest` field is present, which
`minimal/project/fixture.json` does not set — digest drift
(`test-golden-catalog.mjs:112-125`). So the only thing pinning those
four golden files is git itself; if they were stale or fabricated no
gate would notice. Byte-stability of the *validate* envelope is
separately covered by `test-golden-determinism.mjs` via
`envelopeDigest`/`run-manifest.json`, so AC1 holds for what runs — but
the shipped `expected/` goldens are unverified dead weight, and the
report's claim is wrong.

### 3. MAJOR — committed checksum sidecar ships literal `PLACEHOLDER` digests and no gate reads it

`tests/fixtures/suite/v1/checksums/minimal.project.json` contains
`"sha256": "PLACEHOLDER"` for all eight entries. The README promises
"Every golden file has a `checksums/<case>.json` sidecar naming its
sha256 digest … recomputed by the gates, never hand-patched" — in
reality only one sidecar exists for 20 cases, its values are
placeholders, and no gate ever opens `checksums/` (the catalog gate only
exempts it from the orphan scan, `test-golden-catalog.mjs:273`). Either
fill real digests and wire verification, or delete the directory; as
shipped it is dead fixture data documenting a check that does not exist.

### 4. MAJOR — 10 active registry rules have no evidence but are counted as `test-witness`; the state is unverified and the referenced doc doesn't document it

`tests/fixtures/suite/v1/coverage/diagnostic-rules.json` carries ten
rows (e.g. `adapter.protocol-failure`, `expression.binding-invalid`,
`loader.duplicate-key`, `query.tenant-filter-missing`,
`semantic.portable-target-reference`, `transaction.*` ×4) with
`evidence: "test-witness"` and
`gate: "UNMAPPED — acceptance gap, see docs/m7/issue-90-implementation.md"`.
That document contains no mention of them (`git grep UNMAPPED docs/` →
nothing). Worse, `test-witness` is the only evidence state with no
verifiable anchor: the catalog gate requires only a non-empty array
(`test-golden-catalog.mjs:208-213`) — no path existence check like
`family-fixture` gets (lines 195-205) — so `UNMAPPED` passes by
construction. Per the research's own AC3 language ("an exemption list
must not turn incomplete coverage green"), 10/449 rules have no
positive/negative evidence at all. Honest labeling inside the data is
good; labeling it `test-witness` while the gate cannot check the witness
is not. These rows should be a distinct evidence state the gate counts
separately, or the witnesses should exist.

Related miscount: `docs/m7/issue-90-implementation.md` says suite-pair
covers "all 20 `semantic.*` validator rules" — the registry has 20
`semantic.*` rules but only 19 pairs exist;
`semantic.portable-target-reference` is one of the unmapped rows.

### 5. MAJOR — planner-e2e under-delivers the advertised P0 chain (AC6)

`scripts/test-golden-planner-e2e.mjs` header (line 9) and the
implementation report claim "graph/inspect/impact/context projections";
stage `graph-and-query-projections` (lines 91-102) runs only
`graph export` — `inspect`, `impact`, and `context` are never invoked,
and the "shared symbol" it selects is never queried. The
"upstream-digest-linked" claim also does not hold: `loadEnvelopeDigest`
is computed (line 80) and never read; stage 3 (`semantic-diff-mutation`)
and stage 5 (`trace-manifest-chain`) run against checked-in fixture
paths outside the sandbox with no digest link to stage 1 output. The
chain that does run (load → validate → IR → graph → diff ×2 → 6-scenario
lane → trace) is real and useful, but the AC6 evidence row and the
gate's own contract text overclaim it.

### 6. MINOR — a second, unreviewed update path writes tracked suite goldens directly

`scripts/gen-suite-coverage.mjs:571,628`, `scripts/gen-suite-diagnostic-pairs.mjs`
and `scripts/update-golden-run-manifest.mjs:104` `writeFileSync` straight
into `tests/fixtures/suite/v1/` (catalog, coverage index, run-manifest,
pair projects) — no plan, no semantic summary, no accept digest. The
README/impl report describe updates as going through the plan → review →
apply flow, which in practice covers only `expected/` files.
`test-golden-update-policy.mjs:36-41` checks workflows for the string
`update-golden`, so a CI job invoking `gen-suite-*` would not be caught.
`UPDATE_RECIPES` (`scripts/lib/fixture-catalog.mjs:39`) names
`update-golden-coverage`, for which no script exists, and
`gen-suite-diagnostic-pairs.mjs`'s header points at a nonexistent
`scripts/update-golden-suite.mjs`. Consider routing the generators
through the same plan/apply binding or documenting them as reviewed
regenerators.

### 7. MINOR — dead registry/schema surface

`RUNNER_ARGS` (`scripts/lib/fixture-catalog.mjs:47-53`) is exported but
never imported; of the 14 `RUNNERS` ids the runner executes only
`cli-validate` (`run-golden.mjs:128` hardcodes the argv), so
`cli-load`, `cli-load-ir`, `cli-graph-export`, `cli-trace-export`,
`cli-query-model-validate`, `node-scenario-runner`, `protocol-echo` and
`static-recipe` are declarable but unexecutable — a descriptor naming
them would pass the catalog gate and be silently "declared" by the
runner. `coverage.schema.v1.0.0.json` and `run-manifest.schema.v1.0.0.json`
are never compiled against the artifacts they describe;
`golden-update.schema.v1.0.0.json` is only field-probed
(`test-golden-update-policy.mjs:44-49`), never used to validate a
produced plan.

### 8. MINOR — update-flow byte/digest mismatches

`scripts/update-golden-case.mjs:128-134` computes the plan `digest` over
`row.envelope` (raw stdout) but writes the candidate file as
`` `${row.bytes}\n` `` (line 200), so when the CLI already emits a
trailing newline the recorded digest does not match the candidate file
bytes a reviewer is shown. In `apply` (lines 260-268) `declaredPath` is
taken from the plan (`file.declaredPath`) without re-running the
repo-relative path policy that descriptors get — a poisoned-but-accepted
plan could write outside the case directory. Both are
defense-in-depth/correctness nits in the one place byte honesty matters
most.

### 9. MINOR — the update-policy gate's "tracked tree untouched" self-check can never fail

`scripts/test-golden-update-policy.mjs:124-126` computes
`sha256(readFileSync(sentinel))` twice and compares the values to each
other — an always-true tautology. A before/after snapshot of the suite
tree (as the determinism gate does) was presumably intended.

### 10. MINOR — hygiene gate scans only 2 of the 10 suite scripts

`scripts/test-golden-hygiene.mjs:45-46` scans the fixture tree plus only
`run-golden.mjs` and `lib/fixture-catalog.mjs`, despite the header
claiming "the suite gates" are scanned; the other eight suite scripts
(`test-golden-*.mjs`, `update-golden-*.mjs`, `gen-suite-*.mjs`) are
unscanned. Also, `test-golden-normalization.mjs`'s §4 numeric-precision
and §6 CRLF controls are tautologies (the premises are asserted to exist
but the content check is a no-op).

### 11. MINOR — documentation drift vs. shipped suite

- `tests/fixtures/suite/README.md` lists tracked content for
  `v1/{graphs,queries,diff,scenarios,protocol,security,determinism,integrated}/`
  (F07-F15) — none of those directories contain tracked files; only F01
  (`minimal/`) and F06 (`diagnostics/`) were delivered, plus four
  `importedEvidence` rows. The fixture schema's `class` enum likewise
  lists eight case classes that have zero cases.
- README's example case id `diagnostic.semantic.type-recursion.pair`
  doesn't match the shipped `diagnostic.<slug>.pair` convention.
- Impl report, coverage table: "all 20 `semantic.*` validator rules" →
  19 (see finding 4).
- Eight empty untracked directories (`v1/catalog`, `v1/determinism`,
  `v1/diff`, `v1/graphs`, `v1/protocol`, `v1/queries`, `v1/scenarios`,
  `v1/security`) linger in the worktree — leftovers of the planned
  layout; invisible to git but worth deleting.

## Acceptance-criteria checklist

| # | Criterion (issue text per research §AC table) | Verdict |
| --- | --- | --- |
| AC1 | Byte-stable repeat runs | **Met for executed output.** Determinism gate ran twice here → identical `manifestDigest` `sha256:3c8df36b…`; cold-1/cold-2/warm lanes + committed `run-manifest.json` cross-check + pollution detection are real. But `--verify` is dead and declared `expected` goldens are never compared (finding 2). |
| AC2 | LF/CRLF + path-separator normalization | **Met.** LF-only scan over the whole suite, path/NFC/case-fold/reserved-name controls, `.gitattributes` `eol=lf`; `build-test` runs the binary gates on all three OSes. |
| AC3 | Positive AND negative fixture per rule, or justified state | **Partially met.** 19 real pairs execute both polarities; `family-fixture` paths proven present; `interaction-only` is a real justified state. But 308 `test-witness` rows are unverifiable labels and 10 of them are openly `UNMAPPED` (finding 4). |
| AC4 | Adapter conformance reuses shared fixtures | **Met.** `fixture.rs` `include_str!` paths resolved and digest-compared, duplicate-copy scan, registered-copies allowlist. |
| AC5 | Deliberate update with reviewed semantic summary; never auto-run in CI | **Mostly met.** plan/apply flow works end-to-end (verified); schema requires `semanticChanges` + `humanExplanation`; no workflow references `update-golden`. Weakened by direct-write generators and the digest/candidate newline mismatch (findings 6, 8). |
| AC6 | Planner fixture covers P0 end-to-end chain | **Partially met.** Six stages execute the real binary over shared corpora, but inspect/impact/context never run and digest-linking is claimed-not-implemented (finding 5). |
| AC7 | No secrets / host absolute paths | **Met.** Closed pattern classes + live controls pass; zero exemptions. Scan breadth over suite scripts is narrower than advertised (finding 10). |
| — | Fixture dirs registered in `fixture-provenance.json`; existing fixtures/gates untouched | **Met.** `suite: synthetic` added, provenance gate green (64 families); diff is otherwise 100% additive. |

## Bottom line

Land-blocking: finding 1 (CI red). Findings 2-5 are correctness-of-evidence
issues that should be fixed before the issue is claimed satisfied —
they are all small diffs (wire the flag, fill+verify checksums, split
`UNMAPPED` into its own counted state, add the missing query commands,
delete dead surface). The suite's *architecture* — closed schema pair,
catalog, coverage index, lane manifests, digest-bound update flow — is
sound and the additive discipline is exemplary; what it needs is for the
claims in README/impl-report/doc-comments to match what the gates
actually enforce.
