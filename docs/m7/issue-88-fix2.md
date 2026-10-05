# Issue #88: fix 2 for the residual producer-domain finding

Status: **Implemented and locally verified**, 2026-10-05. This addresses the P2
in [Codex review round 2](issue-88-review2-codex.md). Starting HEAD:
`f71a5b5b`; branch `ichinya/m7-issue-88`, with a clean starting worktree/index.
Delivery is a conventional local fix commit, with no push or changes to other
worktrees.

## Finding and correction

Previously, the policy selected Model-only applicability from `fact.target`
alone. A native `hidden.string-reference` fact with unsupported capabilities
could change only its target to `model`, pass add preview/apply, and generate
accepted-risk audit/done evidence. The direct native decoder refused the Model
target, but that check did not establish the neutral input's producer domain.

`ProfileState::producer_domain_admitted` now checks the reserved Model claim
against the current core producer's existing identity/provenance recipe:

| Binding | Model-domain requirement |
| --- | --- |
| Finding identity | The ID must match the core Model producer's hash of its rule, subject, reserved target and semantic-symbol state |
| Model/IR | Both known; CLI admission still verifies them against the selected current project |
| Revision | Known and equal to the canonical hash of that Model/IR pair |
| Adapter/capabilities | Both explicitly unsupported, as emitted by the Model producer |
| Native path | Unknown, as emitted by the Model producer |
| Lint selector | Belongs to the actual core Model producer recipe; a native-only lint selector cannot claim Model ownership |

The current core Model producer is semantic dependency depth. Its rule and
finding identity are owned by `ai_lint/mod.rs`, not by a waiver severity list.
The shared `finding_id` helper keeps emitted identities byte-identical while
allowing the policy to verify the producer binding. Model-derived capability
facts retain that source finding identity and continue to consume their actual
resolved target-profile policy. The existing capability golden remains valid.

`fingerprint_requirements` grants Model-only applicability only after this
admission succeeds. The shared matcher independently checks producer-domain
admission at effectiveness, even when every native fingerprint is known.
An inconsistent reserved-target claim becomes `unverifiable`, effective false,
with the existing `fingerprint-unverifiable` reason. Matching stored decisions
cannot bypass this check. Add preview/apply use the same audit and refuse with
`waivers-ineligible-candidate` before writing.

`crates/lekalo-cli/src/waivers.rs::Loaded` delegates producer-domain admission
to the admitted validation/lint/target state, alongside fingerprint requirements.
The new trait method has a default implementation; this is an additive source
seam. Future #84 profile owners can supply their admitted producer recipes
through it without an assumed future wire. Severity and required-evidence
eligibility still come from the existing resolved profiles. Non-Model facts
retain the required-known native dimensions introduced by fix 1.

This verifies consistency with the recognized producer recipe; it does not
authenticate arbitrary external producers or re-execute their tools. The
documented ownership of supplied neutral facts remains explicit. A free target
label can no longer grant inapplicability to an occurrence whose retained
identity/provenance belongs to a native producer.

## Regression controls

Three new waiver unit tests exercise correctly approved stores bound to the
actual changed facts, isolating producer admission from stale hashes:

- `waivers_model_domain_cannot_be_claimed_by_relabelling_a_native_fact`: native
  finding identity relabelled Model, with both known and unsupported capability
  pins; neither case grants acceptance.
- `waivers_model_domain_requires_the_complete_producer_binding`: changed ID,
  subject, symbol, revision, adapter and path each remain unverifiable. Project
  scope keeps the occurrence matched so scope refusal cannot mask admission.
- `waivers_model_lint_binding_cannot_claim_a_native_only_selector`: a complete
  Model binding still refuses the native-only string-reference selector; the
  genuine Model selector remains effective under the same current lint profile.

The existing Model inapplicability test now uses the actual producer-bound ID
and canonical Model/IR revision rather than an arbitrary Model-labelled ID.
Its unsupported adapter/capability positive case and all unknown/withheld or
missing required-pin negative cases remain enabled.

`scripts/lib/waivers-contract-gate.mjs::producerDomainCases` adds **nine live
controls** to each of the three successor gates: one genuine Model positive,
six negative authoring/audit cases, and two mixed-input order cases. The
negative cases cover unsupported capability, unsupported adapter, both missing,
all-known native pins, a native finding ID recomputed with the Model label, and
a reconstructed Model binding that still claims a native-only selector.
Each negative asserts preview/apply exit 1 with the ineligible reason and
unchanged store bytes, plus a structurally valid matching stored decision whose
audit is unverifiable, waived 0, and exit 3. The mixed cases retain the genuine
Model acceptance while the relabelled native occurrence remains blocking in
both orders. All retained audits, seven native pin controls, expiry, stale pins,
non-waivable profiles, closed decoding and path/plan safeguards remain enabled.

## Independent replay of the review evidence

A separate script, without importing gate helpers, completed **23 real CLI
invocations** in disposable projects outside this checkout. It used the actual
locked round-2 reproduction's applied store, input and captured done receipt.

| Probe | Current result |
| --- | --- |
| Actual old applied relabelled store/input | Locked audit exit 3; `unverifiable`, effective false, `fingerprint-unverifiable`, waived 0; original fact and store bytes retained |
| Old accepted-risk done digest | Exit 1, `waivers-done-stale` |
| Replay old preview and exact old apply plan against an absent store | Both exit 1, `waivers-ineligible-candidate`; no store created |
| Actual native lint with that old store | Exit 3, waived 0; direct native protection remains effective |
| Genuine newly emitted Model occurrence | Locked preview/apply/audit and live lint exit 0, waived 1 |
| Relabel only the native fact's target | Preview/apply exit 1; matching stored decision audit exit 3, unverifiable |
| Recompute the native ID with the Model target | Same refusal and unverifiable result |
| Reconstruct Model ID/revision/pin/path binding but retain native lint selector | Same refusal and unverifiable result |
| Mixed genuine Model and relabelled native input, both orders | Audit exit 3; genuine Model waived, native occurrence unwaived |

Raw invocation envelopes, assertions and results are under
`C:/Users/User/AppData/Local/Temp/lekalo-issue88-fix2-4d9890481af844e18e8c2d610c3ca8d3`.
The original review's external project/captures were copied and remain untouched.
These are synthetic local checks, not external issuer or provider acceptance.

## Verification and delivery boundary

```powershell
cargo build --locked -p lekalo-cli
cargo test --locked -p lekalo-core waivers:: --lib
cargo test --locked waivers
cargo test --locked -p lekalo-core ai_lint:: --lib
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH=$env:NODE_PATH
node scripts/test-waivers-contracts.mjs
node scripts/test-waiver-input-contracts.mjs
node scripts/test-waiver-audit-contracts.mjs
node scripts/test-ai-lint-waivers-contracts.mjs
```

| Check | Result |
| --- | --- |
| Locked CLI build | Passed, development debug profile |
| Focused waiver unit suite and requested filtered workspace run | Passed; 12 matching core tests, 0 failed. The latter filters unrelated test binaries rather than running the full workspace suite |
| Touched AI-lint unit namespace | 3 passed, 0 failed |
| Three successor gates | Each passed: live binary, exact Ajv 8.17.1, 39 retained audits, 7 native pin controls and 9 producer-domain controls |
| Frozen predecessor waiver gate | Passed, live binary and schema 0.6.4 |
| Core/CLI all-targets Clippy | `cargo clippy --locked -p lekalo-core -p lekalo-cli --all-targets -- -D warnings` passed; the existing `make` lint annotation retains its original scope |
| Formatting and JavaScript syntax | Rust format check and shared gate syntax check passed |
| Schema/provenance/version | Generator check passed; 79 synthetic fixture families; product 0.6.5, 118 version-checked artifact families |
| Documentation ownership | Live-help gate passed: 345 surfaces, 13 P0 owners and 33 owner controls |
| Frozen boundaries | All 136 contract artifacts, waiver DTOs, goldens/provenance and CI workflow remain unchanged; registry remains 500 entries at 0.6.4 |

No command or contract family was added, so the existing registered command and
family ownership inventories need no regeneration. The existing CI steps run
the strengthened shared controls after Cargo build with exact Ajv 8.17.1; no
schema, assertion or gate was weakened. The public waiver reference documents
producer admission and stored-decision behavior.

Only eight intended implementation/test/reference/report paths are delivered.
The full unfiltered suites, hosted CI, Linux/macOS, parallel #84 integration,
authenticated approval providers and HLV/AIFHub execution are not claimed by
this focused local fix. Delivery is local only; no push.
