# Issue #88: fix 1 for Codex review F1

Status: **Implemented and locally verified**, 2026-10-05. This fixes the P2
finding in [Codex review round 1](issue-88-review1-codex.md). Starting HEAD:
`6f64b18ae644def96d99225086af45cb0df2dc13`; branch `ichinya/m7-issue-88`.
The starting worktree/index were clean. Delivery is a conventional local fix
commit, with no push or changes to other worktrees.

## F1 and the correction

Previously, matching `Unsupported` states satisfied any fingerprint dimension.
A native producer could declare its capability snapshot or source revision
unsupported, author a matching waiver with normal preview/apply, and change a
blocking native lint result from exit 3 to exit 0. Neither the approval binding
nor the whole-fact digest protected against this case: both were correctly bound
to an incomplete fact.

`waivers::policy::ProfileState::fingerprint_requirements` now supplies the
applicability requirements to the shared matcher. Its current defaults use the
admitted producer target, independently of fingerprint value states:

| Producer domain | Model | IR | Adapter | Revision | Capabilities |
| --- | --- | --- | --- | --- | --- |
| Reserved `model` target | Required known | Required known | Inapplicable; matching unsupported allowed | Required known | Inapplicable; matching unsupported allowed |
| Native/non-Model target, including aggregate targets | Required known | Required known | Required known | Required known | Required known |

The existing native evidence decoder already forbids `target:"model"`
(`ai_lint/input.rs:205`); the new live control verifies that this boundary remains
effective. A native producer cannot acquire Model-only applicability by changing
its admitted target. Neutral supplied facts retain their documented producer
ownership boundary; this fix does not authenticate or re-execute arbitrary
external producer reports.

`waivers/mod.rs::pin_differences` now requires known pins on **both** the stored
decision and current fact for every applicable dimension. Missing applicable
pins produce `fingerprint-unverifiable`; a matching incomplete waiver becomes
`unverifiable` and cannot change disposition. Existing mismatch/stale behavior,
unknown/withheld refusal, expiry precedence and non-waivable severity/required
evidence rules are retained. Facts and their explicit missing states stay visible.

The same audit evaluates `add` eligibility, standalone audits and live lint
application. Consequently preview and apply refuse an ineligible native
candidate, and a pre-existing structurally valid store cannot bypass the fix.
The CLI's loaded-profile wrapper delegates the new method to its admitted
validation/lint/target policy. The default implementation is additive for profile
implementers; parallel #84 can supply its admitted requirements through this seam
without an assumed future wire or diagnostic-code list.

Concrete source changes: `waivers/policy.rs::{FingerprintRequirements,ProfileState}`,
`waivers/mod.rs::{pin_differences,audit}`, and
`crates/lekalo-cli/src/waivers.rs::Loaded`. The public
[waivers reference](../waivers.md) now states required-known native dimensions and
the policy basis for genuine inapplicability.

## Regression evidence

Two new core tests exercise matching approvals and matching fact digests, so
their failures isolate applicability rather than stale-input rejection:

- `waivers_native_applicable_pins_cannot_be_declared_unavailable`: each of the
  five native dimensions with unsupported, unknown and withheld states; also an
  entirely unsupported native fingerprint. Acceptance remains ineffective and
  the original fact is retained.
- `waivers_model_inapplicability_does_not_hide_required_pins`: genuine Model-only
  unsupported adapter/capability pins remain eligible; required Model/IR/revision
  pins cannot be unsupported. Unknown/withheld remain ineffective for every
  dimension, including inapplicable ones.

`scripts/lib/waivers-contract-gate.mjs::nativePinCases` adds live controls to each
of the three existing successor family gates. A one-rule synthetic native CI
fixture isolates one blocking finding. The known-pin positive case still
previews, applies and suppresses it. Six negative cases cover capability and
revision with unsupported/unknown/withheld states. Each asserts:

- Preview and apply refuse with exit 1 and `waivers-ineligible-candidate`, while
  preserving the canonical store bytes.
- A separately bound matching store remains schema-valid but its audit reports
  `unverifiable`, effective false, waived 0 and original effective gate denied.
- Real native `ai-lint --check` retains the identical finding and exits 3.

The existing positive Model waiver controls, known native pin changes, exact
expiry, scope isolation, closed decoding, lineage and path/plan safety controls
remain enabled. Native evidence claiming the reserved Model target also refuses.

An independent replay outside the gate helpers used the original review's
fixture/data recipe in a fresh external project: **14 CLI invocations**. Known
pins retained preview/apply success and exit 0 while waived; one second after
expiry the result returned to exit 3. Unsupported capability and revision each
refused preview with exit 1 and unchanged store bytes. Fresh matching stored
waivers for those exact incomplete facts yielded `unverifiable`, waived 0 and
exit 3 in both standalone audit and actual lint checking.

Two further calls replayed an actual pre-fix candidate/input and its saved audit
receipt from review round 1. The formerly effective unsupported-capability waiver
is now ineffective/unverifiable, its done digest changes, and `--done` with the
old accepted-risk digest refuses with exit 1. Temporary replay scripts and JSON
evidence are under
`C:/Users/User/AppData/Local/Temp/lekalo-issue88-fix1-f464bcbfed6d4df2bf6a3d396cb45ab2`.
They are synthetic local evidence, not committed fixtures or provider acceptance.

## Verification

All required commands passed against the rebuilt CLI, with fixture updates disabled:

```powershell
cargo test --locked -p lekalo-core waivers:: --lib
cargo build --locked -p lekalo-cli
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH=$env:NODE_PATH
node scripts/test-waivers-contracts.mjs
node scripts/test-waiver-input-contracts.mjs
node scripts/test-waiver-audit-contracts.mjs
node scripts/test-ai-lint-waivers-contracts.mjs
```

| Check | Result |
| --- | --- |
| Touched waiver unit suite | 9 passed, 0 failed; seven retained and two new tests |
| Locked CLI build | Passed, development profile with debug information |
| Three successor family gates | Each passed: `live:true`, exact Ajv 8.17.1, 39 retained audits and 7 new native pin controls |
| Frozen predecessor waiver gate | Passed: `live:true`, schema 0.6.4 |
| Core/CLI all-targets Clippy | `cargo clippy --locked -p lekalo-core -p lekalo-cli --all-targets -- -D warnings` passed |
| Formatting and JavaScript syntax | `cargo fmt --all -- --check` and `node --check scripts/lib/waivers-contract-gate.mjs` passed |
| Schema generation check | `node scripts/gen-waivers-contracts.mjs --check` passed |
| Fixture provenance | 79 synthetic families, no undeclared family |
| Contract version check | Passed against starting HEAD: product 0.6.5, 118 artifact families |
| Scope/whitespace | Only the seven intended fix/report paths; staged diff checked before commit |

No contract, schema, golden, product version, registry, CI workflow or ownership
inventory changes are needed. All **136 contract artifacts** at the reviewed
candidate remain unmodified, including every predecessor and successor schema.
The registry remains 500 entries at 0.6.4. The shared live gate is already invoked
by all three successor CI steps after the build; the new controls run there with
the existing exact Ajv installation. No assertion or schema was weakened.

The full core/workspace suites, hosted CI and Linux/macOS execution were not run
for this focused fix. Prior review/implementation test counts are not substituted
for the local results above. No HLV/AIFHub, authenticated approval provider or
parallel #84 implementation is claimed to have been accepted by this fix.
