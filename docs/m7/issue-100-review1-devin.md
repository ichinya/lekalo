# Issue #100: Devin review round 1

**Verdict: ACCEPT** for the delivered scope — deterministic offline protocol,
admission, and replay. Actual external A/B campaigns are honestly pending;
nothing fabricates live results.

Reviewed `7cf81e23` at branch HEAD on `ichinya/m7-issue-100`, base `a2aa1893`.

## Verified live

- `cargo build --locked -p lekalo-cli`: clean.
- `test-framework-lift-contracts.mjs`: **49 checks, all 5 families live**
  (baseline/task/campaign/arm/result), all 6 `evaluation.*` negative codes
  exercised, `externalAgentRuns: 0`, fixture origin `recorded-simulation`.
- `lekalo evaluation --help`: honest surface — validate/preflight/record-arm/
  compare; "Never launches an agent/provider or exports private artifacts".
- Registry: in-place growth of the in-flight `0.6.4` artifact 500→506
  (`LEK-EVAL-001..006`), matching cycle convention (#35/#76/#77 did the same);
  `version.rs` attribution updated honestly.
- Core lib: **1051 passed**; docs-ownership live-help 348 surfaces (`evaluation`
  owned); contract-versions 6/6; provenance 82 families; fmt clean.

## Honesty checks confirmed

- Hard assertions vs judge signals are typed separately; a hard failure with
  judge 100 is preserved as failure (negative fixture).
- Missing scheduled arms stay `not-started` in the denominator — attrition is
  visible, never silently dropped.
- Result scope is `tested-task-profile-only`; marginal Wilson intervals are
  explicitly not a paired lift CI.
- Private campaigns require local provider policy; `private-egress-denied` is a
  first-class negative code; no raw prompt/source/URL in result format.
- #84 dependency is seam-only: architecture-profile ref optional, no contract
  names assumed.

## Merge-order note (not a defect)

#100 grows the `0.6.4` registry in place while #84 introduces the `0.6.5`
successor. Rule sets are disjoint (`evaluation.*` vs `architecture-profile.*`),
and the two-registry dispatch in #84's `for_version` handles both correctly.
Post-merge, version.rs must carry both attributions and Cargo.toml resolves to
0.6.5. No reparenting required.
