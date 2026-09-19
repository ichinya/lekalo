# ADR-0042: NFR constraints as a dedicated family with a separate evidence document

Date: 2026-09-18
Status: accepted for issue #85

## Context

Issue #85 asks for non-functional requirements as versioned semantic
constraints bound to measurable evidence, without a declaration ever
posing as a proven guarantee: endpoint latency mapped to performance
gates, declarations without measurement reported unverified, results
from incompatible environments never merged, constraint changes
reaching scenarios and gates through the impact graph, advisory NFRs
that do not block unless the profile says so, published schemas and
fixtures, and runtime NFRs kept separate from AI-generation budgets.

The research plan (`.m4/issue-85-plan.md`) established the reusable
foundations: the attachment-family pattern (#36/#63/#64/#66/#30), the
evidence/status-coherence model of #63, the strict-profile gate
precedent of #12/#16, the typed changed-input handoff of #16, the
neutral #22 trace manifest, and the #120 valueState vocabulary.

## Decisions

**D1 — Dedicated family, not an extension of #36 (owner decision).**
The requirements contract is external-provider traceability; NFRs are
Lekalo-owned semantic declarations with volatile evidence. A
constraint's `sourceRequirement` references the `{source, requirement}`
pair as plain data with the same grammar #36 defines; #36 stays
untouched, so provider traceability and Lekalo-owned constraint
semantics never conflate.

**D2 — Evidence in a separate document.** Measurements arrive
continuously; embedding them would bump the constraint document and
churn every pinned digest. Evidence pins `constraintId +
constraintRevision`, so a constraint edit invalidates only its own
evidence — unlike #63, where evidence pins the whole
`attachmentRevision`. The evidence document is also the neutral wire
HLV/AIFHub/native runners emit.

**D3 — Exact-equality environment compatibility.** Env id, both owner
digests, runtime, platform, and sorted labels must all be equal;
partial matches ("same runtime, different region") are foreign. A
compatibility-classed successor (`same-platform`, `same-profile`) can
relax this later; v1 stays exact, because a silently merged measurement
is worse than a visible foreign row.

**D4 — Status vocabulary and the unverified rule.** `satisfied`,
`violated`, `unverified`, `stale`, `foreign-environment`,
`open-question`, `unsupported`, `conflict` — computed by the report
engine, never declared. No wire spelling asserts satisfaction; #120's
valueState vocabulary (`known` + value, `unknown`, `unsupported`,
`withheld`) keeps every absent measurement an explicit absence, never a
fabricated zero. Contradictory receipts at one environment and revision
are an explicit `conflict`, never an average.

**D5 — Advisory semantics and "profile".** "Does not block unless the
profile says so" is interpreted as the gate profile (`--strict`), the
#16 impact precedent: default denies mandatory
violated/unverified/stale/unsupported/conflict rows only; strict
escalates advisory violated/unverified/stale; `open-question` rows
never block under either profile. The #12 validation-profile contract
is deliberately not extended — it governs `semantic.*`/`validate.*`
rules only. If the owner later wants an NFR section there, it is a
successor schema.

**D6 — Impact integration via the typed handoff.** A constraint change
reaches `impact::analyze` through a synthesized `ChangedInputSet` (the
attachment's project-relative logical path, `modified`, canonical
evidence, one entry per affected scope symbol). Zero impact contract
changes. A ninth `nfr` risk dimension remains the deferred alternative
for a future owner decision.

**D7 — Determinism.** `stale` is revision mismatch or expiry before the
injected as-of date; `--as-of` is a required explicit CLI argument and
a required resolve parameter. The core never reads a clock.

**D8 — Registry increment.** The `nfr.*` family registers
LEK-NFR-001..013 on the additive 0.4.0 registry successor. The plan
pinned 001..012; the exit-class-4 requirement (an evidence file absent
or unreadable maps to `unavailable`) needs a rule admitting that
status, so LEK-NFR-013 `nfr.evidence-unavailable` was added — recorded
here as the single deviation from the plan's diagnostic table.

## Consequences

- The constraint attachment, evidence document, and report are
  independently versioned and independently consumable; measurement
  churn never touches constraint declarations.
- Bound-less kinds rely on the owner's pass receipt rather than a
  numeric comparison; a successor can add declared numeric bounds for
  retry budgets and consistency staleness without a wire break (the
  kind grammar is closed, so this requires a successor contract, not an
  in-place edit).
- `nfr.constraints` as an optional `lekalo verify` component and the
  ninth impact risk dimension stay open follow-ups.
