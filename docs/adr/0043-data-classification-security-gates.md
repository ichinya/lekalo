# ADR-0043: Data classification, secret/PII boundaries, and sensitive-effect gates

Date: 2026-09-20
Status: accepted for issue #87

Custody: three new independent contract families at v0.4.0 — the
data-classification attachment (`lekalo/data-classification/v0.4.0`,
identity `dev.lekalo.data-classification@0.4.0`), the
classification-policy attachment
(`lekalo/classification-policy/v0.4.0`, identity
`dev.lekalo.classification-policy@0.4.0`), and the derived data-flow
report (`lekalo/data-flow-report/v0.4.0`, identity
`dev.lekalo.data-flow-report@0.4.0`) — plus the `classification.*`
(`LEK-CLS-001..012`) and `dataflow.*` (`LEK-DFL-001..009`) diagnostic
families on the shared diagnostic-registry v0.4.0 successor, the
`preserve.classification` capability definition on the
`dev.lekalo.target-capabilities@0.4.0` registry, and the
`classification.preservation` conformance check. The contract versions
are independent of the product release, of the Model/IR/effect-graph/
transport contract versions, of the authorization, extended-effects,
NFR, and privacy-policy families, and of the diagnostic registry item
wire.

## Context

Issue #87 requires data classification with secret/PII boundaries and
sensitive-effect gates. Three facts shaped the accepted design:

1. **The seam already exists.** The #14 effect graph carries an opaque
   `Sensitivity` marker (`contract`, `classified`) on every edge, and
   the #26 extended-effects family already gates publications and
   external calls with typed destinations, consent/approval, and
   opaque security-gate review references. Nothing populated the
   marker; no fine-grained kind vocabulary existed behind it.
2. **The Model/IR is closed.** `ir.unknown-field` fails closed; no
   classification member can be added to any IR type without a
   reviewed Model successor. #70 (transport) and #85 (NFR) solved the
   same problem by binding *by reference*: an independent versioned
   attachment pinned to the exact `projectId`/`modelRef`/`irRef`
   custody triple, addressing symbols and bounded field paths
   resolved against the pinned compilation.
3. **Enforcement is owned elsewhere.** #119 owns the runtime
   redactor/export enforcer; #120 owns the privacy/export policy
   documents. #87 declares *that* masking, retention, export, and
   encryption are required (by exact references into #120/#85), and
   computes gate *decisions* — it never strips values at a sink.

## Decision

### Attachment over IR extension

Classification attaches purely by reference, exactly as #70 and #85
did. One bounded subject grammar — a dotted semantic id plus zero to
two `/`-separated field segments (definition, field, and payload
level, with `list`/`optional` `TypeRef` wrappers unwrapped) — addresses
every required surface: fields, types, payloads, entities, commands,
queries, events, and endpoints. Subjects resolve against the pinned
compilation; unknown symbols and paths reject with
`classification.unknown-subject`.

### A total, deterministic resolution lattice

Ten closed kinds (`public`, `internal`, `confidential`, `personal`,
`credential`, `financial`, `health`, `tenant-scoped`,
`retention-limited`, `derived`) carry a total lattice rank. Resolution
is the first match of: the exact field-path entry, the
definition-level entry, and the profile default (`unclassifiedFields`
vs `unclassifiedPayloads`) — and the result **widens** to the most
restrictive contributor, never silently lowers. No entry plus no
covering default is the `unclassified` state, distinct from every
kind; the strict profile rejects unclassified subjects on sensitive
sinks. `credential` is sealed: it has no destinations and no
declassification roles in policy, and a grant lowering it is a
structured `classification.invalid-declassification` refusal, not a
finding.

### Declassification is never implicit

Every lowering is a subject-bound grant with an exact id, an opaque
review reference (`SecurityGate.review` shape), a bounded
justification, closed condition tokens (`aggregated`, `anonymized`,
`consent-obtained`, `pseudonymized`, `suppressed`), and an optional
expiry. Self-approval, missing roles, and non-lowerings reject.

### Gates are decisions, not enforcement

The derived data-flow report pins the exact canonical digests of its
two inputs, projects one flow per classified source-to-sink edge with
provenance, confidence, tenant relation, and the gate decision
(`{required, satisfied, reason}`), and carries a first-class unknown
list. Unknown is never safe: an unresolvable hop, partial adapter
outcome, or incomplete observed graph degrades the flow, blocks its
gate, and (for incompleteness) blocks gate satisfaction project-wide.
`credential`, `personal`, `financial`, `health`, and `tenant-scoped`
kinds are cross-tenant-forbidden by default.

### Adapter honesty

The `preserve.classification` capability (registry 0.4.0) lets an
adapter declare whether classification metadata survives its
projections. The `classification.preservation` conformance check
(security class) passes an honest `unsupported`/`unknown` refusal —
the frozen observed-scan wire cannot carry kind tokens, and refusing
is the plan's fail-closed requirement — and fails any support claim
the wire cannot yet represent, because an unverifiable claim *is* the
silent lowering the check exists to catch.

## Boundaries

- No runtime redaction, export enforcement, or sink rewriting (#119).
- No privacy-policy semantics; every `masking`/`retentionRef`/
  `exportRef` is an exact reference into the #120 family (#120).
- No encryption requirements; `encryptionRefs` references exact #85
  NFR constraint ids (#85).
- No IR/Model change; the canonical project layout gains exactly two
  optional declaration homes (`lekalo/classification.json`,
  `lekalo/classification-policy.json`), file-typed and closed.
- No transport or NFR coupling on this branch; the seams are recorded
  in the issue plan (§5) and re-verified at M4 integration.

## Consequences

- `lekalo classification validate|inspect` and
  `lekalo dataflow report` join the CLI; `lekalo validate` runs the
  classification review when the attachment is present (strict
  profile: error findings invalidate; default profile: recorded,
  never silently skipped — a present-but-broken attachment is invalid
  in every profile). The review runs before the authorization review
  so a broken attachment is never masked by an unrelated denial.
- The `Sensitivity` markers on declared effect edges carry
  `dev.lekalo/data-classification@0.4.0` (the effect-graph wire's
  namespaced spelling) once any kind resolves for the edge's subject;
  the wire shape is unchanged.
- The three 0.4.0 schemas pass `check-contract-versions` at the M4
  product version (the earlier pending-bump state is resolved on
  merge).
- The diagnostic-registry v0.4.0 successor created here is shared
  custody with #85 (its thirteen `nfr.*` rows are preserved
  byte-semantically and the additive chain is proven by
  `scripts/test-classification-contracts.mjs`); whichever sibling
  lands second merges rather than duplicates.
