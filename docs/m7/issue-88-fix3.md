# Issue #88: fix 3 for reconstructed Model-domain claims

Status: **Implemented and locally verified**, 2026-10-05. This addresses the
residual P2 in [Codex review round 3](issue-88-review3-codex.md). Starting HEAD:
`dfa685ee`; branch `ichinya/m7-issue-88`, with a clean starting worktree/index.
Delivery is a conventional local fix commit, without a push or changes to other
worktrees.

## Finding and correction

Fix 2 rejected target-only relabelling, but its Model identity hash accepted
arbitrary subject and symbol state. A caller could reconstruct that hash and
the Model/IR revision for a real native depth occurrence. Native and Model depth
share `indirection.depth-exceeded`, so the lint selector guard did not distinguish
their producer shapes. Native depth retains an unknown symbol and opaque native
witness; Model depth emits a known semantic symbol with its dependency-witness
subject. The old workflow accepted the reconstructed fact in add/audit and could
write accepted-risk/done evidence without applicable native pins.

`waivers/policy.rs::admitted_producer_domain` now checks the emitted shape as
well as the existing identity/provenance recipe:

| Reserved Model claim | Required evidence |
| --- | --- |
| Symbol | Known, with the existing two/three-segment semantic-ID grammar |
| Subject | Equal to that known semantic symbol, as emitted from the Model dependency witness; opaque native hashes and unrelated semantic subjects refuse |
| Module | Known and equal to the symbol's owning first segment |
| Source confidence | Known `exact`, as emitted by the core Model depth producer |
| Finding ID | The existing Model depth identity over the admitted subject, reserved target and known symbol state |
| Model/IR and revision | Known; revision equals the canonical Model/IR hash |
| Adapter/capabilities/path | Unsupported adapter/capabilities and unknown path, as emitted by the Model producer |

The shape follows `ai_lint/mod.rs`'s current Model producer and its
`waivers/lint.rs::facts` projection. CLI admission still checks exact current
project/profile, Model/IR digests, known symbol/module membership and, when
requested, the source lock. The hash remains a consistency calculation; an
unknown symbol or native-witness subject cannot establish Model ownership by
rehashing. Replacing only the unknown symbol with a valid project symbol also
refuses while the native subject is retained.

The `ProfileState` default now refuses the reserved Model target and retains
native fingerprint requirements. An owner implementing only `reference` and
`rule` cannot silently opt into Model inapplicability, even for an otherwise
valid Model shape. Existing owners explicitly admit their recognized producer:
lint keeps its Model-rule selector check, validation consumes derived rule
obligations, and target profiles consume derived capability obligations. Each
uses the shared shape check; resolved profile severity and required-evidence
rules remain the eligibility authority. No #84 contract name or future wire is
assumed, and no severity allow/deny list is introduced.

`waivers/mod.rs::audit` independently rechecks the reserved producer shape at
effectiveness, in addition to pin matching and the profile's domain decision.
Even a profile implementation that opts into Model and overrides fingerprint
requirements cannot make an invalid reserved shape effective. Add evaluates
that same audit before preview/apply, while pre-existing approval-bound stores
are rejected at effectiveness without rewriting them. Invalid matching claims
remain `unverifiable`, reason `fingerprint-unverifiable`, waived 0; add refuses
with `waivers-ineligible-candidate` before writing. Non-Model facts retain all
five required-known native dimensions.

This verifies the recognized producer's emitted shape and profile admission.
It does not authenticate arbitrary external reports or re-execute their tools;
fully consistent supplied reports remain producer-owned. The public reference
documents these boundaries and the conservative profile default.

## Regression controls

Three added waiver unit tests cover the residual and adjacent profile seam:

- `waivers_recomputed_model_identity_still_requires_the_emitted_shape` tests
  ten inconsistent shapes with recomputed IDs, exact whole-fact/approval binding
  and project scope. Unknown/unsupported/withheld symbols, opaque/unrelated
  subjects, missing/unrelated modules and non-exact/unavailable confidence remain
  unverifiable. A stale ID or scope mismatch cannot mask admission refusal.
- `waivers_profile_defaults_never_opt_into_model_inapplicability` uses an owner
  implementing only the two required trait methods. A complete Model shape with
  a native-only selector cannot relax adapter/capability requirements or change
  its blocking gate to accepted risk.
- `waivers_shared_effectiveness_rechecks_model_shape_after_profile_opt_in`
  contrasts an accepted valid shape with a recomputed unknown-symbol claim under
  an explicitly permissive owner. The matcher itself refuses the latter.

The existing Model positive unit fixtures now express the actual emitted exact
confidence. All previous native-pin, Model-positive, unknown/withheld, profile,
scope, deadline and atomic-write tests remain enabled.

`scripts/lib/waivers-contract-gate.mjs::modelShapeCases` adds **14 live controls**
to each successor gate:

- A newly emitted native depth occurrence with the review's full reconstructed
  ID/revision/pin/path recipe refuses add preview/apply and stored-waiver audit.
- A known project symbol and exact confidence cannot hide its opaque native
  subject. Other controls rehash unknown/unsupported/withheld symbol states,
  unrelated subjects, unavailable modules and confidence changes.
- Two optional-capability projections retain the invalid source shape and
  refuse preview/apply; matching stored approvals remain unverifiable and
  unwaived. Their original advisory gate stays advisory rather than accepted-risk.
- Mixed genuine Model and forged native depth occurrences remain denied in
  either array order, with only the genuine Model acceptance effective.

Every negative checks unchanged store bytes, a structurally valid matching
approval, retained diagnostic facts, the fingerprint reason and waived 0.
Strict Ajv **8.17.1** validates those live wires. The 39 original audits, seven
native-pin controls and nine fix-2 producer controls remain unchanged and active.
The existing genuine Model add/apply and Model-derived capability golden continue
to pass without golden or schema updates.

## Independent replay of actual pre-fix evidence

A separate script, importing no gate helpers, completed **19 real CLI calls**
in disposable copies outside the checkout. It replayed the round-3 native-depth
project's actual applied store, preview/apply arguments and accepted-risk done
receipt. It also replayed the actual applied optional-capability candidate from
the independent review matrix, then produced fresh genuine Model positives.

| Probe | Current result |
| --- | --- |
| Actual old native-depth store and exact forged facts | Locked audit exit 3; unverifiable, effective false, fingerprint-unverifiable, waived 0; original fact and store bytes retained |
| Exact old preview/apply commands, root store absent | Both exit 1, ineligible candidate; no store created |
| Old accepted-risk done receipt | Exit 1, `waivers-done-stale` |
| Actual native lint with the old store | Exit 3, waived 0; findings unchanged from raw native output |
| Mixed original native and forged depth facts, both orders | Exit 3, forged approval unverifiable, waived 0 |
| Actual old capability candidate | Preview/apply exit 1; matching audit unverifiable and waived 0, original advisory gate retained; stored bytes unchanged |
| Fresh genuine Model occurrence | Locked preview/apply/audit and actual lint exit 0, waived 1; fact retained |
| Fresh genuine Model-derived capability gap | Locked preview/apply/audit exit 0, waived 1; unsupported capability outcome retained |

Actual native lint regenerates its native target/identity, so the old relabelled
entry is nonmatching and reports `unexamined`; it cannot suppress anything.
The matching supplied-input audit reports `unverifiable`. This status distinction
is retained rather than falsely claiming a matching occurrence in live lint.

Raw invocations, assertions, copied old evidence and results are under
`C:/Users/User/AppData/Local/Temp/lekalo-issue88-fix3-d3b54f1d597e45928ddb9325c35fabf1`.
The original round-3 evidence directories remain untouched. These are synthetic
local checks, not authenticated external producer or approval acceptance.

## Verification and delivery boundary

```powershell
cargo build --locked -p lekalo-cli
cargo test --locked -p lekalo-core waivers:: --lib
$env:CARGO_BUILD_JOBS='1'
cargo test --locked waivers
cargo clippy --locked -p lekalo-core -p lekalo-cli --all-targets -- -D warnings
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH=$env:NODE_PATH
node scripts/test-waivers-contracts.mjs
node scripts/test-waiver-input-contracts.mjs
node scripts/test-waiver-audit-contracts.mjs
node scripts/test-ai-lint-waivers-contracts.mjs
cargo fmt --all -- --check
node --check scripts/lib/waivers-contract-gate.mjs
node scripts/gen-waivers-contracts.mjs --check
node scripts/test-fixture-provenance.mjs
node scripts/check-contract-versions.mjs --base dfa685ee
node scripts/test-docs-ownership.mjs
```

| Check | Result |
| --- | --- |
| Locked CLI build | Passed, development debug profile |
| Focused core waiver suite | 15 passed, zero failed |
| Requested filtered workspace waiver run | Passed with one build job; 15 matching core tests, other test binaries filtered. This is not a full workspace-suite run |
| Three successor gates | Each passed: live binary, exact Ajv 8.17.1, 39 audits, 7 native pin controls, 9 producer controls, 14 emitted-shape controls |
| Frozen predecessor waiver gate | Passed, live binary and schema 0.6.4 |
| Core/CLI all-targets Clippy | Passed with `-D warnings`, one build job |
| Formatting, JavaScript syntax, schema generator | Passed |
| Fixture provenance/version | 79 synthetic families; product 0.6.5, 118 contract artifact families |
| Documentation ownership | Live-help check passed: 345 surfaces, 13 P0 owners and 33 owner controls |
| Frozen boundaries | Read-only comparison to `dfa685ee` confirms contracts, goldens/provenance and CI unchanged; registry remains 500 entries at 0.6.4 |

The first default-concurrency filtered workspace run encountered Windows paging
error 1455 while mapping the core library for an unrelated test binary. Repeating
the same test selection with `CARGO_BUILD_JOBS=1` passed; no tests, debug settings,
gates or schemas were weakened.

No command or contract family was added. Registered command/family ownership and
the generated CLI inventory therefore need no regeneration. Existing CI runs
the strengthened shared gate after Cargo build with exact Ajv 8.17.1.

Only the three waiver Rust files, shared gate, public reference and this fix
report are delivered. Full unfiltered suites, hosted CI, Linux/macOS, parallel
#84 integration, authenticated approval issuers and HLV/AIFHub execution are not
claimed. Delivery is local only; no push.
