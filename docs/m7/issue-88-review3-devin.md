# Issue #88: Devin review round 3 (residual fix verification)

**Verdict: ACCEPT.** The residual P2 from Codex r2 is fixed in `880e0639` and
verified. See `issue-88-fix2.md`.

## Residual — producer-domain claim bypass: VERIFIED

- `producer_domain_admitted` now verifies the reserved `model` target claim
  against the core Model producer's identity recipe: finding ID binds
  (rule, subject, reserved target, semantic-symbol state); Model/IR known and
  project-verified; revision = canonical hash of the Model/IR pair;
  adapter/capabilities explicitly unsupported; native path unknown; lint
  selector must belong to the Model producer recipe — a native-only selector
  cannot claim Model ownership.
- The mechanism is sound because the finding ID is a hash over the bound
  target: relabeling `target:"model"` would require a different ID that no
  longer matches the native-emitted occurrence — the bypass path from the
  review is structurally closed.
- `fingerprint_requirements` grants Model-only relaxation only after domain
  admission; the shared matcher re-checks at effectiveness, so matching stored
  decisions cannot bypass either.
- Gate: `producerDomainControls:9` across all three successor families —
  disguised native evidence with relabeled target refused in add preview/apply
  (`waivers-ineligible-candidate`) and marked unverifiable in audit; positive
  Model-domain case retained; capability golden still valid.
- Core: 12/12 waiver tests (+3 producer-domain); all four gates green;
  predecessor 0.6.4 family still validates.

## Combined fix state

F1 (applicable-pin enforcement) + residual (domain-claim verification) are both
active: `nativePinControls:7` and `producerDomainControls:9` in one gate.
