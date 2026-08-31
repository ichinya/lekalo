# Privacy and export policy

Status: accepted privacy-contract successor for issue #120. Issue #120 passed
cold review and was closed on 2026-08-30; the accepted product release is
`0.0.2`. The `0.0.1`/`0.0.2` pair frozen inside the accepted manifest is an
acceptance-time snapshot, not living product state; the living release is
governed by the versioning policy artifact.

The normative files are:

- `contracts/privacy-policy.v1.0.6.json`: accepted policy `1.0.6`;
- `contracts/privacy-policy.v1.0.6.manifest.json` and its `.sha256` sidecars: exact
  custody anchors;
- `contracts/privacy-authorizing-evidence.v1.1.0.json` and its sidecar: exact,
  purpose-bound authorizing-evidence registry;
- `contracts/privacy-authorization-subject-profile.v1.0.0.json` and its sidecar:
  exact canonical authorization-subject projection;
- `contracts/privacy-policy.v1.0.2.classification.json`: exact classification
  decision contract `1.0.0`;
- `contracts/privacy-export.schema.v2.5.json`: strict input schema `2.5.0`;
- `contracts/privacy-export.schema.v2.5.output.json`: strict output schema `1.5.0`;
- `contracts/privacy-cli-error.schema.v1.0.0.json`: separate closed startup/custody
  exit-1 protocol; it is not an `ExportDecisionOutput`;
- `contracts/privacy-export.schema.v2.classification.json`: classification-ref
  schema `1.0.0`;
- `scripts/check-privacy.mjs`: dependency-free reference evaluator.

The ADR is [ADR-0002](adr/0002-privacy-export-policy.md). If explanatory prose
and the machine-readable accepted contract differ, consumers must stop; prose cannot
broaden policy.

## Independent identities and lifecycle

| Identity | Current value | Meaning |
|---|---|---|
| Accepted product release | `0.0.2` | Current M0 product state after #120 acceptance (`0.0.1` is the historical M0 release); a product number, never a policy or schema version |
| Privacy policy | `dev.lekalo.privacy-export-policy@1.0.6` | Accepted corrective successor |
| Decision contract | `dev.lekalo.privacy-export-decision@1.5.0` | Evaluator I/O semantics |
| Input schema | `dev.lekalo.privacy-export-input-schema@2.5.0` | Strict input shape; exact refs refreshed, semantics unchanged |
| Output schema | `dev.lekalo.privacy-export-output-schema@1.5.0` | Strict output shape with corrected current classification/evidence refs |
| CLI startup error schema | `dev.lekalo.privacy-cli-error-schema@1.0.0` | Separate pre-evaluation exit-1 JSON |
| Classification contract/schema | `dev.lekalo.privacy-classification-decision@1.0.0` / schema `1.0.0` | Exact classification custody |
| Authorizing-evidence contract | `dev.lekalo.privacy-authorizing-evidence@1.1.0` | Exact field/purpose/outcome registry |
| Authorization subject profile | `dev.lekalo.privacy-authorization-subject-profile@1.0.0` | Complete canonical decision subject |
| Authority contract | `dev.lekalo.authority-matrix@1.3.1` | Exact 49-kind registry |

Privacy `1.0.0` is recorded as `rejected-unaccepted-wip`. Frozen `1.0.1` is
recorded as `rejected-after-corrective-review`, `accepted:false`, with its exact
policy, manifest and `2.0.0`/`1.0.0` schemas preserved byte-for-byte on the old
`v1` paths. Frozen accepted `1.0.2` is now yanked after the third independent
review, with its exact policy, manifest, schemas and sidecars preserved on their
versioned paths. Frozen accepted `1.0.3` is now yanked after the fourth
independent review, with its exact policy, manifest, schemas and sidecars
preserved byte-for-byte. Frozen accepted `1.0.4` is yanked after exact review
and preserved byte-for-byte. Frozen accepted `1.0.5`, including its output
schema `1.4.0`, is yanked after exact review and preserved byte-for-byte.
Accepted `1.0.6` is an explicit successor, not a silent
mutation. See the [1.0.1 to 1.0.2 migration](privacy-policy-migration-1.0.1-to-1.0.2.md)
and the [1.0.2 to 1.0.3 migration](privacy-policy-migration-1.0.2-to-1.0.3.md),
the [1.0.3 to 1.0.4 migration](privacy-policy-migration-1.0.3-to-1.0.4.md), then
the [1.0.4 to 1.0.5 migration](privacy-policy-migration-1.0.4-to-1.0.5.md), then
the [1.0.5 to 1.0.6 migration](privacy-policy-migration-1.0.5-to-1.0.6.md).
M0 uses product `0.0.x`; M1 later starts product `0.1.0`.

Exact accepted reference:

```json
{
  "policyId": "dev.lekalo.privacy-export-policy",
  "version": "1.0.6",
  "digest": "sha256:99a813a89efbdf336340390c9589a4f05d0dbbc8805748708b455a3d7a329ca7"
}
```

Exact authority reference:

```json
{
  "contractId": "dev.lekalo.authority-matrix",
  "version": "1.3.1",
  "digest": "sha256:5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3"
}
```

The policy digest is SHA-256 over a deterministic canonical JSON projection of
the complete policy with only `policyRef.digest` omitted: object keys are
sorted recursively, array order is preserved, and JSON primitive encoding is
used. This avoids a self-reference. Exact policy/schema/authority bytes are
separately pinned by a manifest whose exact SHA-256 is hard-coded in the
checker and stored in a sidecar. A caller cannot replace the policy, recalculate
a digest, or supply a local taxonomy: replaced/missing manifest or sidecar,
changed bytes, stale references, and alternate authority kinds fail before a
decision is evaluated.

## Strict input and output

`ExportDecisionInput` is a closed object. It requires exact decision,
authority, policy, authorizing-evidence and authorization-subject-profile references; `artifactKind`; an
opaque `artifact-sha256:` ref; a versioned operation;
source and destination repository-role aliases plus opaque `repo-sha256:`
references; destination trust boundary,
repository relation and tenant relation; audience; a non-empty unique
`dataSensitivity[]`; exactly one scalar `exportDisposition`; strict
provenance; resource-path representation; conflict state; constraints; and an
explicit nullable broadening grant. Provenance repeats the source role/ref and
uses an exact classification-contract reference. Measured values use
`valueState`.

Every authorizing field has a separate closed evidence shape anchored to the
accepted evidence registry. Public-fixture permission, license and consent;
consumer-repository ACL permission and consent; general transfer consent;
conflict resolution; declassification; and aggregation each have distinct
constant `evidenceKind` and `purpose`. An evidence record also declares the
exact contract triple, outcome, opaque `evidence-sha256:` identity,
verification/freshness state, and a `subject-sha256:` binding under the exact
authorization-subject profile. A generic `auditRef` is non-authorizing
metadata only and cannot affect `allow` or `transform-required`.

The subject projection clones the complete strictly validated
`ExportDecisionInput`, excludes only the nine authorizing evidence objects and
the non-authorizing `constraints[].constraintRef`, and then applies the
profile's declared set ordering. It includes every other field by construction:
classification and custody refs, kinds/opaque identities, operation, complete
repository/tenant/audience context, provenance flags, path/value state,
sensitivities/disposition, source artifacts, transforms and their evidence,
declassification/aggregation semantic outcomes, constraints and the nullable
grant. Recursive object-key ordering and declared set ordering make equivalent
source/label/transform/constraint permutations stable. Any other semantic
change either becomes malformed or changes the subject digest, so unchanged
evidence denies with `evidence.binding-mismatch`.

All nested objects are action-specific and closed. Missing, unknown, extra,
duplicated, stale, or ambiguous metadata is a fail-closed result. Exact
duplicates in every set-like input collection are malformed: main and source
sensitivities, constraints and their allowed-value arrays, source artifacts,
applied transforms, and removed sensitivities. A repeated opaque identity with
different content is a well-formed semantic conflict and denies. Vocabularies
are versioned and closed; private repository names, URLs and local role aliases
are not inputs.

`ExportDecisionOutput` is also closed. It returns exactly one of `allow`,
`deny`, or `transform-required`, at least one stable reason code, required
transforms, effective exact policy/authority/classification/evidence/subject-profile references,
requirements for a new derived artifact, and whether the evaluated source may
transfer.
`transform-required` never authorizes transfer of the source. It requires a
new artifact with new provenance, retained source references, recorded
transforms and declassification/aggregation evidence, followed by a fresh
evaluation.

CLI protocol: exit `0` for contract-valid/no-decision or `allow`, exit `1` for
malformed input or contract custody failure, and exit `3` for `deny` or
`transform-required`. A shape-valid decision that is semantically malformed
still emits a closed `ExportDecisionOutput` deny before exit `1`. Startup,
parse and custody failures emit the separate closed CLI error object
`{status:"invalid",reasonCodes:[...]}` on stderr; that object never claims the
output-schema identity and must not be treated as an export decision.

## Orthogonal classification vocabularies

Sensitivities are `public`, `internal`, `confidential`, `personal-pii`,
`credential-secret`, `financial`, `health-special-category`, `tenant-scoped`,
and `retention-limited`.

The six dispositions have distinct semantics:

| Disposition | Meaning |
|---|---|
| `local-private` | Local use/derivation only; no repository, network, or trust-boundary transfer |
| `consumer-repository-only` | Every operation requires an originating consumer-repository source/provenance role, non-null coherent repository ref and ordinary consumer origin; repository storage additionally requires the same repository/origin/tenant plus ACL permission and consent; no cross-repository, cross-tenant, or public transfer |
| `shareable-with-redaction` | Source may be used/stored locally; transfer/publish requires a new transformed artifact and reevaluation |
| `public-aggregate` | Only a reevaluated derived aggregate without source rows or identities and with exact purpose-bound aggregation/declassification evidence |
| `public-fixture` | Exclusively synthetic, or backed by three distinct exact, verified, current and context-bound permission, license, and consent records for every source-transfer/storage operation (`repository-store`, `transfer`, and `publish`) |
| `forbidden-to-export` | Local use only; dominates any export allow, grant, or conflict resolution |

Every sensitivity label remains independent of disposition. The evaluator
intersects the allowed operations, destinations, repository/tenant relations,
and audiences for every label. A deny from any applicable label dominates;
label order does not affect the result. Missing rules, duplicates, unknowns,
conflicts and missing consent deny.

## Closed-exact default table

Defaults cover exactly the 49 stable authority `1.3.1` kind IDs, in registry
order. There is no copied taxonomy, extension set, or alias layer.

| Artifact kind | Default disposition |
|---|---|
| `openspec.requirement` | `consumer-repository-only` |
| `openspec.change-intent` | `consumer-repository-only` |
| `openspec.delta-spec` | `consumer-repository-only` |
| `openspec.expected-behavior` | `consumer-repository-only` |
| `ai-factory.execution-plan` | `consumer-repository-only` |
| `ai-factory.task-state` | `consumer-repository-only` |
| `ai-factory.runtime-state` | `local-private` |
| `ai-factory.agent-lifecycle` | `local-private` |
| `ai-factory.provider-evidence-envelope` | `shareable-with-redaction` |
| `lekalo.semantic-model` | `consumer-repository-only` |
| `lekalo.stable-symbol` | `consumer-repository-only` |
| `lekalo.effect` | `consumer-repository-only` |
| `lekalo.scenario` | `consumer-repository-only` |
| `lekalo.target-binding` | `consumer-repository-only` |
| `lekalo.observed-model-draft` | `local-private` |
| `lekalo.cache` | `local-private` |
| `lekalo.generated-intermediate` | `local-private` |
| `hlv.validation-result` | `shareable-with-redaction` |
| `hlv.traceability-result` | `shareable-with-redaction` |
| `hlv.gate-diagnostic` | `shareable-with-redaction` |
| `hlv.project-contract` | `consumer-repository-only` |
| `source-native.source-code` | `consumer-repository-only` |
| `source-native.native-test` | `consumer-repository-only` |
| `generated.summary` | `shareable-with-redaction` |
| `generated.rule` | `consumer-repository-only` |
| `generated.code` | `consumer-repository-only` |
| `authority.contract` | `consumer-repository-only` |
| `authority.contract-manifest` | `consumer-repository-only` |
| `privacy.policy` | `consumer-repository-only` |
| `privacy.export-schema` | `consumer-repository-only` |
| `diagnostics.raw-tool-output` | `forbidden-to-export` |
| `context.capsule` | `local-private` |
| `trace.manifest` | `shareable-with-redaction` |
| `ai.prompt` | `forbidden-to-export` |
| `ai.response` | `forbidden-to-export` |
| `ai.tool-transcript` | `forbidden-to-export` |
| `metrics.evaluation-evidence` | `shareable-with-redaction` |
| `fixture` | `public-fixture` |
| `repository.identity` | `forbidden-to-export` |
| `native.symbol-identity` | `consumer-repository-only` |
| `native.source-map` | `forbidden-to-export` |
| `consumer.model` | `consumer-repository-only` |
| `consumer.target-bindings` | `consumer-repository-only` |
| `export.artifact` | `consumer-repository-only` |
| `export.decision` | `local-private` |
| `redaction.artifact` | `consumer-repository-only` |
| `redaction.decision` | `local-private` |
| `aggregate.artifact` | `public-aggregate` |
| `aggregate.decision` | `local-private` |

Unknown kinds and any missing/extra/duplicate default fail closed. These
defaults never change authority ownership or allowed paths.

## Precedence, narrowing and safe defaults

Custody and strict shape validation occur before decision rules. Exact refs and
registered default come next; unknown value/path states, unresolved or denied
conflicts, forbidden dominance, destination/operation profiles, the
intersection of all sensitivity labels, local constraints, consent/provenance,
path rules, derived-source custody, and disposition branching then apply in
the machine-readable order. No matching allow means deny.

Project, profile and operation constraints are set intersections with the
baseline. They may only narrow it and cannot introduce vocabulary. Broader
values deny. Broadening would require an exact reviewed grant already embedded
in the trusted policy, and the accepted policy contains no grants. A caller-provided
or recomputed local policy cannot legalize broadening.

Secrets never become public or cross-repository. Direct PII does not become
public. Tenant-scoped data does not cross a tenant boundary. Source snippets,
raw prompts/responses/tool output, private repository identity and native
source maps use restrictive defaults. Public fixtures are exclusively
synthetic or carry exact purpose-bound permission/license/consent evidence. For a
non-synthetic fixture, all three refs are mandatory for repository storage,
transfer and publication. Missing consent denies every source-transfer/storage
operation. Reusing one evidence identity across purposes, a wrong purpose,
stale/expired/unverified evidence, a wrong outcome or a binding mismatch fails
closed. A wrong-shaped or unknown evidence contract is malformed.

## Provenance, derivation, paths and values

Every artifact carries an exact
`dev.lekalo.privacy-classification-decision@1.0.0` reference and a stable
repository role plus opaque `repo-sha256:` reference. Provenance modes are
mutually exclusive: `origin:derived` means `derived:true, synthetic:false`;
`origin:synthetic` means `synthetic:true, derived:false`; every other origin
requires both flags false. `same-repository` and
`same-origin` require equal non-null source/destination refs and equal roles;
different repository boundaries require different non-null refs. Provenance
role/ref and origin/synthetic/derived flags must agree.

A derived artifact additionally retains every source kind, an opaque stable
`source-sha256:` ref, exact authority/policy/classification refs, sensitivities
and default disposition. Source kinds must exist in authority `1.3.1`; stale or
unknown classification contracts deny. An exact duplicate source is malformed;
one source ref with conflicting classifications denies; distinct sources are
evaluated as a permutation-invariant set. Applied transforms carry exact transform
version and evidence digest. Removing any source label requires an exact
purpose-bound declassification decision naming precisely those labels. Public
aggregates additionally require exact purpose-bound aggregation evidence, no source rows or
identities, the aggregate transform, and `reevaluated: true`.

Paths are either normalized NFC project-relative POSIX paths or a state-only
representation. Drive, UNC, rooted, home-relative, URI, `..`, backslash,
encoded path syntax, control characters, empty/dot/non-NFC segments, trailing
dots/spaces, NTFS alternate streams, DOS device names (including compatibility
aliases `COM¹`/`COM²`/`COM³`, `LPT¹`/`LPT²`/`LPT³`, `CONIN$`, `CONOUT$` and
`CLOCK$`) and short-name-like
segments deny. `withheld` permits fully offline local use when a representation
cannot safely be supplied; `unknown` and `unsupported` deny. Physical
containment (including link/junction resolution and post-open checks) remains a
downstream adapter obligation and is not claimed by this metadata evaluator.

Measured values use exactly `known`, `unknown`, `withheld`, or `unsupported`.
Only `known` carries a value. Numeric zero is `{"state":"known","value":0}`
and is distinct from all three state-only alternatives.

## Normative consumption seam

| Issue | Must consume | Remains owned there |
|---|---|---|
| #87 | Exact fields/refs and classifications without vocabulary overrides | Semantic propagation and data-flow projections |
| #119 | Exact input/output, reason codes and fail-closed result | Runtime envelope, scanning, redaction, leak enforcement, transfer pipeline, and trusted repository-token binding with integration adapters such as #89 |
| #121 | Exact refs and value-state semantics | Local history, retention and measurement persistence |
| #102 | Only reevaluated public aggregates with exact refs | Publication and disclosure workflow |

All four consumers use one exact policy/decision contract. #120 does not read a
private consumer repository and does not implement any of those downstream
systems.

## Offline evaluator and evidence boundary

After startup custody loading, `evaluateDecision(input, context)` is a pure
metadata evaluator. It reads only the declared input and preloaded immutable
contract context; it performs no filesystem, network or private-repository
access. Synthetic fixtures and subprocess tests exercise `allow`, `deny`, and
`transform-required`, all dispositions, operations, destinations, audiences,
multi-label intersection, provenance, refs, paths, values and mutation attacks.

```sh
node scripts/check-privacy.mjs
node scripts/check-privacy.mjs --decision path/to/decision.json
node scripts/test-privacy-contracts.mjs
node scripts/test-privacy-decisions.mjs
node scripts/test-privacy-coherence.mjs
node scripts/test-privacy-review-regressions.mjs
node scripts/test-privacy-schema-parity.mjs
node scripts/test-privacy-authorizing-evidence.mjs
node scripts/test-privacy-authorization-subject.mjs
node scripts/test-privacy-output-schema.mjs
node scripts/test-privacy-cli.mjs
```

Opaque repository refs prove only coherence of declared tokens. They are
minted, bound to a physically resolved repository context, and freshness-checked
only by trusted #119 runtime envelopes and integration adapters such as #89.
Caller-fabricated equality is not proof of physical identity. Missing, stale or
unverified binding must fail closed before evaluator invocation or before its
decision is accepted. No forgeable binding-attestation field is accepted by
the #120 input contract.

The same boundary applies to authorizing evidence. Trusted #119 runtime
envelopes and #89 integration adapters mint opaque evidence identities, compute
the exact current subject profile after strict validation/normalization, bind
them to its digest and physically resolved authorized context, verify authoritative
permission/license/consent or decision sources, and establish freshness.
#120 checks only exact declared contract, purpose, outcome, status and binding
coherence. It does not claim that caller-declared evidence is cryptographically
or physically authentic.

This proves contract custody and metadata decisions only. It does not inspect
payloads, find secrets/PII/source text, redact or export data, propagate
classifications, persist history, publish aggregates, verify physical path
containment or repository-token binding, or access private repositories.
