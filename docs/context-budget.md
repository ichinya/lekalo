# `lekalo context-budget`

Issue #75: context-budget and local-understandability metrics — how much
deterministic context one symbol, module, or project needs for an AI to
work under a bounded content budget, and when context explodes.

The command is read-only and advisory by default: a report always
renders its evidence, and only an explicitly selected mandatory policy
can turn a finding into an exit-3 denial. There is no default budget
and no universal threshold; a caller without an explicit budget or
named profile gets a usage error.

## Command

```sh
lekalo context-budget --symbol planner.focus_task --budget 12000 --json
lekalo context-budget --module planner --budget 12000
lekalo context-budget --all --budget 12000
lekalo context-budget --symbol planner.focus_task --budget-profile local-12k \
  --profiles config/context-budgets.json --json
lekalo context-budget --symbol planner.focus_task --budget 12000 \
  --simulate-capsule --suggest --json
lekalo context-budget --all --budget-profile local-12k \
  --profiles config/context-budgets.json --baseline reports/base.json
lekalo context-budget --all --budget-profile local-12k \
  --profiles config/context-budgets.json --policy ci/policy.json
```

Exactly one of `--symbol`, `--module`, `--all` selects the scope.
Exactly one of `--budget N` (the generic chars-4 profile with an
explicit content-token budget) and `--budget-profile ID --profiles
FILE` (a named profile inside a closed profile document) is required;
the two are exclusive. `--profile-version` selects the profile version
inside the document (default `1`).

## Flags

| Flag | Meaning |
| --- | --- |
| `--budget TOKENS` | Explicit content-token budget under the pinned offline chars-4 estimator. |
| `--budget-profile ID` | Resolve one named profile from `--profiles`. |
| `--profiles FILE` | The closed budget-profile document (`lekalo/context-budget-profile/v0.6.3`). |
| `--simulate-capsule` | Attach the legacy-capsule simulation at the same budget plus the new required/supporting selection. |
| `--suggest` | Emit advisory extraction-boundary suggestions (data only; never executed). |
| `--source-context RECIPE` | `none` (default) or `mapped-files`. The mapped-files recipe is declared but reports honestly `unsupported` in this generation (the artifact-evidence adapter is not wired). |
| `--baseline FILE` | Compare against an immutable prior report; the envelope carries a `contextBudgetComparison` block with signed deltas. |
| `--policy FILE` | The closed mandatory policy (`lekalo/context-budget-policy/v0.6.3`); the only exit-3 path, and it pins the effective-profile digest. |
| `--project DIR` | Project root selector, relative to the invocation directory. |

## Exit classes

| Status | Exit | When |
| --- | --- | --- |
| `valid` | 0 | Advisory result, including over-budget subjects (the `LEK-CONTEXT-006` warning rides the envelope). |
| `invalid` | 1 | Malformed selector, budget, profile, policy, or baseline; oversized input documents. |
| `denied` | 3 | The selected mandatory policy failed on over-budget, required-incomplete, baseline-required, baseline-incomparable, or baseline-regression. The full report rides the denied envelope. |
| `unsupported-version` | 5 | The profile, policy, or estimator pin is outside the accepted registry (no fallback). |

## The report

The canonical payload (`--json`) is the closed
`lekalo/context-budget-report/v0.6.3` contract: provenance pins (Model
version, IR digest, graph/effect identities, policy/baseline evidence),
the pinned profile and estimator, per-subject metrics M1–M10 under the
four-state wrapper (`known`/`unknown`/`withheld`/`unsupported`; only
`known` carries a value), the explainable dependency breakdown, the
required/supporting fact ledger, closed confidence gaps, advisory
suggestions, the opt-in simulation, and a reconciling summary.

Key reading:

- `metrics.minimumRequiredSemanticTokens` — the deterministic required
  semantic fact set F (measured before any budget selection).
- `metrics.minimumSafeContextEstimate` — F under the profile's
  margin/framing: a structural estimate, never a claim of AI success
  (`empiricallySafeContextTokens` stays unknown until #100 calibration).
- `metrics.transitiveDependencies` — all distinct reachable dependency
  ids, including the direct ones; `indirectOnlyDependencies` is the
  exact difference.
- `subjects[].breakdown` — per-dependency attribution that sums
  (exclusive + shared) to the required total.
- `assessment` / `overByTokens` — the budget verdict and its exact
  remainder under the effective profile cost.

## Profiles

```json
{
  "schemaVersion": "lekalo/context-budget-profile/v0.6.3",
  "identity": "dev.lekalo.context-budget-profile@0.6.3",
  "profiles": [
    {
      "id": "local-12k",
      "version": "1",
      "estimator": {
        "id": "dev.lekalo.estimator.chars-4@0.2.16",
        "version": "0.2.16",
        "specDigest": "sha256:602e648c2ff7c58cace92876f6c834c5c1735ce594e3ba565d7b012f752fb0be"
      },
      "budget": {
        "contextWindowTokens": 16384,
        "reservedOutputTokens": 2048,
        "reservedSystemToolTokens": 2336
      },
      "selection": { "version": "required-semantic-facts/1", "sourceContext": "none" },
      "limits": { "maxNodes": 50000, "maxEdges": 250000, "maxFacts": 50000, "maxSubjects": 10000 }
    }
  ]
}
```

`availableContentTokens = contextWindowTokens − reservedOutputTokens −
reservedSystemToolTokens`; a nonpositive result refuses. The effective
profile digest binds estimator + selection + budget + limits and every
report embeds it; baseline comparability requires the same digest.

## Policy

```json
{
  "schemaVersion": "lekalo/context-budget-policy/v0.6.3",
  "identity": "dev.lekalo.context-budget-policy@0.6.3",
  "mode": "mandatory",
  "profileRef": { "id": "local-12k", "version": "1", "digest": "sha256:…" },
  "failOn": ["over-budget", "required-incomplete", "baseline-regression"],
  "regressionLimits": [
    {
      "metric": "minimumRequiredSemanticTokens",
      "absoluteIncrease": 1000,
      "relativeIncrease": { "numerator": 1, "denominator": 10 }
    }
  ]
}
```

A regression denies when **either** allowance is exceeded (equality
passes; the relative bound skips a zero base). A policy that selects
`baseline-regression` without `--baseline` denies with
`baseline-required` — a mandatory check is never silently skipped.

## Boundaries

No Git access, no provider calls, no writes, no generation, no scans.
Raw source text, file bytes, absolute paths, and timestamps never enter
the report. The measured structural estimates are not claims about
actual provider token usage or task success.
