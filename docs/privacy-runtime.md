# Privacy runtime (issue #119)

Status: the runtime enforcement layer of the frozen #120 contract
family. The normative policy index is [privacy.md](privacy.md); this
document describes the in-core runtime that enforces it: the typed
evaluator, the redaction engine, the leak scanner, the export
pipeline, and the CLI surface. If this prose and the machine-readable
contracts differ, consumers must stop; prose cannot broaden policy.

## The enforcement model

The runtime is metadata-only by construction. The decision layer
never inspects payload contents; the redaction and leak layers never
emit a matched value, offset, timestamp, or absolute host path. The
pieces:

| Piece | Home | Role |
|---|---|---|
| Frozen references | `privacy::refs` | The exact accepted policy/authority/evidence/profile/schema identities and pinned digests |
| Trusted context | `privacy::context::TrustedContext` | The contract bytes embedded from `contracts/`, custody-verified at load (digests, sidecars, manifest acceptance, registry closure, default table) |
| Typed decision input/output | `privacy::input`, `privacy::output` | The exact closed member sets of the strict input/output schemas; deny-unknown construction by type |
| Evaluator | `privacy::evaluate::evaluate_decision` | The exact port of the reference evaluator: same rule order, same reason-code spellings, same outcomes (parity-gated) |
| Redaction engine + leak scanner | `privacy::redact` | The closed transform set and the eight-class leak scanner |
| Export pipeline | `privacy::export::run_export` | Class resolution, decision-input synthesis, evaluation, transforms, verification pass, and the reserved writes |

## Decision-input synthesis

`lekalo privacy export` reads an artifact envelope: a JSON object
with the required members `artifactKind` (a closed authority-registry
kind) and `payload`, and the optional members `class` (an array of
closed #120 sensitivity labels), `synthetic`/`derived` (origin
booleans), `repository` (a declared repository identity name for the
scanner subject), `protectedTerms` (declared person names), and
`confinement` (the #89 evidence document, see below).

The class resolves as follows (fix round 2, C-F2):

1. the envelope `class` member is parsed (unknown labels refuse);
2. when the project classification attachment parses
   (`classification.json`, under the #87 contract, with the governing
   policy validated when present), its unclassified-payload default is
   mapped onto the #120 label on the restrictive side (`credential` →
   `credential-secret`, `personal` → `personal-pii`, `health` →
   `health-special-category`, `derived` → `internal`, and so on) and
   **unioned into the effective set** — a claim below the declared
   floor widens and never silently lowers (the #87 propagation
   doctrine);
3. no claim and no attachment — the export refuses with
   `privacy.class-missing` before any evaluation runs.

`synthetic: true` on the envelope is honored only when corroborated:
the artifact must sit inside a project-local
`tests/fixtures/<family>/` directory whose entry in the project's
`tests/fixtures/fixture-provenance.json` declares
`origin: "synthetic"`. Uncorroborated claims drop to the
non-synthetic origin, so the evaluator's public-fixture evidence
requirements apply. `derived` stays claimed (claiming derived adds
requirements in the evaluator — self-limiting).

Fail-closed: a missing or unknown class refuses before evaluation;
an invalid classification attachment refuses; an incomplete envelope
refuses.

The destination is selected by the closed `--destination` spec:
`workspace`, `repository-store`, `transfer-tenant`,
`transfer-external`, `transfer-cross-tenant`, `publish`. Each maps
onto the exact operation/destination/audience vocabulary with
coherent repository-backed sources for transfers and stores. The
repository refs are declared-coherence opaque tokens: the SHA-256 of
`"lekalo.repository-identity\n"` plus the custody basis — the
classification attachment bytes when present, else the artifact
envelope bytes — scoped per endpoint role. The token is deterministic
across clones and proves custody coherence, never host identity or
host layout (fix round 2, C-F3); the physical binding, freshness, and
containment checks remain the #89 adapter obligation and are not
claimed here.

Optional authorizing evidence is supplied with `--consent FILE` (the
export-transfer-consent position). The record goes into the decision
input **verbatim** — the runtime never mints, adds, or corrects any
evidence member, and never computes a binding on the caller's behalf
(fix round 2, C-F1). `binding` is a required evidence member: an
absent binding fails input-shape validation (exit 1); a declared
binding that mismatches the computed subject digest denies
`evidence.binding-mismatch` (exit 3). The evaluator owns the check
(identity reuse, unverified, stale, expired, outcome mismatch,
binding mismatch). Declared evidence proves shape, coherence, and
subject binding only; issuance and authenticity custody is the
evidence-store obligation (#121).

Author evidence against the canonical subject with the `lekalo
privacy subject` verb:

```sh
lekalo privacy subject --artifact artifact.json --destination transfer-tenant [--project DIR]
```

It prints `{subjectDigest, subjectProfileRef}` of the synthesized
decision input — metadata-only; the evidence positions are excluded
from the subject projection, so authoring never needs a fixpoint.

## Transform vocabulary

The evaluator's `requiredTransforms` names the closed transforms of
the pinned vocabulary. The engine implements:

| Transform | Behavior |
|---|---|
| `redact-content` | The payload body is dropped to a deterministic kind+class stub (`{"class":[…],"kind":"redacted-content"}`); the stub carries no content fingerprint — a hash over a low-entropy body would be trivially reversible, which the contract forbids |
| `redact-secrets` | Secret tokens and credential assignments become `<redacted:secret>` |
| `redact-pii` | Emails, declared person names, and phone-like runs become `<redacted:pii>` |
| `replace-repository-identity` | The declared repository name becomes `<repo:{role}:{n}>`, never the real name |
| `normalize-project-relative-paths` | Absolute/host paths become deterministic `<path:Hn>` role aliases |

The scanner classes `url` and `tenant-id` map to the closed markers
`<redacted:url>` and `<redacted:tenant>`. Path pseudonymization and
repository-identity replacement are always-on hygiene: they apply to
every export payload regardless of the required transforms, and they
are deterministic role aliases over a small domain — never bare
hashes.

## Leak scanner and the fail-closed matrix

The scanner detects eight closed classes: secret tokens (including
JWTs), URI-scheme URLs (any `[a-z][a-z0-9+.-]*://` scheme), absolute
and drive-relative path fragments, emails, phone-like digit runs
(separated groups and bare 9–16-digit runs), declared person names,
tenant ids, and declared repository names. It runs inside every
transform and as the verification pass over the final payload.

Honest scope: "never silently ships" means never ships a payload that
still matches the **closed class vocabulary**. Content outside that
vocabulary is the export decision's job (classification + disposition
+ evidence), not the scanner's; prose cannot broaden the class set.

| State | Outcome |
|---|---|
| `deny` from the evaluator | Exit 3 with the exact reason codes; nothing written |
| `transform-required` | The closed transforms apply; the scanner verifies the candidate; clean candidates are written under `.lekalo/privacy/exports/` with the decision record under `.lekalo/privacy/decisions/export/` |
| Residual leak over the candidate | The export refuses (exit 3, `leak.<class>` codes) — never silently ships |
| Malformed envelope / unknown class / invalid attachment | Refusal before evaluation (exit 3 for policy refusals, exit 1 with the closed `{status:"invalid",reasonCodes:[…]}` object for malformed input) |
| Unknown artifact kind | The evaluator denies `artifact-kind.unknown` |
| Missing consent for a transfer/store/publish | The evaluator denies `provenance.consent-required` |
| `--dry-run` | The exact candidate payload and the redaction diff print; nothing is written |

## The #89 integration seam

An adapter-produced artifact may carry its `confinement` evidence
(the issue #89 document). Where present, the export pipeline reads
it, requires the closed member set to be coherent (a malformed
evidence refuses), and records its digest in the decision record.
The evidence is metadata custody only: physical containment,
freshness, and binding remain the adapter obligation, and this read
grants adapters no new filesystem or network scope.

## Gates

- `scripts/check-privacy.mjs` — the reference evaluator and its
  custody chain (Node).
- `scripts/test-privacy-*.mjs` — the contract corpus gates (Node).
- `scripts/test-privacy-evaluator-parity.mjs` — the Rust evaluator
  must reproduce the JS evaluator's outcomes over the full pinned
  corpus (120 vectors).
- `scripts/test-privacy-runtime-cli.mjs` — the `lekalo privacy`
  surface end to end, including the written-bytes-are-secret-free
  proofs.
- `scripts/test-privacy-leak-corpus.mjs` — the synthetic leak corpus
  (`tests/fixtures/privacy-leaks/`): every declared class must appear
  in the redaction diff, every refusing class must refuse the export
  with its `leak.*` code, and hygiene classes must ship
  pseudonymized.
- `scripts/test-fixture-provenance.mjs` — every fixture family is
  declared synthetic or evidence-backed.

## Receipt envelope propagation (fix round 2, C-F5)

The exportable receipt surfaces (scan receipt, generate receipt)
carry the additive optional `class`/`policyRef` members. The
propagated `class` is the declared project **floor** — the #120 label
of the classification attachment's unclassified-payload default —
not a per-artifact resolution. An invalid attachment propagates no
claim; absence means "no trustworthy claim", and an export attempt
re-derives the class at enforcement time (unioning the floor into any
envelope claim) and refuses fail-closed when nothing trustworthy is
declared.
