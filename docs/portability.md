# Portability

Status: **Implemented** for the assessment surface (`lekalo portability`): the
capability-based, four-dimension portability report over the typed IR, exact
target/profile pins, and qualified recorded evidence. Native execution and
migration itself stay out of scope; this command never launches anything.

Owner: portability

## What the command answers

```bash
lekalo portability
lekalo portability --from node-typescript --to php-laravel
lekalo portability --to go-service --json
```

One report binds one exact source target and one or more destination targets.
Every classification is a capability question ("what does the destination
profile resolve to, what does the recorded evidence prove"), never a
hard-coded product-name comparison. Unknown capability is never treated as
supported, and unsupported never collapses into untested or unknown.

## The four dimensions

Each subject is assessed per destination across the orthogonal dimensions the
reviewed recipe assigns to its definition kind:

| Dimension | Question it answers |
| --- | --- |
| Structural | Do the type, references, and annotations carry a target-specific risk? |
| Behavioral | Does the destination have the operation hook (generated, custom, foreign, external), and does a qualified mapping cover it? |
| Storage | Do required semantic policies (decimal, isolation, ordering, null-handling, rounding) survive onto the destination's offered policies? |
| Operational | Do declared publication obligations and qualified operational requirements join the destination's declared capabilities? |

## Reading a report

- **Portable** — the destination covers the obligation with no recorded loss.
- **Supported with mapping** — a reviewed semantic mapping credits the row;
  the mapping is scoped to exactly this destination and names exactly this
  obligation's requirement, bridges through a reviewed recipe definition
  (exact definition-version pin, reviewed preconditions only), and the
  mapped capability must be declared by the destination profile's resolved
  capability set at full support. A mapping authored for another target,
  another requirement, or with unreviewed pins/conditions is invisible or
  unqualified here — never a credit.
- **Partial / behavior loss** — restricted coverage (a weaker capability, a
  narrowed decimal policy, an unsupported implicit hook). These rows block
  the strict gate.
- **Target-specific** — declared on the subject or reached through the
  bounded dependency closure; needs an explicit replacement obligation.
- **Unsupported / missing** — an explicit verdict from a complete
  implementation inventory, never a guess.
- **Unknown** — an evidence gap with a name (`evidenceNeeded` on the
  planning row says what to collect). Unknown is not failure and not safety.

The report carries three separate surfaces: the per-subject obligation rows,
the per-destination dimension summaries (the class counts partition eligible
applicable subjects at their conservative primary class — a subject with
several obligations in one dimension counts once — and the published ratio
is structural `portable / eligibleApplicableSubjects`, an integer ratio with
an explicit basis: one percentage never hides the risk classes behind it,
and the human output renders all four dimensions and names any clipping), and the
advisory migration surface: stable planning rows with actions
(`implement-hook`, `replace-mapping`, `review-storage-policy`, `bind-target`,
`verify-scenario`, `collect-evidence`), each with prerequisites, affected
scenarios and gates, and the evidence still needed. A recorded scenario
failure surfaces as its own `failed` join state, never as "planned", and a
failure dominates any sibling pass for the same scenario.

With no destination resolved at all (a bare run whose selection leaves no
other target), the report is the structural inventory plus an explicit
`destination-evidence-gap` diagnostic — no supported destination is ever
fabricated, and the strict gate denies the empty join.

## Inputs and provenance

- **Targets configuration** (`config/portability-targets.json`): the closed
  binding of target ids to exact profile documents and digest pairs, plus the
  source/destination defaults. Absent defaults derive from the declared Model
  targets.
- **Evidence manifest** (`--evidence`): digest-pinned families, recorded
  adapter snapshots, reviewed semantic mappings (destination- and
  requirement-scoped), required/offered storage policies, operational
  requirements, scenario receipts, gates, and observed implicit hooks.
  Every claimed execution result — scenario or gate — is digest-bound
  custody through the closed receipt contract: the referenced receipt
  artifact must exist, hash to the pinned digest, decode through
  `lekalo/portability-receipt/v0.8.0`, name exactly the claimed subject
  and destination, pin this exact compilation and the destination's
  verified profile, and state a result that agrees with the claim — a
  fail receipt can never evidence a pass, and a contradicting claim
  refuses outright. An authored `executed` gate state without qualifying
  custody demotes to `declared` with a named gap; an affected scenario
  no gate record covers is an explicit `gate-coverage-unknown`
  indication, never a silently complete surface. A semantic mapping or
  bridged operational requirement is admitted only when it pins the
  bridge's reviewed definition version with every pin for the capability
  in agreement and declares exactly the bridge's reviewed precondition
  set for its own destination (an empty list is never a vacuous credit,
  and another target's vocabulary never admits this one). Contradictory
  same-(target, kind) storage offerings refuse at decode regardless of
  order. A family declared complete must present its artifact through
  its owner envelope (`lekalo/portability-family/v0.8.0`): readable,
  digest-matching, project-pinned custody whose complete storage
  inventory equals the manifest's declared inventory — a content-free
  or contradicting object never certifies completeness. A recorded
  adapter snapshot qualifies only through the closed
  `lekalo/portability-adapter-snapshot/v0.8.0` artifact it references:
  every field must reconcile exactly, so recorded provenance is never
  fabricated from manifest fields. The manifest pins the exact project
  IR revision and the exact reviewed recipe; anything stale is refused
  before assessment.
- **Profile documents**: resolved through the existing target-profile
  resolver; the report carries the exact id, version, declaration digest,
  effective digest, component versions, and composed capabilities of every
  pinned profile.
- **Lock**: when `lekalo.lock` exists at the project root, it is decoded
  through the strict lock parser and reconciled against the selected
  targets' exact profile pins before its digest is recorded in the
  report's input pins; garbage bytes or a contradicting pin refuse. The
  `known` state is reserved for locks that corroborate every selected
  pinned profile — a lock that corroborates nothing selectable reports
  `unreconciled`, never known.

Every artifact read goes through the confined, reparse-checked project
filesystem: a symlink or junction inside the project can never redirect a
read outside the root, a multiply-linked file (a checked-in hardlink to
foreign bytes) is refused outright, and every declared reference (profile
document, storage profile, adapter evidence, implementation artifact) is
read and digest-verified before it may influence any verdict.

## The strict gate

```bash
lekalo portability --check
```

`--check` denies (exit 3, report preserved in the envelope) when any required
obligation is unresolved: unsupported or missing hooks, lossy, partial, or
explicitly unsupported storage policies, unresolved target-specific
replacement work, unknown evidence, a truncated extraction, or an empty
assessment. A pass means the assessed scope has no known unresolved
obligation — it is never a claim that two native implementations behave
equivalently.

## Contracts

- `lekalo/portability-targets/v0.8.0` — the targets configuration.
- `lekalo/portability-evidence/v0.8.0` — the evidence manifest.
- `lekalo/portability-recipe/v0.8.0` — the reviewed extraction recipe
  (embedded; the report binds its digest).
- `lekalo/portability-family/v0.8.0` — the complete-family artifact
  envelope (storage, extended-effects, scenario, trace).
- `lekalo/portability-receipt/v0.8.0` — the execution-receipt custody
  artifact (scenario and gate joins).
- `lekalo/portability-adapter-snapshot/v0.8.0` — the recorded adapter
  snapshot custody artifact.
- `lekalo/portability-report/v0.8.0` — the closed report shape, also the
  machine contract the migration-planning consumer reads.
