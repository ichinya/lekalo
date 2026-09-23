# Data classification and the data-flow report (issue #87)

One project may declare **what its data is** (the classification
attachment), **who and what may touch it** (the classification-policy
attachment), and receive a derived, read-only projection of **where it
flows and where the gates stand** (the data-flow report). All three
are metadata-only: they carry kind tokens and references — never
field values, source text, physical paths, runtime principals, tokens,
secrets, or provider transcripts.

## The two declared attachments

Both live at the project root under the canonical layout:

- `lekalo/classification.json` — `lekalo/data-classification/v0.4.0`
  (`dev.lekalo.data-classification@0.4.0`): custody triple
  (`projectId`, `modelRef`, `irRef`), per-profile `defaults`, the
  `classifications[]` entries, and the `declassifications[]` grants.
- `lekalo/classification-policy.json` —
  `lekalo/classification-policy/v0.4.0`
  (`dev.lekalo.classification-policy@0.4.0`): per-kind rules
  (readers/writers over the closed authorization scope dimensions,
  the closed destination set, the exact masking/retention/export
  references into the #120 privacy family, the exact
  `encryptionRefs` into the #85 NFR family, `consentRequired`,
  `crossTenant`, `declassifyRoles`) and the six sink ceilings
  (`logs`, `traces`, `contextCapsules`, `diagnostics`, `evidence`,
  `exports`).

Both documents are optional; a project that declares one must declare
the other (`classification.policy-missing` otherwise), and a
present-but-broken attachment is invalid in every profile.

### Subjects

A subject is a dotted semantic id alone (definition level) or with a
bounded field path under it:

```
core.entity.user                 # the whole entity
core.entity.user/email           # one field
core.command.create_user/payload/ssn   # a payload field through a value object
```

`list` and `optional` type wrappers unwrap transparently; unknown
symbols and unknown paths are `classification.unknown-subject`.

### Kinds and resolution

The closed kinds, ascending by rank: `public`, `internal`, `derived`,
`retention-limited`, `tenant-scoped`, `confidential`, `financial`,
`personal`, `health`, `credential`. Resolution for any concrete
subject is total and deterministic: the exact entry, then the
definition-level entry, then the profile default — and the result
widens to the most restrictive contributor, never silently lowers.
No entry plus no covering default is the **unclassified** state
(distinct from `public`): a warning in the `default` profile, an
error on sensitive sinks in `strict`.

Hard rules:

- `credential` is sealed. Its policy row must list no destinations
  and no declassification roles; a grant lowering it is a structured
  refusal (`classification.invalid-declassification`), never a pass.
- Every lowering needs a grant: an exact id, an opaque review
  reference, a bounded justification, optional closed conditions
  (`aggregated`, `anonymized`, `consent-obtained`, `pseudonymized`,
  `suppressed`), and an optional expiry (a grant past `expiresAt`
  is a `classification.expired-declassification` finding — dead
  grants never lower anything, on any surface: validate, inspect,
  the exposure rule, the consent gate, and the report all share one
  validity predicate). The fixed reference date for expiry and
  validity evaluation is `2026-01-01T00:00:00Z`; the CLI flag
  `--as-of` or the environment variable `LEKALO_AS_OF` can override
  this date on validation surfaces (`classification validate`,
  `classification inspect`, `dataflow report`). Self-approval rejects
  (`classification.self-approved`) — a depth-defense check: the wire
  grammar already makes it unreachable on parsed input (an `approvedBy`
  review reference forbids `@`, a grant id requires it), so the rule
  protects library callers constructing grants in memory.
  The `declassifyRoles` check is structural: `approvedBy` is an opaque
  reference with no role token, so validation verifies the from-kind
  rule declares roles; binding the approving role to the declared set
  needs the issuer story (ADR-0043 §9).
- `personal`, `credential`, `financial`, `health`, and
  `tenant-scoped` are cross-tenant-forbidden by default
  (`crossTenant: forbidden`).

### The sensitivity marker on effect edges

When the attachment is present, every declared effect edge whose
subject resolves to any kind carries the opaque
`Sensitivity` marker `dev.lekalo/data-classification@0.4.0`
(`state: classified`) in the effect-graph canonical bytes. Edges
without classification coverage stay unmarked. The wire shape is
unchanged; the marker is a reference, never a value.

## The derived data-flow report

`lekalo dataflow report` derives
`lekalo/data-flow-report/v0.4.0` (`dev.lekalo.data-flow-report@0.4.0`)
from the pinned compilation plus the exact canonical digests of the
two attachments (`classificationRef`, `policyRef`). The report is
read-only, never an input to itself, and never feeds validation as a
declaration.

- `flows[]` — one row per classified source-to-sink edge: the closed
  `sinkKind`, `provenance` (`canonical`/`observed`/`declared`),
  `confidence` (`high`/`low`/`unknown`), `tenantRelation`
  (`same`/`crossing`/`unknown`), the resolved kind, and the gate
  decision `{required, satisfied, reason}` on gated sinks.
  Note: `tenantRelation: same` is based on a declared-partitioned
  heuristic (entity declares a field named `tenant`, `tenant_id`, or
  `*_tenant_id`) and not an enforced tenancy declaration.
- `findings[]` — registered `classification.*`/`dataflow.*` rule
  violations with bounded detail tokens. Subjects are hashed inside
  diagnostics; the report rows reference paths.
- `unknowns[]` — the first-class unknown list. Unknown is never safe:
  an unresolved subject, dynamic hop, foreign implementation,
  partial adapter outcome, or incomplete observed graph degrades the
  flow, blocks its gate, and (for incompleteness) blocks gate
  satisfaction project-wide.
- `verdict` — `pass` only when inputs were complete and no
  error-severity finding exists; otherwise `denied`.

### Gated sinks and the emit-event boundary

The gated sink set — the surfaces whose destination, approval/consent,
and forbidden-destination rules the analyzer evaluates — is
`external-call`, `publication`, `cache-write`, `export`, and the
public-endpoint response. `event-publish` (the projection of declared
`emit-event` edges) is deliberately **not** in that set: a declared
event edge is an intra-model domain event consumed inside the model
boundary, while external publication is the detected `publish-output`
kind (`publication`, gated per ADR-0013 §3). This is a documented
boundary, not an oversight; cross-tenant safety for emitted events
still rides the tenant rule (a `forbidden` kind emitting with an
underivable tenant relation is treated as crossing). Gating
`event-publish` itself waits for sink-actor bindings on the event bus
(the same observed-evidence seam as the gated detected kinds).

## Commands

```console
$ lekalo classification validate --attachment lekalo/classification.json \
    --policy lekalo/classification-policy.json
classification valid: 11 subject(s), 0 findings

$ lekalo classification inspect --attachment ... --policy ... --json
{"status":"valid","subjects":[{"subject":"notify.user","kind":"personal"},...]}

$ lekalo dataflow report --attachment ... --policy ... --json
{"status":"valid","report":{"schemaVersion":"lekalo/data-flow-report/v0.4.0",...}}
```

`lekalo validate` runs the classification review automatically when
the attachment is present: the `strict` profile invalidates on any
error-severity finding; the `default` profile records it without
failing (the findings stay visible through
`lekalo classification validate`).

## Adapter preservation

Adapters declare `preserve.classification` (`full`, `partial`,
`unsupported`, or `unknown`) on the describe capability map. The
`classification.preservation` conformance check (security class)
passes an honest refusal and fails any support claim the projection
wire cannot represent: an unverifiable claim is exactly the silent
lowering the check exists to catch (`dataflow.adapter-metadata-loss`
is registered as the analysis-side rule and gains its producer when
the observed-evidence seam lands — it is not emitted today).

## Boundaries

- Masking/redaction enforcement and export enforcement: #119.
- Privacy/export policy semantics: #120 — this family only references.
- Encryption-at-rest/in-transit requirements: #85 NFR constraints.
- No Model/IR change: classification binds by reference.
- Contract versions are independent of the product release and of
  every sibling family; see ADR-0043 for the full decision record.
