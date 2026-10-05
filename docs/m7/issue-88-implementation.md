# Issue #88: scoped, justified and expiring waivers implementation

Status: **Implemented** local candidate, 2026-10-05. Acceptance authority:
[issue #88](https://github.com/ichinya/lekalo/issues/88) and the user-authorized
implementation following [the committed research](issue-88-research.md), commit
`302103c142d5a4c53d0a77fa747bc300fca6e0dc`. Branch: `ichinya/m7-issue-88`;
integration/source base: `a2aa1893766c01434842ca5062188a43898f41c5`.
The new independently versioned artifacts take product `0.6.5`; existing Model/IR,
registry, validation profiles, authority, privacy, lock, lint report/config/evidence
and historical waiver schemas retain their versions and bytes. No pushes, hosted
CI execution, external issuer approval or HLV/AIFHub execution are claimed.

## Delivered surfaces and compatibility

| Surface | Concrete implementation and evidence |
| --- | --- |
| Unified exception store | `contracts/ai-lint-waivers.schema.v0.6.5.json`, `crates/lekalo-core/src/waivers/wire.rs::{Store,Entry}`. This is a successor in the shipped family, not a second suppression family. `ai-lint --waivers` dispatches by exact header to the predecessor or successor, without merging stores. |
| Shared governance decisions | `waivers/mod.rs::{validate_store,validate_input,audit,compare}`. The successor matcher is used by lint and generic diagnostic/capability audit; the predecessor retains its historical compatibility path. |
| Neutral immutable input | `contracts/waiver-input.schema.v0.6.5.json`, `wire.rs::{Input,Fact,Fingerprint}`, `waivers/lint.rs::facts`; `ai-lint --waiver-facts` emits an actual producer input. The source input includes unsupported/unknown outcomes without converting them to support. |
| Machine-readable audit | `contracts/waiver-audit.schema.v0.6.5.json`, `wire.rs::{Audit,AuditEntry,Disposition,Change,Summary}`, `waivers/mod.rs::{audit,render}`. Each fact and decision keeps its own digest and provenance. |
| CLI inventory/audit/add | `crates/lekalo-cli/src/waivers.rs`, dispatch in `main.rs`. Inventory explicitly labels effectiveness unexamined; evaluation requires current profile, supplied facts and explicit UTC time. Add previews and applies an exact plan digest. |
| Safe committed-file write | `waivers/store.rs::{safe_path,read,plan_id,apply}`. Exact project-root home, physical file checks, concurrent-update guard, byte preconditions, staged/synced replacement and post-write verification. No Git, target execution or cache write is part of add. |
| Profile integration seam | `waivers/policy.rs::ProfileState` with existing validation, lint and target implementations. No names or payloads are assumed for parallel issue #84. |
| Review and completion proof | `audit --base`, `audit --done`, `audit --locked`; typed change rows, before/after digests, whole-report done digest and source lock prerequisite. These are companion evidence rather than edits to frozen Model diff/lock wires. |

The `0.6.4` predecessor is not silently migrated: it lacks the approver,
profile/revision pins and review metadata needed to mint a successor decision.
Its original compatibility behavior, including its historical optional expiry,
remains confined to explicit legacy lint input. New `waivers add/audit/list`
admit only the finite-deadline successor. Moving an old exception to the new
workflow requires a newly reviewed entry and current provenance.

The registry still has **500 entries at `0.6.4`**. No new diagnostic IDs are
allocated, so no registry schema edit or renumbering is required. Existing
registered waiver-invalid, version-unsupported and policy-denied diagnostics
carry stable detail tokens; audit reasons have a separate closed vocabulary.
Any future new diagnostic allocation must follow the registry successor seam
and cannot edit historical registry artifacts.

## Acceptance criteria

| AC from #88 | Implemented behavior | Concrete proof |
| --- | --- | --- |
| 1. Suppressed diagnostic remains visible with a waiver link | `waivers/lint.rs::apply` changes only native disposition/link and count projections. `Audit.findings` embeds the unchanged Fact with `factDigest`, original/effective gates, waiver ID and entry digest. | `scripts/lib/waivers-contract-gate.mjs` compares every native finding against the raw CLI result after removing only disposition/link; asserts raw counts and `waived=1`. `waiver-audit/golden/active.json` links the retained fact to `waiver-depth`. |
| 2. Expiry restores the original gate | Exact whole-second UTC; effective at deadline equality, ineffective one second later. The earliest expiry/review deadline wins. No ambient clock, renewal, rule deletion or severity rewrite. | Live exact-boundary and review-deadline cases; a real `ai-lint --check` run with all eligible findings waived exits 0, then exits 3 after expiry. `waiver-audit/golden/expired.json` retains facts and original denied gate. |
| 3. No unrelated-symbol scope leakage | Occurrence ID, immutable fact digest, selector, subject, target, typed scope, condition and every provenance pin must agree. Even project/module/profile scopes accept one approved occurrence. Wildcards/aliases refuse; ambiguous and overlapping decisions refuse. | Live symbol/module/project/target/profile controls plus native exact path; a second different symbol remains active. Hostile replacements reuse subject/condition and then occurrence ID under project scope: neither is accepted. Negative wildcard and malformed scope probes. The core shares one exact matcher across both diagnostic and capability facts. |
| 4. Model/adapter revision invalidation follows declared policy | Model/IR, adapter artifact/identity, source revision/manifest, capabilities, profile and condition differences stop acceptance. Unknown/withheld source pins are unverifiable; explicit unsupported dimensions are inapplicable. | Live mismatches for all five fingerprint dimensions; actual native producer version and revision change tests; CLI admission checks current Model/IR. `waiver-audit/golden/stale.json` carries the mismatched revision reason. |
| 5. Security-critical fixture is non-waivable by profile | Validation policy consumes the existing selected registry severity and profile override restrictions. Error severity and required evidence cannot be accepted. Target capability requirements come from resolved components; intrinsic invalid/security/data-loss outcomes remain denied. | Real CLI audit under the admitted default profile selects `semantic.public-output-private-type`. A supplied warning cannot downgrade the profile's error: exit 3, non-waivable and effective denied. `security-critical.json`; core tests contrast optional pooling with serverless-required pooling. No security-code list exists in the matcher. |
| 6. Audit reports active, expiring, expired and stale | Deterministic sorted entries, deadlines, reasons, mismatch pins and per-state counters; also unverifiable, non-waivable, orphan, unexamined, revoked and superseded. Inventory alone does not claim current effectiveness. | Live status and deadline controls, current list digest, finite review deadline, orphan versus partial input, revocation/supersession checks. Active/expired/stale goldens and machine schema validation with exact Ajv 8.17.1. |
| 7. Waiver changes are visible in review/done evidence | Git tracks the store; `--base` reports added/removed/changed decision digests. `doneDigest` binds store/input/profile/time/lock/base/provenance/dispositions. `--done` refuses changed proof; source `--locked` validates the exact current lock digest. | Live changed-reason approval/update produces a typed change, changes done digest, and refuses the previous done digest. `waiver-audit/golden/review-diff.json`; current/mismatched lock checks. Semantic `lekalo diff` and lock `0.3.2` remain unchanged; consumers include the audit companion in review/done bundles. |

## Field coverage and scope admission

| Required field group | Successor representation and enforcement |
| --- | --- |
| Stable waiver ID | `Entry.id`; unique in a store, add refuses reuse of a present ID. Version control preserves deleted history; no global ID registry is claimed. |
| Diagnostic/rule/capability ID | Typed `selector.kind/id`; rules must be active registered IDs. Add accepts a diagnostic code by resolving it to the registered rule ID. Capability eligibility comes from its resolved profile; unknown policy grants nothing. |
| Project/module/symbol/path/target/profile | Closed `scope.kind/id`; reuse semantic-ID segment grammar and portable project-relative paths. Known Model symbols/modules are checked against the compilation. Subject and target further constrain every named scope. |
| Reason | Required bounded nonblank prose, closed JSON and approval-subject binding. |
| Owner/approver | Distinct required exact tokens and a required `approvalRef` with the approved decision digest. Local integrity is checked; authenticated organizational approval remains repository review. |
| Created time | Required canonical UTC `createdAt`; future creation is ineffective. |
| Expiry/review | Required explicit states for both fields; at least one known real finite deadline, neither before creation. Earlier known deadline governs. |
| Source issue/decision | Required typed `sourceRef`, exact ID, explicit digest state; add requires exactly one source kind. |
| Accepted risk | Closed correctness/compatibility/security/data-loss/capability-gap class. Risk metadata never overrides non-waivable policy. |
| Model/adapter/revision provenance | Explicit Model, IR, adapter, revision and capabilities states plus profile and condition digests. Known differences are stale; incomplete proof cannot grant acceptance. |
| Replacement/supersession history | Active/revoked/superseded lifecycle and explicit predecessor state. Add retains and marks the old decision. Missing predecessors, duplicate successors, cycles and overlapping effective approvals refuse. |

Fingerprint matching is deliberately content based. A commit that contains the
waiver must not immediately invalidate itself through Git HEAD or a store hash
inside the source-revision domain. Model-only revision pins canonical Model/IR;
native revision pins the admitted revision and source manifest/fingerprints.
Adapter identity/version/artifact and capability evidence are separate pins.
Profile references bind resolved policy, including source provenance where the
existing profile machinery exposes it.

## Profile, authority and privacy boundaries

`ValidationState` delegates selected-rule severity to `validator/profile.rs` and
the registered default. Its existing prohibition on error downgrades is retained.
`LintState` consumes configured severity, confidence threshold, warning gate and
required coverage. `TargetState` consumes actual component capability requirements.
`ProfileState` exposes enabled/severity/required-evidence/blocking/waivable and
exact profile reference. #84 can supply its resolved state through that seam;
its separately implemented profile format is not guessed or awaited.

The store is placed at user-owned project-root `lekalo.waivers.json`, rather than
the research's possible `lekalo/waivers.json`. This is an explicit implementation
choice: the canonical Model root is closed and no authority/structure successor
is authorized implicitly by a new file. All supported project selections retain
that boundary. Add has an exact restricted home and writes only after preview
plan verification. An empty synchronization guard is local coordination, not
canonical evidence; it never contains exception decisions.

No authority grant, privacy grant, revocation store, export approval, diagnostic
fact, required-evidence fact, target support, baseline validity or raw regression
is weakened by a waiver. In particular the independent enforcement documented in
`docs/privacy.md` still governs export/leak decisions. Approval metadata is not a
signed issuer identity and cannot mint an authorization grant. Invalid, security
and data-loss source outcomes remain facts and blocked gates.

## Audit storage, lock/diff and integrations

The derived audit contains immutable fact, source evidence, entry/approval,
profile, store/input/lock digests, exact evaluation time and lineage. HLV/AIFHub
can preserve these supplied-document references; this implementation does not
claim an external provider has accepted or executed anything. Audit bytes can be
archived explicitly as a CI artifact. Nothing is silently stored as canonical
waivers in cache or run-history. The existing #121 history wire is unchanged.

Existing lock and semantic diff contracts do not have invented waiver fields.
Review/done consumers pair their normal receipts with the closed audit companion:
`--locked` binds the source-run lock, `--base` identifies waiver-only changes, and
`--done` verifies the exact complete audit. The store digest is included even when
the Model is unchanged. This distinction is required: an unchanged semantic diff
alone does not prove governance policy remained unchanged.

## Contracts, CI and documentation ownership

All three families are generated by `scripts/gen-waivers-contracts.mjs`; `--check`
compares committed schemas with the DTO-derived closed shapes. Their synthetic
goldens are under `ai-lint-waivers/golden-v0.6.5`, `waiver-input/golden` and
`waiver-audit/golden`, with family provenance in `fixture-provenance.json`.
The existing waiver fixture family is reused; the two derived families are added.

`test-waivers-contracts.mjs`, `test-waiver-input-contracts.mjs` and
`test-waiver-audit-contracts.mjs` each require the local CLI, exact Ajv **8.17.1**,
fresh producer output, schema-valid goldens and hostile/live controls. The shared
gate checks that the `ci.yml` build-test job invokes them after
`cargo build --workspace --locked`. The existing job runs on Linux, Windows and
macOS with its externally provisioned pinned Ajv; no gate or schema is weakened.

`scripts/lib/docs-maintenance.mjs` registers command owner `waivers` and the two
new family owners. The existing `ai-lint-waivers` family owner links to the new
reference. `scripts/update-docs-owners.mjs --write` regenerates the live-help
inventory and `cli.md` index. User-facing behavior is described in `docs/waivers.md`.

## Local verification and remaining integration limits

| Check | Local result |
| --- | --- |
| Workspace build | Passed with locked/offline dependencies, one build job and local development debug symbols disabled. |
| Workspace/all-targets Clippy | Passed with `-D warnings`; no lint gate was weakened. |
| Rust formatting | `cargo fmt --all -- --check` passed. |
| Waiver core tests | Seven passed, including exact occurrence/fact binding, profile requirements, closed decoding and write concurrency. |
| Complete core library suite | Outside the subprocess sandbox: **1,058 passed, 2 ignored**, zero failed and no filtered tests. The initial sandbox run hung in the deadline-child test; its isolated outside-sandbox retry and the complete rerun passed. |
| Three successor live family gates | Passed with updates disabled, exact Ajv 8.17.1, fresh CLI/golden comparison and 39 audit cases per family plus native/CLI/hostile checks. |
| Five shipped AI-lint family gates | Report, evidence/real collectors, config, predecessor waivers and comparison all passed against the refreshed committed goldens. |
| Schema generator | `gen-waivers-contracts.mjs --check` passed. |
| Fixture provenance | All 79 families declared synthetic; zero undeclared families. |
| Ownership/version checks | Live-help ownership inventory and changed-contract product-version checks passed at `0.6.5`. |
| Frozen artifacts | All **133** pre-existing contract artifacts compared byte-for-byte with the integration base; registry remains `0.6.4`, 500 entries. |
| Documentation replay metadata | Static examples gate passed: 12 examples, 2 setup cases and 7 controls. |

An initial
default-debug rebuild encountered a host LLVM memory allocation failure; the
successful rebuild used one Cargo job and disabled local development debug
symbols. Checked-in Cargo profiles and CI build settings were not changed.

Five existing AI-lint goldens required regeneration because product `0.6.5`
changes lock-derived attachment bytes. A read-only JSON comparison proved their
changes were confined to attachment references, attachment-derived condition
digests, waiver-reference digest and baseline/candidate references. Native fact
identities, severities, outcomes, counts and behavioral assertions are unchanged.
Predecessor contracts are verified independently rather than regenerated.

The parallel #84 integration, hosted Linux/macOS acceptance, authenticated
approval-provider integration and external HLV/AIFHub execution remain explicit
integration limits, not local acceptance evidence. The supplied neutral-input
workflow does not re-execute arbitrary native tools; producers own those facts
and provenance. Automatic audit retention, automatic legacy approval migration,
and implicit application to every unrelated command are not implemented or
claimed. The requested governance commands, lint integration, current profile
policy seam and all seven AC have concrete local evidence above.
