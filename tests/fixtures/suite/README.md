# Suite v1 — the unified deterministic golden fixture suite (issue #90)

This directory owns the **versioned fixture-suite harness** for Lekalo:
a catalogued, byte-stable, cross-platform suite of golden cases that
protects the semantic contracts and determinism of `lekalo-core` and
`lekalo-cli`. It is **additive**: every existing fixture family under
`tests/fixtures/<family>/` and every existing gate stays untouched.

## Layout

```text
suite/
  README.md                        this file
  schema/                          closed JSON Schema documents for the suite metadata
  v1/
    catalog.json                   the exact case inventory + imported evidence rows
    run-manifest.json              the pinned cold-1 execution manifest (AC1 anchor)
    coverage/diagnostic-rules.json the per-rule evidence index over all 449 active rules
    coverage/kinds.json            the definition-kind coverage index
    minimal/project/               F01 minimal valid project + 4 golden envelopes
    diagnostics/<rule>/            F06 paired trigger/non-trigger projects (one per rule)
    checksums/<case>.json          per-case sha256 sidecars (verified by the catalog gate)
    importedEvidence (in catalog)  registered shared corpora owned by other families

  Delivered in fix round 1: F01, F06, the coverage index, the run
  manifest, and the integrated P0 chain as an executed gate
  (scripts/test-golden-planner-e2e.mjs) rather than an extra fixture
  directory. F07-F14 coverage continues to live in the existing
  families (graph/, impact/, diff/, scenario/, target-protocol/, ...)
  and is registered through the catalog's importedEvidence rows;
  new suite-owned case directories are added by the reviewed update
  flow as they are produced.
```

## Authoring contract

- **Case identity.** Every case has a stable dotted ID
  (`minimal.project`, `diagnostic.type-recursion.pair`, ...),
  a `fixtureSchema` (`dev.lekalo.fixture@1.0.0`) and a monotonically
  increasing `revision` (integer, starts at 1). A corrected expectation
  bumps `revision` with a reviewed rationale in the commit message.
- **Paths.** All descriptor paths are slash-separated and
  repository-relative; no `..`, no absolute paths, no host spellings.
  Dangerous path inputs are *content recipes* inside security cases,
  never descriptor paths the runner follows.
- **Bytes.** Wire bytes stay LF-only (`.gitattributes` pins LF).
  Canonical JSON is compact with sorted keys where the producer sorts;
  golden bytes are compared exactly. Every golden file has a
  `checksums/<case>.json` sidecar naming its sha256 digest
  (`sha256:<hex>`), recomputed by the gates, never hand-patched.
- **Product pins.** Cases pin the product contracts they exercise
  (e.g. `dev.lekalo.ir@0.2.16`, diagnostic registry `0.4.0`, protocol
  `0.3.2`) independently of the suite schema version. Bumping the suite
  harness never bumps a product contract.
- **Runners and recipes.** Runner IDs resolve to a closed registry in
  `scripts/lib/fixture-catalog.mjs`. Fixture data can never supply
  arbitrary executable paths or shell strings.
- **Update flow.** Golden `expected/` updates go through the
  deliberate `scripts/update-golden-case.mjs` flow
  (plan -> semantic summary -> digest-bound apply). Metadata artifacts
  (catalog, coverage index, run manifest, checksums, pair projects)
  are maintained by reviewed regenerators (`gen-suite-coverage.mjs`,
  `gen-suite-diagnostic-pairs.mjs`, `update-golden-run-manifest.mjs`,
  `update-golden-checksums.mjs`) whose output the gates verify against
  the registries and tracked bytes; CI never invokes any of them.

## Gates

| Gate | Responsibility |
| --- | --- |
| `scripts/test-golden-catalog.mjs` | catalog/schema/provenance validation, exact case inventory, unique IDs, path safety |
| `scripts/test-golden-normalization.mjs` | LF/newline/path normalization vectors and byte-policy controls |
| `scripts/run-golden.mjs` | read-only verifier: materialize, execute, compare bytes, emit receipts |
| `scripts/test-golden-determinism.mjs` | repeat-run byte stability (cold cold + warm), run manifest comparison |
| `scripts/test-golden-diagnostic-coverage.mjs` | registry-to-case coverage index for all 449 active rules |
| `scripts/test-golden-adapter-shared.mjs` | shared fixture reuse across core/Node/PHP consumers |
| `scripts/test-golden-planner-e2e.mjs` | the P0 end-to-end chain |
| `scripts/test-golden-hygiene.mjs` | no secrets / host paths in the tracked suite |
| `scripts/test-golden-update-policy.mjs` | the deliberate update flow behaves as specified |

## Honest coverage statement

The registry embeds 449 active rules across 51 subsystems. Many of those
rules are exercised by *other* committed suites (typed-attachment
negative matrices, runtime gates, Rust unit/integration tests) rather
than by projects this suite can execute through the CLI. The coverage
index (`coverage/diagnostic-rules.json`) enumerates **every** rule with
an explicit evidence state:

- `suite-pair` — this suite owns a trigger + non-trigger project pair;
- `family-fixture` — an existing committed fixture family pins the rule
  (named family + file), verified to still exist by the gate;
- `test-witness` — a named Rust/Node gate asserts the rule with
  injected inputs (the gate must exist for the entry to stay green);
- `interaction-only` — reachable only through live host interaction
  (process kill, exclusive locks, platform I/O errors); named in the
  index and verified by rule-family scan, not by a stored project.

An entry with no evidence fails the gate. This is the documented,
honest backlog: `suite-pair` coverage grows over time without the gate
ever pretending a rule is covered when it is not.
