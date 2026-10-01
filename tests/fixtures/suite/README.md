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
    catalog.json                   the exact case inventory (F-cases, G-outputs, imported evidence)
    coverage/diagnostic-rules.json the per-rule positive/negative coverage index
    coverage/kinds.json            the definition-kind coverage index
    minimal/                       F01 minimal valid project case
    diagnostics/                   F06 per-rule paired trigger/non-trigger projects
    graphs/                        F07 graph/effects projections and their negatives
    queries/                       F08 inspect/impact/context golden projections
    diff/                          F09 semantic-diff classification witnesses
    scenarios/                     F10 scenario outcomes over the shared corpus
    protocol/                      F11 target-protocol request/response transcripts
    security/                      F14 hostile-path / hygiene controls (runtime recipes)
    determinism/                   F14 determinism perturbation declarations
    checksums/                     per-case digest expectations (sha256 sidecars)
    integrated/                    F15 planner P0 end-to-end chain manifest
```

## Authoring contract

- **Case identity.** Every case has a stable dotted ID
  (`minimal.project`, `diagnostic.semantic.type-recursion.pair`, ...),
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
- **Update flow.** Golden updates go through the deliberate
  `scripts/update-golden-*.mjs` flow (plan -> review summary -> apply),
  which prints a semantic review summary bound to before/after digests.
  It is never run automatically in CI; CI only verifies.

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
